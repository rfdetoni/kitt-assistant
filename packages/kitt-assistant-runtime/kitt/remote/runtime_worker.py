"""Opt-in single-runtime container worker. Not the paired browser HTTP server."""
from __future__ import annotations

import argparse
import hashlib
import ipaddress
import json
import os
import re
import secrets
import shutil
import ssl
import tempfile
import threading
import time
from compression import zstd
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from kitt.history.database import HistoryDatabase
from kitt.runtime.conversation_runtime import (
    ContainerCliRuntimeDriver, ConversationRuntimeManager, _local_capacity, _run_host,
)
from kitt_protocol import RuntimeBackend

_MAX_JSON = 64 * 1024
_MAX_BLOB = 8 * 1024 * 1024
_MAX_CACHE = 128 * 1024 * 1024
_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}")
_HASH = re.compile(r"[0-9a-f]{64}")


def discover_nvidia(workspace):
    """Bounded on-demand physical discovery, never assume configured GPU count."""
    binary = shutil.which("nvidia-smi")
    if binary is None:
        return []
    try:
        response = _run_host(
            [binary, "--query-gpu=uuid,memory.total,memory.free", "--format=csv,noheader,nounits"],
            cwd=workspace, timeout=3,
        )
    except (OSError, RuntimeError, TimeoutError):
        return []
    except Exception:
        return []
    if not response.get("success") or response.get("truncated"):
        return []
    lines = response.get("stdout", "").splitlines()
    if len(lines) > 64:
        return []
    devices = []
    seen = set()
    for line in lines:
        fields = [part.strip() for part in line.split(",")]
        if len(fields) != 3:
            return []
        identifier, total, free = fields
        if (not re.fullmatch(r"GPU-[0-9a-fA-F-]{36}", identifier)
                or identifier in seen or not total.isdecimal() or not free.isdecimal()):
            return []
        total, free = int(total), int(free)
        if not 0 <= free <= total <= 1024 * 1024 or not total:
            return []
        seen.add(identifier)
        devices.append({"uuid": identifier, "total_bytes": total * 1024 ** 2,
                        "free_bytes": free * 1024 ** 2})
    return sorted(devices, key=lambda item: (-item["free_bytes"], item["uuid"]))


class WorkerDriver(ContainerCliRuntimeDriver):
    """Dedicated scratch workspace, immutable image and reduced container privileges."""

    def __init__(self, backend, binary, workspace, cache):
        super().__init__(backend, binary, workspace)
        self.cache = cache

    def provision(self, config):
        runtime_id = config["runtime_id"]
        argv = [
            self.binary, "run", "-d", "--name", runtime_id,
            "--pull", "never", "--network", "none", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges", "--read-only",
            "--tmpfs", "/tmp:rw,nosuid,nodev,size=32m",
            "--cpus", f"{config['cpu_millis'] / 1000:.3f}",
            "--memory", str(config["memory_bytes"]),
            "--memory-swap", str(config["memory_bytes"]),
            "--pids-limit", str(config["pids_limit"]),
            "--mount", f"type=bind,src={self.root},dst=/workspace",
            "--mount", f"type=bind,src={self.cache},dst=/mnt/kitt/cache,readonly",
            "-w", "/workspace", "--entrypoint", "sleep",
            config["image"], "infinity",
        ]
        if config.get("gpu_count", 0):
            # Fixed operator policy, never accept device IDs or engine flags from clients.
            devices = config.get("gpu_devices")
            if devices:
                # Docker's --gpus flag parses CSV; quote the device field so
                # comma-separated UUIDs remain one option.
                argv[3:3] = ["--gpus", '"device=' + devices + '"']
            else:
                raise RuntimeError("physical GPU reservation is missing")
        result = _run_host(argv, cwd=self.root, timeout=120)
        if not result["success"]:
            raise RuntimeError("container provision failed")
        return runtime_id


class Worker:
    def __init__(self, root, runtime_id, *, image, engine="docker",
                 cpu_millis=1000, memory_bytes=256 * 1024 * 1024, pids_limit=64, gpu_count=0, gpu_min_free_bytes=0):
        if not _ID.fullmatch(runtime_id):
            raise ValueError("invalid worker runtime ID")
        if engine not in {"docker", "podman"}:
            raise ValueError("unsupported worker engine")
        if not re.fullmatch(r"[^\s,]+@sha256:[0-9a-f]{64}", image) or image.startswith("-"):
            raise ValueError("worker image must be pinned by SHA-256 digest")
        if not 10 <= cpu_millis <= 256000 or not 16 * 1024 * 1024 <= memory_bytes <= 1024 ** 4:
            raise ValueError("invalid worker resource limits")
        if not 16 <= pids_limit <= 65536:
            raise ValueError("invalid worker PID limit")
        if type(gpu_count) is not int or not 0 <= gpu_count <= 16 or (gpu_count and engine != "docker"):
            raise ValueError("GPU count requires Docker and must be 0..16")
        if (type(gpu_min_free_bytes) is not int or not 0 <= gpu_min_free_bytes <= 1024 ** 4
                or (gpu_min_free_bytes and not gpu_count)):
            raise ValueError("invalid minimum per-GPU free VRAM")
        root = Path(root).expanduser().resolve()
        if root == Path(root.anchor):
            raise ValueError("worker requires a dedicated non-root state directory")
        root.mkdir(parents=True, exist_ok=True, mode=0o700)
        if not root.is_dir():
            raise ValueError("invalid worker state directory")
        self.workspace = root / "workspace"
        self.cache = root / "cache"
        state = root / "control"
        for path in (self.workspace, self.cache, state):
            path.mkdir(exist_ok=True, mode=0o700)
            if path.is_symlink() or not path.is_dir() or any(c in str(path) for c in ',"\n\r'):
                raise ValueError("unsafe worker directory")
        self.runtime_id = runtime_id
        self.lock = threading.RLock()
        self.started_at = time.monotonic()
        self.counters = {"executions": 0, "execution_failures": 0,
                         "execution_ms": 0, "uploaded_bytes": 0,
                         "downloaded_bytes": 0, "cache_hits": 0}
        backend = RuntimeBackend.DOCKER if engine == "docker" else RuntimeBackend.PODMAN
        binary = shutil.which(engine)
        if binary is None:
            raise RuntimeError("worker container engine not installed")
        self.db = HistoryDatabase(state)
        self.driver = WorkerDriver(backend, binary, self.workspace, self.cache)
        self.manager = ConversationRuntimeManager(
            self.workspace, "worker", runtime_id, self.db,
            drivers={backend: self.driver},
        )
        self.backend = backend
        self.policy = {
            "runtime_id": "kitt-worker-" + hashlib.sha256(
                (str(root) + "\0" + runtime_id).encode()
            ).hexdigest()[:24],
            "image": image, "cpu_millis": cpu_millis, "memory_bytes": memory_bytes,
            "pids_limit": pids_limit, "gpu_count": gpu_count,
            "gpu_min_free_bytes": gpu_min_free_bytes, "network_disabled": True,
            "hibernate_on_pause": True, "auto_resume": True, "timeout_seconds": 120,
        }

    def _admission(self):
        cpu, memory = _local_capacity()
        devices = discover_nvidia(self.workspace) if self.policy["gpu_count"] else []
        selected = [item for item in devices
                    if item["free_bytes"] >= self.policy["gpu_min_free_bytes"]][:self.policy["gpu_count"]]
        available = (cpu >= self.policy["cpu_millis"] and memory is not None
                     and memory >= self.policy["memory_bytes"]
                     and len(selected) == self.policy["gpu_count"])
        return available, cpu, memory, selected

    def _cache_entries(self):
        return [p for p in self.cache.iterdir()
                if _HASH.fullmatch(p.name) and p.is_file() and not p.is_symlink()]

    def capacity(self):
        # Admission hint only. The provision operation is the authoritative
        # reservation; never make another scheduler wait behind a running command.
        if not self.lock.acquire(blocking=False):
            return {"available": False, "state": "BUSY"}
        try:
            binding = self.manager.binding()
            operation = self.manager.operation_status()
            available, cpu, memory, devices = self._admission()
            policy = {key: self.policy[key] for key in (
                "image", "cpu_millis", "memory_bytes", "pids_limit", "gpu_count",
            )}
            return {
                "available": binding is None and not operation.get("active")
                and available,
                "state": binding.state if binding is not None else "UNBOUND",
                "policy": policy,
                "host_available_cpu_millis": cpu,
                "host_available_memory_bytes": memory,
                "gpu_detected_count": len(devices),
                "gpu_min_available_vram_bytes": min((item["free_bytes"] for item in devices), default=0),
                "gpu_total_available_vram_bytes": sum(item["free_bytes"] for item in devices),
            }
        finally:
            self.lock.release()

    def dispatch(self, action, payload):
        if action == "capacity":
            return self.capacity()
        with self.lock:
            binding = self.manager.binding()
            if action == "metrics":
                entries = self._cache_entries()
                return {"runtime_id": self.runtime_id,
                        "state": binding.state if binding is not None else "UNBOUND",
                        "uptime_seconds": int(time.monotonic() - self.started_at),
                        "cache_entries": len(entries),
                        "cache_bytes": sum(path.stat().st_size for path in entries),
                        **self.counters}
            if action == "cache_prune":
                if binding is not None or self.manager.operation_status().get("active"):
                    raise RuntimeError("cache cleanup requires an idle unbound worker")
                age = payload.get("min_age_seconds")
                if type(age) is not int or not 3600 <= age <= 365 * 86400:
                    raise ValueError("cache cleanup age must be 3600..31536000 seconds")
                cutoff = time.time() - age
                removed = freed = 0
                for path in self._cache_entries():
                    stat = path.stat()
                    if stat.st_mtime < cutoff:
                        path.unlink()
                        removed += 1
                        freed += stat.st_size
                return {"removed": removed, "freed_bytes": freed}
            if action == "provision":
                requested = payload.get("config", {})
                if not isinstance(requested, dict):
                    raise ValueError("invalid provision config")
                if requested.get("image", self.policy["image"]) != self.policy["image"]:
                    raise PermissionError("image not authorized by worker")
                if requested.get("readonly_mounts") or requested.get("network_disabled") is False:
                    raise PermissionError("worker does not accept host mounts or network elevation")
                for key in ("cpu_millis", "memory_bytes", "pids_limit", "gpu_count"):
                    if key in requested and (type(requested[key]) is not int
                            or requested[key] != self.policy[key]):
                        raise ValueError("requested resource limits must match worker policy")
                if binding is not None:
                    raise RuntimeError("runtime already provisioned; attach instead")
                available, _, _, devices = self._admission()
                if not available:
                    raise RuntimeError("worker physical capacity no longer meets configured policy")
                minimum = requested.get("gpu_min_free_bytes", 0)
                if type(minimum) is not int or not 0 <= minimum <= 1024 ** 4:
                    raise ValueError("invalid minimum requested VRAM")
                if minimum and (not devices or min(item["free_bytes"] for item in devices) < minimum):
                    raise RuntimeError("worker free VRAM no longer meets request")
                policy = dict(self.policy)
                if devices:
                    policy["gpu_devices"] = ",".join(item["uuid"] for item in devices)
                self.manager.provision(self.backend, policy)
                return {"runtime_id": self.runtime_id}
            if action == "health":
                if binding is None:
                    return {"ok": False, "state": "UNBOUND", "runtime_id": self.runtime_id}
                value = self.manager.health()
                return {"ok": value.get("ok") is True, "state": value.get("state", binding.state),
                        "runtime_id": self.runtime_id, "hibernated": value.get("hibernated", False)}
            if binding is None:
                raise RuntimeError("runtime not provisioned")
            if action == "attach":
                if not self.manager.health().get("ok"):
                    raise RuntimeError("runtime not healthy")
            elif action == "execute":
                argv = payload.get("argv")
                if not isinstance(argv, list) or not 1 <= len(argv) <= 128 or any(
                    not isinstance(arg, str) or not arg or "\0" in arg or len(arg) > 8192
                    for arg in argv
                ):
                    raise ValueError("invalid command argv")
                request_id = payload.get("request_id")
                if request_id is not None and not isinstance(request_id, str):
                    raise ValueError("invalid request ID")
                started = time.monotonic()
                self.counters["executions"] += 1
                try:
                    result = self.manager.execute(argv, request_id=request_id)
                except Exception:
                    self.counters["execution_failures"] += 1
                    raise
                finally:
                    self.counters["execution_ms"] += int((time.monotonic() - started) * 1000)
                if not result.get("success") and not result.get("receipt", {}).get("success"):
                    self.counters["execution_failures"] += 1
                return {key: result[key] for key in (
                    "success", "returncode", "stdout", "stderr", "truncated",
                    "stdout_total_bytes", "stderr_total_bytes", "receipt", "replayed",
                ) if key in result}
            elif action == "pause":
                self.manager.pause()
            elif action == "resume":
                self.manager.resume()
            elif action == "snapshot":
                value = self.manager.snapshot_state()
                return {"runtime_id": self.runtime_id, "state_digest": value.get("state_digest"),
                        "healthy": value.get("healthy", False)}
            elif action == "terminate":
                self.manager.terminate()
            else:
                raise ValueError("unknown runtime action")
            return {"ok": True, "runtime_id": self.runtime_id}

    def blob(self, digest):
        if not _HASH.fullmatch(digest):
            raise ValueError("invalid blob digest")
        return self.cache / digest

    def put_blob(self, digest, source, length):
        if not 0 <= length <= _MAX_BLOB:
            raise ValueError("blob size exceeds limit")
        target = self.blob(digest)
        with self.lock:
            existing = self._cache_entries()
            used = sum(p.stat().st_size for p in existing)
            if target not in existing and (len(existing) >= 128 or used + length > _MAX_CACHE):
                raise ValueError("worker cache quota exceeded")
            if target.is_symlink():
                raise ValueError("invalid cache target")
            fd, name = tempfile.mkstemp(prefix=".stage-", dir=self.cache)
            stage = Path(name)
            try:
                hasher = hashlib.sha256()
                remaining = length
                with os.fdopen(fd, "wb") as handle:
                    while remaining:
                        chunk = source.read(min(remaining, 64 * 1024))
                        if not chunk:
                            raise ValueError("truncated blob")
                        remaining -= len(chunk)
                        hasher.update(chunk)
                        handle.write(chunk)
                    if hasher.hexdigest() != digest:
                        raise ValueError("blob integrity check failed")
                    handle.flush()
                    os.fsync(handle.fileno())
                if target in existing:
                    with target.open("rb") as old:
                        valid = hashlib.file_digest(old, "sha256").hexdigest() == digest
                    if valid:
                        self.counters["cache_hits"] += 1
                        self.counters["uploaded_bytes"] += length
                        os.utime(target, None)
                        return
                    if used - target.stat().st_size + length > _MAX_CACHE:
                        raise ValueError("worker cache quota exceeded")
                os.replace(stage, target)
                self.counters["uploaded_bytes"] += length
            finally:
                stage.unlink(missing_ok=True)


class RuntimeHTTPServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True
    request_queue_size = 8

    def __init__(self, address, worker, token):
        if not isinstance(token, str) or len(token) < 32:
            raise ValueError("worker token must be at least 32 characters")
        self.worker = worker
        self.token = token
        self.slots = threading.BoundedSemaphore(4)
        super().__init__(address, Handler)

    def process_request(self, request, address):
        if not self.slots.acquire(blocking=False):
            self.shutdown_request(request)
            return
        try:
            super().process_request(request, address)
        except BaseException:
            self.slots.release()
            raise

    def process_request_thread(self, request, address):
        try:
            super().process_request_thread(request, address)
        finally:
            self.slots.release()


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        self.request.settimeout(10)
        super().setup()

    def log_message(self, *_args):
        # Do not log authorization headers, command arguments or URLs.
        pass

    def _reply(self, status, value):
        body = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
        if len(body) > 1024 * 1024:
            status, body = 500, b'{"error":"response exceeds limit"}'
        encoding = None
        # Only explicit opt-in; small or incompressible bodies stay identity.
        if (status == 200 and self.command == "POST" and len(body) >= 1024
                and self.headers.get_all("Accept-Encoding") == ["zstd"]):
            packed = zstd.compress(body, level=1)
            if len(packed) < len(body):
                body, encoding = packed, "zstd"
        self.send_response(status)
        if encoding:
            self.send_header("Content-Encoding", encoding)
        self.send_header("Vary", "Accept-Encoding")
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)
        self.close_connection = True

    def _authorized(self):
        values = self.headers.get_all("Authorization") or []
        expected = ("Bearer " + self.server.token).encode()
        if len(values) != 1 or not secrets.compare_digest(values[0].encode(), expected):
            self._reply(401, {"error": "unauthorized"})
            return False
        return True

    def _route(self):
        prefix = f"/v1/runtimes/{self.server.worker.runtime_id}/"
        if not self.path.startswith(prefix):
            raise KeyError("unknown runtime")
        return self.path[len(prefix):]

    def _length(self, maximum, *, allow_zstd=False):
        lengths = self.headers.get_all("Content-Length") or []
        if len(lengths) != 1 or not lengths[0].isascii() or not lengths[0].isdigit():
            raise ValueError("valid Content-Length required")
        encodings = self.headers.get_all("Content-Encoding") or []
        if self.headers.get("Transfer-Encoding") or (encodings and not (allow_zstd and encodings == ["zstd"])):
            raise ValueError("unsupported body encoding")
        size = int(lengths[0])
        if size > maximum:
            raise ValueError("request exceeds size limit")
        return size

    def _handle(self):
        if not self._authorized():
            return
        try:
            route = self._route()
            worker = self.server.worker
            if self.command == "POST":
                size = self._length(_MAX_JSON, allow_zstd=True)
                raw = self.rfile.read(size)
                if len(raw) != size:
                    raise ValueError("truncated JSON")
                if self.headers.get("Content-Encoding") == "zstd":
                    try:
                        info = zstd.get_frame_info(raw)
                        if info.decompressed_size is None or info.decompressed_size > _MAX_JSON:
                            raise ValueError("decoded JSON exceeds size limit")
                        decoder = zstd.ZstdDecompressor(options={zstd.DecompressionParameter.window_log_max: 16})
                        raw = decoder.decompress(raw, max_length=_MAX_JSON + 1)
                        if len(raw) > _MAX_JSON or not decoder.eof or decoder.unused_data:
                            raise ValueError("invalid bounded Zstd frame")
                    except (zstd.ZstdError, EOFError) as exc:
                        raise ValueError("invalid Zstd frame") from exc
                value = json.loads(raw)
                if not isinstance(value, dict):
                    raise ValueError("JSON object required")
                self._reply(200, worker.dispatch(route, value))
                return
            if not route.startswith("blobs/"):
                raise KeyError("unknown route")
            digest = route.removeprefix("blobs/")
            if self.command == "PUT":
                worker.put_blob(digest, self.rfile, self._length(_MAX_BLOB))
                self._reply(200, {"sha256": digest})
                return
            if self.command not in {"GET", "HEAD"}:
                raise ValueError("unsupported method")
            with worker.lock:
                blob = worker.blob(digest)
                if blob.is_symlink() or not blob.is_file():
                    raise KeyError("blob not found")
                with blob.open("rb") as handle:
                    size = os.fstat(handle.fileno()).st_size
                    if size > _MAX_BLOB or hashlib.file_digest(handle, "sha256").hexdigest() != digest:
                        raise ValueError("blob integrity check failed")
                    handle.seek(0)
                    self.send_response(200)
                    self.send_header("Content-Type", "application/octet-stream")
                    self.send_header("Content-Length", str(size))
                    self.send_header("Cache-Control", "no-store")
                    self.send_header("Connection", "close")
                    self.end_headers()
                    self.close_connection = True
                    if self.command == "GET":
                        shutil.copyfileobj(handle, self.wfile, length=64 * 1024)
                        worker.counters["downloaded_bytes"] += size
                    else:
                        worker.counters["cache_hits"] += 1
                    os.utime(blob, None)
        except KeyError:
            self._reply(404, {"error": "not found"})
        except PermissionError:
            self._reply(403, {"error": "worker policy denied request"})
        except (ValueError, TypeError):
            self._reply(400, {"error": "invalid worker request"})
        except (RuntimeError, TimeoutError, OSError):
            self._reply(409, {"error": "runtime operation failed or requires operator recovery"})

    do_POST = _handle
    do_PUT = _handle
    do_GET = _handle
    do_HEAD = _handle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state-dir", required=True)
    parser.add_argument("--runtime-id", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--engine", choices=["docker", "podman"], default="docker")
    parser.add_argument("--cpu-millis", type=int, default=1000)
    parser.add_argument("--memory-bytes", type=int, default=256 * 1024 * 1024)
    parser.add_argument("--pids-limit", type=int, default=64)
    parser.add_argument("--gpu-count", type=int, default=0,
                        help="Operator-reserved Docker GPU count; requires installed GPU runtime")
    parser.add_argument("--gpu-min-free-bytes", type=int, default=0,
                        help="Minimum free VRAM per selected physical NVIDIA GPU")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=7348)
    parser.add_argument("--tls-cert")
    parser.add_argument("--tls-key")
    args = parser.parse_args()
    host = ipaddress.IPv4Address(args.host)
    if not host.is_loopback and not (host.is_private and args.tls_cert and args.tls_key):
        parser.error("non-loopback worker requires a private IPv4 address and TLS")
    if bool(args.tls_cert) != bool(args.tls_key):
        parser.error("both TLS certificate and key are required")
    token = os.environ.get("KITT_REMOTE_RUNTIME_TOKEN", "")
    if len(token) < 32:
        parser.error("set KITT_REMOTE_RUNTIME_TOKEN to a high-entropy token (at least 32 characters)")
    worker = Worker(args.state_dir, args.runtime_id, image=args.image, engine=args.engine,
                    cpu_millis=args.cpu_millis, memory_bytes=args.memory_bytes,
                    pids_limit=args.pids_limit, gpu_count=args.gpu_count,
                    gpu_min_free_bytes=args.gpu_min_free_bytes)
    try:
        with RuntimeHTTPServer((str(host), args.port), worker, token) as server:
            if args.tls_cert:
                context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                context.minimum_version = ssl.TLSVersion.TLSv1_2
                context.load_cert_chain(args.tls_cert, args.tls_key)
                server.socket = context.wrap_socket(server.socket, server_side=True)
            server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        worker.db.close()


if __name__ == "__main__":
    main()

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
            argv[3:3] = ["--gpus", str(config["gpu_count"])]
        result = _run_host(argv, cwd=self.root, timeout=120)
        if not result["success"]:
            raise RuntimeError("container provision failed")
        return runtime_id


class Worker:
    def __init__(self, root, runtime_id, *, image, engine="docker",
                 cpu_millis=1000, memory_bytes=256 * 1024 * 1024, pids_limit=64, gpu_count=0):
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
            "pids_limit": pids_limit, "gpu_count": gpu_count, "network_disabled": True,
            "hibernate_on_pause": True, "auto_resume": True, "timeout_seconds": 120,
        }

    def capacity(self):
        # Admission hint only. The provision operation is the authoritative
        # reservation; never make another scheduler wait behind a running command.
        if not self.lock.acquire(blocking=False):
            return {"available": False, "state": "BUSY"}
        try:
            binding = self.manager.binding()
            operation = self.manager.operation_status()
            cpu, memory = _local_capacity()
            policy = {key: self.policy[key] for key in (
                "image", "cpu_millis", "memory_bytes", "pids_limit", "gpu_count",
            )}
            return {
                "available": binding is None and not operation.get("active")
                and cpu >= policy["cpu_millis"]
                and memory is not None and memory >= policy["memory_bytes"],
                "state": binding.state if binding is not None else "UNBOUND",
                "policy": policy,
                "host_available_cpu_millis": cpu,
                "host_available_memory_bytes": memory,
            }
        finally:
            self.lock.release()

    def dispatch(self, action, payload):
        if action == "capacity":
            return self.capacity()
        with self.lock:
            binding = self.manager.binding()
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
                self.manager.provision(self.backend, self.policy)
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
                result = self.manager.execute(argv, request_id=request_id)
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
            existing = [p for p in self.cache.iterdir()
                        if _HASH.fullmatch(p.name) and p.is_file() and not p.is_symlink()]
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
                        return
                    if used - target.stat().st_size + length > _MAX_CACHE:
                        raise ValueError("worker cache quota exceeded")
                os.replace(stage, target)
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
        self.send_response(status)
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

    def _length(self, maximum):
        lengths = self.headers.get_all("Content-Length") or []
        if len(lengths) != 1 or not lengths[0].isascii() or not lengths[0].isdigit():
            raise ValueError("valid Content-Length required")
        if self.headers.get("Transfer-Encoding") or self.headers.get("Content-Encoding"):
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
                size = self._length(_MAX_JSON)
                raw = self.rfile.read(size)
                if len(raw) != size:
                    raise ValueError("truncated JSON")
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
                    pids_limit=args.pids_limit, gpu_count=args.gpu_count)
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

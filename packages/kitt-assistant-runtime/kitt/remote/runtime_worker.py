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
import sys
import tempfile
import threading
import time
from compression import zstd
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from kitt.history.database import HistoryDatabase
from kitt.remote.cas_store import (
    WorkspaceCasStore,
    ZSTD_WINDOW_LOG_MAX,
    compress_zstd_bytes,
    decompress_zstd_bytes,
    should_compress_zstd,
)
from kitt.remote.gpu_manager import HostGpuLeaseStore, SCOPE_NOTE, select_gpus
from kitt.remote.migration import live_migration_supported
from kitt.runtime.conversation_runtime import (
    ContainerCliRuntimeDriver, ConversationRuntimeManager, RuntimeOutcomeUnknown, _local_capacity, _readonly_mounts, _run_host,
)
from kitt_protocol import RuntimeBackend

_MAX_JSON = 64 * 1024
_MAX_BLOB = 64 * 1024 * 1024
_MAX_CACHE = 512 * 1024 * 1024
_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}")
_HASH = re.compile(r"[0-9a-f]{64}")
_GPU_ID = re.compile(r"GPU-[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
_DRM_ID = re.compile(r"(AMD|INTEL)-[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]")
_RENDER = re.compile(r"/dev/dri/renderD[0-9]{1,5}")
_GPU_LEASE = "io.kitt.gpu-lease"
_GPU_OWNER = "io.kitt.gpu-owner"


def _gpu_lease_name(identifier):
    if not (_GPU_ID.fullmatch(identifier) or _DRM_ID.fullmatch(identifier)):
        raise ValueError("invalid physical GPU identifier")
    return "kitt-gpu-lease-" + hashlib.sha256(identifier.lower().encode("ascii")).hexdigest()[:32]


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
        if (not _GPU_ID.fullmatch(identifier)
                or identifier in seen or not total.isdecimal() or not free.isdecimal()):
            return []
        total, free = int(total), int(free)
        if not 0 <= free <= total <= 1024 * 1024 or not total:
            return []
        seen.add(identifier)
        devices.append({"uuid": identifier, "total_bytes": total * 1024 ** 2,
                        "free_bytes": free * 1024 ** 2})
    return sorted(devices, key=lambda item: (-item["free_bytes"], item["uuid"]))


def discover_drm(vendor, *, sysfs=Path("/sys/class/drm"), devroot=Path("/dev/dri")):
    """Linux DRM render-node discovery; unknown VRAM is not fabricated."""
    if sys.platform != "linux" or vendor not in {"amd", "intel"}:
        return []
    expected = {"amd": "0x1002", "intel": "0x8086"}[vendor]
    devices = []
    seen = set()
    try:
        entries = sorted(sysfs.glob("renderD*"))
        if len(entries) > 64:
            return []
        for entry in entries:
            if not re.fullmatch(r"renderD[0-9]{1,5}", entry.name):
                continue
            device = (entry / "device").resolve(strict=True)
            node = devroot / entry.name
            if (device / "vendor").read_text().strip() != expected or not node.is_char_device():
                continue
            identifier = vendor.upper() + "-" + device.name.lower()
            if not _DRM_ID.fullmatch(identifier) or identifier in seen:
                continue
            total = free = None
            try:
                total = int((device / "mem_info_vram_total").read_text())
                used = int((device / "mem_info_vram_used").read_text())
                if not 0 <= used <= total <= 1024 ** 4 or total == 0:
                    total = free = None
                else:
                    free = total - used
            except (OSError, ValueError):
                total = free = None
            seen.add(identifier)
            devices.append({"uuid": identifier, "total_bytes": total, "free_bytes": free,
                            "render_node": "/dev/dri/" + entry.name})
    except (OSError, ValueError):
        return []
    return sorted(devices, key=lambda item: (-(item["free_bytes"] if item["free_bytes"] is not None else -1),
                                             item["uuid"]))


class WorkerDriver(ContainerCliRuntimeDriver):
    """Dedicated scratch workspace, immutable image and reduced container privileges."""

    def __init__(self, backend, binary, workspace, cache):
        super().__init__(backend, binary, workspace)
        self.cache = cache
        self.leases = HostGpuLeaseStore(Path(cache) / ".leases")

    def leased_gpus(self):
        result = _run_host([self.binary, "ps", "-a", "--filter", f"label={_GPU_LEASE}=1",
                            "--format", "{{.Names}}"], cwd=self.root, timeout=3)
        if not result.get("success") or result.get("truncated"):
            raise RuntimeError("GPU lease discovery failed")
        names = set(result.get("stdout", "").splitlines())
        if any(not re.fullmatch(r"kitt-gpu-lease-[0-9a-f]{32}", name) for name in names):
            raise RuntimeError("invalid GPU lease inventory")
        for uid in self.leases.active_uuids():
            names.add(_gpu_lease_name(uid))
        return names

    def _gpu_devices(self, config):
        value = config.get("gpu_devices", "")
        devices = value.split(",") if value else []
        if len(devices) != config.get("gpu_count", 0) or len({d.lower() for d in devices}) != len(devices):
            raise ValueError("invalid physical GPU selection")
        for identifier in devices:
            _gpu_lease_name(identifier)
        return sorted(devices, key=str.lower)

    def _release_lease_ids(self, identifiers):
        for identifier in identifiers:
            result = _run_host([self.binary, "rm", identifier], cwd=self.root, timeout=10)
            if not result.get("success") and "No such" not in result.get("stderr", ""):
                raise RuntimeOutcomeUnknown("GPU lease cleanup requires operator recovery")

    def _reserve_gpus(self, devices, runtime_id, image, *, vendor="nvidia", vram_bytes=0):
        # Host flock ledger + engine container names coordinate multi-worker exclusivity.
        dev_specs = [{"uuid": d, "vendor": vendor} for d in devices]
        self.leases.reserve_atomic(
            runtime_id,
            dev_specs,
            backend=self.backend.value,
            holder_id=self.binary,
            mode="exclusive",
            vram_per_gpu_bytes=vram_bytes,
        )
        created = []
        lease_containers = {}
        try:
            for identifier in devices:
                name = _gpu_lease_name(identifier)
                result = _run_host([self.binary, "create", "--name", name,
                                    "--label", f"{_GPU_LEASE}=1", "--label", f"{_GPU_OWNER}={runtime_id}",
                                    "--pull", "never", "--network", "none", "--read-only",
                                    "--cap-drop", "ALL", "--entrypoint", "sleep",
                                    image, "infinity"], cwd=self.root, timeout=30)
                if not result.get("success"):
                    self._release_lease_ids(created)
                    raise RuntimeError("physical GPU is already leased or lease creation failed")
                container_id = result.get("stdout", "").strip()
                if not _HASH.fullmatch(container_id):
                    raise RuntimeOutcomeUnknown("GPU lease creation returned an invalid ID")
                created.append(container_id)
                lease_containers[identifier] = container_id
            return lease_containers
        except Exception:
            self.leases.mark_state(runtime_id, "RELEASED", uuids=list(devices))
            raise

    def _owned_lease_ids(self, devices, runtime_id, *, allow_missing=False):
        identifiers = []
        for device in devices:
            name = _gpu_lease_name(device)
            result = _run_host([self.binary, "inspect", "--format",
                                '{{.Id}} {{index .Config.Labels "io.kitt.gpu-owner"}}',
                                name], cwd=self.root, timeout=10)
            if not result.get("success"):
                if allow_missing and "No such" in result.get("stderr", ""):
                    continue
                raise RuntimeOutcomeUnknown("GPU lease is unavailable; operator recovery required")
            fields = result.get("stdout", "").split()
            if len(fields) != 2 or not _HASH.fullmatch(fields[0]) or fields[1] != runtime_id:
                raise RuntimeOutcomeUnknown("GPU lease ownership mismatch")
            identifiers.append(fields[0])
        return identifiers

    def _drm_nodes(self, config, devices):
        vendor = config.get("gpu_vendor", "nvidia")
        if vendor == "nvidia":
            if any(not _GPU_ID.fullmatch(device) for device in devices):
                raise ValueError("invalid NVIDIA GPU identity")
            return []
        if vendor not in {"amd", "intel"} or any(not device.startswith(vendor.upper() + "-") for device in devices):
            raise ValueError("GPU vendor does not match selected devices")
        nodes = config.get("gpu_render_nodes", "").split(",")
        if len(nodes) != len(devices) or len(set(nodes)) != len(nodes) or any(not _RENDER.fullmatch(node) for node in nodes):
            raise ValueError("invalid GPU render node selection")
        discovered = {item["uuid"]: item["render_node"] for item in discover_drm(vendor)}
        if any(discovered.get(device) != node for device, node in zip(devices, nodes, strict=True)):
            raise RuntimeError("GPU render node mapping changed")
        return nodes

    def resume(self, runtime_id, config):
        devices = self._gpu_devices(config)
        if devices:
            self._owned_lease_ids(devices, runtime_id)
            self._drm_nodes(config, devices)
        super().resume(runtime_id, config)

    def terminate(self, runtime_id, config):
        devices = self._gpu_devices(config)
        identifiers = self._owned_lease_ids(devices, runtime_id, allow_missing=True)
        if devices:
            self.leases.mark_state(runtime_id, "RELEASING", uuids=list(devices))
        super().terminate(runtime_id, config)
        self._release_lease_ids(identifiers)
        if devices:
            self.leases.mark_state(runtime_id, "RELEASED", uuids=list(devices))

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
        for alias, source in _readonly_mounts(config):
            if alias.lower() == "cache":
                raise ValueError("cache mount alias is reserved")
            argv[3:3] = ["--mount", f"type=bind,src={source},dst=/mnt/kitt/{alias},readonly"]
        devices = self._gpu_devices(config)
        lease_containers = {}
        if devices:
            # Reserve all physical UUIDs before starting the actual workload.
            vendor = config.get("gpu_vendor", "nvidia")
            nodes = self._drm_nodes(config, devices)
            lease_containers = self._reserve_gpus(
                devices, runtime_id, config["image"],
                vendor=vendor, vram_bytes=int(config.get("gpu_min_free_bytes") or 0),
            )
            if nodes:
                flags = [flag for node in nodes for flag in ("--device", node + ":" + node + ":rw")]
                if vendor == "amd":
                    if Path("/dev/kfd").is_char_device():
                        flags += ["--device", "/dev/kfd:/dev/kfd:rw"]
                    idx_list = ",".join(str(i) for i in range(len(nodes)))
                    flags += ["-e", f"ROCR_VISIBLE_DEVICES={idx_list}", "-e", f"HIP_VISIBLE_DEVICES={idx_list}"]
                elif vendor == "intel":
                    idx_list = ",".join(str(i) for i in range(len(nodes)))
                    flags += ["-e", f"ZE_AFFINITY_MASK={idx_list}"]
                argv[3:3] = flags
            elif self.backend == RuntimeBackend.PODMAN:
                flags = [flag for d in devices for flag in ("--device", f"nvidia.com/gpu={d}")]
                flags += ["-e", "NVIDIA_VISIBLE_DEVICES=" + ",".join(devices)]
                argv[3:3] = flags
            else:
                argv[3:3] = ["--gpus", '"device=' + ",".join(devices) + '"']
        result = _run_host(argv, cwd=self.root, timeout=120)
        if not result["success"]:
            if devices:
                inspect_res = _run_host([self.binary, "inspect", runtime_id], cwd=self.root, timeout=10)
                if not inspect_res.get("success") and "No such" in inspect_res.get("stderr", ""):
                    # Container definitively never existed -> safe rollback of newly created leases.
                    self._release_lease_ids(list(lease_containers.values()))
                    self.leases.mark_state(runtime_id, "RELEASED", uuids=list(devices))
                    raise RuntimeError("GPU container provision failed before creation; leases rolled back")
                self.leases.mark_state(runtime_id, "INDETERMINATE", uuids=list(devices), reason="provision status indeterminate")
                raise RuntimeOutcomeUnknown("GPU container provision failed; leases retained")
            raise RuntimeError("container provision failed")
        if devices:
            cid = str(result.get("stdout", "")).strip() or None
            self.leases.mark_active(runtime_id, list(devices), container_id=cid, lease_containers=lease_containers)
        return runtime_id


class Worker:
    def __init__(self, root, runtime_id, *, image, engine="docker",
                 cpu_millis=1000, memory_bytes=256 * 1024 * 1024, pids_limit=64, gpu_count=0, gpu_min_free_bytes=0, gpu_vendor="nvidia", mount_manifest=None):
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
        if type(gpu_count) is not int or not 0 <= gpu_count <= 16 or (gpu_count and engine not in {"docker", "podman"}):
            raise ValueError("GPU count requires Docker/Podman and must be 0..16")
        if gpu_vendor not in {"nvidia", "amd", "intel"} or (gpu_count and gpu_vendor != "nvidia" and sys.platform != "linux"):
            raise ValueError("AMD/Intel GPU support requires Linux containers")
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
        mounts = {}
        if mount_manifest is not None:
            from kitt.remote.mount_sources import prepare_mounts
            mounts = prepare_mounts(root, Path(mount_manifest).expanduser())
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
            "gpu_min_free_bytes": gpu_min_free_bytes, "gpu_vendor": gpu_vendor, "network_disabled": True,
            "hibernate_on_pause": True, "auto_resume": True, "timeout_seconds": 120,
            **({"readonly_mounts": mounts} if mounts else {}),
        }

    def _admission(self):
        cpu, memory = _local_capacity()
        devices = []
        if self.policy["gpu_count"]:
            vendor = self.policy["gpu_vendor"]
            devices = discover_nvidia(self.workspace) if vendor == "nvidia" else discover_drm(vendor)
        if devices:
            try:
                leased = self.driver.leased_gpus()
                devices = [item for item in devices if _gpu_lease_name(item["uuid"]) not in leased]
            except (OSError, RuntimeError, TimeoutError):
                devices = []
            except Exception:
                devices = []
        floor = self.policy["gpu_min_free_bytes"]
        if self.policy["gpu_count"]:
            try:
                selected = select_gpus(
                    devices,
                    leased_uuids=set(),
                    allocated_vram=self.driver.leases.allocated_vram_by_uuid(),
                    requested_count=self.policy["gpu_count"],
                    min_free_bytes=floor,
                )
            except RuntimeError:
                selected = []
        else:
            selected = []
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
                "image", "cpu_millis", "memory_bytes", "pids_limit", "gpu_count", "gpu_vendor",
            )}
            return {
                "available": binding is None and not operation.get("active")
                and available,
                "state": binding.state if binding is not None else "UNBOUND",
                "policy": policy,
                "mount_aliases": sorted(self.policy.get("readonly_mounts", {})),
                "host_available_cpu_millis": cpu,
                "host_available_memory_bytes": memory,
                "gpu_detected_count": len(devices),
                "gpu_min_available_vram_bytes": min((item["free_bytes"] for item in devices), default=0)
                if all(item["free_bytes"] is not None for item in devices) else None,
                "gpu_total_available_vram_bytes": sum(item["free_bytes"] for item in devices)
                if all(item["free_bytes"] is not None for item in devices) else None,
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
            if action == "snapshot_export":
                from kitt.remote.workspace_snapshot import export_workspace
                if (binding is None or binding.state != "PAUSED"
                        or self.manager.operation_status().get("active")
                        or not self.manager.health().get("hibernated")):
                    raise RuntimeError("snapshot requires a stopped, hibernated runtime")
                if payload.get("allow_sensitive") is not True:
                    raise PermissionError("snapshot requires explicit sensitive-data authorization")
                with tempfile.TemporaryFile(dir=self.cache) as source:
                    digest, size = export_workspace(self.workspace, source, self.policy["image"])
                    self.put_blob(digest, source, size)
                return {"sha256": digest, "bytes": size, "image": self.policy["image"]}
            if action == "snapshot_import":
                from kitt.remote.workspace_snapshot import import_workspace
                if binding is not None or self.manager.operation_status().get("active"):
                    raise RuntimeError("snapshot restore requires an unbound worker")
                if payload.get("allow_sensitive") is not True:
                    raise PermissionError("snapshot requires explicit sensitive-data authorization")
                digest = payload.get("sha256")
                path = self.blob(digest)
                if path.is_symlink() or not path.is_file() or path.stat().st_size > _MAX_BLOB:
                    raise ValueError("snapshot blob unavailable")
                with path.open("rb") as source:
                    import_workspace(self.workspace, source, digest, self.policy["image"])
                return {"ok": True, "sha256": digest}
            if action == "provision":
                requested = payload.get("config", {})
                if not isinstance(requested, dict):
                    raise ValueError("invalid provision config")
                if requested.get("image", self.policy["image"]) != self.policy["image"]:
                    raise PermissionError("image not authorized by worker")
                if requested.get("gpu_vendor", self.policy["gpu_vendor"]) != self.policy["gpu_vendor"]:
                    raise PermissionError("GPU vendor not authorized by worker")
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
                if minimum and (not devices or any(item["free_bytes"] is None or item["free_bytes"] < minimum for item in devices)):
                    raise RuntimeError("worker free VRAM no longer meets request")
                policy = dict(self.policy)
                if devices:
                    devices = sorted(devices, key=lambda item: item["uuid"].lower())
                    policy["gpu_devices"] = ",".join(item["uuid"] for item in devices)
                    if self.policy["gpu_vendor"] != "nvidia":
                        policy["gpu_render_nodes"] = ",".join(item["render_node"] for item in devices)
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
            elif action == "leases":
                snap = self.driver.leases.snapshot()
                return {**snap, "runtime_id": self.runtime_id, "scope_note": SCOPE_NOTE}
            elif action == "leases_reconcile":
                live_workers = {self.runtime_id} if binding is not None else set()
                return self.driver.leases.reconcile(
                    live_workers=lambda: live_workers,
                    release_orphans=bool(payload.get("release_orphans", False)),
                )
            elif action == "leases_force_release":
                key = str(payload.get("uuid") or payload.get("lease_key") or "").strip()
                reason = str(payload.get("reason") or "").strip()
                if not key or not reason:
                    raise ValueError("uuid and reason are required")
                return self.driver.leases.force_release(key, reason)
            elif action == "cas_gc":
                if binding is not None or self.manager.operation_status().get("active"):
                    raise RuntimeError("cas_gc requires an idle unbound worker")
                cas = WorkspaceCasStore(self.cache / "cas", max_bytes=_MAX_CACHE)
                return cas.gc(self.runtime_id, min_tmp_age_seconds=float(payload.get("min_age_seconds", 300)))
            elif action == "migrate":
                return {
                    "runtime_id": self.runtime_id,
                    "live_supported": live_migration_supported()[0],
                    "live_reason": live_migration_supported()[1],
                    "mode": str(payload.get("mode") or "cold"),
                }
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
                length = self._length(_MAX_BLOB, allow_zstd=True)
                if self.headers.get("Content-Encoding") == "zstd":
                    raw_packed = self.rfile.read(length)
                    if len(raw_packed) != length:
                        raise ValueError("truncated blob")
                    raw_uncompressed = decompress_zstd_bytes(
                        raw_packed, max_bytes=_MAX_BLOB, window_log_max=ZSTD_WINDOW_LOG_MAX,
                    )
                    import io as _io
                    worker.put_blob(digest, _io.BytesIO(raw_uncompressed), len(raw_uncompressed))
                else:
                    worker.put_blob(digest, self.rfile, length)
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
                    accepts_zstd = "zstd" in (self.headers.get("Accept-Encoding") or "").lower()
                    if self.command == "GET" and accepts_zstd and 4096 <= size <= 16 * 1024 * 1024:
                        raw_data = handle.read(size)
                        if should_compress_zstd(raw_data[:16], size):
                            packed = compress_zstd_bytes(raw_data, level=3)
                            if len(packed) < size:
                                self.send_response(200)
                                self.send_header("Content-Type", "application/octet-stream")
                                self.send_header("Content-Encoding", "zstd")
                                self.send_header("Vary", "Accept-Encoding")
                                self.send_header("Content-Length", str(len(packed)))
                                self.send_header("Cache-Control", "no-store")
                                self.send_header("Connection", "close")
                                self.end_headers()
                                self.close_connection = True
                                self.wfile.write(packed)
                                self.wfile.flush()
                                worker.counters["downloaded_bytes"] += len(packed)
                                os.utime(blob, None)
                                return
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
    parser.add_argument("--gpu-vendor", choices=["nvidia", "amd", "intel"], default="nvidia")
    parser.add_argument("--mount-manifest", type=Path, help="Operator-only pinned GitHub/S3 mount manifest")
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
                    gpu_min_free_bytes=args.gpu_min_free_bytes, gpu_vendor=args.gpu_vendor,
                    mount_manifest=args.mount_manifest)
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

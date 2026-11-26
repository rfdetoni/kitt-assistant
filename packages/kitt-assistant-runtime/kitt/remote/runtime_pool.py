"""Operator-only bounded selection of a dedicated remote worker."""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import os
import re
import threading
import time

from kitt.runtime.conversation_runtime import RemoteRuntimeDriver, _remote_endpoint, _runtime_id

_RESOURCE_KEYS = ("cpu_millis", "memory_bytes", "pids_limit", "gpu_count")
_BOUNDS = ((10, 256000), (16 * 1024 * 1024, 1024 ** 4), (16, 65536), (0, 16))
_CB_LOCK = threading.Lock()
_CIRCUIT_BREAKER: dict[tuple[str, str], dict[str, float | int]] = {}
_CB_THRESHOLD = 3
_CB_COOLDOWN_SECONDS = 30.0


def _resources(value):
    result = []
    for key, (low, high) in zip(_RESOURCE_KEYS, _BOUNDS, strict=True):
        number = value.get(key)
        if type(number) is not int or not low <= number <= high:
            raise ValueError(f"invalid worker resource: {key}")
        result.append(number)
    return tuple(result)


def provision_best_worker(rpc, params):
    """Probe at most eight explicitly configured workers, then provision once.

    No discovery broadcast, background scheduler, or fallback after dispatch.
    Dedicated worker availability is advisory until provision succeeds.
    """
    conversation = str(params["conversation_id"])
    if rpc.dispatch("runtime.status", {"conversation_id": conversation}).get("binding") is not None:
        raise RuntimeError("conversation already has a runtime")
    nodes = params.get("workers")
    if not isinstance(nodes, list) or not 1 <= len(nodes) <= 8:
        raise ValueError("provide 1..8 explicit worker configurations")
    requested = _resources(params.get("resources", {}))
    minimum_vram = params["resources"].get("gpu_min_free_bytes", 0)
    if type(minimum_vram) is not int or not 0 <= minimum_vram <= 1024 ** 4 or (minimum_vram and not requested[3]):
        raise ValueError("invalid minimum per-GPU VRAM")
    gpu_vendor = params["resources"].get("gpu_vendor")
    if gpu_vendor is not None and (gpu_vendor not in ("nvidia", "amd", "intel") or not requested[3]):
        raise ValueError("invalid requested GPU vendor")
    allow_agent_execution = params.get("agent_execute", False)
    if type(allow_agent_execution) is not bool:
        raise ValueError("agent_execute must be boolean")
    transport_zstd = params.get("transport_zstd", False)
    if type(transport_zstd) is not bool:
        raise ValueError("transport_zstd must be boolean")
    image = params.get("image")
    if not isinstance(image, str) or not re.fullmatch(r"[^\s,]+@sha256:[0-9a-f]{64}", image):
        raise ValueError("image must be pinned by digest")
    configs = []
    seen = set()
    for node in nodes:
        if not isinstance(node, dict) or set(node) - {"endpoint", "runtime_id", "token_env"}:
            raise ValueError("worker configuration accepts endpoint, runtime_id and token_env")
        endpoint, _ = _remote_endpoint(str(node.get("endpoint") or ""))
        rid = _runtime_id(str(node.get("runtime_id") or ""))
        token_env = node.get("token_env", "KITT_REMOTE_RUNTIME_TOKEN")
        if not isinstance(token_env, str) or not re.fullmatch(r"KITT_REMOTE_RUNTIME_TOKEN(?:_[A-Z0-9_]{1,64})?", token_env):
            raise ValueError("invalid worker token environment reference")
        if len(os.environ.get(token_env, "")) < 32:
            raise PermissionError("worker authentication must be configured")
        if (endpoint, rid) in seen:
            raise ValueError("duplicate worker")
        seen.add((endpoint, rid))
        configs.append({"endpoint": endpoint, "runtime_id": rid, "token_env": token_env})
    driver = RemoteRuntimeDriver()
    now = time.monotonic()

    def _probe(indexed):
        index, config = indexed
        key = (config["endpoint"], config["runtime_id"])
        with _CB_LOCK:
            cb = _CIRCUIT_BREAKER.get(key)
            if cb and float(cb.get("open_until", 0.0)) > now:
                return None
        try:
            report = driver._request(config["runtime_id"], "capacity", {**config, "timeout_seconds": 8})
            policy = report.get("policy")
            with _CB_LOCK:
                _CIRCUIT_BREAKER.pop(key, None)
            if report.get("available") is not True or not isinstance(policy, dict) or policy.get("image") != image:
                return None
            capacity = _resources(policy)
        except (ValueError, RuntimeError, OSError):
            with _CB_LOCK:
                entry = _CIRCUIT_BREAKER.setdefault(key, {"failures": 0, "open_until": 0.0})
                entry["failures"] = int(entry["failures"]) + 1
                if entry["failures"] >= _CB_THRESHOLD:
                    entry["open_until"] = time.monotonic() + _CB_COOLDOWN_SECONDS
            return None
        if any(have < need for have, need in zip(capacity, requested, strict=True)):
            return None
        if requested[3]:
            detected = report.get("gpu_detected_count")
            free = report.get("gpu_min_available_vram_bytes")
            vendor = policy.get("gpu_vendor", "nvidia")
            if vendor not in ("nvidia", "amd", "intel") or (gpu_vendor is not None and vendor != gpu_vendor):
                return None
            if type(detected) is not int or detected < requested[3]:
                return None
            if minimum_vram and (type(free) is not int or free < minimum_vram):
                return None
        gpu_excess = capacity[3] - requested[3]
        mem_excess = capacity[1] - requested[1]
        cpu_excess = capacity[0] - requested[0]
        pids_excess = capacity[2] - requested[2]
        score = (gpu_excess, mem_excess, cpu_excess, pids_excess, index)
        breakdown = {
            "gpu_excess": gpu_excess,
            "memory_excess_bytes": mem_excess,
            "cpu_excess_millis": cpu_excess,
            "pids_excess": pids_excess,
            "rank_index": index,
        }
        return (score, config, capacity, policy.get("gpu_vendor", "nvidia"), breakdown)

    if len(configs) <= 1:
        probed = [_probe((0, configs[0]))]
    else:
        with ThreadPoolExecutor(max_workers=min(8, len(configs))) as pool:
            probed = list(pool.map(_probe, enumerate(configs)))
    candidates = [item for item in probed if item is not None]
    if not candidates:
        raise RuntimeError("no available permitted worker meets requested resources")
    _, selected, capacity, selected_vendor, breakdown = min(candidates, key=lambda item: item[0])
    config = {
        **selected, **dict(zip(_RESOURCE_KEYS, capacity, strict=True)),
        "image": image, "network_disabled": True, "timeout_seconds": 120,
        "gpu_min_free_bytes": minimum_vram, "agent_execute": allow_agent_execution,
        "transport_zstd": transport_zstd,
        **({"gpu_vendor": selected_vendor} if capacity[3] else {}),
    }
    result = rpc.dispatch("runtime.provision", {
        "conversation_id": conversation, "backend": "REMOTE", "config": config,
    })
    return {"worker": {"runtime_id": selected["runtime_id"],
                       **dict(zip(_RESOURCE_KEYS, capacity, strict=True))},
            "runtime": result, "score_breakdown": breakdown}

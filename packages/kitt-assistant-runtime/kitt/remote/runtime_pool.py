"""Operator-only bounded selection of a dedicated remote worker."""
from __future__ import annotations

import os
import re

from kitt.runtime.conversation_runtime import RemoteRuntimeDriver, _remote_endpoint, _runtime_id

_RESOURCE_KEYS = ("cpu_millis", "memory_bytes", "pids_limit", "gpu_count")
_BOUNDS = ((10, 256000), (16 * 1024 * 1024, 1024 ** 4), (16, 65536), (0, 16))


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
    candidates = []
    for index, config in enumerate(configs):
        try:
            report = driver._request(config["runtime_id"], "capacity",
                                     {**config, "timeout_seconds": 8})
            policy = report.get("policy")
            if report.get("available") is not True or not isinstance(policy, dict) or policy.get("image") != image:
                continue
            capacity = _resources(policy)
        except (ValueError, RuntimeError, OSError):
            continue
        if any(have < need for have, need in zip(capacity, requested, strict=True)):
            continue
        if requested[3]:
            detected = report.get("gpu_detected_count")
            free = report.get("gpu_min_available_vram_bytes")
            vendor = policy.get("gpu_vendor", "nvidia")
            if vendor not in ("nvidia", "amd", "intel") or (gpu_vendor is not None and vendor != gpu_vendor):
                continue
            if type(detected) is not int or detected < requested[3]:
                continue
            if minimum_vram and (type(free) is not int or free < minimum_vram):
                continue
        # Reserve scarce configured GPUs first, then minimize excess RAM/CPU.
        score = (capacity[3] - requested[3], capacity[1] - requested[1],
                 capacity[0] - requested[0], capacity[2] - requested[2], index)
        candidates.append((score, config, capacity, policy.get("gpu_vendor", "nvidia")))
    if not candidates:
        raise RuntimeError("no available permitted worker meets requested resources")
    _, selected, capacity, selected_vendor = min(candidates, key=lambda item: item[0])
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
            "runtime": result}

"""Shared host GPU discovery, atomic lease coordination, and VRAM allocation."""
from __future__ import annotations

import json
import os
import re
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Callable, Iterator

try:
    import fcntl
except ImportError:  # pragma: no cover - Windows host fallback
    fcntl = None  # type: ignore[assignment]

VRAM_SAFETY_MARGIN_BYTES = 256 * 1024 * 1024
_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
_NVIDIA_UUID_RE = re.compile(r"^GPU-[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}$")
_MIG_UUID_RE = re.compile(r"^MIG-[0-9A-Fa-f-]{16,80}$")
_PCI_DRM_RE = re.compile(r"^(AMD|INTEL)-[0-9A-Fa-f]{4}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}\.[0-9A-Fa-f]$")

VALID_LEASE_STATES = frozenset({"RESERVING", "ACTIVE", "RELEASING", "RELEASED", "INDETERMINATE"})
SCOPE_NOTE = (
    "Exclusive within KITT-managed workers and lease containers on this host; "
    "external host processes outside KITT cannot be preempted and are only inferred via free VRAM."
)


def valid_gpu_uuid(value: str) -> bool:
    text = str(value or "").strip()
    return bool(
        _NVIDIA_UUID_RE.fullmatch(text)
        or _MIG_UUID_RE.fullmatch(text)
        or _PCI_DRM_RE.fullmatch(text)
    )


def _atomic_write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, sort_keys=True, separators=(",", ":"))
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    finally:
        if tmp.exists():
            tmp.unlink(missing_ok=True)


class HostGpuLeaseStore:
    """File-locked host lease ledger with explicit state transitions and rollback."""

    def __init__(self, state_dir: Path | str) -> None:
        self.state_dir = Path(state_dir)
        self.path = self.state_dir / "gpu_leases.json"
        self.lock_path = self.state_dir / "gpu_leases.lock"

    @contextmanager
    def _locked(self) -> Iterator[dict[str, Any]]:
        self.state_dir.mkdir(parents=True, exist_ok=True)
        fd = os.open(self.lock_path, os.O_RDWR | os.O_CREAT, 0o600)
        try:
            with os.fdopen(fd, "a+", encoding="utf-8") as lock_file:
                if fcntl is not None:
                    fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
                try:
                    data = self._read_unlocked()
                    yield data
                finally:
                    if fcntl is not None:
                        fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)
        except OSError:
            raise

    def _read_unlocked(self) -> dict[str, Any]:
        if not self.path.exists():
            return {"version": 2, "scope_note": SCOPE_NOTE, "leases": {}}
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {"version": 2, "scope_note": SCOPE_NOTE, "leases": {}}
        if not isinstance(raw, dict) or not isinstance(raw.get("leases"), dict):
            return {"version": 2, "scope_note": SCOPE_NOTE, "leases": {}}
        return {
            "version": 2,
            "scope_note": SCOPE_NOTE,
            "leases": dict(raw["leases"]),
        }

    def snapshot(self) -> dict[str, Any]:
        with self._locked() as data:
            return json.loads(json.dumps(data))

    def active_uuids(self) -> set[str]:
        with self._locked() as data:
            return {
                uuid
                for uuid, rec in data["leases"].items()
                if isinstance(rec, dict) and rec.get("state") in {"RESERVING", "ACTIVE", "RELEASING", "INDETERMINATE"}
            }

    def allocated_vram_by_uuid(self) -> dict[str, int]:
        out: dict[str, int] = {}
        with self._locked() as data:
            for rec in data["leases"].values():
                if not isinstance(rec, dict):
                    continue
                if rec.get("state") not in {"RESERVING", "ACTIVE", "RELEASING", "INDETERMINATE"}:
                    continue
                uid = str(rec.get("uuid") or "")
                if not uid:
                    continue
                out[uid] = out.get(uid, 0) + max(0, int(rec.get("vram_bytes") or 0))
        return out

    def reserve_atomic(
        self,
        worker_id: str,
        devices: list[dict[str, Any]],
        *,
        backend: str,
        holder_id: str,
        mode: str = "exclusive",
        vram_per_gpu_bytes: int = 0,
    ) -> list[dict[str, Any]]:
        if not _ID_RE.fullmatch(worker_id):
            raise ValueError(f"invalid worker_id: {worker_id}")
        if mode not in {"exclusive", "shared", "mig"}:
            raise ValueError(f"invalid GPU lease mode: {mode}")
        now = time.time()
        with self._locked() as data:
            leases: dict[str, Any] = data["leases"]
            reserved_keys: list[str] = []
            try:
                for dev in devices:
                    uid = str(dev.get("uuid") or "").strip()
                    if not valid_gpu_uuid(uid):
                        raise ValueError(f"invalid GPU UUID: {uid}")
                    existing = [
                        rec
                        for rec in leases.values()
                        if isinstance(rec, dict)
                        and rec.get("uuid") == uid
                        and rec.get("state") in {"RESERVING", "ACTIVE", "RELEASING", "INDETERMINATE"}
                    ]
                    if existing and (mode == "exclusive" or any(r.get("mode", "exclusive") == "exclusive" for r in existing)):
                        raise RuntimeError(f"GPU {uid} already leased by {existing[0].get('owner_worker_id')}")
                    key = f"{uid}:{worker_id}" if mode != "exclusive" else uid
                    rec = {
                        "lease_key": key,
                        "uuid": uid,
                        "vendor": str(dev.get("vendor") or ("nvidia" if uid.startswith(("GPU-", "MIG-")) else "drm")),
                        "render_node": dev.get("render_node"),
                        "owner_worker_id": worker_id,
                        "backend": backend,
                        "holder_id": holder_id,
                        "container_id": None,
                        "lease_container": None,
                        "mode": mode,
                        "vram_bytes": max(0, int(vram_per_gpu_bytes)),
                        "state": "RESERVING",
                        "created_at": now,
                        "updated_at": now,
                    }
                    leases[key] = rec
                    reserved_keys.append(key)
                _atomic_write_json(self.path, data)
                return [dict(leases[k]) for k in reserved_keys]
            except Exception:
                for k in reserved_keys:
                    leases.pop(k, None)
                _atomic_write_json(self.path, data)
                raise

    def mark_active(
        self,
        worker_id: str,
        uuids: list[str],
        *,
        container_id: str | None = None,
        lease_containers: dict[str, str] | None = None,
    ) -> None:
        now = time.time()
        with self._locked() as data:
            leases = data["leases"]
            for key, rec in list(leases.items()):
                if not isinstance(rec, dict):
                    continue
                if rec.get("owner_worker_id") == worker_id and rec.get("uuid") in uuids:
                    rec["state"] = "ACTIVE"
                    if container_id:
                        rec["container_id"] = container_id
                    if lease_containers and rec.get("uuid") in lease_containers:
                        rec["lease_container"] = lease_containers[rec["uuid"]]
                    rec["updated_at"] = now
                    leases[key] = rec
            _atomic_write_json(self.path, data)

    def mark_state(self, worker_id: str, state: str, *, uuids: list[str] | None = None, reason: str | None = None) -> None:
        if state not in VALID_LEASE_STATES:
            raise ValueError(f"invalid lease state: {state}")
        now = time.time()
        with self._locked() as data:
            leases = data["leases"]
            for key, rec in list(leases.items()):
                if not isinstance(rec, dict):
                    continue
                if rec.get("owner_worker_id") != worker_id:
                    continue
                if uuids is not None and rec.get("uuid") not in uuids:
                    continue
                if state == "RELEASED":
                    leases.pop(key, None)
                else:
                    rec["state"] = state
                    rec["updated_at"] = now
                    if reason:
                        rec["reason"] = str(reason)[:240]
                    leases[key] = rec
            _atomic_write_json(self.path, data)

    def owned_by(self, worker_id: str) -> list[dict[str, Any]]:
        with self._locked() as data:
            return [
                dict(rec)
                for rec in data["leases"].values()
                if isinstance(rec, dict) and rec.get("owner_worker_id") == worker_id
            ]

    def force_release(self, uuid_or_key: str, reason: str) -> dict[str, Any]:
        if not reason or not str(reason).strip():
            raise ValueError("operator reason is required for force release")
        removed: list[dict[str, Any]] = []
        with self._locked() as data:
            leases = data["leases"]
            for key in list(leases):
                rec = leases[key]
                if not isinstance(rec, dict):
                    continue
                if key == uuid_or_key or rec.get("uuid") == uuid_or_key or rec.get("owner_worker_id") == uuid_or_key:
                    removed.append(leases.pop(key))
            _atomic_write_json(self.path, data)
        return {"released": len(removed), "records": removed, "reason": str(reason)[:240]}

    def reconcile(
        self,
        *,
        live_workers: Callable[[], set[str]] | None = None,
        live_containers: Callable[[], set[str]] | None = None,
        release_orphans: bool = False,
        stale_reserving_seconds: float = 120.0,
    ) -> dict[str, Any]:
        active_workers = live_workers() if live_workers else None
        active_containers = live_containers() if live_containers else None
        now = time.time()
        orphans: list[dict[str, Any]] = []
        released: list[str] = []
        with self._locked() as data:
            leases = data["leases"]
            for key in list(leases):
                rec = leases[key]
                if not isinstance(rec, dict):
                    leases.pop(key, None)
                    continue
                owner = str(rec.get("owner_worker_id") or "")
                cid = rec.get("container_id")
                state = str(rec.get("state") or "")
                age = now - float(rec.get("updated_at") or rec.get("created_at") or now)
                is_orphan = False
                if active_workers is not None and owner and owner not in active_workers:
                    is_orphan = True
                if active_containers is not None and cid and cid not in active_containers:
                    is_orphan = True
                if state in {"RESERVING", "RELEASING"} and age > stale_reserving_seconds:
                    is_orphan = True
                if is_orphan:
                    orphans.append(dict(rec))
                    if release_orphans:
                        leases.pop(key, None)
                        released.append(key)
                    elif state != "INDETERMINATE":
                        rec["state"] = "INDETERMINATE"
                        rec["updated_at"] = now
                        leases[key] = rec
            if orphans or released:
                _atomic_write_json(self.path, data)
        return {"orphans": orphans, "released": released, "remaining": len(data["leases"])}


def select_gpus(
    discovered: list[dict[str, Any]],
    *,
    leased_uuids: set[str],
    allocated_vram: dict[str, int],
    requested_count: int,
    min_free_bytes: int = 0,
    requested_vram_bytes: int = 0,
    exclusive: bool = True,
    allow_unknown_vram: bool = False,
    explicit_uuids: list[str] | None = None,
) -> list[dict[str, Any]]:
    """Select free GPUs respecting exclusive locks, quantitative VRAM, and MIG."""
    if requested_count <= 0:
        return []
    wanted_min = max(0, int(min_free_bytes), int(requested_vram_bytes))
    candidates: list[dict[str, Any]] = []
    explicit_set = {u.strip() for u in (explicit_uuids or []) if u.strip()}
    for dev in discovered:
        uid = str(dev.get("uuid") or "").strip()
        if not uid:
            continue
        if explicit_set and uid not in explicit_set:
            continue
        if exclusive and uid in leased_uuids:
            continue
        free = dev.get("free_bytes")
        total = dev.get("total_bytes")
        if total is None or free is None:
            if not allow_unknown_vram and wanted_min > 0:
                continue
            if not allow_unknown_vram and explicit_set == set():
                # Permit non-CUDA DRM devices without telemetry only when VRAM requirement is 0
                if wanted_min > 0:
                    continue
        else:
            reserved = allocated_vram.get(uid, 0)
            margin = VRAM_SAFETY_MARGIN_BYTES if wanted_min > 0 else 0
            effective_free = int(free) - reserved - margin
            if effective_free < wanted_min:
                continue
        candidates.append(dev)
    if len(candidates) < requested_count:
        raise RuntimeError(
            f"insufficient GPU capacity: need {requested_count} device(s) with >= {wanted_min} bytes free VRAM"
        )
    # Prefer smallest-fit device meeting requirement to pack multi-tenant workloads cleanly.
    candidates.sort(key=lambda d: (int(d.get("free_bytes") or 0), str(d.get("uuid") or "")))
    return candidates[:requested_count]

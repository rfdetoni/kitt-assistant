"""Transactional 10-step worker migration orchestration (cold, warm, and live-gated)."""
from __future__ import annotations

import shutil
import time
import uuid
from typing import Any, Callable

VALID_MODES = frozenset({"cold", "warm", "live"})


def live_migration_supported() -> tuple[bool, str]:
    if shutil.which("criu") is None:
        return False, "CRIU binary not installed on host; live checkpoint/restore unavailable"
    return True, "CRIU binary present (requires privileged container runtime support)"


def migrate_worker(
    *,
    source_id: str,
    target_id: str,
    workspace_id: str,
    mode: str = "cold",
    export_snapshot: Callable[[str], dict[str, Any]],
    import_snapshot: Callable[[str, dict[str, Any]], dict[str, Any]],
    provision_target: Callable[[str], dict[str, Any]],
    health_check: Callable[[str], dict[str, Any]],
    pause_source: Callable[[str], dict[str, Any]],
    resume_target: Callable[[str], dict[str, Any]],
    cutover_routing: Callable[[str, str], dict[str, Any]],
    terminate_source: Callable[[str], dict[str, Any]],
    terminate_target: Callable[[str], dict[str, Any]],
) -> dict[str, Any]:
    """Execute explicit 10-step migration with fail-closed gates and pre-cutover rollback."""
    if mode not in VALID_MODES:
        raise ValueError(f"unsupported migration mode: {mode}")
    if source_id == target_id:
        raise ValueError("source_id and target_id must differ")
    if mode == "live":
        ok, reason = live_migration_supported()
        if not ok:
            raise RuntimeError(reason)

    op_id = f"mig-{uuid.uuid4().hex[:12]}"
    steps: list[dict[str, Any]] = []
    target_provisioned = False
    cutover_done = False

    def record(step: str, detail: dict[str, Any] | None = None) -> None:
        steps.append({"step": step, "at": time.time(), "detail": detail or {}})

    try:
        record("1.preflight", {"mode": mode, "source_id": source_id, "target_id": target_id})
        if mode == "cold":
            record("2.quiesce_source", pause_source(source_id))
        else:
            record("2.precopy_prepare", {"mode": mode})

        snap = export_snapshot(source_id)
        record("3.export_snapshot", {"digest": snap.get("digest"), "size": snap.get("size")})

        if mode in {"warm", "live"}:
            record("4.quiesce_source", pause_source(source_id))
        else:
            record("4.quiesce_confirmed", {"state": "PAUSED"})

        imported = import_snapshot(target_id, snap)
        record("5.import_snapshot", imported)

        prov = provision_target(target_id)
        target_provisioned = True
        record("6.provision_target", prov)

        health = health_check(target_id)
        if not health.get("ok", True):
            raise RuntimeError(f"target health check failed: {health}")
        record("7.health_verify", health)

        resumed = resume_target(target_id)
        record("8.resume_target", resumed)

        routed = cutover_routing(source_id, target_id)
        cutover_done = True
        record("9.cutover_routing", routed)

        cleaned = terminate_source(source_id)
        record("10.cleanup_source", cleaned)
        return {
            "status": "completed",
            "operation_id": op_id,
            "mode": mode,
            "workspace_id": workspace_id,
            "source_id": source_id,
            "target_id": target_id,
            "steps": steps,
        }
    except Exception as exc:
        rollback_detail: dict[str, Any] = {"attempted": False}
        if target_provisioned and not cutover_done:
            rollback_detail["attempted"] = True
            try:
                rollback_detail["target_cleanup"] = terminate_target(target_id)
            except Exception as cleanup_exc:  # pragma: no cover - defensive
                rollback_detail["cleanup_error"] = str(cleanup_exc)
        raise RuntimeError(f"migration {op_id} failed ({exc}); rollback={rollback_detail}") from exc

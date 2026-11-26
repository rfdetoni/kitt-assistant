"""Tests for shared GPU host leases, CAS dedup/CDC, adaptive Zstd, and 10-step migration."""
from __future__ import annotations

import hashlib
import json
import threading
from unittest.mock import Mock, patch
from urllib.request import Request, urlopen

import pytest

from kitt.remote.cas_store import (
    WorkspaceCasStore,
    compress_zstd_bytes,
    decompress_zstd_bytes,
    should_compress_zstd,
)
from kitt.remote.gpu_manager import HostGpuLeaseStore, select_gpus
from kitt.remote.migration import live_migration_supported, migrate_worker
from kitt.remote.runtime_worker import RuntimeHTTPServer, Worker, WorkerDriver
from kitt.remote.workspace_snapshot import export_workspace_v2, import_workspace_v2
from kitt_protocol import RuntimeBackend


def test_host_lease_atomicity_rollback_reconcile_and_force_release(tmp_path):
    store = HostGpuLeaseStore(tmp_path)
    g1 = {"uuid": "GPU-12345678-1234-1234-1234-123456789abc", "vendor": "nvidia"}
    g2 = {"uuid": "GPU-22345678-1234-1234-1234-123456789abc", "vendor": "nvidia"}
    store.reserve_atomic("w1", [g1], backend="docker", holder_id="/bin/docker", vram_per_gpu_bytes=2048)
    with pytest.raises(RuntimeError, match="already leased"):
        store.reserve_atomic("w2", [g2, g1], backend="docker", holder_id="/bin/docker")
    # Rollback of g2 leaves only g1 leased by w1
    assert store.active_uuids() == {g1["uuid"]}
    rec = store.reconcile(live_workers=lambda: set(), release_orphans=False)
    assert len(rec["orphans"]) == 1
    assert store.snapshot()["leases"][g1["uuid"]]["state"] == "INDETERMINATE"
    with pytest.raises(ValueError):
        store.force_release(g1["uuid"], "")
    released = store.force_release(g1["uuid"], "operator verified GPU idle")
    assert released["released"] == 1
    assert store.active_uuids() == set()


def test_select_gpus_vram_margin_and_unknown_vram():
    g_small = {"uuid": "GPU-11111111-1111-1111-1111-111111111111", "total_bytes": 4 << 30, "free_bytes": 2 << 30}
    g_big = {"uuid": "GPU-22222222-2222-2222-2222-222222222222", "total_bytes": 16 << 30, "free_bytes": 12 << 30}
    # Requiring 2 GiB free minus 256 MiB margin excludes g_small
    picked = select_gpus(
        [g_small, g_big],
        leased_uuids=set(),
        allocated_vram={},
        requested_count=1,
        requested_vram_bytes=2 << 30,
    )
    assert [d["uuid"] for d in picked] == [g_big["uuid"]]

    intel_unknown = {"uuid": "INTEL-0000:04:00.0", "total_bytes": None, "free_bytes": None}
    with pytest.raises(RuntimeError):
        select_gpus([intel_unknown], leased_uuids=set(), allocated_vram={}, requested_count=1, requested_vram_bytes=1024)
    assert select_gpus(
        [intel_unknown], leased_uuids=set(), allocated_vram={}, requested_count=1,
        requested_vram_bytes=1024, allow_unknown_vram=True,
    ) == [intel_unknown]


def test_podman_cdi_and_rocm_env_flags(tmp_path):
    driver = WorkerDriver(RuntimeBackend.PODMAN, "/bin/podman", tmp_path, tmp_path)
    gpu = "GPU-12345678-1234-1234-1234-123456789abc"
    with patch("kitt.remote.runtime_worker._run_host", side_effect=[
        {"success": True, "stdout": "a" * 64},
        {"success": True, "stdout": "cid1"},
    ]) as run:
        driver.provision({
            "runtime_id": "pod-gpu",
            "image": "example@sha256:" + "a" * 64,
            "cpu_millis": 1000,
            "memory_bytes": 256 << 20,
            "pids_limit": 32,
            "gpu_count": 1,
            "gpu_vendor": "nvidia",
            "gpu_devices": gpu,
        })
    argv = run.call_args.args[0]
    assert "--device" in argv and f"nvidia.com/gpu={gpu}" in argv

    amd_driver = WorkerDriver(RuntimeBackend.DOCKER, "/bin/docker", tmp_path, tmp_path)
    amd_gpu = "AMD-0000:03:00.0"
    with patch("kitt.remote.runtime_worker._run_host", side_effect=[
        {"success": True, "stdout": "b" * 64},
        {"success": False, "stderr": "daemon rejected"},
        {"success": False, "stderr": "No such container"},
        {"success": True},
    ]), patch.object(amd_driver, "_drm_nodes", return_value=["/dev/dri/renderD128"]):
        with pytest.raises(RuntimeError, match="rolled back"):
            amd_driver.provision({
                "runtime_id": "amd-w",
                "image": "example@sha256:" + "a" * 64,
                "cpu_millis": 1000,
                "memory_bytes": 256 << 20,
                "pids_limit": 32,
                "gpu_count": 1,
                "gpu_vendor": "amd",
                "gpu_devices": amd_gpu,
                "gpu_render_nodes": "/dev/dri/renderD128",
            })
    # Rolled back host lease cleanly when inspect confirms container never existed
    assert amd_gpu not in amd_driver.leases.active_uuids()
    assert gpu in driver.leases.active_uuids()


def test_cas_cdc_snapshot_v2_and_gc(tmp_path):
    import random
    cas = WorkspaceCasStore(tmp_path / "cas")
    ws = tmp_path / "ws"
    ws.mkdir()
    data = random.Random(42).randbytes(900 * 1024)
    (ws / "alpha.bin").write_bytes(data)
    (ws / "obsolete.txt").write_text("old")
    image = "example@sha256:" + "a" * 64
    m1 = export_workspace_v2(ws, cas, "ws-a", image, chunking="cdc")
    assert m1["version"] == 2 and m1["dedup"]["uploaded_chunks"] >= 2
    assert not cas.has_blob("ws-b", m1["entries"][0]["chunks"][0]["sha256"])

    # Prepend 4 KiB to trigger boundary shift: CDC realigns and reuses tail chunks
    (ws / "alpha.bin").write_bytes(b"PREFIX" * 680 + data)
    (ws / "obsolete.txt").unlink()
    m2 = export_workspace_v2(ws, cas, "ws-a", image, chunking="cdc", parent_manifest=m1)
    assert m2["deleted"] == ["obsolete.txt"]
    assert m2["dedup"]["reused_chunks"] >= 1

    restore = tmp_path / "restore"
    import_workspace_v2(restore, cas, "ws-a", m1, image)
    assert (restore / "obsolete.txt").exists()
    import_workspace_v2(restore, cas, "ws-a", m2, image)
    assert not (restore / "obsolete.txt").exists()
    assert (restore / "alpha.bin").read_bytes() == b"PREFIX" * 680 + data

    # Orphan GC keeps referenced blobs and removes unbound ones
    orphan_digest, _, _ = cas.put_bytes("ws-a", b"unreferenced-orphan-data")
    cas.unbind_snapshot("ws-a", m1["snapshot_id"])
    gc_res = cas.gc("ws-a")
    assert gc_res["removed_blobs"] >= 1
    assert not cas.has_blob("ws-a", orphan_digest)


def test_adaptive_zstd_blob_transfer_and_bomb_guard(tmp_path):
    assert not should_compress_zstd(b"tiny", 4)
    assert not should_compress_zstd(b"\x89PNG\r\n\x1a\n" + b"0" * 5000, 5008)
    assert should_compress_zstd(b"hello " * 1000, 6000)

    packed = compress_zstd_bytes(b"hello " * 1000)
    assert decompress_zstd_bytes(packed, max_bytes=16384) == b"hello " * 1000
    with pytest.raises(ValueError, match="trailing"):
        decompress_zstd_bytes(packed + b"extra", max_bytes=16384)
    with pytest.raises(ValueError):
        decompress_zstd_bytes(compress_zstd_bytes(b"A" * 5000), max_bytes=1024)

    image = "example@sha256:" + "a" * 64
    with patch("kitt.remote.runtime_worker.shutil.which", return_value="/bin/docker"):
        worker = Worker(tmp_path, "zstd-w", image=image)
    worker.manager = Mock()
    worker.manager.binding.return_value = None
    token = "z" * 48
    server = RuntimeHTTPServer(("127.0.0.1", 0), worker, token)
    t = threading.Thread(target=server.serve_forever, daemon=True)
    t.start()
    base = f"http://127.0.0.1:{server.server_port}/v1/runtimes/zstd-w/blobs"
    try:
        raw = b"COMPRESSIBLE-BLOCK-" * 400
        digest = hashlib.sha256(raw).hexdigest()
        req = Request(
            f"{base}/{digest}", data=compress_zstd_bytes(raw), method="PUT",
            headers={"Authorization": f"Bearer {token}", "Content-Encoding": "zstd"},
        )
        with urlopen(req, timeout=3) as resp:
            assert json.load(resp)["sha256"] == digest
        get_req = Request(
            f"{base}/{digest}", method="GET",
            headers={"Authorization": f"Bearer {token}", "Accept-Encoding": "zstd"},
        )
        with urlopen(get_req, timeout=3) as resp:
            assert resp.headers.get("Content-Encoding") == "zstd"
            assert decompress_zstd_bytes(resp.read(), max_bytes=len(raw) + 16) == raw
    finally:
        server.shutdown()
        server.server_close()
        t.join(timeout=3)
        worker.db.close()


def test_10_step_migration_and_criu_gate():
    log = []
    out = migrate_worker(
        source_id="w1", target_id="w2", workspace_id="ws", mode="warm",
        export_snapshot=lambda s: log.append(("exp", s)) or {"digest": "d", "size": 10},
        import_snapshot=lambda t, sn: log.append(("imp", t)) or {"ok": True},
        provision_target=lambda t: log.append(("prov", t)) or {"runtime_id": t},
        health_check=lambda t: {"ok": True},
        pause_source=lambda s: {"state": "PAUSED"},
        resume_target=lambda t: {"state": "RUNNING"},
        cutover_routing=lambda s, t: {"active": t},
        terminate_source=lambda s: {"terminated": s},
        terminate_target=lambda t: {"rolled_back": t},
    )
    assert out["status"] == "completed"
    assert len(out["steps"]) == 10

    rolled_back = []
    with pytest.raises(RuntimeError, match="target health check failed"):
        migrate_worker(
            source_id="w1", target_id="w2", workspace_id="ws", mode="cold",
            export_snapshot=lambda s: {"digest": "d", "size": 10},
            import_snapshot=lambda t, sn: {"ok": True},
            provision_target=lambda t: {"runtime_id": t},
            health_check=lambda t: {"ok": False},
            pause_source=lambda s: {"state": "PAUSED"},
            resume_target=lambda t: {},
            cutover_routing=lambda s, t: {},
            terminate_source=lambda s: {},
            terminate_target=lambda t: rolled_back.append(t) or {"ok": True},
        )
    assert rolled_back == ["w2"]

    with patch("kitt.remote.migration.shutil.which", return_value=None):
        ok, _ = live_migration_supported()
        assert not ok
        with pytest.raises(RuntimeError, match="CRIU"):
            migrate_worker(
                source_id="w1", target_id="w2", workspace_id="ws", mode="live",
                export_snapshot=lambda s: {}, import_snapshot=lambda t, sn: {},
                provision_target=lambda t: {}, health_check=lambda t: {},
                pause_source=lambda s: {}, resume_target=lambda t: {},
                cutover_routing=lambda s, t: {}, terminate_source=lambda s: {},
                terminate_target=lambda t: {},
            )

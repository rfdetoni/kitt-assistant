"""Explicit operator benchmark; no resident benchmark task and no implicit execution."""
import argparse
import hashlib
import io
import json
import os
import statistics
import tempfile
import time
from pathlib import Path

from kitt.artifacts.remote_cache import RemoteArtifactCache
from kitt.remote.cas_store import WorkspaceCasStore, compress_zstd_bytes, decompress_zstd_bytes
from kitt.remote.workspace_snapshot import export_workspace_v2
from kitt.runtime.conversation_runtime import RemoteRuntimeDriver


def _percentiles_ms(samples_s: list[float]) -> dict[str, float]:
    ordered = sorted(s * 1000.0 for s in samples_s)
    if not ordered:
        return {"p50_ms": 0.0, "p95_ms": 0.0, "p99_ms": 0.0}
    n = len(ordered)
    return {
        "p50_ms": round(statistics.median(ordered), 3),
        "p95_ms": round(ordered[min(n - 1, int(0.95 * (n - 1)))], 3),
        "p99_ms": round(ordered[min(n - 1, int(0.99 * (n - 1)))], 3),
    }


def run_local_suite(*, samples: int = 12, warmup: int = 2) -> dict:
    """Reproducible local CPU, RAM, Zstd, and Fixed-vs-CDC snapshot benchmark."""
    samples = max(3, min(100, int(samples)))
    warmup = max(1, min(10, int(warmup)))
    payload = (b"KITT-RUNTIME-BENCHMARK-" * 180000)[: 4 * 1024 * 1024]  # 4 MiB deterministic text
    incompressible = os.urandom(512 * 1024)

    for _ in range(warmup):
        hashlib.sha256(payload).digest()
        bytearray(payload).copy()
        decompress_zstd_bytes(compress_zstd_bytes(payload[: 512 * 1024], level=3), max_bytes=1024 * 1024)

    sha_times, copy_times, zstd_c_times, zstd_d_times = [], [], [], []
    comp_ratio = 1.0
    for _ in range(samples):
        t0 = time.perf_counter()
        hashlib.sha256(payload).digest()
        sha_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        _ = bytearray(payload).copy()
        copy_times.append(time.perf_counter() - t0)

        t0 = time.perf_counter()
        packed = compress_zstd_bytes(payload[: 512 * 1024], level=3)
        zstd_c_times.append(time.perf_counter() - t0)
        comp_ratio = len(packed) / (512 * 1024)

        t0 = time.perf_counter()
        _ = decompress_zstd_bytes(packed, max_bytes=1024 * 1024)
        zstd_d_times.append(time.perf_counter() - t0)

    with tempfile.TemporaryDirectory(prefix="kitt-bench-") as tmp:
        root = Path(tmp)
        ws = root / "ws"
        ws.mkdir()
        (ws / "large.bin").write_bytes(payload[: 2 * 1024 * 1024])
        (ws / "random.bin").write_bytes(incompressible)
        image = "example@sha256:" + "a" * 64

        cas_fixed = WorkspaceCasStore(root / "cas-fixed")
        t0 = time.perf_counter()
        m1 = export_workspace_v2(ws, cas_fixed, "bench", image, chunking="fixed")
        fixed_first_s = time.perf_counter() - t0
        # Prepend 4 KiB to test shift sensitivity (fixed breaks all shifted chunks; CDC preserves tail)
        (ws / "large.bin").write_bytes(b"HEAD" * 1024 + payload[: 2 * 1024 * 1024 - 4096])
        m2 = export_workspace_v2(ws, cas_fixed, "bench", image, chunking="fixed", parent_manifest=m1)

        (ws / "large.bin").write_bytes(payload[: 2 * 1024 * 1024])
        cas_cdc = WorkspaceCasStore(root / "cas-cdc")
        t0 = time.perf_counter()
        c1 = export_workspace_v2(ws, cas_cdc, "bench", image, chunking="cdc")
        cdc_first_s = time.perf_counter() - t0
        (ws / "large.bin").write_bytes(b"HEAD" * 1024 + payload[: 2 * 1024 * 1024 - 4096])
        c2 = export_workspace_v2(ws, cas_cdc, "bench", image, chunking="cdc", parent_manifest=c1)

    return {
        "mode": "local_suite",
        "warmup": warmup,
        "samples": samples,
        "cpu_sha256": {"mib_per_s": round(4.0 / max(statistics.median(sha_times), 1e-9), 2), **_percentiles_ms(sha_times)},
        "ram_copy": {"mib_per_s": round(4.0 / max(statistics.median(copy_times), 1e-9), 2), **_percentiles_ms(copy_times)},
        "zstd_level3": {
            "compression_ratio": round(comp_ratio, 4),
            "compress_mib_per_s": round(0.5 / max(statistics.median(zstd_c_times), 1e-9), 2),
            "decompress_mib_per_s": round(0.5 / max(statistics.median(zstd_d_times), 1e-9), 2),
            "compress_latency": _percentiles_ms(zstd_c_times),
            "decompress_latency": _percentiles_ms(zstd_d_times),
        },
        "chunking_shift_dedup": {
            "fixed_first_ms": round(fixed_first_s * 1000.0, 2),
            "fixed_shifted_dedup": m2["dedup"],
            "cdc_first_ms": round(cdc_first_s * 1000.0, 2),
            "cdc_shifted_dedup": c2["dedup"],
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint")
    parser.add_argument("--runtime-id")
    parser.add_argument("--local-suite", action="store_true", help="Run local CPU/RAM/Zstd/CAS CDC benchmark without remote worker")
    parser.add_argument("--samples", type=int, default=12)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--transfer", action="store_true", help="Upload and verify a fresh 1 MiB cache blob")
    parser.add_argument("--compute", action="store_true", help="Run fixed CPU/RAM probes in an already-running dedicated Python worker")
    parser.add_argument("--nvidia", action="store_true", help="Verify actual nvidia-smi visibility inside the running worker")
    parser.add_argument("--gpu-compute", choices=["cuda", "xpu"],
                        help="Run a small synchronized PyTorch matrix workload on NVIDIA/ROCm or Intel")
    args = parser.parse_args()
    if args.local_suite:
        print(json.dumps(run_local_suite(samples=args.samples, warmup=args.warmup), indent=2, sort_keys=True))
        return
    if not args.endpoint or not args.runtime_id:
        parser.error("--endpoint and --runtime-id are required unless --local-suite is specified")
    token = os.environ.get("KITT_REMOTE_RUNTIME_TOKEN", "")
    if len(token) < 32:
        parser.error("KITT_REMOTE_RUNTIME_TOKEN must be configured")
    config = {"endpoint": args.endpoint, "timeout_seconds": 30, "transport_zstd": True}
    driver = RemoteRuntimeDriver(lambda _: token)
    result = {}
    start = time.perf_counter()
    result["capacity"] = driver._request(args.runtime_id, "capacity", config)
    result["capacity_latency_ms"] = (time.perf_counter() - start) * 1000
    result["metrics"] = driver._request(args.runtime_id, "metrics", config)
    if args.transfer:
        data = os.urandom(1024 * 1024)
        digest = hashlib.sha256(data).hexdigest()
        cache = RemoteArtifactCache(config, args.runtime_id, token)
        start = time.perf_counter()
        transferred = cache.push(io.BytesIO(data), digest, len(data))
        upload = time.perf_counter() - start
        start = time.perf_counter()
        with cache.open_blob(digest) as response:
            got = hashlib.file_digest(response, "sha256").hexdigest()
        download = time.perf_counter() - start
        if not transferred or got != digest:
            raise RuntimeError("network benchmark integrity failure")
        result["network"] = {"bytes": len(data), "upload_mib_s": 1 / max(upload, 1e-9),
                             "download_mib_s": 1 / max(download, 1e-9), "sha256": digest}
    if args.compute or args.nvidia or args.gpu_compute:
        if result["metrics"]["state"] != "RUNNING":
            raise RuntimeError("compute probes require an already-running dedicated worker")
    if args.compute:
        code = ("import hashlib,time,json; b=bytearray(4*1024*1024); "
                "t=time.perf_counter(); [hashlib.sha256(b).digest() for _ in range(32)]; "
                "cpu=128/max(time.perf_counter()-t,1e-9); "
                "t=time.perf_counter(); [b.copy() for _ in range(1)]; "
                "ram=4/max(time.perf_counter()-t,1e-9); "
                "print(json.dumps({'sha256_mib_s':cpu,'copy_mib_s':ram}))")
        probe = driver.execute(args.runtime_id, ["python3", "-c", code], config)
        if not probe.get("success"):
            raise RuntimeError("CPU/RAM probe requires Python in worker image")
        result["compute"] = json.loads(probe["stdout"])
    if args.nvidia:
        probe = driver.execute(args.runtime_id, [
            "nvidia-smi", "--query-gpu=uuid,memory.total,memory.free", "--format=csv,noheader,nounits",
        ], config)
        if not probe.get("success") or not probe.get("stdout", "").strip():
            raise RuntimeError("NVIDIA GPU is not visible inside worker")
        result["nvidia_inside_worker"] = probe["stdout"].strip()
    if args.gpu_compute:
        code = (
            "import sys,time,json,torch; d=sys.argv[1]; "
            "backend=getattr(torch,d); assert backend.is_available(); "
            "a=torch.ones((512,512),device=d,dtype=torch.float32); "
            "a@a; backend.synchronize(); t=time.perf_counter(); "
            "[a@a for _ in range(16)]; backend.synchronize(); "
            "print(json.dumps({'device':d,'iterations':16,'matrix_size':512,"
            "'seconds':time.perf_counter()-t}))"
        )
        probe = driver.execute(args.runtime_id, ["python3", "-c", code, args.gpu_compute], config)
        if not probe.get("success"):
            raise RuntimeError("GPU workload requires a compatible PyTorch build and physical device")
        result["gpu_compute"] = json.loads(probe["stdout"])
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()

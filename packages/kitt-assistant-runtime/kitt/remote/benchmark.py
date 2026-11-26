"""Explicit operator benchmark; no resident benchmark task and no implicit execution."""
import argparse
import hashlib
import io
import json
import os
import time

from kitt.artifacts.remote_cache import RemoteArtifactCache
from kitt.runtime.conversation_runtime import RemoteRuntimeDriver


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--runtime-id", required=True)
    parser.add_argument("--transfer", action="store_true", help="Upload and verify a fresh 1 MiB cache blob")
    parser.add_argument("--compute", action="store_true", help="Run fixed CPU/RAM probes in an already-running dedicated Python worker")
    parser.add_argument("--nvidia", action="store_true", help="Verify actual nvidia-smi visibility inside the running worker")
    args = parser.parse_args()
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
    if args.compute or args.nvidia:
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
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()

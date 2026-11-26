from hashlib import sha256
from io import BytesIO
import json
import threading
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from unittest.mock import Mock, patch

import pytest

from kitt.remote.runtime_worker import RuntimeHTTPServer, Worker


def test_remote_worker_policy_and_content_addressed_cache(tmp_path):
    image = "example@sha256:" + "a" * 64
    with patch("kitt.remote.runtime_worker.shutil.which", return_value="/bin/docker"):
        worker = Worker(tmp_path, "lab", image=image)
    worker.manager = Mock()
    worker.manager.binding.return_value = None
    token = "x" * 48
    server = RuntimeHTTPServer(("127.0.0.1", 0), worker, token)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f"http://127.0.0.1:{server.server_port}/v1/runtimes/lab"

    def call(path, method="GET", body=None, auth=True, extra_headers=None):
        headers = {"Authorization": "Bearer " + token} if auth else {}
        headers.update(extra_headers or {})
        return urlopen(Request(base + "/" + path, data=body, method=method,
                               headers=headers), timeout=3)

    try:
        with pytest.raises(HTTPError) as denied:
            call("health", "POST", b"{}", auth=False)
        assert denied.value.code == 401
        with pytest.raises(HTTPError) as denied:
            call("provision", "POST", json.dumps({"config": {"image": "evil"}}).encode())
        assert denied.value.code == 403
        worker.manager.provision.assert_not_called()
        worker.manager.operation_status.return_value = {"active": False}
        with patch("kitt.remote.runtime_worker._local_capacity", return_value=(4000, 1024 ** 3)):
            with call("capacity", "POST", b"{}") as response:
                capacity = json.load(response)
                assert capacity["available"] is True
                assert capacity["policy"]["gpu_count"] == 0
        with patch("kitt.remote.runtime_worker._local_capacity", return_value=(4000, 1024 ** 3)):
            with call("provision", "POST", b"{}") as response:
                assert json.load(response)["runtime_id"] == "lab"
        policy = worker.manager.provision.call_args.args[1]
        assert policy["network_disabled"] is True
        assert policy["image"] == image
        worker.manager.binding.return_value = Mock(state="RUNNING")
        worker.manager.execute.return_value = {"success": True, "stdout": "ok"}
        with call("execute", "POST", b'{"argv":["true"]}') as response:
            assert json.load(response)["success"]
        worker.manager.execute.assert_called_once_with(["true"], request_id=None)
        from compression import zstd
        raw = json.dumps({"argv": ["echo", "a" * 4096]}).encode()
        packed = zstd.compress(raw, level=1)
        worker.manager.execute.return_value = {"success": True, "stdout": "b" * 4096}
        headers = {"Content-Encoding": "zstd", "Accept-Encoding": "zstd"}
        with call("execute", "POST", packed, extra_headers=headers) as response:
            assert response.headers["Content-Encoding"] == "zstd"
            assert json.loads(zstd.decompress(response.read()))["stdout"] == "b" * 4096
        calls = worker.manager.execute.call_count
        for malformed in (packed[:-1], packed + packed, zstd.compress(b"x" * 65537)):
            with pytest.raises(HTTPError) as rejected:
                call("execute", "POST", malformed, extra_headers=headers)
            assert rejected.value.code == 400
        assert worker.manager.execute.call_count == calls
        from kitt.runtime.conversation_runtime import RemoteRuntimeDriver
        driver = RemoteRuntimeDriver(lambda _: token)
        assert driver.execute("lab", ["echo", "a" * 4096], {
            "endpoint": f"http://127.0.0.1:{server.server_port}",
            "transport_zstd": True,
        })["stdout"] == "b" * 4096
        data = b"payload" * 10000
        digest = sha256(data).hexdigest()
        with call("blobs/" + digest, "PUT", data) as response:
            assert json.load(response)["sha256"] == digest
        with call("blobs/" + digest) as response:
            assert response.read() == data
        with call("blobs/" + digest, "HEAD") as response:
            assert int(response.headers["Content-Length"]) == len(data)
        with pytest.raises(HTTPError) as rejected:
            call("blobs/" + digest, "PUT", b"wrong")
        assert rejected.value.code == 400
        assert worker.blob(digest).read_bytes() == data
        # Exercise the real Agent client against the real worker HTTP handler.
        from kitt.artifacts.remote_cache import RemoteArtifactCache
        cache = RemoteArtifactCache({"endpoint": f"http://127.0.0.1:{server.server_port}"}, "lab", token)
        assert not cache.push(BytesIO(data), digest, len(data))
        another = b"agent-client" * 1024
        another_hash = sha256(another).hexdigest()
        assert cache.push(BytesIO(another), another_hash, len(another))
        with cache.open_blob(another_hash) as stream:
            assert stream.read() == another
        with pytest.raises(ValueError):
            worker.put_blob("0" * 64, BytesIO(b"x"), 2)
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)
        worker.db.close()


def test_worker_container_flags_are_fixed(tmp_path):
    image = "example@sha256:" + "a" * 64
    with patch("kitt.remote.runtime_worker.shutil.which", return_value="/bin/docker"):
        worker = Worker(tmp_path, "lab", image=image)
    try:
        with patch("kitt.remote.runtime_worker._run_host", return_value={"success": True}) as run:
            worker.driver.provision(worker.policy)
        argv = run.call_args.args[0]
        assert argv[argv.index("--network") + 1] == "none"
        assert argv[argv.index("--cap-drop") + 1] == "ALL"
        assert "--read-only" in argv
        assert argv[argv.index("--pull") + 1] == "never"
        assert argv[argv.index("--security-opt") + 1] == "no-new-privileges"
        assert image in argv
    finally:
        worker.db.close()


def test_gpu_count_is_operator_fixed(tmp_path):
    image = "example@sha256:" + "a" * 64
    with patch("kitt.remote.runtime_worker.shutil.which", return_value="/bin/docker"):
        worker = Worker(tmp_path, "gpu", image=image, gpu_count=2)
    try:
        gpus = ["GPU-12345678-1234-1234-1234-123456789abc",
                "GPU-22345678-1234-1234-1234-123456789abc"]
        with patch("kitt.remote.runtime_worker._run_host", side_effect=[
            {"success": True, "stdout": "a" * 64},
            {"success": True, "stdout": "b" * 64},
            {"success": True},
        ]) as run:
            worker.driver.provision({**worker.policy, "gpu_devices": ",".join(gpus)})
        argv = run.call_args.args[0]
        assert argv[argv.index("--gpus") + 1] == '"device=' + ",".join(gpus) + '"'
        assert [call.args[0][1] for call in run.call_args_list] == ["create", "create", "run"]
        with pytest.raises(ValueError):
            worker.dispatch("provision", {"config": {"gpu_count": 3}})
    finally:
        worker.db.close()


def test_physical_gpu_discovery_and_rechecked_admission(tmp_path):
    from kitt.remote.runtime_worker import discover_nvidia
    image = "example@sha256:" + "a" * 64
    gpu = "GPU-12345678-1234-1234-1234-123456789abc"
    with patch("kitt.remote.runtime_worker.shutil.which", return_value="/bin/nvidia-smi"), patch(
        "kitt.remote.runtime_worker._run_host",
        return_value={"success": True, "stdout": f"{gpu}, 8192, 4096\n"},
    ):
        assert discover_nvidia(tmp_path) == [{"uuid": gpu, "total_bytes": 8192 * 1024 ** 2,
                                             "free_bytes": 4096 * 1024 ** 2}]
    with patch("kitt.remote.runtime_worker.shutil.which", return_value="/bin/docker"):
        worker = Worker(tmp_path, "gpu", image=image, gpu_count=1, gpu_min_free_bytes=1024 ** 3)
    try:
        with patch("kitt.remote.runtime_worker._local_capacity", return_value=(4000, 1024 ** 3)), patch(
            "kitt.remote.runtime_worker.discover_nvidia", return_value=[]
        ):
            assert not worker.capacity()["available"]
            with pytest.raises(RuntimeError, match="physical capacity"):
                worker.dispatch("provision", {})
        with patch("kitt.remote.runtime_worker._local_capacity", return_value=(4000, 1024 ** 3)), patch(
            "kitt.remote.runtime_worker.discover_nvidia", return_value=[
                {"uuid": gpu, "free_bytes": 4 * 1024 ** 3, "total_bytes": 8 * 1024 ** 3}
            ]
        ), patch.object(worker.driver, "leased_gpus", return_value=set()), patch.object(worker.manager, "provision") as provision:
            assert worker.capacity()["gpu_min_available_vram_bytes"] == 4 * 1024 ** 3
            worker.dispatch("provision", {"config": {"gpu_min_free_bytes": 2 * 1024 ** 3}})
            assert provision.call_args.args[1]["gpu_devices"] == gpu
    finally:
        worker.db.close()


def test_gpu_lease_conflict_rolls_back_only_own_new_leases(tmp_path):
    from kitt.remote.runtime_worker import WorkerDriver
    from kitt_protocol import RuntimeBackend
    driver = WorkerDriver(RuntimeBackend.DOCKER, "/bin/docker", tmp_path, tmp_path)
    gpus = ["GPU-12345678-1234-1234-1234-123456789abc",
            "GPU-22345678-1234-1234-1234-123456789abc"]
    with patch("kitt.remote.runtime_worker._run_host", side_effect=[
        {"success": True, "stdout": "a" * 64},
        {"success": False, "stderr": "conflict"},
        {"success": True},
    ]) as run:
        with pytest.raises(RuntimeError, match="leased"):
            driver._reserve_gpus(gpus, "worker-a", "example@sha256:" + "b" * 64)
    assert run.call_args_list[-1].args[0] == ["/bin/docker", "rm", "a" * 64]


def test_gpu_lease_ownership_mismatch_refuses_release(tmp_path):
    from kitt.remote.runtime_worker import WorkerDriver
    from kitt.runtime.conversation_runtime import RuntimeOutcomeUnknown
    from kitt_protocol import RuntimeBackend
    driver = WorkerDriver(RuntimeBackend.DOCKER, "/bin/docker", tmp_path, tmp_path)
    gpu = "GPU-12345678-1234-1234-1234-123456789abc"
    with patch("kitt.remote.runtime_worker._run_host", return_value={
        "success": True, "stdout": "a" * 64 + " worker-other",
    }) as run:
        with pytest.raises(RuntimeOutcomeUnknown, match="ownership mismatch"):
            driver.terminate("worker-a", {"gpu_count": 1, "gpu_devices": gpu})
    assert run.call_count == 1

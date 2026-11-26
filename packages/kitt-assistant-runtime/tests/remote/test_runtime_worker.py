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

    def call(path, method="GET", body=None, auth=True):
        headers = {"Authorization": "Bearer " + token} if auth else {}
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

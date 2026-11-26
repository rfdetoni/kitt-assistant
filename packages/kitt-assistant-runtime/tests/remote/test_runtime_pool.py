from unittest.mock import Mock, patch

import pytest

from kitt.remote.runtime_pool import provision_best_worker


def test_pool_best_fit_and_no_retry_after_provision(monkeypatch):
    monkeypatch.setenv("KITT_REMOTE_RUNTIME_TOKEN", "x" * 48)
    image = "example@sha256:" + "a" * 64
    nodes = [{"endpoint": "http://127.0.0.1:7348", "runtime_id": rid}
             for rid in ("big", "busy", "small")]
    params = {"conversation_id": "conv", "workers": nodes, "image": image,
              "resources": {"cpu_millis": 1000, "memory_bytes": 256 * 1024 ** 2,
                            "pids_limit": 32, "gpu_count": 0}}
    rpc = Mock()
    rpc.dispatch.side_effect = [{"binding": None}, {"binding": {"state": "RUNNING"}}]

    def report(rid, action, config):
        assert action == "capacity"
        return {"available": rid != "busy", "policy": {
            "image": image, "cpu_millis": 2000, "memory_bytes": 512 * 1024 ** 2,
            "pids_limit": 64, "gpu_count": 2 if rid == "big" else 0,
        }}

    with patch("kitt.remote.runtime_pool.RemoteRuntimeDriver._request", side_effect=report):
        result = provision_best_worker(rpc, params)
        assert result["worker"]["runtime_id"] == "small"
        assert rpc.dispatch.call_count == 2
        assert rpc.dispatch.call_args.args[1]["config"]["runtime_id"] == "small"
        rpc.reset_mock()
        rpc.dispatch.side_effect = [{"binding": None}, RuntimeError("uncertain provision")]
        with pytest.raises(RuntimeError, match="uncertain"):
            provision_best_worker(rpc, params)
        assert rpc.dispatch.call_count == 2

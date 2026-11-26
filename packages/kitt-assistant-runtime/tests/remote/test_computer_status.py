from unittest.mock import Mock

from kitt.remote.gateway import DaemonGateway


def test_computer_status_is_scoped_and_projects_only_public_fields(tmp_path):
    gateway = DaemonGateway(str(tmp_path))
    gateway.request = Mock(return_value={"computer": {"binding": {
        "backend": "REMOTE", "state": "RUNNING", "runtime_id": "lab",
        "updated_at": 10, "workspace_ref": "/private", "config": {"token": "secret"},
    }}})
    assert gateway.computer_status("session") == {
        "bound": True, "backend": "REMOTE", "state": "RUNNING",
        "runtime_id": "lab", "updated_at": 10,
    }
    gateway.request.assert_called_once_with("computer.status", {"session_id": "session"})
    gateway.request.return_value = {"computer": {"binding": None}}
    assert gateway.computer_status("session") == {"bound": False}

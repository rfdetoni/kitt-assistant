from __future__ import annotations

import errno
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import MagicMock, patch

from kitt.daemon import process


def _transport_without_pid() -> MagicMock:
    transport = MagicMock()
    transport.read_pid.return_value = None
    return transport


def test_resolve_python_executable_rejects_stale_interpreter() -> None:
    with patch.object(process.sys, "executable", "/definitely/missing/kitt-python"):
        assert process._resolve_python_executable() is None


def test_start_daemon_detached_contains_popen_enoent_before_spawn() -> None:
    with TemporaryDirectory() as temp:
        transport = _transport_without_pid()
        missing = FileNotFoundError(errno.ENOENT, "No such file or directory")
        with (
            patch.object(process, "IPCTransport", return_value=transport),
            patch.object(process, "_resolve_python_executable", return_value=Path(sys.executable)),
            patch.object(process, "_resolve_spawn_cwd", return_value=Path(temp)),
            patch.object(process.subprocess, "Popen", side_effect=missing),
        ):
            result = process.start_daemon_detached(temp)

    assert result["status"] == "error"
    assert result["bootstrap_failed"] is True
    assert result["spawned"] is False
    assert result["errno"] == errno.ENOENT
    assert "before process creation" in result["error"]
    transport.cleanup.assert_not_called()


def test_start_daemon_detached_marks_post_spawn_timeout_as_spawned() -> None:
    with TemporaryDirectory() as temp:
        transport = _transport_without_pid()
        proc = MagicMock(pid=4321)
        proc.poll.return_value = 1
        with (
            patch.object(process, "IPCTransport", return_value=transport),
            patch.object(process, "_resolve_python_executable", return_value=Path(sys.executable)),
            patch.object(process, "_resolve_spawn_cwd", return_value=Path(temp)),
            patch.object(process.subprocess, "Popen", return_value=proc),
            patch.object(process, "_pid_alive", return_value=False),
            patch.object(process, "_terminate_spawned"),
        ):
            result = process.start_daemon_detached(temp, timeout_seconds=0)

    assert result["status"] == "error"
    assert result["spawned"] is True
    assert "bootstrap_failed" not in result
    transport.cleanup.assert_called_once()

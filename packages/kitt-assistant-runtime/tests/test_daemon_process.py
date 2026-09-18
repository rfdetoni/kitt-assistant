from __future__ import annotations

import errno
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import AsyncMock, MagicMock, patch

from kitt.daemon import process


def _transport_without_pid() -> MagicMock:
    transport = MagicMock()
    transport.read_pid.return_value = None
    return transport


class DaemonProcessBootstrapTests(unittest.TestCase):
    def test_spawn_cwd_prefers_upgrade_stable_workspace(self) -> None:
        with TemporaryDirectory() as temp:
            self.assertEqual(process._resolve_spawn_cwd(temp), Path(temp).resolve())

    def test_resolve_python_executable_rejects_stale_interpreter(self) -> None:
        with patch.object(process.sys, "executable", "/definitely/missing/kitt-python"):
            self.assertIsNone(process._resolve_python_executable())

    def test_start_daemon_detached_contains_popen_enoent_before_spawn(self) -> None:
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

        self.assertEqual(result["status"], "error")
        self.assertIs(result["bootstrap_failed"], True)
        self.assertIs(result["spawned"], False)
        self.assertEqual(result["errno"], errno.ENOENT)
        self.assertIn("before process creation", result["error"])
        transport.cleanup.assert_not_called()

    def test_start_daemon_detached_replaces_authenticated_incompatible_daemon(self) -> None:
        with TemporaryDirectory() as temp:
            transport = MagicMock()
            transport.read_pid.side_effect = [1234, 4321]
            proc = MagicMock(pid=4321)
            proc.poll.return_value = None

            with (
                patch.object(process, "IPCTransport", return_value=transport),
                patch.object(process, "_pid_alive", side_effect=[True, False, False]),
                patch.object(
                    process,
                    "_probe_daemon",
                    new=AsyncMock(side_effect=[False, True]),
                ),
                patch.object(
                    process,
                    "_stop_daemon_via_ipc",
                    new=AsyncMock(return_value={"status": "ok"}),
                ) as stop,
                patch.object(
                    process,
                    "_resolve_python_executable",
                    return_value=Path(sys.executable),
                ),
                patch.object(process, "_resolve_spawn_cwd", return_value=Path(temp)),
                patch.object(process.subprocess, "Popen", return_value=proc),
            ):
                result = process.start_daemon_detached(temp)

        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["pid"], 4321)
        stop.assert_awaited_once_with(temp)
        transport.cleanup.assert_called_once()

    def test_start_daemon_detached_marks_post_spawn_timeout_as_spawned(self) -> None:
        with TemporaryDirectory() as temp:
            transport = _transport_without_pid()
            proc = MagicMock(pid=4321)
            proc.poll.return_value = 1
            with (
                patch.object(process, "IPCTransport", return_value=transport),
                patch.object(process, "_resolve_python_executable", return_value=Path(sys.executable)),
                patch.object(process, "_resolve_spawn_cwd", return_value=Path(temp)),
                patch.object(process.subprocess, "Popen", return_value=proc),
                patch.object(process, "_probe_daemon", new=AsyncMock(return_value=False)),
                patch.object(process, "_pid_alive", return_value=False),
                patch.object(process, "_terminate_spawned"),
            ):
                result = process.start_daemon_detached(temp, timeout_seconds=0)

        self.assertEqual(result["status"], "error")
        self.assertIs(result["spawned"], True)
        self.assertNotIn("bootstrap_failed", result)
        transport.cleanup.assert_called_once()


if __name__ == "__main__":
    unittest.main()

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from kitt.daemon.client import DaemonClient
from kitt.daemon.server import DaemonServer


class DaemonLoggingTests(unittest.IsolatedAsyncioTestCase):
    async def test_live_daemon_accepts_level_2_logging_configuration(self) -> None:
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
            root = Path(temp).resolve()
            server = DaemonServer(workspace_root=str(root))
            await server.start()
            client = DaemonClient(workspace_root=str(root), token=server.token)
            try:
                self.assertTrue(await client.connect())

                log_path = root / ".kitt" / "logs" / "agent-cli.log"
                response = await client.send_request(
                    "runtime.set_logging",
                    {
                        "workspace": str(root),
                        "level": 2,
                        "path": str(log_path),
                    },
                )
                self.assertEqual(response.get("status"), "ok")
                self.assertEqual(response.get("level"), 2)
                self.assertEqual(
                    Path(response.get("path")).resolve(),
                    log_path.resolve(),
                )

                rendered = log_path.read_text(encoding="utf-8")
                self.assertTrue(rendered.strip())
                payload = json.loads(rendered.splitlines()[-1])
                self.assertEqual(
                    payload.get("extra_data", {}).get("event"),
                    "daemon.logging.configured",
                )

                disabled = await client.send_request(
                    "runtime.set_logging",
                    {
                        "workspace": str(root),
                        "level": 0,
                        "path": None,
                    },
                )
                self.assertEqual(disabled.get("status"), "ok")
                self.assertEqual(disabled.get("level"), 0)
            finally:
                await client.close()
                await server.stop()


if __name__ == "__main__":
    unittest.main()

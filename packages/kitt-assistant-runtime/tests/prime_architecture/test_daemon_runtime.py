import asyncio
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from kitt import DAEMON_PROTOCOL_VERSION
from kitt.daemon.client import DaemonClient
from kitt.daemon.protocol import DaemonEvent
from kitt.daemon.server import DaemonServer
from kitt.history.database import HistoryDatabase


class FakeClient:
    def __init__(self, text: str):
        self.text = text

    def chat(self, *args, **kwargs):
        return self.text

    def chat_stream(self, *args, **kwargs):
        for chunk in (self.text[:4], self.text[4:]):
            yield chunk


class TestDaemonRuntime(unittest.IsolatedAsyncioTestCase):
    """End-to-end integration tests for persistent DaemonServer and multiplexed DaemonClient."""

    async def asyncSetUp(self):
        self.temp_dir = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.root = Path(self.temp_dir.name).resolve()
        (self.root / "src").mkdir(parents=True, exist_ok=True)
        (self.root / "src" / "main.py").write_text("def hello(): pass\n", encoding="utf-8")

        self.server = DaemonServer(
            workspace_root=str(self.root),
            context_client=FakeClient('{"intent":"ASK","confidence":1.0}'),
            execution_client=FakeClient("Hello from KITT Daemon"),
        )
        await self.server.start()

        # Create session under real runtime workspace identity
        rt = await self.server._get_or_create_runtime()
        conv = rt.history.new_conversation("Session One")
        self.session_id = conv["id"]

    async def asyncTearDown(self):
        await self.server.stop()
        self.temp_dir.cleanup()

    async def test_00_event_and_handshake_share_daemon_protocol_version(self):
        event = DaemonEvent(
            sequence_id=1,
            session_id=self.session_id,
            event_type="probe",
            payload={},
            created_at=time.time(),
        )
        self.assertEqual(event.protocol_version, DAEMON_PROTOCOL_VERSION)

        client = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        self.assertTrue(await client.connect())
        ping = await client.send_request("ping")
        self.assertEqual(ping.get("daemon_protocol_version"), DAEMON_PROTOCOL_VERSION)
        self.assertEqual(ping.get("lifecycle_state"), "ready")
        self.assertIs(ping.get("ready"), True)
        await client.close()

    async def test_00_readiness_waits_for_required_runtime_start(self):
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
            root = Path(temp).resolve()
            release_runtime = asyncio.Event()
            server = DaemonServer(workspace_root=str(root))
            client = None
            start_task = None

            async def delayed_runtime(*_args, **_kwargs):
                await release_runtime.wait()
                return object()

            try:
                with patch.object(server, "_get_or_create_runtime", side_effect=delayed_runtime):
                    start_task = asyncio.create_task(server.start())
                    for _ in range(100):
                        if server.token and server.transport.endpoint_file.exists():
                            break
                        await asyncio.sleep(0.01)
                    else:
                        self.fail("daemon listener did not become reachable during startup")

                    client = DaemonClient(workspace_root=str(root), token=server.token)
                    self.assertTrue(await client.connect(require_compatible=False))
                    ping = await client.send_request("ping")
                    self.assertEqual(ping.get("lifecycle_state"), "starting")
                    self.assertIs(ping.get("ready"), False)
                    self.assertFalse(await client.is_running())

                    release_runtime.set()
                    await asyncio.wait_for(start_task, timeout=2.0)
                    ping = await client.send_request("ping")
                    self.assertEqual(ping.get("lifecycle_state"), "ready")
                    self.assertIs(ping.get("ready"), True)
            finally:
                release_runtime.set()
                if start_task is not None:
                    await asyncio.gather(start_task, return_exceptions=True)
                if client is not None:
                    await client.close()
                await server.stop()

    async def test_00a_stop_closes_an_idle_connected_client_and_releases_instance(self):
        client = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        self.assertTrue(await client.connect())
        try:
            await asyncio.wait_for(self.server.stop(), timeout=2.0)
            self.assertIsNone(self.server._instance_lock_fd)
            self.assertIsNone(self.server._server)
            self.assertFalse(self.server.transport.endpoint_file.exists())
        finally:
            await client.close()

    async def test_00b_surface_capability_and_semantic_action_round_trip(self):
        rt = await self.server._get_or_create_runtime()
        snapshot = rt.surface_service.publish(
            {
                "id": "test-surface",
                "catalog_id": "kitt.core.v1",
                "root": "root",
                "components": [
                    {
                        "id": "root",
                        "component": "Button",
                        "props": {"label": "Apply", "action": "apply"},
                        "children": [],
                    }
                ],
            }
        )
        self.assertEqual(snapshot["revision"], 1)

        client = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        self.assertTrue(await client.connect())
        capabilities = await client.send_request("surface.capabilities")
        self.assertEqual(capabilities.get("status"), "ok")
        self.assertIn("Button", capabilities["capabilities"]["components"])

        result = await client.send_request(
            "surface.action",
            {
                "session_id": self.session_id,
                "surface_id": "test-surface",
                "component_id": "root",
                "surface_action": "apply",
                "context": {"source": "test"},
            },
        )
        self.assertEqual(result.get("status"), "ok")
        self.assertEqual(result["surface_action"]["action"], "apply")
        await client.close()

    async def test_01_daemon_real_turn_executes_and_emits_events(self):
        """Verify DaemonServer executes turn and emits real stream of turn events."""
        client = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        connected = await client.connect()
        self.assertTrue(connected)

        received_events = []

        def _on_event(evt):
            received_events.append(evt)

        # Attach to session
        attach_res = await client.attach(self.session_id, on_event=_on_event)
        self.assertEqual(attach_res.get("status"), "ok")

        # Send input
        submitted = await client.send_input(self.session_id, "Hello KITT")
        self.assertTrue(submitted)

        # Wait for events to be processed and emitted
        for _ in range(30):
            if any(e.event_type in ("TurnCompleted", "TurnFailed") for e in received_events):
                break
            await asyncio.sleep(0.1)

        event_types = [e.event_type for e in received_events]
        self.assertIn("TurnStarted", event_types)
        await client.close()

    async def test_02_create_session_and_session_isolation(self):
        """Verify explicit session creation and isolation between multiple sessions."""
        client = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        await client.connect()

        # Create session explicitly
        create_res = await client.send_request("create_session", {"title": "Isolated Session"})
        self.assertEqual(create_res.get("status"), "ok")
        new_session_id = create_res.get("session_id")
        self.assertIsNotNone(new_session_id)

        # Attach and verify isolation
        events = []
        await client.attach(new_session_id, on_event=lambda e: events.append(e))
        await client.send_input(new_session_id, "Test Isolation")

        for _ in range(30):
            if any(e.event_type in ("TurnCompleted", "TurnFailed") for e in events):
                break
            await asyncio.sleep(0.1)

        # All received events must belong to new_session_id
        self.assertGreater(len(events), 0)
        for e in events:
            self.assertEqual(e.session_id, new_session_id)

        await client.close()

    async def test_03_incremental_replay_on_reconnect(self):
        """Verify client reconnecting with last_sequence retrieves only new events."""
        client1 = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        await client1.connect()
        terminal = asyncio.Event()

        def on_event(event):
            if event.event_type in {"TurnCompleted", "TurnFailed", "TurnCancelled", "TurnBlocked"}:
                terminal.set()

        await client1.attach(self.session_id, on_event=on_event)
        await client1.send_input(self.session_id, "Turn for replay")

        # Replaying an active turn may legitimately find a newer event between
        # attachments. Await its terminal event instead of assuming a duration.
        await asyncio.wait_for(terminal.wait(), timeout=10.0)
        await client1.close()

        # Connect client 2 and verify replay
        client2 = DaemonClient(workspace_root=str(self.root), token=self.server.token)
        await client2.connect()
        replayed_res = await client2.attach(self.session_id, last_sequence=0)
        replayed = replayed_res.get("events", [])
        self.assertGreater(len(replayed), 0)

        highest_seq = max(e.sequence_id for e in replayed)
        # Re-attach asking for events after highest_seq
        empty_res = await client2.attach(self.session_id, last_sequence=highest_seq)
        empty_replay = empty_res.get("events", [])
        self.assertEqual(len(empty_replay), 0)

        await client2.close()


    async def test_04_pending_approval_survives_arbitrary_daemon_wait(self):
        """Persisted PENDING approvals remain discoverable regardless of elapsed wall time."""
        rt = await self.server._get_or_create_runtime()
        turn_id = "turn-no-timeout"
        approval_id = "approval-no-timeout"
        action_hash = "hash-no-timeout"
        now = time.time()

        with rt.database.get_connection() as conn:
            ordinal = conn.execute(
                "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM turns WHERE conversation_id = ?",
                (self.session_id,),
            ).fetchone()[0]
            conn.execute(
                """INSERT INTO turns
                   (id, conversation_id, ordinal, state, mode, started_at)
                   VALUES (?, ?, ?, 'RUNNING', 'auto', ?)""",
                (turn_id, self.session_id, ordinal, now),
            )

        req = rt.approval.register_request(
            turn_id,
            self.session_id,
            rt.workspace_id,
            action_hash,
            approval_id,
            tool_name="process.run",
            summary="wait for user",
        )
        self.assertEqual(req.expires_at, 0.0)

        with rt.database.get_connection() as conn:
            conn.execute(
                """INSERT INTO pending_actions
                   (id, approval_request_id, turn_id, conversation_id, workspace_id,
                    tool_name, normalized_args_json, action_hash, source_response_sha256,
                    affected_paths_json, before_hashes_json, created_at, expires_at,
                    state, security_context_json)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)""",
                (
                    "pending-no-timeout",
                    approval_id,
                    turn_id,
                    self.session_id,
                    rt.workspace_id,
                    "process.run",
                    '{"argv":["echo","ok"]}',
                    action_hash,
                    "source-hash",
                    "[]",
                    "{}",
                    now,
                    0.0,
                    "{}",
                ),
            )

        with patch("kitt.daemon.server.time.time", return_value=now + 7 * 24 * 60 * 60):
            approvals = self.server._approval_payloads(
                rt,
                session_id=self.session_id,
                approval_id=approval_id,
                limit=1,
            )

        self.assertEqual(len(approvals), 1)
        self.assertEqual(approvals[0]["approval_id"], approval_id)
        self.assertEqual(approvals[0]["expires_at"], 0.0)

    async def test_05_direct_pending_is_never_age_or_capacity_evicted(self):
        """Existing direct approvals stay active; capacity only blocks creating new ones."""
        rt = await self.server._get_or_create_runtime()
        now = time.time()

        for idx in range(65):
            approval_id = f"direct-{idx}"
            action_hash = f"direct-hash-{idx}"
            rt.approval.register_request(
                f"direct-turn-{idx}",
                self.session_id,
                rt.workspace_id,
                action_hash,
                approval_id,
                tool_name="run_command",
                summary="direct approval",
            )
            self.server._direct_pending[approval_id] = {
                "approval_id": approval_id,
                "turn_id": f"direct-turn-{idx}",
                "conversation_id": self.session_id,
                "workspace_id": rt.workspace_id,
                "tool_name": "run_command",
                "args": {"argv": ["echo", str(idx)]},
                "action_hash": action_hash,
                "security_context": None,
                "created_at": now - idx,
                "expires_at": 0.0,
            }

        with patch("kitt.daemon.server.time.time", return_value=now + 30 * 24 * 60 * 60):
            self.server._prune_direct_pending(rt)

        self.assertEqual(len(self.server._direct_pending), 65)
        self.assertEqual(len(rt.approval.list_pending(rt.workspace_id)), 65)

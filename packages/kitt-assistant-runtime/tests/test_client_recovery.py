import asyncio
from types import SimpleNamespace
from unittest.mock import patch

import pytest
from kitt import DAEMON_PROTOCOL_VERSION
from kitt.daemon.client import DaemonClient, _compatible_ping


def test_compatible_ping_rejects_stale_daemon_that_only_reports_new_disk_agent_version():
    base = {
        "status": "ok",
        "ready": True,
        "lifecycle_state": "ready",
        "agent_version": "0.84.4",
        "daemon_protocol_version": DAEMON_PROTOCOL_VERSION,
    }
    with (
        patch(
            "kitt.daemon.client._AGENT_STARTUP_VERSION", "0.84.4"
        ),
        __import__("unittest.mock").mock.patch(
            "kitt.daemon.client._RUNTIME_STARTUP_VERSION", "0.2.40"
        ),
    ):
        assert not _compatible_ping(base)
        assert not _compatible_ping(
            {**base, "assistant_runtime_version": "0.2.39"}
        )
        assert _compatible_ping(
            {**base, "assistant_runtime_version": "0.2.40"}
        )


@pytest.mark.asyncio
async def test_eof_fails_pending_requests_without_cancelling_callers(tmp_path):
    client = DaemonClient(tmp_path)
    client.reader = asyncio.StreamReader()
    client.reader.feed_eof()
    closed = []
    client.writer = SimpleNamespace(is_closing=lambda: False, close=lambda: closed.append(True))
    client._connected = True
    future = asyncio.get_running_loop().create_future()
    client._pending_requests["request"] = future
    await client._reader_loop()
    assert not client.connected
    assert closed == [True]
    with pytest.raises(ConnectionError, match="outcome may be unknown"):
        await future


@pytest.mark.asyncio
async def test_resync_stops_live_delivery_before_cursor_can_skip_history(tmp_path):
    from kitt.daemon.protocol import encode_message
    client = DaemonClient(tmp_path)
    client.reader = asyncio.StreamReader()
    client.reader.feed_data(encode_message({"type": "RESYNC_REQUIRED"}))
    client.reader.feed_data(encode_message({"type": "EVENT", "event": {"sequence_id": 99}}))
    client.reader.feed_eof()
    client.writer = SimpleNamespace(is_closing=lambda: False, close=lambda: None)
    client._connected = True
    delivered = []
    client._event_callback = delivered.append
    await client._reader_loop()
    assert client.resync_required
    assert not client.connected
    assert delivered == []

import asyncio
from types import SimpleNamespace
import pytest
from kitt.daemon.client import DaemonClient


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

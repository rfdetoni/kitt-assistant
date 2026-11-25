import asyncio
from types import SimpleNamespace
from unittest.mock import Mock

import pytest

from kitt.daemon.server import DaemonServer


def test_computer_ipc_requires_owned_session_and_overrides_nested_scope():
    async def check():
        server = DaemonServer.__new__(DaemonServer)
        server._session_locks = {}
        server._active_turns = {}
        server._blocking_executor = None
        repo = Mock()
        repo.get_conversation.side_effect = lambda sid: {
            "id": sid, "workspace_id": "workspace" if sid == "owned" else "foreign",
        }
        dispatch = Mock(return_value={"binding": None})
        rt = SimpleNamespace(workspace_id="workspace", history=SimpleNamespace(repo=repo),
                             rpc=SimpleNamespace(dispatch=dispatch))
        result = await server._computer_action(rt, "computer.status", {
            "session_id": "owned", "params": {"conversation_id": "foreign"},
        })
        assert result == {"binding": None}
        dispatch.assert_called_once_with("runtime.status", {"conversation_id": "owned"})
        with pytest.raises(ValueError, match="Cross-workspace"):
            await server._computer_action(rt, "computer.status", {"session_id": "foreign"})
        with pytest.raises(ValueError, match="Unsupported"):
            await server._computer_action(rt, "computer.execute", {"session_id": "owned"})
        server._active_turns["owned"] = "turn"
        with pytest.raises(RuntimeError, match="active turn"):
            await server._computer_action(rt, "computer.terminate", {"session_id": "owned"})
        assert dispatch.call_count == 1

    asyncio.run(check())

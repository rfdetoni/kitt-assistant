# Assistant runtime 0.3.11 — Agent 0.86.14 lock alignment

- Update `packages/kitt-assistant-runtime/uv.lock` to pin the Agent
  commit `0f3b457f3d61ef7cac1080ba47d0ab420f75a849` (v0.86.14).
- Bump the Python Assistant runtime to 0.3.11 and raise the minimum
  Agent requirement to 0.86.14.
- No native daemon/HUD, Docker/Podman, or Protocol changes.
- Addresses the previously stale Python lock (Agent 0.86.12).

The Agent's new single-flight guards prevent one TUI bridge from
re-attaching or submitting the same prompt in concurrent callbacks;
separate frontend instances remain independently authorized daemon clients.

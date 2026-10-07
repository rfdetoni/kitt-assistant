# K.I.T.T. Assistant 0.1.26 — Agent CLI 0.84.3 alignment

Assistant 0.1.26 / Python runtime 0.2.39 aligns its immutable Agent dependency with Agent CLI 0.84.3.

Agent CLI 0.84.3 refreshes a stale resident Reverse Proxy control plane before managed start/restart and verifies the returned Proxy log path is in the Agent log directory. The Assistant runtime has no behavior change of its own in this release.

- Agent CLI lock: `87d3954adbeb37a21eae367cd30e3499c3fdb162`
- Supported Agent range remains `>=0.81.0,<0.85`.
- Runtime/lifecycle CI uses the same immutable Agent revision.
- No daemon IPC, Protocol, Memory, HUD or native lifecycle contract changes.

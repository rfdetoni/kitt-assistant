# K.I.T.T. Assistant 0.1.29 / runtime 0.2.42 — Agent CLI 0.84.7 alignment

Python runtime 0.2.42 resolves Agent CLI 0.84.7 at `f9c07452dd211359e754c2b9a1eb54e3854c41a1`.

Agent 0.84.7 fixes the visible first-prompt bootstrap path in the full-screen TUI. The UI transitions from home to session before `TurnEventBridge.start(...)` finishes daemon connection/attachment and automatic-contract startup, showing the submitted prompt, `STARTING` state and core task immediately.

This release only aligns the daemon/runtime consumer lock. Assistant daemon startup identity, IPC, Protocol and Memory contracts are unchanged.

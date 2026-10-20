# Assistant 0.1.61 / Runtime 0.4.16 — autonomous defaults

- Agent CLI 0.88.22 is pinned to merge `53a8a22a0c83915852ad21893da265c3261ad053` and validated through the split Python/CI workflow.
- New workspace sessions begin in `autonomous` unless a prior user setting exists. The daemon continues to own the effective policy; Autonomy menu `S` (ASK) and `D` (DENY) overrides persist.
- Agent-triggered host process execution requires a strong OS sandbox; when no functional bubblewrap >=0.12 is available, it returns a sandbox failure rather than escaping the workspace. VM/guest actions remain authenticated, scoped, and separate from the host runner.
- Rust packages 0.1.61, Python runtime 0.4.16, and all local lock refs aligned. Existing hard restrictions (paths, credentials, network, KITT control plane) stay intact.

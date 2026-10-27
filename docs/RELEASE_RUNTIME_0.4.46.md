# Assistant Runtime 0.4.46 — Isolated Autopilot worktrees

- Agent CLI 0.89.4 required, pinned to `eaa4f198a14c24356c4d0578e9d41f54466c533b` in uv lock and both CI checkouts.
- Agent task sessions may start a clean Git worktree, run sandboxed edits and validation there, and optionally publish an unchanged verified result through the preauthorized GitHub publisher.
- No automatic local promotion is claimed. Docker or trusted Compose broker is mandatory for an isolated task; the host checkout remains untouched.
- Sibling Rust, Protocol and HUD pins are unchanged.

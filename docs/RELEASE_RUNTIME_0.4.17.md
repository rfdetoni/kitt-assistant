# KITT Assistant Runtime 0.4.17

The Python daemon remains the authority for persisted autonomy and approval decisions. Agent CLI 0.88.23 updates the UI's approval menu so **A** selects autonomous mode for the active workspace; **w** remembers only the selected tool. The existing daemon RPC `runtime.set_autonomy` is used, so no additional protocol version is required.

The Agent CLI source pin in `packages/kitt-assistant-runtime/uv.lock` and both checkout pins in `.github/workflows/ci.yml` are aligned to `fa1ab23e9bbdb497f945517813d9aa9e06649f66`.

The Rust Assistant 0.1.61 workspace and `Cargo.lock` are unchanged. Automatic host subprocesses still require the configured strong sandbox; user ASK and DENY persist by workspace. No VM identity or network capability gate was widened.

Validation: Assistant CI Python runtime, lifecycle portability, Rust locked checks and daemon regression suite.

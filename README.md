# K.I.T.T. Assistant

Low-footprint personal assistant designed to stay resident from OS startup while keeping the HUD, Python workers and LLM workloads ephemeral.

## Runtime

- `kittd`: always-on Rust daemon. Loopback-only IPC, token auth, memory owner, model provider and HUD lifecycle.
- `kittctl`: tiny CLI used by hotkeys/launchers/scripts.
- `kitt-hud`: Tauri 2 + TypeScript floating HUD. Spawned only when visual output is needed and exits after its TTL.
- `kitt-memory`: shared SQLite memory engine.
- OpenAI-compatible provider: Ollama by default; any compatible base URL can be configured.

## Resource strategy

At idle, only `kittd` is required. No WebView, Python worker or model is launched by the Assistant itself until a request needs it.

## Quick start

1. Build `kitt-memory` and make it available beside this workspace (or replace the path dependency with a tagged Git dependency).
2. `cargo build --release --workspace`
3. Put `kittd`, `kittctl` and the built `kitt-hud` beside one another.
4. Set `KITT_MODEL` or edit the generated config.
5. Start `kittd`.
6. `kittctl ask "Olá, KITT"`
7. `kittctl image /absolute/path/to/image.png`

The daemon writes its auth token with owner-only permissions on Unix. API keys are referenced by environment-variable name and are never stored in config.

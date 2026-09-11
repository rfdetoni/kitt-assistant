# K.I.T.T. Assistant

<p align="center">
  <strong>Resident local assistant, control center and voice/HUD runtime for K.I.T.T.</strong><br>
  Rust daemon · authenticated loopback IPC · web Control Center · voice · ephemeral HUD · Python runtime
</p>

<p align="center">
  <a href="https://github.com/rfdetoni/kitt-assistant/blob/main/LICENSE"><img alt="License MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-resident%20daemon-000000?logo=rust&logoColor=white">
  <img alt="Loopback" src="https://img.shields.io/badge/network-loopback--first-6f42c1">
  <img alt="Control Center" src="https://img.shields.io/badge/UI-Control%20Center-3178C6">
</p>

K.I.T.T. Assistant is the long-lived local runtime of the K.I.T.T. ecosystem. It provides a low-overhead native daemon, authenticated IPC, the K.I.T.T. Control Center web application, voice orchestration, ephemeral desktop HUD output and the separately packaged Python runtime used by K.I.T.T. Agent for daemon/remote integration.

The implementation follows Clean Architecture boundaries:

```text
domain <- application <- infrastructure <- apps
```

---

## What’s included

- `kittd`: low-footprint Rust resident daemon.
- `kittctl`: authenticated CLI client and service manager.
- K.I.T.T. Control Center SPA served locally by the daemon.
- `kitt-hud`: transparent ephemeral desktop overlay built with Tauri/TypeScript/Vite.
- Voice activation, microphone recovery, STT routing and system TTS integration.
- Fast/Heavy model routing.
- Shared memory integration with privacy-aware egress.
- Native OS service management for Linux, macOS and Windows.
- `packages/kitt-assistant-runtime`: Python `kitt.daemon` and `kitt.remote` capabilities consumed by the Agent.

---

## Quick links

- **K.I.T.T. ecosystem:** https://github.com/rfdetoni/kitt
- **Agent CLI:** https://github.com/rfdetoni/kitt-agent-cli
- **Memory:** https://github.com/rfdetoni/kitt-memory
- **Protocol:** https://github.com/rfdetoni/kitt-protocol
- **AI workers / STT:** https://github.com/rfdetoni/kitt-ai-workers

---

## Runtime architecture

```text
                          ┌───────────────────────┐
                          │        kittd          │
                          │   resident Rust core  │
                          └──────────┬────────────┘
                                     │
             ┌───────────────────────┼───────────────────────┐
             │                       │                       │
             ▼                       ▼                       ▼
      Authenticated IPC       Control Center Web        Voice pipeline
      127.0.0.1:41827         127.0.0.1:41828          STT / TTS / wake
             │                       │                       │
             ├──────────────┬────────┴──────────────┬────────┘
             │              │                       │
             ▼              ▼                       ▼
          kittctl        KITT HUD              Memory / models

KITT Agent
    │
    ▼
kitt-assistant-runtime
    ├── kitt.daemon
    └── kitt.remote
```

The Rust daemon and Python runtime deliberately have different ownership: `kittd` owns the native long-lived service; `kitt-assistant-runtime` owns Agent-facing Python orchestration that benefits from sharing the `kitt.*` namespace.

---

## Requirements & build

Build the Rust workspace:

```bash
cargo build --release --workspace
```

Validate the Python resident runtime during development:

```bash
python -m pip install -e ../kitt-agent-cli
python -m pip install --no-deps -e packages/kitt-assistant-runtime
python -m unittest discover -s packages/kitt-assistant-runtime/tests -v
```

Build the optional HUD:

```bash
cd apps/kitt-hud
npm install
npm run build
cd ../..
```

For normal users, the root [`rfdetoni/kitt`](https://github.com/rfdetoni/kitt) installer composes these pieces automatically.

---

## K.I.T.T. Control Center

`kittd` embeds and serves the Control Center locally:

```text
http://127.0.0.1:41828/
```

The SPA provides global settings search, dynamic catalog generation, health checks, a diff viewer before writes and revisioned atomic configuration overlays.

Web security includes loopback-only access, CSRF protection for mutation methods, strict CSP headers, `X-Frame-Options: DENY` and `no-store` caching.

---

## Service management

`kittctl` manages `kittd` through native operating-system service mechanisms: systemd on Linux, LaunchAgent on macOS and Scheduled Tasks on Windows.

```bash
./target/release/kittctl service install
./target/release/kittctl service start
./target/release/kittctl service status
./target/release/kittctl service restart
./target/release/kittctl service stop
./target/release/kittctl service uninstall
```

---

## CLI usage

```bash
# Health
./target/release/kittctl ping

# Ask the assistant with automatic Fast/Heavy routing
./target/release/kittctl ask "Olá KITT"

# Explicit heavy route
./target/release/kittctl ask --route heavy \
  "Escreva um algoritmo de ordenação em Rust"

# Persistent memory
./target/release/kittctl remember \
  "Prefiro respostas concisas em português"

# Show an image in the ephemeral HUD
./target/release/kittctl image /path/to/screenshot.png
```

---

## Voice & audio

Supported activation modes include:

- `auto`: prefers a local wakeword model and falls back gracefully to transcript-prefix matching;
- `wakeword`: uses a local Rustpotter `.rpw` model;
- `transcript_prefix`: recognizes prefixes such as `kitt`, `hey kitt` and `ei kitt`.

The microphone stream is recovered with exponential backoff after device failures. Audio utterance caches are cleaned automatically, and temporary system-TTS files use restrictive permissions where supported.

Heavy STT dependencies live in `kitt-ai-workers`; the Assistant can supervise the local `kitt-stt` process only when voice transcription requires it.

---

## Configuration

Configuration is loaded from:

```text
${XDG_CONFIG_HOME:-~/.config}/kitt/assistant/
```

Main files:

| File | Responsibility |
| --- | --- |
| `config.json` | daemon endpoints, model/API settings, privacy and HUD behavior |
| `models.json` | Fast/Heavy/STT routing profiles |
| `voice.json` | voice activation, locale, audio thresholds and TTS |

Control Center overrides are layered atomically from:

```text
${XDG_CONFIG_HOME:-~/.config}/kitt/control-center/overrides.json
```

---

## Security & privacy

K.I.T.T. Assistant is loopback-first and treats its resident state as privileged local infrastructure.

Key controls include:

- daemon listeners restricted to loopback addresses;
- authenticated IPC with a local secret token stored with restrictive permissions;
- secret/private memory stripped from remote-provider paths unless policy explicitly permits it;
- bounded stream readers to prevent unbounded allocation;
- CSRF/CSP/frame/cache protections in Control Center;
- temporary audio files created with restrictive permissions;
- on-demand heavy workers rather than permanently resident ML runtimes.

---

## Performance philosophy

The resident core stays in Rust so an always-on Assistant can remain inexpensive while idle. Heavy ML/STT work is delegated to on-demand workers, browser automation belongs to `kitt-reverse-proxy`, and coding-agent orchestration remains in `kitt-agent-cli`.

This separation keeps the always-running process focused on IPC, lifecycle, routing and local UX instead of accumulating every ecosystem dependency in one daemon.

---

## Testing & linting

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python -m unittest discover -s packages/kitt-assistant-runtime/tests -v
```

CI also composes the current Agent control plane with the runtime and verifies that `kitt.daemon` and `kitt.remote` resolve from `kitt-assistant-runtime`.

---

## Contributing

Preserve the resident-service budget and Clean Architecture boundaries. Features that require heavyweight dependencies should generally run out of process rather than expanding `kittd`’s steady-state footprint.

---

## K.I.T.T. ecosystem

| Repository | Responsibility |
| --- | --- |
| [`kitt`](https://github.com/rfdetoni/kitt) | installer and ecosystem composition |
| [`kitt-agent-cli`](https://github.com/rfdetoni/kitt-agent-cli) | autonomous agent control plane |
| [`kitt-reverse-proxy`](https://github.com/rfdetoni/kitt-reverse-proxy) | authorized provider gateway |
| [`kitt-protocol`](https://github.com/rfdetoni/kitt-protocol) | shared contracts and SDKs |
| [`kitt-memory`](https://github.com/rfdetoni/kitt-memory) | persistent memory engine |
| [`kitt-toolbox`](https://github.com/rfdetoni/kitt-toolbox) | native code/system data plane |
| [`kitt-ai-workers`](https://github.com/rfdetoni/kitt-ai-workers) | isolated AI/ML workers and evals |

---

## License

MIT. See [LICENSE](LICENSE).

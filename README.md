# K.I.T.T. Assistant

## Release 0.1.30 / runtime 0.2.43 — Agent CLI 0.84.8 progress alignment

Python runtime **0.2.43** locks Agent CLI **0.84.8** at `809151b40ef4600c8364c73583bd38717b1665cf`. Daemon-owned `mode=auto` turns now forward bounded inner Goal progress to the outer user turn, so planning, context, tool activity and edits remain visible while durable contract items execute.

Agent 0.84.8 also bounds interactive memory recall and makes temporary kitt-memoryd unavailability fail-soft for prompt enrichment. Assistant daemon, IPC, Protocol and native contracts are unchanged.

## Release 0.1.29 / runtime 0.2.42 — Agent CLI 0.84.7 TUI alignment

Python runtime **0.2.42** locks Agent CLI **0.84.7** at `f9c07452dd211359e754c2b9a1eb54e3854c41a1`. The TUI now leaves the home screen immediately when the first prompt is submitted, before daemon attachment and automatic-contract bootstrap finish, so daemon-owned `mode=auto` execution exposes `STARTING` progress instead of appearing frozen.

No Assistant daemon, IPC, Protocol or Memory contract changed.

## Release 0.1.28 / runtime 0.2.41 — Agent CLI 0.84.6 loop alignment

Python runtime **0.2.41** locks Agent CLI **0.84.6** at `0f14d68b5fbdd6848660f68707230d7d6c3db584`. This brings the complete Goals/TaskPlan ownership fix into daemon-owned `mode=auto` execution: GOAL items without a nested TaskPlan no longer receive TaskPlan host-verification context or its completion gate, leaving post-turn checks to `GoalStepVerifier`.

The stale-daemon startup-identity fix from 0.1.27/runtime 0.2.40 remains unchanged. Protocol, Memory and native daemon contracts are unchanged.

## Release 0.1.27 — stale daemon rejection and Agent CLI 0.84.4

Python runtime **0.2.40** freezes both the Agent and Assistant-runtime versions when the daemon process starts. The authenticated ping exposes that immutable startup identity, and clients reject daemons that do not report the exact current runtime identity. This fixes in-place upgrades where old daemon code stayed resident while `importlib.metadata` began reporting newly installed package versions.

An incompatible authenticated daemon is recycled by the existing daemon bootstrap path; no second lifecycle manager was added. The runtime lock and CI pin Agent CLI **0.84.4** at `fc985bc6d842d3c684ad1188ff2fc8a0c427e6a8`, including durable-loop hardening. See [release notes](docs/RELEASE_0.1.27.md).

## Release 0.1.26 — Agent CLI 0.84.3 managed Proxy logging alignment

Python runtime **0.2.39** locks Agent CLI **0.84.3**, including the stale Reverse Proxy control-plane refresh that makes managed Proxy logging honor the Agent log directory after upgrades. Native daemon, Protocol, Memory and IPC contracts are unchanged.

## Release 0.1.25 — automatic contract daemon routing

Python runtime **0.2.38** routes persisted daemon-owned `mode=auto` turns through Agent CLI **0.84.2**'s durable Plan → Execute → Validate → Retry contract wrapper. Explicit `ask`, `plan` and no-history turns remain direct TurnProcessor turns.

Goal-owned approvals are resolved inside the daemon and resume the same contract item instead of creating a disconnected foreground continuation. Cancellation of the outer daemon turn cancels the durable contract and its active inner turn. The Agent lock is pinned to the validated 0.84.2 main revision. See [release notes](docs/RELEASE_0.1.25.md).

## Release 0.1.24 — Agent CLI 0.84.1 verification refactor alignment

Python runtime **0.2.37** locks Agent CLI **0.84.1** at its validated main revision. The supported Agent range remains `>=0.81.0,<0.85`; native daemon, Protocol and Memory contracts are unchanged. See [release notes](docs/RELEASE_0.1.24.md).

## Release 0.1.23 — Agent CLI 0.84 contract compatibility

Python runtime **0.2.36** expands its Agent compatibility range to `>=0.81.0,<0.85` and locks Agent CLI **0.84.0** at its validated main revision. The native daemon/control-center behavior and shared Protocol/Memory contracts are unchanged; this release is a consumer-alignment bump only. See [release notes](docs/RELEASE_0.1.23.md).

## Release 0.1.22 — unified model selection endpoint trust

Python runtime 0.2.35 locks Agent CLI 0.83.19, covering both managed Reverse Proxy role binding and the general model picker. Runtime and lifecycle CI use the same immutable Agent revision. See [release notes](docs/RELEASE_0.1.22.md).

## Release 0.1.21 — managed proxy endpoint trust in the locked Agent

Python runtime 0.2.34 locks Agent CLI 0.83.18 at its validated main revision, so frozen Assistant environments include the fix for explicitly selected Reverse Proxy endpoints on newly allocated ports. Native workspace metadata is 0.1.21; Protocol and other dependencies retain their locked revisions. See [release notes](docs/RELEASE_0.1.21.md).

## Release 0.1.20 — consistent Protocol consumer locks

Align the HUD npm lock with Protocol 0.9.1 at the same immutable revision used by both Cargo locks and the Python runtime lock. A regression checks all four Protocol consumers together in the runtime CI suite. Native workspace version is 0.1.20; Python runtime remains 0.2.33 and HUD package metadata remains unchanged.

See [release notes](docs/RELEASE_0.1.20.md).

## Release 0.1.19 — execution boundary hardening

Publish Python runtime 0.2.33 with authoritative connection health, explicit ConnectionError on EOF (request outcome may be unknown), awaited reader cleanup and reconnect-ready resync state. Pair with Agent 0.83.16, Protocol 0.9.1 and Memory 0.9.2.

See [release notes](docs/RELEASE_0.1.19.md).

## Assistant 0.1.18 / Python runtime 0.2.32

Validated with Agent CLI **0.83.12**, Protocol **0.9.0** and native Memory **0.9.1**. Assistant 0.1.18/runtime 0.2.32 makes resident lifecycle fail-closed: daemon readiness is reported only after the required Agent runtime starts, native service commands propagate OS failures, and lifecycle/correlation state is observable without taking policy or memory authority from Agent/Memory. The companion keeps the reviewed Agent range `>=0.81.0,<0.84`; HUD package version metadata remains unchanged, while the native HUD Protocol dependency is aligned to Protocol 0.9.0.

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

The Python daemon treats human tool/command approvals as durable interaction state: `PENDING` approvals have no wall-clock timeout and are not evicted to make room for newer approval prompts. Capacity limits apply as backpressure to new requests; grant TTLs begin only after the user explicitly approves.

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

### Python runtime compatibility

`kitt-assistant-runtime 0.2.25` supports Agent CLI 0.76.x, 0.77.x and 0.78.x on Python 3.14+. K.I.T.T. sibling dependencies now follow their `main` branches; the root ecosystem CI resolves those moving refs once per run and validates the composed result.

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


### Shared memory v0.2 integration

The daemon is pinned to kitt-memory 0.2.1 and kitt-protocol 0.2.1. Protocol-v1 memory requests now preserve optional conversation `scope_key` and point-in-time `as_of` fields end-to-end. Workspace/global callers remain compatible with omitted fields, while conversation-scoped records are isolated by their explicit key.


## Assistant 0.1.5 / runtime 0.2.17 — consistency hardening

- The Python daemon protocol now imports one `DAEMON_PROTOCOL_VERSION` authority from the Agent package. Handshake metadata and `DaemonEvent.protocol_version` can no longer drift independently.
- HUD event fan-out snapshots subscriber sockets under the mutex and performs bounded socket writes after releasing it, so a slow HUD client does not stall all subscribers or subscription management.
- Rust memory dependencies historically used fixed revisions; current Assistant manifests follow `kitt-memory` `main`.
- Standalone Python-runtime CI uses an immutable known-compatible Agent 0.74.4 SHA rather than mutable `main`; the root ecosystem integration remains authoritative for the exact promoted component set.


## Assistant 0.1.6 / runtime 0.2.18 — current-interpreter alignment

- Python runtime metadata now requires Python 3.14+, matching the interpreter continuously validated by the ecosystem.
- Rust daemon, CLI, HUD and CI are pinned to KITT Protocol 0.2.1; wire protocol v1 remains unchanged.
- HUD package/Tauri metadata and Cargo/npm locks are aligned with the same 0.1.6/Protocol 0.2.1 snapshot.


## Assistant 0.1.7 / runtime 0.2.19 — semantic Surface renderer

Assistant now consumes KITT Protocol 0.3.0 and Memory 0.3.0. The Python daemon negotiates the host Surface component catalog, exposes a narrow semantic action route and persists `SurfaceAction` events. Remote Web renders validated Surface snapshots using a dedicated declarative renderer with bounded graph traversal and safe text DOM APIs; buttons send only semantic actions back to the daemon. Arbitrary model HTML/JavaScript and generic browser-side SafeRuntime execution remain unsupported.


## 0.1.8 — optional native voice build

The resident Assistant no longer requires Linux ALSA development packages merely to install the KITT ecosystem. The `kittd` voice-capture stack is now the Cargo feature `voice` (enabled by default for normal source builds), while the installer may compile `kittd` with `--no-default-features` when Linux native audio development dependencies are unavailable.

Disabling the build feature only removes resident microphone capture/wake-word processing. Core Assistant APIs, model routing, memory, Control Center, HUD transport, transcription requests and system TTS abstractions remain available.


## 0.1.9 / runtime 0.2.20 — external memory authority

The Assistant remains an optional UX/runtime component. Durable Agent memory is no longer hosted by Assistant/kittd: Agent CLI 0.76+ talks to standalone `kitt-memoryd`, owned by the kitt-memory repository. Assistant runtime compatibility is aligned to Agent 0.76.x without becoming a dependency of the minimal Agent + Reverse Proxy installation.


## 0.1.10 / runtime 0.2.21 — Agent 0.77 compatibility

The Python companion runtime now accepts `kitt-agent-cli>=0.76.0,<0.78`, covering the promoted Agent CLI 0.77.0 evidence-first execution release while preserving 0.76.x compatibility. Resident daemon ownership, approval durability and memory boundaries are unchanged.

Workspace/HUD version metadata and Cargo/npm locks are aligned to Assistant 0.1.10.


## 0.1.11 — voice-disabled build hotfix

- Fixed `kittd` compilation when the `voice` feature is disabled: Control Center voice-overlay types are now compiled only when `feature = "voice"` is active.
- CI now runs the same `cargo build --workspace --no-default-features --locked` path used by the ecosystem installer when Linux ALSA development headers are unavailable.
- Core Assistant APIs, model routing, memory integration, Control Center, HUD transport, transcription requests and system TTS remain available in the voice-disabled build.
- Assistant workspace and HUD metadata are aligned at `0.1.11`. The Python companion runtime remains `0.2.21` because its code and compatibility contract are unchanged.


## 0.1.12 / runtime 0.2.22 — Agent 0.78 compatibility

- Widen the Python companion runtime range to `kitt-agent-cli>=0.76.0,<0.79` after validating Agent CLI 0.78.0.
- Pin standalone Python-runtime CI to Agent CLI 0.78.0 revision `f51dbba8a0506e90366ddb4e95026dc6c6614699`.
- Align Rust workspace, HUD/Tauri metadata and lockfiles at Assistant 0.1.12.
- Preserve approval durability, protocol versions, memory ownership and daemon/remote API contracts.


## 0.1.13 / runtime 0.2.23 — Memory 0.5 / Agent 0.78.3 alignment

- Pin native Assistant memory dependencies to KITT Memory 0.5.0, keeping standalone `kitt-memoryd` as the durable Agent-memory authority.
- Validate the Python companion runtime against Agent CLI 0.78.3 revision `89a63da16368a59eac5eee185373bfbf89f516b1`.
- Align Rust workspace, HUD/Tauri metadata and lockfiles at Assistant 0.1.13.
- Preserve daemon/remote APIs, approval durability, Protocol 0.4.0 and the existing `kitt-agent-cli>=0.76.0,<0.79` compatibility range.


## 0.1.14 / runtime 0.2.24 — Agent 0.78.4 staged execution

Standalone CI now validates Agent CLI 0.78.4 revision `7d56faec43fa6f5c0e4b1f63c0c18e269a0eb9d8`. The Assistant runtime keeps its existing Agent 0.78 compatibility range while validating the reduced bootstrap/delta reverse-proxy prompt contract.


## Agent 0.78.6 compatibility

Assistant runtime 0.2.24 remains API-compatible with Agent CLI 0.78.6. CI now validates the composed Python namespace against revision `d6eae285b62e22add5b870fbfb14dbab1a6b4186`, including the structural reverse-proxy tool-schema transport.


## Assistant 0.1.15 / runtime 0.2.25 — main-first ecosystem dependencies

Native Assistant dependencies now follow `kitt-memory` and `kitt-protocol` `main` instead of embedding cross-repository SHAs. The HUD follows `kitt-protocol#main`, and the root ecosystem installer refreshes those K.I.T.T. dependencies inside its temporary build checkout before compiling. Component lockfiles remain local build artifacts, not ecosystem revision authorities.

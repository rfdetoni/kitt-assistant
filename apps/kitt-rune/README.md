# KITT Rune UI

Cross-platform UI shell for the KITT ecosystem. Built with **Rune**, pinned to
an immutable commit of `anomalyco/rune` (see `Cargo.toml`). It lives in a
separate Cargo workspace so native GPU dependencies never enter `kittd` or
the KITT Agent's normal boot path.

## What works in this stage

- A real Rune UI on Linux, macOS, Windows and the experimental WebGPU target.
- Unified dark shell with accessible navigation across Overview, Assistant,
  Models, Proxy, Cortex, Agents, Memory, Sessions and Settings.
- Read-only live status from the existing loopback Control Center: daemon,
  voice/STT, model names, memory DB status and Reverse Proxy status.
- No tokens, private memory, browser cookies, untrusted tool outputs, or
  arbitrary configuration fields are forwarded to the canvas.
- The native UI uses an allowlisted, bounded, asynchronous-in-relation-to-UI
  background thread to fetch status. Web uses a same-origin read-only host
  with allowlisted proxy endpoints and a `wasm_bindgen` state bridge.
- Rune tooling can capture headless screenshots and the element tree.

This **does not yet replace** edit/save configuration, session streaming,
approval flows, HUD overlay and agent tool controls. The existing UIs stay
available until all equivalent Rune actions pass security and functional QA.

## Desktop

Install Rust with a toolchain supported by the pinned Rune revision.
On Linux install `libxkbcommon-dev`, `libwayland-dev`, `pkg-config` and
other system libraries required by the Rune shell.

```sh
cargo run --manifest-path apps/kitt-rune/Cargo.toml --bin kitt-rune
```

The native shell reads `127.0.0.1:41828` on a 15-second interval, via GET
requests only. No credentials are accepted in an environment-controlled URL.
Run the existing KITT Control Center first.

## HUD nativo opcional

O `kitt-rune-hud` assina eventos pelo IPC autenticado já utilizado pelo HUD atual.
Ele renderiza estados e mensagens em Rune, sem executar ferramentas. Para testar
sem substituir o Tauri permanentemente, configure `KITT_HUD_BIN` com o caminho
absoluto do binário compilado e reinicie o daemon. Eventos de imagem são mostrados
por seu texto alternativo nesta etapa; mantenha o HUD antigo para paridade total.

```sh
cargo run --manifest-path apps/kitt-rune/Cargo.toml --bin kitt-rune-hud
```

## WebGPU

Rune's WebGPU target is experimental. Build using its pinned repository
checkout and wasm-bindgen version from upstream's `WEB.md`:

```sh
git clone https://github.com/anomalyco/rune.git
git -C rune checkout 4a2e437954f01eaacc13c0515a1d39474dd6512c
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked

RUNE_REPO="$PWD/rune" sh apps/kitt-rune/build-web.sh
node apps/kitt-rune/web/serve.mjs
```

Access `http://127.0.0.1:41931`. The server *only* listens on loopback,
serves compiled WebGPU assets, and forwards three GET endpoints to the local
Control Center after stripping all non-allowlisted fields. API mutation,
session endpoints, tool invocation and secrets are not exposed.

If WebGPU cannot initialize, the browser shows a text fallback; use the
original Control Center at `http://127.0.0.1:41828`.

## Release / compatibility

- Root KITT release pins are **not** changed until the new shell and remaining
  parity gates pass. No new runtime is started automatically.
- Build output is not committed. Use immutable Rune revisions and license
  review before distributing binaries.
- The Rune app is not the tool execution authority. Approval, wallet,
  session permissions, privacy and mutations stay with their original owners.
- Validation: `node --test apps/kitt-rune/web/serve.test.mjs` and
  `cargo test --manifest-path apps/kitt-rune/Cargo.toml --lib`.

# Assistant 0.1.41 / Runtime 0.3.21 — release-pin lock repair

The root KITT ecosystem installation failed at `Perform release-channel Assistant lock smoke` with `Assistant Cargo.lock does not match release pins: kitt-protocol`. Root version 0.15.27 composes Protocol 0.13.2, but Assistant's Rust workspace and HUD Cargo.lock still resolved the previous Protocol 0.13.1 commit.

## Changes
- Rust workspace and HUD Cargo.lock now resolve Protocol `0.13.2`,
  SHA `0b9300e418557b00e371eeb2c70ef91fbf50855d`.
- Python Runtime uv.lock resolves Agent CLI `0.87.9`,
  SHA `f69d2119579bb4d56178de01d7f7763ebdc9bf74`,
  with Python Protocol 0.13.2.
- Workspace `0.1.41`, HUD `0.1.18`, Python Runtime `0.3.21`.
- No behavioral or IPC changes; this is immutable dependency alignment.

## Required checks
Rust `cargo build --locked`, Python `uv sync --locked`, HUD
lock/package consistency and the root release-channel full installation,
without weakening pin validation or bypassing test gates.

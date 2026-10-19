# Coordinated Memory lock update (2026-10-19)

The root KITT 0.16.19 release gate exposed a mismatch between current Memory main
`646a213672833c42f0e4eacf1faf048261e5a4c6` and the Assistant lock's older git revision.
The Assistant lock now pins both kitt-memory-core and kitt-memory-sqlite to that
immutable revision and adds the sha2 0.11 dependency graph alongside the
existing sha2 0.10 graph, with version-qualified edges.

CI must run `cargo check --workspace --locked` (Rust 1.88), relevant tests and
root `ecosystem-integration` against the exact Assistant and Memory SHAs.
Do not publish the root release if the pinned Assistant lock smoke fails.
No dependency bounds or MSRV were loosened and no installer lock gate was bypassed.

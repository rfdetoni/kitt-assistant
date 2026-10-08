# Assistant 0.1.34 / Runtime 0.3.1 — Protocol v4 Release Pin Repair

**Cause:** the KITT ecosystem `0.15.0` release manifest pins Protocol `ed096ba68368c8b449c1baf715e0000871ec9fb9` (0.11.0), but the prior Assistant `Cargo.lock`, HUD Cargo/npm locks, and Python runtime lock still pinned `c3bb542f81f8808f0d93f950ab6af5c223fddf4b` (0.10.0). The release installer correctly refused to build from mixed immutable revisions. The Assistant Python runtime also declared `kitt-agent-cli>=0.85.0,<0.86`, which excluded the Agent 0.86.0 pinned by the ecosystem.

**Changes:** upgrade the native Assistant/HUD dependency locks to Protocol 0.11.0 and the merged immutable SHA; align the Assistant runtime lock with Agent 0.86.0 and its immutable SHA; update the Python package compatibility range; keep all release integrity checks active. Synchronize CI checkouts, native package versions, HUD npm metadata, and the release notes.

**Verification:** Assistant CI must pass `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo check --workspace --no-default-features --locked`, `cargo test --workspace --locked`, Python runtime installation plus `pip check`, tests, and HUD `npm ci`/build. The root release-channel `--preset full` smoke must additionally consume the pinned Assistant SHA and pass the unmodified release lock guards.

Avoid hiding or bypassing the lock mismatch: immutable revisions are an explicit release guarantee.

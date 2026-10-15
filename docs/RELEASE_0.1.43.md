# Assistant native 0.1.43 — Memory 0.10.0 lock alignment

KITT root 0.15.42 release installation failed before compile because Assistant native Cargo.lock pinned kitt-memory-core/sqlite 0.9.3. Pin both to Memory 0.10.0 at `eee55669ef7400b2c1f31b6e658a28f018b77723`, bump native workspace package and lock metadata to 0.1.43. No Rust API or wire changes.

Validate: `cargo check --workspace --locked`, `cargo test --workspace --locked`, and root `ecosystem-integration` with an updated Assistant SHA.
# Assistant 0.1.56 / Runtime 0.4.11 — Clean-Room dependency lock reconciliation

- Promote Agent CLI 0.88.11 and Protocol 0.14.2 with immutable commit pins.
- Align Rust workspace, Cargo.lock, HUD Tauri Cargo.lock, HUD npm package-lock and Python runtime uv.lock.
- Keep daemon/IPC, Clean-Room host authority, and existing user-facing behavior unchanged.
- Release announcements now live in CHANGELOG.md; README stays focused on architecture, install and usage.
- Full Python/Rust/HUD CI is required; Windows Oracle conformance of freshly generated Wine code remains unverified.

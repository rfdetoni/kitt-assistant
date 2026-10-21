# Assistant Runtime 0.4.33 — Agent CLI 0.88.43

This revision locks the Assistant Python companion to the Agent CLI 0.88.43 main commit `21706d0a48e5a6f31675a7d9380eef9fdd63bd75`.

- Python package: `kitt-assistant-runtime 0.4.33`
- Compatible Agent CLI range: `>=0.88.43,<0.89`
- `uv.lock` Agent version and SHA and two immutable CI Agent checkout refs synchronized
- No Rust source, Cargo lock or authorization-policy changes

The underlying Agent fix adds strictly content-anchored `*** Begin Patch` / `*** Update File` parsing in the same guarded mutation/rollback engine. `*** Add/Delete/Move File` remain unsupported, and malformed hunks are rejected. Unattributed ChatGPT Web responses still require explicit safe continuation; they are never automatically replayed.

Validate with Assistant Python `uv lock --check`, dependency pin regression, and CI life-cycle suites before promoting the root KITT ecosystem release.

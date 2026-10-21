# Assistant Runtime 0.4.32 — pin Agent CLI 0.88.42

This release synchronizes the Assistant Python companion and CI with the immutable Agent CLI main revision `de7c0ad8919363bf4bf8c58ee8aad1d4c28730e4`.

- Package `kitt-assistant-runtime` version: **0.4.32**
- Agent dependency minimum: **0.88.42**, bounded below 0.89
- Agent CLI `uv.lock` revision and two CI checkout refs: **de7c0ad...**
- Rust runtime / Cargo.lock: unchanged

## Why

The prior Assistant lock and CI checks used Agent CLI 0.88.40, preventing the Assistant from consistently deploying the fixes included in KITT 0.16.42 and the newer Goal budget-checkpoint continuation. Keep Python package metadata, lock and CI checkout pins synchronized to avoid behavioral drift.

## Validation

The pinned Agent SHA is immutable. A regression verifies the Agent lock SHA, both CI checkouts and the declared Python version range are consistent. Run Assistant Python CI, including `uv lock --check`, before integrating. This is a dependency/pin update; it does not alter Rust behavior or widen approval permissions.

# kitt-assistant 0.1.31 / runtime 0.2.44 — Immediate lifecycle events and cancellation

The daemon flushes TurnStarted and thinking lifecycle events immediately in execution and continuation paths. Cancellation owns a single terminal notification; late producer events cannot appear after the cancellation. The runtime aligns its Agent and Protocol locks with this release.

## Verification

Regression checks cover the concrete bugs fixed by this release. Native changes are validated with Rust formatting, Clippy, workspace tests and a Python 3.14 wheel integration. Live provider accounts and STT model inference are not part of these local checks.

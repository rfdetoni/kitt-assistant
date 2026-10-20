# Assistant Runtime 0.4.18

- Agent CLI dependency: `>=0.88.24,<0.89`, source pinned to `58ffe92a3e6322e47f3b5fa797cbc19851c04702`.
- Python runtime metadata and `uv.lock`: 0.4.18.
- CI immutable checkouts: Agent CLI `58ffe92a3e6322e47f3b5fa797cbc19851c04702`.
- Rust runtime 0.1.61 and `Cargo.lock`: no change.

The CLI now emits a precise error and model-facing example when `plan.submit` omits `arguments.proposal`; `plan.verify` needs a previously persisted plan; sandbox-unavailable errors distinguish host OS isolation failures from human approval requests. Strong host sandbox policies and explicit guest identity checks are unchanged.

CI should verify Python lock, contracts and portable service lifecycle before promoting the source pin to the ecosystem installer.

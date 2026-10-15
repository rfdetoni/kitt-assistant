# Assistant Runtime 0.3.35

- Agent CLI 0.87.28 pin: f5eaff224b80c02ceaaf390bc3b84e0943d95abf
- Update immutable Agent checkout in CI and Python uv.lock; runtime package and lock to 0.3.35.
- Safety: after `ui_timeout` with no KAP action, goal blocks until explicit inspection/resume, avoiding repeated browser prompts.
- No native Assistant API changes.

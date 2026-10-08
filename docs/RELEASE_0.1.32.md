# kitt-assistant 0.1.32 / runtime 0.2.45 — WebChat token ownership

The runtime lock and CI select Agent CLI 0.84.10 (`0a06eac924162a063945384ae79e30e5384689f9`), which delegates reverse-proxy input/output token limits to WebChat. Remote context telemetry displays WebChat ownership when local window_size is zero, instead of showing a false capacity percentage or limit. Existing IPC, lifecycle, cancellation and native contracts retain their behavior.

Validation: JavaScript syntax, focused Python runtime tests and compatible runtime/lifecycle/HUD CI suites.

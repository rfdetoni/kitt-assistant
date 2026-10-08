# Assistant Python runtime 0.3.2 — KITT Agent CLI 0.86.1

The native Assistant and HUD code remain unchanged at version 0.1.34 (HUD 0.1.17); Protocol remains at 0.11.0.

## Release alignment

KITT Agent CLI 0.86.1 adds bounded parallel subagent fan-out for dependency-ready, disjoint file scopes during automatic Goal contracts under the user's autonomous policy. The host retains goal leases, checks, reviews and durable state. The Agent TUI `Ctrl+X, A` panel now receives asynchronous child lifecycle events; Allow All permits direct non-shell `run_command` argv subject to security boundaries.

Assistant's Python runtime `uv.lock` pins Agent commit `4155006ecb92d84c47fb618a9641df0a45263f01` (0.86.1), and the integration workflow checks out the same immutable commit. Bump runtime from 0.3.1 to 0.3.2 without changing the Rust/Protocol dependency graph.

## Acceptance gates

- `uv.lock` aligns with both the Assistant runtime version and the Agent CLI immutable revision;
- Assistant CI integrates the Agent and existing Protocol, does not bypass locks;
- KITT ecosystem root manifest pins this merged Assistant SHA and Agent CLI 0.86.1 SHA;
- Root release `--preset full` installation verifies the pinned dependencies instead of silently mixing revisions.

No backwards compatibility layer is introduced.

# Assistant Runtime 0.4.43 — Autopilot compatibility

- Requires Agent CLI `>=0.89.1,<0.90`.
- Exact Agent revision: `0ddae4475cdd05d1c0c14059759ac0fd8d3ade88`, consistent across Assistant CI source checkouts and `uv.lock`.
- Includes prior Agent CLI work on Docker/Bubblewrap sandbox toolchains, a trusted persistent Compose broker, recoverable task checkpoints with compare-and-swap rollback, sandboxed verification, and opt-in root network delegation.
- Keeps Rust workspace, Protocol lock SHA, Hud and Proxy unchanged.
- Publishing and remote effects still require separate operator-authorized connectivity and credentials; this update does not automatically grant access to the Docker socket or host shell.

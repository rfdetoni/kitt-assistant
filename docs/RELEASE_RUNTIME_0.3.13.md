# Assistant Runtime 0.3.13 — Daemon readiness for optional MCP REA

During REA MCP initialization, the daemon can remain in "starting" longer
than ten seconds. The old detached launcher opened authenticated ping
connections roughly ten times per second, terminated a still-starting
daemon after ten seconds, and a new frontend could also request a
replacement of an authenticated daemon whose state was still "starting".

Fix:
- Extend the bounded startup readiness budget from 10 to 45 seconds.
- Limit IPC health probing during bootstrap to about 1.3 attempts/second.
- Preserve authenticated, already-starting daemons and wait for their
  readiness rather than trying an authenticated stop/replacement.
- Add a regression test proving that a second launcher does not kill
  or replace a daemon still initializing.

Pins:
- Agent CLI 0.86.16 at
  `482e0b8e24fab85936a711e540f64caef301afe0`
  (MCP stdio buffer/size agreement).
- Both cross-platform CI checkout pins and the Python uv.lock are updated.

Scope: host-managed daemon and optional MCP integrations only. No
Docker/Podman/Windows guest or RE lab changes.

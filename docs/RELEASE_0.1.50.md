# Assistant 0.1.50 / Python Runtime 0.4.5 — compatible Agent CLI 0.88.5

The Python runtime requires Agent CLI >=0.88.5,<0.89. Standalone CI and the Python uv lock select the same immutable validated Agent main commit.

Agent CLI 0.88.5 automatically selects a working Docker or Podman engine for RE tooling (explicit selections remain authoritative) and recognizes installer-managed Linux RE and Dockur Windows containers without recreating them. Read-only inventory and operator-only lifecycle retain Docker identity/isolation checks and do not grant unattended Windows oracle probes.

Rust workspace package metadata and its five Cargo.lock entries bump to 0.1.50. Python Assistant runtime bumps to 0.4.5. No Rust source, Protocol wire contract, Memory, Toolbox or HUD changes.

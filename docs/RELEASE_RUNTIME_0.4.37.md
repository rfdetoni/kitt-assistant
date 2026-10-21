# Assistant Runtime 0.4.37 — Git -C read-only inspection

Python package 0.4.37, min dependency `kitt-agent-cli>=0.88.48,<0.89`, `uv.lock`, and both CI Agent checkouts pin merged commit `9f612193689006d79b2106a88b98e3f1633ac1a4`.

The 2026-10-21 `mbs-wine` run failed at `git -C mbs-wine status --short --branch`, a read-only Git request denied by the Agent PolicyEngine. Agent CLI 0.88.48 permits the safe relative `-C` form only for read-only status/diff/log/branch queries within the workspace, while preserving denial of mutating commands, outside paths and protected Git metadata.

Rust/daemon/Cargo and Reverse Proxy are unchanged. Missing Wine-Staging checkout, Oracle validation and Reverse Proxy child session under RSS pressure remain unverified/blocked. This update synchronizes the independently merged Agent patch into the Assistant Python runtime.

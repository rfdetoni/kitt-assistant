# KITT Assistant 0.1.60 / Runtime 0.4.15

The Agent CLI 0.88.21 source-tree version fallback was inconsistent with its already-pinned package metadata, blocking automatic release validation. Agent [PR #127](https://github.com/rfdetoni/kitt-agent-cli/pull/127) aligned `kitt/__init__.py` with `pyproject.toml` and `uv.lock`. This Assistant release fixes its immutable CLI pin to the merged `0484f2e81351d8c8bdab0cd9722d5b026ba98b25`, updates the Python lock and CI checkout, and bumps Assistant versions.

- Assistant package 0.1.60, Python runtime 0.4.15.
- Runtime behavior unchanged from 0.1.59/0.4.14: TUI obtains daemon authoritative autonomy status, and durable approval lists return trusted executor identity.
- No sandbox, network, workspace-policy or approval-elevation bypass.

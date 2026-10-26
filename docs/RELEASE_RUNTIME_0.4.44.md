# Assistant Runtime 0.4.44

- Agent CLI `>=0.89.2,<0.90`, at immutable main SHA `105ebd7eaf7477ab6cc6ec6cf789d2c707afc614`.
- Python package constraint, uv lock and both Assistant CI checkouts agree.
- Optional `autopilot.publish` runs on the trusted Agent side only after operator-selected repo/branch, scoped token and passing validation; it cannot grant credentials to guest workers.
- Rust/Protocol/HUD locks unchanged.

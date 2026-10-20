# KITT Assistant 0.1.59 / Runtime 0.4.14

- Runtime IPC returns effective daemon autonomy with `runtime.get_autonomy` and preserves executor identity when listing pending approvals.
- Python runtime dependency/uv.lock now pin Agent CLI 0.88.21 (`306f312b6d9323731ff373a29b557ce54e29058b`); all five Rust crate versions bump to 0.1.59 in Cargo.toml/Cargo.lock.
- No grant of new capabilities: command sandbox requirements, path policy, network gates and critical operations stay enforced.
- Includes durable approval identity and readback regression tests. See `docs/RELEASE_RUNTIME_0.4.14.md`.

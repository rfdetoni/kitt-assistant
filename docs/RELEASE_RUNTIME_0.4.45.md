# Assistant Runtime 0.4.45 — Autopilot persistent worker verification

- Pin Agent CLI 0.89.3 (SHA `1b17a65f04723b2135d876d9eb4b96432412c430`) in the Python runtime requirement, uv.lock, and the two CI Agent checkouts.
- The Agent's Docker smoke test validates real isolated persistent worker reuse, network denial, readonly enforcement, and container reprovisioning.
- Trusted GitHub publisher, Protocol revision, and Rust/Node dependencies are unchanged.

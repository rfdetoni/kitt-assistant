# Assistant 0.1.52 / Runtime 0.4.7

- Runtime consumes Agent CLI 0.88.7 and pins its immutable merged main SHA
  `58fdac1235f13aeac21b4b764f54dc5e99f560e9`.
- Python runtime lock, dependency constraint and CI checkout use the same
  Agent version/SHA. Five local Rust workspace crates bump to 0.1.52 without
  modifying the shared protocol or runtime APIs.
- Agent owns the RE Lab Docker orchestration and Windows OEM probe transport.
  The Assistant does not introduce another guest provisioner.
- Windows/KVM OEM, pinned SSH and live Wine/Windows differential E2E still
  require operator-side execution on authorized hardware; CI does not grant
  Oracle READY merely from container state.

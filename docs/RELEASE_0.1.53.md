# Assistant 0.1.53 / Runtime 0.4.8

- Uses Agent CLI 0.88.8 at immutable merged main SHA
  `fb4f1966125a7bd3eece65989df01632956d2439`.
- Updates Python runtime dependency constraint, uv.lock Git SHA, version
  and GitHub CI checkout consistently.
- Bumps local Rust workspace versions and Cargo.lock to 0.1.53. Registry
  dependencies, checksums, IPC and daemon contracts are unchanged.
- Private Win32 probe compilation and isolated Wine candidate execution are
  Agent-owned; sensitive OEM SSH keys never enter the Wine guest mount.
- Live Windows 11/KVM OEM/SSH/probe E2E is still operator-side UNVERIFIED.

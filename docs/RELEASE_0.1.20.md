# Assistant 0.1.20

The immutable ecosystem installation exposed a stale HUD npm lock still resolving Protocol 0.9.0. Resolve Protocol 0.9.1 at the same SHA as both Cargo consumers and the Python runtime. The new runtime CI regression reproduces the mismatch before the lock update and compares all four consumers after it. The root installer's fail-closed lock guard remains intact.

Native workspace metadata is 0.1.20. Python runtime remains 0.2.33 and HUD package metadata remains unchanged; wire version stays 1.

Validation: lock-parity regression, HUD clean npm installation and build, native locked metadata and the Assistant CI matrix.

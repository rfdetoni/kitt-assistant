# KITT Assistant 0.1.33 / runtime 0.3.0

Align Python runtime dependencies and immutable Agent/Protocol revisions with Agent contract v3. Runtime requires Agent >=0.85.0,<0.86; the previous range would reject the new consumer. Native Cargo locks use Protocol 0.10.0, and lifecycle/runtime CI checks the same Agent revision. HUD and lifecycle behavior remain unchanged.

Runtime/lifecycle CI, frozen consumer locks and root clean installation validate the compatible composition.

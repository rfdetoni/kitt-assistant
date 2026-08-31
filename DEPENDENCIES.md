# Cross-repository dependency during development

The bundle uses sibling path dependencies (`../kitt-memory`) so all code can be reviewed together. After repositories are created, replace the sibling `kitt-memory` and `kitt-protocol` path/file dependencies with immutable Git tags or published packages, e.g.:

```toml
kitt-memory-core = { git = "https://github.com/rfdetoni/kitt-memory", tag = "v0.1.0" }
kitt-memory-sqlite = { git = "https://github.com/rfdetoni/kitt-memory", tag = "v0.1.0" }
kitt-protocol = { git = "https://github.com/rfdetoni/kitt-protocol", tag = "v0.1.0" }
```

For the HUD, replace `file:../../../kitt-protocol` with an immutable Git tag such as `github:rfdetoni/kitt-protocol#v0.1.0`, or a published `@kitt/protocol` package.

Do not depend on `main` in release builds.

# Cross-repository dependencies

KITT Assistant is a separate repository and does not require sibling repository folders.

Shared ecosystem components are consumed from immutable Git revisions:

- `kitt-protocol`: canonical IPC/data contracts;
- `kitt-memory-core` and `kitt-memory-sqlite`: memory domain/storage.

Rust manifests pin reviewed commit SHAs and the Cargo lockfiles preserve the resolved graph. The HUD consumes `@kitt/protocol` from the same immutable Git revision instead of a relative sibling path.

The distribution repository (`rfdetoni/kitt`) remains the authority for the compatible ecosystem snapshot. Cross-repository dependency bumps must be coordinated with `ecosystem.lock.json`.

There is no legacy IPC compatibility layer. `kittd`, `kittctl`, HUD and external clients use KITT Protocol v1 exclusively.

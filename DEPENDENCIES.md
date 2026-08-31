# Cross-repository dependencies

KITT Assistant is a separate repository. It must not require sibling repository folders.

Shared ecosystem components are consumed from their own Git repositories:

- `kitt-protocol`: canonical IPC/data contracts.
- `kitt-memory-core` and `kitt-memory-sqlite`: memory domain/storage.

During laboratory development, dependencies track `main` and `Cargo.lock` pins the exact Git commits resolved by Cargo. Before a public release, replace branch dependencies with signed release tags or published crates.

There is no legacy IPC compatibility layer. `kittd`, `kittctl`, HUD and external clients use KITT Protocol v1 exclusively.

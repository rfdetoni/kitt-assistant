# Cross-repository dependencies

KITT Assistant is a separate repository and does not require sibling repository folders.

Shared ecosystem components are consumed from immutable Git revisions:

- `kitt-protocol`: canonical IPC/data contracts;
- `kitt-memory-core` and `kitt-memory-sqlite`: memory domain/storage (currently the reviewed 0.2.1 snapshot).

Rust manifests pin reviewed commit SHAs and the Cargo lockfiles preserve the resolved graph. The HUD consumes `@kitt/protocol` from the same immutable Git revision instead of a relative sibling path.

The distribution repository (`rfdetoni/kitt`) remains the authority for the compatible ecosystem snapshot. Cross-repository dependency bumps must be coordinated with `ecosystem.lock.json`.

There is no legacy IPC compatibility layer. `kittd`, `kittctl`, HUD and external clients use KITT Protocol v1 exclusively.


The standalone Python-runtime CI intentionally pins a known-compatible Agent revision instead of following `main`. This pin is a reproducible compatibility fixture, not the ecosystem promotion authority. The exact multi-repository production/development composition remains `rfdetoni/kitt/ecosystem.lock.json`, whose integration workflow installs and tests the complete frozen set.

The shared KITT Protocol v1 and the Agent Python daemon protocol are separate version domains. KITT Protocol v1 covers the cross-language Assistant/Memory/HUD envelopes; `DAEMON_PROTOCOL_VERSION` covers Agent ↔ Python daemon session/control messages. The latter has one authority in the Agent root package and is imported by both daemon client/server event code.

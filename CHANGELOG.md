# Changelog

## 0.1.18 / runtime 0.2.32 — 2026-10-02

- Make Python daemon readiness depend on successful startup of the required Agent runtime; authenticated ping now exposes `starting|ready|degraded|stopping|stopped|failed` lifecycle semantics and a fail-closed `ready` flag.
- Preserve existing request/conversation/turn correlation identifiers in relevant daemon telemetry and native request-error logs.
- Make `kittctl service start|stop|restart|status` propagate native service-manager failures instead of reporting false success; add a narrow Linux/macOS/Windows lifecycle portability matrix for service compilation plus launcher/path/cleanup tests.
- Bump the native Assistant workspace to 0.1.18 and Python runtime to 0.2.32. Only local package versions changed in locks; HUD metadata and external dependency revisions remain unchanged.
- Keep Agent CLI as execution/policy/approval authority and `kitt-memoryd` as durable semantic-memory authority; no Protocol or root-`kitt` contract change is included.

## 0.1.17 / runtime 0.2.31 — 2026-10-02

- Refresh the native Rust graph to KITT Memory 0.9.0 and KITT Protocol 0.9.0 without changing protocol wire version 1.
- Validate the Python companion against promoted Agent CLI 0.83.5 revision `47a8e75573f199cea9b465745a540606ea71fbee` and resolve Protocol 0.9.0 in the uv lock.
- Bump the native Assistant workspace to 0.1.17 and Python runtime to 0.2.31; HUD metadata remains 0.1.16 because its behavior and dependency surface are unchanged.
- Preserve ownership boundaries: Agent owns policy/cancellation/wallet/runtime authority, while standalone kitt-memoryd owns durable Agent semantic memory.

## runtime 0.2.30 — 2026-10-02

- Close accepted connections before awaiting the asyncio listener; bound graceful shutdown and abort clients that do not drain. Apply the same rule to startup rollback and await writer task cancellation.
- Validate the entire Python runtime test tree, including the new idle-client shutdown regression and semantic surface actions, against Agent CLI 0.83.3.
- Refresh the main-resolved Python lock. Native workspace/HUD versions remain 0.1.16.
- Review: High severity / High confidence, `kitt/daemon/server.py`: Python 3.14 listener shutdown waits for active clients; the old order could hang indefinitely and retain the instance lock/runtime. Fix: close clients before waiting, five-second grace then abort, bounded writer cleanup. Validation: live daemon/client shutdown, surface round trip and all runtime tests.


## runtime 0.2.29 — 2026-10-02

- Validate the companion runtime against Agent CLI 0.83.0 and widen its compatible range to `<0.84`.
- Discover the entire runtime test tree with pytest, including daemon integration cases in namespace subdirectories missed by unittest discovery.
- Record the main-resolved Python graph for uv consumers. Native workspace/HUD versions are unchanged because their behavior did not change.
- Preserve Agent ownership of policy, wallet, cancellation and prompt construction; Assistant reuses these corrections through its shared control plane.

## runtime 0.2.27 - 2026-10-01

- Widen the reviewed Python companion range to `kitt-agent-cli>=0.81.0,<0.83` and validate the runtime against Agent CLI 0.81.0 revision `5529cbf5c3ddf9e1400408089a36bfaf73b9051d`.
- Keep approval, policy, budget, run coordination, ContextEpoch and workspace snapshot authority in Agent CLI; this is a compatibility-only runtime release.

## runtime 0.2.26 - 2026-09-30

- Require and validate Agent CLI 0.80.x as the companion control-plane contract.
- Forward executable principal identity through daemon `approval.remember` so workspace saved permissions preserve the same USER/GOAL/CHILD scoping as local Agent execution.
- Keep policy, approval, run coordination, budgets, ContextEpochs and workspace snapshots owned by Agent CLI rather than duplicating those authorities in the Assistant runtime.

## 0.1.14 / runtime 0.2.24 / HUD 0.1.14 - 2026-09-29

- Validate the companion runtime against Agent CLI 0.78.4 (`7d56faec43fa6f5c0e4b1f63c0c18e269a0eb9d8`).
- Align workspace and HUD version metadata at 0.1.14.
- Preserve existing daemon, approval and Memory 0.5.0 contracts.


## 0.1.13 / runtime 0.2.23 / HUD 0.1.13 - 2026-09-29

- Pin native memory dependencies to KITT Memory 0.5.0.
- Validate the Python runtime against Agent CLI 0.78.3 (`89a63da16368a59eac5eee185373bfbf89f516b1`).
- Align Rust workspace, HUD/Tauri metadata and lockfiles at 0.1.13.
- Preserve Protocol 0.4.0, standalone memory authority and daemon/remote APIs.


## 0.1.12 / runtime 0.2.22 / HUD 0.1.12 - 2026-09-28

- Widen Assistant runtime compatibility through Agent CLI 0.78.x (`f51dbba8a0506e90366ddb4e95026dc6c6614699`).
- Pin standalone Python-runtime CI to the promoted Agent 0.78.0 revision.
- Align Rust workspace, HUD/Tauri metadata and lockfiles at 0.1.12.
- Preserve daemon/remote APIs, approval durability, Protocol 0.4.0 and standalone Memory ownership.

## runtime 0.2.21 - 2026-09-28

- Widen the companion runtime to Agent CLI 0.76.x–0.77.x while preserving daemon/remote APIs and ownership boundaries.

## 0.1.9 / runtime 0.2.20 - 2026-09-28

- Align the Python runtime with Agent CLI 0.76.x.
- Document standalone kitt-memoryd as the durable Agent-memory authority.
- Keep Assistant optional for Agent + Reverse Proxy installations.


## 0.1.8 - 2026-09-28

- Make resident microphone/wake-word capture a `kittd` Cargo feature named `voice`.
- Keep `voice` enabled for normal source builds while allowing the ecosystem installer to compile a portable non-audio daemon when Linux ALSA development packages are unavailable.
- Preserve Control Center, memory, HUD, model routing, transcription APIs and TTS abstractions in non-audio builds.


## 0.1.7 / runtime 0.2.19 / HUD 0.1.7 - 2026-09-28

- Integrate KITT Protocol 0.3.0 and Memory 0.3.0 and extend Python-runtime compatibility through Agent CLI 0.75.x.
- Add capability-aware declarative Surface rendering to the remote Control Center with bounded component graphs and text-only DOM projection.
- Add a narrow daemon/HTTP semantic `surface.action` path; browser clients never receive a generic `kitt_runtime` execution endpoint.
- Pin standalone Python CI to Agent CLI 0.75.1 and refresh Rust/npm/Tauri locks.

## 0.1.6 / runtime 0.2.18 / HUD 0.1.6 - 2026-09-27

- Require Python 3.14+ for the Assistant Python runtime, matching the single current-interpreter support policy.
- Pin Rust daemon/CLI and HUD to kitt-protocol 0.2.1 while preserving wire protocol v1.
- Refresh Cargo/npm lock metadata and HUD/Tauri package versions for the 0.1.6 snapshot.


## 0.1.5 / runtime 0.2.17 - 2026-09-27

- Unify Python daemon handshake/event protocol version on the Agent's `DAEMON_PROTOCOL_VERSION` authority and add an end-to-end parity test.
- Move HUD socket writes outside the global subscriber mutex so slow clients cannot serialize all event delivery.
- Pin kitt-memory 0.2.1 and refresh the Cargo lock.
- Make standalone Assistant CI reproducible by pinning a known-compatible Agent 0.74.4 revision instead of mutable `main`.


## 0.1.4 / runtime 0.2.16 / HUD 0.1.4 - 2026-09-27

- Pin kitt-memory 0.2.0 and kitt-protocol 0.2.0 across daemon, domain and infrastructure crates.
- Preserve conversation scope keys and point-in-time recall through kittd protocol-v1 handlers.
- Preserve zero-result recall requests instead of coercing limit 0 to 1.
- Update Rust memory struct literals for schema-v4 contracts and expose scope_key in recalled DTOs.
- Extend the Python runtime compatibility bound through kitt-agent-cli 0.74.x.
- Refresh locked dependency graph, CI protocol pin, HUD protocol dependency and npm/Tauri locks.

## 0.2.15 - 2026-09-25

- Extend the Python runtime compatibility range through KITT Agent CLI 0.72.x.
- Keep the upper bound explicit so later Agent contract changes remain reviewable.


## 0.1.0 - Unreleased

- Initial KITT ecosystem foundation.

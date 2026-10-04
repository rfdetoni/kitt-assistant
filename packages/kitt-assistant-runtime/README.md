# K.I.T.T. Assistant Runtime

Python runtime companion for K.I.T.T. Assistant. It owns the local Agent daemon transport/server and remote web runtime while the coding control plane remains in `kitt-agent-cli`.

Source extraction baseline: `rfdetoni/kitt-agent-cli@710597fc15677e3d88097623f1113120577b0ee2`.

The package intentionally shares the `kitt` namespace with the Agent control plane. It depends on `kitt-agent-cli` for orchestration/domain services and contains no duplicate Agent core implementation.

## Runtime 0.2.32 — lifecycle and readiness

Runtime 0.2.32 is validated with Agent CLI 0.83.10 revision `5e4235470bd3822b3410be14adb2a693385b63ba` and Protocol 0.9.0. Readiness is fail-closed: an authenticated listener may exist while startup is in progress, but the daemon is `ready` only after the required Agent runtime has started. Request, conversation and turn identifiers are preserved in relevant daemon telemetry.

## Daemon lifecycle

The resident runtime exposes six lifecycle states with narrow meanings:

- `starting`: the daemon owns startup resources but its required runtime is not ready yet.
- `ready`: the required runtime has started; this is the only state that satisfies readiness.
- `degraded`: a resident PID exists but authenticated readiness cannot be established.
- `stopping`: an authenticated stop was accepted and the process is still draining or cleaning up.
- `stopped`: no resident daemon is active and local daemon state is cleaned up.
- `failed`: startup or cleanup failed, or stale state exists without a live resident process.

Detached startup continues to use the running Python interpreter and an upgrade-stable working directory. Stop remains authenticated over IPC and never signals an unverified PID.

## Approval lifetime

Human tool/command approvals are durable interaction state. A request in `PENDING` has no wall-clock timeout and remains listable/decidable across long idle periods. The daemon never evicts an active approval to satisfy queue capacity; when the direct-approval queue is full it rejects creation of a new request instead. Short-lived, single-use TTLs apply only after an approval grant is issued.

## Agent compatibility

Runtime 0.2.32 supports `kitt-agent-cli>=0.81.0,<0.84` on Python 3.14+ and is validated against Agent CLI 0.83.10 revision `5e4235470bd3822b3410be14adb2a693385b63ba`. Agent CLI owns run coordination, execution budgets, policy, approvals, ContextEpochs and workspace execution. Standalone `kitt-memoryd` remains the durable Agent semantic-memory authority.

## Semantic Surface

The daemon exposes capability negotiation and a narrow semantic Surface action endpoint backed by the Agent SafeRuntime. Remote clients never receive a generic `kitt_runtime` endpoint: they can only submit a validated `surface_id`, `component_id`, semantic action id and bounded context. The web renderer consumes only the host-advertised component catalog and constructs DOM nodes with text APIs rather than model-provided HTML.

## Release history

Historical compatibility and release notes live in the repository root [CHANGELOG](../../CHANGELOG.md). This README documents the current runtime contract instead of accumulating per-release history.

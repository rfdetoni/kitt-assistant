# K.I.T.T. Assistant Runtime

Python runtime companion for K.I.T.T. Assistant. It owns the local Agent daemon transport/server and remote web runtime while the coding control plane remains in `kitt-agent-cli`.

Source extraction baseline: `rfdetoni/kitt-agent-cli@710597fc15677e3d88097623f1113120577b0ee2`.

The package intentionally shares the `kitt` namespace with the Agent control plane. It depends on `kitt-agent-cli` for orchestration/domain services and contains no duplicate Agent core implementation.


## Approval lifetime

Human tool/command approvals are durable interaction state. A request in `PENDING` has no wall-clock timeout and remains listable/decidable across long idle periods. The daemon never evicts an active approval to satisfy queue capacity; when the direct-approval queue is full it rejects creation of a new request instead. Short-lived, single-use TTLs apply only after an approval grant is issued.


## Agent compatibility

Runtime 0.2.23 supports `kitt-agent-cli>=0.76.0,<0.79` on Python 3.14+. Agent CLI 0.78.3 is validated by standalone CI and the root ecosystem through the composed shared `kitt.*` namespace. The version range remains intentionally bounded so future Agent contract changes require review.


## Runtime 0.2.19 semantic Surface support

The daemon exposes capability negotiation and a narrow semantic Surface action endpoint backed by the Agent SafeRuntime. Remote clients never receive a generic `kitt_runtime` endpoint: they can only submit a validated `surface_id`, `component_id`, semantic action id and bounded context. The web renderer consumes only the host-advertised component catalog and constructs DOM nodes with text APIs rather than model-provided HTML.


## Runtime 0.2.21 Agent 0.77 compatibility

The companion runtime accepts both Agent 0.76.x and 0.77.x. No daemon/remote API is duplicated or widened; this release only acknowledges the reviewed Agent 0.77 control-plane contract and keeps the shared namespace composition installable.


## Runtime 0.2.22 Agent 0.78 compatibility

The companion runtime widens its reviewed Agent range to `>=0.76.0,<0.79` and is validated against Agent CLI 0.78.0 revision `f51dbba8a0506e90366ddb4e95026dc6c6614699`. No daemon/remote API, approval lifetime or memory ownership semantics are changed.


## Runtime 0.2.23 — Agent 0.78.3 compatibility

The companion runtime keeps the reviewed `>=0.76.0,<0.79` range and validates Agent CLI 0.78.3 revision `718e837bada08c69a88d83fc41e890d2f2b748d1`. Durable semantic memory remains owned by standalone `kitt-memoryd`; Assistant does not reintroduce a second Agent-memory authority.

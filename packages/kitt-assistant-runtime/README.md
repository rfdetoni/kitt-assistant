# K.I.T.T. Assistant Runtime

Python runtime companion for K.I.T.T. Assistant. It owns the local Agent daemon transport/server and remote web runtime while the coding control plane remains in `kitt-agent-cli`.

Source extraction baseline: `rfdetoni/kitt-agent-cli@710597fc15677e3d88097623f1113120577b0ee2`.

The package intentionally shares the `kitt` namespace with the Agent control plane. It depends on `kitt-agent-cli` for orchestration/domain services and contains no duplicate Agent core implementation.


## Approval lifetime

Human tool/command approvals are durable interaction state. A request in `PENDING` has no wall-clock timeout and remains listable/decidable across long idle periods. The daemon never evicts an active approval to satisfy queue capacity; when the direct-approval queue is full it rejects creation of a new request instead. Short-lived, single-use TTLs apply only after an approval grant is issued.

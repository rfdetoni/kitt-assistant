# K.I.T.T. Assistant Runtime

Python runtime companion for K.I.T.T. Assistant. It owns the local Agent daemon transport/server and remote web runtime while the coding control plane remains in `kitt-agent-cli`.

Source extraction baseline: `rfdetoni/kitt-agent-cli@710597fc15677e3d88097623f1113120577b0ee2`.

The package intentionally shares the `kitt` namespace with the Agent control plane. It depends on `kitt-agent-cli` for orchestration/domain services and contains no duplicate Agent core implementation.

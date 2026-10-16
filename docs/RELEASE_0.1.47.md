# Assistant 0.1.47 / Runtime 0.4.2 — acknowledge approved decisions after execution failure

The daemon now acknowledges a valid Goal approval decision even when execution fails. It reports `execution_status=failed` and `execution_error` separately, so the TUI can display the failure and close the decided request. Rejected/unknown approvals remain RPC errors. Successful Goal approvals resume the same durable contract item.

Requires Agent CLI 0.88.2, which permits additive remembered allow rules during explicit approval resume while preserving revocation, policy, principal, sandbox and nonce checks. Real daemon-handler tests use SQLite, Goal state transitions, the actual approval manager, snapshot validation and process execution for both remembered scopes and the terminal failure case.

The full IPC suite requires Unix sockets or the platform transport. The validation container prohibits Unix socket creation (EPERM); handler integration and all non-IPC tests are validated locally. Rust source and shared Protocol contracts are unchanged.

# Assistant Runtime 0.4.14

- Read-only IPC `runtime.get_autonomy` returns the daemon's current persisted policy to the TUI. Explicit operator calls to `runtime.set_autonomy` remain authoritative.
- Persisted approval queues include `executable_identity` from pending-action security context, never from user-controlled tool content. This keeps "always workspace" and session rules bound to the same executor even after queue refresh.
- No bypass of network, path, sandbox or dedicated control-plane checks.
- Regression test checks identity from a durable pending approval.

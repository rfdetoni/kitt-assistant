# K.I.T.T. Assistant 0.1.25 — automatic contract daemon routing

Assistant 0.1.25 / Python runtime 0.2.38 aligns the authoritative daemon execution path with Agent CLI 0.84.2.

## Daemon behavior

Persisted user turns submitted with `mode=auto` are now observed through Agent CLI's durable automatic contract wrapper rather than calling TurnProcessor directly.

The wrapper performs planning, persists ordered Goal contract items, lets the existing GoalScheduler execute one item at a time, and emits progress/terminal events back through the normal daemon event ledger.

Explicit `ask`, `plan` and no-history turns stay on the direct TurnProcessor path.

## Approval and cancellation

When an approval belongs to a Goal-owned contract turn, the daemon executes exactly the approved pending action, records its output/affected paths, and resumes the same contract item without resetting attempts.

Denying such an approval blocks the item and fails the contract with the denial reason.

Cancelling the outer daemon turn cancels the automatic contract; Agent CLI also cancels the active inner TurnProcessor turn when one is running.

## Compatibility

The runtime lock resolves Agent CLI 0.84.2 at `c9bbe79bf2393179a58b616a90552906b422311a`.

No daemon protocol version, KITT Protocol wire schema, Memory contract or native IPC schema changes are introduced.

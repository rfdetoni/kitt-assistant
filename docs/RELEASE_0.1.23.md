# K.I.T.T. Assistant 0.1.23 — Agent CLI 0.84 contract compatibility

Assistant 0.1.23 is a compatibility/alignment release for Agent CLI 0.84.0.

## Changes

- Python runtime bumps from 0.2.35 to 0.2.36.
- The supported Agent range expands from `>=0.81.0,<0.84` to `>=0.81.0,<0.85`.
- The runtime lock resolves Agent CLI 0.84.0 at `b2f627b09643289f31dcb181904722da69395689`.
- Native workspace metadata bumps from 0.1.22 to 0.1.23 so the repository release reflects the consumer-lock change.

## Compatibility

No daemon IPC, Protocol, Memory, HUD, remote-control or native lifecycle contract changed. The Assistant remains a consumer of Agent-owned GoalScheduler behavior; it does not duplicate the new task-contract runtime.

Protocol 0.9.1 and Memory 0.9.2 remain at their current locked revisions.

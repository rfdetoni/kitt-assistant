# K.I.T.T. Assistant 0.1.30 / runtime 0.2.43 — Agent CLI 0.84.8 alignment

Python runtime 0.2.43 resolves Agent CLI 0.84.8 at `809151b40ef4600c8364c73583bd38717b1665cf`.

Agent 0.84.8 forwards bounded, non-terminal Goal inner-turn progress through the existing outer automatic-contract stream. Daemon-owned sessions therefore expose planning, context, thinking, tool and edit progress without changing scheduler authority, approval/cancellation semantics or wire contracts.

Interactive kitt-memory recall is also bounded/fail-soft inside Agent prompt enrichment, with dedicated latency telemetry for memory recall and prompt construction.

This release is consumer alignment only. Assistant daemon, IPC, Protocol and native contracts are unchanged.

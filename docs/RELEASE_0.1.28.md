# K.I.T.T. Assistant 0.1.28 / runtime 0.2.41 — Agent CLI 0.84.6 loop alignment

Python runtime 0.2.41 locks Agent CLI 0.84.6 at `0f14d68b5fbdd6848660f68707230d7d6c3db584`.

Agent 0.84.6 fixes daemon-owned automatic durable contracts that could stall inside the first item: GOAL-owned turns without a nested TaskPlan no longer enter the TaskPlan completion-recovery path. Their existing `GoalStepVerifier` remains the post-turn verification authority.

Assistant's daemon startup-identity behavior from 0.1.27/runtime 0.2.40 is unchanged. A stale resident daemon is still rejected during compatibility handshake and recycled by the existing process lifecycle.

No Protocol, Memory, IPC or native Assistant contract changed.

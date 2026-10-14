# Assistant 0.1.43 / Python runtime 0.3.24 — Agent history continuity

The embedded Python companion now pins Agent CLI **0.87.12**
(`978eaee4ee0e152098095a1f33611d6fd4228e70`) with durable conversation resumption across interactive
restarts. The Python `uv.lock`, companion project metadata, CI checkout
pins, native Cargo workspace package version, and Cargo.lock are synced.

History data stays in the Agent workspace SQLite database; the companion
does not create a second conversation store. Native runtime/HUD APIs are
unchanged. Tests should cover the companion compatibility and Agent CI
at the exact immutable SHA before root release promotion.

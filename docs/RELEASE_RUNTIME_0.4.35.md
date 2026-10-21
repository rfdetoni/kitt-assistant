# Assistant Runtime 0.4.35 — SQLite index startup recovery

This release synchronizes the Assistant Python companion with Agent CLI **0.88.46**. The Python project dependency is `kitt-agent-cli>=0.88.46,<0.89`; `uv.lock` and both CI Agent checkouts pin merged Agent CLI `737ad5cefaaa4a04e8db22b910f3513194505f56`.

The fix prevents a second KITT instance from crashing with `sqlite3.OperationalError: database is locked` when the primary daemon is writing the same workspace repository index. The second process switches its index to an in-memory fallback after a bounded two-second SQLite busy wait. Disk index content and WAL are preserved. The fallback is not persistent; closing it does not sync the index back to disk.

The Rust daemon, HUD, protocol and Cargo lock remain unchanged. CI must validate Python pin consistency, core, lifecycle and OS-specific suites before merge.

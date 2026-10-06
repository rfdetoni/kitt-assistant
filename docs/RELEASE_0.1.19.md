# kitt-assistant 0.1.19

Publish Python runtime 0.2.33 with authoritative connection health, explicit ConnectionError on EOF (request outcome may be unknown), awaited reader cleanup and reconnect-ready resync state. RESYNC_REQUIRED stops live delivery immediately so replay cannot skip undelivered history; invalid frames also close the stream. Pair with Agent 0.83.16, Protocol 0.9.1 and Memory 0.9.2.

Validation uses Python 3.14, Node 24 and Rust checks where applicable. Cross-repository CI covers supported deployment environments.

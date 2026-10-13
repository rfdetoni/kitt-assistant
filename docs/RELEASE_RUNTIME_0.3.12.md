# Assistant Runtime 0.3.12 — Agent 0.86.15 integration

Synchronize the Python runtime dependency and CI checkouts with the Agent CLI v0.86.15 revision `78907e6568b77f88ec050ab6ba750980c76505c4`. The version and uv.lock are updated together, and both cross-platform CI checkout pins match the lock. This enables the Agent's bounded 16-minute HTTP wait for a live slow ChatGPT Web generation without modifying native Assistant services or Docker/Podman.

# K.I.T.T. Assistant 0.1.27 / runtime 0.2.40

## Stale resident daemon root cause

The daemon/client compatibility handshake previously called `importlib.metadata.version("kitt-agent-cli")` when each ping was handled. During an in-place ecosystem upgrade, a long-lived daemon could keep old Python modules loaded while the package metadata on disk was replaced. The old process could therefore advertise the new Agent version and pass compatibility checks even though its execution path was still old.

That failure explains why an upgraded TUI could continue sending `mode=auto` into a daemon that did not execute the current durable-contract wrapper.

## Correction

The daemon captures the Agent and Assistant-runtime distribution versions once when its process imports the daemon runtime. Ping returns those immutable startup identities. The client captures its own installed identities and requires both, plus the existing daemon protocol/lifecycle checks, to match.

An older daemon does not expose `assistant_runtime_version`, so the new client rejects it. The existing authenticated `start_daemon_detached` path already stops an incompatible daemon and launches the current runtime; no new process manager or compatibility layer is introduced.

## Compatibility

Python runtime is 0.2.40 and native Assistant metadata is 0.1.27. The immutable Agent lock and CI pin are Agent CLI 0.84.4 at `fc985bc6d842d3c684ad1188ff2fc8a0c427e6a8`. Protocol and Memory contracts are unchanged.

## Regression evidence

A focused client test presents a ping that claims the current Agent version but omits or supplies a stale Assistant runtime identity. Both are rejected; only the exact startup identity is accepted.

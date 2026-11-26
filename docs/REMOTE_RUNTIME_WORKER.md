# Dedicated remote runtime worker (opt-in)

This service is distinct from `kitt remote`, which is the paired session UI.
It accepts one operator-configured runtime ID and a dedicated scratch directory,
uses Docker or Podman (prefer rootless), and does not mount the Assistant's
normal workspace, credentials, daemon socket or control database. It requires
a locally available image pinned by digest with a working `sleep infinity`.

```sh
# Set KITT_REMOTE_RUNTIME_TOKEN to a unique high-entropy secret on both hosts.
python -m kitt.remote.runtime_worker \
  --state-dir /path/to/dedicated-worker \
  --runtime-id lab \
  --image alpine@sha256:<actual-local-image-digest>
```

The Agent REMOTE config uses endpoint `http://127.0.0.1:7348` for a local worker
or an SSH-tunneled loopback connection, runtime_id `lab`, and token_env
`KITT_REMOTE_RUNTIME_TOKEN`. For direct private-network access configure a
private IPv4 bind, a trusted TLS cert and key and HTTPS on the Agent. No public
or unauthenticated listener is supported. The image and resource caps are fixed
by the operator; requests cannot choose host paths, network access, or override
caps. Each worker has one runtime and one token, not a multi-tenant service.

The authenticated runtime endpoints match Agent provision/attach/execute/pause/
resume/health/snapshot/terminate. Container commands run with all Linux
capabilities dropped, no-new-privileges, read-only rootfs and no network. The
scratch directory and a 32 MiB tmpfs are writable; do not use a directory with
credentials. Containers are not a substitute for a VM when executing hostile
code. Physical engine, OS and filesystem behavior must be validated locally.

Blob transport uses authenticated PUT/GET/HEAD
`/v1/runtimes/lab/blobs/<sha256>` with raw bytes, 8 MiB per blob, 128 MiB/128
entries total. Send SHA-256 in the route. HEAD validates an existing blob so
the caller can skip retransmission; PUT verifies the complete digest before
atomic publication. The cache is mounted read-only at /mnt/kitt/cache. The
worker token grants access to all blobs for this dedicated runtime. No
automatic cross-tenant sharing, eviction, scheduler or remote metadata import.
Operator must clean cache after stopping worker/containers. Zstd framing in
Agent is a separate codec; this HTTP interface uses raw bytes.

A crash/timeout can retain a durable Agent operation fence. Do not retry
uncertain effects, or delete the state database while an operation may still
run. Inspect the worker container and recover locally before starting again.
Tests exercise HTTP authentication, the fixed policy and blob integrity with
mocked engine execution; actual Docker/Podman and TLS need lab validation.

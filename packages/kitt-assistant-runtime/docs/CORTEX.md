# KITT Cortex event plane (opt-in)

The authenticated Python daemon now exposes **durable event ingress, read-only
classification, minimal contextual state, and preflight**. It does not execute
commands, dispatch arbitrary tools, start a second Agent, or override approvals.

## Ownership

- `kitt-assistant-runtime/kitt/cortex`: bounded per-workspace SQLite WAL inbox.
- `kitt-agent-cli`: **only** authority for tools, budgets, planning and approvals.
- `kitt-memoryd`: durable semantic memory, unchanged by this feature.
- `nodexlm`: optional typed System One advisory, called via loopback.
- `kitt-reverse-proxy`: provider transport, not tool permissions.

The daemon protocol is authenticated before a `cortex.*` request is accepted.
The existing cross-workspace guard runs before any Cortex action. External event
payloads are **always** marked untrusted, including authenticated IPC submissions.
A source-provided event ID is idempotent; reused IDs with different content fail.

## API surface (authenticated Python daemon IPC)

Actions accepted by the existing message transport:

| Action | Inputs | Result |
| --- | --- | --- |
| `cortex.publish` | `topic`, `source`, `payload`, optional `event_id`, `severity`, `sensitivity`, `ttl_seconds` | `event` |
| `cortex.claim` | `consumer`, optional `limit`, `lease_seconds` | `deliveries` |
| `cortex.settle` | `consumer`, `event_id`, `lease_token`, `success` | `settled` |
| `cortex.status` | none | `cortex` |
| `cortex.context.update` | `field`, `value`, `ttl_seconds` | `context_update` |
| `cortex.context.get` | none | `context` |
| `cortex.preflight` | `event_id`, `effect`, `tool_domain` | `preflight` |
| `cortex.route` | `event_id` | `routing` |
| `cortex.prune` | none | `deleted` |

Examples of event topics: `saas.error.500`, `home.perimeter.motion`,
`communications.email.arrived`, `dev.pull_request.created`. Built-in
subscriptions route these to four first-party department IDs. Consumers claim
and settle leased deliveries. Retried deliveries are **at least once**, not
exactly once; Agent receipts are required for idempotent side effects.

The contextual projection is intentionally restricted to `location`,
`activity_state`, and `active_channel`. The Agent remains the authority
for task/cost budgets. Both events and contextual values have TTL.

## Optional local System One

Set trusted local environment variables on the **Python daemon**:

```sh
export KITT_CORTEX_DECISION_URL=http://127.0.0.1:8787
export KITT_CORTEX_DECISION_MODEL=ollaya/laya:multilingual
export NODEXLM_API_KEY=...  # only if NodexLM requires it
```

The advisor submits bounded event metadata to `POST /v1/systemone`,
never attachments, full payload, tool results, or credentials. It has a
loopback-only URL and a finite timeout. A missing model, timeout, invalid reply,
or offline NodexLM does not escalate to cloud. Private/secret/personally
sensitive event routes remain local-only. A decision is never an authorization.

## Security and operational boundaries

- Do not expose daemon authentication tokens to webhook senders or plugins.
  An ingress adapter must authenticate external callbacks separately and
  publish through the existing trusted local IPC transport.
- Delivery can be acknowledged only with the current lease token.
- The `GREEN/YELLOW/RED` result is a **preflight signal**, not a grant.
  Execute all real tools only through the Agent policy, approvals and sandbox.
- There is no automatic mutation/door unlock/financial action on event ingestion.
- Persistence uses per-workspace SQLite under `~/.kitt/cortex` with 0700
  directory and 0600 database on POSIX.
- Limits: 16 KiB/event, 10k pending deliveries, 50 claims/batch, 5 attempts,
  up to 300-second leases, 30-second to one-day TTL.
- Run `cortex.prune` periodically from the trusted daemon maintenance task;
  this first implementation does not claim to provide production HA.

## Validation

```sh
cd packages/kitt-assistant-runtime
python -m unittest discover -s tests -p 'test_cortex.py' -v
```

Integration testing must include two authenticated clients, invalid token,
cross-workspace rejection, daemon restart, expired leases, NodexLM disconnected,
and an untrusted cross-domain action. Sensor adapters (MQTT/RTSP/mail), new
plugin hot-loading, UI cards, and automatic task dispatch are deliberately
**not yet installed** by this stage.

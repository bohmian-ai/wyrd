# Deployment targets for direct and queued verification

Status: approved by the maintainer 2026-11-15 as spec revision 13 (REQ-015).

## Problem

Revision 12 protected direct verification with an in-process queued
reservation: at boot, `QueuedShare::reserve` charged a fixed share of the
process memory (25%) and scratch (50%) ledger to queued work, which held it
whenever nothing was queued. No borrowing was implemented. The maintainer
rejected the reservation because it repeats the static-share failure recorded
in `changes/active/verifier-code-mode/design/capacity-diagnosis.md`. On
test-sized plans it also could not hold one Python session, so hosted test
servers failed to boot.

A read-only review found that guaranteeing direct is unaffected by queued work
that already holds memory, scratch, CPU or provider quota needs independent
capacity. Spare capacity inside one process is either held idle or reclaimed
with no finite bound. Comparable systems get isolation by running background
work on separate processes from one binary, with an all-in-one default:
GitLab (Puma and Sidekiq), Sentry (`web` and `taskworker`), Temporal
(`frontend`, `history`, `matching`, `worker`), Loki and Mimir (`all` plus
per-component targets), and Airflow (`standalone` plus per-component roles).

## Decision

One closed `WYRD_TARGET` set. The old values `all`, `server`, `oracle`,
`scribe` and `forge-worker` are removed without aliases.

| Target | Public serving | Direct `verify` | Queued enqueue / webhook | Queued claim | Bifrost roles |
|---|---|---:|---:|---:|---|
| `combined` (default) | all routes | yes | yes | yes | Scribe, Oracle, Forge coordinator and worker |
| `direct` | direct and shared control routes | yes | no | no | none |
| `worker` | queued ingress, status and control | no | yes | yes | none |
| `bifrost-ingest` | Bifrost ingest | no | no | no | Scribe |
| `bifrost-query` | Bifrost query | no | no | no | Oracle |
| `bifrost-maintenance` | none | no | no | no | Forge coordinator and worker |

- **`combined`**: direct and queued work share one hosted capacity owner,
  charged to the one process ledger only while in use. Nothing is reserved.
  `WYRD_VERIFICATION_QUEUED_EXECUTIONS` caps queued concurrency and holds
  nothing idle. Colocation does not guarantee direct noninterference.
- **`direct`**: executes only synchronous `verify`. It refuses enqueue,
  webhooks, deferred inputs, approvals and queued-only controls, and never
  claims queued executions.
- **`worker`**: admits and executes queued workflows, including GitHub webhook
  ingress with its settlement-before-2xx contract, and refuses synchronous
  `verify`.
- **Refusals**: wrong-target work is refused before admission or any side
  effect, identically over HTTP and MCP. Nothing is forwarded.
- **Readiness** follows the target's capabilities. Public serving, Verifier
  capabilities and Bifrost roles are decoupled, so a target with no Bifrost
  role boots and stages outbox writes to a peer Scribe.
- **Unchanged**: original deadlines, Busy deferral, fenced takeover across pods
  and graceful drain.

Split deployments route `verify` to `direct` pods and queued routes and
webhooks to `worker` pods at the load balancer.

## Removed

`QueuedShare`, `HostedLimits.queued_share`, the
`WYRD_VERIFICATION_QUEUED_{CPU,MEMORY,SCRATCH}_PERCENT` settings, the
reservation-only tests and the duplicated direct/queued pools and lanes in
`combined`. `ProcessResources::reserve` goes too, unless another production
owner uses it.

## Accepted limits

- Separate pods still share provider quota, Postgres and Bifrost, and possibly
  host CPU and storage. Operators provision independent budgets.
- `combined` permits contention between direct and queued work. The queued
  concurrency cap does not reserve direct headroom.

## Proof

Real SDK, HTTP and MCP journeys show:
- every target accepts exactly its permitted work, and refused work creates no
  execution or admission side effect;
- an idle `combined` queued runtime holds no reservation;
- a `worker` pod completes webhook, deferred-input and approval journeys
  through a remote Scribe;
- a `worker` crash or shutdown allows takeover without resetting deadlines;
- a `direct` pod never claims queued executions.

Direct c64 on a `direct` pod is compared with and without saturated `worker`
pods.

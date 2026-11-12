# Gateway governance latency

Status: exploring

## Problem

Quota-covered Gateway calls currently await PostgreSQL admission and buffered
settlement. The maintainer wants approximate cached enforcement so database
coordination does not delay provider dispatch or answer delivery. This is
separate from the existing no-quota Verification benchmark's Agent latency.

## Desired outcome

Admit/reserve from each server's cached shared usage plus pending local use,
return the provider answer, and batch PostgreSQL usage updates/refresh in the
background. Follow existing Gateway owners and ordinary cache-first/background
accounting patterns. Keep the implementation small.

## Constraints and non-goals

- Approximate enforcement is accepted for request, token, concurrency and
  monetary budgets. No strict global ceiling or numerical overshoot guarantee.
- Pending usage can remain in memory and may be lost on abrupt process death
  or an expired shutdown deadline. No local WAL or durable queue.
- Missing usable state, unhealthy refresh or full accounting buffers refuse
  with the existing retryable 503 until background recovery succeeds.
- Keep tenant isolation, permissions, credential handling and capture admission.
- No Redis, replica coordination, quota leases, separate service or new public API.
- TASK-006 needs no TDD cycle; focused regression/integration proof remains.

## Proposed implementation

Extend the existing process Gateway cache with local usage/reservations. Charge
local headroom atomically before dispatch and release concurrency at execution
completion, including stream/cancellation completion. Reuse the existing
background lifecycle to batch persisted usage and refresh shared totals. Keep
pending local contributions across refresh and avoid double-counting flushed
updates/retries. Refuse new work before bounded pending capacity is exceeded.

Reuse existing cache/listener health and generation fencing. On a cold/missing
snapshot, load in background and refuse until usable; do not fall back to a
request-path PostgreSQL read. Restart loads committed usage and accepts that
another process's unflushed memory usage cannot be recovered.

Batching, cadence, bounded sizes and private schema-preparation ownership are
implementation choices. Do not build strict distributed accounting to solve
an unrequested guarantee. Existing SQL migration/drain conventions suffice;
no separate rollout or version-coordination protocol is requested.

## Evidence and rationale

[Research](../../benchmark-latency-diagnosis/research/agent-check-and-gateway-governance.md)
records the current ledger/callback/cache paths, Bifrost reuse precedent,
schema compilation sites and saved Gateway timing windows. Gateway already
caches configuration, but its usage/reservation ledger remains synchronous.

LiteLLM's official request-flow documentation describes cached checks and
post-response asynchronous usage updates; its production guide recommends
batched spend writes. This supports the simple pattern, not copying its Redis
infrastructure or synchronous cache-miss database fallback.

## Decisions and alternatives

On 2026-11-11 the maintainer approved the spec revision, explicitly accepted
loss of unflushed usage after crash, chose refusal until background recovery
for missing/unhealthy/full state, waived TDD, and reiterated that approximate
enforcement and the smallest sufficient implementation are required.

Strict replica-wide reservations and crash-safe local persistence are excluded.
Moving only settlement after answering is insufficient because admission would
still await PostgreSQL.

## Standards conflicts

[Latency spec revision 7](../../benchmark-latency-diagnosis/spec.md) explicitly
supersedes the Gateway-capacity revision-4 before-answer quota-settlement
constraint. The owning Gateway quota section of `architecture/wyrd-design.md`
records the approximate contract. Other security/capture rules remain.

## Open questions

No material product decision remains for this scope. Implementation owns
private batching/cadence, bounded capacities and the smallest reuse mechanics.

## Handoff

[TASK-006](../../benchmark-latency-diagnosis/tasks/TASK-006-gateway-local-governance-and-agent-check-latency.md)
uses revision 7, with regression/integration/performance proof and no TDD
requirement. Spec approval and task planning do not execute implementation,
experiments or Git actions.

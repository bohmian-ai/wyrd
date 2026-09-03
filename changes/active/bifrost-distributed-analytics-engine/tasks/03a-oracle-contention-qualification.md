---
id: BIFROST-R4-T03A-CONTENTION-QUALIFICATION
title: Qualify Oracle admission under lowest-rung multi-tenant contention
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T03-R01-TERMINAL-LIFECYCLE-EVIDENCE]
requirements: [REQ-005, REQ-006, REQ-007, REQ-011]
invariants: [INV-003, INV-004, INV-005, INV-006, INV-008]
acceptance: [AC-004, AC-007, AC-008]
parent_task: BIFROST-R3-T2-PRODUCTION-ACTIVATION
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Oracle contention qualification

## Outcome and value

Before public Analytical routing is activated, one production-shaped journey
proves that the existing Oracle admission owner remains safe and useful on the
lowest supported Oracle node. One real memory-heavy Analytical query runs while
bounded Interactive queries from two tenants make progress; memory stays below
the shared root, the Analytical query spills instead of exhausting the process,
and every owner returns to baseline.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- **Retain:** `OracleAdmission`'s one mutex-owned class/tenant queue, existing
  per-tenant FIFO and round-robin grant order, `OracleResources` aggregate root,
  query-local grants, protected Interactive capacity, Task 3's qualified
  join/group/spill query, `WyrdTestCluster::role_separated`, production
  telemetry, and stream-owned cleanup.
- **Delete from Task 4:** ownership of the admission algorithm and its focused
  saturation test. Task 4 consumes this task and proves only that public path
  selection uses the qualified owner.
- **Unfinished:** combined evidence that the retained admission and memory model
  preserves Interactive progress for two tenants while real Analytical memory,
  exchange, and spill ownership are live at the exact Oracle memory floor.

## Owners, scope, consumers, and non-goals

`oracle/admission.rs` remains the sole class/tenant admission owner.
`resources.rs` remains the sole aggregate-memory, query-grant, and scratch
owner. `oracle/query_stream.rs` and Task 2's supervisor remain the settlement
owners. `wyrd-testing/tests/bifrost/oracle/capacity.rs` owns the integrated
journey, using existing cluster, client, inspection, and telemetry support.
Task 4 consumes the qualified admission behavior before enabling public
Analytical selection.

Do not add an allocator, scheduler, adaptive grant, per-tenant memory pool,
calibration service, configuration key, retry, benchmark framework, or second
telemetry owner. Do not change the 32 MiB slot charge or the 32–256 MiB grant
range in this task.

## Ordered implementation scenarios

### Scenario 1 — Focused tenant rotation and Interactive protection

**Behavior.** Under one atomic admission owner, Analytical saturation cannot
consume Interactive capacity; FIFO is preserved within each tenant and the
tenant cursor grants a waiting peer tenant before returning to the first
tenant. Rejection changes no class, tenant, memory, spill, or root ownership.
Maps REQ-005, REQ-006, INV-005, INV-006, AC-004.

**RED.** Add
`oracle::admission::tests::analytical_saturation_preserves_interactive_floor_and_rotates_tenants`.
Construct the existing owner with two Interactive slots, one Analytical slot,
single-tenant ceiling two, multi-tenant ceiling one, and the existing finite
queue wait. Hold the Analytical permit for tenant A; enqueue Interactive A1,
A2, and B1 in that order. Assert A1 and B1 are granted while Analytical remains
held, A2 remains queued until one peer tenant releases, A1 precedes A2, and all
class/tenant/root counters return exactly to their initial snapshot. Fill the
finite queue once and assert the rejected request mutates no snapshot. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::admission::tests::analytical_saturation_preserves_interactive_floor_and_rotates_tenants)'
```

**GREEN.** Reuse `OracleAdmissionConfig`, `ClassState`, `TenantQueue`,
`enqueue_waiter`, `grant_waiters`, and the existing `OracleResources` grant.
If the retained implementation does not satisfy the test, correct grant order
inside `grant_waiters` while its existing admission mutex owns the complete
transition. Interactive capacity remains independently grantable while the
Analytical class is full; the aggregate resource acquisition and counter update
remain one checked grant with existing rollback on failure.

**REFACTOR.** Keep one queue owner and one root grant. Do not add a fairness
trait, path scheduler, background worker, or second counter ledger.

### Scenario 2 — Lowest-rung production contention journey

**Behavior.** A real memory-heavy inactive Analytical query holds production
admission, runtime, exchange, and spill ownership on the exact supported Oracle
memory floor. While that stream is intentionally left open, bounded public
Interactive queries from two tenants both complete. The Analytical query then
drains successfully with real spill evidence, aggregate memory never exceeds
the root, and every query and graph owner is released. Maps REQ-005, REQ-006,
REQ-007, REQ-011, INV-003–INV-006, AC-004, AC-007, AC-008 and the admission-
pressure portions of Journeys A and D.

**RED.** Implement
`capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`
in the existing Oracle journey target. Start
`BifrostClusterSpec::role_separated()` and apply this exact injected observation
to both Oracle-only nodes:

- `memory_limit_bytes = 512 * 1024 * 1024`, the supported Oracle-only floor;
- `effective_cpu = 2`;
- `scratch_capacity_bytes = scratch_available_bytes = 1024 * 1024 * 1024`;
- injected memory and CPU sources.

Create tenant A from the fixture and tenant B through
`WyrdTestCluster::add_tenant`; use the existing tenant client helper and seed one
bounded Interactive table for each. Reuse Task 3's admitted join/group/output-
sort workload for tenant A. Open its inactive Analytical stream through the
existing Oracle path and consume only until production inspection reports the
Analytical query, graph, query grant, and nonzero physical memory as live; leave
the stream open without adding a sleep or production pause hook.

While that ownership is live, issue one bounded public Interactive query for
tenant A and one for tenant B against the same coordinator. Both must return
their exact rows and successful Interactive behavior before the Analytical
stream is resumed. Drain the Analytical stream and assert Task 3's exact result
digest and operator-attributed positive `spill_count`, `spilled_bytes`, and
`spilled_rows`. Across production inspection and telemetry assert:

- the issued query grant is derived from the injected 512 MiB observation;
- shared-root current and peak memory never exceed its managed-memory limit;
- neither server exits, loses readiness, panics, or reports an OOM;
- admission wait/queue metrics use bounded labels and both tenant requests make
  progress without tenant IDs becoming metric labels; and
- active queries, queues, grants, graphs, attempts, leases, tasks, caches,
  exchanges, scratch, spill files, slots, connections, and current gauges return
  to their pre-query baseline after the terminal.

Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants)' --run-ignored=all"
```

The test initially fails because `capacity.rs` has no journey and the current
test support does not return an open inactive Analytical stream together with
its production grant/root telemetry.

**GREEN.** Move the smallest reusable Task 3 query/expected-result helpers into
the existing Oracle test support module. Add one test-support method that opens
the existing inactive Analytical `OracleQueryStream` without collecting it;
the stream remains the production cancellation, terminal, and cleanup owner.
Project the already-owned query grant and aggregate current/peak observations
through existing inspection/telemetry types. Make no production behavior change
unless Scenario 1 or this journey exposes a violation; any correction stays in
`OracleAdmission` or `OracleResources` and preserves the approved fixed grant,
root, floor, queue, and settlement model.

**REFACTOR.** Test support may observe and temporarily stop polling the real
stream. It may not synthesize memory, spill, fairness, refusal, terminal, or
cleanup evidence and may not become a lifecycle owner.

## Cross-scenario decisions and authority

This task qualifies the current fixed policy; it does not tune it. The 512 MiB
snapshot is the exact Oracle-only startup floor already enforced by
`BifrostRuntimeResources`. The focused test proves deterministic queue order;
the journey proves user-visible progress and bounded physical ownership. Task 4
must consume these results rather than rebuilding admission or repeating the
load matrix.

Authority: `AGENTS.md`, approved spec REQ-005/006/007/011, Journeys A and D,
AC-004/007/008, `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`,
`architecture/references/domain/olap-serving.md`,
`architecture/references/domain/datafusion.md`, and
`architecture/references/domain/analytical-operations-reliability.md`.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run check:bifrost-resource-governance
git diff --check
```

## Completion evidence

- Focused snapshots proving tenant rotation, per-tenant FIFO, Interactive
  protection, rejection rollback, and exact zero release.
- One fixed 512 MiB two-Oracle/one-Scribe run with a live memory-heavy
  Analytical stream and successful concurrent Interactive queries from two
  tenants.
- Query grant, aggregate current/peak, spill, queue/wait, readiness, terminal,
  and zero-owner production evidence.
- No new allocator, scheduler, configuration surface, telemetry owner, or
  tuning claim.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if passing requires changing tenant fairness,
the Interactive floor, public query semantics, fixed grant ownership, terminal
behavior, or the supported minimum topology. Return `PLAN_BLOCKED` if Task 3's
real spilling stream cannot remain open while existing production inspection
observes its grant and root memory; report the missing seam rather than adding
a second execution or resource owner.

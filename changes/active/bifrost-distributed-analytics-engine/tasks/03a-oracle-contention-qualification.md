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
each query's issued ceiling, root admission accounting stays within its managed
budget, the Analytical query spills, and every owner returns to baseline.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- **Retain:** `OracleAdmission`'s one mutex-owned class/tenant queue, existing
  per-tenant FIFO and round-robin grant order, `OracleResources` aggregate root,
  query-local grants, protected Interactive capacity, Task 3's qualified
  join/group/spill query, `WyrdTestCluster`'s concrete descriptor path, production
  telemetry, and stream-owned cleanup.
- **Delete from Task 4:** ownership of the admission algorithm and its focused
  saturation test. Task 4 consumes this task and proves only that public path
  selection uses the qualified owner.
- **Unfinished:** combined evidence that the retained admission and memory model
  preserves Interactive progress for two tenants while real Analytical memory,
  exchange, and spill ownership are live at the exact Oracle memory floor, using
  the three Oracle participants required for Task 3's physical distributed cut.

## Owners, scope, consumers, and non-goals

`oracle/admission.rs` remains the sole class/tenant admission owner.
`resources.rs` remains the sole root-admission, query-pool, grant, and scratch
owner. `oracle/query_stream.rs` and Task 2's supervisor remain the settlement
owners. `wyrd-testing/tests/bifrost/oracle/capacity.rs` owns the integrated
journey, using existing cluster, client, inspection, and telemetry support.
Task 4 consumes the qualified admission behavior before enabling public
Analytical selection.

Do not add an allocator, scheduler, adaptive grant, per-tenant memory pool,
calibration service, configuration key, retry, benchmark framework, or second
telemetry owner. Do not change the 32 MiB slot charge or the 32–256 MiB grant
range in this task. Do not add `QueryExecutionPath` or an execution-path field
to `QueryTerminalFrame`; Task 4 owns that public contract change.

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
drains successfully with real spill evidence, each query pool stays within its
issued grant, root admission accounting stays within its managed budget, and
every query and graph owner is released. Maps REQ-005, REQ-006, REQ-007,
REQ-011, INV-003–INV-006, AC-004, AC-007, AC-008 and the admission-pressure
portions of Journeys A and D.

**RED.** Implement
`capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`
in the existing Oracle journey target. Add and start the concrete
`BifrostClusterSpec::three_oracles_one_scribe()` descriptor through
`WyrdTestCluster::start_spec`: nodes 1, 2, and 3 are Oracle-only and node 4 is
Scribe-only. The coordinator is one Oracle and the other two Oracles are Task
3's minimum non-coordinator followers. Apply this exact injected observation to
all three Oracle-only nodes:

- `memory_limit_bytes = 512 * 1024 * 1024`, the supported Oracle-only floor;
- `effective_cpu = 2`;
- `scratch_capacity_bytes = scratch_available_bytes = 1024 * 1024 * 1024`;
- injected memory and CPU sources.

Create tenant A from the fixture and tenant B through
`WyrdTestCluster::add_tenant`; use the existing tenant client helper and seed one
bounded Interactive table for each. Reuse Task 3's admitted join/group/output-
sort workload for tenant A. Open its inactive Analytical stream through the
existing Oracle path and consume only until production inspection reports the
Analytical query, graph, issued grant, and nonzero query-pool reservation as
live; leave the stream open without adding a sleep or production pause hook.

While that ownership is live, arm a non-stalling one-shot resource-probe capture
on the coordinator and take a telemetry checkpoint. Open tenant A's bounded
query through its real public Rust client without draining it, await the
captured probe, and sample production telemetry while the client still owns the
stream. Assert `oracle_queries_active{class="interactive"}` increased from the
checkpoint by exactly one, then drain the exact rows and a
`QueryTerminalOutcome::Success` terminal. Across the completed window, sum the
bounded `reason` series for
`oracle_admission_total{class="interactive",outcome="admitted"}` and assert a
delta of exactly one; the corresponding `class="analytical"` admitted-series
sum remains zero. Retain the probe and repeat the same
checkpoint, arm, public open, active sample, successful drain, and capture
sequence for tenant B. The controller has one next-query slot, so the two
captures are intentionally serial while the Analytical stream remains live.
Drain the Analytical stream and assert Task 3's exact result digest and
operator-attributed positive `spill_count`, `spilled_bytes`, and
`spilled_rows`. Across the direct Analytical probe, the two captured Interactive
probes, production inspection, and telemetry assert:

- the issued query grant is derived from the injected 512 MiB observation;
- for the Analytical query and each Interactive query, query-pool current and
  peak reservations satisfy `current <= peak <= issued_grant`, each completed
  Interactive probe reports `current == 0`, and each observed peak is positive;
- on every sampled Oracle root,
  `scribe_memory_used_bytes + oracle_memory_used_bytes + forge_memory_used_bytes
  <= plan.managed_memory_bytes`;
- every server remains live and ready throughout the workload;
- admission wait/queue metrics use bounded labels and both tenant requests make
  progress without tenant IDs becoming metric labels; and
- active queries, queues, grants, graphs, attempts, leases, tasks, caches,
  exchanges, scratch, spill files, slots, connections, and current gauges return
  to their pre-query baseline after the terminal.

Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants)' --run-ignored=all"
```

The test initially fails because `capacity.rs` has no journey, the named
three-Oracle/one-Scribe descriptor does not exist, the stream resource probe
does not yet expose its issued grant and query-pool current/peak observations,
and the existing public-route probe binding is available only through the
cancelling schema-stall fault.

**GREEN.** Add only the named four-node factory beside the existing
`BifrostClusterSpec` factories; do not add a topology enum variant or change
`role_separated()`. Move the smallest reusable Task 3 query/expected-result
helpers into the existing Oracle test support module. Reuse
`QueryResourceProbe`/`QueryResourceSnapshot`: extend that query-local probe with
the issued `granted_memory_bytes`, current `MemoryPool::reserved()`, and peak.
`OracleQueryResources` retains one query-local `Arc<AtomicUsize>` beside its
pool; `PeakTrackingMemoryPool` updates that counter after successful growth;
`RetainedQuerySessionShape` transfers the pool and counter into `LocalPermit`;
and `attach_resource_probe` gives both to the probe, whose snapshot reads
current from the pool and peak from the counter.

Retain direct `resource_probe_for_test()` access only for the inactive
Analytical stream. For each Interactive query, extend the existing test-only
`QueryStreamFaultController` and `/v1/query` route binding with one
`CaptureProbe` mode and one notification-backed `QueryStreamProbeCapture`.
Arming stores exactly one capture beside the controller's existing atomic
next-query fault. Route claim binds `result.resource_probe_for_test()` to that
capture and notifies its waiter before constructing the response body; unlike
`StallAfterSchema`, `CaptureProbe` does not branch in the body loop, truncate,
stall, call `OracleQueryStream::cancel`, or change terminal behavior.
`QueryStreamFaultController::capture_next_probe()` returns the newly armed
`Arc<QueryStreamProbeCapture>` directly; the capture's notification-safe
`wait_resource_probe()` returns the bound probe, and
`WyrdTestServer::capture_next_query_resource_probe()` only delegates that arm
operation. Do not add another retained harness slot. The journey holds each
returned capture, drains the corresponding public client stream, and then
awaits its probe under the existing server drain deadline. It repeats this
one-shot separately for tenants A and B, so both query bodies still drain
exclusively through the real public client.

Do not use `query_stream_stall`, `wait_query_schema_stall`, or
`query_resource_probe(query_id)` for the successful Interactive cases; those
are cancellation evidence. Do not add a process-global query registry or a
second telemetry owner. Project each Oracle node's existing `ResourceSnapshot`
through the cluster inspection used by the journey. Make no production behavior
change unless Scenario 1 or this journey exposes a violation; any correction
stays in `OracleAdmission` or `OracleResources` and preserves the approved fixed
grant, root, floor, queue, and settlement model.

**REFACTOR.** Test support may stop polling only the inactive Analytical stream.
Interactive capture must remain passive and one-shot: it observes the probe but
never owns, stalls, cancels, truncates, or drains the public stream. Test support
may not synthesize memory, spill, fairness, refusal, terminal, or cleanup
evidence and may not become a lifecycle owner.

## Cross-scenario decisions and authority

This task qualifies the current fixed policy; it does not tune it. The 512 MiB
snapshot is the exact Oracle-only startup floor already enforced by
`BifrostRuntimeResources`. Three Oracle-only nodes are required because the
coordinator does not participate in its worker set and the retained Task 3
physical exchange needs two followers. The focused test proves deterministic
queue order; the journey proves user-visible progress, per-query pool bounds,
root admission containment, and real spill. The injected observation is not an
OS memory limit, so this task makes no aggregate physical-memory or no-OOM
claim. Task 4 must consume these results rather than rebuilding admission or
repeating the load matrix.

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
- One fixed 512 MiB three-Oracle/one-Scribe run with a live memory-heavy
  Analytical stream, successful public-client terminals from two tenants while
  that stream remains live, and class-labelled admission/active telemetry
  proving both queries were Interactive.
- Per-query grant/current/peak, root admission accounting, spill, queue/wait,
  readiness, terminal, and zero-owner production evidence.
- No new allocator, scheduler, configuration surface, telemetry owner, or
  tuning claim.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if passing requires changing tenant fairness,
the Interactive floor, public query semantics, fixed grant ownership, terminal
behavior, or the supported minimum topology. Return `PLAN_BLOCKED` if Task 3's
real spilling stream cannot remain open while existing production inspection
observes its grant and root memory; report the missing seam rather than adding
a second execution or resource owner.

## Execution evidence

### Scenario 1 — `analytical_saturation_preserves_interactive_floor_and_rotates_tenants`

- **RED.** The named test did not exist; the module did not compile against it.
  Once written, it passed on its first run against the retained admission
  owner, so the task's conditional GREEN ("if the retained implementation does
  not satisfy the test, correct grant order") required no production change.
  This scenario is verified regression coverage of behavior already correct.
- **GREEN.** `PASS [0.018s]` via the task's exact command.
- **REFACTOR.** The queue-full half was extracted into
  `prove_rejection_charges_nothing`, and the waiter bindings renamed
  (`tenant_a_head` / `tenant_a_tail` / `peer_head`), to satisfy
  `clippy::too_many_lines` and `clippy::similar_names` without weakening an
  assertion. Both halves still run against one shared `OracleResources` root.
- Production `enqueue_waiter` runs a grant pass per arrival, so the three
  Interactive waiters are pushed through the module's existing `push_waiter`
  fixture and settled by one `drain_grants` pass. That is the only way all
  three contend in a single pass, which is what the stated assertions describe.

### Scenario 2 — `capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`

- **RED.** `capacity.rs` held no journey, `three_oracles_one_scribe()` did not
  exist, the stream resource probe exposed neither its issued grant nor its
  query-pool current/peak, and the only probe binding on the public route was
  the cancelling schema stall. Those four seams were built first (passive
  `QueryStreamFault::CaptureProbe`, `QueryStreamProbeCapture`,
  `QueryResourceSnapshot` grant/pool fields, `oracle_resource_snapshots`).
- **GREEN.** `PASS [50.794s]` via the task's exact command, and again in the
  full lane.
- **Diagnosis, in order, each a fixture defect rather than a production one:**
  1. The expected Analytical grant was sampled by acquiring and dropping an
     envelope *while the Analytical query held one*, which the root correctly
     refuses at 512 MiB. Moved to the pre-query baseline, where the envelope is
     genuinely free.
  2. `ORDER BY` with no fetch is a "complex" plan, so the planner classified
     the bounded read Analytical; it then queued behind the live Analytical
     query and rejected on `queue_deadline`. `ORDER BY ... LIMIT` then failed
     as `unsupported distributed Oracle operator: SortExec(TopK)`. The
     Interactive read is now a flat projection, which is what actually
     exercises the Interactive floor.
  3. A 64-row Interactive result fits in the transport's buffers, so the server
     finished and released the query before the window sampled it
     (`oracle_queries_active{class="interactive"}` read 0 → 0). The bounded
     table is now 200k rows, so the undrained client is what holds the query
     open. This is TCP/stream backpressure, not a sleep.
  4. `support::live_ownership` demanded an Oracle from every node, which a
     Scribe-only node correctly does not compose. It now skips nodes with no
     Oracle; every existing caller's cluster is all-Oracle, so their behavior
     is unchanged.
  5. On the single-threaded `#[tokio::test]` default, four in-process pods plus
     a distributed 700k-row join starved the pods' own heartbeats, their Oracle
     role leases expired, and the graph failed with a partial result. The
     journey now runs `flavor = "multi_thread", worker_threads = 8`, matching
     the existing `scribe::horizontal_ingest` journeys.
- **Limitation.** The 512 MiB / 2 CPU / 1 GiB scratch figures are an injected
  observation, so this run bounds what admission *accounts for* on the lowest
  rung. It makes no claim about aggregate physical process memory.

### Broader verification

| Command | Result |
| --- | --- |
| `mise run fmt` | pass |
| `mise run lints` | pass |
| `mise run test:bifrost` | 974 passed, 0 skipped |
| `mise run test:bifrost:journey:oracle` | 17 passed, 0 skipped |
| `mise run check:bifrost-resource-governance` | pass |
| `git diff --check` | clean |

No command in the task was stale; every one ran verbatim.

### Review remediation (FIND-03a-1, FIND-03a-2)

Both findings were assertion gaps in the Scenario 2 journey. Neither required a
production change, and no new hook, abstraction, or fixture was added.

- **FIND-03a-1 — contention was proven once, not per window.** Analytical
  ownership was sampled only before the Interactive loop, so a stream that
  drained afterward would leave both windows uncontended while every assertion
  still passed. `prove_interactive_window` now takes the Analytical query's
  probe and re-reads `live_analytical_ownership` in the same window as the
  Interactive gauge sample, so each tenant's admission is observed while the
  Analytical query still holds admission, a live graph, its grant, and nonzero
  pool memory.
- **FIND-03a-2 — settled release was asserted for Analytical only.**
  `prove_pool_within_grant` checked `current <= peak <= grant`, which a
  nonzero settled current can satisfy. It now rejects any nonzero settled
  `pool_current_bytes`, and the Analytical-only duplicate check was deleted.
  All three probes are held to the same claim.

| Command | Result |
| --- | --- |
| Scenario 2 exact command | `PASS [43.543s]` |
| `mise run fmt` | pass |
| `mise run lints` | pass |
| `mise run test:bifrost` | 974 passed, 0 skipped |
| `mise run test:bifrost:journey:oracle` | 17 passed, 0 skipped |
| `mise run check:bifrost-resource-governance` | pass |
| `git diff --check` | clean |

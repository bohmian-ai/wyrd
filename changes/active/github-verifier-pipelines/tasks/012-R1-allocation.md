---
id: TASK-012-R1
status: proposed
approved_spec_revision: 16
requirements: [REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024]
acceptance: [AC-015, AC-016, AC-017, AC-018]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-012]
---

# Replace speculative allocation with shared workload management

A small verification must not reserve a maximum worker/frame envelope or six
possible media copies. Gateway, verification and Bifrost must instead coordinate
their live demand, let idle capacity be borrowed, and preserve progress and
tenant authority when the process becomes contended.

The maintainer agreed the design and requested this task before TASK-013 on
2026-11-16. This is a new behavioral amendment, not another execution of the old
TASK-012-R1 review contract. The reviewed task number is intentionally retained
with the requested `allocation` filename. The maintainer explicitly approved
[spec revision 16](../spec-revisions/16-shared-workload-management.md) on
2026-11-16. That revision authorizes the changed policy; historical review
evidence does not prove this task's implementation.

## Input, selected approach and output

TASK-012 supplies the composed code-mode/dynamic-pipeline implementation,
existing shared ledger, hosted Monty/media paths, gateway integration and
functional evidence. Reuse those actual paths; do not port from an older branch.
Follow the finalized [workload design](../design/shared-workload-management.md),
especially **Memory budget, overhead and growth**, **Aggregate pressure
observation**, **Fair shares and contention**, **CPU and execution admission**
and **Per-area allocation and local limits**.

Extend `wyrd-runtime::ProcessResources` with one shared workload manager. The
existing byte ledger handles retained custody but has no cross-engine pressure
or scheduling policy; `HostedAdmission` only orders hosted envelope grants;
gateway quotas constrain calls, not combined memory/CPU. Extending the shared
owner connects these workflows without duplicate allocation authorities.
Keep Bifrost's complete memory/spill/workspace workflow and adapt only its
existing root/CPU execution boundaries to shared pressure/admission.

The output is a functionally verified, reviewed allocation implementation and
full clean L1 qualification evidence. TASK-013 consumes that exact implementation
for its 100-tenant claim/drain regression and canonical mixed L2 baseline; no
TASK-013 benchmark is executed against an intermediate allocation implementation.
TASK-014 retains later throughput optimization. Do not invent another benchmark
command, rewrite benchmark criteria, or fold TASK-013's comparison into this task.

## Owners and connected flow

- Server boot/configuration composes the existing process resource plan and
  passes the same manager to gateway, verification and Bifrost. Reuse P, H, C,
  scratch and existing local ceilings; no duplicate settings or aliases.
- `wyrd-runtime/src/resources.rs` owns shared custody/pressure and scheduling;
  preserve RAII transfer and scratch accounting. Add only policy necessary for
  the selected sharing and CPU admission; no replacement allocator or broker.
- Server `main.rs` owns mimalloc. Use its installed aggregate observation API
  at this binary boundary; any necessary narrow feature/dependency bridge stays
  here, never in SDK/client libraries. No parent per-allocation hooks or expensive
  detailed allocation statistics. Parent-plus-child commitment and ledger usage
  are overlapping observations, not additive usage.
- Server `verification/hosted/{sandbox,worker,admission,blocks,evidence}.rs`
  supplies actual worker/parent-buffer custody, coalesced 100 ms telemetry and
  500 ms staleness handling. Remove the maximum session/frame envelope, retain
  local heap/time limits and real idle-worker consumption. Serialize telemetry
  through the worker writer; preserve Monty's framed protocol and no allocator
  IPC. Crates.io Monty remains the selected dependency; if its current adapter
  cannot carry the selected telemetry, report the exact seam rather than
  silently forking/upgrading it or inventing a reservation fallback.
- `vala-verify/src/blocks/agent.rs`, hosted evidence and server
  `verification/{state,results,runner}.rs` remove six-copy media and future
  checkpoint/result precharging and sizing-only traversal. Retain dominant
  materialized owners and background checkpoint/outbox handoff. Do not replace
  direct recording with resumable state or account JSON length as exact heap.
- Gateway owner/invocation, body buffering, provider transport and stream/capture
  owners carry initiating tenant/workload identity and substantial retained-byte
  custody. Keep commercial/provider quotas, authorization and active stream
  lifetime. Network/provider waits hold no CPU lane. Verification-originated
  nested calls must not create a second independent root debit.
- Bifrost `resources.rs`, Scribe `execution_lanes.rs`, Oracle
  `telemetry.rs::QuerySpanJoinSetTracer` and actual Oracle/Forge execution
  integrate the manager. The pinned DataFusion fork's `JoinSet`/`SpawnedTask`
  wrapping seam is available; extend current span propagation for context and
  CPU admission, including entry and spawned parallel work. Do not treat one
  output-stream/query-lifetime permit as control of all execution threads.
  Preserve existing pools, class policy, query plans, batch/Parquet/compaction
  sizing, cache ownership and spill/WAL/fence contracts.
- Existing outbox, host-settlement and workspace owners keep accepted work
  alive and drain under pressure. No CPU permit across persistence/IO/admission
  waits, lost charge on cancellation, or unbounded drain-memory exemption.

Paths guide the work, not an implementation allowlist. Private helpers, concrete
module layout, sampling API bridge and test seams are implementation-owned.
Do not add public surfaces, SQL/migrations, a general allocator or OS-specific
isolation. Reconcile affected resource-owner comments/contracts with approved
revision 16, rather than retaining contradictory precharge instructions.

## Acceptance scenarios and live evidence

Execute these as ordered test-first cycles. Add proof references/results to
each entry; this is the evidence record, not a separate obligations ledger.
For new tests, confirm and record the actual exact-name runner before RED.
Use controlled owner gates, payload sizes, limits and logical/sample clocks;
never generate synthetic host load or rely on noisy elapsed-time fairness.

1. **Retained consumption and handoff — REQ-019,023; AC-015.** In runtime unit
   tests, create a substantial owner, share/transfer it and release the last
   reference. The root counts physical retention once, not entitlement or the
   number of observers. Refused newly materialized buffers are disposed; disk
   cleanup still retains charge until confirmed deletion. Existing correct
   split/merge/release behavior uses its regression tests, without fabricated
   RED. New attribution/ownership behavior must fail before its implementation.

2. **Small work and real worker observations — REQ-019,020; AC-015,016.** Drive
   real hosted Monty and media paths from server integration owners. Small work
   no longer holds configured maxima, future parent frames or multiplied media
   copies; actual raw/encoded/parent buffers and observed worker bytes remain
   present through their real lifetimes. Reuse and reset an idle worker, hold a
   running turn, delay/coalesce telemetry, cross 500 ms staleness, then confirm
   exit. Independent samples arrive during a held turn; stale/cancelled owners
   never become zero usage. Old envelopes/multipliers and absent live telemetry
   make the changed observations RED. Keep local heap/time/payload refusals.

3. **Capacity, sharing and pressure transitions — REQ-020,021; AC-016.** Runtime
   tests use the agreed P/H/M and q=max(L/M,A/P), unequal demand/local caps and
   multiple tenants. One workload borrows idle capacity without charged floors;
   a new contender changes soft targets. Exercise q=0.95, q=1 and recovery at
   q<=0.90 for two samples. Above-share borrowers pause, untracked allocator
   pressure cannot be bypassed, and sample/ledger overlap is not added. Critical
   reclamation precedes one-at-a-time cancellable victim selection/reassessment;
   acknowledged/mandatory work is ineligible. Missing controller behavior is
   RED; do not manufacture runtime heap-precision claims from counters.

4. **CPU lanes and tenant service — REQ-019,022; AC-017.** Runtime unit tests
   submit controlled jobs from multiple tenants/workloads. Show C managed lanes,
   all-lane borrowing while peers are idle, least normalized busy service at
   both levels, running-service inclusion and no idle-credit windfall. Nested
   polling inherits one lane; Pending/IO releases it; cancellation removes the
   waiter/lease once. Local blockage cannot hold other eligible work. No CPU
   lane is held while awaiting memory or downstream admission. Current separate
   semaphores/no common service accounting make the new behavior RED. Do not
   assert strict time slices or instant interruption of a long native phase.

5. **Actual execution hooks — REQ-022,024; AC-017.** Bifrost integration tests
   run parallel query/repartition/sort/spill and Forge work through production
   execution; Scribe runs actual CPU lane jobs and Monty runs actual turns.
   Resource context and lane observations cover entry and spawned jobs while
   preserving spans/local policies. Hold selected jobs to demonstrate competing
   eligible work obtains released lanes. A wrapper only around public streaming
   must fail this proof. Confirm existing Bifrost workspace/cache/spill and local
   ceiling behavior unchanged; already-correct behavior needs regression/static
   proof, not a forced production rewrite. Missed integration is a task failure.

6. **Connected public journey and bounded drain — REQ-019–024; AC-018.** Extend
   existing Rust-client/WyrdTestServer/Postgres journey owners with verification
   invoking Agent media/code/model work while standalone gateway traffic and
   Bifrost work compete on one combined server. Nested calls inherit initiating
   attribution without double debits; standalone calls retain gateway identity.
   Slow streams remain byte-bounded through completion. Configure low resource
   limits/held dependencies to produce pressure, cancellation and deadline
   expiry. Local quotas/ceilings still refuse at their original boundaries;
   mandatory persistence/host settlement drains before custody releases. Check
   accepted evidence/results and terminal classification through real consumers.
   Existing public methods/wire shapes stay unchanged; do not invent SDK APIs.
   Current disconnected admission must make the new connected observation RED.

7. **Functional closure and performance — REQ-024; AC-018.** Run affected
   functional lanes, then full clean L1 verification, gateway and all three
   Bifrost experiments against compatible clean records in benchmarks/history.
   Record identities/settings, validity, correctness/reconciliation, throughput,
   tail latency, CPU efficiency, memory observations, queue waits and worker
   churn through existing owners/record format. No regression, invented tolerance
   or exit-zero-only claim. Report the cost of sampling/task wrappers and actual
   capacity/admission margin behavior; do not claim improvement from deletion
   alone. Benchmark observations are not test-first RED. TASK-013 retains the
   later full L2 mixed/control and 100-tenant claim fairness work.

## Verification owners and commands

Unit tests belong beside runtime/controller logic and existing Vala/gateway
owners. Real worker/hosted/state/custody integration belongs in server's existing
`integration` target. Bifrost engine integration belongs in redux's existing
target; public client/server proof belongs in `wyrd-testing`/existing SDK journey
owners so there is no dependency cycle. Runtime is SQL/PyO3-free. No new test
binary, command or feature for convenience. New test selectors are recorded by
the implementer, not fabricated here.

Use the existing tasks before raw Cargo commands. Existing focused commands:

```bash
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=resources::tests::process_grants_follow_use_without_double_promises)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=resources::tests::release_wakes_a_registered_waiter)'
mise run test:verification:sandbox
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:server
mise run test:gateway:native
mise run test:bifrost:journey:server
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:forge
mise run fmt:check
mise run lints
git diff --check
```

Use managed Postgres for focused server tests, including these existing
exact-name custody/deadline/replica regressions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::deadline_terminal_waits_for_started_host_work) | test(=pg_verification_runtime::held_persistence_bounds_queued_custody_by_claim_permits) | test(=pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once)'"
```

The focused command must select every named test; record per-selector execution,
not just a passing aggregate. Add changed owner unit tests with their actual
commands. Integrated change review owns unaffected language/full journey lanes;
any changed language-lifetime behavior is tested in that language.

After functional checks, run sequentially on the same qualified host:

```bash
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1:bifrost
```

Complete default coverage and VALID clean records are required. `--smoke`,
`--steps`, `--profile` and `--allow-dirty` diagnostics are not qualification.
Commits/clean-candidate scheduling remain maintainer-owned: report a dirty-tree
qualification prerequisite rather than committing or weakening validity.

## Completion and TASK-013 handoff

Complete means approved revision-16 contract, all scenario evidence, preserved
local/Bifrost/settlement behavior, reviewed implementation and the full L1
comparison. Supply immutable candidate identity, functional commands/results,
L1 record paths/settings and known observations to TASK-013. Its canonical L2
and 100-tenant test then consume this implementation, not HEAD 0ae00f429's old
envelopes or historical TASK-012-R1 charge assertions. Preserve historical
evidence rather than relabelling it PASS for the changed allocation policy.

If the selected telemetry/task hooks cannot cover a real owner, explain that
specific boundary before redesign. Do not silently substitute exact RSS/CPU
guarantees, speculative precharges, new dependency versions, persistence waits,
unbounded queues or weakened existing benchmarks. No production work is part of
this planning turn.

Planning validation (2026-11-16): independent comprehension review of the draft
revision-16 amendment, this task and TASK-013/014 handoffs found no material
explanation or scope gaps. Local document links, dependency ordering and
draft/approved status separation were checked. No implementation tests or
benchmarks were run; this is not implementation evidence or a runtime verdict.

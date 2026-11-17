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

   Evidence: `resources::tests::attribution_follows_custody_and_counts_shared_buffers_once`
   (attribution follows custody, split/merge/transfer counts shared bytes once,
   last release zeroes the owner). RED before `ByteCharge::attribute`: failed at
   `resources.rs:379` (attributed 0, expected 100). GREEN with command R1.
   Existing split/merge/release regressions:
   `process_grants_follow_use_without_double_promises` and
   `release_wakes_a_registered_waiter`, both GREEN with R1 and not re-REDed.
   Refused buffers are disposed, and disk cleanup keeps its charge until the
   deletion is confirmed. Both are existing behavior, proved by their owners'
   regressions, all GREEN and not re-REDed:
   - Hosted workspace:
     `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/verification::hosted::workspace::tests::(archives_unpack_beside_their_staged_file|abandoned_cleanup_keeps_its_scratch_charge|surviving_workspace_keeps_its_scratch_charge|extraction_charges_process_memory_and_scratch|admitted_writes_charge_process_scratch)$/)'`:
     5 passed.
     - Refused extraction and unpack charge nothing and leave no output
       file.
     - An abandoned or surviving cleanup keeps its scratch charge, and a
       confirmed one returns it.
   - Bifrost spill: `oracle::spill::tests::spillers_share_the_process_scratch_ledger`
     passed. A refused write charges nothing, a deleted file returns its
     bytes, and a file whose deletion failed keeps them charged.
   - Bifrost graph cleanup:
     `oracle::analytical::tests::a_failed_graph_cleanup_keeps_its_query_admission_charged`
     passed.
   Status: PASS.

   R1: `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/resources::/)'`
   (15 passed).

2. **Small work and real worker observations — REQ-019,020; AC-015,016.** Drive
   real hosted Monty and media paths from server integration owners. Small work
   no longer holds configured maxima, future parent frames or multiplied media
   copies; actual raw/encoded/parent buffers and observed worker bytes remain
   present through their real lifetimes. Reuse and reset an idle worker, hold a
   running turn, delay/coalesce telemetry, cross 500 ms staleness, then confirm
   exit. Independent samples arrive during a held turn; stale/cancelled owners
   never become zero usage. Old envelopes/multipliers and absent live telemetry
   make the changed observations RED. Keep local heap/time/payload refusals.

   Seam (2026-11-16): crates.io `monty-pool`/`monty-proto` 1.1.0 cannot carry
   worker telemetry. `Checkout::turn_io` and the raw turn loop read worker
   frames only inside a turn. `ChildEvent.kind` is a closed oneof in which any
   non-`Print` kind ends the turn. `Print` text is sandbox-controlled output,
   so it cannot carry trusted samples. Idle workers, and workers suspended
   between turns, are never read. Monty has no memory event or field, and
   1.6.0 still has none. The maintainer first chose vendoring, then replaced
   it with a fork (2026-11-16): `bohmian-ai/monty` branch `wyrd-1.6`, cut from
   v1.6.0 and pinned by rev in `[patch.crates-io]` for every Monty crate in
   the graph. It carries three commits:
   - a `worker-core` build without the ty/ruff type checker, replacing the
     `monty-type-checking` stand-in;
   - a `MemorySample` worker event that the pool's per-worker reader task
     records outside turns, idle or busy;
   - reaping of dead idle workers when memory is listed.
   Fork tests: `idle_workers_report_memory_without_a_checkout`,
   `memory_samples_inside_a_turn_are_recorded_not_answers` and
   `idle_memory_listing_reaps_workers_that_died` (RED: the dead worker was
   never replaced). They run with `cargo test -p monty-pool --test pool_test`
   in the fork: 126 passed, with a fourth commit added later,
   `evicted_idle_workers_are_replaced_by_fresh_ones` (`Pool::evict_idle`).

   Evidence:
   - Worker telemetry, reuse/reset, held turn, staleness, exit and prewarm
     reclaim: `mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/sandbox_worker::/)'`
     (13 passed). The cases are `stale_worker_telemetry_stops_new_work_without_freeing_usage`,
     `sandbox_reuses_processes_with_fresh_tenant_sessions`,
     `suspended_sessions_release_runnable_capacity`,
     `failed_workers_settle_before_capacity_returns` and
     `prewarmed_workers_are_charged_and_reaped_under_pressure`. The last one
     was RED, timing out on "the pool prewarms fresh workers again", while
     reclaim closed the pool. It is GREEN with `evict_idle`.
   - Media copies: `bound_media_holds_only_its_guard_through_the_call`
     asserts one read and a peak equal to the encoded length. vala-verify:
     114 passed.
   - Checkpoint precharge: `verification::state::tests::live_state_stays_charged_through_acknowledged_revisions`.
     RED: 7276 vs 4064 under the 4× precharge. GREEN; state and outbox unit
     tests: 39 passed.
   - Result precharge: `runner.rs::conclude` no longer charges a second
     copy of the decision before building the result write. Once the write
     exists, the decision's charge grows by its weight while both live, then
     fits to the write when the decision drops. This is a source/diff proof
     only, with no RED, so TDD is VIOLATED for this sub-case. Regressions:
     `pg_verification_runtime::{completion_waits_for_the_durable_result,
     lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates,
     reclaimed_attempt_after_a_failed_settlement_redelivers_the_selected_result,
     result_redelivery_is_idempotent_after_restart}` plus the three named
     custody/deadline/replica regressions: 7/7 passed, per selector, with
     managed Postgres.
   - Deferral under pressure: `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees`
     passed (see D3).

   Diagnoses. Each was made by a fresh Codex diagnostician given only the
   failing command, its trace and the diff.
   - **D1. Symptom:** `sandbox_worker::workspace_writes_share_one_budget`
     aborted with a stack overflow after the Monty 1.6 bump.
     **Evidence:** debug frames of 696 KiB (test poll), 652 KiB (sandbox
     program) and 172 KiB (its timeout); the test passes with
     `RUST_MIN_STACK` of 2.5 MiB. **Cause:** `Sandbox::exec`'s state machine,
     embedding the larger 1.6 checkout turns, was stored by value in every
     caller. **Fix site:** `Sandbox::exec` boxes its body once, and
     `run_turn` takes boxed turns; the two call-site boxes were removed.
   - **D2. Symptom:** `stale_worker_telemetry_stops_new_work_without_freeing_usage`
     failed with "worker charges never settled" after SIGKILL.
     **Evidence:** the pool retained the killed idle worker unreaped, the
     sandbox treated the zombie pid as alive, and only checkout reaped dead
     workers, while checkout was blocked by that worker's stale telemetry.
     **Fix site:** the fork's `Pool::idle_worker_memory` reaps dead idle
     workers through acquisition's own accounting.
   - **D3. Symptom:** `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees`
     was never deferred. **Evidence:** the trace shows Scribe refusing
     `execution_inputs` as saturated, the ledger 5,881 bytes over its 2 GiB
     limit, and a runner retry instead of a deferral. **Cause:** the fixture
     filled the whole shared ledger, starving the run's input read before it
     reached Python. **Fix site:** the test-support fixture.
     `Sandbox::overcommit_for_test` reports an overcommitted worker through
     `WorkloadManager::observe_commitment`, keeping pressure critical with
     ledger room left. Raw-ledger occupation stays for the isolated refusal
     tests.

   Independent check (fresh read-only checker, task file plus diff):
   - Unproved cases:
     - Closed: `verification::hosted::worker::tests::delayed_telemetry_coalesces_into_one_current_sample`.
       `report` now takes its writer, and the worker passes stdout, whose
       `write_all` keeps one lock per frame. A sink delays the first frame
       while the committed bytes change. The next frame carries the current
       value, there is one reading per frame and none queue, and the
       reporter stops once its output closes. Mutation: reading once
       before the loop fails on the stale value. Added after the reporter
       (TDD VIOLATED). GREEN:
       `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::hosted::worker::tests::delayed_telemetry_coalesces_into_one_current_sample)'`;
       the `sandbox_worker::` selector above: 14 passed.
     - Samples arriving during a held turn are proved only in the Monty fork.
     - Cancelled owners already had a case:
       `failed_workers_settle_before_capacity_returns` cancels a run
       mid-turn, and `settle` asserts on every poll that no worker's charge
       returns while its process exists. Stale owners are covered by
       `stale_worker_telemetry_stops_new_work_without_freeing_usage`.
     - Raw media buffers charged through their real lifetime: the media test
       sets `peak = encoded` and uses a fake reader, not a hosted media path.
     - The result precharge removal in `runner.rs::conclude` is proved by
       source only.
     - The `Sandbox::exec` stack-overflow fix has no regression test.
     - `stale_worker_telemetry_stops_new_work_without_freeing_usage` reaches
       staleness on a 500 ms wall clock, not a sample clock.
   - Commands missing for "vala-verify: 114 passed" and "state and outbox
     unit tests: 39 passed".
   - The fork, the 1.1→1.6 upgrade and `libmimalloc-sys` rest on the
     recorded maintainer decision, not on revision 16 text.
   - TDD VIOLATED, previously unmarked:
     - `stale_worker_telemetry_stops_new_work_without_freeing_usage`;
     - the rewritten `sandbox_reuses_processes_with_fresh_tenant_sessions`,
       `suspended_sessions_release_runnable_capacity` and
       `failed_workers_settle_before_capacity_returns`;
     - `bound_media_holds_only_its_guard_through_the_call`;
     - the repaired `busy_queued_execution_is_deferred_until_capacity_frees`.
   Status: FAIL (the unproved cases above).

3. **Capacity, sharing and pressure transitions — REQ-020,021; AC-016.** Runtime
   tests use the agreed P/H/M and q=max(L/M,A/P), unequal demand/local caps and
   multiple tenants. One workload borrows idle capacity without charged floors;
   a new contender changes soft targets. Exercise q=0.95, q=1 and recovery at
   q<=0.90 for two samples. Above-share borrowers pause, untracked allocator
   pressure cannot be bypassed, and sample/ledger overlap is not added. Critical
   reclamation precedes one-at-a-time cancellable victim selection/reassessment;
   acknowledged/mandatory work is ineligible. Missing controller behavior is
   RED; do not manufacture runtime heap-precision claims from counters.

   Evidence (`resources::workload::tests::`):
   - `borrowing_contention_and_recovery_follow_samples`: q=max(L/M,A/P), borrowing, 0.95/1.0, two-sample recovery, no overlap addition.
   - `local_caps_redistribute_and_tenants_share_equally`: unequal caps, new contender, tenants.
   - `untracked_allocator_pressure_pauses_all_growth`.
   - `critical_pressure_reclaims_then_cancels_one_victim_per_sample`: reclaim first, one victim per sample, uncancellable ineligible.
   - `paused_admission_starts_after_recovery`.

   RED: all 5 failed on behavior assertions against the stub controller. GREEN: R1. TDD: PASS.

   Independent check: `paused_admission_starts_after_recovery` documented
   that a dropped waiter withdraws its demand but did not assert it. It now
   asserts that a waiting Verification admission gives Verification a soft
   target, and that once dropped, the next sample's target is `None`.
   Added after the behaviour (TDD VIOLATED for that assertion). GREEN:
   `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/resources::workload::tests::/)'`
   (6 passed).
   Status: PASS.

4. **CPU lanes and tenant service — REQ-019,022; AC-017.** Runtime unit tests
   submit controlled jobs from multiple tenants/workloads. Show C managed lanes,
   all-lane borrowing while peers are idle, least normalized busy service at
   both levels, running-service inclusion and no idle-credit windfall. Nested
   polling inherits one lane; Pending/IO releases it; cancellation removes the
   waiter/lease once. Local blockage cannot hold other eligible work. No CPU
   lane is held while awaiting memory or downstream admission. Current separate
   semaphores/no common service accounting make the new behavior RED. Do not
   assert strict time slices or instant interruption of a long native phase.

   Evidence (`resources::lanes::tests::`):
   - `idle_peers_lend_every_lane_and_least_service_goes_next`
   - `running_leases_count_as_service` (mutation-checked: dropping running service makes it fail)
   - `idle_time_earns_no_credit`
   - `tenants_share_their_workload_by_service`
   - `nested_polls_inherit_and_pending_returns_the_lane`
   - `cancelled_waiters_return_lanes_exactly_once` (also checks that idle tenants are forgotten)
   - `blocking_jobs_wait_for_a_lane_and_nest`

   RED: 6 failed on assertions; the blocking test hung on the stub. GREEN: R1.

   TDD: VIOLATED for the cancellation sub-case. The withdraw logic preceded its test, so that test passed on first run.

   "No lane while awaiting memory/downstream admission": `WorkloadManager::admit` takes no lane, and `Laned` releases on Pending. Integration proof is due in scenario 5.

   Independent check, closed:
   - Local blockage:
     - `nested_polls_inherit_and_pending_returns_the_lane` already shows
       laned work awaiting memory holds no lane while the free lane runs
       other work.
     - `resources::workload::tests::local_caps_redistribute_and_tenants_share_equally`
       now keeps a paused heavy tenant's admission waiting while the light
       tenant's admission starts. This replaces the deleted
       `a_non_fitting_head_does_not_block_another_tenant`.
   - Downstream admission: `resources::workload::tests::admission_waits_hold_no_cpu_lane`.
     Under critical pressure, laned work waiting in `WorkloadManager::admit`
     leaves the one lane free, and other work runs on it.
   - Both were written after the behaviour (TDD VIOLATED). GREEN with the
     workload command in scenario 3 (6 passed).
   Status: PASS.

5. **Actual execution hooks — REQ-022,024; AC-017.** Bifrost integration tests
   run parallel query/repartition/sort/spill and Forge work through production
   execution; Scribe runs actual CPU lane jobs and Monty runs actual turns.
   Resource context and lane observations cover entry and spawned jobs while
   preserving spans/local policies. Hold selected jobs to demonstrate competing
   eligible work obtains released lanes. A wrapper only around public streaming
   must fail this proof. Confirm existing Bifrost workspace/cache/spill and local
   ceiling behavior unchanged; already-correct behavior needs regression/static
   proof, not a forced production rewrite. Missed integration is a task failure.

   Partial evidence: inherited lanes.
   - `wyrd_runtime::resources::Inherited` captures the laned work running on
     a thread, its lanes and context, so spawned work runs laned as the same
     work.
     Test: `resources::lanes::tests::spawned_work_inherits_the_parents_lanes`,
     where a spawned future and a spawned blocking job both queue while the
     only lane is held, then run as the parent. RED: the stub inherited
     nothing. GREEN: `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/resources::lanes::/) | test(/resources::workload::/) | test(/^resources::tests::/)'`
     (16 passed).
     The first GREEN run failed on a race in the test itself: one
     `yield_now` did not let the blocking thread queue. The test now waits
     for both waiters.
   - DataFusion's process `JoinSetTracer` (`oracle/telemetry.rs`), shared by
     Oracle and Forge, runs spawned futures and blocking closures on the
     spawner's inherited lanes.
     Test: `oracle::telemetry::tests::spawned_query_partitions_run_on_the_spawners_lanes`,
     a 4-partition aggregate run inside laned Bifrost work, with a UDF probe
     on the spawned partition task. RED, run against the prior tracer, which
     was written first: `[None]`. TDD is VIOLATED by that order, though the
     RED was shown. GREEN: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::telemetry::/)'`
     (5 passed, 3 runs).
   - Cost: one `Arc` clone per laned poll, to record it with scenario 7.
   - Oracle: `execute_retained_root` admits a `Workload::Bifrost` operation
     for the query's tenant, bounded by the query deadline, which surfaces as
     `QueryTimeout`. Both interactive and analytical batch streams go through
     `laned_batches`, which polls each batch on a lane and keeps the operation
     live with the stream. Spawned partitions inherit through the tracer.
     Test: `oracle::tests::laned_batches_poll_each_batch_on_a_lane`. RED (stub
     passthrough): batch 0 ran with context `None`. GREEN.
   - Forge: `ForgeResources` carries the process workload manager.
     `ForgeManagedRewrite::rewrite_plan` polls admission of a
     `Workload::Bifrost` operation for the binding's tenant once. When
     pressure does not admit it, the attempt fails with the existing
     retryable `ForgeError::Capacity`, before any output exists. Otherwise
     the core rewrite runs laned.
     Test: `forge::managed_rewrite::managed_rewrite_is_refused_under_critical_pressure`.
     The first version waited for admission. RED: the rewrite finished under
     critical pressure. GREEN.
   - Diagnosis, Forge journey hang:
     - **Symptom:** `live_rewrite::failed_memory_attempt_retries_without_partial_publication`
       (`mise run test:bifrost:journey:forge`) ran past 300 s. Run alone,
       it was killed at a 200 s timeout.
     - **Evidence:** the trace shows both plans started, then nothing until
       shutdown failed them. The test fills the shared root
       (`occupy_root_for_test`, which charges the process ledger) and frees
       it only after a rewrite fails.
     - **Cause:** the first version's unbounded wait in `admit`. Critical
       pressure kept the rewrite from ever running, so the memory refusal
       the test waits on could never happen.
     - **Fix site:** Forge's call site. Oracle, gateway and hosted callers
       already bound admission themselves, so `admit` stays as it is. Forge
       turns "not admitted now" into its existing retryable capacity
       refusal.
     - **Diagnostician (Codex, read-only):** independently found the same
       circular wait and the other callers' bounds. It proposed a fallible
       `admit`; that was not taken, because each caller already owns its
       bound.
     - GREEN: the journey passes in 11.9 s.
   - Regressions:
     - `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux --test integration -P journey --run-ignored=all"`:
       75 passed.
     - The same wrapper with `--lib`: 940 passed. Run without the wrapper,
       19 Postgres-fixture tests fail on the unset
       `WYRD_TEST_DATABASE_ADMIN_URL`; that is an environment limit, not a
       regression.
   - Not victims: Oracle and Forge operations pass no cancellation token, so
     pressure never picks them as victims. Queries keep their own deadline
     and cancel paths; rewrites drain through attempt cancellation.
   - Scribe: `Scribe::build` joins the ingress and persistence CPU pools to
     the process lanes through `ScribeResources::workloads`. Each Rayon job
     runs through `on_lane`, which begins a non-cancellable Bifrost
     operation and calls `CpuLanes::run_blocking`. The WAL IO pool is not
     laned. Tests:
     `scribe::execution_lanes::tests::{ingress,persistence}_jobs_run_on_the_shared_cpu_lanes`.
     RED (pools ignore the lanes): no job queued for the held lane. GREEN.
     Regressions: under the same wrapper, `--lib` 942 passed and
     `--test integration` 75 passed.
   - `mise run test:bifrost:journey:oracle`: 65 passed.
   - Cross-workload, production execution:
     `wyrd-testing --test oracle capacity::public_query_waits_for_a_shared_cpu_lane`
     (`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=capacity::public_query_waits_for_a_shared_cpu_lane)'"`).
     The test holds every process lane as gateway work and runs a public
     query through the SDK. The query queues and stays unfinished; once one
     lane is released, it returns its row. PASS.
     TDD: VIOLATED, because the test was written after the Oracle wiring.
     Mutation check: with `laned_batches` as a passthrough, it fails with
     "the query finished while other work held every CPU lane".
   - At `b7fbc3897`: `mise run test:bifrost:journey:scribe` 45 passed
     (1 skipped); `mise run test:bifrost:journey:forge` 24 passed.
   - Release before IO (REQ-022): `CpuLanes::off_lane(io)` returns the lane
     a `run_blocking` job holds for the IO, then queues for a lane again as
     the same work. An unwinding job takes no new lane. Scribe staging runs
     directory preparation, the encoder's 8 MiB `sync_data`
     (`SyncingFile::write`), and the final run fsync, preflight and
     directory fsync (`MemberStager::encode_runs`) through it. Only
     encoding keeps the lane.
     Test: `resources::lanes::tests::blocking_io_runs_off_the_lane_and_requeues`.
     RED (stub ran the IO on the lane): competing work timed out. GREEN.
     The Scribe call sites have no runtime proof of their own; this rests on
     the cited source plus the regressions: `wyrd-runtime --lib` 93 passed,
     redux `--lib` 942 passed, `--test integration` 75 passed.
   - The other direction, an open query giving its lanes to verification:
     `capacity::open_query_stream_holds_no_cpu_lane` (Oracle journey
     binary).
     - It scans about 50 MiB in 8 MiB batches, more than the transport
       buffers, and reads one batch.
     - While the stream is open and the server is still mid-result,
       verification work takes every lane the idle server had.
     - The query then delivers all 50,000 rows.
     - Gateway calls take no CPU lane, so verification is the competing
       workload.
     - TDD VIOLATED: the test was written after the implementation.
     - Mutation check: `laned_batches` holding one lane for the stream's
       life fails "an open query stream kept a CPU lane from verification
       work". GREEN in 13.6 s:
       `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=capacity::open_query_stream_holds_no_cpu_lane)'"`.
   - Scribe tenant service: request-path ingress jobs run on the lane as
     their frame's tenant. That covers canonical decode, preprocessing an
     admitted append, and shredding a slice. Persistence jobs mix tenants
     and stay unattributed (`None`).
     - Test: `scribe::execution_lanes::tests::ingress_jobs_run_on_the_shared_cpu_lanes`
       now asserts the job's `WorkContext.tenant`.
     - RED: `run` took the tenant but `on_lane` ignored it; "the lane
       serves the job's tenant" failed with left `None`. GREEN.
     - Regressions: redux `--lib` 942 passed, `--test integration` 75
       passed, `mise run test:bifrost:journey:scribe` 45 passed and 1
       skipped.
   - Spill IO off the lane (the user chose "release for the rest of the
     poll").
     - A laned poll records its lease where `CpuLanes::off_lane` finds it.
       In a poll, `off_lane` returns the lane and the rest of that poll
       runs without one, because waiting for a lane would block the async
       worker. The next poll queues again.
     - Blocking jobs keep release-and-requeue. `ChargedSpillWriter`'s
       write, flush and finish run through `off_lane`.
     - Ceiling: the rest of a poll after a spill write can run with more
       than C polls running at once.
     - Tests:
       - `resources::lanes::tests::polled_io_returns_the_lane_for_the_rest_of_the_poll`.
         RED: "the lane is free while the poll does IO", left 0. GREEN;
         `wyrd-runtime --lib` 94 passed.
       - `oracle::spill::tests::spill_writes_return_the_query_lane`,
         written after the implementation. Mutation check: a plain write
         fails it.
   Final lane runs, with the spill and polled-lane changes included:
   redux lib under managed Postgres, 943 passed
   (`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux --lib"`),
   and `mise run test:bifrost:journey:oracle`, 67 + 1 passed.

   Independent check:
   - Closed: `sandbox_worker::a_running_turn_holds_its_cpu_lane`. With one
     effective CPU, a busy Python turn leaves 0 lanes free while it runs,
     and the lane returns when its deadline ends the turn. Mutation:
     dropping the lease right after `run_turn` acquires it fails with
     left 1, right 0. Added after the wiring (TDD VIOLATED). GREEN:
     `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(=sandbox_worker::a_running_turn_holds_its_cpu_lane)'"`.
   - Closed: `oracle::telemetry::tests::spawned_query_partitions_run_on_the_spawners_lanes`
     now asserts its physical plan contains `RepartitionExec` and
     `SortExec`. Separate probes before the hash exchange and after the
     final aggregate, which then sorts, both see only the query's laned
     context. Extended after the behaviour (TDD VIOLATED). GREEN:
     `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::telemetry::tests::spawned_query_partitions_run_on_the_spawners_lanes)'`.
   - Closed: `main.rs` `tests::committed_bytes_follow_touched_allocations`.
     Under the server's mimalloc global allocator, `committed_bytes()`
     while a touched 64 MiB block is held is at least the block. Mutation:
     reading the page-fault field instead fails with 0. A field that also
     covers held memory (RSS) would pass; that `commit` is the right field
     rests on mimalloc's `mi_process_info` signature. Added after the
     sampler (TDD VIOLATED). GREEN:
     `mise exec -- cargo nextest run --locked -p wyrd-server --bin wyrd-server -E 'test(=tests::committed_bytes_follow_touched_allocations)'`.
   - Closed: `scribe::parquet_writer::tests::syncing_file_resets_backlog_at_interval`
     now runs inside a one-lane laned poll. Writes below the interval keep
     the lane (free 0), and the interval sync returns it (free 1).
     Mutation: syncing without `off_lane` fails with left 0, right 1.
   - Closed: `scribe::member_stager::tests::staging_disk_io_returns_the_cpu_lane`.
     `encode_runs` inside a one-lane laned poll leaves the lane free after
     its directory and fsync IO. Mutation: running both stager IO sections
     without `off_lane` fails with left 0, right 1. Because a poll stays
     lane-less after its first release, this proves the stager returns its
     lane for disk work, not each of its two sites separately. Both tests
     were added after the wiring (TDD VIOLATED). GREEN:
     `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::syncing_file_resets_backlog_at_interval) | test(=scribe::member_stager::tests::staging_disk_io_returns_the_cpu_lane)'`.
   - The spill choice, release for the rest of the poll, lets more than C
     polls run at once during spill IO. That is the recorded user decision,
     not revision 16 text, and it should be weighed against scenario 4's
     "C managed lanes".
   - TDD VIOLATED, previously unmarked: the Monty lane wiring in
     `sandbox.rs` and the `main.rs` sampler.
   Status: PASS for the runtime proofs. The spill concurrency choice above
   stays open for the user's call against scenario 4's "C managed lanes".

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

   Partial evidence: gateway admission.
   - `GatewayInvocation::admit_call` admits every call as a process
     `Operation` through `AppStateInner::resources`, the boot-composed plan,
     before any capture or quota reservation. The wait is bounded by the
     call's deadline. The victim token is the call's cancellation.
   - Standalone calls are `Workload::Gateway`. `ServerWyrdGatewayCaller`
     (Agent model calls) admits as `Workload::Verification` via `.nested`.
   - Test: `components::gateway::pg_invocation_tests::critical_pressure_pauses_gateway_calls_until_recovery`.
     RED: "the paused call never dispatched" (it dispatched). GREEN, plus all
     gateway unit tests (79 passed):
     `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/components::gateway::/)'"`.
   - Buffered answers: `respond` charges a buffered JSON or media answer's
     exact length, which already exists, and attributes it to the call's
     operation. The charge rides on `GatewayCallResponse` until it drops.
     Streams stay bounded by their frame channel. Capture payloads are
     charged by the outbox when staged.
     Test: `components::gateway::pg_invocation_tests::buffered_answers_count_against_the_calling_workload`
     proves standalone bytes count as gateway work and nested bytes as
     verification work, never both, and both return on drop. RED: left 0,
     right 22. GREEN (80 passed, same command).
   - Connected pressure journey:
     `verification_runtime::isolated_capacity::connected_work_waits_out_pressure_with_its_own_attribution`.
     - Setup: one combined server is held at critical pressure.
     - Actions: a standalone chat completion over raw HTTP waits in
       admission as Gateway work. A public SDK query waits as Bifrost work.
       A queued Agent run waits in its Bifrost input read.
     - Synchronization: the test waits on the new admission debug event,
       "new work waits for process pressure to admit it", from
       `WorkloadManager::admit`. It does not sleep.
     - After recovery, all three complete. The nested model call is recorded
       against the verified Service's UID, and the standalone call against
       no subject. No workload keeps bytes afterwards.
     - TDD VIOLATED: the journey was written after the implementation.
     - Mutation check: replacing the gateway `admit` with `begin`, the
       disconnected behaviour, fails "Gateway work never waited for pressure
       1 times". GREEN in 4.3 s:
       `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::isolated_capacity::connected_work_waits_out_pressure_with_its_own_attribution)'"`.
   - Request bodies (user decision: hand the charge to the call).
     `WyrdBodyLimit` offers a generic body's charge as a `BodyCustody`
     extension. The gateway ingress layer puts it into `IngressClock`, and
     `GatewayInvocation::parsed` takes it. Admission attributes it to the
     call's operation, and `AdmittedCall` keeps it until the call settles.
     `RequestJson::parse` shares the HTTP bytes, so the body is counted
     once. A call with no handed charge (a nested Agent call's serialized
     body) is charged its kept text at admission. An untaken charge still
     returns with the response.
     - `components::gateway::pg_invocation_tests::call_bodies_count_against_the_calling_workload_while_the_call_runs`:
       - Held at dispatch, a nested body counts as Verification work, at its
         exact length. RED: left 0, right 33.
       - A standalone call handed a 4096-byte charge through its clock
         counts exactly 4096 as Gateway work. It takes the charge from the
         custody, and no bytes outlive the call.
       - GREEN, with all gateway and middleware lib tests (96 passed):
         `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/components::gateway::/) | test(/http::middleware::/)'"`.
     - `http::middleware::body_limit::tests::a_taken_body_charge_outlives_the_response`:
       a taken 40-byte charge stays on the ledger after the response, once,
       and returns when dropped. An untaken charge stays while a handler
       that dropped the request's extensions still runs; the middleware
       keeps its own custody handle until the response returns. RED: left
       [0], right [20]. GREEN (7 passed):
       `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(/http::middleware::body_limit::/)'`.
     - Journey `verification_runtime::isolated_capacity::standalone_call_bodies_count_as_gateway_work_while_the_call_runs`:
       - Over raw HTTP, a 64 KiB chat completion held at the provider (the
         existing `HeldUpstream`, now in front of the desk's upstream)
         counts its exact body length as Gateway work and nothing for
         other workloads, then 0 after it settles.
       - Mutation: dropping the clock's custody still passes, because
         admission then charges the kept text. Only the ledger total would
         differ, and that total moves with background work (measured
         51–63 KB against a 65 KB body), so the handoff itself is proved by
         the two lib tests above.
       - GREEN, with the other two capacity journeys (3 passed):
         `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(/verification_runtime::isolated_capacity::/)'"`.
     - TDD: the nested and handed cases were RED first; the HTTP wiring and
       the journey were written after it (VIOLATED for that wiring).
   - Relayed answers: `relay` keeps a buffered answer's charge inside the
     HTTP body (`Full` with `map_frame`, which keeps the exact size hint,
     so `Content-Length` is unchanged). It returns once the body is sent or
     dropped. Batch answers carry no charge.
     - `components::gateway::pg_invocation_tests::buffered_answers_count_against_the_calling_workload`
       now also renders a chat completion through the public handler and
       asserts the answer stays charged until its HTTP body is read. RED:
       "a relayed answer stays charged until its HTTP body is sent".
       GREEN (96 passed, the gateway and middleware lib command above).
     - Wire unchanged: `mise run test:gateway:native` (17 Rust gateway
       journeys and 8 Python native-client journeys passed).
   - Pressure cancellation of a running call:
     `components::gateway::pg_invocation_tests::critical_pressure_cancels_a_running_gateway_call`.
     A standalone call held at dispatch carries its body as Gateway bytes.
     Two critical samples (reclaim, then one victim) cancel it, the caller
     gets `ServiceUnavailable`, and its bytes return. Written after the
     body attribution (TDD VIOLATED). Mutation: removing the body's
     attribution fails "workload_bytes(Gateway) > 0", so the call is never
     eligible. GREEN with the gateway lib command above.
   - Deadline expiry under pressure, already proved:
     `critical_pressure_pauses_gateway_calls_until_recovery` (gateway) and
     `pg_verification_routes::direct_execution_waits_end_at_the_arrival_deadline`
     (verification, 504 at the arrival deadline).
   - Named custody, deadline and replica regressions, per selector (3 passed):
     `deadline_terminal_waits_for_started_host_work`,
     `held_persistence_bounds_queued_custody_by_claim_permits`,
     `two_replicas_claim_each_queued_run_exactly_once`, with the managed
     Postgres command in "Verification owners and commands".
   - Slow streams: journey
     `verification_runtime::isolated_capacity::a_slow_stream_holds_only_its_request_body`.
     A raw-HTTP streamed chat completion (2,000 chunks) is held open by the
     provider after its first event. `HeldUpstream` now sends a held event
     stream's first event at once and the rest on release. While it is
     open, its Gateway work is exactly its request body, kept past the
     handler's response, and no relayed frame is charged. On release, all
     frames arrive, and after a drained shutdown Gateway holds 0. Mutation: unattributing the body fails with left 0.
     Written after the handoff (TDD VIOLATED). GREEN with the other three
     capacity journeys (4 passed), using the isolated-capacity command above.
   - Remaining cases (user decision: existing owner tests count as the
     evidence):
     - Code work under pressure: the isolated-capacity Python journey
       (`direct_verification_runs_while_queued_capacity_is_held`). Media
       answers take the same buffered-answer charge and relay path as JSON;
       `mise run test:gateway:native` covers the media operation journeys.
     - Local quotas and ceilings: the 96 gateway and middleware lib tests,
       including the ledger, quota and capture-slot refusals, plus the
       verification execution-slot and direct-ceiling refusals in the
       isolated-capacity and route tests above.
     - Bounded drain: the three named custody regressions above.
     - Cancellation and deadline: the pressure tests above.
   - Independent check:
     - The connected journey itself has no media or code work,
       cancellation, deadline expiry or drain. Those rest on owner tests,
       by user decision.
     - Closed: the connected journey now also asserts exactly two call
       records. The nested call is recorded once, as the Service's, never
       also as gateway traffic. The lib tests above prove its bytes are
       never counted as both workloads.
     - Closed: `buffered_answers_count_against_the_calling_workload` now
       relays a buffered media answer. It counts exactly its 29 bytes as
       Gateway work until the HTTP body is read, then 0. Mutation: relaying
       the media bytes without their charge fails with left 0, right 29.
       Both were added after the behaviour (TDD VIOLATED).
     - TDD VIOLATED, previously unmarked: the three rewritten deferral
       tests were changed to match the implemented behaviour (user
       decision), and the gateway relay wiring had no test first.
   - Removing the session precharge broke the premise of three tests,
       that 64 MiB free is short of one Python session. A Codex
       diagnostician found the cause: nothing refused the run, so it never
       deferred or timed out. Ledger-full setups starve the Bifrost input
       read, and critical pressure now stops that read at Oracle admission.
       The user chose to assert the real behaviour, that work waits and
       then completes.
       - `verification_runtime::isolated_capacity::direct_verification_runs_while_queued_capacity_is_held`:
         under critical pressure, the queued Python run waits in its
         Bifrost read and completes after recovery.
       - `pg_verification_routes::direct_execution_waits_end_at_the_arrival_deadline`:
         under critical pressure, the direct run times out with 504 at its
         deadline, then passes.
       - `verification::pg_tests::execution_waits_and_refusals_follow_hosted_capacity`
         (MCP): the same, plus a queued run that passes after recovery.
       - All three pass. Stale "session envelope" comments and the
         `one_session` name are gone. A sweep for tests of removed
         allocation logic found no test that only asserts removed
         behaviour.
     Status: PASS.

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

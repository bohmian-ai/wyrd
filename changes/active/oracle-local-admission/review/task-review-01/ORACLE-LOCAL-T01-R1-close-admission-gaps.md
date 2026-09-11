---
id: ORACLE-LOCAL-T01-R1
title: Close Oracle local-admission acceptance gaps
kind: remediation
status: ready
skill: wyrd-implement
spec: changes/active/oracle-local-admission/spec.md
task: changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
reviewed_candidate: fe2fb5f8ba5e1eb09e2faf8a7bf909e34a8c61b7
findings:
  - FIND-ORACLE-LOCAL-T01-1
  - FIND-ORACLE-LOCAL-T01-2
  - FIND-ORACLE-LOCAL-T01-3
  - FIND-ORACLE-LOCAL-T01-4
  - FIND-ORACLE-LOCAL-T01-5
  - FIND-ORACLE-LOCAL-T01-6
  - FIND-ORACLE-LOCAL-T01-7
---

# Close Oracle local-admission acceptance gaps

Required execution skill: `$wyrd-implement`.

## Subject and authority

Implement against the cumulative candidate beginning at `fe2fb5f8ba5e1eb09e2faf8a7bf909e34a8c61b7`, preserving the approved behavior in `changes/active/oracle-local-admission/spec.md` revision 2 and the original task at `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`. The next review covers the complete cumulative range from base `d3888ddae83c833c3eb85edc0ce226eb6debadce`.

## Diagnosis and intended correction

### Governed memory is not yet bounded by each query ceiling

FIND-ORACLE-LOCAL-T01-1: `OracleMemoryRoot::grow` classifies infallible bytes only against pod-wide free floor/elastic capacity. It does not apply the query view's remaining ceiling, so bytes above a query's immutable grant may remain governed rather than becoming explicit process headroom. The existing shared-root test exercises infallible growth but never distinguishes the two charge components.

Keep `OracleMemoryRoot`, the query view, and `OracleMemoryCharge` as the existing owners. For every infallible growth, the governed component must be bounded by both the pod's currently available governed capacity and that query's remaining governed ceiling; the remainder is retained as headroom and later released headroom-first. This closes the gap without another pool, owner, or configuration surface.

### Class quanta still masquerade as resident memory

FIND-ORACLE-LOCAL-T01-2: follower acquisition retains an `OracleMemoryLease` for the 32/64 MiB class quantum before any DataFusion reservation, while leader admission retains and reports the same quantum as reserved memory. This contradicts the selected separation: slots govern concurrency, actual shared-pool growth governs memory, and scratch remains independently leased.

Remove query-class quantum memory ownership and synthetic memory reporting from leader and follower paths. Reuse the existing aggregate slot ledger, query memory view, actual root-growth accounting, and scratch owner. An admitted query or follower with no live pool reservation must consume zero governed query bytes; its first actual consumer growth must be the first memory charge.

### Calibration accepts a different contract than the profile declares

FIND-ORACLE-LOCAL-T01-3: calibrated tenant limits are clamped into range. Invalid evidence therefore boots with silently rewritten values.

Make calibration translation reject an Interactive limit outside `1..=total_local_units`. When Analytical is enabled, reject limits outside `ANALYTICAL_QUERY_SLOT_UNITS..=analytical_slots`; when disabled, require zero. Preserve the existing v2 names and do not add aliases or migration behavior.

### Worker eligibility and advertised class capacity disagree

FIND-ORACLE-LOCAL-T01-4: every pod advertises Analytical even when its local split disables it, and remote bounding happens before class compatibility. Class-neutral/Interactive roster construction also records selected remotes.

Publish `supported_classes` from the installed local split: Interactive remains supported; Analytical is present only when the pod can admit its two-unit minimum. Once the physical root derives Analytical, select the configured deterministic subset from the pinned live, authorized, Analytical-capable remote roster. That single list remains the authority for reservation, dispatch, metrics, and cleanup. Preserve stable node ordering, UUID rotation, zero-worker local execution, the leader, deadlines, security checks, and the no-successor terminal policy.

### Cross-boundary proof is weaker than the accepted task

FIND-ORACLE-LOCAL-T01-5: the server journey observes eventual wins from nondeterministic races but not FIFO or exact equal-weight rotation. The metrics test does not produce memory refusal, nonzero headroom, or selected-worker evidence, and RED results were not recorded.

Reuse the existing schema-stall, queue inspection, and release choreography. Hold capacity, enqueue identified tenant requests in a known order, observe that they are queued, then release before the existing bounded wait expires and assert completion order. This must prove per-tenant FIFO and one-grant equal tenant rotation through real authenticated server requests without a new public API or a longer production wait. Extend the existing metrics contract through the real shared root and Analytical cut so it asserts a refusal series, nonzero headroom, and bounded selected-worker observation. Record every original task RED and GREEN command result.

### Default unit-test compilation and import placement regressed

FIND-ORACLE-LOCAL-T01-6: default-feature Redux lib tests consume instrumentation compiled only with `test-support`, so the task's exact named commands fail with `E0599`.

Reuse the repository's established `cfg(any(test, feature = "test-support"))` boundary for instrumentation needed by both internal Rust unit tests and external test-support consumers. Do not add a feature or change the approved exact commands.

FIND-ORACLE-LOCAL-T01-7: new function-scoped imports violate the repository rule that a module's imports form its dependency manifest.

Move the new imports to the existing owning test-module import blocks. Do not suppress or weaken the rule.

## Constraints and preserved behavior

- Keep PostgreSQL out of query admission and retain historical migrations and decode-only audit history.
- Keep one `OracleAdmission` scheduler and one `BifrostResourceGovernor` slot ledger; add no scheduler, semaphore, lease, cache, coordinator, factory, trait, or dependency.
- Keep one shared Oracle `FairSpillPool` root and private query ceiling views.
- Preserve authentication, authorization, tenant identity, audit WAL, physical-root query-class derivation, result limits, spill formats, object reads, deadlines, terminal failure, and fail-closed cleanup.
- Preserve independently enforced scratch, slot, CPU parallelism, queue, and selected-worker bounds.
- Preserve the deterministic selected-worker algorithm and single-attempt policy after moving it to the class-eligible cut.
- Keep changes within the original task's owners and test surfaces.

## Non-goals

- No specification revision, public API, SDK, Card, migration, compatibility alias, deployment topology, or distributed admission mechanism.
- No unrelated Forge readiness or test-inventory work.
- No broad refactor of Oracle planning, dispatch, or resource ownership beyond these findings.
- No new test framework or generalized scheduling fixture.

## Acceptance criteria

- AC-R1-001 / FIND-ORACLE-LOCAL-T01-1: infallible growth above a query ceiling charges at most the ceiling as governed, records the remainder as headroom even when the pod root has free bytes, and shrink/drop restores every counter.
- AC-R1-002 / FIND-ORACLE-LOCAL-T01-2: a newly admitted leader and follower consume slot/scratch ownership but zero governed query memory until a pool consumer grows; actual growth and release reconcile the shared root exactly.
- AC-R1-003 / FIND-ORACLE-LOCAL-T01-3: zero/oversized Interactive caps, one-unit/oversized enabled Analytical caps, and nonzero disabled Analytical caps are rejected rather than clamped; valid bounds remain unchanged.
- AC-R1-004 / FIND-ORACLE-LOCAL-T01-4: an Analytical-disabled pod does not advertise Analytical; bounded selection chooses only Analytical-capable remotes; an incapable unselected replica cannot affect success; Interactive work records zero selected workers.
- AC-R1-005 / FIND-ORACLE-LOCAL-T01-5: the real authenticated server journey deterministically proves per-tenant FIFO and exact equal-weight rotation in addition to bounded overload, floor protection, borrowing, and both-class progress; the metrics contract observes refusal, nonzero headroom, and selected-worker count.
- AC-R1-006 / FIND-ORACLE-LOCAL-T01-6: every exact default-feature Redux lib-test command named in the original task compiles and executes the intended single test without selecting zero tests.
- AC-R1-007 / FIND-ORACLE-LOCAL-T01-7: no task-added function-scoped import remains.

## Focused proof and broader verification

Run every exact named RED command from the original task sequentially, including the default-feature Redux lib-test commands. Add the narrow focused cases required by AC-R1-001 through AC-R1-005 to the existing owning tests rather than creating new test binaries.

Then run:

```bash
mise run check:bifrost-resource-governance
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:sql
mise run test:bifrost:integration:server
mise run test:bifrost:journey:oracle
mise run codegen:check
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce..<new-candidate>
```

Record the exact focused commands, selected test names, exit status, and direct behavioral evidence in the original task before requesting cumulative re-review.

## Remediation evidence

Cumulative candidate `d3888ddae..648e152e7`; remediation commits
`617155857..648e152e7`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R1-001 / FIND-1 — infallible growth bounded by the query ceiling | `fa76783ab`: `OracleMemoryRoot::grow` passes `view.ceiling_bytes - ledger.governed` into `reserve_oracle_query_memory_infallible`, which now takes an explicit `governed_ceiling`; `OracleQueryMemoryLedger` tracks `governed` so the split survives release, headroom-first | `resources::tests::oracle_queries_share_one_governed_memory_root` — extended with a ceiling-bound block. RED before the fix: governed rose to 402653184 against a 268435456 grant; GREEN after: excess is headroom | PASS |
| AC-R1-002 / FIND-2 — no class quantum masquerading as resident memory | `c6e55b1f5`: deleted `Grant.memory`, `ClassState.memory_used`, `LocalPermit.memory`, `OracleWorkerResources::lease`/`memory_bytes()`, `OracleQueryResources::memory_bytes`, and `OracleWorkerClass::memory_bytes()`; `try_acquire_worker` no longer takes a memory lease; both report constructors read `self.shared.resources.shared_memory_reserved()` | `resources::tests::oracle_leader_and_follower_govern_no_memory_until_their_pools_grow` (new); `oracle::dispatcher::resource_tests::remote_worker_stream_retains_slot_units_until_terminal_drop` and `oracle::dispatcher::resource_tests::oracle_peer_remote_execution_owns_one_worker_slot_quantum` — both converted from memory-byte to slot-unit observables plus a zero-governed-bytes assertion | PASS |
| AC-R1-003 / FIND-3 — calibrated tenant caps rejected, not clamped | `75e8320f7`: `wyrd-server/src/config.rs` rejects an Interactive cap outside `1..=allocation_sum` and an Analytical cap outside `ANALYTICAL_QUERY_SLOT_UNITS..=analytical_slots` (exactly `0` when the class is disabled); new `proposal_u32_allowing_zero` makes the disabled-class rule expressible | `config::tests::oracle_admission_config_translates_calibration` — four rejection cases plus the disabled-class pair; `config::tests::oracle_capacity_is_local_cpu_and_memory_bounded` | PASS |
| AC-R1-004 / FIND-4 — selection is class-eligible and the metric is class-correct | `80182aa89`: `select_bounded_workers` retains only the leader and replicas advertising `QueryClass::Analytical`; `bifrost_oracle_selected_workers` moved from class-neutral `freeze` to `finalize`, recording `0` for Interactive. Selection stays in `freeze` because `build_physical_root` consumes the roster before the class exists | `oracle::tests::analytical_worker_selection_is_bounded_and_stable`; `oracle::tests::analytical_worker_selection_skips_class_incapable_replicas` (new, `954898f4e`); `analytical_activation::analytical_reserves_only_configured_workers`; `analytical_activation::selected_peer_failure_is_terminal`; `boot::tests::oracle_advertises_only_locally_admissible_classes` (new) proves a pod advertises Analytical only when it can admit it | PASS |
| AC-R1-005 / FIND-5 — deterministic FIFO and rotation; driven metric signals | `75d0e0caf`: `record_oracle_capacity` now runs wherever governed memory actually moves (`try_reserve_oracle_query_memory`, `reserve_oracle_query_memory_infallible`, `release_oracle_query_memory`), not only at admit/release — the headroom and used gauges were otherwise stale between those points. `99d0089c7`: new journey saturates one pod's slot units, enqueues three identified requests in a known order, confirms each enqueue through the node's own inspection, and releases one unit so completion order is grant order | `capacity::queued_tenants_are_granted_fifo_and_rotated` (new) — observed exactly `["first-older", "second", "first-newer"]`; falsified by inverting the expectation, which failed with the real observed order. `oracle::tests::oracle_metrics_describe_only_local_capacity` — now asserts a `result="refused"` acquisition counter, a nonzero `bifrost_oracle_local_bytes{kind="memory_headroom"}` peak, and `bifrost_oracle_selected_workers` count/max `(1, 2)` from a real Analytical cut. `capacity::two_tenants_make_bounded_progress_across_query_classes` still covers borrowing, overload, floor, and both-class progress | PASS |
| AC-R1-006 / FIND-6 — default-feature unit tests compile | `617155857`: widened the instrumentation gates from `cfg(feature = "test-support")` to the repository's `cfg(any(test, feature = "test-support"))` boundary for `graph_leases_activated_total`, its backing `AtomicU64` field, initializer, and increment site, and for `runtime_inspection`/`OracleRuntimeInspection`. No feature added; no approved command changed | Every exact named `-p vala-bifrost-redux --lib` command below runs without `--features`; RED was 11× `E0599`, then `E0615` on the backing field | PASS |
| AC-R1-007 / FIND-7 — imports in module manifests | `8d846fc91`: hoisted the task-added function-scoped imports into the owning test modules' import blocks (`super::participant_cut::tests::lease`, `chrono::Utc`, `wyrd_spec::vala::api::{ClusterCapabilities, ClusterRole}`) | `mise run lints` clean; `mise run fmt` clean | PASS |

### Exact named commands re-run (all GREEN)

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_queries_share_one_governed_memory_root)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::local_admission_is_fair_and_work_conserving)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::admission::tests::follower_release_wakes_waiting_leader_without_reordering)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::analytical_worker_selection_is_bounded_and_stable)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::oracle_metrics_describe_only_local_capacity)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=config::tests::oracle_capacity_is_local_cpu_and_memory_bounded)'
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::heterogeneous_oracles_ignore_historical_admission_rows)'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::memory_refusal_preserves_oracle_health_and_next_query)'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::two_tenants_make_bounded_progress_across_query_classes)'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=analytical_activation::analytical_reserves_only_configured_workers)'"
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=analytical_activation::selected_peer_failure_is_terminal)'"
```

Tests added or renamed by this remediation, each run the same way:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::tests::analytical_worker_selection_skips_class_incapable_replicas)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::oracle_leader_and_follower_govern_no_memory_until_their_pools_grow)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=resources::tests::resource_plan_oracle_metadata_leases_spend_floor_first)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::dispatcher::resource_tests::remote_worker_stream_retains_slot_units_until_terminal_drop)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::dispatcher::resource_tests::oracle_peer_remote_execution_owns_one_worker_slot_quantum)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=config::tests::oracle_admission_config_translates_calibration)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=boot::tests::oracle_advertises_only_locally_admissible_classes)'
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=capacity::queued_tenants_are_granted_fifo_and_rotated)'"
```

The original task named
`oracle::dispatcher::tests::remote_worker_stream_retains_root_quantum_until_terminal_drop`;
its module is `resource_tests` and its observable is now slot units, so the
corrected path above is the one that runs.

### Lane verification

| Lane | Result |
|---|---|
| `mise run check:bifrost-resource-governance` | PASS (fixture coverage + check) |
| `mise run test:bifrost:integration:redux` | PASS — 972/972 |
| `mise run test:bifrost:integration:sql` | PASS — 107/107 |
| `mise run test:bifrost:integration:server` | PASS — 67/67 |
| `mise run test:bifrost:journey:oracle` | PASS — 28/28 |
| `mise run codegen:check` | PASS — no drift |
| `mise run verify:bifrost` | PASS — 9/9 lanes, 1494.96s |
| `mise run fmt` | PASS |
| `mise run lints` | PASS |
| `git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce..648e152e7` | clean |

### Scope

No non-goal was implemented: no specification revision, public API, SDK, Card,
migration, compatibility alias, deployment topology, or distributed admission
mechanism, and no unrelated Forge readiness or test-inventory work. The write
set stayed inside the original task's owners: `vala-bifrost-redux`
(`resources.rs`, `oracle/{mod,admission,dispatcher,participant_cut}.rs`),
`wyrd-server` (`config.rs`, `boot/mod.rs`), and the Oracle journey suite
(`wyrd-testing/tests/bifrost/oracle/capacity.rs`).

One earlier claim in the original task is now superseded: AC-003 recorded that
`max_queue_wait = 250 ms` made queue order unobservable through the public
path. It is observable — the whole enqueue-confirm-release choreography fits
inside the first waiter's bounded wait, and
`capacity::queued_tenants_are_granted_fifo_and_rotated` proves per-tenant FIFO
and equal-weight tenant rotation through real authenticated requests.

### Material risk

The queue journey depends on three grants completing inside the first waiter's
250 ms bounded wait. It warms both clients' query paths first and polls the
node's own inspection at 1 ms, and it passed on every run here (4.2–17.0 s
wall), but a heavily loaded runner could turn it into a timeout rather than an
ordering failure. The failure would read as "a queued waiter was never
granted", which is distinguishable from an ordering violation.

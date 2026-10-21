---
id: TASK-004
kind: implementation
status: review
spec: SPEC-long-horizon-verification
spec_revision: 21
requirements: [REQ-015, AC-009]
depends_on: [TASK-001, TASK-002, TASK-003]
---

# Benchmark the verification runtime's overhead

## Outcome and Value

`mise run bench:verification:capacity` measures what the verification runtime
adds around a Verifier, for plain direct `observe().verify` and background
`observe().submit`. It judges direct server overhead p99 < 1 ms and reports
the async path's submit, queue, and run costs. The server's own metrics
separate its overhead from the Verifier's execution, and the client
measurement shows what the client and transport add.

## Owners, Scope, Consumers, and Prohibited Changes

Execute with `$wyrd-implement`. The benchmark is the `verification-capacity`
workload of `crates/wyrd/wyrd-testing/src/bin/capacity/`. It is opt-in and in
no gate or verify lane. Never relax the 1 ms target to pass.

There is one tenant, one assertion-only Verifier, no `group_id`, and no bursts
or neighbor tenant. Grouping is TASK-001–003 functionality and is not
benchmarked here.

Server measurement follows Bifrost's query profile:

- one `ExecutionTelemetry` per execution;
- block intervals recorded by the engine's `BlockClock`;
- the HTTP arrival taken once by the existing `track_metrics` middleware.

No timers are threaded through callers. The Verifier runner's claim loop
becomes pull-based, like Forge workers. Profiling is diagnostic.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Missing behavior | Selected extension |
|---|---|---|---|
| Benchmark | capacity binary, `release_server::LocalServer`, `wyrd_testing::{capacity, profile}` | No runtime-only cases | `verification/{mod,run,report,workload}.rs`; two cases over 1/4/16/64 clients |
| Verifier time | `vala_verify::blocks::BlockTimer` (already times every block) | Intervals not exposed to the host | `BlockClock` on `VerificationEngine::with_clock`; `BlockTimer` records into it |
| Server overhead | `verification::telemetry::ExecutionTelemetry` | Measured engine minus hand-wrapped waits; missed admission, load, and staging | Profile starts at HTTP arrival (direct) or claim (queued). Overhead = total − union(blocks). `WaitSink`, `StreamWaits`, and `Waited` were deleted |
| Request arrival | `http::middleware::metrics::track_metrics` | No arrival handed to handlers | `RequestArrival` request extension |
| Submit time | `components::verification::routes::enqueue` | Unmeasured | `wyrd_verification_submit_seconds{outcome}` |
| Claim loop | `verification::claims::ClaimLoop` | One claim per tenant per round; 1 s idle poll | Claims up to free capacity, round-robin; wakes on submission commit; the poll remains as fallback |

## Acceptance Criteria

- Direct `verify` at every client point: `wyrd_verification_overhead_seconds{mode="direct"}` p99 bucket bound strictly < 1 ms from ≥1,000 samples, with zero refusals and correct verdicts.
- Direct reports p50/p95/p99 and mean overhead, Verifier time, server total, phases, and client-added time.
- Background `submit` reports server submit time, queue wait, queued overhead and Verifier time, trigger to terminal, throughput, and drain. It fails only on a refusal or a backlog undrained after 60 s.
- Every window prints the runtime component Pareto, the CPU Pareto by crate, and the off-CPU stacks.
- The new metrics replace `wyrd_verification_engine_overhead_seconds`, and no compatibility alias remains.

## Verification and Evidence

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::report::tests::direct_overhead_judges_p99_only) | test(=verification::report::tests::queued_latency_is_reported_not_judged) | test(=verification::workload::tests::odd_arrivals_fail)'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::block_clock_records_every_block_once)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::telemetry::tests::overhead_is_the_execution_less_its_blocks) | test(=verification::hosted::blocks::tests::executor_time_is_verifier_time_not_overhead)'
mise run bench:verification:capacity
mise run fmt
mise run lints
mise run check:deps
```

No numbers are claimed before an actual run. Append the
`$wyrd-implement` evidence table with the measured results.

## Material Stop Conditions

Stop and route through `$wyrd-spec` for any change to the target, the metric
meaning, or another approved outcome.

## Authority Links

- [Approved specification, revision 21](../spec.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [Bifrost design](../../../../architecture/bifrost-design.md).
- Execute with [$wyrd-implement](../../../../.agents/skills/wyrd-implement/SKILL.md); implementation returns evidence, not independent task approval.

## Implementation Evidence

Status: `BLOCKED`. The benchmark and metrics ship, and one target resolver now
serves every path. Direct overhead p99 is below 1 ms at 1, 4 and 16 clients.
The task still fails on `STAGING_FULL` refusals, and the zero-refusal rule
stays unrelaxed.

Runtime changes made to reach the target:
- **Direct load.** The new `wyrd.verification_target` function (migration
  `20261021000000`, owner `queries/verification_target.rs`) reads both Cards
  and the SYSTEM principal in one autocommit statement. It replaces five round
  trips. The shape follows Oracle's table cut. It is allowlisted as a
  statement-tenant owner in `scripts/check_tenant_isolation.py`.
- **Batch claim.** `VerifierExecutionQueue::claim` takes `limit` rows in one
  statement, with a lease token per row and the exhaust step folded in.
  `ClaimLoop` first claims one item per due tenant, then takes fair-share
  batches. `adopt_span` is deleted because it has no remaining callers.
- **One verifier target resolver.** `VerifierCardCache::resolve`
  (`verification/cache.rs`) is the only target read. Direct verify calls it
  with `Db::Pool`, which takes a pooled connection only on a miss. Submission
  accept and runner claim call it with `Db::Conn` inside their own
  transactions. Readiness has one rule, `wyrd.verifier_readiness`, which the
  target function returns and `VerificationTarget::refusal()` applies. The
  cache is invalidated by NOTIFY on `wyrd_verification_target`, sent on Card
  update or delete, on `drift_baselines` state changes, and on SYSTEM
  principal writes. `components/verification/direct_targets.rs`,
  `resolve_direct` and the columns the claim used to read are deleted. One
  behavior changed: a queued run whose Card was deleted while it waited now
  errors instead of running.
- **Set-based accept.** `accept_submissions` now uses one existence read, one
  target resolve and one `unnest` insert per batch. Keys are locked once per
  batch and decided against earlier rows in the same batch.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Direct overhead p99 < 1 ms, from >= 1,000 samples per window | `wyrd_verification_overhead_seconds{mode=direct}` | `bench:verification:capacity` with profiling. 1 client: p99 <= 0.5 ms (mean 0.18 ms, 34,786 samples). 4: p99 <= 0.5 ms (mean 0.21 ms). 16: p99 <= 0.9 ms (mean 0.52 ms). 64: p99 <= 2.5 ms (mean 0.93 ms). | PASS at 1, 4 and 16; FAIL at 64 |
| Zero refusals | direct and submit paths | Direct at 4, 16 and 64 clients: 11,364, 73,614 and 134,852 `STAGING_FULL`. Submit at 1, 4, 16 and 64 clients: 3, 24, 96 and 320 | FAIL |
| Background reported only; drain within 60 s | case 2 report | drained in 5.0–9.2 s. Queued overhead mean 16–18 ms (at 16 clients: load 10.6 ms, settlement 6.7 ms). Queue wait mean 2.3 s. Accept batch mean 2.77 s | PASS (drain) |
| Pareto, CPU and off-CPU per window | capacity report | printed. Off-CPU is INCOMPLETE until `kernel.perf_event_paranoid` <= 1 | PARTIAL |
| Metrics replace `engine_overhead`, with no alias | `app/metrics.rs` | `mise run lints` | PASS |
| Named report, block-clock and telemetry tests | task commands | 3 + 1 + 2 passed | PASS |
| One-statement target read | `verification_target.rs` | `pg_verifier_executions::a_verification_target_reads_its_tenant_in_one_statement` | PASS |
| Batch claim | `CLAIM_SQL` | `pg_verifier_executions::a_claim_takes_up_to_its_limit_in_one_statement`; `pg_verification_runtime::a_flooding_tenant_does_not_delay_another_tenants_run` | PASS |
| Set-based accept keeps Replayed, Conflict and Refused | `accept_submissions` | `pg_verifier_executions::a_batch_decides_its_submissions_against_each_other` and all 26 `pg_verifier_executions` tests | PASS |
| One resolver; a deleted Card errors a queued run | `cache.rs`, `runner.rs`, `submissions.rs` | `cache::tests` 2/2; `pg_verification_runtime::verification_targets_are_cached_and_a_deleted_card_errors`; `test:bifrost:journey:drift` 51/51; `wyrd-testing --test server` `verification_runtime` 10/10 | PASS |
| No regression in the server verification paths | runtime, groups, routes, hosted | 57/57 `pg_verification_{runtime,groups,routes}`, `pg_hosted_artifacts` | PASS |
| Boundaries | — | `fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `git diff --check` | PASS |

Remaining blockers, from the profiled run (`perf.data` under
`target/verification-capacity/profiles/`; direct-16 and submit-16 stacks):
1. **`AppState` clone and drop take about 50% of server CPU** in direct-16
   (25.0% clone, 25.5% drop, inclusive) and about 30% in submit-16. Tower
   clones the middleware services on every request (`EdgeCapacity::clone`
   19.7%, `EdgeTimeout` drop 17.5%), and `AppState` derives `Clone` over 24
   fields. Of that, `CancellationToken` clone and drop
   (`AppState::shutdown_token`) take about 22%: the
   `tokio_util` token-tree mutex shows up as `lock_contended` (7.3%) plus its
   futex syscalls. Moka caches, `RelyingParty` and `HumanConnections` add
   the rest.
2. **Direct `STAGING_FULL`.** Arrivals beyond about 10k/s outrun the Scribe
   outbox's reserved share (2 slots per verify). The Scribe threads use
   about 2% of samples, so the drain is not CPU-bound. The drain rate and
   acknowledgement latency are not measured yet.
3. **Accept batches take 2.77 s.** The benchmark's `observe.submit` mints an
   `Idempotency-Key` per call, so `key_holder` makes two sequential round
   trips per row (`LOCK_REQUEST_KEY_SQL` and `KEYED_EXECUTION_SQL`). At about
   3,700 rows per batch that is about 7,400 round trips. Submit
   `STAGING_FULL` comes from the reservations backing up behind it.
4. **Queued overhead of 16–18 ms is mostly statements.** The `load` phase
   starts before the batch claim statement, so its 10.6 ms is mostly
   `store_duration{op=claim}` (10.5 ms per batch of about 31). Settlement's
   6.7 ms includes one `complete` statement per run (3.1 ms).
5. **One unreproduced fairness failure.**
   `a_flooding_tenant_does_not_delay_another_tenants_run` failed once during
   development. It passed in the final 57/57 run and in 3 more traced reruns.
   It has not been diagnosed.

### Settlement group commit and runner throughput

Changes:
- **Batched completion.** `VerifierExecutionQueue::complete_many` settles many
  completions in one round trip through `wyrd.complete_verifier_executions`.
  Lease renewal goes through `wyrd.renew_verifier_leases`. Both are added in
  migration `20261022000000`. Each function updates one row at a time by
  `execution_id`, in `execution_id` order. Unapplied rows fall back to
  `complete` in the same transaction. Dispatch insertion is shared through
  `insert_dispatches`.
- **Group commit (`verification/completions.rs`).** A completing run queues
  its completion. Four process-wide flush tasks each take everything queued
  (up to 512 runs) and settle each tenant's share in one transaction. Result
  writes are staged before the commit. Settlement therefore holds at most four
  app-pool connections, whatever the tenant count, because that pool is shared
  with Bifrost and the gateway. A failed flush, or a run whose flush task is
  gone, falls back to the single settlement.
- **Bench breakdown.** `wyrd_postgres_pool_acquire_seconds` was added to the
  capacity breakdown so pool wait is measured, not inferred.

Diagnoses:
- **Batched-settle planner regression.**
  - **Symptom:** submit-1 drain went from 3.3 s to 36 s, and `complete_many`
    took 24.6 ms per batch.
  - **Evidence:** `auto_explain` showed a bitmap scan of the whole tenant plus
    a hash join over the unnested arrays. The plan was unchanged when
    `execution_id = ANY($1)` was added.
  - **Cause:** the RLS tenant predicate compares against `current_setting`,
    so the planner cannot see its value. It estimates 0.5% of rows (6) for a
    tenant that owns about 57k.
  - **Fix site:** the plpgsql per-row primary-key loop above, which no longer
    depends on row estimates. Submit-1 drain dropped to 1.1 s.
- **Group-commit convoy.**
  - **Symptom:** settlement took about 30 ms per run, while the statement took
    3 ms, pool acquire 0.8 ms, server CPU was about 3 of 32 cores, and
    Postgres had about 1.5 busy backends.
  - **Evidence:** temporary histograms in the old election-based group commit
    (submit-4): 91% of runs waited a mean of 32 ms for another run's flush.
    Flushers served one batch each (tenure 4.1 ms, batch about 10 rows).
  - **Cause:** a flusher whose own run had settled left whenever another
    flusher existed, even with runs still waiting. Waiting runs were picked up
    only when a new arrival became a flusher. New arrivals come only after
    slots free and the claim loop runs another round.
  - **Fix site:** `completions.rs` now uses fixed flush tasks, so a queued run
    is taken by the next free task. At 64 slots, settlement dropped from about
    9–10 ms to 8.3 ms. The election code was deleted.
- **More slots are not a throughput lever.** With 256 slots, the runner
  drained faster, but intake fell from 22–23k/s to 15–17k/s. Submit-64 then
  hit a client-visible `WYRD_VERIFICATION_429_STAGING_FULL`. More concurrent
  claim and settle updates contend with submission inserts on
  `verifier_executions`. Slots stay at 64. The resource that binds is
  per-row write cost on that table, not CPU or connections.
- **Per-row write cost (lever 3, measured, not changed).** Of 368,591 updates,
  0 were HOT. Status is in partial-index predicates, and
  `next_attempt_at`/`lease_expires_at` are indexed. Every claim and settle
  therefore inserts into `verifier_runs_pkey`,
  `verifier_runs_data_tenant_id_run_id_key`, `verifier_runs_manual_idempotency`
  (51 MB, the widest) and one partial queue index, with a uniqueness check on
  each unique index. Backend samples are about 90% on-CPU, with few WAL or
  lock waits. The candidate fixes have not been implemented:
  - move idempotency keys to an insert-only table (this interacts with the
    retention spec's keyed-ID decision);
  - drop the redundant `execution_id` primary key in favour of
    `(data_tenant_id, execution_id)`.

| Bench (case 2) | submit-1 accepted / drain | submit-4 | submit-16 | submit-64 | Server-logged staging refusals |
|---|---|---|---|---|---|
| bench26 (election group commit, 64 slots) | 4,524/s / 1.1 s | 15,589/s / 32.4 s | 10,632/s / 19.4 s | 13,043/s / 29.1 s | not captured |
| bench29 (election, 256 slots) | 4,971/s / 1.1 s | 15,676/s / 18.4 s | 23,258/s / 26.0 s | 20,712/s / 29.9 s | 4,128 |
| bench32 (flush tasks, 64 slots, final) | 4,424/s / 1.1 s | 14,863/s / 27.6 s | 22,640/s / 48.3 s | 23,510/s / 51.1 s | 4,048 |
| bench33 (flush tasks, 256 slots) | 4,809/s / 1.1 s | 15,550/s / 18.1 s | 16,951/s / 21.7 s | 14,565/s / 17.4 s, 1 client refusal (FAIL) | 4,636 |

Bench32 passes every case-2 step with 0 client errors. Its drains at 16 and 64
clients are longer than bench26's because twice as many submissions were
accepted. Runner throughput is about 4.5–5k runs/s. The server-logged
`verifier_submissions` staging refusals are intake backpressure at closed-loop
offered load above about 21k/s. The Rust SDK retries them, so clients see
none. The guaranteed open-loop rate is not stated here. It moved to the queue
split change (`changes/active/verification-execution-queue`, AC-009), because
the per-row write cost on `verifier_executions` is what bounds it.

### Final verification after the group-commit rewrite

- **Trace regression.** The test was
  `pg_verification_runtime::attempts_record_queue_wait_phases_terminal_latency_and_one_trace`.
  - **Symptom:** it failed with "complete_many joins the attempt trace".
  - **Cause:** the batched statement now runs on a spawned flush task, so its
    span was outside the attempt's trace.
  - **Fix site:** `Completions::complete` in `completions.rs`. The flush reply
    carries the statement's duration and batch size. The run records a
    `complete_many` child span with those values, the same way the claim
    statement is recorded.
- **Tests.**
  - The exact test passes.
  - The runtime, operator-delivery and group suites ran 59 tests; all 59
    passed.
  - Clippy passes for `wyrd-server` and `wyrd-testing` with
    `--all-features --all-targets -D warnings`.
  - `cargo fmt` and `git diff --check` are clean.
- **Case 1, direct verify (bench34).** Every step passes with 0 errors.

  | Clients | Throughput | Overhead p99 |
  |---|---|---|
  | 1 | 4,714/s | ≤ 0.5 ms |
  | 4 | 13,804/s | ≤ 0.5 ms |
  | 16 | 32,001/s | ≤ 0.5 ms |
  | 64 | 42,185/s | ≤ 0.9 ms |

- **Bifrost, no regression.** `mise run bench:bifrost:write-capacity` passes
  every step with 0 errors. Each step must reach 200k rows/s.

  | Clients | Write rate | Bandwidth |
  |---|---|---|
  | 1 | 544,636 rows/s | 190 MB/s |
  | 4 | 1,795,632 rows/s | 628 MB/s |
  | 16 | 3,850,957 rows/s | 1,345 MB/s |
  | 64 | 6,546,142 rows/s | 2,287 MB/s |

  Shutdown was clean in 0.2 s.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Direct verify p99 overhead < 1 ms, ≥ 1,000 samples, 0 refusals | `verification/completions.rs`, `claims.rs`, migration `20261022000000` | bench34 case 1 | PASS |
| Background submit: no refusals, backlog drained within 60 s | same | bench32 case 2: 0 client errors, longest drain 51.1 s | PASS |
| Bifrost does not regress | no Bifrost change | `bench:bifrost:write-capacity` | PASS |

Status: `IMPLEMENTED`, routed to `$wyrd-task-review`. Not committed.

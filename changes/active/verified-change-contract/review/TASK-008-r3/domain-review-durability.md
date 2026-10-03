# Durability and process-lifecycle domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review/remediation evidence: `review/TASK-008-r1/` and `review/TASK-008-r2/`

`HEAD` resolved to the candidate before and after this review. CodeGraph is not
available in this checkout. Prior-review claims were treated as hypotheses and
rechecked against the requested cumulative range and current source.

## Reviewed boundary

This pass traced the durability- and lifecycle-sensitive paths changed by the
task:

- the absolute benchmark lifetime from preparation through measurement,
  client shutdown, replica termination, log retention, and report writing;
- migration, tenant-setup, server, and profiling child-process ownership,
  cancellation, termination, and reaping;
- request-task cancellation and the disposition of admitted client work and
  already-durable effects;
- durable Verifier-run, Scribe, audit-outbox, and Forge-demand backlog
  observation and the exact 60-second drain boundary;
- the cross-replica claim/fencing and tenant-fairness tests; and
- the AC-041 real-server durability and flat-client-byte journey.

The deployed path is one then two release `wyrd-server` replicas sharing
PostgreSQL and storage, driven through public Rust clients. The candidate does
not change production queue, claim, Scribe, audit, Forge, or SDK durability
semantics; it changes their benchmark/test evidence and lifecycle harness.

## Authority and source coverage

| Boundary | Authority and source evidence | Assessment |
|---|---|---|
| Whole-command deadline and cleanup | REQ-171 (`spec.md:1706-1713`); prior FIND-TASK-008-CLOSEOUT-2; `capacity/main.rs:79-188,227-343,397-452,500-531,621-637`; `release_server.rs:130-214,347-388,469-483,618-635`; `mise.toml:507-524` | **FAIL.** The async lifetime cannot preempt blocking migration/setup children; see DUR-R3-001. Cooperative expiry, client cleanup, server stop ordering, report failure, and abnormal server `Drop` are otherwise coherent. |
| In-flight request cancellation and partial progress | Bifrost structured-cancellation and durable-uncertainty rules; `capacity/load.rs:255-332,369-455`; `capacity/step.rs:303-381`; `capacity/main.rs:346-394` | PASS. `JoinSet` owns request tasks, dropping the lane aborts them, server-accepted effects remain, queued client observations remain owned until later client flush/shutdown or owner drop, and no partial step record is promoted as comparable evidence. |
| Scribe durable backlog | REQ-171 saturation SLO; `bifrost-design.md:109-153,298-327,834-868,896-918`; `capacity/evidence.rs:89-104`; `capacity/step.rs:383-420` | PASS. Persistence-queue, immutable-generation, and staged live-member gauges cover the handoff through durable unpublished staging. The focused test proves a staged member prevents a false zero. |
| Durable run, audit, and Forge backlog | REQ-171; repository audit and Forge authority; `capacity/evidence.rs:159-180,223-265`; `capacity/step.rs:353-380,383-420` | PASS. The benchmark reads production PostgreSQL authorities, bounds rows to work created/requested by the step stop, counts expected-but-not-yet-created runs, and does not substitute process counters for durable state. |
| Exact drain edge | REQ-171 backlog SLO; prior FIND-TASK-008-CLOSEOUT-9; `capacity/step.rs:239-261,383-420`; `report.rs` backlog test | PASS. Empty at or before 60 seconds passes; a nonempty read at 60 seconds or the first empty read after it fails. |
| Server and profiler child ownership | `release_server.rs:217-313,347-388,469-483`; `capacity/profile.rs:47-171`; Bifrost structured-cancellation rule | PASS apart from DUR-R3-001. Started server and `perf` children have owners whose normal or drop paths kill and reap them; server abnormal drop retains its log path. Migration/setup children are the uncovered exception. |
| Cross-replica durable claims and fairness | Revision-57 capacity/test split (`spec.md:1748-1756,2438-2451`); `pg_verification_runtime.rs:1267-1401`; production claim/lease/fence path | PASS within available evidence. The two-runtime tests share PostgreSQL, hold executions to expose duplicate claims/order, require both replicas to claim, require first-attempt settlement and exactly one durable summary, and derive fairness from persisted lease deadlines. |
| AC-041 durability and bounded client ownership | AC-041 (`spec.md:2364-2370`); `observe_run.rs` sustained journey; public `WyrdState`/Bifrost path | PASS within available evidence. The real-server journey uses the default queue, resubmits only all-or-none `QUEUE_FULL` refusals, proves flat and then zero owned bytes, and reads one 100-row `record_id` group per emitted observation. |

## Verification evidence and limits

Fresh execution on candidate `5c3bb79b3598abd88a3a234611fc400096adc975`:

```text
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
```

Result: **14 passed, 0 failed**. This covers the cooperative benchmark-deadline
test, staged-Scribe backlog aggregation, exact drain edge, resource-window
arithmetic, report/verdict behavior, and supporting pure benchmark logic.

Limits:

- The lifetime test uses a cooperatively pending async future. It cannot prove
  interruption, termination, or reaping of the synchronous operator children
  in DUR-R3-001.
- I inspected but did not rerun the PostgreSQL-backed two-replica claim and
  fairness tests or the AC-041 SDK journey. The r1 remediation records their
  passing exact commands; source inspection confirms they still exercise the
  production owners described above.
- The unmodified default benchmark required by
  `FIND-TASK-008-CLOSEOUT-13` was not run. Per the caller's sequencing
  direction, that empirical run is deferred until the other workstreams are
  integrated and is **not a blocker for this candidate**.
- A passing unit target establishes benchmark logic, not actual deployment
  capacity or recovery under a wedged child process.

## Material proposed finding

### DUR-R3-001 — INCORRECT: blocking setup subprocesses bypass the absolute benchmark lifetime

- **Violated obligation:** Revision-57 REQ-171 and prior
  `FIND-TASK-008-CLOSEOUT-2` require one enforceable 30-minute lifetime from
  setup through bounded cleanup. Expiry must stop later work, retain
  diagnostics, fail the report, and return through owned cleanup.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:152-188,311-314,397-429,639-680`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:130-190,618-635`.
- **Evidence:** `Lifetime::measure` wraps `Benchmark::measure` with
  `tokio::time::timeout_at`, which can cancel only when the measured future
  yields. `Benchmark::provision` awaits `LocalServer::start`, but that async
  function runs `wyrd-server migrate` and each `wyrd-server setup` through
  `run`, whose `std::process::Command::output()` synchronously waits for the
  child. A wedged child keeps the future inside one poll, so the Tokio timer
  cannot fire. The focused lifetime test supplies `std::future::pending`, a
  yielding/cancellable future, and never enters this real subprocess path.
- **Observable consequence:** A stalled migration or tenant setup can keep
  `mise run bench:capacity` alive beyond the approved ceiling without writing
  the required deadline failure report. When setup stalls after the first
  server starts, that server and the already-created tenant/durable state do
  not reach benchmark cleanup until the child eventually exits or an operator
  externally kills the command; the setup child itself has no benchmark owner
  capable of terminating and reaping it on expiry.
- **Required testable correction:** Keep the correction in the existing
  benchmark release-server harness. Give migration/setup subprocesses explicit
  ownership under the benchmark's remaining absolute deadline, and on expiry
  terminate and reap the active child before returning control to the existing
  report and `Benchmark::clean_up` paths. Preserve production request
  deadlines, server/queue durability semantics, and the existing server stop
  owner. Add a focused process-level proof with a controlled non-terminating
  operator child that demonstrates bounded expiry, child termination/reaping,
  server cleanup, retained diagnostics, and a reportable failure; the current
  async-pending test remains useful but is not sufficient.

## Overall result

**FAIL**

The durable backlog and exact drain remediations are faithful, and the focused
correctness/durability tests remain appropriately separate from the capacity
verdict. The candidate does not close the process-lifecycle part of prior
`FIND-TASK-008-CLOSEOUT-2`: a real blocking migration/setup child can defeat
the absolute deadline and prevent timely cleanup and reporting.

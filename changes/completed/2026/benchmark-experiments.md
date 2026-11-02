# Benchmark experiments

- Change: `SPEC-benchmark-experiments`, approved revision 24.
- Completed: 2026-10-31.
- Reviewed target: `4379acc295df2e0a955d89556ba4296a4b6a6330`.
- Base: `7b14ab9143ffe779803ba5df910dfe260e6dde5e`.
- Delivery: [PR #224 against main](https://github.com/bohmian-ai/wyrd/pull/224).
- Completion authority: the maintainer explicitly waived final
  `$wyrd-change-review` and authorized completion in place of a change-review
  PASS. No final change-review PASS is claimed.

Wyrd's Bifrost, Verification and Gateway benchmarks now use a shared experiment
lifecycle and attributable records. Engineers can read server latency, CPU cost,
throughput and correctness from one run, regenerate its report, and inspect
history without confusing a missed performance target with broken measurement.
This record preserves the delivered outcome and accepted limits; current
architecture remains authoritative.

## Delivered behavior

The opt-in `bench:l1` and engine-specific L1 commands measure isolated work;
L2 commands retain competing traffic, overload and recovery. Each measured step
warms up and runs three repetitions. Bifrost L1 has separate `writes-parquet`,
`hot` and `parquet` records. Smoke is requested separately and checks the actual
measurement and reporting plumbing without judging production targets.

Bounded server HDR histograms measure handler and overhead intervals independently
of operator trace export. Counts reconcile with each admitted cohort through
drain; server CPU, driver CPU and measured work stop at the window boundary.
Client timings remain diagnostic. Reports show percentile bounds and each
target as met, missed or unsettled, alongside work per provisioned vCPU, work
per server CPU-second and CPU per operation. Write records distinguish retained
Arrow bytes from OTLP wire bytes and include write-phase timing.

Schema-v3 records support one reader and a self-contained HTML history report.
Complete clean unprofiled full runs, including INVALID runs and target misses,
are retained on the separate `benchmark-records` branch. Partial, smoke, dirty
and profiled runs stay under `target/benchmarks/`. Unknown schemas are refused;
record publication cannot overwrite an earlier run. Obsolete thresholds,
benchmark aliases and duplicate measurement/report paths were retired.

Hot reads now exercise both memtables and staged Parquet. Before measurement,
the fixture teaches a span layout, stages half its rows and leaves half in
memtables. Explicit stage/publish controls belong only to the `bench` server
build and require platform administrator authorization. Parquet reads settle
their fixtures before measurement. The controls change placement outside
measurement windows, preserving production read and durability paths.

Verification inputs, detailed outputs and payload-free result summaries have
separate Bifrost owners. Submitted, scheduled and grouped work share canonical
input publication, their own Scribe ACK, per-tenant batched queue admission and
the distributed Bifrost-reading runner. Queue acceptance atomically advances
the expected schedule cursor or consumes the selected group generation; newer
activity survives delayed ACKs. Direct verification keeps immediate execution
and asynchronous archival without queue admission or an ACK wait.

Production tracing separates log filtering, sampled operator export and
authorized tenant recording. Bounded export failures remain observable without
blocking operations. Shared outbox dispatch and wake calculation use one clock
reading, closing a linger-deadline race that could strand pending writes.

## Lasting decisions and limits

Production authentication, tenant isolation, exact-result checking, acknowledged
durability and nonblocking audit remain measured work. PostgreSQL owns
coordination time. Benchmarks remain manual evidence outside CI gates; the
harness uses portable process accounting and plain child processes on Linux
x86_64, Linux arm64 and macOS arm64. Optional samply profiles are diagnostic.
Published summaries contain synthetic fixture facts and symbolic secret inputs.

VALID/INVALID describes measurement and integrity, with exits 0/2. Performance
target misses do not invalidate a run. The final implementation still enforces
the driver saturation threshold; its accepted Verification exception below
does not rewrite that verdict or remove the threshold.

Queued verification reuses concrete preparers, a closed admission enum and the
existing queue/lease engine. It adds group-generation coordination metadata,
not a preparation table, durable spool or second input loader. Publication and
queue acceptance are separate transactions: a crash can leave orphan input
records and repeat preparation. Group selection can see published inputs later
refused by admission. Source retention limits, asynchronous result loss and
at-least-once provider execution remain. Deployment requires draining old
schedulers/runners before revised generation writers accept work.

Approved revisions replaced trace-export-dependent timing with bounded HDR
measurement, separated smoke from full runs, removed baseline qualification
from run validity, increased the writes budget to 480 seconds, moved history
to its own branch and made hot placement deterministic. Revision 24 superseded
the temporary-table and alternate-runner proposals with ACK-gated shared
admission. The early plan and research describe earlier revisions; their
historical baseline exclusions do not supply missing current proof.

Profile-led work retained decoder/Variant and write-path improvements, parsing
SQL once per attempt, selected Parquet planning reductions and capture-local
filtering before logical reconstruction. Cross-query plan reuse stayed parked;
the Parquet access-plan handoff and page-recipe proposals were not retained.

**TASK-008 hot shredding did not complete.** Commit `07a157cb9` rolled its
implementation back to `471f69ff9` after five performance acceptance misses and
a red Bifrost verification lane. Its associated fork-pin, memory scan counters
and fixture repairs were also rolled back. Hot shredding is carried to a
follow-up change, without claiming its targets as delivered here. The next
attempt needs an end-to-end test of the real `COUNT(*)`/attribute predicate,
same-key and same-selectivity shredded/unshredded comparisons, and staged versus
memtable profiling. No follow-up identifier was supplied.

## Acceptance and evidence

The retired [specification](https://github.com/bohmian-ai/wyrd/blob/4379acc295df2e0a955d89556ba4296a4b6a6330/changes/active/benchmark-experiments/spec.md),
[plan](https://github.com/bohmian-ai/wyrd/blob/4379acc295df2e0a955d89556ba4296a4b6a6330/changes/active/benchmark-experiments/plan.md),
[research](https://github.com/bohmian-ai/wyrd/blob/4379acc295df2e0a955d89556ba4296a4b6a6330/changes/active/benchmark-experiments/research.md),
[tasks](https://github.com/bohmian-ai/wyrd/tree/4379acc295df2e0a955d89556ba4296a4b6a6330/changes/active/benchmark-experiments/tasks)
and [review evidence](https://github.com/bohmian-ai/wyrd/tree/4379acc295df2e0a955d89556ba4296a4b6a6330/changes/active/benchmark-experiments/review)
remain addressable at the reviewed target.

| Obligation | Recorded closure and limits |
|---|---|
| Shared experiments, timing, accounting, reports and history (REQ-001–016 as revised, AC-001–010/012/013) | Task evidence records lifecycle, arithmetic, cohort reconciliation, report readback, smoke and malformed-record checks. Later revisions supersede early schema and regression judgments. Final L1 records below establish current measurements; no fresh final-source L2 or optional-platform run is claimed. |
| Production tracing (REQ-017–020, INV-010, AC-011) | TASK-003 evidence and its R2 PASS at `71d2c240a` record independent filtering, exporter loss and quiet-policy tenant-negative journeys. TASK-004/005 record HDR calibration and all-engine smoke/readback; older FIX_REQUIRED reports remain historical, not a blanket final PASS. |
| Measured optimization and hot placement (REQ-021, REQ-023–034, AC-014) | TASK-006/007 evidence records measured retained/reverted changes and deterministic hot placement. TASK-008 was rolled back and deferred; final target misses below remain open performance work. |
| Shared ACK admission, atomic scheduling, distributed execution and three payload owners (REQ-035–037, AC-015–018) | TASK-009 records own-input ACK, rollback/generation, mixed-origin batching, missing-notification, tenant/input identity, stale-lease and payload-owner proof; Rust/Python/TypeScript journeys passed. Exact regressions retained four attempts, one scheduled wait and 14 group queries. R2 review at `b6e69d66b` passed bounded remediation only, with final gate/measurement then pending. |
| Final-source gate (REQ-038, AC-019) | TASK-009 closeout records `mise run -c -j 4 gate` at `92b62f9c0`, exit 0 in 1,318 seconds, every lane green, Bifrost 276/276. Logs were outside the tree. This is recorded execution evidence, not a completion-time rerun. |
| Final L1 and delivery (REQ-039, AC-019) | Five clean full schema-v3 records measured `92b62f9c0`; four VALID and Verification INVALID only for driver saturation. The maintainer accepted that driver limit in closeout. Thus the literal five-VALID-record outcome was not met. History commit `274cbfb83` was recorded as pushed; PR #224 targets main. |

Final L1 at `92b62f9c0d3c8de03fa0ec4ed05c53d5cb435e89` is retained in
[history commit 274cbfb83](https://github.com/bohmian-ai/wyrd/tree/274cbfb83).
All five runs finished within their budgets; all recorded correctness criteria
were met. Target judgments use the median of three repetitions, with HDR upper
bounds shown below; a single repetition can differ.

| Experiment / target | Final outcome |
|---|---|
| [Hot](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/bifrost/hot/20261031T130456Z-92b62f9c0.json), memtable point c64 p99 <5 ms | VALID, 220.0 s; **met**, 4.981 ms. |
| [Parquet](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/bifrost/parquet/20261031T130839Z-92b62f9c0.json), point c64 p99 <5 ms | VALID, 285.4 s; **missed**, 11.698 ms. |
| [Writes](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/bifrost/writes-parquet/20261031T125731Z-92b62f9c0.json), OTLP peak ≥30 wire MiB/CPU-s; native ≥200k rows/s | VALID, 440.9 s; spans **missed**, 15.852 MiB/CPU-s; logs **met**, 43.230 MiB/CPU-s; native **met**, 2,206,178 rows/s. |
| [Verification](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/verification/direct-verify/20261031T131328Z-92b62f9c0.json), c64 overhead p99 <0.5 ms | **INVALID**, 69.3 s, solely `driver.saturated`: repetitions 1/3 used 6.54/6.47 cores against 6.40. Recorded overhead target **met**, 0.318 ms; no errors, wrong verdicts or lost evidence. Maintainer-accepted exception, not a VALID replacement. |
| [Gateway](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/gateway/overhead/20261031T131441Z-92b62f9c0.json), c64 overhead p99 <0.5 ms | VALID, 195.0 s; 1 KiB buffered **met**, 0.491 ms; streamed head/end **met**, 0.239/0.234 ms; 64 KiB metadata **missed**, 1.877 ms; 64 KiB payload **missed**, 156.762 ms. |

Earlier L2 evidence at `02f55beb8` recorded Verification and Gateway passing and
Bifrost INVALID under contention; those are historical results, not final-source
qualification. Later scope concentrated on L1. Real macOS arm64/Linux arm64 runs
were optional and are not claimed. The maintainer's final-review waiver closes
this packet without asserting that every original performance or qualification
outcome succeeded.

## Current owners

- [Benchmark commands, methodology and history](../../../benchmarks/README.md),
  [testing lanes](../../../TESTING.md),
  [shared experiment owner](../../../crates/wyrd/wyrd-testing/src/experiment.rs)
  and [bounded server timing](../../../crates/shared/wyrd-telemetry/src/timing.rs).
- [Verification contracts and execution](../../../architecture/wyrd-design.md#verifier),
  [Bifrost storage/visibility and Variant authority](../../../architecture/bifrost-design.md),
  [security and audit](../../../architecture/wyrd-security-posture.md), and
  [operator tracing](../../../architecture/operations/reliability-and-recovery.md).
- [ACK admission owner](../../../crates/wyrd/wyrd-server/src/verification/submissions.rs),
  [Bifrost input contract](../../../crates/vala/vala-bifrost-redux/src/tables/verification/execution_inputs.rs),
  [runtime integration tests](../../../crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs),
  [group integration tests](../../../crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs)
  and [public verification journeys](../../../crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs).

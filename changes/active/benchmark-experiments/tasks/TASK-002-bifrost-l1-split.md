---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 6
requirements: [REQ-008, REQ-009, REQ-013, AC-001, AC-007]
depends_on: [TASK-001]
parent_task:
remediates: []
---

Superseded by the proposed revision-8 tasks once that revision is approved.
Preserve this task and its evidence as history; do not execute its conflicting
raw-span or older matrix instructions alongside the replacement packet.


## Outcome and Value

Bifrost Level 1 becomes three short experiments, each with its own record,
verdict, and budget: `writes`, `hot`, and `parquet`. Before this change, one
`isolated` experiment ran every read and write at 1/4/16/64 clients and
overran its 15-minute budget. After it, `bench:l1:bifrost:writes`, `:hot`,
and `:parquet` each finish within 6, 5, and 9 minutes, and between them they
cover hot reads, Parquet reads, span Bloom and shredding behavior, and all
three write paths.

## What Changes and What Must Stay True

- `writes` measures native, OTLP spans, and OTLP logs writes at 1 and 64
  clients. Nothing waits for publication. Each path's storage ratio is
  reported as inapplicable, with a reason pointing at `parquet`.
- `hot` runs with the earlier hot tier's rotation settings, so every row
  stays in a memtable. A staged or published row makes the run INVALID. It
  measures events point lookups at 1 and 64 clients, a small aggregate at 1,
  and a shredded-attribute span filter at 1.
- `parquet` takes one 200,000-row storage sample per write path, loads 10M
  events and 1M spans, and settles once.
  - It measures events point lookups at 1 and 64 clients, a small aggregate
    and a full scan at 1, a `trace_id` lookup at 1 and 64, and shredded and
    long-tail attribute filters at 1.
  - Shredding and span-time pruning are each checked once after settlement.
  - A settled query that lacks its scan facts makes the run INVALID.
- Scaling is reported only for read classes measured at both 1 and 64.
- Point p99 < 5 ms at 64 clients is the only latency criterion.
- The smoke no longer waits for publication.
- L2 calibrates from the newest compatible `writes` record. L2 behavior is
  otherwise unchanged.
- Out of scope: Verification, Gateway, and L2 workloads.

## Approach

1. Add an `Experiment` selection to the Bifrost plan: `capacity l1 bifrost
   <writes|hot|parquet>`. The bare form runs all three, each under a fresh
   Postgres.
2. Build one declaration per experiment from shared read and write step
   builders, each with its own budget.
3. Port the span questions and the shredding and pruning judgments from the
   earlier telemetry journey.
4. Give the workload a per-experiment prepare: no fixture for `writes`, an
   unsettled load for `hot`, and samples, load, and settle for `parquet`.
5. Update the mise tasks, README, L2 calibration source, and declaration
   tests.

## Ordered Implementation Scenarios

### Scenario 1 — Each experiment declares only its own coverage

**Behavior.** Each experiment declares only the steps the spec lists, at 1
and 64 clients only.
- `writes`: write steps and peaks; storage ratio inapplicable.
- `hot`: events point, small aggregate, and span shredded filter.
- `parquet`: the hot reads plus full scan, trace lookup, and long-tail
  filter.

The point SLO and peak floors keep their limits. Baseline criteria name only
eligible entries with the same case and client count.

**RED.** Adapt `level_one_declares_complete_engine_coverage` to expect the
three declarations. It fails, because only `isolated` exists.

**GREEN.** Add the `Experiment` declarations.

**REFACTOR.** Share the step and baseline builders.

### Scenario 2 — Span questions are exact and the settled checks judge

**Behavior.**
- Each span question's expected answer is computed from the fixture's span
  functions.
- The shredding judgment fails when no file is shredded, when a file misses
  a core or mid key, or when a file shreds `retry.count` or a `custom.*` key.
- The pruning judgment fails when the window scanned no fewer files and no
  fewer row groups than the full table.

**RED.** Add unit tests for the judgments and the long-tail presence. They
fail until the code is ported.

**GREEN.** Port the telemetry questions and judgments.

**REFACTOR.** Generate the tail keys once, for both the span builder and the
answers.

## Acceptance Criteria

| Obligation | Observation |
|---|---|
| REQ-008 coverage and budgets | Declaration test lists every step and budget of the three experiments |
| REQ-008 hot residency | A `hot` run records no staged or published rows; otherwise INVALID |
| REQ-008 settled checks | `parquet` judges `span-shredding` and `span-pruning` |
| REQ-009 calibration | L2 reads `benchmarks/bifrost/writes/` records |
| AC-001 | Each of the three real runs completes within its budget |

## Verification and Evidence

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`
- `mise run fmt && mise run lints`
- `mise run bench:smoke`
- `mise run bench:l1:bifrost:writes`, `:hot`, `:parquet`

## Material Stop Conditions

- A `hot` server cannot hold the fixture in memory on the reference host.
- The span Variant `->>` filter is unsupported on hot data.

## Authority Links

- `changes/active/benchmark-experiments/spec.md` revision 6
- `AGENTS.md`, `TESTING.md`

## Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Three experiments, own records and budgets | `query/mod.rs` `Experiment`, `l1_declaration` | `main::tests::level_one_declares_complete_engine_coverage`; 32/32 capacity tests | PASS |
| Smoke without publication wait | `query/mod.rs` `smoke` | release smoke for writes, hot, parquet | PASS |
| `writes` isolated per path | `isolated.rs` `closed` settles before each later path's first warmup; native runs last | `benchmarks/bifrost/writes/20261024T190917Z-3e1718b02.json`: valid, 372 s, every baseline passes | PASS |
| `hot` rows stay in memtables | `isolated.rs` `hot` | `benchmarks/bifrost/hot/20261024T191549Z-3e1718b02.json`: valid, 190 s | PASS |
| `parquet` shredding, pruning, storage ratio, scan facts | `telemetry.rs` judgments, `isolated.rs` `settled` | `benchmarks/bifrost/parquet/20261024T191916Z-3e1718b02.json`: valid, 393 s; 13/13 files shred the 13 expected keys; window scanned 1 of 13 files | PASS |

Diagnosis, `writes` CPU contamination. Symptom: `otlp-spans-c1` 173 µs CPU/row against a 20 µs baseline. Evidence: 35.7 GB server RSS during that step, left by `native-write-c64`; the same step alone on a fresh server measured 61,672 rows/CPU-s. Cause: one server ran every write step back to back, so each window's server CPU paid the previous step's publication backlog. Fix site: `Isolated::closed`, which waits for earlier rows to publish before each later path's first warmup, outside every window. An independent read-only diagnosis agreed.

Deviations awaiting approval:
- The `writes` budget is 8 minutes, against 6 in the spec. Each settle has a floor of about 62 s, because the 30 s retention applies once to a memtable generation's age and again to a staged member's assembly.
- The spans storage ratio comes from the 1M-span fixture, not a 200,000-row sample.

Measured FAILs, criteria unchanged:
- `writes`: `otlp-spans-peak-floor` 16.7 against ≥30 wire MiB/CPU-s. The approved spans baseline is 16.5.
- `hot`: point c1 and small-aggregate c1 queries/CPU-s are 18–20% below their historical baselines, with throughput unchanged. Point c64 p99 is 7.8 ms against the <5 ms SLO.
- `parquet`: point c1, point c64 and small-aggregate c1 queries/CPU-s are 21–29% below their historical baselines. Point c64 p99 is 14.4 ms against the <5 ms SLO. Not diagnosed.

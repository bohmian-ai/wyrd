---
id: TASK-006-R2
kind: remediation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 19
requirements: [REQ-004, REQ-005, REQ-008, REQ-011, REQ-012, REQ-019, REQ-020, REQ-023, REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, INV-005, AC-002, AC-003, AC-012, AC-013]
depends_on: [TASK-006]
parent_task: TASK-006
---

# Execute the revision 19 supplement

## Outcome and value

Execute the items in the spec's `## Revision 19 supplement` section, in order.
A run is VALID or INVALID only, with targets reported. Each record stands on
its own with per-core relative metrics. After that, the measured write and read
costs are reduced one at a time. The spec stays authoritative: if it and this
file disagree, the spec wins and this file is corrected. The items below are
copied from that section of the spec.

The maintainer approved this execution scope on 2026-10-27: a benchmark that
runs and collects correct metrics is successful; benchmarks are outside CI and
do not gate performance. The numbered order below is the execution order.
Items 1–5 make every run's own record trustworthy and complete before
optimization. The maintainer approved items 1, 3 and 10 explicitly; item 10's
design is shown to the maintainer before its code. Existing
requirements own changed behavior; new IDs below cover additional outcomes.

## 1. Valid or invalid only

**Intent and evidence.** A slower result is useful evidence, not a broken
benchmark. The maintainer explicitly approved successful runs that collect
correct metrics and rejected performance gating because benchmarks are not CI.

**Requirement.** Apply REQ-004, REQ-005, REQ-008, REQ-011 and REQ-012 in every
reader, report and command. VALID/0 requires trustworthy complete measurement
and every full-run correctness and integrity criterion in every repetition;
otherwise report INVALID/2 with reasons. Nothing exits 1. Median performance
targets remain per-metric observations, including `unsettled`, never exit
conditions. `backlog-bounded` and
`bytes-bounded` are targets. Update `mise.toml` descriptions and
`benchmarks/README.md`; remove the claim “fails (exit 1) on a declared criterion”.

**Acceptance.** AC-002, AC-003 and AC-012 show that a missed target and an
unsettled target each leave the run VALID with exit 0, and that each integrity
or measurement failure gives INVALID/2. Both L1 aggregates
return the worst of 0/2 and retain the individual results. Reports and task
and README wording agree. No benchmark becomes a CI or verification gate.

**Order/dependency.** First; items 2–4 follow.

## 2. Writes budget

**Intent and evidence.** Two clean writes-parquet runs took 311.3 and 311.2
seconds on commits f96c23fa2 and d86cf238e against 300 seconds. Both are always
INVALID under that budget, and native-write-c64 loses a repetition.

**Requirement.** REQ-008 gives writes-parquet 480 seconds: the 433 seconds
measured on 434aaf5c1 with item 4's added path, whose own publication settle
takes about 62 seconds, plus margin. It retains 300 seconds for hot and
parquet, and updates the Bifrost L1 aggregate maximum to 1,080 seconds. Keep the same clock boundaries and full coverage; do not shorten or
omit repetitions to fit.

**Acceptance.** A complete writes-parquet record contains every repetition,
including native-write-c64, and reports its actual elapsed time against 480
seconds. The aggregate's stated maximum agrees with its component budgets.
An actual overrun still invalidates rather than truncating evidence silently.

**Order/dependency.** After item 1; before item 4.

## 3. History location, and profiled runs never enter it

**Intent and evidence.** Profiled writes-parquet records
`20261027T134553Z-f96c23fa2.json` and `20261027T140216Z-f96c23fa2.json`, and
matching profiled hot and parquet records from those runs, entered history
contrary to its complete, clean, unprofiled rule; they are already deleted.
Records committed on the code branch also made code review carry about 264,000
lines of run data, so history now lives on the `benchmark-records` branch. That
branch already holds every earlier eligible record, and the code branch no
longer tracks any.

**Requirement.** Enforce REQ-013. The recorder writes history only under
`benchmarks/history/<engine>/<experiment>/`, and every reader of history uses
that same directory: the `capacity results` page, the L2 calibration that reads
the newest compatible `writes-parquet` record, and their tests. No reader
falls back to the old `benchmarks/<engine>/<experiment>/` location. The
recorder writes profiled runs only under `target/benchmarks/`, regardless of
validity or completeness. `benchmarks/README.md` explains the
`benchmark-records` worktree and how to keep a record. This is correction of
ineligible history, not permission to delete slow results.

**Acceptance.** An eligible full run writes its record under
`benchmarks/history/`, and `capacity results` and L2 calibration read it from
there. A profiled full execution creates no history record. Ordinary eligible
full runs still enter history regardless of target misses. History on
`benchmark-records` contains none of the identified profiled records.

**Order/dependency.** After item 2.

## 4. Realistic span fixture

**Intent and evidence.** Current OTLP spans are about 339 wire bytes each,
against 1,253 for logs. Per-row server costs are similar: about 48k spans/s per
server core against 36k log records/s per core. At 64 clients this reads as
15.5 wire MiB/s per core for spans against 43.3 for logs. Payload size must be
visible before interpreting that throughput difference.

**REQ-023 — Representative span coverage.** Keep the small-span steps and add
a second deterministic OTLP span fixture of about 1 KiB of wire per span,
including resource attributes, a dozen span attributes and events. Measure it
beside small spans in writes-parquet at 1 and 64 clients. Both sizes report
actual wire bytes per span, rows/s per server core and wire MiB/s per server
core with explicit denominators. Keep fixture identities distinct so a comparison
never pairs different payloads.

**Acceptance.** The full record and report distinguish small and approximately
1 KiB spans at both client counts, preserve exact results and report both row
and byte throughput from measured wire sizes. Smoke exercises both fixtures.
The expanded matrix completes within the item 2 budget.

**Order/dependency.** After item 3; before item 5.

## 5. Self-contained records

**Intent and evidence.** Every record already stores its measurements, host,
build, commit and controls, and its report already prints relative metrics
such as 43.3 wire MiB/s per server core for logs. Yet every run reports "no
reviewed reference" because REQ-011 gated reports on a hand-reviewed
`baselines.toml` that was never filled.

**Requirement.** REQ-011 and REQ-012 as amended. Remove `baselines.toml`,
`--qualify-baseline`, `capacity reference` and every regression criterion and
the code that reads them. The headline relative metrics are wire MiB/s per
server core, uncompressed MiB/s per server core, rows/s per server core, CPU
per row or query, and server and client p50/p95/p99.

**Acceptance.** AC-002 and AC-013. The next VALID run of each experiment reports
no reference-related reason and is complete on its own.

**Order/dependency.** After items 1–4; before items 6–12.

## 6. Per-phase write timing

**Intent and evidence.** At one client a write costs 11–14 ms server-side:
native 14.2 ms p50, OTLP spans 10.8 ms, logs 12.1 ms. Current timing exposes
one `bifrost.gate.write` or `bifrost.otlp.*` span and cannot locate that cost.

**REQ-024 — Write-phase evidence.** Record decode, validation, WAL append,
WAL sync and acknowledgement timing, plus shard wait under concurrency, from
the same server HDR timing used by Verification. Include phase populations and
boundaries so shared work, waiting and overlapping phases are explicit; do not
sum overlapping timings as an end-to-end latency. Retain the phase timing in
the record and make it readable without external profile artifacts.

**Acceptance.** Native, span and log records at 1 and 64 clients expose the
applicable phase distributions with correct populations, labelled unavailable
phases and no fabricated samples. Controlled timing and record/readback proof
exercise the real collector and report. End-to-end timing remains unchanged.

**Order/dependency.** After item 5; before the tail diagnosis in
item 9. Use it to explain subsequent write changes.

## 7. Span decode CPU

**Intent and evidence.** Candidate costs are repeated parsing and validation.
Prior estimates, not measurements, assign 10–13% of span CPU to message passes,
and about 8% to repeated Variant validation. The Gate's logical identity is
already reused by Scribe preprocessing when known, yet SHA-256 still takes about
1.8% of write CPU in the 2026-10-27 profile from an unidentified caller.

**REQ-025 — Measured write CPU reduction.** Measure these changes individually
before and after each change, in the following order: (a) a single-byte fast path
in `WireFields::read_varint` in
`crates/wyrd/wyrd-server/src/otlp_decode.rs`; (b) avoid fully re-validating
server-built Variants in `finish_variant` in
`crates/vala/vala-bifrost-redux/src/tables/signal.rs`, which currently passes
them through `EncodedVariant::from_bytes`, while preserving size and depth
limits exactly; (c) use two passes per OTLP message instead of 7–10, preserving
exact capacity reservation, last-wins semantics and every malformed-input
check; (d) attribute the remaining SHA-256 cost to its caller from the profile
and remove it only if it repeats a digest already computed for the same bytes.
Keep external validation and identity semantics unchanged. Item 8 extends this
same measured CPU-reduction requirement; all memory accounting remains exact.

**Acceptance.** Each change has its own comparable before/after result for
CPU per row, row and wire throughput and latency, with correctness proof for
its affected malformed, boundary and duplicate-field behavior. No estimate is
presented as measured improvement; no combined result hides an individual cost.

**Order/dependency.** After item 6; implement and measure (a)–(d) one at a time.

## 8. Write CPU from the 2026-10-27 profile

**Intent and evidence.** The writes-parquet profile on f96c23fa2 attributes
4.7% inclusive to `scribe::memory::unshared_arrow_bytes`, which calls
`Array::to_data` per array per batch; 1.1% to the per-enqueue gauge lookup in
`scribe::execution_lanes::record_lane_enqueued`; 0.8% to `mkdir` under
`encode_runs`; and 3.9% of server CPU to rayon worker idle spin (`__sched_yield` under
`wait_until_cold`).
Idle CPU directly lowers every per-core result.

**Requirement.** Under REQ-025, reduce these four costs individually without
changing memory-accounting values, ownership, lifetime or admission behavior.
Keep WAL, staging, filesystem error handling and worker progress guarantees.

**Acceptance.** Each change has a separate comparable measurement and profile
showing its cost, plus focused proof that retained/shared/released memory is
still charged exactly and queues still progress and drain. Report observed
changes, including no improvement; percentages above are diagnostic evidence,
not promised savings.

**Order/dependency.** After item 7, one change and measurement at a time.

## 9. 64-client write tail

**Intent and evidence.** On d86cf238e, client p99 at 64 clients is 170 ms for
spans, 289 ms for logs and 615 ms for native writes.

**REQ-026 — Diagnose write tails before changing code.** Use item 6 phase
timing to locate the cause of each tail, separating client duration, shard
wait and server work. Choose a change only after the timing supports its cause;
preserve durability, ordering and exact accounting.

**Acceptance.** Retain phase evidence explaining each affected path and a
causal diagnosis before a fix. Measure any resulting change separately, before and after, showing client/server tails and CPU rather than hiding queued work.

**Order/dependency.** After item 8; depends on item 6 timing.

## 10. Fix 4: memtable Variant shredding

**Intent and evidence.** The shredded-attribute span filter takes 50.4 ms client
p99 from the memtable (hot), against 17.7 ms from Parquet.

**REQ-027 — Design approval before memtable shredding.** Produce a separate
design section here and obtain maintainer approval before implementation. The
design must state what is shredded, when, how memory is charged, how reads use
it, and equivalence with Parquet shredding. The maintainer approved this
item on 2026-10-27; the design is written here and shown to the maintainer
before code changes.

**Design questions.** Current authority in
`architecture/bifrost-design.md`, Storage format and Variant, requires the
unshredded metadata/value Arrow representation at the wire. Parquet uses
metadata, residual value and typed leaves, with adaptive per-file layout;
recovery-stage runs stay unshredded. Oracle uses the existing DataFusion Variant
semantics, not a Wyrd-specific Variant function or planner. Current Scribe
`MemtableBucket` retains Arrow batches and a shared `RetainedArrowCharge`;
Oracle `MemtableSource` reads those captured batches and keeps their accounting
owners alive. These facts do not settle a live shredded representation or its
construction policy. The design must answer:

- Which Variant columns and literal paths get typed leaves, how keys and types
  are selected, and how residual, missing, null, nested, list and mixed-type
  values remain exact. Does the layout reuse Parquet's selection policy or
  require an architecture amendment?
- When construction occurs for active appends and immutable generations, how
  concurrent captures see a coherent layout, and whether work adds to write
  acknowledgement or query latency.
- Which owner charges original Arrow buffers, typed leaves, residuals,
  metadata and construction scratch, how shared captures avoid double charges,
  and when retirement releases memory under existing admission limits.
- How local and remote live reads use the same DataFusion semantics while
  preserving the canonical wire schema, projection by field id, fallback to
  residual values and whole-column results. Can existing owners provide this
  without a new planner, adapter or persisted format?
- How hot, staged and published reads prove equal types and values, including
  mixed layouts, malformed boundaries and tenant-negative cases, and how
  ingest CPU/memory costs and filter-tail benefits will be measured.

**Acceptance.** The design resolves these questions with source and authority
evidence, states any required owning architecture amendments, and is shown to
the maintainer before code changes. Subsequent implementation proves exact
hot/Parquet equivalence and accounting, then measures the filter and write cost.

**Order/dependency.** After item 9. The written design is a dependency for
this implementation only.

## 11. Parquet tails at 64 clients

**Intent and evidence.** Parquet point lookups have 16.1 ms client p99 and span
trace reads 26.1 ms at 64 clients against the 5 ms goal. Keep client diagnostics
and the server-latency goal distinctly labelled.

**REQ-028 — Profile-led Parquet tail work.** Analyse the captured profile of
the 2026-10-27 parquet run before choosing a fix. Identify the work or waiting
that explains the tail; preserve read correctness, tenant scope, source cuts
and memory limits. A missing or inconclusive profile is a stated evidence gap,
not permission to infer a cause from the p99 alone.

**Acceptance.** Record the profile's supported causal explanation before a
chosen change, then measure that change before and after with separate
client/server p99 and CPU/query. Targets stay unchanged and remain advisory.

**Order/dependency.** Independent of items 1–10; the profile analysis may run
at any time. This analysis decides whether item 12 can reopen.

## 12. Cross-query plan reuse is parked

**Intent and evidence.** Planning is no longer a hotspot after per-query
planning; cross-query reuse would add complexity without measured need.

**Requirement.** Under REQ-021 and REQ-028, park the remainder of fix 5.
Reopen cross-query plan reuse only if item 11's profile shows planning as a
material cost. A performance target miss alone does not reopen it.

**Acceptance.** No cross-query reuse is implemented without that profile
evidence and a stated resulting scope. If planning is absent, record that the
work remains parked.

**Order/dependency.** Last; conditional on item 11's profile analysis.

## Already landed and measured context

These are context, not new work: gRPC listeners disable Nagle in d86cf238e;
one-client p50 fell from 42.0 to 16.0 ms for native and 44.0 to 13.4 ms for
logs, and one-client write throughput rose 1.6–3.1×. The Parquet byte-range
cache landed in f96c23fa2, path-only Variant validation in 432d186bb, and
Kafka-style Scribe lane linger with a byte-bounded outbox in edc1f2eac. Preserve
these changes and measure subsequent work separately.


## Implementation evidence

### Diagnosis: item 4 smoke, unclassified 1 KiB spans

- **Symptom:** the `bench:l1:bifrost:writes -- --smoke` run on 80167c4ed
  (`target/benchmarks/smoke/bifrost/writes-parquet/20261027T164637Z-80167c4ed.json`)
  was INVALID: `otlp-spans-1k-c1` and `-c64` had no `bifrost.otlp.traces`
  population.
- **Evidence:** the reasons read "143 requests declared an unknown benchmark
  case or phase" and "0 `bifrost.otlp.traces` timing observations of case
  otlp-spans-1k". Every other step was classified.
- **Cause:** the server classifies only cases in the closed set
  `wyrd_telemetry::timing::CASES` (`crates/shared/wyrd-telemetry/src/timing.rs`),
  which did not contain `otlp-spans-1k`.
- **Fix site:** that closed set, the one owner every engine's labels pass
  through. The new `query::tests::every_benchmark_case_is_declared_to_the_server`
  checks every write and read case the capacity driver declares (4bab4d7ff).

### Item 11: profile analysis of the 2026-10-27 parquet run

- **Profile:** `../benchmark-experiments/target/benchmarks/bifrost/parquet/20261027T140911Z/profiles/`.
  - Its recorded binary (build id `b5791094…`) was since rebuilt, so the
    profile was symbolicated against the rebuilt one by per-region address
    offsets, accepted only where the return address follows a call
    instruction (99.98% on an exact-match control, 7% unshifted).
  - 5.3–5.5% of leaf samples stay unsymbolicated.
  - The samples were segmented per step from the record's timing `taken_ns`
    windows: point-c64 is profile seconds 25.5–39.5, span-trace-c64 is
    136–150.
- **Supported explanation:** CPU saturation. In both c64 windows the server
  is on-CPU on 29 of 32 cores, 99.4% of it in tokio workers.
  - CPU per query: about 4.3 ms (point) and 7.4 ms (span).
  - Server p50/p99 is 6.1/13.1 ms (point) and 13.4/24.2 ms (span), against
    2.1/3.2 and 3.2/4.4 ms at one client: queries wait for cores.
  - Lock self time is under 2%. Off-CPU waiting is not sampled, which is a
    stated gap.
- **Largest costs (inclusive):**

  | Cost | point-c64 | span-trace-c64 |
  |---|---|---|
  | ZSTD page decompression (`decode_page`) | 31.8% | 13.2% |
  | Row-filter evaluation | 40% | 10.6% |
  | `skip_records` decode-then-skip | 13.9% | 6.7% |
  | Row-group statistics pruning | — | 10% |
  | Bloom filter loading | — | 6.2% |

  Allocator self time is 16% / 23%.
- **Planning (item 12):** `create_physical_plan` is 8.5% / 9.0%, of which
  2.6% is Iceberg `Table::scan` → `available_parallelism` per query.
  `SessionState::optimize` is 4.8% / 7.3% and SQL parsing about 1.5%.
  - DataFusion planning is about 10–13% of server CPU, from a subtraction
    estimate (the exact breakdown pass did not finish).
  - Planning is therefore a material cost, but a smaller one than page
    decode.
- **Evidence gaps:** the 025401Z span-trace-c64 window was not aggregated,
  and the planning share above is a subtraction estimate.
- **Chosen direction:** reduce decompression per lookup first. The byte-range
  cache holds compressed pages, and data pages use the default size
  (`parquet/writer_properties.rs`). No change has been made or measured yet.

### Diagnosis: writes-parquet over its 420-second budget

- **Symptom:** the clean `bench:l1:bifrost` run on 434aaf5c1 had
  writes-parquet INVALID at 432.7 s of 420 s, and native-write-c64 lost
  repetition 3.
- **Evidence:** the per-step timing snapshots show about 235 s of measured
  windows and warmups. Before each of the three later paths' c1 warmups
  there is a gap of 61.4, 62.5 and 62.5 s.
- **Cause:** `Residence::settle` waits until every earlier row has
  published, which takes about 62 s under the 30 s retention whatever the
  backlog. Item 4's `otlp-spans-1k` added a fourth path and so a third
  settle. The 420 s budget covered only that path's windows, not its settle.
- **Fix site:** the REQ-008 budget, raised to 480 s (Bifrost aggregate
  1,080 s), by maintainer decision on 2026-10-27, in eaa8ad209. Shortening
  or skipping a settle was rejected because it would break per-path CPU
  isolation.

### Acceptance evidence, items 1–5

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. VALID/INVALID only; targets reported | 466805448 | 60 capacity/experiment unit tests; parquet record `20261027T171900Z-434aaf5c1` VALID with `point-p99-slo` missed (11.2 ms) | PASS |
| 2. writes-parquet completes every repetition within budget | ce6a6f188, eaa8ad209 (480 s) | `writes-parquet/20261027T173358Z-eaa8ad209`: VALID, 420.1 s of 480 s, native-write-c64 3/3 repetitions | PASS |
| 3. History only for clean full runs | 7d639b0b1 | V7 publish-destination test; the three records published to `benchmarks/history` and committed on `benchmark-records` (d7426b338) | PASS |
| 4. Small and ~1 KiB spans at 1 and 64 clients with wire B/row, rows and wire MiB per core | 5dc650807, 4bab4d7ff | `wide_spans_carry_about_one_kib_with_distinct_ids`; `every_benchmark_case_is_declared_to_the_server`; the eaa8ad209 record reports `otlp-spans` and `otlp-spans-1k` c1/c64 with `wire_bytes_per_row`, `rows_per_cpu_s` and `wire_mib_per_cpu_s` | PASS |
| 5. Records self-contained; no reference reason | 466805448, 80167c4ed | hot `20261027T171701Z-434aaf5c1` and parquet records VALID with no reference reason; `reports_regenerate_from_persisted_records` checks server and client latency in each row | PASS |

### Item 6 phase population diagnosis

- **Symptom:** the first item 6 run (`20261027T180214Z-074033fa1`, VALID)
  reported 856 `bifrost.scribe.write` samples in `native-write-c1`
  against 524 requests (logs c1: 516 against 330).
- **Evidence:** the window's timing snapshots
  (`target/benchmarks/bifrost/writes-parquet/20261027T180214Z/timings/native-write-c1/repetition-1.json`)
  hold 525 `native-write`/`repetition-1` observations (reconciled 525 = 525)
  and 331 more labelled `none`/`none`.
- **Cause:** `set_write_phases` read `Population::elapsed`, which merges
  every case and phase, so the server's own unlabelled Scribe appends were
  mixed into the step's phases.
- **Fix site:** `set_write_phases` (`query/isolated.rs`) now reads
  `Population::only(case, phase)`, the existing filter that L2 uses; the test
  `write_phases_read_back_with_populations_and_labelled_gaps` records an
  unlabelled append and proves it is excluded. Gate and OTLP boundary readings
  are reconciled by cohort and were unaffected.

### Item 6 acceptance (REQ-024)

Before: `writes-parquet/20261027T173358Z-eaa8ad209`. After:
`writes-parquet/20261027T181224Z-a76039813` (VALID, unprofiled). Medians of
three repetitions; 180214Z-074033fa1 is a second after-sample of end-to-end
timing (its phase readings predate the population fix and the record is
discarded).

| Step | server p50 ms before / 180214Z / after | server p99 ms | rows/cpu-s |
|---|---|---|---|
| otlp-spans-c1 | 10.8 / 10.8 / 11.1 | 19.1 / 20.5 / 18.7 | 56.6k / 58.5k / 56.0k |
| otlp-spans-c64 | 35.3 / 25.7 / 36.4 | 137.1 / 141.4 / 148.0 | 48.8k / 47.6k / 47.1k |
| otlp-spans-1k-c1 | 18.4 / 18.8 / 18.3 | 32.0 / 34.1 / 30.1 | 29.7k / 29.6k / 30.2k |
| otlp-spans-1k-c64 | 56.2 / 61.2 / 62.4 | 183.6 / 197.5 / 204.3 | 22.6k / 21.9k / 22.8k |
| otlp-logs-c1 | 12.1 / 12.3 / 12.0 | 24.4 / 34.4 / 19.4 | 47.9k / 47.6k / 51.6k |
| otlp-logs-c64 | 61.0 / 86.1 / 68.7 | 189.1 / 259.8 / 203.6 | 34.8k / 36.4k / 34.9k |
| native-write-c1 | 13.4 / 13.5 / 13.7 | 74.4 / 76.5 / 113.3 | 641k / 652k / 651k |
| native-write-c64 | 175.0 / 170.7 / 168.4 | 780.7 / 542.6 / 543.7 | 778k / 795k / 931k |

End-to-end timing and CPU per row show no consistent direction across the
two after-runs: each step's after-values bracket or sit within the
run-to-run spread (logs-c64 p50 86.1 then 68.7; native-c1 p99 76.5 then
113.3, an fsync tail). The added cost is one timing-registry lock per
acknowledged write, only when benchmark timing is enabled.

Phase p50/p99 ms, repetition 1 of the after record (`phase_<name>_*_ms`):

| Step | scribe | preprocess | shard_wait | wal_append | wal_sync | visibility | otlp_decode |
|---|---|---|---|---|---|---|---|
| otlp-spans-c1 | 6.7/18.4 | 0.9/1.5 | 0.1/6.9 | 0.1/0.2 | 3.6/9.0 | 1.3/2.5 | 1.3/2.1 |
| otlp-spans-c64 | 23.2/137.2 | 1.1/1.9 | 7.2/101.5 | 2.2/9.1 | 7.1/31.4 | 4.1/7.4 | 1.9/3.2 |
| otlp-spans-1k-c1 | 8.1/20.7 | 1.4/2.2 | 0.1/8.4 | 0.2/0.3 | 4.1/10.2 | 1.3/2.1 | 3.6/4.6 |
| otlp-spans-1k-c64 | 20.9/77.4 | 1.6/2.7 | 5.4/48.8 | 1.3/26.3 | 7.5/34.9 | 3.2/10.9 | 4.5/7.4 |
| otlp-logs-c1 | 7.9/13.1 | 2.1/2.9 | 0.1/3.8 | 0.2/0.4 | 3.6/7.0 | 1.3/2.2 | 1.0/1.4 |
| otlp-logs-c64 | 54.5/188.9 | 2.7/4.4 | 15.7/126.1 | 6.8/47.3 | 19.6/64.4 | 5.1/8.6 | 1.6/3.1 |
| native-write-c1 | 13.5/70.0 | 5.4/7.3 | 0.1/20.3 | 0.5/1.2 | 5.1/41.6 | 1.7/2.8 | unavailable |
| native-write-c64 | 168.2/537.9 | 9.7/18.9 | 58.5/399.0 | 31.5/99.9 | 57.7/344.5 | 8.3/12.1 | unavailable |

Admission, Scribe decode and ack stay below 1.7 ms p99 on every path.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Native, span and log records at 1 and 64 clients expose applicable phase distributions | `WriteProfile::record_timing` (`scribe/profile.rs`), `timed_decode` (`grpc/otlp.rs`), `set_write_phases` (`query/isolated.rs`), `WRITE_PHASES` (`query/mod.rs`) | 181224Z-a76039813: every write step carries 20 phase readings | PASS |
| Correct populations | `set_write_phases` reads `Population::only(case, phase)` | each repetition's phase `samples` equals its reconciled cohort (e.g. spans-c64 4633 = 4633); `write_phases_read_back_with_populations_and_labelled_gaps` excludes an unlabelled append | PASS |
| Labelled unavailable phases, no fabricated samples | OTLP decode on native is missing with its reason | same test; native records carry `samples: null` with the Arrow IPC reason | PASS |
| Controlled timing and record/readback exercise the real collector | real Scribe append and timing registry | `scribe::ingress::tests::acknowledged_append_records_each_write_phase_once`, `grpc::otlp::tests::decode_timing_records_only_successful_decodes`, the readback test | PASS |
| End-to-end timing remains unchanged | timing only when `timing::enabled()` | the table above: no consistent shift over two after-runs | PASS |

Commands: `cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::ingress::tests::acknowledged_append_records_each_write_phase_once)'`;
`cargo nextest run --locked -p wyrd-server --lib -E 'test(/^grpc::otlp::tests::/)'`;
the capacity lane (61 passed); `mise run fmt`, `mise run lints`,
`git diff --check`; `mise run bench:l1:bifrost:writes` twice.

### Item 7(a) acceptance: single-byte varint fast path

Change: `76bcfa75a`, `WireFields::read_varint` (`otlp_decode.rs`) returns a
byte below 0x80 before the general loop. Before:
`writes-parquet/20261027T181224Z-a76039813`; after:
`writes-parquet/20261027T182756Z-76bcfa75a` (VALID, unprofiled). Medians of
three repetitions.

| Step | otlp_decode p50 ms | rows/cpu-s | wire MiB/cpu-s | server p50 / p99 ms |
|---|---|---|---|---|
| otlp-spans-c1 | 1.36 → 1.32 | 55.95k → 56.24k | 18.1 → 18.1 | 11.1/18.7 → 11.2/19.1 |
| otlp-spans-c64 | 1.89 → 1.89 | 47.09k → 48.34k | 15.2 → 15.6 | 36.4/148.0 → 27.7/133.0 |
| otlp-spans-1k-c1 | 3.64 → 3.69 | 30.20k → 29.62k | 30.4 → 29.8 | 18.3/30.1 → 18.4/30.1 |
| otlp-spans-1k-c64 | 4.51 → 4.45 | 22.75k → 22.45k | 22.9 → 22.6 | 62.4/204.3 → 61.2/205.3 |
| otlp-logs-c1 | 0.99 → 0.97 | 51.63k → 48.36k | 61.7 → 57.8 | 12.0/19.4 → 12.2/22.6 |
| otlp-logs-c64 | 1.67 → 1.51 | 34.94k → 36.67k | 41.7 → 43.8 | 68.7/203.6 → 89.7/258.7 |

Result: no measurable improvement. Decode time and CPU per row move in both
directions within the run-to-run spread recorded for item 6. The change is
kept: it is correct and adds no cost.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Own before/after result for CPU per row, row and wire throughput, latency | the table above | two VALID unprofiled writes-parquet records | PASS (no improvement observed) |
| Correctness for malformed and boundary varints | fast path only for a present byte below 0x80 | `otlp_decode::tests::varints_read_boundaries_and_refuse_malformed_input` (0, 0x7f/0x80, 0x3fff/0x4000, u32::MAX, u64::MAX; empty, truncated, tenth-byte overflow, eleven bytes); 55 `otlp` lib tests | PASS |

### Item 7(b) acceptance: server-built Variants

Change: `a8cf9e95a`. `finish_variant` (`tables/signal.rs`) calls the new
`EncodedVariant::from_builder` (`wyrd-queue/src/variant.rs`): it takes the
`VariantBuilder` itself, keeps the size check and the bounded
`scan_encoded` walk, and skips upstream's recursive re-validation and the two
byte copies of `from_bytes`. External bytes still go through `from_bytes`.
Before: `writes-parquet/20261027T182756Z-76bcfa75a`; after:
`writes-parquet/20261027T184447Z-a8cf9e95a` (VALID, unprofiled). Medians of
three repetitions.

| Step | rows/cpu-s | wire MiB/cpu-s | server p50 / p99 ms |
|---|---|---|---|
| otlp-spans-c1 | 56.24k → 59.68k | 18.1 → 19.3 | 11.2/19.1 → 10.4/17.2 |
| otlp-spans-c64 | 48.34k → 51.23k | 15.6 → 16.5 | 27.7/133.0 → 36.2/143.3 |
| otlp-spans-1k-c1 | 29.62k → 31.05k | 29.8 → 31.2 | 18.4/30.1 → 17.8/29.0 |
| otlp-spans-1k-c64 | 22.45k → 23.38k | 22.6 → 23.5 | 61.2/205.3 → 48.2/178.4 |
| otlp-logs-c1 | 48.36k → 46.15k | 57.8 → 55.1 | 12.2/22.6 → 12.4/32.7 |
| otlp-logs-c64 | 36.67k → 34.25k | 43.8 → 40.9 | 89.7/258.7 → 64.7/197.8 |
| native-write-c64 | 672k → 766k | — | 173.4/557.3 → 142.5/409.2 |

Result: every span step rose 3–6% in rows/cpu-s, at or just above the
spread of the four earlier records (spans-c1 55.9k–58.5k; 1k-c1
29.6k–30.2k); logs fell within its spread (46.1k–51.6k at c1). Native writes
build no OTLP Variants, so their change is run-to-run variation. Scribe
`preprocess` p50 is unchanged. This is suggestive for spans, not a measured
improvement beyond noise.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Own before/after result | the table above | two VALID unprofiled writes-parquet records | PASS (spans +3–6%, at the noise edge) |
| Size and depth limits preserved exactly, plus the numeric domain | same `within_size_limit` and `scan_encoded` as `from_bytes` | `variant::tests::built_variants_match_full_byte_validation` (accepted object, NaN in a list, root infinity, size + 1, depth 66: identical result and path to `from_bytes`; the four refusals are refusals); 23 `variant::` and 47 `tables::` lib tests | PASS |

### Item 7(a) correction and decoder deduplication

The 7(a) record above covers traces only. `otlp_logs_decode.rs` and
`otlp_metrics_decode.rs` each held their own copy of `WireFields`, so the
fast path in `otlp_decode.rs` did not reach logs: the logs rows of the 7(a)
table measured unchanged code, and their movement is run-to-run variation.

With the maintainer's agreement, the duplicate copies were deleted before
7(c), so 7(c) changes one owner:

- logs now use `otlp_decode.rs`'s `WireFields`/`WireValue`, count and
  last-field helpers, `KeyValue`/`AnyValue` decode, and resource, scope and
  entity-reference decode. These copies differed only in names, comments and
  two error texts ("optional resource" and "optional instrumentation scope"
  are now "resource" and "instrumentation scope"; same malformed refusal).
- metrics use the shared `WireFields`/`WireValue` and leaf helpers
  (`fixed_*`, `protobuf_*`, `wrong_wire`, `malformed`,
  `validate_any_value_wire`). Their message decoders have diverged
  (`MessageBodies`, `decode_last_string`) and stay in place.

1,079 lines deleted. All 55 `otlp` lib tests pass (the count is unchanged)
and lints are clean. The next writes run measures 7(a) for logs.

7(a) for logs: before `writes-parquet/20261027T184447Z-a8cf9e95a`, after
`writes-parquet/20261027T191939Z-916be62bb` (VALID, unprofiled; traces
unchanged by the deduplication).

| Step | otlp_decode p50 ms | rows/cpu-s | wire MiB/cpu-s |
|---|---|---|---|
| otlp-logs-c1 | 1.05 → 0.96 | 46.15k → 49.36k | 55.1 → 59.0 |
| otlp-logs-c64 | 1.66 → 1.60 | 34.25k → 36.14k | 40.9 → 43.2 |

Result: no measurable improvement. Both moves sit inside the logs spread of
the earlier records (c1 46.1k–51.6k rows/cpu-s; decode p50 0.97–1.05 ms).
Span and native steps moved within their spreads.

### Item 7(c) acceptance: two passes per OTLP message

Every trace and log message (span, event, link, status, key/value, resource,
entity reference, scope, resource and scope groups, log record) now takes one
`Scanned` pass, which records each tag's length-delimited count and last body,
and one fill pass that validates every known field's wire type. Before, each
count, last-wins field and merged singular message rescanned the message
(about 10 passes per span). A singular message that occurs once decodes from
its one body; one split across occurrences keeps the existing merge path.
The existing owner was the per-field helpers in `otlp_decode.rs`; the gap was
the repeated scan, and `Scanned` replaces those calls in place.

Before `writes-parquet/20261027T191939Z-916be62bb`, after
`writes-parquet/20261027T194538Z-3adfc0c14` (VALID, unprofiled, commit
3adfc0c14). Medians of three repetitions; every repetition of every step
moved by more than the spread.

| Step | otlp_decode p50 ms | rows/cpu-s | wire MiB/cpu-s |
|---|---|---|---|
| otlp-spans-c1 | 1.30 → 0.74 | 59.2k → 63.2k | 19.1 → 20.4 |
| otlp-spans-c64 | 1.85 → 1.15 | 51.7k → 55.6k | 16.7 → 17.9 |
| otlp-spans-1k-c1 | 3.67 → 2.44 | 31.3k → 34.0k | 31.4 → 34.2 |
| otlp-spans-1k-c64 | 4.50 → 3.13 | 24.0k → 24.9k | 24.1 → 25.0 |
| otlp-logs-c1 | 0.96 → 0.70 | 49.4k → 50.3k | 59.0 → 60.0 |
| otlp-logs-c64 | 1.60 → 1.15 | 36.1k → 38.2k | 43.2 → 45.6 |

Result: decode time fell 27–43% on every OTLP step. Rows per CPU-second
rose 2–9%. Small spans still miss the 30 wire MiB/cpu-s floor. Native steps
do not decode OTLP and moved within their spreads.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Two passes per message | `Scanned` in `otlp_decode.rs`; trace and log decoders | the table above | PASS |
| Exact capacity, last-wins, merges and malformed refusals preserved | fill pass keeps every `wrong_wire` arm; split singulars use `decode_merged_*` | 56 `otlp` lib tests, including `otlp_decode::tests::trace_fixed_decode_keeps_last_strings_and_refuses_wrong_wire_types`, `trace_fixed_decode_has_no_spare_collection_capacity`, `trace_fixed_decode_merges_split_singular_messages`, `logs_fixed_decode_merges_duplicate_singular_fields`; fmt, lints, `git diff --check` | PASS |

### Item 7(d) attribution: the remaining SHA-256 caller

The 2026-10-27 profile's binary had been rebuilt, so it could not be
symbolized. A new profiled run on 3adfc0c14 code
(`target/benchmarks/bifrost/writes-parquet/20261027T200319Z`, VALID, build id
matched) was symbolized with `addr2line`, and samples were bucketed into 5 s
windows.

| Window | `prepare_slice` → `logical_data_identity` | Gate `LogicalIdentity::of` | share of window CPU (slice digest) |
|---|---|---|---|
| otlp spans c64 | 935 | 786 | 2.1% |
| otlp 1 KiB spans c64 | 857 | 816 | 2.2% |
| otlp logs c64 | 1,570 | 1,564 | 4.4% |
| native writes c64 | 3,166 | 0 | 7.9% |

The caller is `scribe::preprocess::prepare_slice`. It recomputes
`logical_data_identity` for every OTLP slice even though the Gate computed one,
because `LogicalIdentity::for_rows` never matches an OTLP batch. Scribe decode
(`stamp_correlation_columns`) adds the correlation columns `run_id`,
`sub_principal_id`, `act_principal_id` and `workload_card_uid`, and may
overwrite dimension columns. Those columns are non-volatile, so the slice
digest hashes field names and bytes the Gate's batch never contained. The two
digests cover different bytes. Native slices have no Gate digest at all.

Result: the remaining caller repeats no digest of the same bytes, so per
REQ-025 it is not removed and identity semantics are unchanged. No code
change, so no before/after run. Other SHA-256 users in the same profile
(WAL append, Parquet artifact sealing, staging, remote verification, Forge
promotion) digest different data and are outside 7(d).

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

**Requirement.** REQ-008 gives writes-parquet 420 seconds, enough for the
current 311 seconds plus item 4's two added steps, retains 300 seconds for hot
and parquet, and updates the Bifrost L1 aggregate maximum to 1,020 seconds. Keep the same clock boundaries and full coverage; do not shorten or
omit repetitions to fit.

**Acceptance.** A complete writes-parquet record contains every repetition,
including native-write-c64, and reports its actual elapsed time against 420
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


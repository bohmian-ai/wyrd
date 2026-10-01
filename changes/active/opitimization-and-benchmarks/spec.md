---
id: SPEC-opitimization-and-benchmarks
revision: 6
status: draft
---

# Bifrost read and write performance within striking distance of ClickHouse

## Objective and user value

Bifrost is the analytical store behind every Wyrd verification read and every
observation write. Its read path (Oracle) and write path (Gate, Scribe, Forge
publication) have to perform like a real OLAP engine. Today we have one
internal capacity benchmark and no comparison against an external reference.

Bifrost is both an analytical store and an observability store, so this
change measures it in both roles. It does three things:

1. It reproduces the ClickHouse benchmark (ClickBench) in full against the real
   production `wyrd-server`, on the owner's local machine inside an
   16-vCPU/32-GiB envelope, the same shape as ClickBench's reference machine.
   The results are written in ClickBench's own result
   format with a full description of the environment. For internal analysis
   only, they are compared with ClickHouse's already-published ClickBench
   results. We do not run ClickHouse, and we publish only Bifrost's numbers
   against its own declared setup.
2. It adds an observability benchmark that ClickBench does not cover: ingest
   rate per core (rows/s and MB/s), compression ratio, and query latency over
   fixed time windows on logs, traces, and metrics. This benchmark measures
   Bifrost alone against absolute targets; ClickHouse is not run on it.
3. It optimizes Bifrost reads and writes until both benchmarks meet the
   targets in this specification. Bifrost does not have to beat ClickHouse. It
   has to come within the stated ratios.

ClickBench deliberately leaves out concurrency, writes during reads, tail
latency, and memory limits. The existing internal capacity benchmark
(`bifrost_query_capacity`) covers those and stays as a third, required
evidence class.

The audience is operators and platform teams choosing a store for verification
evidence, and Wyrd maintainers, who get a repeatable external yardstick for
every later performance change.

**Owner instruction.** Where the architecture and design documents
(`architecture/*.md`, `architecture/bifrost-design.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`) constrain this
work, they do not bind this specification. They may be out of date. They
describe how the system works today and are context only. This change does not
edit them. Where an optimization departs from them, the departure is recorded
in the implementing task's evidence. Any follow-up documentation is a separate
change.

## Current baseline

Measured with `mise run bench:bifrost:query-capacity`
(`crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity`) on an 8-CPU/16-GiB
node, in TASK-004 of `changes/active/bifrost-scribe-live-reads`:

| Measure | Result |
|---|---|
| Ingest (6 narrow columns) | ~1.08M rows/s |
| Selective point lookup, 1 client | p50 6.8 ms |
| Selective point lookup, peak | ~1,222 qps at 32 clients |
| 1M-row aggregate | 231 qps |
| Heavy full scan (100M rows) | p99 ~1.08 s |
| Heavy broad window | p99 ~333 ms |

Profiling shows where point lookups lose time. Each lookup spends about 6.5 ms
of CPU and opens about 88 hot (uncompacted) Parquet files. Planning takes
about 0.6 ms, and sequential Postgres catalog round trips take about 1.8 ms
per query. The scan leaves are `OracleIcebergScanExec` and `HotParquetExec` in
`crates/vala/vala-bifrost-redux/src/oracle/exec.rs`.

## Requirements

### Benchmark harness

**REQ-001 — Full ClickBench reproduction.** A Bifrost ClickBench run uses:

- the standard `hits` dataset: 99,997,497 rows and 105 columns, taken from the
  published ClickBench `hits.parquet` (or the partitioned `hits_*.parquet`
  set) and verified by row count and checksum before loading;
- all 43 ClickBench queries (Q0–Q42);
- three runs of each query. Before the first run of each query, the harness
  flushes buffers and drops the OS page cache (`sync`, then
  `echo 3 > /proc/sys/vm/drop_caches`), as ClickBench does. Run 1 is the cold
  time, and the faster of runs 2 and 3 is the hot time.

The harness pins a ClickBench repository commit. The run's queries, scoring
rules, and dataset must match that commit.

**REQ-002 — Query semantics and correctness.** Bifrost runs each query in its
own SQL dialect, but every query must mean the same as the ClickBench
original. The harness translates each pinned ClickHouse query into Bifrost SQL
and records every textual difference. ClickBench publishes timings, not
answers, so the harness checks each result against a committed reference
answer file chosen by decision D-3. A mismatch fails the run, with one
exception: queries whose ClickBench text has no deterministic order or
tie-break are compared as sets, and the harness lists every such query.

**REQ-003 — Real production server, default settings.** The harness drives one
release `wyrd-server` binary through its normal operator journey (migrate,
serve, setup). It uses the public Rust client over the public ingest and query
surfaces: `wyrd_client::Bifrost`, `POST /v1/query` or the gRPC query service,
and HTTP or gRPC ingest. The run uses no test-only feature, no hidden
environment variable, and no configuration a default deployment lacks. The
table is registered through the public table API. Its declared
`physical_layout` (partition granularity, sort keys, Bloom columns) and
compaction target are allowed because they are public, per-table, and
documented, like ClickHouse's `ORDER BY`. The harness records the full
registration request. Every query passes through normal authentication,
authorization, admission, and read audit.

**REQ-004 — Benchmark machine and environment description.** Every run uses
the owner's local host, with Bifrost (server plus its Postgres catalog)
confined to a 16-CPU/32-GiB systemd scope, matching the CPU and memory of
ClickBench's c6a.4xlarge reference. The host (Ryzen 9 9950X, 32 threads,
91 GiB) fits that scope. The internal capacity benchmark keeps its own
8-CPU/16-GiB envelope (REQ-010, AC-7). Every result carries an environment description: CPU
model and the CPUs the scope allows, enforced memory limit, storage device and
filesystem, kernel, `wyrd-server` version and commit, dataset version, and
the ClickBench commit. Published Bifrost numbers are always stated against
this description.

**REQ-005 — Published ClickHouse reference (internal only).** The reference
for internal analysis is the ClickHouse
result JSON for c6a.4xlarge in the pinned ClickBench commit (the
self-managed `clickhouse` entry). It supplies ClickHouse's load time, data
size, and three runs per query. Bifrost's cold and hot times are derived from
its own three runs exactly as ClickBench derives them from the published ones.
The published run used a c6a.4xlarge (16 vCPU, 32 GiB, gp2 disk). Our scope
matches its CPU count and memory but not its CPU model or disk, so the comparison is a rough internal yardstick, not an
apples-to-apples result, and the report says so.

**REQ-006 — ClickBench metrics and output.** Every Bifrost run produces:

- load time in seconds (defined in REQ-007);
- data size on disk in bytes (defined in REQ-008);
- cold and hot times for all 43 queries;
- the ClickBench relative score. Each query's ratio is
  `(t_bifrost + 0.01 s) / (t_clickhouse + 0.01 s)`. The score is the geometric
  mean of those ratios, computed separately for cold and hot. Load time and
  data size get their own ratios. The ACs use this direct
  Bifrost/ClickHouse ratio. These ratios live in an internal report only.

The output is a result JSON in the pinned ClickBench schema (`system`, `date`,
`machine`, `cluster_size`, `tags`, `load_time`, `data_size`, `result`),
carrying the REQ-004 environment description in `machine` and `tags`. That
file holds Bifrost's numbers only and is the publishable artifact. A separate
internal report holds the ClickHouse ratios and pass/fail per AC.
It also includes the server log and a `/metrics` snapshot. A failed, rejected,
or timed-out query is reported as `null`, as ClickBench does, and fails the run.

**REQ-007 — Load time definition.** Bifrost load time runs from the first
ingest request until every row is (a) acknowledged and (b) visible in the
published (Iceberg) cut, so that a query with no live Scribe contribution
returns the full row count. The report also lists acknowledgement-only time
and publication lag as their own fields. Query runs start only after (b).

**REQ-008 — Data size definition.** Bifrost data size is the bytes the
published table occupies in object storage (data files, deletes, and Iceberg
metadata) once compaction has settled after load. WAL, staging, Postgres
catalog bytes, and hot files still present are reported in their own fields.
They do not count toward the ClickBench `data_size` unless decision D-5
chooses otherwise.

**REQ-009 — Invocation.** The benchmark is opt-in, one `mise` task, part of no
gate or verify lane, and needs no cloud credentials. It downloads and caches
the dataset and the pinned ClickBench results. It exits nonzero when an AC target is
missed or a result is wrong. It is reproducible from a clean checkout on a
Linux host that meets REQ-004.

**REQ-010 — Internal capacity benchmark kept.** The existing
`bench:bifrost:query-capacity` benchmark keeps running as the third evidence
class. It covers what ClickBench and the observability benchmark leave out:
selective reads, concurrent clients, reads while writing, tail latency under
load, overload and queueing, and behavior at the memory limit. AC-7 sets new
targets for it. Its fixture, envelope, and cases may change
only through a revision of this specification.

### Optimization

**REQ-011 — Optimize reads and writes to the targets.** The change optimizes
the Oracle read path and the Gate/Scribe/Forge write and publication path
until AC-1 through AC-12 pass. The specification fixes the targets and the
proof, not the mechanisms. Each optimization task reports its before and after
numbers on the benchmark that motivated it.

Candidate areas, in rough expected-value order. These are scope, not
prescription:

- **Per-query fixed cost.** Sequential Postgres catalog round trips during cut
  pinning and authorization, repeated planning work, and session and plan
  construction per query.
- **Hot-file fan-out.** About 88 uncompacted hot files open per point lookup.
  Hot-file promotion and compaction cadence, file and row-group pruning from
  statistics and Bloom filters before any open, and cached Parquet footers and
  metadata.
- **Scan efficiency.** Projection and predicate pushdown into Parquet
  (row-group, page-index, and late-materialization filtering), decode
  parallelism, target partitions, and avoiding repeated reader construction
  per file.
- **Physical layout written by Scribe and Forge.** Row-group and page sizes,
  dictionary and compression choices, sort order applied at write and at
  compaction, statistics and page-index emission, and Bloom filters.
- **Operators that ClickBench stresses.** High-cardinality `GROUP BY`, `COUNT
  DISTINCT`, string `LIKE` and regex functions, top-N `ORDER BY ... LIMIT`,
  and memory admission sized so these complete under default settings and are
  not rejected.
- **Ingest throughput for wide rows.** Request encoding and decoding, Arrow
  validation, WAL append and sync cost, Scribe buffering and rotation,
  staging-to-publication latency, and client-side batching and parallelism in
  the public SDK.
- **Result delivery.** The cost of encoding and streaming query results to the
  client.
- **Observability paths.** OTLP decode and table-owned projection, time-window
  partition and file pruning, trace-id lookup, attribute-map filtering, and
  time-bucketed aggregation.

**REQ-012 — Optimizations are product behavior.** An optimization ships in the
default production configuration. A gain that needs a non-default setting, a
benchmark-only code path, or a dataset-specific special case does not count
toward an AC.

### Observability benchmark

**REQ-013 — Observability dataset.** The observability benchmark uses one
fixed logs, traces, and metrics dataset shaped like OpenTelemetry data. It has
realistic service, operation, and attribute cardinalities, log bodies, span
trees, and metric series, over a fixed time span. The dataset is either public
or generated deterministically from a recorded seed (decision D-11). Its scale
is fixed by decision D-12. Every run checks the dataset's row counts and
checksum before loading. Bifrost receives it through its canonical observation
tables (`vala.logs`, `vala.traces`, `vala.metrics`). Because the dataset is
generated, the generator also produces the expected answer for every REQ-016
query.

**REQ-014 — Ingest rate per core.** The benchmark loads the dataset into
Bifrost on the REQ-004 machine, through the public client with default server
settings (REQ-003). For each signal, the report gives:

- rows per second per core, which is rows divided by the server's CPU-seconds
  over the load, read from the server's cgroup;
- MB per second per core, the same calculation with raw input bytes;
- wall-clock rows/s and MB/s;
- peak server memory.

Raw input bytes are the uncompressed size of the dataset in the encoding
chosen in decision D-13. Server CPU includes the Postgres catalog. Client and
load-generator CPU is excluded and reported separately. As in REQ-007, the load is
complete when every row is acknowledged and visible in the published cut.

**REQ-015 — Compression ratio.** For each signal, compression ratio is raw
input bytes (REQ-014) divided by the bytes on disk once compaction has
settled, using the REQ-008 accounting.

**REQ-016 — Fixed time-window queries.** A fixed, versioned query set runs
over windows anchored at the dataset's end time: the last 5 minutes, 1 hour,
and 24 hours. The set covers the typical observability reads:

- log search by service, severity, and body text, newest first, with a limit;
- error counts per time bucket;
- trace lookup by trace id;
- p95 span duration per service and operation;
- metric aggregation per time bucket, grouped by an attribute.

Each query and window runs once cold (as in REQ-001) and then enough warm
repetitions for stable percentiles; the default is 100. The report gives cold
time and warm p50, p95, and p99 per query and window. Every result must match
the generator's expected answer (REQ-013).

**REQ-017 — Observability output and invocation.** The observability
benchmark meets REQ-003, REQ-004, and REQ-009 the same way ClickBench does:
real production server, the same machine, and one opt-in `mise` task that
exits nonzero on a missed target or a wrong result. It writes a
machine-readable result alongside the ClickBench outputs, plus the
human-readable report.

## Invariants

These hold whatever the architecture documents say:

- **INV-1 — Correct results.** No optimization changes a query result. Every
  existing Bifrost correctness, journey, and capacity answer check still
  passes.
- **INV-2 — Tenant isolation.** Authentication, authorization (including
  object-scoped query permission), and per-file tenant proof stay enforced on
  every read and write. No cache or shared structure lets one tenant's data,
  metadata, or plan state reach another tenant.
- **INV-3 — Audit.** Query and write authorization decisions are still
  audited. The audit path's cost is part of the measured time.
- **INV-4 — Acknowledged durability.** An acknowledged append survives a
  server crash, unless decision D-6 approves a change.
- **INV-5 — Public contract stability.** Optimizations add no public API
  field, route, or persisted format unless a revision of this specification
  approves it. Changes to Parquet writer settings stay within the existing
  Iceberg/Parquet formats, and data already written remains readable.

## Non-goals

- Beating ClickHouse, or matching it on every query.
- Distributed or multi-node ClickBench runs. Only the single-node result
  counts.
- Running ClickHouse. Comparison uses its published results only.
- Publishing any Bifrost-versus-ClickHouse comparison. Only Bifrost's numbers
  against its declared environment are published.
- Matching ClickBench's reference hardware.
- Submitting results upstream to the ClickBench repository. Producing
  submittable output is in scope; submitting it is not.
- Making the benchmark a CI gate.
- Publishing absolute numbers as marketing claims.
- Editing the architecture or design documents.

## Expensive-to-reverse decisions fixed here

- Bifrost runs on the owner's machine in a 16-CPU/32-GiB envelope. ClickBench
  ratios compare it with ClickHouse's published c6a.4xlarge results for
  internal analysis only; ClickHouse is never run (REQ-004, REQ-005). The
  observability benchmark uses absolute targets.
- The benchmark uses only the public, default-configured server and public
  client surfaces (REQ-003, REQ-012).
- Load time includes publication. Data size counts the published table
  (REQ-007, REQ-008).
- The benchmark has three evidence classes, and each is required: ClickBench,
  the observability benchmark, and the internal capacity benchmark (REQ-010,
  REQ-013 to REQ-017).
- Per-core ingest counts server CPU only (REQ-014).

## Acceptance criteria

AC-2 to AC-6 are Bifrost in the 16-CPU/32-GiB envelope divided by ClickHouse's
published c6a.4xlarge results, using the ClickBench `+0.01 s` scoring. They
are internal targets. AC-8 to AC-12 are absolute targets on Bifrost in the
same envelope. The proposed targets need owner
confirmation (D-1).

| ID | Criterion | Proposed target |
|---|---|---|
| AC-1 | Completeness and correctness: all 43 queries complete under default settings and match the reference answers (REQ-002) | 43/43 |
| AC-2 | Hot-run geometric-mean ratio | ≤ 2.0× |
| AC-3 | Cold-run geometric-mean ratio | ≤ 3.0× |
| AC-4 | Worst single-query hot ratio | ≤ 10× |
| AC-5 | Load-time ratio (REQ-007) | ≤ 2.0× |
| AC-6 | Data-size ratio (REQ-008) | ≤ 1.5× |
| AC-7 | Internal capacity benchmark at 8 CPU/16 GiB: selective p50 at 1 client, ingest rows/s, and every existing row | p50 ≤ 2 ms, ingest ≥ 1.0M rows/s, no regression of other rows |
| AC-8 | Observability ingest rows/s per core, per signal (REQ-014) | logs ≥ 150k, spans ≥ 100k, metric points ≥ 300k |
| AC-9 | Observability ingest MB/s per core, per signal (REQ-014) | ≥ 50 MB/s per core |
| AC-10 | Compression ratio, per signal (REQ-015) | logs ≥ 10×, traces ≥ 8×, metrics ≥ 10× |
| AC-11 | Time-window warm p50 and p95, every query (REQ-016) | 5 min: p95 ≤ 50 ms; 1 h: p95 ≤ 200 ms; 24 h: p95 ≤ 1 s; trace-id lookup p95 ≤ 20 ms at every window |
| AC-12 | Time-window warm p99, every query, with answers matching the generator | p99 ≤ 2× the AC-11 p95 target; 100% of answers match |

**Evidence classes.** For each AC, the evidence is:

- the committed harness and its `mise` task;
- the ClickBench result JSONs and report from one complete run on the declared
  host, attached to the final task's evidence;
- per-optimization before and after numbers (REQ-011);
- the observability result files and report from one complete run on the
  declared host;
- the capacity-benchmark report;
- the existing Bifrost journey and correctness lanes passing (INV-1 to INV-5).


## Open decisions

The owner decides each of these before approval. Each has a recommended
default.

1. **D-1 — Target ratios.** Confirm or change the AC-2 to AC-12 targets.
   Recommended: the values in the table. A 2× hot geomean puts Bifrost among
   the leading Parquet-based engines on ClickBench. The cold target is looser because
   Iceberg and object-store metadata cost the most on a cold start. The AC-8 to
   AC-12 absolute targets are starting proposals for an observability store on
   one 16-CPU/32-GiB node; confirm or change each. The local CPU and NVMe are
   faster than c6a.4xlarge's CPU and gp2 disk, which flatters the ClickBench
   ratios, most of all the cold runs.
2. **D-2 — Machine (decided, revision 6).** The owner's local machine in a
   16-CPU/32-GiB envelope, described in every result (REQ-004).
3. **D-3 — Reference answers.** ClickBench publishes no query answers. Choose
   how the committed answer file (REQ-002) is produced: (a) from Bifrost's
   first complete run, hand-checked against facts known about `hits` (total
   row count 99,997,497 and distinct counts computed independently from the
   source Parquet), then frozen; or (b) from one off-line ClickHouse run
   used only to produce answers, never timings. Recommended: (a), which keeps
   ClickHouse out entirely. Its risk is freezing a Bifrost bug as the
   reference, which the hand checks are there to catch.
4. **D-4 — Bifrost table layout.** Allow the public `physical_layout` (up to
   four sort keys, for example `CounterID, EventDate, UserID, EventTime`) and
   Bloom columns, mirroring ClickHouse's `ORDER BY`. Also decide which column
   feeds the managed `wyrd_event_time`: `EventTime`, or the server receipt
   time. Recommended: allow the layout and map `EventTime` to
   `wyrd_event_time`. The ClickHouse entry is tuned the same way.
5. **D-5 — Data-size accounting.** Choose between published table bytes only
   (REQ-008) and including WAL, hot files, and catalog bytes. Recommended:
   published bytes after compaction settles, with the rest reported
   separately.
6. **D-6 — Durability semantics.** ClickHouse acknowledges inserts without
   fsync by default. Decide whether Bifrost may relax INV-4 (for example, group
   commit or acknowledging before sync) to close an ingest gap. Recommended:
   keep INV-4 and report the difference. Weaker durability changes a product
   promise.
7. **D-7 — Cold-run definition.** Drop the OS page cache only, as ClickBench
   does for ClickHouse, or also restart `wyrd-server` to clear in-process
   caches. Recommended: OS page cache only, to match ClickBench methodology.
8. **D-8 — Load client.** Choose between a single-stream public-SDK loader and
   a parallel loader with N concurrent writers, and between HTTP and gRPC
   ingest. Recommended: the public Rust SDK over gRPC with bounded
   parallelism. The concurrency used is recorded in `tags`.
9. **D-9 — Public contract additions.** Whether optimizations may add public
   fields or persisted metadata (for example, new layout options or table
   properties), which INV-5 currently forbids without a revision. Recommended:
   forbid by default and revise the specification per case.
10. **D-10 — Where results live.** Commit result JSONs under a tracked
    directory (for example, `benchmarks/clickbench/results/`), or keep them
    only as task evidence. Recommended: commit one JSON per system per
    accepted run, so regressions have a reference point.

11. **D-11 — Observability dataset source.** Choose a deterministic generator
    in the repository, seeded and modeled on the OpenTelemetry Demo services
    (frontend, checkout, cart, and so on), or a public dataset (for example,
    Loghub for logs, or recorded OpenTelemetry Demo telemetry). Recommended:
    the seeded generator. No public dataset covers logs, traces, and metrics
    together at a controllable scale, and a generator carries no download or
    license dependency. Its seed, version, and cardinality parameters are
    recorded with the results.
12. **D-12 — Observability dataset scale.** Recommended: a 7-day span with
    about 1 billion rows in total (about 600M log records, 300M spans, and
    100M metric points), so the 24-hour window covers about 140M rows and
    stays clear of cache effects. A smaller "standard" scale can serve for
    iteration but does not count toward the ACs.
13. **D-13 — Raw-byte encoding.** Choose the encoding that defines raw
    input bytes: uncompressed OTLP protobuf, or newline-delimited JSON of the
    same records. Recommended: uncompressed OTLP protobuf. It is the OTel wire
    form, and it does not inflate compression ratios the way JSON would.

## Revision history

- Revision 1 (2026-10-01): Initial draft from the owner's intent. It adds the
  full ClickBench reproduction against the production server, a same-host
  ClickHouse baseline, ratio targets, and the optimization scope. Draft;
  awaiting owner decisions D-1 to D-10.
- Revision 2 (2026-10-01): Adds the observability benchmark at the owner's
  request ("We are also an observability store. we need both."): ingest rate
  per core in rows/s and MB/s, compression ratio, and fixed time-window
  latency at p50, p95, and p99, each compared with ClickHouse on the same data
  and envelope (REQ-013 to REQ-017, AC-8 to AC-12, D-11 to D-14). The internal
  capacity benchmark is now an explicitly required third evidence class
  covering what ClickBench leaves out (REQ-010). Draft; awaiting owner
  decisions D-1 to D-14.
- Revision 3 (2026-10-01): Owner decisions. The observability benchmark runs
  Bifrost only, against absolute targets (AC-8 to AC-12); ClickHouse is
  compared on ClickBench alone, and the ClickHouse observability ingest
  decision is removed. Expected answers come from the generator. ClickBench
  query text is translated from the ClickHouse queries, with no other
  engine's entry as a source. AC-7's 2 ms selective p50 is confirmed. Draft;
  awaiting owner decisions D-1 to D-13.
- Revision 4 (2026-10-01): Owner decision: ClickHouse is not run. ClickBench
  results are compared with ClickHouse's published c6a.4xlarge results, so AC
  judgment runs Bifrost on a c6a.4xlarge (REQ-004, REQ-005, D-2). Correctness
  uses a committed reference answer file (REQ-002, D-3). The report places
  Bifrost in the published ranking. Draft; awaiting owner decisions D-1 to
  D-13.
- Revision 5 (2026-10-01): Owner decisions. Benchmarks run on the owner's
  machine in an 8-CPU/16-GiB envelope with a full environment description
  (REQ-004, D-2 decided). The ClickHouse comparison is internal analysis only
  and drops the published-ranking placement; only Bifrost's numbers against
  its declared setup are published. Draft; awaiting owner decisions D-1 and
  D-3 to D-13.
- Revision 6 (2026-10-01): Owner decision. ClickBench and the observability
  benchmark run in a 16-CPU/32-GiB envelope, matching ClickBench's reference
  CPU count and memory; the internal capacity benchmark stays at
  8-CPU/16-GiB. Draft; awaiting owner decisions D-1 and D-3 to D-13.

## Authority and context

- [Repository rules](../../../AGENTS.md). The owner's instruction sets aside
  constraining architecture rules for this change.
- [Bifrost design](../../../architecture/bifrost-design.md) for current
  behavior only.
- `changes/active/bifrost-scribe-live-reads` (TASK-004 profiling, capacity
  benchmark).
- ClickBench methodology: <https://github.com/ClickHouse/ClickBench> (pinned
  commit chosen at implementation).

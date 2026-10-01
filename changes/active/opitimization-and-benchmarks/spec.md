---
id: SPEC-opitimization-and-benchmarks
revision: 1
status: draft
---

# Bifrost read and write performance within striking distance of ClickHouse

## Objective and user value

Bifrost is the analytical store behind every Wyrd verification read and every
observation write. Its read path (Oracle) and write path (Gate, Scribe, Forge
publication) have to perform like a real OLAP engine. Today we have one
internal capacity benchmark and no comparison against an external reference.

This change does two things:

1. It reproduces the ClickHouse benchmark (ClickBench) in full against the real
   production `wyrd-server`, with ClickHouse run on the same machine. The
   results are published in ClickBench's own result format, so Bifrost numbers
   compare directly with ClickHouse's.
2. It optimizes Bifrost reads and writes until the benchmark meets the target
   ratios in this specification. Bifrost does not have to beat ClickHouse. It
   has to come within the stated ratios.

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

**REQ-002 — Query semantics.** Bifrost runs each query in its own SQL dialect,
but every query must return the same answer as the ClickBench original. The
harness takes a query's text from the pinned DataFusion ClickBench entry where
one exists, and records every textual difference from the ClickHouse query. It
compares each Bifrost result with ClickHouse's result on the same data. A
mismatch fails the run, with one exception: queries whose ClickBench text has
no deterministic order or tie-break are compared as sets, and the harness lists
every such query.

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

**REQ-004 — Measured resource envelope.** The run declares its hardware.
Bifrost (server plus its Postgres catalog) and ClickHouse each run alone, one
after the other, inside the same resource envelope: 16 vCPU and 32 GiB of
memory, the c6a.4xlarge reference shape. On a host larger than that, the
envelope is enforced as a systemd scope, the way the existing capacity
benchmark does it. The output records the host CPU model, storage device,
filesystem, kernel, and the enforced limits. Ratio targets (AC-*) are judged
only against the ClickHouse run from the same host and envelope. Comparisons
with the published c6a.4xlarge numbers are informational.

**REQ-005 — Side-by-side ClickHouse baseline.** The harness installs a pinned
ClickHouse release and runs the pinned ClickBench `clickhouse` entry unchanged
(its schema, load, and queries) in the same envelope, on the same storage
device, from the same source data. That gives the reference load time, data
size, and per-query cold and hot times for every ratio below.

**REQ-006 — ClickBench metrics and output.** For each system, every run
produces:

- load time in seconds (defined in REQ-007);
- data size on disk in bytes (defined in REQ-008);
- cold and hot times for all 43 queries;
- the ClickBench relative score. Each query's ratio is
  `(t_bifrost + 0.01 s) / (t_clickhouse + 0.01 s)`. The score is the geometric
  mean of those ratios, computed separately for cold and hot. Load time and
  data size get their own ratios.

The output is a result JSON in the pinned ClickBench schema (`system`, `date`,
`machine`, `cluster_size`, `tags`, `load_time`, `data_size`, `result`), one
per system, plus a human-readable report with the ratios and pass/fail per AC.
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
the dataset and the ClickHouse binary. It exits nonzero when an AC target is
missed or a result is wrong. It is reproducible from a clean checkout on a
Linux host that meets REQ-004.

**REQ-010 — Internal capacity benchmark kept.** The existing
`bench:bifrost:query-capacity` benchmark keeps running and is the regression
signal for selective reads, concurrency, reads while writing, and overload.
AC-7 sets new targets for it. Its fixture, envelope, and cases may change
only through a revision of this specification.

### Optimization

**REQ-011 — Optimize reads and writes to the targets.** The change optimizes
the Oracle read path and the Gate/Scribe/Forge write and publication path
until AC-1 through AC-7 pass. The specification fixes the targets and the
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

**REQ-012 — Optimizations are product behavior.** An optimization ships in the
default production configuration. A gain that needs a non-default setting, a
benchmark-only code path, or a dataset-specific special case does not count
toward an AC.

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
- Optimizing ClickHouse, or tuning its pinned ClickBench entry.
- Submitting results upstream to the ClickBench repository. Producing
  submittable output is in scope; submitting it is not.
- Making the benchmark a CI gate.
- Publishing absolute numbers as marketing claims.
- Editing the architecture or design documents.

## Expensive-to-reverse decisions fixed here

- ClickHouse is measured in the same envelope, and the ratios are judged
  against that run, not against published numbers (REQ-004, REQ-005).
- The benchmark uses only the public, default-configured server and public
  client surfaces (REQ-003, REQ-012).
- Load time includes publication. Data size counts the published table
  (REQ-007, REQ-008).

## Acceptance criteria

All ratios are Bifrost divided by ClickHouse from the same host and envelope,
using the ClickBench `+0.01 s` scoring. The proposed targets need owner
confirmation (D-1).

| ID | Criterion | Proposed target |
|---|---|---|
| AC-1 | Completeness and correctness: all 43 queries complete under default settings and match ClickHouse answers (REQ-002) | 43/43 |
| AC-2 | Hot-run geometric-mean ratio | ≤ 2.0× |
| AC-3 | Cold-run geometric-mean ratio | ≤ 3.0× |
| AC-4 | Worst single-query hot ratio | ≤ 10× |
| AC-5 | Load-time ratio (REQ-007) | ≤ 2.0× |
| AC-6 | Data-size ratio (REQ-008) | ≤ 1.5× |
| AC-7 | Internal capacity benchmark at 8 CPU/16 GiB: selective p50 at 1 client, ingest rows/s, and every existing row | p50 ≤ 2 ms, ingest ≥ 1.0M rows/s, no regression of other rows |

**Evidence classes.** For each AC, the evidence is:

- the committed harness and its `mise` task;
- the ClickBench result JSONs and report from one complete run on the declared
  host, attached to the final task's evidence;
- per-optimization before and after numbers (REQ-011);
- the capacity-benchmark report;
- the existing Bifrost journey and correctness lanes passing (INV-1 to INV-5).

## Open decisions

The owner decides each of these before approval. Each has a recommended
default.

1. **D-1 — Target ratios.** Confirm or change the AC-2 to AC-7 targets.
   Recommended: the values in the table. A 2× hot geomean puts Bifrost among
   DataFusion-class engines on ClickBench. The cold target is looser because
   Iceberg and object-store metadata cost the most on a cold start.
2. **D-2 — Hardware.** Use the 16-vCPU/32-GiB systemd envelope on the local
   Ryzen 9 9950X host (32 threads, 91 GiB, NVMe), or rent an actual
   c6a.4xlarge with 500 GB gp2. Recommended: the local envelope for iteration
   and for AC judgment, and an optional c6a.4xlarge run for published-number
   comparison.
3. **D-3 — ClickHouse side-by-side is required.** Recommended: yes, as REQ-005
   states. Published numbers come from different hardware and would make the
   ratios meaningless.
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

## Revision history

- Revision 1 (2026-10-01): Initial draft from the owner's intent. It adds the
  full ClickBench reproduction against the production server, a same-host
  ClickHouse baseline, ratio targets, and the optimization scope. Draft;
  awaiting owner decisions D-1 to D-10.

## Authority and context

- [Repository rules](../../../AGENTS.md). The owner's instruction sets aside
  constraining architecture rules for this change.
- [Bifrost design](../../../architecture/bifrost-design.md) for current
  behavior only.
- `changes/active/bifrost-scribe-live-reads` (TASK-004 profiling, capacity
  benchmark).
- ClickBench methodology: <https://github.com/ClickHouse/ClickBench> (pinned
  commit chosen at implementation).

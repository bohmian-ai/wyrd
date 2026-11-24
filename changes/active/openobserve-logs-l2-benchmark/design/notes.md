# OpenObserve logs Level 2 Bifrost benchmark

Status: exploring

## Problem

The maintainer needs to run the published OpenObserve versus ClickHouse logs
workload against Wyrd alone, at **500,000,000 rows**, within **2 TB total local
storage**. Today Bifrost's L2 experiment measures concurrent ingest, overload,
and recovery over a different 100M-row fixture. It cannot answer how Wyrd
performs on these nineteen serial, frozen-dataset queries.

The intended result is one reproducible capacity record answering: how long
does loading the reference-shaped corpus take, how quickly does each exact
predicate return cold and hot, are all rows durable, and what does the stored
corpus including indexes cost? Published 1B-row results provide context, not a
same-scale competition or a Wyrd performance target.

## Desired outcome

A separate named L2 experiment loads one 500M-row log corpus, settles it to
Parquet with maintenance complete, then measures all nineteen cases five times
serially. The capacity report shows all five server samples, first-run cold
proxy, hot `min(run 2, run 3)`, total hot, geometric mean, totals for the three
query classes, durable storage row count, indexed storage, raw/on-disk
compression, and ingest wall time / rows/s / MB/s.

It embeds the published numbers beside Wyrd's with the row counts and all
comparison concessions visible. Missing evidence is missing or INVALID, never
zero. No Wyrd result is claimed by this design.

## Constraints and non-goals

- Design only; all edits stay in this packet. No production changes, reference
  script execution, benchmark runs, branches, commits, spec, or task packet.
- Keep all existing benchmark commands and the `competing` experiment intact.
  This is opt-in and belongs to no gate or CI lane.
- One local host, file-backed warehouse, repository Postgres, one Wyrd server,
  one isolated benchmark tenant. No ClickHouse or OpenObserve process runs.
- No query-specific copies, materialized views, precomputed query answers in
  the server, fabricated matching rows, or substitutions for reference needles.
- Preserve the portable platform rule. No privileged cache drops, affinity,
  cgroups, namespaces, `/proc`, platform conditionals, or memory-pressure load.
- Current unrelated OTLP work remains untouched. Mapping evidence below reads
  the current working tree; implementation must revalidate it against the
  eventual integrated revision and record the measured build/dirty state.

## Proposed implementation

### Entry point and L2 lifecycle

Proposed command, **not available today**:

```text
mise run bench:l2:bifrost:openobserve-logs
```

Add that narrow mise task over the existing `bench:build` and repository
Postgres wrapper, selecting `capacity l2 bifrost openobserve-logs`. Extend the
existing capacity dispatch/declaration path with this named L2 workload. Reuse
`LocalServer`, the public `wyrd_client::Bifrost`, OTLP request transport, the
experiment recorder/validity machinery and record/report/compare readers.
Do not create a separate script runner or invoke the reference generator.

This requires an **explicit experiment-specific L2 lifecycle**, not the
current fixed L2 warmup/windows: load once → settle → query once → report. The
95 requests are five samples per case, not five loads or three competing-load
cycles. No warmup query may consume run 1. No L1 calibration, 12/s query stream,
overload injection, or 50/30/20 mixed ingest belongs to this experiment.
Declare zero query warmup and five ordered single-request repetitions. Declare
one finite ingest step and untimed setup/settlement controls; do not pretend
one ingest observation is five repetitions. Reuse the existing declaration
`Timing` rather than add another scheduling framework. Extend it only where
its current timed-step assumption prevents representing a finite step.

Recommend a declared **24-hour full-run budget**, including generation,
ingestion, settlement, queries and final collection; each query has a declared
10-minute timeout. These are protective proposed limits, not performance
claims. The existing 30-minute/minimum-20-minute L2 definition cannot represent
this workload: accepting the named finite-corpus L2 subtype is decision D1.
A small smoke uses the same protocol and nineteen cases/five samples with
50,000 rows and stays diagnostic; it never qualifies a 500M result. Do not
schedule it automatically before every full run.

### Generate → ingest → durable rows

Port only the inspected generator's record/template logic into the existing
capacity query-fixture owner. The source version, source SQL, mapping and
published samples are preserved in [workload.md](workload.md), with unchanged
[reference SQL](reference-queries.json) and [published data](reference-results.json).

Use the source's `total=500000000`, seed `0xC0FFEE`, 8,000 records per logical
batch and six concurrent sends. Preserve `splitmix64(seed XOR record_index)`,
ChaCha8, call order, distributions, UUID-v5 namespace and message templates.
Use its total-dependent universe formulas, rather than arbitrarily fixing
cardinalities at their 1B values:

```text
traces = round(total / 7.8)
client IPs = round(total * 0.005127018383746945)
JA3 = round(total * 0.06173306600579376)
```

The existing workspace uses rand 0.9; the source uses rand 0.8 and rand_chacha
0.3. Both generations are already in Cargo.lock. Exact draws require the old
API/algorithm: use pinned benchmark-only dependencies from the existing lock
and enable existing UUID v5 support at the testing crate. Do not replace them
with a different PRNG to avoid declaring a dependency. Golden fixtures must
pin the output. Audit the port and preserve required upstream license notices.
No dependency belongs in a production crate for this fixture.

Choose a fixed event anchor inside the pinned query window, e.g.
`2026-08-19T00:00:00Z`; retain the generator's actual monotonic elapsed offset
per batch and +1 microsecond per row. Record each batch's starting timestamp,
index range and digest in a bounded-size provenance ledger so replay can
reconstruct exact timestamps. This retains the source timestamp model; a
500M local load will not necessarily span the published ~4h13m. The full-year
bounds remain identical; do not compress time or force the published duration.

**Stream; do not materialize raw NDJSON.** Serialize each logical source row
once to count exact UTF-8 bytes including its newline, hash the logical input,
then release that serialization after the bounded batch is converted/sent.
Keep at most six batches plus the current builder and bounded correctness
summaries. No unbounded producer queue or 500M-row vector. This removes the
~1.1 TB raw file while preserving the raw byte numerator.

Use **OTLP/HTTP protobuf at `POST /v1/logs`**, authenticated through the same
benchmark tenant as queries. This is an existing public log-ingest path.
The inspected server has no Elasticsearch/Splunk-compatible NDJSON logs
endpoint. OTLP does **not** carry the reference's identical NDJSON request
bytes. It carries the same 27 typed values; that is a deliberate transport
concession requiring D2. Do not disguise it by wrapping an entire NDJSON
record as a log message, duplicating it into attributes, or creating an adapter
endpoint just for the benchmark.

Map event time, severity, message and binary trace/span IDs to canonical log
columns, and all 22 other fields to record attributes under their original
names; the complete mapping is in [workload.md](workload.md#proposed-lossless-mapping-of-all-27-fields).
Queries reconstruct the original 27-column projection. Canonical extra
columns and Variant representation count toward Wyrd's stored bytes.

Recommend uncompressed OTLP protobuf, using the existing decoder. Record the
additional difference from the source's default gzip NDJSON, and report logical
NDJSON bytes separately from OTLP wire bytes. Keep the 8,000-row batch shape:
the current 16 MiB request default may be too small, so explicitly configure
`scribe.ingest_request_bytes=67108864` (64 MiB), record it, and check the actual
encoded and expanded sizes. Do not silently split oversized requests or
weaken admission. Six maximum-sized requests still require bounded memory;
use normal Scribe transport/expansion admission. A refusal invalidates the run.

Start the shared-pass clock **before generating the first measured batch**;
stop only when the final batch is positively acknowledged. Count each source
row/byte once. Rows/s = acknowledged rows / wall seconds; raw MB/s = counted
NDJSON bytes / seconds / 1,000,000; also show MiB/s and OTLP wire MB/s. Retain
server ingest timings separately. The published 16,666 s / 60,002 rec/s /
128.1 MB/s is one three-engine fan-out pass, not any engine's isolated ingest
capacity. Wyrd's one-sink pass cannot establish an ingest speedup against it.

An OTLP success must have zero rejected records and no nonempty partial-error
message. Failures, partial acceptance and uncertain acknowledgements are
INVALID. OTLP has no source batch UUID deduplication contract: do not blindly
retry an ambiguous response and double the rows. Abort the measured attempt;
a later attempt uses a fresh benchmark namespace/data root. Do not resume a
partly acknowledged corpus as a successful run.

### Index and file geometry choices

| Property | Verified Wyrd today | Proposed benchmark / gap |
|---|---|---|
| `message` full-text | `body_text` scalar; no full-text layout/index contract found | Whole-token SQL scan; no inverted index. Exact index parity requires product work. |
| `trace_id` | Canonical fixed binary; Parquet embedded Bloom, FPP 0.01 | Keep native binary equality and Bloom. This differs from O2's external Bloom plus Tantivy secondary index. |
| `span_id` | Canonical fixed binary; absent from default log Bloom list | Enable via existing table layout edit before loading; embedded Bloom FPP 0.01. No new product code required. |
| `kubernetes_pod_name` | Attribute Variant; no promoted canonical scalar or secondary index | Preserve the exact equality as a Variant predicate. Index parity requires a supported indexed projection/promotion. |
| container | Attribute Variant | No index, matching the reference's deliberate omission. |
| other default Blooms | service/subject/workload/run attribution defaults | Replace benchmark table Bloom declaration with trace_id + span_id only; no hidden extras. |
| file target | Forge default about 1 GiB; versioned target already configurable | Set `WYRD_BIFROST_FORGE_TARGET_FILE_SIZE_BYTES=10737418240` before fresh table creation, verify the resolved target; this interprets O2's 10240 MiB setting as 10 GiB. |
| sorting/partition | Hourly event-time partition; attributed dimensions then time DESC | Keep hourly time partitions; choose time-only DESC through existing layout edit. Record direction versus CH time ASC and mandatory Wyrd partitioning. |

The public layout edit replaces creation defaults, as allowed by the authority.
Describe the canonical table, bind `expected_table_version`, set the layout
before ingestion, and retain the resolved definition. Configure writer policy
before creation; changing an environment variable after loading does not
rewrite old files. If public setup cannot establish this definition without
writing fixture rows, use a separately excluded setup sample and remove its
whole isolated dataset before the measured fresh load; never count it in the
500M corpus. Do not add nullable pod columns that OTLP will never populate.

**Recommendation: run the current-product workload with partial index parity**,
clearly stated on every comparison, rather than bundle a new full-text product
into a benchmark. D3 must accept that concession. If exact parity is required,
stop before implementation of the benchmark: a separate design/spec must
settle full-text tokenization, ingestion/maintenance/recovery, index accounting,
and pod-name indexing. A fixture-only index, alternate native table, or a LIKE
substring predicate is not parity. No second raw copy or materialized view.

### Fit within 2 TB

Treat the limit as **2,000,000,000,000 bytes**, not 2 TiB, and account for
existing disk usage too. The raw estimate alone is 500M × ~2,200 = ~1.1 TB.
Keeping that file beside data/indexes, rewrite inputs/outputs, WAL and Postgres
could exceed the ceiling. Stream and delete no unrelated artifacts.

The following is an admission envelope, **not a storage prediction**. Budgets
are decimal GB and sum to 2,000 GB. A smaller/free-space-constrained host uses
the smaller effective envelope and can refuse the run.

| Consumer | Maximum planning allocation GB |
|---|---:|
| Current live Parquet including embedded indexes, Iceberg metadata | 1,050 |
| Retained obsolete files plus simultaneous compaction output/copies | 300 |
| Scribe WAL, staging, and unpublished objects | 150 |
| Postgres data, catalog/indexes, and pg_wal | 100 |
| Oracle/Forge scratch and spill | 100 |
| OS, builds, existing artifacts, controller logs/provenance/results | 100 |
| Free reserve | 200 |

Reference 500M-equivalent stored sizes would be about 337–513 GB if linear;
that is a sizing hint only. Wyrd's canonical overhead, Variant layout,
compression and unindexed scans have not been measured. No assumed compression
ratio makes the full run safe.

Before the full load, run the same separate diagnostic experiment at 5M rows,
settle it, measure every category and peak, estimate 500M live bytes with at
least 25% margin, and include one concurrent rewrite, observed backlog growth,
Postgres growth and existing occupancy. Remove only that experiment's owned
sample dataset through normal lifecycle; do not retain it beside the full
corpus. The 500M plan is admitted only if the projection fits each allocation
and the total envelope. A capacity refusal is a useful failure, not authority
to reduce the required row count. A failed envelope returns to the maintainer
for storage/product choice; it does not trigger a new architecture silently.

During load and convergence, use the installed portable filesystem/OS reader
(`sysinfo`) and existing storage/governor owners to sample actual free bytes,
file inventory, Scribe backlog, scratch, Postgres size and pg_wal. The scratch
limit bounds scratch; it does not bound live/retained data. Serialize large
rewrites using existing maintenance concurrency and bound the producer. Stop
new generation before any allocation/reserve is exhausted; retain at least
one maximum admitted batch plus in-flight rewrite headroom to drain safely.
WAL/staging and retained snapshots must be reclaimed by normal Scribe/Forge
ownership, never manual file deletion. If retention cannot converge inside
the envelope, report INVALID/storage-insufficient; do not bypass fences or
retention to finish. Budget abort still leaves a diagnostic record.

### Settle → check storage → freeze → query

After the ingest clock stops, record separate time-to-published and
maintenance-settle durations. Let Scribe drain and publish, Forge promote and
compact toward the declared target, and normal retention/GC reclaim obsolete
objects. Define settlement through owners' durable inventories: no live
memtable/staged/unpublished corpus rows, no outstanding corpus maintenance,
no in-flight rewrite, stable active manifest/file-list inventory for two
maintenance observations. If eligible work remains, do not call it settled.

Pin the tenant's table version, Iceberg snapshot and active object list. Check
**storage-level** rows: sum active data-file record counts from the authoritative
Iceberg/file-list inventory, cross-check Parquet footers, require no deletes,
and require exactly 500,000,000. Also issue a normal SDK COUNT(*) and compare
it with the acknowledged/generator count. A successful HTTP response or tenant
SQL count alone is not the storage-level row check.

Collect object lengths before cache reset and again after queries. Count every
unique active data object once; embedded Bloom/footer/page-index bytes are
already inside the Parquet object and must not be added again. Include any
separate index object if later product work introduces one. Keep these readings:

- `active_data_and_indexes_bytes`: reference-compatible compressed corpus;
- `metadata_bytes`, retained old versions/orphans, WAL/staging, scratch and
  Postgres/pg_wal, each separate, plus steady and peak total occupied bytes;
- reference compression = exact generated NDJSON bytes /
  active_data_and_indexes_bytes; also show raw / full settled deployment bytes.

Object lengths match the reference's metastore compressed-byte convention;
filesystem free-space deltas capture allocation overhead and enforce the host
ceiling. They are different readings. Include required managed columns and
attribute shredding in the corpus, not an idealized 27-column-only estimate.

Stop corpus maintenance only after it has converged, through the existing
configuration/lifecycle; do not freeze an unfinished rewrite. No ingest,
compaction or GC may change the pinned corpus during queries. Background
accountability is preserved. After measurement verify snapshot/object lengths
and storage rows unchanged; a mutation or missing source makes the run INVALID.

### Portable cold method and hardware

There is no portable API here for a verified OS cache drop or a hard per-process
8-vCPU/64-GiB partition. The platform rule prevents copying the reference's
Linux cache command. Restarting the process clears process state, **not**
filesystem cache. Reading a large eviction file or synthesizing memory pressure
is neither proof nor permitted quiet-host measurement.

**Recommended cold method (D4):** one graceful persistent-server restart after
settlement/storage checks and before the suite, retaining the dataset and
Postgres. Health/metadata readiness only; no corpus warmup. Label run 1 for
each case as the requested cold statistic with
`cold_method=process_restart`, `reset_scope=suite`,
`os_page_cache_cold_verified=false`; report the visible caption
**“cold proxy: first run; OS cache unverified”**. Never print an unqualified
storage-cold result. Later cases may inherit pages from earlier queries.
The source published reset scope and driver conflict are documented in
[workload.md](workload.md#cache-reset-evidence-conflict).

Alternatives requiring maintainer selection: (a) an operator-managed reboot
of the dedicated host before a resumable query-only phase, with attestation
and no corpus reads before the phase; portable in principle but requires a
new lifecycle handoff for controller/Postgres too; (b) accept cold unavailable
and retain first-run timing only. No benchmark-triggered reboot is proposed.
A genuinely storage-cold-per-query contract would need nineteen such resets
and differs from the published suite-wide reset procedure. Do not claim a
process restart satisfies either OS-cache contract.

**Recommended hardware treatment (D5):** use the single host as required and
record the deviation. Set existing cooperative limits
`WYRD_BIFROST_EFFECTIVE_CPU=8` and
`WYRD_BIFROST_PROCESS_MEMORY_LIMIT_BYTES=68719476736` (64 GiB) if the host can
provide them **plus** separate Postgres/controller/OS headroom. Record resolved
limits, actual host CPUs/RAM/architecture, storage type/capacity, file-store
filesystem and server/driver/Postgres measurements. Wyrd derives managed memory
below the process ceiling for server headroom. These settings govern Wyrd
resources, not OS CPU affinity, RSS or host page cache; `--server-cores=8`
only calibrates existing L2 and is not a pin. Do not use it to claim parity.

The i8g.2xlarge reference has 8 vCPU, 64 GiB, local NVMe, ARM hardware and a
separate c7g.2xlarge driver. Co-resident generation, result decoding and local
Postgres consume this host; server-side timing removes client clock/network
time but not CPU/disk/memory contention. A non-NVMe or non-ARM host and a
larger host OS cache remain declared differences. If hard allocation/separate
client are essential, provision suitable hardware outside this codebase; the
requested local run cannot promise identical setup. This alternative changes
the maintainer's deployment constraint and is not silently selected.

### Exact query timing and statistics

Issue each query through `wyrd_client::Bifrost` → HTTP `POST /v1/query`, consume
its entire terminal-safe Arrow stream immediately, and require a success
terminal and exact expected answer. No query concurrency. Preserve original
execution order (interleaved count/rows for each predicate, then aggregates)
and fixed time bounds. The whole host must pass existing quiet-host checks;
stop generator workers before queries and keep builds/tests/other benchmarks
absent. Poll diagnostics outside each measured query; no parallel load.

The public `QueryTerminalFrame` currently has **no elapsed field**. Reuse
`query/routes.rs::QueryTiming` at the existing `bifrost.query` boundary and
`wyrd-benchmark` snapshots. The boundary is HTTP arrival through handoff of the
terminal frame, including server planning/admission/execution/framing, rather
than client round trip or time to first batch. For each serial request, bracket
it with snapshots using a closed case label; require exactly one successful
server timing sample and reconciled scan facts. Record that sample's HDR bucket
bounds (3 significant digits), using the upper bound as the displayed elapsed
estimate and preserving its interval. Record client RTT only as diagnostic.
No public timing field, second recorder, sidecar, ad hoc timer or log parser
is needed. This is server-query-surface instrumentation; it is not a claim
that an elapsed scalar already exists in the production response.

Disable Oracle Parquet `metadata_cache_bytes` and `data_cache_bytes` with zero
budgets for **all five runs** through existing settings, retaining that resolved
configuration in the record. Oracle has no inspected query-result cache flag
on `BifrostQueryRequest`; prove no result/plan-answer memoization bypasses work
in the selected build rather than invent a `use_cache=false` request property.
Normal catalog/cut caches remain process metadata. OS cache is intentionally
allowed to warm between runs and cases. Persist each query's scanned
rows/bytes/files/row groups; the original comparison omitted much of that work
evidence. Repeated results alone do not prove a fresh execution.

For each case i retain all ordered samples `t[i,1..5]` and intervals:

```text
cold[i] = t[i,1]                 # proxy status accompanies it
hot[i] = min(t[i,2], t[i,3])     # never use runs 4/5 to improve this
suite_hot_ms = sum(hot[1..19])
geometric_mean_ms = exp(sum(ln(hot[i])) / 19)
class_hot_ms = sum(hot[i]) for Q1–Q8, Q9–Q11, Q12–Q19
```

Runs 4/5 still appear and contribute to reported median/p50/p95/p99/min/max
and spread as in the source driver. Those are descriptive, not statistically
strong tail estimates from five observations. Preserve interval bounds when
deriving min/sum/geomean; overlapping intervals are not a demonstrated win.
Missing/zero/unresolved elapsed values invalidate the aggregate; do not omit
hard cases, clamp zeros to epsilon, or substitute a timeout's limit. Retain
failed samples and their errors; do not retry timed cases until a faster result
appears.

The generator keeps bounded reference answers while generating: eight
predicate counts, namespace counts, hourly all/failed counts and at most 100
newest original records per predicate (boundary timestamp ties handled as
unordered valid ties). This is independent of SQL execution, not precomputed
server data. All five query results must agree with those answers; absent
needles are reported as empty. Do not use answer computation as timed work on
the query host. Store compact answers/provenance, not the whole dataset.

### Record and comparison reader

The canonical `capacity report` must render the nineteen Wyrd rows, all five
samples/cold proxy/hot/statistics, the three class totals, suite sum and
geomean, ingestion, storage/compression, storage rows and comparison caveats.
Embed reference data/provenance in the run record so regenerating a report
does not depend on a scratch clone or network. Extend `capacity compare` and
the existing results reader only as necessary to preserve this experiment's
identity and expose its readings; no separate spreadsheet or report builder.

The context table includes all three 1B systems from Round 2, their cold/hot
per-query samples, totals/geomean and storage, with the shared fan-out ingest
figure printed once. Exact samples/class totals are retained in the linked
research. Published totals: CH 11,694 ms; O2 Parquet 3,487 ms; O2 Vortex 2,533 ms.
Published geometric means: 181.9 / 66.3 / 54.3 ms. Storage including indexes:
1,026.5 / 673.5 / 710.7 GB; compression 2.14× / 3.26× / 3.09×.

If displaying wins/ties/losses or latency ratios versus those columns, label
them **descriptive at unequal scales/configurations**; they are not a
performance verdict. Use the source's millisecond-floor caution (differences
up to 3 ms are descriptive ties, also tie overlapping Wyrd timing intervals).
Empty/mismatched-selectivity cases are incomparable, shown separately rather
than counted as wins. Do not normalize a Wyrd latency by two, extrapolate to
1B, or call the corpus load a faster per-engine ingest result.

For Wyrd build-to-build comparison, require VALID records with the same host,
500M scale, fixture revision, settings, transport/index/cold method, query
manifest and sampling protocol. Existing record rules retain clean full runs
in `benchmarks/history/bifrost/openobserve-logs/`; smoke, dirty, selected-step,
profiled or paced runs stay diagnostic under `target/benchmarks/`. Protocol
concessions must be approved declaration controls before a run can be VALID
for this named contextual experiment. They never make it an exact reproduction.

## Evidence and rationale

Repository evidence inspected, not executed as experiments:

- [Benchmark rules](../../../../benchmarks/AGENTS.md), [levels and measurements](../../../../benchmarks/README.md),
  `mise.toml::bench:l2:bifrost`: existing L2 is a competing workload, not this corpus/suite.
- [Capacity query owner](../../../../crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs),
  its `run.rs`, `isolated.rs`, `competing.rs`, `main.rs`/`levels.rs`,
  [experiment owner](../../../../crates/wyrd/wyrd-testing/src/experiment.rs):
  existing fixture/step, lifecycle, declaration, validity and reader reuse.
- [Canonical log ledger](../../../../crates/vala/vala-bifrost-redux/src/tables/logs/records.rs),
  `logs/projection.rs`, `tables/mod.rs::attributed_layout`,
  [writer properties](../../../../crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs):
  log types, default Bloom list, replaceable layout, 0.01 FPP, ZSTD3/128 MiB row groups.
- [OTLP HTTP owner](../../../../crates/wyrd/wyrd-server/src/http/otlp.rs),
  `gate/limits.rs`: existing logs protobuf/JSON endpoint and bounded admission;
  no compatible raw NDJSON logs route found in inspected HTTP registration.
- [Query timing](../../../../crates/wyrd/wyrd-server/src/query/routes.rs),
  [terminal contract](../../../../crates/wyrd-spec/src/vala/api.rs),
  [recorder registry](../../../../crates/shared/wyrd-benchmark/src/registry.rs):
  timing exists in benchmark histograms, not the public terminal.
- [Resource configuration](../../../../crates/wyrd/wyrd-server/src/config.rs),
  `vala-bifrost-redux/src/resources.rs`, `wyrd-runtime/src/resources.rs`,
  `release_server.rs`: cooperative CPU/memory/scratch, zero cache budgets,
  portable process observation and restart support, no hard portable pin.
- [Bifrost authority](../../../../architecture/bifrost-design.md), owning sections:
  Table and row identity / Table versions and publication, Storage format and
  Variant / Variant SQL, Durability and visibility, Oracle Admission and
  memory, Maintenance managed compaction and publication, Telemetry, Public
  surface. These govern the proposed flow; source behavior cannot override them.
- [Reference evidence](workload.md): generator/schema/query/index provenance,
  exact literals/27 fields, five-sample data and source ambiguities.

The requested `target/benchmarks/bifrost/writes-parquet` directory is an
artifact directory inside the supplied worktree, not a separate Git checkout.
This packet is intentionally rooted at the actual worktree's `changes/`.

## Decisions and alternatives

The maintainer specified 500M rows, Wyrd only, a 2 TB ceiling, all comparison
criteria, L2 ownership and design-only changes. Those are settled. Streaming,
bounded queues, fixed literals, public query/OTLP paths and existing readers
are proposed reversible implementation choices supporting those outcomes.

**These recommendations are not maintainer approval.** Material decisions:

| ID | Recommendation | Alternative / consequence |
|---|---|---|
| D1 — L2 shape | Add named finite-corpus L2 experiment and 24 h budget; preserve existing competing plan. | If L2 must always mean capacity/overload/recovery, this serial reference workload needs a taxonomy decision before specification. Mixing the two does not reproduce the reference. |
| D2 — identical bytes | Accept typed-value-preserving OTLP/HTTP protobuf, uncompressed, as an explicit contextual deviation. | Literal NDJSON bytes require an independently approved ingest surface/product contract first; no existing compatible endpoint was found. |
| D3 — index parity | Run with trace + span embedded Blooms and unindexed message/pod predicates; disclose missing FTS/pod index and distinct physical representation. | Exact index parity requires separate full-text and indexed-pod product work before this benchmark can qualify. |
| D4 — cold | Report run-1 process-cold proxy after one suite-wide persistent-server restart; OS cache unverified. | Require operator reboot/resumable phase, or declare cold unavailable. Neither is silently substituted with a Linux command. |
| D5 — hardware | Accept co-resident driver/Postgres and cooperative 8 CPU / 64 GiB settings with actual host disclosure. | Hard parity requires externally provisioned resource-isolated hardware and a separate client, changing the stated local setup. |

## Standards conflicts

Keep root `AGENTS.md`'s locked portable-platform rule. The reference's cache
reset and hard OS resource controls cannot be copied. No standard exception
or authority update is approved by this design. D4/D5 choose a declared
comparison concession or external provisioning, never an in-code exemption.

`benchmarks/README.md::Levels` currently defines L2 as timed competing work.
D1 requires a scoped amendment in that owning document during implementation;
this design does not change it. All existing quiet-host, telemetry, durable
acknowledgement, tenant, recording and history rules remain applicable.

No public API, canonical column, index implementation or architecture change is
part of the recommended benchmark-only approach. Choosing exact wire/index
parity changes that scope and requires its own owner-led product design.

## Open questions

D1–D5 remain explicit maintainer decisions; no questions were sent in this
turn. Until accepted, the recommended contextual experiment is not an approved
substitute for the request's exact setup. No portable fully equivalent setup
was established.

Bounded implementation proof remains: golden generator compatibility under
pinned RNG/serialization versions; all SQL translations on the pinned
DataFusion fork; public layout setup and effective cache settings; settlement
inventory access and actual projected peak disk. These are validation work,
not assumed evidence. A 5M admission run can refuse the full run without
changing the design's required 500M scale.

## Handoff

Keep `exploring` while D1–D5 materially affect what is claimed. Do not create a
spec/task packet or begin implementation from these notes. Once the maintainer
selects the comparison contract, reconcile it here, finalize, then use
`wyrd-spec` and human approval before `wyrd-plan`.

Implementation reach after those decisions:

| Owner | Necessary change |
|---|---|
| `mise.toml`, `benchmarks/README.md` | Named opt-in L2 command; finite-corpus lifecycle/budgets, declared differences and readings. |
| `capacity/main.rs`, `query/mod.rs`, focused reference workload/fixture module(s) | Dispatch/declaration; bounded source generator, OTLP batch mapping, exact query translations, answer summaries, settlement and storage controls. Do not replace existing bulky-log fixture. |
| `wyrd-testing/Cargo.toml` / workspace dependency owner | Benchmark-only source-compatible RNG declarations and existing UUID v5 feature as needed; no production dependency. |
| `experiment.rs` and existing report/compare/results owners | Only finite-step representation/reader support the existing model lacks; retain five ordered samples and comparison context. |
| Existing `wyrd-benchmark` closed case registry | Register bounded nineteen-case labels if current labels cannot express them; reuse timing/scan series. |
| Existing capacity unit/PG journey test owners | Generator/mapping/statistics/invalidity proof and real public-edge query/ingest/settlement verification below. |

Observable proof before qualification:

1. Golden generated rows preserve 27 names/types/values, PRNG draws and exact
   NDJSON byte counting; replay with recorded batch time is byte-identical.
   Map round-trip tests protect binary IDs, integers and message bytes.
2. A small real-SDK → local server + repository Postgres → SDK journey ingests
   OTLP, settles to physical Parquet, reconciles inventory/footer/SQL counts,
   executes all nineteen exact predicates and 27-column row projections, and
   matches the generator's answers including empty needles and timestamp ties.
3. Statistics tests use five intentionally distinct values to prove hot ignores
   runs 4/5, cold takes run 1, and all/class totals and geometric mean use every
   required case. Reader tests include fixed reference samples, correct units,
   inequality/cold/index notices, and unchanged legacy experiment rendering.
4. Negative proof rejects OTLP partial/ambiguous acceptance, wrong storage
   count, a missing/degraded terminal, missing/reset timing population, zero or
   timeout timing, corpus mutation, incomplete compaction, disk-budget shortfall
   and a busy host. Retain errors/partial diagnostics; never promote them into
   a favorable result.
5. The separate 5M diagnostic establishes storage admission. Only a clean full
   500M run with 95 successful, checked server timings and unchanged settled
   inventory qualifies the new experiment. Run targeted format/lints and the
   owning capacity unit/PG lanes, using each named test's exact nextest command
   at implementation time; no test or benchmark was run for this design.

This handoff was checked by its author against the requested flow, criteria,
source records, proposed ownership and unresolved decisions. It has no claim
of independent review or experimental proof.

# TASK-012-R2 design: benchmark and fix hot reads

Status: finalized. Scope approved as spec revision 17 on 2026-11-18.

## Start here: the actual queries

[SQL catalog and page schedules](ui-sql/README.md) ·
[SQL and parameter values as JSON](ui-sql/queries.json)

| UI workload | Concrete SQL | What it exercises |
|---|---|---|
| Traces | [traces.sql](ui-sql/traces.sql) | Latest roots, five overview aggregates, five distinct facets, dependent trace counts |
| Verification | [verification.sql](ui-sql/verification.sql) | Paginated executions, seven facets and two charts, including the real status/checkpoint/result joins |
| Logs | [logs.sql](ui-sql/logs.sql) | Latest records, severity timeline and four facets |
| Metrics | [metrics.sql](ui-sql/metrics.sql) | Facets, names, scalar windows, histogram calculations and group keys |
| Filter/page/detail variants | [variants.sql](ui-sql/variants.sql) | Child-only trace matches, cursors, full payloads, separate-series labels, nested grouping and 24h metrics |
| Shared stat queries | [statstrip.sql](ui-sql/statstrip.sql) | Counts, percentiles, token/cost totals and time buckets |

The catalog was extracted by evaluating the original pure query builders from
commit `4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36`. JSON retains their exact SQL
strings and source hashes; SQL files add whitespace for reading. Example IDs,
metric names and timestamps are design inputs, not observations from live data.
The driver must bind actual preceding results for trace counts/series labels.
Resolve example IDs and metric names against the deterministic fixture and
freeze the resolved binds before measurement. Positive scalar, histogram and
queued-verification cases need known matching inputs and expected output
cardinalities/values. An aggregate row over zero matches is not positive
coverage; retain valid zero values and test intentional emptiness separately.

There are 69 exported statements/examples, not 69 mandatory timed L1 steps.
Use the original page schedules plus representative standalone cases as specified
below. The fixture data, expected answers and executable L1 driver remain to be
implemented. No server query or performance experiment ran during extraction.

## Problem and outcome

Fresh-data UI queries lag until the first seal. Existing hot L1 prelearns a
Variant layout and mostly checks integer aggregates; it does not measure these
page SQL bundles. Hot capture opening also computes every retained result batch
before downstream execution can consume or cancel it.

The outcome is an honest original-SQL baseline, then faster shared hot reads
with identical results and resource/terminal safety. Query rewriting and page
rendering are outside this task. No new numerical UI SLO is agreed.

## Execution order

```text
Frozen original SQL + deterministic evidence
                ↓
Existing Bifrost L1 harness: canonical-hot → staged → published
                ↓
Clean VALID baseline saved — production reads still unchanged
                ↓
Lazy shared hot source + conservative scalar identity pruning
                ↓
Same SQL, data, binds and schedules → matched qualified comparison
```

1. Extend the existing capacity experiment/setup, public-client runner, typed
   answer validation, placement controls, metrics, profiles and history. Copy
   the catalog into the benchmark fixture owner; do not build/import the UI.
2. Load default-500-request-equivalent and 10× deterministic evidence through
   existing native/OTLP paths. Record actual table rows, batches, bytes and a
   digest. Include multiple services/Verifiers, repeated trace IDs, nulls,
   timestamp ties, nested payloads, queued states/checkpoint revisions, late
   terminal results, metric resets/histograms and exact integers above 2^53.
3. Measure identical rows and absolute ranges in canonical memtables, staged
   and published storage. A separate prelearned-layout arm uses identical
   measured values and an out-of-range teaching sample. Placement/ingest drain
   happens outside windows; verify residence and exact row reconciliation.
4. Measure representative individual queries at one client and original page
   bundles at one/eight pages, both data sizes and relevant residences. Keep
   first-request observations separate from equally warmed reads. Declare
   adequate windows/budget for three repetitions and existing minimum samples
   per repetition (1,000 for p99, 200 for p95); insufficient samples are INVALID.
5. Save the full clean, unprofiled baseline before changing production hot reads.
   On that revision/data, capture a separate diagnostic CPU profile of the slow
   hot/Parquet representatives, with heap/allocation evidence where material.
   Attribute the gap to CPU, waits, layout, scanning and allocations before
   optimization, and connect measured costs to the selected changes. A profile
   does not replace qualification. If evidence contradicts the approach, report
   it and options before redesign. Freeze corpus/data/method hashes throughout
   optimization; then change the shared source and repeat the same experiment.

## Shared source changes

| Today | Selected change |
|---|---|
| Capture opening eagerly computes result batches | Retain descriptors/statistics/owners; metadata-only selection during open |
| Filters/projection/reconstruction precede demand | Compute bounded batches only when the execution stream pulls |
| Repeated expression/statistics/allocation work | Adapt once per generation; reuse recorded facts and zero-copy buffers |
| Identity predicates lack string/binary bounds | Extend existing scalar statistics conservatively; measure append CPU/memory cost |
| Computed outputs accumulate before execution | Keep query reservations with actual outputs; release as consumers dispose them |

Extend `scribe::LiveCapture`, `ReadableSource`, tail RPC, Oracle follower and
existing `MemtableSource`. Reuse current byte-sized partitions, static pruning,
Variant typed-leaf/fallback evaluation, query memory and shared CPU lanes.
No new scanner service, worker pool, index service, cache or source selector.
Moving eager collection onto another thread alone is insufficient.

Keep field-ID/schema adaptation, all-match proofs, conservative unknown bounds,
filter/projection/fetch rewrites and dynamic/volatile expression semantics.
A sorted LIMIT may still require all input. Large retained appends must not
become unbounded evaluation units. Keep captures, shared Arrow charges, query
outputs and staged leases alive for their actual lifetimes, including late
children after cancellation. Preserve signed attempts, pinned cuts, publication
deduplication and valid terminals across local/remote followers; never reopen
fresh state or accept successful partial results after a late failure.

## Evidence and completion

Compare server/client latency, first batch, full bundle/primary result times,
correct pages/s, CPU, memory, refusals and scanned rows/batches/bytes. Attribute
planning/admission, capture opening, expression preparation, filtering,
projection/reconstruction, execution and client decode. Do not add nested times
or subtract percentiles. Report the residual hot/Parquet gap and first-request
cost instead of assuming media or layout is the whole cause.

Every remaining material contributor to the shared-read slowdown needs its
stage/owner, evidence, before/after cost and disposition recorded. Small gains
with unexplained material lag are not completion. Unresolved material work
keeps the task incomplete unless the maintainer explicitly accepts the
evidenced limit; report outside-scope costs without silently expanding the task.
Benchmark VALID means a trustworthy measurement, not a completed optimization.

All typed answers must match across residence/layout, including nulls, decimals,
timestamps and Variant metadata. Original integer checks stay intact. Keep
mixed-source/schema/publication and lifecycle failures in connected coverage.
Qualify the original workload on the optimized engine and unchanged full
Bifrost/Gateway/Verification L1 against compatible history: no correctness,
refusal, memory or write-performance tradeoff. If the selected approach fails,
report evidence/options before redesigning it. Commits remain maintainer-owned.

## Contract

Revision 16 REQ-018/019/022–024 supplies inherited no-regression, CPU and custody
constraints. Bifrost architecture already requires bounded captured pull-driven
streams. Revision 17, approved on 2026-11-18, extends REQ-024 beyond resource-manager
integration for the selected optimization.

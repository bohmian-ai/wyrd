---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 22
parent_task: TASK-007
depends_on: [TASK-007]
requirements: [REQ-006, REQ-029, REQ-030, REQ-031, REQ-032, REQ-033, REQ-034]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-009]
acceptance: [AC-001, AC-005, AC-010]
---

# TASK-008 — close the hot read path and its evidence

## What changes and why

Remove avoidable staged-read costs: use the published-file reader and planning
owner on the Scribe that holds the files, retain real run facts, and push the
plan's projection into staged and memtable scans. Fix the demonstrated time
warning and complete memory scan measurements. Benchmarks measure performance;
correctness belongs to unit and SDK journey tests in their owning code.

Preserve logical results, tenant proof, the publication cut, exactly-once
generation selection, capture identity, deadlines, fences, cancellation and
query-bound leases. Discovery, listing and `LiveTableRoutes` are unchanged:
the leader never plans another node's files, and no footer crosses the network.

Non-goals: decoded caches, retained staged Arrow, new Card kinds, SDK/public
HTTP changes, benchmark modes, environment variables, guards, plan caching,
writer recipe changes or `writes-parquet` changes. Keep existing stage/publish
controls and the `prepare_hot` gauge check unchanged.

## Six steps

Execute in this order, each committed on its own when commits are authorized.
This planning edit does not authorize implementation or commits.

### 1. Same reader

In `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`, build the
`hot_parquet_scan` `HotObjects::Staged` reader exactly as
`TenantWarehouseReads::reader_factory` does: `CachedParquetFileReaderFactory`
with the node's `FileMetadataCache`, and `RangeCache` through
`TenantParquetReaderFactory`, over `LocalFileSystem` instead of the warehouse
store. Confirm metadata-cache entries are validated against `ObjectMeta`
before relying on path keys. Delete the uncached branch and the
"rewritable/never cached" claim.

### 2. Real file facts

`LiveCapture::read` / `CapturedRead` in `scribe/memtable.rs` hand each staged
run to the local reader with its size, row count and time bounds from the
`RunFooters` already held, instead of a bare path. Retain exact per-run sizes
at staging/recovery; `StagedSource.bytes` is a member total.

`ScribeTailResolver::live_leaf` in `oracle/follower.rs` stops stat-ing paths
and inventing unknown metadata, passes the table time column to
`hot_parquet_scan`, and uses the same `keep` pruning and footer row-group
partition cap as `OracleTableProvider::persisted_input`, through one shared
owner. Preserve `planned.files` scan candidates versus `planned.kept` footer
sizing and conservative treatment of unknown bounds. Keep local capture and
lease ownership; routing and listing do not change.

**Behavior.** Staged and published scans share planning and the cached reader.
An excluded run is never opened; a repeated query rereads no footer. Early
stream drop releases its lease.

**RED.** V-A1 proves these properties with separate scan-open and footer-read
counts; the current handoff loses run facts and uses divergent planning.

**GREEN.** Complete the shared reader and local metadata handoff above, then
run V-A1.

**REFACTOR.** Delete duplicate staged planning and reader construction.

### 3. Projection reaches the scan — maintainer decision 1

The live assignment carries the plan's projection expressions for staged and
memtable reads. The follower runs projection pushdown as well as filter
pushdown. `ReadableSource::project` evaluates the expressions on physical
arrays without reconstructing an unused whole Variant. Keep the ordered input
dependency closure, including hidden tenant and predicate inputs, separate
from output expressions and aliases.

Keep A2's existing codec and authority rules: an ordered, nonempty projection
of serialized `PhysicalExprNode` expressions and aliases through the Oracle
physical expression codec; opaque bytes/names in `wyrd-spec`; a fresh proto
tag after reserved tags 7/8. Bind exact ordered bytes and aliases into a new
assignment-digest domain version and the projected fragment's plan fingerprint.
Expression-less assignments fail closed; public API versions do not change.

Preflight checks assignment/fragment agreement before IO. Decode validates
columns/indices against authenticated inputs, function availability,
aliases/order, logical output schema and the full-table fingerprint. Missing,
malformed, tampered or mismatched projection produces no output or whole-column
fallback. Preserve tenant proof even for `count(*)`, rebind downstream
references, and retain filter semantics without double application or index
drift. Local and remote dispatch execute the same bound fragment. Whole-value
requests still use standard reconstruction; scalar requests do not.

**Behavior.** A staged shredded-key read reads only its leaf and required
metadata/residue; a memtable scalar read reconstructs no whole Variant.
Logical results remain identical through memtable, stage and publish.

**RED.** V-A2 proves physical leaf reads and malformed/tampered projection
refusal; V-JOURNEY covers the SDK lifecycle and remote Scribe execution.

**GREEN.** Extend the existing assignment, codec, leader, follower and
`ReadableSource::project` together. Run V-A2 and V-JOURNEY plus V-A1.

**REFACTOR.** Remove whole-column-only assumptions and update construction
sites and codec/security fixtures without a compatibility path.

### 4. B — mixed known and unknown time bounds prune without warnings

**Behavior.** A `start_time_unix_nano` predicate over the actual hot container
statistics excludes known-disjoint batches/runs, keeps unknown containers,
and emits no conversion warning. Nulls, absent/inexact bounds, nanosecond
precision and time-zone-bearing timestamp fields remain correct.

**RED.** First add the planned V-TIME regression
using `batch_statistics`, `set_time_bounds`, projected/inexact statistics and
`BatchPruning`/`PrunableStatistics` as the hot path does. Mix typed exact bounds
with the unknown containers that actually reach it. Assert the typed min/max
arrays, keep mask and warning absence; prove the failing warning path, not
merely a timestamp arithmetic failure. Record the concrete failing inputs.

**GREEN.** If Wyrd supplies a wrong type/unit/timezone, invalid exactness or
misaligned container statistics, fix that shared construction owner. If valid
schema-aligned inputs fail because DataFusion inserts untyped
`ScalarValue::Null` among typed values, fix `datafusion-common`'s
`PrunableStatistics::get_exact_column_statistics` in the pinned fork with
schema-typed unknowns, with its own focused regression. This evidence selects
the owner; it is not an architecture choice. Format fork changes with
`mise exec -- cargo +nightly fmt` in the fork, then update every root
DataFusion patch pin and lockfile coherently. Never edit Cargo's checkout or
suppress warnings. Rerun V-TIME and the earlier scenarios.

**REFACTOR.** Keep one construction/conversion rule at the demonstrated owner;
remove any workaround made redundant by that fix. Unknown statistics must
still fail open for pruning, never discard potentially matching rows.

### 5. C — memory reads report measured rows and bytes once

**Behavior.** Count the physical input rows examined and physical Arrow bytes
accessed by the projected/predicate columns after batch pruning, before row
filtering and scalar output reduction. For shredded scalar reads count their
physical leaf dependencies, metadata and required residue, excluding unused
siblings. Deduplicate shared input buffers within a scanned batch; count a
batch again if execution actually scans it again. Excluded batches contribute
zero; an instrumented empty scan reports numeric zero. This is memory scan
work, not Parquet compressed IO, retained heap, sizing estimates, returned
rows or Arrow transport bytes. State the distinction in metric documentation.

Capture-side filtering/projection is already execution work: preserve its
facts even when it returns no rows. Carry those facts into the follower's scan
evidence without counting the same batch again in `MemtableSource`. Both
`NativeCompletion` and remote `WorkerFooter` carry them; Oracle sums mixed
memory/staged contributions once per validated completion. Missing facts remain
unavailable rather than zero, and cancelled/incomplete remote attempts cannot
claim successful complete facts.

**RED.** V-FACTS adds planned
pruned/empty/mixed memory facts and extends the existing aggregation test at
`exec.rs:2202`. Preserve generic uninstrumented memory absence; assert numeric
facts for the Scribe owner. Exercise both native and encoded completions,
repeated finalization, missing evidence and rejected terminal frames. Before
this step memory bytes are absent and remote rows have no transport field.

**GREEN.** Instrument the actual memory-read owner, extend `WorkerScanStats`
in `wyrd-spec` and the private proto with optional scanned rows, and propagate
through conversion, follower finalization, dispatch and Oracle's remote
accumulator. Preserve availability independently for each metric. Update the
current row aggregation that excludes remote scans.
Rerun V-FACTS and earlier scenarios.

**REFACTOR.** Keep accounting in the existing evidence flow, remove stale
“memory means no scan facts” assertions/comments, and prevent plan traversal
or multiple finalization from double counting. Add no query metric endpoint.

### 6. Measure

Run `mise run bench:l1:bifrost:hot`, then
`mise run bench:l1:bifrost:parquet`, exclusively, with no overlapping build,
benchmark or synthetic load. Keep clean unprofiled records through the existing
history workflow. Compare by step ID against the records below using
**server p99 ms**, **server CPU ms per query** and **scanned bytes per query**.
Show all three repetitions, medians and spread, host/build differences, memory
scanned rows/query, point queries/CPU-second and elapsed budget. Historical
missing memory facts remain missing; do not substitute estimates.

## Tests

Use unit tests in the owning modules and existing real-server SDK journeys.
New selectors are planned: confirm their actual names after adding them and
before RED. Run focused RED/GREEN proof during implementation, sequentially,
with `WYRD_LOG=info,vala_bifrost_redux=debug` while diagnosing; read the trace.

### V-A1 — same planning, reader, pruning, footer reuse and lease release

First selector is planned; the others exist. Cover steps 1 and 2 together.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::staged_and_published_share_planning_pushdown_and_footer_reuse)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately)'
```

### V-A2 — staged leaf, memtable scalar and fail-closed projection

First two selectors are planned; extend existing digest and codec fixtures for
ordered expression bytes/aliases, missing/malformed projections and tampering.
Both live sources require projection; invalid assignments fail before source IO.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::live_projection_reads_shredded_leaves_without_whole_variants)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::live_projection_mismatch_fails_before_source_io)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::assignment_authority::tests::every_assignment_field_changes_the_digest)'
mise exec -- cargo nextest run --locked -p wyrd-tonic --lib -E 'test(=private_conversion::tests::follower_assignment_v2_contract)'
mise exec -- cargo nextest run --locked -p wyrd-tonic --lib -E 'test(=private_conversion::tests::follower_assignment_v1_shaped_peer_is_rejected)'
```

### V-TIME — B's warning regression

Planned Wyrd regression; reproduce the real hot statistics and warning first.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scan_sizing::tests::nanosecond_pruning_preserves_unknown_container_bounds)'
```

If the test identifies the pinned DataFusion fork as owner, confirm the planned
`pruning::tests` selector there before RED, then run in the fork:

```sh
mise exec -- cargo nextest run --locked -p datafusion-common --lib -E 'test(=pruning::tests::mixed_timestamp_bounds_preserve_unknown_containers)'
mise exec -- cargo +nightly fmt
```

### V-FACTS — C's local and remote scan facts

First selector is planned; extend the other two for instrumented memory,
pruned/empty/mixed input and exactly-once native/remote aggregation. Generic
uninstrumented memory facts remain absent.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::live_memory_scan_facts_survive_native_and_remote_completion)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::scan_stats_preserve_non_file_absence_and_scan_aggregation)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::live::tests::live_staged_read_reports_scanned_bytes_to_the_query)'
```

### V-JOURNEY — SDK memtable, stage, publish and remote Scribe execution

Use existing `WyrdTestServer`, repository Postgres and the journey profile.
The scalar journey is planned; the lifecycle journey exists. Assert identical
logical answers and tenant refusal.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=telemetry::shredded_scalar_reads_match_in_memory_staged_and_published)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=telemetry::variant_layouts_survive_memory_stage_publication_and_restart)"'
```

The planned remote journey uses the existing `oracle` peer-network cluster
setup and covers staged/memtable projection, owning-Scribe placement and
fail-closed projection/tenant refusal:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::security::remote_shredded_projection_preserves_results_and_rejects_tampering)"'
```

## Acceptance by record

| Reference record under `benchmarks/history/bifrost/` | Required comparison |
|---|---|
| `hot/20261029T180639Z-471f69ff9.json` | Every matching hot step, both sources; shredded memtable 36.63 ms / 185.9 CPU ms/query, staged 98.57 / 258.8 / 19.71 MB/query; long-tail memtable 23.69 / 99.0, staged 59.74 / 164.0 / 19.71 MB/query. Historical missing memory bytes stay missing. |
| `hot/20261027T171701Z-434aaf5c1.json`, `hot/20261029T154823Z-12fc33f08.json` | Historical `point-c1` and `point-c64` compare to current `point-memtable-c1/c64` by an explicit table mapping only; do not rename current IDs. Earlier 861–886 queries/CPU-s and 4.22–4.31 ms server p99 versus 794 and 4.90 in 471f69ff9. Note changed mixed-source fixture controls. |
| `parquet/20261029T134546Z-459617bae.json` | All matching parquet steps; shredded c1 16.92 / 75.5 / about 18 MB/query, long-tail c1 37.49 / 194.9 / about 29.4 MB/query. Preserve the warehouse path. |

Recompute figures from records. Ratios use corresponding staged/memtable steps
in the same record; a smaller ratio cannot pass if memory got slower.

| Record | Acceptance target |
|---|---|
| Full hot | VALID within 300 s; both shredded source steps improve p99 and CPU/query against 471f69ff9; staged/memtable p99 and CPU ratios shrink. Point memtable c1 reaches at least 861 queries/CPU-s; c64 server p99 is at most 4.31 ms. |
| Full parquet | No latency, CPU or scanned-byte regression on matching steps against 459617bae; no invented tolerance. |

Historical point figures remain absolute recovery targets despite changed
fixture controls, not proof of a like-for-like regression or its cause. Report
each miss as unfinished performance acceptance. A valid performance miss stays
VALID; these targets introduce no benchmark verdict rules. Do not promise
staged/memory parity or attribute gains without measurements.

## Closeout

Run `mise run fmt`, `mise run lints` and `mise run codegen:check` for changed
contracts, then `mise run verify:bifrost` on the integrated candidate. Record
actual failures and repairs. Update `architecture/bifrost-design.md` for live
expression projection and the shared local planning/cached reader; retain its
existing bench-only stage/publish controls, platform-admin `tenant_suspend`
and nonblocking `platform.authz` audit. Close TASK-007's outstanding gate and
evidence without erasing history. Add a compact implementation/verification/
result table and step-ID comparisons here, with every performance miss explicit.

Stop if implementation requires weakening tenant proof, logical results,
fail-closed decoding, leases, durability or a non-goal. If cache validation
cannot be confirmed, stop before relying on path keys. If B cannot reproduce
the warning, retain it as open and collect real inputs rather than guess a fix.

## Evidence

### Diagnoses

| Failure | Symptom | Evidence | Cause | Fix site |
|---|---|---|---|---|
| `ingest_bounds::native_ingest_bounds_expanded_bytes_not_rows` | `SELECT count(*)` failed with query execution failed | the live fragment carried an empty projection; the projection wire rejects an empty list | `LiveUnionProjection`, now on every Oracle session, let the live leaf absorb the zero-column `count(*)` projection | `LiveScribeExec::try_swapping_with_projection` refuses empty projections, so they stay on the leader; checked against every swap caller in `oracle/live.rs` |
| `distributed::remote_staged_footer_refusal_fails_closed` | `QUERY_EXECUTION_FAILED` where `QueryTenantInvariant` was expected | `peer_service` WARN `ParquetError(... "Corrupt footer")`; `HotFileSource::staged` sizes from `run.bytes` (`oracle/exec.rs:1380`) | the fixture re-encoded the run at a new length; step 2 plans with the recorded size, so the footer read fell inside the file before the tenant proof ran. Independent read-only diagnosis agreed: the fixture is at fault, and production matches step 2 | `rewrite_footer_tenant` now overwrites the owner id with an equal-length foreign id inside the footer, so only the tenant proof can fail. No other fixture rewrites a staged run |
| `published::source_transitions_preserve_exact_rows` | 2 cut reads where 1 was expected; 2 of 6 runs failed | trace: Forge's case-1 promotion commit at 39.602; warm query served a stale cached cut at 39.604; NOTIFY eviction; held query read twice | a latent test race, not caused by this diff. The warm-up exited on a cache hit while Forge's promotion NOTIFY was still in flight (independent read-only diagnosis) | the warm-up first waits until `file_tier_counts` reports no hot file left to promote; 8 of 8 runs pass |

### Step 5: memory scan facts

Reuse map. The capture already owns the memory read (`ReadableSource::project`). Facts ride the existing chain `CapturedRead` → `LiveTailBatches` → `MemtableSource` → `OracleQueryScanStats::visit` → `FollowerScanEvidence::finalize` → `WorkerScanStats` → `RemoteScanMetrics`, so no new channel was added. Leaf selection reuses the fork's public `ScalarFunctionExpr::required_input_fields`, the same declaration the Parquet projection mask uses.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Memory rows are counted after bounds pruning and before filtering; bytes are distinct buffers of the declared leaves | `MemoryScan`, `BufferTally`, `ReadableSource::project` in `scribe/memtable.rs` | `oracle::follower::tests::live_memory_scan_facts_survive_native_and_remote_completion`: 3 rows; scalar bytes < whole `attrs` bytes. A mutation that disables leaf selection fails it (206 of 206) | PASS |
| A filter-emptied capture keeps its facts; generic memory stays absent; an instrumented empty scan reports 0 | `live_leaf` builds the leaf when `scanned.rows > 0`; `LiveTailBatches::from_snapshot` is `None` | `oracle::exec::tests::scan_stats_preserve_non_file_absence_and_scan_aggregation` | PASS |
| Native and remote completions carry identical facts; a refused completion records nothing; repeated finalization does not double count | `WorkerScanStats.rows_scanned`, proto tag 8, `private_conversion.rs` | follower test above; `oracle::live::tests::live_staged_read_reports_scanned_bytes_to_the_query`; wyrd-tonic footer round-trip | PASS |
| Mixed memory/staged/remote sums once; a participant without rows leaves rows unavailable | `OracleQueryScanStats::finalize`, `RemoteScanMetrics::terminal_values` | exec aggregation test (mixed: bytes 42, rows `None`) | PASS |
| The metric documentation states that memory bytes are scan work | `WorkerScanStats` rustdoc, proto comment, `architecture/bifrost-design.md` Traces | `mise run lints`, `mise run codegen:check` | PASS |

Lanes:
- `cargo nextest run -p vala-bifrost-redux -p wyrd-tonic -p wyrd-spec --lib` under test Postgres: 1697 of 1698 pass. The one failure is the step-4 V-TIME test, which waits on the fork pin.
- Five journeys pass: remote shredded projection, the two shredded and layout telemetry journeys, staged footer refusal, and native ingest bounds.

### Step 4: V-TIME

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Unknown and inexact containers keep min/max pruning of nanosecond UTC bounds, with no "Failed to convert min values" warning | Fork `datafusion/common/src/pruning.rs`, commit `8d6d6bf58`: `PrunableStatistics::get_exact_column_statistics` fills unknown containers with a null typed by the schema, replacing the untyped `ScalarValue::Null` that `iter_to_array` rejected | fork `pruning::tests::mixed_timestamp_bounds_preserve_unknown_containers` fails before the fix and passes after it; `scan_sizing::tests::nanosecond_pruning_preserves_unknown_container_bounds` fails on `19796a13e` and passes on `8d6d6bf58` | PASS |
| One conversion rule at the demonstrated owner | The fix is at the fork owner. Wyrd's statistics types were already consistent, so there was no workaround to remove. The 11 pins, `Cargo.lock` and `ORACLE_DATAFUSION_REVISION` now point to `8d6d6bf58` | `oracle::codec::tests::plan_fingerprint_binds_the_pinned_datafusion_revision`; lib suite 1698/1698 under test Postgres; `mise run lints` | PASS |

Cache validation (step 1) is confirmed:
- `CachedFileMetadataEntry::is_valid_for` compares `ObjectMeta` size and `last_modified`.
- Staged run paths are unique per assembly key, shard and generation, and a lane's generations only move forward.
- The caches are process-local.

Path keys therefore never alias a different run. The limit is that a run rewritten in place at its recorded size would be served from a stale cache entry; that is outside the staging contract.

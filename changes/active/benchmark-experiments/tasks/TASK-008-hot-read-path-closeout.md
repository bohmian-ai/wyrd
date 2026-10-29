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

### Closeout gate

`mise run verify:bifrost` on `3b7f9d5af` is **red with seven failures. All seven predate this task.** Each one was rerun alone on HEAD and on the pre-task base `471f69ff9`, with the same result on both:

| Test | HEAD | Base | Read |
|---|---|---|---|
| `wyrd-testing` lib `bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot` | FAIL 322122546 vs 161061273 | FAIL, same | Stale assertion. It expects only the footer-cache charge; `storage/mod.rs:163` also charges the byte-range budget |
| `pg_verification_runtime::attempts_record_queue_wait_phases_terminal_latency_and_one_trace` | FAIL (2 queue waits vs 1) | FAIL | Deferred attempt at `verification/runner.rs:573` |
| `pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once` | fails only in the full lane; passes alone | same | Ordering within the lane |
| `verifier_building_blocks::custom_verdict_data_survives_readback` | FAIL "No field named outcome" | FAIL | Stale query; `bifrost.verification.results` has no `outcome` column |
| `verify_in_real_time::verification_staging_refuses_and_recovers` | FAIL `[6]` vs `[5]` | FAIL | One extra `execution_inputs` row |
| `server` `query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup` | FAIL 0 vs 1 (`query.rs:877`) | FAIL | Predates this task |
| `server` `owner_inspection::failed_wal_replay_is_role_local` | FAIL (`owner_inspection.rs:365`) | FAIL | Predates this task |

Every Bifrost read lane this task touches is green: oracle 62/62, scribe 42/42, forge 24/24. The seven failures are in the verification runtime, WAL replay, the audit journey and stale SDK/cluster tests, all outside TASK-008's write set, and are left for their owners.

### Step 6: exclusive benchmarks

Both runs were exclusive: nothing else built, tested or loaded the host, and the tree was clean at each start. The records are committed on `benchmark-records` (`3412ffce9`, not pushed).

| Run | Record | Verdict | Measured elapsed / budget |
|---|---|---|---|
| `mise run bench:l1:bifrost:hot` | `hot/20261029T223134Z-9a6e15750.json` | VALID, clean | 219.2 s / 300 s |
| `mise run bench:l1:bifrost:parquet` | `parquet/20261029T223613Z-9a6e15750.json` | VALID, clean | 284.8 s / 300 s |

Host and build match every reference record: AMD Ryzen 9 9950X, 32 CPU / 91 GiB, Linux, rustc 1.99.0 (b940084d7), release, `-C force-frame-pointers=yes`, line-tables-only. Other-process load before and after each run was 0.52–0.65 cores, under the 3.2-core limit. The server binary differs only by this task's code and the fork repin (1,226,533,368 bytes vs 1,221,643,760 in 471f69ff9). The hot controls are identical to 471f69ff9's, and the parquet controls are identical to 459617bae's.

Spread is the range across the three repetitions. A "—" for historical memory bytes or rows means the reference record never had them; no estimate is substituted. Memory bytes are distinct Arrow buffer bytes of the declared leaves (step 5), so they measure scan work, not storage, and are not comparable with staged Parquet bytes.

#### Hot, against `hot/20261029T180639Z-471f69ff9.json`

| Step | Metric | Repetitions | Median | Spread | Reference median | Δ |
|---|---|---|---|---|---|---|
| `point-memtable-c1` | p99 ms | 1.608 / 1.582 / 1.640 | 1.608 | 0.058 | 1.542 | +4.2% |
| `point-memtable-c1` | CPU ms/q | 1.298 / 1.299 / 1.298 | 1.298 | 0.001 | 1.260 | +3.1% |
| `point-memtable-c1` | bytes/q | 393,244 / 393,244 / 393,320 | 393,244 | 76 | — | — |
| `point-memtable-c1` | rows/q | 16,390 / 16,390 / 16,393 | 16,390 | 3 | — | — |
| `point-memtable-c1` | q/CPU-s | 770.5 / 770.0 / 770.1 | 770.1 | 0.5 | 793.9 | -3.0% |
| `point-memtable-c64` | p99 ms | 5.022 / 4.964 / 5.214 | 5.022 | 0.250 | 4.903 | +2.4% |
| `point-memtable-c64` | CPU ms/q | 1.783 / 1.783 / 1.792 | 1.783 | 0.009 | 1.745 | +2.2% |
| `point-memtable-c64` | bytes/q | 393,296 / 393,288 / 393,265 | 393,288 | 31 | — | — |
| `point-memtable-c64` | rows/q | 16,392 / 16,391 / 16,390 | 16,391 | 1 | — | — |
| `point-memtable-c64` | q/CPU-s | 560.9 / 560.9 / 558.2 | 560.9 | 2.7 | 573.1 | -2.1% |
| `point-staged-c1` | p99 ms | 3.557 / 3.416 / 3.398 | 3.416 | 0.160 | 3.598 | -5.1% |
| `point-staged-c1` | CPU ms/q | 2.872 / 2.788 / 2.801 | 2.801 | 0.085 | 2.868 | -2.3% |
| `point-staged-c1` | bytes/q | 222,766 / 222,211 / 222,778 | 222,766 | 566 | 213,323 | +4.4% |
| `point-staged-c1` | rows/q | 1 / 1 / 1 | 1 | 0 | — | — |
| `point-staged-c1` | q/CPU-s | 348.2 / 358.7 / 357.0 | 357.0 | 10.6 | 348.7 | +2.4% |
| `small-aggregate-memtable-c1` | p99 ms | 3.308 / 3.408 / 3.469 | 3.408 | 0.162 | 2.902 | +17.4% |
| `small-aggregate-memtable-c1` | CPU ms/q | 2.897 / 2.933 / 2.919 | 2.919 | 0.036 | 2.434 | +19.9% |
| `small-aggregate-memtable-c1` | bytes/q | 2,264,040 / 2,263,670 / 2,263,945 | 2,263,945 | 370 | — | — |
| `small-aggregate-memtable-c1` | rows/q | 94,324 / 94,308 / 94,320 | 94,320 | 15 | — | — |
| `small-aggregate-memtable-c1` | q/CPU-s | 345.2 / 340.9 / 342.6 | 342.6 | 4.3 | 410.9 | -16.6% |
| `small-aggregate-staged-c1` | p99 ms | 5.648 / 6.033 / 5.825 | 5.825 | 0.385 | 5.616 | +3.7% |
| `small-aggregate-staged-c1` | CPU ms/q | 5.235 / 5.338 / 5.303 | 5.303 | 0.104 | 5.220 | +1.6% |
| `small-aggregate-staged-c1` | bytes/q | 456,427 / 455,657 / 456,927 | 456,427 | 1,270 | 439,677 | +3.8% |
| `small-aggregate-staged-c1` | rows/q | 78,143 / 78,143 / 78,143 | 78,143 | 0 | — | — |
| `small-aggregate-staged-c1` | q/CPU-s | 191.0 / 187.3 / 188.6 | 188.6 | 3.7 | 191.6 | -1.6% |
| `span-shredded-memtable-c1` | p99 ms | 38.011 / 37.454 / 38.273 | 38.011 | 0.819 | 36.635 | +3.8% |
| `span-shredded-memtable-c1` | CPU ms/q | 164.740 / 164.516 / 164.870 | 164.740 | 0.354 | 185.897 | -11.4% |
| `span-shredded-memtable-c1` | bytes/q | 278,134,783 / 278,134,783 / 278,134,783 | 278,134,783 | 0 | — | — |
| `span-shredded-memtable-c1` | rows/q | 500,223 / 500,223 / 500,223 | 500,223 | 0 | — | — |
| `span-shredded-memtable-c1` | q/CPU-s | 6.1 / 6.1 / 6.1 | 6.1 | 0.0 | 5.4 | +12.8% |
| `span-shredded-staged-c1` | p99 ms | 90.571 / 91.095 / 89.981 | 90.571 | 1.114 | 98.566 | -8.1% |
| `span-shredded-staged-c1` | CPU ms/q | 255.172 / 254.138 / 254.310 | 254.310 | 1.034 | 258.772 | -1.7% |
| `span-shredded-staged-c1` | bytes/q | 19,709,951 / 19,709,951 / 19,709,951 | 19,709,951 | 0 | 19,709,951 | +0.0% |
| `span-shredded-staged-c1` | rows/q | 31,487 / 31,487 / 31,487 | 31,487 | 0 | — | — |
| `span-shredded-staged-c1` | q/CPU-s | 3.9 / 3.9 / 3.9 | 3.9 | 0.0 | 3.9 | +1.8% |
| `span-long-tail-memtable-c1` | p99 ms | 25.641 / 24.822 / 24.723 | 24.822 | 0.918 | 23.691 | +4.8% |
| `span-long-tail-memtable-c1` | CPU ms/q | 108.000 / 107.699 / 108.222 | 108.000 | 0.523 | 99.046 | +9.0% |
| `span-long-tail-memtable-c1` | bytes/q | 278,134,783 / 278,134,783 / 278,134,783 | 278,134,783 | 0 | — | — |
| `span-long-tail-memtable-c1` | rows/q | 500,223 / 500,223 / 500,223 | 500,223 | 0 | — | — |
| `span-long-tail-memtable-c1` | q/CPU-s | 9.3 / 9.3 / 9.2 | 9.3 | 0.0 | 10.1 | -8.3% |
| `span-long-tail-staged-c1` | p99 ms | 60.228 / 63.308 / 56.492 | 60.228 | 6.816 | 59.736 | +0.8% |
| `span-long-tail-staged-c1` | CPU ms/q | 159.889 / 161.798 / 158.352 | 159.889 | 3.446 | 163.977 | -2.5% |
| `span-long-tail-staged-c1` | bytes/q | 19,709,951 / 19,709,951 / 19,709,951 | 19,709,951 | 0 | 19,709,951 | +0.0% |
| `span-long-tail-staged-c1` | rows/q | 505 / 505 / 505 | 505 | 0 | — | — |
| `span-long-tail-staged-c1` | q/CPU-s | 6.3 / 6.2 / 6.3 | 6.3 | 0.1 | 6.1 | +2.6% |

Point steps against the earlier records, by explicit mapping only (`point-c1` → `point-memtable-c1`, `point-c64` → `point-memtable-c64`). Those records predate the mixed-source fixture controls, so they are absolute recovery targets, not a like-for-like comparison:

| Current step | Metric | 9a6e15750 | 471f69ff9 | 434aaf5c1 (`point-*`) | 12fc33f08 (`point-*`) |
|---|---|---|---|---|---|
| `point-memtable-c1` | q/CPU-s | 770.1 | 793.9 | 860.8 | 885.8 |
| `point-memtable-c1` | p99 ms | 1.608 | 1.542 | 1.620 | 1.514 |
| `point-memtable-c64` | p99 ms | 5.022 | 4.903 | 4.219 | 4.305 |
| `point-memtable-c64` | q/CPU-s | 560.9 | 573.1 | 578.0 | 584.0 |

Staged/memtable ratios, each within its own record:

| Pair | Ratio | 9a6e15750 | 471f69ff9 | Memtable side |
|---|---|---|---|---|
| shredded | p99 | 2.383 | 2.690 | slower: 36.635 → 38.011 ms |
| shredded | CPU/q | 1.544 | 1.392 | faster: 185.897 → 164.740 ms |
| long-tail | p99 | 2.426 | 2.521 | slower: 23.691 → 24.822 ms |
| long-tail | CPU/q | 1.480 | 1.656 | slower: 99.046 → 108.000 ms |

#### Parquet, against `parquet/20261029T134546Z-459617bae.json`

`full-scan-c1` reports p95, as the experiment declares. The warehouse path is unchanged: scanned bytes and rows on full-scan, shredded and long-tail are identical.

| Step | Metric | Repetitions | Median | Spread | Reference median | Δ |
|---|---|---|---|---|---|---|
| `point-c1` | p99 ms | 2.871 / 2.906 / 3.176 | 2.906 | 0.305 | 2.886 | +0.7% |
| `point-c1` | CPU ms/q | 2.215 / 2.232 / 2.229 | 2.229 | 0.017 | 2.160 | +3.2% |
| `point-c1` | bytes/q | 185,436 / 185,150 / 185,179 | 185,179 | 286 | 181,370 | +2.1% |
| `point-c1` | rows/q | 1 / 1 / 1 | 1 | 0 | 1 | +0.0% |
| `point-c1` | q/CPU-s | 451.4 / 448.0 / 448.6 | 448.6 | 3.4 | 462.9 | -3.1% |
| `point-c64` | p99 ms | 11.338 / 11.297 / 11.182 | 11.297 | 0.156 | 10.789 | +4.7% |
| `point-c64` | CPU ms/q | 3.604 / 3.626 / 3.605 | 3.605 | 0.022 | 3.501 | +3.0% |
| `point-c64` | bytes/q | 185,297 / 185,287 / 185,316 | 185,297 | 29 | 181,411 | +2.1% |
| `point-c64` | rows/q | 1 / 1 / 1 | 1 | 0 | 1 | +0.0% |
| `point-c64` | q/CPU-s | 277.5 / 275.7 / 277.4 | 277.4 | 1.7 | 285.6 | -2.9% |
| `small-aggregate-c1` | p99 ms | 5.738 / 6.033 / 5.722 | 5.738 | 0.311 | 5.784 | -0.8% |
| `small-aggregate-c1` | CPU ms/q | 5.016 / 5.412 / 5.021 | 5.021 | 0.395 | 5.076 | -1.1% |
| `small-aggregate-c1` | bytes/q | 503,765 / 503,759 / 503,651 | 503,759 | 114 | 496,285 | +1.5% |
| `small-aggregate-c1` | rows/q | 100,031 / 100,031 / 100,031 | 100,031 | 0 | 100,031 | +0.0% |
| `small-aggregate-c1` | q/CPU-s | 199.3 / 184.8 / 199.2 | 199.2 | 14.6 | 197.0 | +1.1% |
| `full-scan-c1` | p95 ms | 53.674 / 53.543 / 52.527 | 53.543 | 1.147 | 52.658 | +1.7% |
| `full-scan-c1` | CPU ms/q | 468.119 / 467.228 / 459.608 | 467.228 | 8.511 | 467.426 | -0.0% |
| `full-scan-c1` | bytes/q | 248,512,511 / 248,512,511 / 248,512,511 | 248,512,511 | 0 | 248,512,511 | +0.0% |
| `full-scan-c1` | rows/q | 10,002,431 / 10,002,431 / 10,002,431 | 10,002,431 | 0 | 10,002,431 | +0.0% |
| `full-scan-c1` | q/CPU-s | 2.1 / 2.1 / 2.2 | 2.1 | 0.0 | 2.1 | +0.0% |
| `span-trace-c1` | p99 ms | 4.469 / 4.604 / 4.456 | 4.469 | 0.147 | 4.370 | +2.2% |
| `span-trace-c1` | CPU ms/q | 3.755 / 3.927 / 3.853 | 3.853 | 0.172 | 3.795 | +1.5% |
| `span-trace-c1` | bytes/q | 688,024 / 688,335 / 689,583 | 688,335 | 1,559 | 688,315 | +0.0% |
| `span-trace-c1` | rows/q | 8 / 8 / 8 | 8 | 0 | 8 | +0.0% |
| `span-trace-c1` | q/CPU-s | 266.3 / 254.6 / 259.5 | 259.5 | 11.7 | 263.5 | -1.5% |
| `span-trace-c64` | p99 ms | 26.411 / 24.609 / 24.347 | 24.609 | 2.064 | 23.642 | +4.1% |
| `span-trace-c64` | CPU ms/q | 7.095 / 7.021 / 7.040 | 7.040 | 0.074 | 6.925 | +1.7% |
| `span-trace-c64` | bytes/q | 688,907 / 689,311 / 688,956 | 688,956 | 405 | 689,045 | -0.0% |
| `span-trace-c64` | rows/q | 8 / 8 / 8 | 8 | 0 | 8 | +0.0% |
| `span-trace-c64` | q/CPU-s | 140.9 / 142.4 / 142.0 | 142.0 | 1.5 | 144.4 | -1.6% |
| `span-shredded-c1` | p99 ms | 17.203 / 16.957 / 17.416 | 17.203 | 0.459 | 16.925 | +1.6% |
| `span-shredded-c1` | CPU ms/q | 74.788 / 74.247 / 76.116 | 74.788 | 1.869 | 75.518 | -1.0% |
| `span-shredded-c1` | bytes/q | 18,006,015 / 18,006,015 / 18,006,015 | 18,006,015 | 0 | 18,006,015 | +0.0% |
| `span-shredded-c1` | rows/q | 62,527 / 62,527 / 62,527 | 62,527 | 0 | 62,527 | +0.0% |
| `span-shredded-c1` | q/CPU-s | 13.4 / 13.5 / 13.1 | 13.4 | 0.3 | 13.2 | +1.0% |
| `span-long-tail-c1` | p99 ms | 39.059 / 36.700 / 36.176 | 36.700 | 2.884 | 37.487 | -2.1% |
| `span-long-tail-c1` | CPU ms/q | 196.549 / 201.958 / 195.139 | 196.549 | 6.819 | 194.931 | +0.8% |
| `span-long-tail-c1` | bytes/q | 29,442,047 / 29,442,047 / 29,442,047 | 29,442,047 | 0 | 29,442,047 | +0.0% |
| `span-long-tail-c1` | rows/q | 977 / 977 / 977 | 977 | 0 | 977 | +0.0% |
| `span-long-tail-c1` | q/CPU-s | 5.1 / 5.0 / 5.1 | 5.1 | 0.2 | 5.1 | -0.8% |

#### Acceptance by record

| Target | Result | Status |
|---|---|---|
| Full hot VALID within 300 s | VALID, 219.2 s | PASS |
| `span-shredded-staged-c1` improves p99 and CPU/q against 471f69ff9 | 98.566 → 90.571 ms; 258.772 → 254.310 ms/q | PASS |
| `span-shredded-memtable-c1` improves p99 and CPU/q against 471f69ff9 | CPU 185.897 → 164.740 ms/q; p99 36.635 → 38.011 ms (+3.8%) | **MISS** (p99) |
| Staged/memtable p99 and CPU ratios shrink, with memory not slower | shredded p99 ratio shrinks, but memtable p99 is slower; shredded CPU ratio grows 1.392 → 1.544; long-tail ratios shrink, but the memtable is slower on both | **MISS** |
| Point memtable c1 ≥ 861 q/CPU-s | 770.1 | **MISS** |
| Point memtable c64 server p99 ≤ 4.31 ms | 5.022 ms | **MISS** |
| Parquet: no latency, CPU or scanned-byte regression against 459617bae (no tolerance) | Higher on point c1 (p99 +0.7%, CPU +3.2%, bytes +2.1%), point c64 (p99 +4.7%, CPU +3.0%, bytes +2.1%), small-aggregate bytes (+1.5%), span-trace c1 (p99 +2.2%, CPU +1.5%), span-trace c64 (p99 +4.1%, CPU +1.7%), shredded p99 (+1.6%), long-tail CPU (+0.8%), full-scan p95 (+1.7%). Lower or equal on small-aggregate p99/CPU, full-scan CPU, shredded CPU, long-tail p99 | **MISS** |

The other hot memtable steps also cost more server CPU per query than in 471f69ff9: point c1 +3.1%, point c64 +2.2%, small-aggregate +19.9%, long-tail +9.0%. Their staged counterparts held or improved on CPU, except small-aggregate staged (+1.6%). Staged scanned bytes rose on point (+4.4%) and small-aggregate (+3.8%). These measurements do not establish a cause, and none is claimed. Every **MISS** above is unfinished performance acceptance; it does not change either VALID verdict.

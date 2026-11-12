---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-variant-file-reads
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, AC-001, AC-002, AC-003]
depends_on: [TASK-001]
---

## Outcome and Value

Complete the consolidated Variant query path for staged and published Parquet.
The same covered attribute query uses relevant leaves under each file's own
layout, with the same exact result as memory. A staged query must benefit from
leaf execution without losing the lease that keeps its scratch file readable.
Published reads retain their tenant-bound cache behavior.

This extends the requirement contract from TASK-001. Much of file-reader sharing
already exists: both source kinds use `hot_parquet_scan`. Preserve and prove that
shared behavior; change only missing requirement propagation or file adaptation.
TASK-003 decides whether the resulting staged execution delivers measured gains.

## What Changes and What Must Stay True

Own file-specific requirement consumption, per-file planning/adaptation and the
integrated correctness/lifecycle proof across all source tiers. Reuse the existing
DataFusion Parquet source, planner, storage factories and source owners. Each file
is planned by its data owner against its own physical schema. Remote footers stay
on that owner. Different files may shred different keys or store residual types.

Covered literal/nested/list expressions and supported comparison, `IN` and null
filters retain authority-defined leaf selection and exact filtering. Whole-root,
missing-path and unsupported-expression behavior remains complete and exact.
Truthful sizes, row counts and event-time bounds reach planning; missing metadata
stays a typed unknown. Statistics never answer COUNT in a way that skips tenant
footer proof, and unsafe statistics never exclude matching rows.

Staged files remain uncached scratch protected by their source-generation lease.
Published immutable files retain their existing cache eligibility and existence
checks. Publication cuts, retirement, cancellation and remote failures keep each
row once and release charges/leases at the existing boundary. Accepted completion
work counts once; refused/stale attempts contribute none. Unknown file IO stays
distinct from zero and decoded Arrow bytes.

No new reader, cache, file format, public query selector, Forge behavior or learning
policy is authorized. The material cost is proving file-layout variation and
lifecycle safety through the shared workflow, not replacing its orchestration.

## Approach

1. Revalidate TASK-001's source requirement contract through staged follower and
   published worker/local file resolution; identify any remaining lost expression.
2. Extend the existing per-file path only where needed for covered leaf execution,
   exact adaptation and conservative metadata under different source layouts.
3. Prove file-specific leaf selection and fallback, then close the real query
   matrix across memory/staging/publication and local/remote execution.
4. Exercise publication, retirement, refusal and cancellation with retained sources;
   preserve accepted-completion accounting and cache distinctions.

## Ordered Implementation Scenarios

### Scenario 1 — File sources consume the same validated requirements

**Behavior.** A leaf-only projection/filter and filter-only COUNT reach staged
and published scans through the existing per-file planner. Required leaves and
residuals are selected per file; different layouts never change logical results.
Root output remains complete, and nested/list access retains the authority's
coverage. Exact predicates remain exact when statistics cannot safely prune.

**RED.** Add the planned staged-versus-warehouse execution test
[V1](#verification-and-evidence) using the existing real-file fixtures. Assert
decoded leaf selection, logical output/order and filter results for both source
kinds, including mixed residual types. A lost requirement or sibling decode must
fail even if results agree. If the integrated TASK-001 path already passes a
case, retain it as verification-only proof; do not manufacture production changes.

**GREEN.** Repair only the missing adaptation or requirement consumption in the
existing reader path. Rerun V1 and the authority-focused leaf/null/statistics
regressions V2–V5.

**REFACTOR.** Remove duplicate file policy when demonstrated; preserve source-
specific factory/cache/lease ownership and keep the test facade test-only.

### Scenario 2 — SDK queries agree across source transitions

**Behavior.** The same canonical and learned logical documents are queried from
memory, staged files and published files, both locally and on role-separated
peers. Scalar projection, filter-only COUNT, missing/null, mixed types, nested/list,
whole-root and fallback cases preserve values/types/cardinality. A held query sees
its selected rows once through publication and retirement, and a new query sees
the valid successor cut. Unauthorized or mismatched tenant reads return no rows.

**RED.** Add the planned cross-source journey [V6](#verification-and-evidence)
through the real SDK and WyrdTestServer/peer cluster. Use held-source synchronization
already provided by the journey owners. Assert the matrix and source-local leaf
execution; fail on reconstruction of unrelated roots, lost/duplicate rows or a
tenant-proof bypass. Previously correct transition cases are regression proof.

**GREEN.** Close missing file and source-union consumer behavior until V6 and the
earlier focused tests pass. Rerun TASK-001's memory journey after shared-path edits.

**REFACTOR.** Share the existing logical query expectations where useful; retain
distinct source placement and lifecycle observations so one tier cannot mask another.

### Scenario 3 — Completion evidence reflects only accepted source work

**Behavior.** A validated completed source contributes once. A refused, stale or
cancelled-before-acceptance attempt contributes no accepted-source work. Unknown
measurements remain unknown; known zero remains zero. A remote footer or lease
failure terminates the attempt rather than returning cached or partial success.

**RED.** Add the planned completion-accounting test
[V7](#verification-and-evidence) at the existing execution/dispatch seam. Drive
accepted completion and refused/stale paths, asserting totals and unknown/zero
values. Any new branch introduced by this task must fail the new assertion before
repair; existing correct branches are verification-only proof. Pair it with the
real remote-failure/cancellation regressions V9/V10.

**GREEN.** Keep folding at the existing validated completion boundary; repair any
new propagation gap without counting raw attempts or planned source work.
Rerun V7 and the relevant lifecycle tests.

**REFACTOR.** Reuse the current metrics owner; delete duplicate folds rather than
introduce another accumulator or public telemetry contract.

## Acceptance Criteria

| Obligation | Proof and failure observation |
|---|---|
| REQ-001/002; remaining AC-001/002 | V1–V6 prove source-specific leaf execution and exact SQL results; lost expression, sibling decode, changed types or wrong counts fails. Together with TASK-001, all three source tiers are covered. |
| REQ-003; AC-002 | V1 and V8 retain shared reader policy, staged lease/uncached behavior and published cache/existence checks. Incorrect known metadata or unsafe pruning fails. |
| REQ-004; AC-003 | V6/V8–V10 retain tenant footer proof, pinned-cut exactness, lease retention and cancellation/failure termination; early deletion or authorization bypass fails. |
| REQ-005; AC-003 | V7 plus V9/V10 exclude refused/stale attempts and preserve one fold per validated completion and unknown/zero meaning. Mislabelled decoded bytes fails. |

## Implementation Reference

- `crates/vala/vala-bifrost-redux/src/oracle/exec.rs::hot_parquet_scan` constructs
  the same `ParquetSource` and session policy for `HotObjects::Warehouse` and
  `HotObjects::Staged`; the differences are reader factory/cache and retained
  lease. `OracleTableProvider` calls it for warehouse sources.
- `oracle/follower.rs::ScribeTailResolver::live_leaf` calls that same
  reader for staged files; `required_schema` currently carries the root closure.
  It obtains actual file size locally and uses unknown row/event-time statistics.
  The task closes expression consumption from TASK-001 at this existing seam.
- `oracle/exec.rs::TenantWarehouseReads` and `TenantParquetReaderFactory` own
  tenant-bound readers, cached ranges and footer tenant proof. Their existing
  foreign-footer, vanished-object and range-cache tests pin the safety boundary.
- `oracle/nested_pushdown.rs::FileReadPlan` is a test facade over the fork's
  `PerFileParquetReadPlanner`, not another production reader. Existing real SQL
  tests cover scalar leaves, list paths, nested residuals and conditional typed
  statistics. Reuse their production scan fixture to observe actual leaf decoding.
- `oracle/exec.rs::RemoteScanMetrics::record_footer` folds validated completed
  participant evidence, and `OracleQueryScanStats` preserves absent file IO for
  memory. Trace callers in live completion and dispatcher before changing them;
  test at that acceptance boundary, not by manually incrementing counters alone.
- `scribe/tail_rpc.rs::LiveTailBatches` carries staged runs and their lease;
  its retained-capture lifecycle tests and follower dropped-scan test already
  exercise ownership. Extend the existing release path, not a parallel lease.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs` contains real
  remote footer-refusal, live failure and source reconciliation journeys;
  `published.rs` covers Variant sessions. The oracle target and peer fixtures
  already own the needed local and remote query surfaces.

Likely changes are in these owners, TASK-001's private consumers where necessary,
their inline tests and oracle journey modules. No new dependency or SDK API is
needed. Keep manifests/codec revision bindings consistent if an allowed fork
repin proves necessary. Synchronize owning architecture prose if source execution
changes make its description inaccurate, preserving the approved invariants.

## Verification and Evidence

Run from the repository root, Cargo sequentially. V1/V6/V7 are **planned** tests
in confirmed existing targets; confirm actual selectors after adding them before
RED. Existing regressions are verification-only and need no manufactured failure.

**V1 — planned actual staged/warehouse leaf execution (unit with real local files).**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::nested_pushdown::tests::staged_and_warehouse_variant_requirements_select_the_same_leaves)'
```

**V2–V5 — existing leaf, list, residual and statistics regressions.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::nested_pushdown::tests::variant_key_reads_decode_only_their_leaves)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::nested_pushdown::tests::list_variant_key_reads_decode_only_their_leaf)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::nested_pushdown::tests::nested_variant_keys_filter_through_shredded_and_residual_leaves)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::nested_pushdown::tests::typed_variant_statistics_prune_only_with_all_null_residuals)'
```

**V6 — planned canonical/learned source-transition journey.**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::variant_leaf_queries_agree_across_source_transitions)'"
```

**V7 — planned validated completion/attempt accounting (unit).**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::variant_source_completion_counts_only_accepted_work)'
```

**V8 — existing footer, cache and lease safety regressions.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::vanished_warehouse_object_fails_as_stale)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::warehouse_reads_serve_cached_ranges_and_read_only_the_rest)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately)'
```

**V9/V10 — existing remote refusal and failure/cancellation journeys.**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::remote_staged_footer_refusal_fails_closed)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::live_query_terminal_failure_matrix)'"
```

Then run the owner families and affected journey lane:
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^oracle::(exec|follower|nested_pushdown)::/) or test(/^scribe::tail_rpc::/)'
mise run test:bifrost:journey:oracle
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
```

Rerun TASK-001's exact memory journey command when shared requirement handling
changes. If generated contracts change, regenerate from their owner and run
`mise run codegen:check`. Record leaf masks/executed work, metadata truthfulness,
cache/lease observations and completion accounting. Do not claim CPU/latency gains
from these tests. Run `git diff --check` at closeout. Integrated change review
owns the full Bifrost cross-language gate once; this task adds no language surface.

## Material Stop Conditions

- Correctness would require leader-side remote footers, another file planner,
  cache, resident representation or public execution API.
- Meeting the leaf contract requires changing persistent format, learning/Forge
  policy, exactness, tenant proof, cache eligibility, generation lifetime or
  cancellation semantics rather than preserving them.
- The inherited requirement contract cannot describe file behavior within its
  approved boundary; revise the responsible task instead of silently weakening it.

Private wiring, fixture support and no-op proof for already-correct shared
behavior remain implementation decisions. An allowed compatible fork repin
must preserve all engine-revision/codec integrity checks.

## Authority Links

- [Approved specification](../spec.md), revision 2;
  [memory/source requirements](TASK-001-memory-variant-execution.md).
- [Nested reads](../../../../architecture/bifrost-design.md#nested-field-reads),
  [source authority](../../../../architecture/bifrost-design.md#immutable-planning-and-source-authority),
  [resource/lifecycle invariants](../../../../architecture/bifrost-design.md#resource-and-failure-invariants).
- [Peer security](../../../../architecture/wyrd-security-posture.md#peer-identity-and-distributed-oracle).
- [Execution discipline](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [Rust](../../../../agent_docs/rust.md).

## Implementation Evidence

### Revalidated owners and gap

- Requirement contract: TASK-001's signed `RemoteScanPayload` reaches
  `ScribeTailResolver::live_leaf` as `ReadRequirements`; nothing is lost
  before the staged branch. Published reads are planned on the leader, whose
  session runs `FilterPushdown` then `ProjectionPushdown`, so the warehouse
  `ParquetSource` already carries the pushed `variant_get` outputs and filter
  (existing V2–V5 proof).
- Gap: the staged branch wrapped `hot_parquet_scan` in a `FilterExec` and
  `ProjectionExec`, but the follower ran only `FilterPushdown` after decode.
  The projection stayed above the scan, so the staged `ParquetSource`
  decoded every leaf of the signed closure, sibling shredded keys included.
  V1 RED: the staged read of `v ->> 'a'` decoded all 16 leaves of the
  fixture file, while the warehouse plan decoded 3 (`v.metadata`,
  `v.typed_value.a.{value,typed_value}`). The journey RED (fix withheld):
  the staged covered key scanned 917 bytes, the same as its whole root.

### Repair

- `oracle/follower.rs::staged_scan`: the staged leaf is built in one place
  (shared `hot_parquet_scan` with `HotObjects::Staged(lease)`, unchanged
  factory, no footer cache, lease extension per file). The requirements'
  operators are pushed into it in the Oracle session order (`FilterPushdown`,
  then `ProjectionPushdown`), so each staged run plans its own leaves against
  its own physical schema through the same per-file planner as published files.
- The post-decode `FilterPushdown` in `PhysicalPlanFollower::execute` is
  deleted. Scribe fragments ship only the placeholder (TASK-001), so the
  staged leaf's own pushdown was its only remaining effect; its comment was
  stale.
- No reader, cache, planner, format, public selector or accounting fold was
  added. The architecture prose ("each Oracle reader decodes only that path's
  leaves") was already the contract; the staged path now matches it.

Callers checked: `live_leaf` is the only staged-scan consumer; `execute` is
Scribe-only (`validate_assignment`), and the existing staged tests
(`scribe_staged_scan_prunes_non_matching_row_groups`,
`a_dropped_staged_scan_releases_its_lease_immediately`) push their own
filters and still pass.

### Selector corrections

- V7 lives in `oracle::live::tests`, beside `LiveFrameDecoder`, the validated
  completion boundary that calls `RemoteScanMetrics::record_footer`:
  `test(=oracle::live::tests::variant_source_completion_counts_only_accepted_work)`.
  It is verification-only: no accounting branch changed, and it passed first.
- V1's filter-only case reads `SELECT 1 AS one … WHERE …` on the warehouse
  side. The test's empty registered `MemTable` lets `DataFusion` answer
  `count(*)` from statistics before the hot scan is swapped in; the scan
  projection (zero columns) is identical.

### Diagnosis

- **V6 `Flush("staged rows did not settle within 60s")`.** Symptom: the
  publish issued while the query was held timed out. Evidence (traced run,
  `WYRD_LOG=info,vala_bifrost_redux=debug`): 33 distinct queries before the
  hang, i.e. every tier passed and the hang was the held-query publish.
  `ScribeImpl::flush_staged` publishes, then `retire_committed` waits for the
  held query's retained capture. Cause: the journey awaited the flush before
  releasing the paused query, so retirement could not finish; this is the
  production retention rule the journey must observe. Fix site: the journey
  runs the publish concurrently, waits until the table's live authorities are
  only `Published` while the query is still held, then releases and drains it,
  and lets the flush finish retirement.

### Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002; remaining AC-001/002 | `follower.rs::staged_scan` pushdown | V1 (staged = warehouse leaves, exact rows incl. residual `"str"`, list path, zero-column filter-only); V2–V5; V6 both topologies across memory/staged/published, covered reads reconstruct 0, learned staged and published covered key scans fewer bytes than its root | PASS |
| REQ-003; AC-002 | shared `hot_parquet_scan`, `HotObjects::Staged` factory and lease unchanged | V1; V8 (foreign/missing footer, vanished object, cached ranges, dropped staged lease) | PASS |
| REQ-004; AC-003 | no change to proof, cuts, leases or cancellation | V6 held query returns each row once through publication and retirement, successor cut once, unprivileged caller refused; V9; V10 | PASS |
| REQ-005; AC-003 | `LiveFrameDecoder::accept` unchanged | V7 (accepted once; foreign-fragment and stale footers refused; dropped attempt folds nothing; known zero `Some(0)`; memtable bytes `None`); V9; V10 | PASS |

Commands (repository root, sequential): V1, V2–V5, V7, V8 exact selectors;
V6 alone, with the fix withheld (RED), and with TASK-001's V3 journey;
owner families `oracle::(exec|follower|nested_pushdown|live|codec|memtable_source)`
and `scribe::(tail_rpc|memtable)` (110/110); `mise run test:bifrost:journey:oracle`
(65/65, includes V3, V6, V9, V10); `mise run fmt`; `mise run lints`;
`mise run check:deps`; `mise run check:tenant-isolation`; `git diff --check`.
No generated contract changed, so `codegen:check` was not required.

Non-goals stayed excluded: no new reader, cache, file format, public query
selector, Forge or learning change, and no CPU/latency claim (TASK-003).
Changed files: `oracle/follower.rs`, `oracle/nested_pushdown.rs` (tests),
`oracle/live.rs` (test), `wyrd-testing/tests/bifrost/oracle/distributed.rs`.
The pre-existing `changes/active/benchmark-scaling` deletions in the working
tree are not part of this task and were left unstaged.

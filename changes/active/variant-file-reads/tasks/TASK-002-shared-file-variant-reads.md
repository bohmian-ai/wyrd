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

---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-variant-file-reads
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-004, REQ-005, AC-001, AC-002, AC-003]
depends_on: []
---

## Outcome and Value

Make memory queries benefit from the learned Variant layout already retained by
Scribe. A query selecting or filtering one learned attribute must not reconstruct
unrelated attributes before evaluating that expression. For example, counting
rows whose `attributes ->> 'region'` is `eu-west` returns the same answer while
avoiding whole-root reconstruction. Establish the baseline execution defect and
prove the repair through executed work; TASK-003 owns measured performance.

## What Changes and What Must Stay True

Own the query-requirement path from Oracle planning through local and remote
source resolution to memory execution, including the necessary private assignment,
codec, capture and consumer closure. Requirements must carry ordered outputs,
filter inputs and exact-versus-pruning meaning to the data owner before IO.
Source-local adaptation uses the existing DataFusion/Arrow capability.

Logical results retain their schema, order, types, null semantics and cardinality.
Physical layouts remain private. Whole-root output retains residual fields;
unshredded and unsupported uses retain exact fallback. Filter-only inputs do not
leak into output. A zero-column output still returns its actual row count.

Preserve tenant/table/version proof, signed assignment and schema validation,
permissions, deadlines, cancellation and captured-generation ownership. Source
expression requirements must be bound by the existing authenticated assignment
and plan integrity checks; no executable expression is trusted from unvalidated
request bytes. Keep runtime expressions out of IO-free contracts. No new public
API, planner, resident copy, learning policy, persisted format or ingest path is
part of this task. Temporary query allocations remain governed and releasable.

The cost is source-layout adaptation and its correctness proof. Do not duplicate
the complete source workflow just to reuse a leaf evaluator. Preserve existing
staged/published behavior while changing the common requirement path; TASK-002
closes file-specific behavior and lifecycle proof.

## Approach

1. Trace a learned generation's query from projection/filter planning through
   capture and memory source execution. Record the first point that loses leaf
   requirements or reconstructs an unnecessary root, with a focused reproduction.
2. Extend the existing requirement/assignment path and its consumers so the
   owner has sufficient validated information to evaluate covered expressions.
3. Execute covered memory paths using existing layout adaptation and Variant
   semantics; preserve full logical fallback and trustworthy statistics.
4. Prove local and remote result equivalence, source authority and release with
   focused owner tests and real query journeys. Leave L1 measurement to TASK-003.

## Ordered Implementation Scenarios

### Scenario 1 — Requirements survive source resolution

**Behavior.** An ordered scalar projection and its filter reach local and remote
memory owners before reconstruction. Repeated/reordered outputs, filter-only
inputs and COUNT keep the intended schema and exact-versus-pruning distinction.
An assignment or schema mismatch is refused before source IO.

**RED.** Add the planned requirement-propagation test in the existing follower
test module, using the current capturing tail-source precedent. Assert the
received expression requirements, resulting schema/order and pre-IO refusal.
Run [V1](#verification-and-evidence); the missing expression information must
cause the expected assertion failure, rather than a setup or compile failure.

**GREEN.** Extend the existing source contract and all necessary encoders,
validators and local/remote consumers to preserve these requirements. Rerun V1
and the existing projection-closure regression V4.

**REFACTOR.** Remove redundant derivation of requirements while preserving the
same validated meaning, output order and negative assertions.

### Scenario 2 — Covered memory paths avoid unrelated reconstruction

**Behavior.** A frozen learned generation serves literal scalar, nested and list
paths, including supported comparisons, `IN` and null tests, from required leaves,
metadata and path residuals. The following variations have distinct observations:

| Use | Expected observation |
|---|---|
| Scalar output or filter-only COUNT | Exact result with no unrelated root reconstruction |
| Missing/null or mixed types | Exact SQL values/types; unsafe statistics do not discard rows |
| Whole root or unsupported/unshredded path | Complete logical value through fallback |
| Empty output projection | Original surviving row count with zero output columns |
| Canonical and learned generations together | Compatible logical output and each row once |

**RED.** Add the planned memory-execution test at [V2](#verification-and-evidence).
Execute the real capture/memory path and assert results plus reconstruction work
at the existing owner boundary. A root reconstruction for the leaf-only case must
fail even when its SQL answer is correct. Confirm baseline behavior before repair.

**GREEN.** Make the smallest source-local change that avoids that work while
passing every variation. Rerun V1/V2 and preserve conservative filtering.

**REFACTOR.** Reuse the existing physical expression adapter and logical result
conversion; remove duplicate evaluation only where exactness remains proven.

### Scenario 3 — The real query path retains semantics and authority

**Behavior.** A claim teaches a layout; a later generation adopts it. Real SDK
queries over that memory return the same values/types/cardinality as canonical
data, both in-process and on role-separated peers. Include covered projection,
COUNT, residual fallback and whole-root output. An unauthorized caller receives
no data, and cancellation releases query-held generation ownership.

**RED.** Add the planned oracle journey [V3](#verification-and-evidence), extending
the existing SDK/server and peer-cluster owners. Use a deterministic execution
observation for unwanted reconstruction alongside returned results; correct
storage shredding alone must not make the baseline pass. Establish ordinary
correctness and refusal assertions as regressions if they already pass.

**GREEN.** Complete missing consumer and lifecycle wiring until the journey and
earlier focused tests pass without exposing physical layouts or widening authority.

**REFACTOR.** Reuse existing cluster and learning setup; keep behavioral assertions
in the journey and execution-only assertions at their lower-tier owner.

## Acceptance Criteria

| Obligation | Proof and failure observation |
|---|---|
| REQ-001; memory portion of AC-001 | V1/V3 preserve ordered requirements and logical schema; missing expressions, changed output or pre-validation IO fails. |
| REQ-002; memory portion of AC-002 | V2/V3 prove leaf execution and exact edge/fallback results; reconstructing an unrelated root fails even with a correct answer. |
| REQ-004; memory portion of AC-003 | V1/V3 and V5 retain tenant proof, generation ownership and cancellation/refusal behavior; leaked ownership or unauthorized data fails. |
| REQ-005; memory portion of AC-003 | V6 preserves unknown-versus-zero scan measurements; memory Arrow allocation must not become compressed file IO. File/attempt completion accounting closes in TASK-002. |

## Implementation Reference

Paths identify existing owners to extend, not an allowlist or prescribed layout.

- `vala-bifrost-redux/src/oracle/exec.rs`: `OracleTableProvider::scan` and
  `OracleScanProjection::try_new` build the shared ordered root-column closure;
  `projected_memory_source` is a memory consumer. Inspected
  `projection_closure_order_is_stable` and
  `projected_leaf_union_outputs_the_requested_closure` pin output semantics.
  The gap is scalar-expression requirements beyond root-column selection.
- `oracle/live.rs::LiveScribeExec` forwards filters into fragment execution;
  `oracle/codec.rs::OraclePhysicalExtensionCodec` and
  `oracle/follower.rs::ScribeTailResolver` validate and resolve the
  physical source. The resolver's capturing-tail tests inspect required fields
  and pruning. Extend this whole path, preserving assignment/body integrity.
- `scribe/tail_rpc.rs::FetchLiveTailRequest` currently carries ordered fields
  and pruning; `FetchLiveTailService::open_capture` reads the retained source.
  `scribe/memtable.rs::ReadableSource::project` adapts a physical predicate but
  then calls `schema::logical_column` for each projected root. This is concrete
  reconstruction evidence to reproduce, not a claim that it explains all latency.
- `oracle/memtable_source.rs::MemtableSource` wraps the shared captured buffers,
  pruning and static exact filtering. Its projection/filter rewrites retain
  accounting owners; dynamic/volatile predicates remain with the upper operator.
  Preserve this workflow rather than introduce another memory representation.
- `wyrd-spec/src/vala/api.rs::FollowerScanAssignment` is the existing IO-free
  assignment contract; `ExecuteFragmentRequest` carries plan bytes. Any required
  private payload change reaches its validators/codecs/consumers without embedding
  runtime execution types or adding a public execution API.
- `wyrd-testing/tests/bifrost/scribe/telemetry.rs` already proves learned layouts
  across memory/staging/publication/restart. `tests/bifrost/oracle/distributed.rs`
  provides real peer queries and source lifetime journeys; `published.rs` provides
  Variant SQL/session and privilege precedents. Extend those existing owners.

## Verification and Evidence

All commands run from the repository root. Run Cargo commands sequentially.
V1–V3 are **planned** tests in confirmed existing packages/modules/targets;
after adding each test, confirm its actual selector before RED. A private module
split may update a selector with equivalent proof recorded. No benchmark runs here.

**V1 — planned requirement propagation (unit, no Postgres).**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::learned_variant_requirements_reach_memory)'
```

**V2 — planned executed memory work and edge matrix (unit, no Postgres).**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::covered_variant_paths_avoid_unrelated_reconstruction)'
```

**V3 — planned SDK/server memory journey (Postgres and real peer servers).**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::learned_variant_memory_queries_preserve_source_contract)'"
```

**V4 — existing projection-closure regression (unit).**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::projection_closure_order_is_stable)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::projected_leaf_union_outputs_the_requested_closure)'
```

**V5 — existing learned-layout lifecycle journey.**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::variant_layouts_survive_memory_stage_publication_and_restart)'"
```

**V6 — existing measurement absence/aggregation regression (unit).**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::scan_stats_preserve_non_file_absence_and_scan_aggregation)'
```

Then run the affected owner families and static checks:
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::(memtable|tail_rpc)::/) or test(/^oracle::(exec|live|codec|follower|memtable_source)::/)'
mise run test:bifrost:journey:oracle
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
```

Correctness journeys use the existing production query surface. No new language
surface is introduced; integrated change review runs the existing cross-language
Bifrost lanes once. If this task changes a generated contract source, regenerate
through its owner and run `mise run codegen:check`; never hand-edit generated files.
Record baseline cause, RED/GREEN results, executed reconstruction observations,
affected callers and local/remote equivalence. Run `git diff --check` at closeout.

## Material Stop Conditions

- The repair requires a public API, persisted layout, learning policy, new owner,
  resident representation or dependency family outside the approved boundaries.
- Correct leaf execution requires changing tenant authority, assignment integrity,
  exactness, cancellation or durability semantics.
- Available DataFusion capabilities cannot support the approved behavior. A
  compatible fork repin is allowed by the spec, but must bind the new revision
  consistently across manifests, lockfile, codec fingerprints and peer tests;
  an incompatible contract or new engine requires specification authority.

Ordinary private interfaces, test support, runtime-free private payload wiring,
and bounded command corrections remain implementation-owned. Discovery of the
actual cause is required work, not a reason to stop without investigation.

## Authority Links

- [Approved specification](../spec.md), revision 2.
- [Bifrost Variant learning](../../../../architecture/bifrost-design.md#variant-shredding),
  [query semantics](../../../../architecture/bifrost-design.md#variant-sql),
  [nested reads](../../../../architecture/bifrost-design.md#nested-field-reads),
  [source authority](../../../../architecture/bifrost-design.md#immutable-planning-and-source-authority).
- [Peer security](../../../../architecture/wyrd-security-posture.md#peer-identity-and-distributed-oracle).
- [Execution discipline](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [Rust](../../../../agent_docs/rust.md).

## Implementation Evidence

### Baseline cause

Traced on main `5234961d5` with a planning probe over a V3 table holding an
`attributes` Variant plus one live route (scratch test, removed):

- `SELECT attributes ->> 'region' AS r` planned
  `ProjectionExec(variant_get_text) → UnionExec → CooperativeExec → LiveScribeExec`.
  `UnionExec::try_swapping_with_projection` pushes only narrowing projections,
  so the path stayed above the union and the live leaf signed the root
  `attributes` alone.
- `COUNT(*) WHERE attributes ->> 'region' = 'eu-west'` planned
  `FilterExec(projection=[]) → LiveScribeExec`: the leaf reported every filter
  unsupported, so the leader kept a `FilterExec` that needs `attributes`.
- The fragment therefore shipped `FilterExec → placeholder(closure)`, and the
  Scribe's `ReadableSource::project` ran `schema::logical_column` (a full
  unshred of typed leaves plus residual) for every projected root of every
  kept row before any expression ran. V2 RED reproduced it: the scalar case
  returned correct values with `reconstructions = 1`, expected `0`.

### Repair

- `scribe/memtable.rs`: `ReadRequirements { filter, outputs }` reach
  `LiveCapture::read`/`ReadableSource::project`. Per generation, the filter and
  each computed output adapt once to the physical layout through
  `DefaultPhysicalExprAdapterFactory`; the filter applies exactly, outputs
  evaluate on typed leaves, a bare column output reconstructs only that root,
  and any expression that does not adapt, evaluate, or type-match falls back
  to the lazily reconstructed logical batch. Statistics are projected and made
  inexact when rows drop; empty batches are skipped; zero outputs keep the row
  count. The `None` path is unchanged.
- `oracle/codec.rs`: `RemoteScanPayload` carries the exact filter and ordered
  outputs (`PhysicalExprNode`s plus names) inside the fingerprinted
  placeholder; codec version 2 so a mixed build refuses rather than misreads.
- `oracle/live.rs`: `LiveScribeExec` keeps the closure schema separately from
  its output schema, reports static non-volatile `Pre` filters exact (dynamic
  and volatile stay with their producer), unprojects filters over absorbed
  outputs, absorbs projections (`try_swapping_with_projection`), and
  `LiveUnionBoundary` moves a projection left above a live-holding union onto
  each input (live side through `ProjectionPushdown`, siblings keep it directly
  above, as before). Fragments ship only the placeholder.
- `oracle/follower.rs`: preflight records each placeholder payload (the
  `FilterExec`-above-placeholder derivation is removed); `signed_requirements`
  parses them only against the closure derived from the authenticated schema,
  refusing non-boolean filters, off-closure or misaligned outputs before tail
  IO; the resolver passes requirements and pruning to the tail, the memtable
  leaf exposes the output schema, staged runs get `FilterExec`/`ProjectionExec`
  that pushdown moves into the Parquet scan, and `decode` checks the leaf
  against the same derived schema.

Callers checked: every `LiveCapture::read`, `FetchLiveTailRequest`,
`FollowerSourceResolver::resolve`, `RemoteSourcePlaceholderExec::new`
(including `wyrd-testing` peer-security fixture, unchanged: no requirements
keeps the closure) and `RemoteScanPayload` construction.

### RED / GREEN

| Scenario | RED (baseline or requirements withheld) | GREEN |
|---|---|---|
| 1 — V1 | `requirements: None` at the tail: panicked "requirements reach the live read" | ordered outputs `r, v, r2` and a zero-column COUNT reach memory; three refused payloads never open the tail |
| 2 — V2 | scalar path correct with `reconstructions = 1`, expected `0` | covered paths reconstruct 0; whole-root and fallback reconstruct and keep residuals |
| 3 — V3 | baseline fragment needed the whole root (see Baseline cause), so covered queries reconstruct | covered scalar, repeated output, COUNT, `coalesce … IS NOT NULL` and `to_json(v)` reconstruct 0; bare `SELECT v` reconstructs only when learned |

Local/remote equivalence: V3 runs the same proof over `one_mixed()` (in process)
and `role_separated()` (leader and Scribe peers); both pass with identical rows.

### Diagnoses

- **V3 `to_json(v)` reconstructed 0.** Symptom: the journey asserted a
  whole-root reconstruction for `to_json(v)`. Evidence: values matched exactly
  and the test-support counter was live. Cause: `to_json(v)` is a computed
  output that adapts to and evaluates on the shredded layout; it is not a bare
  root. Fix site: the journey's expectation (0 for `to_json`), plus a bare
  `SELECT v` that must reconstruct when learned.
- **V3 cancellation "the Scribe producer never paused".** Symptom: the paused
  learned read never paused. Evidence: the producer pauses before its second
  batch, and learned memory held one batch. Cause: the fixture wrote a single
  cohort. Fix site: the journey writes an extra cohort before the paused query.
- **`peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal`.**
  Symptom: `live_subquery.rs:159`, "the H2 fragment lacks the static bound
  1793977200000000". Evidence (`WYRD_LOG=info,vala_bifrost_redux=debug`): the
  rows, the two dispatched fragments and one footer row each all matched; only
  the plan text check failed. Cause: `LiveScribeExec::fragment` now signs the
  exact filter into `RemoteScanPayload.filter`, inside the extension's opaque
  bytes, so `Debug` of the `PhysicalPlanNode` shows bytes, not the literals. An
  independent read-only diagnosis confirmed this: the test's observation is
  stale and production is correct. No other caller decodes fragment plan text.
  Fix site: the journey decodes the placeholder through the now-`pub`
  `OraclePhysicalExtensionCodec::preflight_extension` and checks both literals
  in the signed filter.

  Independent diagnostician report (read-only Explore agent; given only the
  failing command, the trace log and the diff against `5234961d5`):

  > **Verdict:** the test's observation is stale; production behavior is
  > correct.
  >
  > **Cause:** commit `7692bd507` changed `LiveScribeExec::fragment`
  > (`oracle/live.rs`). It no longer wraps the placeholder in a `FilterExec`;
  > it passes the pushed predicate to
  > `RemoteSourcePlaceholderExec::with_requirements`.
  > `OraclePhysicalExtensionCodec::try_encode` (`oracle/codec.rs`) serializes
  > that predicate as `RemoteScanPayload.filter`, inside the
  > `OracleExtensionEnvelope` payload. The fragment root is therefore
  > `PhysicalPlanType::Extension`, and its `node` field is opaque bytes. The
  > test (`live_subquery.rs:203-210`) formats
  > `PhysicalPlanNode::decode(plan)` with `{:?}` and searches for the decimal
  > micros strings. Debug prints the nested payload as a byte list, so neither
  > `1793977200000000` nor the scalar literal can appear as text. Every earlier
  > check passed: the returned rows, 2 dispatched fragments and 2 footer rows.
  > The in-crate unit tests in `live.rs` were already updated for this; they
  > decode the extension and inspect
  > `preflight_extension(&extension.node).filter`.
  >
  > **Fix site:** the test's plan check (`live_subquery.rs:203-210`). Match the
  > `Extension` root, decode its payload, and assert both literals in the
  > Debug output of the payload's `filter`, as the scalar-subquery unit test in
  > `live.rs` (~1578) does. `preflight_extension` is `pub(crate)`
  > (`codec.rs:366`); make it `pub` or add a public test-only decode helper.
  > `RemoteScanPayload` is already `pub`.
  >
  > **Other affected callers:** none. `scribe_fragment_output` /
  > `last_fragment_plan` (`peer_cluster.rs:912`, `wyrd-server/src/state.rs:959`)
  > have no other consumers, and no other wyrd-testing test decodes fragment
  > plan text. The follower's `PhysicalPlanType::Filter` branch
  > (`follower.rs:1157`) no longer receives that shape from the leader. That
  > may be dead code, but it is not a failure.

  Disposition: applied the `pub` option, the smallest change that reuses the
  existing decoder rather than adding a helper. Checked the `follower.rs`
  branch: it is the generic preflight walker over every supported plan node,
  not a leader-shape special case, so it stays.

### Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001; memory AC-001 | `oracle/live.rs` absorb/union boundary; `oracle/codec.rs` payload; `oracle/follower.rs::signed_requirements` | V1, V3 (both topologies), V4 (both tests) | PASS |
| REQ-002; memory AC-002 | `scribe/memtable.rs` `ReadRequirements`/`AdaptedRequirements`, `logical_column` | V2, V3 reconstruction counts | PASS |
| REQ-004; memory AC-003 | signed placeholder payload, preflight refusal before tail IO, unchanged leases | V1 refusals, V3 unprivileged refusal and cancellation, V5 | PASS |
| REQ-005; memory AC-003 | scan stats untouched; memory Arrow bytes not reported as file IO | V6 | PASS |

Commands (repository root, sequential): V1, V2, V4 and V6 exact selectors;
V3 and V5 through `with-test-postgres.sh`; the owner families (99/99);
`mise run test:bifrost:journey:oracle` (63/63); `mise run fmt`; `mise run lints`;
`mise run check:deps`; `mise run check:tenant-isolation`; `git diff --check`.
The shards tests that need `WYRD_TEST_DATABASE_ADMIN_URL` are outside the owner
selector and were not run in the plain lib lane.

Non-goals stayed excluded: no public query API, persisted format, learning
policy, planner, resident copy or ingest change. Staged-file lifecycle and
benchmarks are left to TASK-002/003. The only crate-surface change is
`preflight_extension` becoming `pub` for the cross-crate journey. The codec
version is now 2, so mixed-version peers refuse rather than misread. The
pre-existing `changes/active/benchmark-scaling/*` deletions are untouched.

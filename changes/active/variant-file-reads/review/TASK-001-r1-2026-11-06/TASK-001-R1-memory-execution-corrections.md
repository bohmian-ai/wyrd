---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-variant-file-reads
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5]
---

# Preserve governed and exact live expression execution

## Authority and subject

- [Approved specification](../../spec.md), revision 2.
- [Original task and evidence](../../tasks/TASK-001-memory-variant-execution.md).
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Reviewed candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- [Verdict](verdict.md) and [independently validated ledger](findings-validation.md).

Implement only the five validated corrections below under `wyrd-implement`.
This is a correction packet within the approved behavior, not a new design
plan. Keep the original task, this review and candidate identity as the later
review's cumulative acceptance context. Git commits/pushes require caller authority.

## Intended outcome

Keep the learned-leaf benefit while making its execution obey the existing query
budget, preserve successful scalar-subquery outputs and refuse all malformed
closure references before opening a source. Keep private payload operations
internal and complete mandatory documentation in this same correction.

Maintainer clarification after review: all affected bytes and memory ownership
must be tracked and attributed through the core Bifrost-owned budget and tracker.
There is one authoritative accounting source per process; query ceilings and
role/tenant/table attribution are views of that ownership, not separate trackers.
This clarifies the existing resource invariant, not a new review finding.

## Diagnoses and selected corrections

### FIND-TASK-001-1 — Govern newly retained query allocations

The new `ReadableSource::project` eagerly invokes `AdaptedRequirements::evaluate`
for every captured batch and retains all resulting arrays before returning.
Computed output, partial filtering and logical fallback can allocate new buffers.
`LiveCapture::read` returns only original `RetainedArrowCharge` handles; those
charge the immutable generation, not newly computed query results. The session's
pool does not automatically intercept Arrow allocations. An ordinary arithmetic
projection over canonical memory now retains outputs for the whole capture,
whereas the baseline evaluated the expression on pull. Concurrent queries can
therefore exceed the admitted budget without their pool recording the growth.

Evidence: `scribe/memtable.rs:1990–2023,2293–2310,2573–2623`, capture request/result
in `scribe/tail_rpc.rs`, `oracle/follower.rs::ScribeTailResolver::live_leaf`,
`scribe/memory.rs::RetainedArrowCharge` and
`resources.rs::OracleQueryMemoryReservation`. Existing reconstruction-count and
post-production cancellation tests do not exercise pool refusal during construction.
This violates the explicit governed/releasable temporary-allocation constraint.

Reuse the admitted follower session's existing query pool/reservation mechanism,
backed by `BifrostRuntimeResources` and its existing `GovernedMemoryRoot` and
`BifrostResourceGovernor`, at the shared memory-read owner. These are the existing
core accounting path, not independent budgets or ledgers. Thread query-allocation ownership through the
existing private request/read/result/source/stream path, separately from original
generation charges. Account incremental retained allocations as batches are
evaluated; refuse growth beyond the existing grant without returning a partial
provider. Bound the current-batch evaluation/fallback workspace within that same
existing execution budget. Release reservations on failure, drop and cancellation,
and keep ownership through source cloning/projection rewrites/open streams.

Every affected allocation must appear in the core Bifrost accounting and existing
holder/query/tenant/table attribution. Reuse existing reservation handles so each
allocation has one accounting owner; query ceilings and diagnostic breakdowns must
derive from that owner rather than maintain a parallel byte total. Holding a query
reservation must not charge the root again for bytes already charged through its
pool, and sharing a generation buffer must not charge it once per query. Verify
the actual follower pool is connected to the core root; a standalone bounded
query pool or a second tracker does not satisfy this correction.

This is a moderate private cross-owner correction because the pool/lifetime owner
is currently absent from the path. Counting bytes only after full materialization,
charging only leader delivery, adding a metric or cloning original source charges
cannot close it. Do not create another resource root, double-charge shared source
buffers, reinterpret generation accounting or start an allocator-instrumentation
project. Preserve the existing resource exhaustion/error semantics.

### FIND-TASK-001-2 — Snapshot absorbed outputs per execution

`LiveScribeExec::resolved` snapshots its predicate only and returns early when
there is no predicate. The new absorbed output expressions are encoded unchanged.
For supported `SELECT id, (SELECT max(id) FROM table) AS m FROM table`, DataFusion
populates scalar runtime results before executing the main plan, but serialization
stores only the runtime scalar index/type, not that result. Follower output parsing
has no surrounding scalar coordinator and refuses the expression. Baseline leader
projection succeeds. Both local and remote paths use this payload, so repeated
query execution cannot recover without correction.

Evidence: `oracle/live.rs:459–466,489–501,585–603`, `follower.rs::signed_requirements`,
and pinned DataFusion scalar physical planning/encoding/decoding cited in the
validation ledger. Successful predicate and two-row scalar-refusal tests do not
prove a successful scalar output. This is a supported SQL regression, not new
scalar-query scope.

At the existing execution-time snapshot owner, apply recursive
`snapshot_physical_expr` to every absorbed output as well as the predicate,
including the no-predicate path. Preserve aliases/order/schema, route dealing,
pruning and the original runtime-bound plan for later executions. Pending output
values must refuse before dispatch. Reusing the predicate snapshot mechanism is
the smallest correction; do not create a follower scalar context, new codec,
coordinator or broad unsupported-expression framework, or disable the covered
Variant projection benefit.

### FIND-TASK-001-3 — Validate the complete closure before source access

`signed_requirements` treats filter `data_type` errors as false through
`is_ok_and`; it also does not traverse nested column references. A bare out-of-range
filter with empty outputs can therefore reach `tail.open`. Merely propagating the
type error is insufficient: a null-test expression returns Boolean even when its
child references an invalid ordinal, and the same malformed output can project a
Boolean schema. An empty capture can accept it without ever evaluating it.

Evidence: `oracle/follower.rs:143–183,757,918`, pinned Column protobuf decoding and
IsNull expression type behavior, and V1's existing malformed cases. This violates
the explicit pre-IO refusal obligation; no public attacker or tenant escape is
claimed. Current tests cover a bare invalid output, not these nested/filter paths.

Keep the correction at `signed_requirements`, the existing shared validation
owner before `tail.open` and reused by decode. Use installed DataFusion column
traversal to validate all referenced ordinals in every parsed filter and output
against the authenticated closure. Propagate filter type errors and require a
Boolean filter, preserving output derivation, expression order, redacted errors
and existing authority/fingerprint checks. This is a small local validation fix;
no new parser/planner or repeated guards in downstream consumers are justified.

### FIND-TASK-001-4 — Narrow only the unnecessary new public operations

The baseline resolver seam was already public and carried executable expressions.
It does not need to be privatized or redesigned. The unnecessary additions are
the ordinary public `preflight_extension`, requirement builder, writable private
payload contents and output type. Production users of those operations are
internal; the only external parser consumer is the scalar-predicate journey.
Task evidence explicitly says parser visibility was widened to inspect the opaque
payload. Existing `test-support` observation exports provide the intended boundary.

Evidence: `oracle/codec.rs:107–130,177,366`, its public module and actual callers,
and `wyrd-testing/tests/bifrost/oracle/peer_network/live_subquery.rs:212`. This is
the narrowed explicit visibility/scope violation from validation, not a security
bypass or a demand to alter the existing public follower facade.

Restore crate-private parser/builder/payload contents/output type. Keep the nominal
public payload transport type where the existing resolver signature needs it.
Expose only the filter observation needed by the cross-crate test under existing
`test-support`, reusing the production decoder and existing protobuf filter type.
Preserve both literal assertions. This small visibility/test-support correction
needs no new feature, public production API, decoder, facade rewrite or shim.

### FIND-TASK-001-5 — Complete changed-item documentation

`logical_column` indexes aligned projection/physical/source-schema data without
documenting those panic invariants. `AdaptedRequirements::evaluate` indexes restored
columns and uses `expect("restored just above")` without panic documentation.
The new `prove_variant_memory_answers` assumes nonempty fixtures and rendered row
shape without documenting its indexing invariants. `live_leaf` still promises the
closure schema/exact append bounds even though outputs and filtered/projected
statistics now determine its leaves.

Evidence: `scribe/memtable.rs:2359,2537`,
`wyrd-testing/tests/bifrost/oracle/distributed.rs:1252`, and
`oracle/follower.rs:468`. Root instructions and Rust guidance make these mandatory
changed-item documentation obligations. Document the actual invariants and
`# Panics`, or eliminate an invariant panic through an equally small expression;
describe actual output schema/statistics in `live_leaf`. Include documentation
for items changed by the other corrections. No runtime guards, new helpers or
tests solely for docs, and no separate style-only remediation round.

## Preserved behavior and scope

Preserve learned leaf execution, exact fallback, residuals, SQL null/type/order/
cardinality semantics, filter-only/zero-column output, original source charges,
tenant/table/version proof, schema/fence/integrity checks, generation/staged leases,
deadlines, cancellation, published cache behavior and private codec refusal.
Preserve the independently diagnosed scalar-predicate assertion correction.

No learning/sampling/WAL/ingest/publication/Forge policy, new persistent format,
dependency family, planner, resident representation, public query API, source/layout
selector or resource root is authorized. L1 comparison/performance remains TASK-003;
full file-specific acceptance remains TASK-002. Do not broaden this correction
into either task. Private runtime wiring and minimal adjacent test support remain
implementation-owned. Escalate an actual required material boundary change rather
than weaken approved behavior.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-TASK-001-1 | The existing core Bifrost tracker records each additional allocation exactly once with its existing attribution, and core-backed query limits refuse aggregate computed outputs beyond grant without partial provider/reservation residue. Fitting filter/computed/fallback results update core total/query/holder snapshots and release their ownership on errors/drop/cancellation; original source charges remain unchanged through retirement and final stream release. No independent query tracker or budget is introduced. |
| FIND-TASK-001-2 | No-WHERE and nested computed scalar outputs carry current literals and return correct values/types/aliases; resolving the same original plan with two scalar results produces each new value; pending output refuses before dispatch. Existing predicates/two-row refusal remain correct. |
| FIND-TASK-001-3 | Bare invalid filter/empty output, nested invalid null-test filter and nested invalid output all refuse before any tail-open call, including an empty source. Valid reordered/repeated/COUNT cases still pass. |
| FIND-TASK-001-4 | Ordinary builds retain necessary existing resolver plumbing but do not expose the newly unneeded payload operations/contents. Existing test-support lets the exact journey inspect the real decoded filter and both literals. |
| FIND-TASK-001-5 | Cited changed items and new remediation items document real intent/invariants/errors/panics; leaf docs match output schema and filtered/projected statistics. Source review closes the mandatory gate. |

Use TDD for new executable branches: add the focused assertion, confirm RED for
the diagnosed reason, implement the smallest correction, rerun earlier affected
cases, then refactor with proofs green. Documentation/static visibility work needs
no manufactured behavioral failure. The following planned tests use confirmed
existing packages/modules/targets; verify their actual selectors after addition
before RED. No new harness or integration binary is required.

**P1 — planned query-memory accounting/refusal/lifetime owner test.**
Use the production-equivalent core Bifrost resources and inspect its existing
resource snapshots alongside query refusal/release. Prove core/attributed growth
and release, no double charge for shared source buffers or nested query ownership,
and no residue after failed growth. An isolated test-only memory counter does not
prove the required single-source accounting path.
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::computed_live_outputs_respect_query_memory_budget)'
```

**P2 — planned scalar output snapshot/re-execution/pending owner test.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::live::tests::absorbed_scalar_outputs_snapshot_per_execution)'
```

**P3 — extend existing recording-tail pre-IO validation proof.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::learned_variant_requirements_reach_memory)'
```

**P4 — planned successful live scalar-output SDK journey.** Exercise the existing
mixed and role-separated query owners without a full journey-family run.
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::scalar_subquery_outputs_survive_live_projection_absorption)'"
```

**P5 — existing exact scalar-predicate journey after test-support visibility repair.**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal)'"
```

**P6 — preserve learned-memory benefit, exact rows and cancellation through SDK.**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::learned_variant_memory_queries_preserve_source_contract)'"
```

Then the narrow affected owner families and required static checks, sequentially:
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::(memtable|tail_rpc)::/) or test(/^oracle::(live|codec|follower|memtable_source)::/)'
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
```

If resource-owner production code is changed, also run its existing exact focused
reservation tests after confirming their selectors from that owner; thread
existing capabilities instead of changing their semantics when possible. Verify
normal/test-support visibility with the existing compiler/lint configurations and
source/caller inspection. Do not run full language/journey suites or `gate` here;
integrated change review owns broad qualification once. No benchmark is required.
Finish with `git diff --check` and diff review. Record per-finding implementation,
RED/GREEN/closure evidence and any bounded command corrections, preserving stable
finding IDs for the next focused review.

## Implementation evidence

Candidate: `9e8f1d9d0..96fea41e1` (commits `10e58672e`, `c0563eca2`,
`3cac3e7b0`, `971f21ede`, `96fea41e1`).

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1 | `scribe/memtable.rs`: `LiveReadOwners` holds the shared capture charges plus one `MemoryReservation` registered on the follower session's pool (`FetchLiveTailRequest.memory`, threaded in `oracle/follower.rs` from `session.runtime_env().memory_pool`, the existing `ScribeResources::follower_execution` `GovernedMemoryView`). `project` grows the reservation by `unshared_arrow_bytes(batch, &[captured])` per emitted batch and resizes a workspace reservation for logical fallback rows; refusal maps to `ScribeError::Internal` → existing `FollowerResolutionError::Fault`. `MemtableSource` keeps the owners alive until stream drop. No new tracker or budget. | P1 RED with growth removed: reserved `0` vs expected `24984`. P1 GREEN: computed read reserves exactly the unshared bytes in pool and root `oracle_total_bytes`, capture charges unchanged; drop releases to 0; bare column read charges no shared buffers; 1.5-batch ceiling refuses with 0 pool and 0 root residue. | PASS |
| FIND-TASK-001-2 | `oracle/live.rs` `resolved()` snapshots absorbed outputs with `try_map_exprs(snapshot_physical_expr)` before the predicate early return, so no-WHERE outputs ship literals and a pending output errors before any fragment. | P2 RED: "a pending output cannot resolve" panic. P2 GREEN: pending refuses; the same plan resolved at 41 and 97 ships each literal, names `[v, m]`, no `ScalarSubquery`. P4 GREEN (mixed, role-separated; exact rows for bare, nested computed and Variant-projection scalar outputs; fragment count rises). P5 GREEN. | PASS |
| FIND-TASK-001-3 | `oracle/follower.rs` `signed_requirements`: every `collect_columns` column must match the closure field at its index by name, and a filter must type to `Boolean` over the closure, before `tail.open`. | P3 RED: "requirements outside the closure refuse" panic. P3 GREEN: bare filter, nested `IsNull(unused_payload@2)` filter and nested output refuse; tail opened only for the two valid cases. | PASS |
| FIND-TASK-001-4 | `oracle/codec.rs`: `RemoteScanPayload` fields, `RemoteScanOutputs`, `with_requirements`, `preflight_extension` are `pub(crate)`; `remote_scan_filter_for_test` under `test-support` decodes through the follower preflight. `live_subquery.rs` uses it. | Default-feature `cargo clippy -p vala-bifrost-redux -- -D warnings` clean; `mise run lints` clean; P5 GREEN inspecting the real decoded filter. | PASS |
| FIND-TASK-001-5 | Rustdoc for `live_leaf` (memtable at `output_schema` with projected/inexact statistics; staged runs at `required_schema` with the same requirements), `logical_column` and `evaluate` (invariants, `# Panics`; the `expect` replaced by `Option::insert`), `resolved`, `signed_requirements`, `read`/`project`, new items; `prove_variant_memory_answers` replaces indexing with slice patterns. | Source review; `mise run lints`. P6 GREEN. | PASS |

### Diagnosis — P4 passes without the FIND-TASK-001-2 fix

- **Symptom:** `distributed::scalar_subquery_outputs_survive_live_projection_absorption` passed with the `resolved()` output snapshot removed (rebuild confirmed).
- **Evidence:** temporary root-plan print in `oracle/mod.rs` (removed) showed the server plans projection scalars as a Left `NestedLoopJoinExec` over the aggregate (`max` directly; unprovable shapes through `single_value`); no `ScalarSubqueryExec` or `ScalarSubqueryExpr` reaches `LiveScribeExec`. A default `SessionStateBuilder` plan (unit experiment, removed) does absorb `ScalarSubqueryExpr` outputs into the leaf. `EXPLAIN` is refused by the query surface.
- **Cause:** the server's logical optimizer decorrelates projection scalar subqueries into joins, so the defect is latent on the public SQL surface.
- **Fix site:** none outside the task: the fix stays in `LiveScribeExec::resolved` (the only output-shipping owner); P2 proves it at the owner, P4 pins exact SDK results on both topologies whichever plan the server picks. No assertion was changed.

### Bounded command corrections

- The owner-family filter also selects Postgres-backed `scribe::shards` fixtures; they ran under `scripts/postgres/with-test-postgres.sh` (58/58).
- A peer-network output journey was tried for P4 RED and dropped for the same latent cause; no harness was added.

### Commands

| Command | Result |
|---|---|
| P1, P2, P3 exact selectors | PASS |
| P4, P5, P6 exact journey selectors under `with-test-postgres.sh` | PASS |
| `cargo nextest run -p vala-bifrost-redux --lib` owner families `scribe::(memtable\|tail_rpc)`, `oracle::(live\|codec\|follower\|memtable_source)` | 75/75 PASS |
| `scribe::shards` under `with-test-postgres.sh` | 58/58 PASS |
| `mise run fmt`, `mise run lints`, default-feature clippy `-D warnings` | PASS |
| `mise run check:deps`, `mise run check:tenant-isolation` | PASS |
| `git diff --check` | PASS |

### Non-goals and limits

No new resource root, tracker, budget, public API, codec version, persisted
format or dependency. `changes/active/benchmark-scaling/*` untouched. Workspace
reservation covers logical fallback rows; other per-batch transients stay bounded
to one batch. Full journey suites and `gate` were not run (change review owns them).

# Behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md`, revision 1
- Original task: no task artifact exists; the approved specification plus the owner's direction to verify safe Bifrost error mapping and non-hack, performance-safe live subquery support is the task authority.
- Cumulative range reviewed: `git diff 526355fb8..f1e1be815`

The candidate commit remained `f1e1be815069b23fa9920a61c1198524ecaad514` during this review.

## Navigation and caller coverage

The error path was traced from Oracle's pre-planning table extraction and cut pinning through `Oracle::plan_logical`, `map_sql_planning_error`, the stable `BifrostError`/`WyrdError` catalog, the shared HTTP client, and the Rust, Python, TypeScript, and MCP journeys. The authorization path resolves and authorizes the complete table set before registering only those providers into a query-local DataFusion session, so diagnostics from logical planning cannot discover an unauthorized provider.

The subquery path was traced from DataFusion's `ScalarSubqueryExec`/`ScalarSubqueryExpr`, through DataFusion physical filter pushdown, `LiveScribeExec::handle_child_pushdown_result`, fragment encoding, and local and remote Scribe live execution. Existing ordinary predicate pushdown and route pruning remain active for conjuncts which do not read a scalar subquery. The added expression walk runs during physical planning, not per row or per batch.

The owned dependency sources at DataFusion revision `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` were also inspected because placement is part of the task. `FilterPushdownPhase::Pre` documents that expressions referencing an `ExecutionPlan` cannot be pushed during that phase, while `FilterExec::gather_filters_for_pushdown` currently forwards every conjunct. `ScalarSubqueryExpr` reads state populated by its owning `ScalarSubqueryExec`, and its proto decoder accepts it only within that surrounding execution node.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002: logical SQL caller faults are safe 4xx errors carrying DataFusion's actionable message | `oracle/mod.rs:3018-3033`, `3746-3782`; logical planning uses `map_sql_planning_error`, unwraps DataFusion's context/diagnostic/shared/collection wrappers, and exposes only SQL/Plan/Schema/NotImplemented leaves | Rust, Python, TypeScript, and MCP journeys cover unknown column in single-table, join, and subquery forms, plus type mismatch, ambiguous reference, and unknown function; focused mapper unit test is present | PASS for the covered logical-planning faults |
| REQ-002: unknown table receives the required planner diagnostic | Table resolution and cut pinning happen before a DataFusion planning session is populated (`oracle/mod.rs:1678-1711`, `3231-3275`); catalog lookup returns `BifrostError::TableNotFound` | MCP keeps the base expectation `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` at `wyrd-mcp/tests/bifrost/mcp/query.rs:315-319` and makes no planner-detail assertion | FAIL (`BHV-001`) |
| REQ-002: internal failures remain 5xx and do not expose dependency internals | `oracle/mod.rs:3790-3814`; physical planning retains a fixed safe detail and non-caller errors use the general catalog mapper | `oracle::tests::datafusion_query_rejections_are_safe_and_actionable` checks Internal, Execution-in-Collection, physical planning, and resource exhaustion; unauthorized-table journeys on Rust/Python/TS prove authorization happens before schema diagnostics | PASS, subject to full lane execution |
| REQ-002: behavior is consistent over HTTP/Rust/Python/TypeScript/MCP | Public Rust query journeys exercise shared HTTP transport; Python and TypeScript project the same `wyrd-client` transport; MCP calls the real `/mcp` endpoint | `query_bifrost.rs`, `test_query_bifrost.py`, `query-bifrost.test.ts`, and MCP `query.rs` assert stable code and detail | PASS for covered faults |
| REQ-006: scalar, IN, EXISTS, and run-id-correlated live-table subqueries equal equivalent joins | `oracle/live.rs:395-423`, `509-523` keeps scalar-result expressions at the leader; IN/EXISTS are handled by DataFusion's normal plans | Rust/Python/TypeScript/MCP live journeys compare each required form with its join; analytical correctness covers published, live, mixed, and remote-live multi-table workloads | PASS behaviorally |
| REQ-006: support is owned at the correct layer rather than a consumer-specific workaround | Wyrd downcasts to DataFusion's concrete `ScalarSubqueryExpr` in `oracle/live.rs:515-522` | Dependency source shows the violated invariant originates in DataFusion `FilterExec::gather_filters_for_pushdown`, which forwards the plan-linked expression during `Pre` despite the phase contract | FAIL (`BHV-002`) |
| REQ-006: no query hot-path performance regression | Added traversal is bounded by each offered physical expression and occurs once during planning; ordinary non-subquery conjuncts still reach route and fragment pruning | Existing `pushed_filter_ships_on_the_fragment_and_stays_on_the_leader` pins ordinary predicate pushdown; analytical live/mixed/remote journeys exercise realistic query shapes. No capacity benchmark result was supplied | PASS by source analysis; benchmark evidence remains a verification limit |
| INV-001: tenant isolation | Full table identities are resolved and authorized before provider registration and planning; each planning session owns an isolated catalog | Rust/Python/TS unauthorized cross-schema journeys assert `WYRD_VALA_403_QUERY_FORBIDDEN` and no requested/real column spelling leaks | PASS |
| AC-001: each requirement has cross-surface journeys which fail on base and pass on candidate | New Rust/Python/TS/MCP subquery and error-detail assertions are in the cumulative diff; Rust shared HTTP is the direct HTTP path | Source comparison confirms the prior MCP detail assertion expected the fixed generic message, while new scalar/live journeys do not exist on base | PASS except the unchanged unknown-table case covered by `BHV-001` |
| AC-002 as applicable to this split: unauthorized Bifrost table leaks nothing | Authorization precedes query-local provider registration and logical planning | Rust/Python/TS negative journeys misspell an unauthorized column and assert neither spelling is exposed | PASS |
| AC-003: format, lints, codegen, and owning lanes pass | `mise.toml` defines `verify:bifrost` as the scoped aggregate | The exact mapper unit test passed; no aggregate results were supplied | NOT PROVEN |
| Non-goal: do not expose physical/dependency internals | Physical-planning failures use `PHYSICAL_PLANNING_REFUSAL`, while logical diagnostics come only after table authorization in a query-local catalog | Mapper unit tests distinguish logical and physical detail | PASS |

## Proposed findings

### BHV-001 — INCORRECT — unknown-table behavior does not satisfy REQ-002

- **Violated obligation:** REQ-002 explicitly includes an unknown table among caller SQL planning faults whose 4xx `WyrdError` detail carries the planner's message, and AC-001 requires the journey to fail on base and pass on the candidate.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1678-1711`, `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:388-391`, and `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:315-319`.
- **Evidence:** `prepare_query_attempt` parses table references and pins their catalog cuts before `plan_logical` creates a DataFusion plan. A missing registration therefore returns `TableNotFound` directly. The MCP journey preserves the base's `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` expectation and does not assert a planner diagnostic for the case. No changed Rust, Python, or TypeScript journey covers unknown-table detail.
- **Observable consequence:** an unknown-table caller receives the unchanged catalog error rather than the approved planner-detail behavior, and this part of REQ-002 has no base-failing/candidate-passing proof.
- **Required testable correction:** reconcile the missing-table path with the approved contract while preserving authorization-before-diagnostics. Add public Rust/HTTP, Python, TypeScript, and MCP assertions for the selected safe actionable detail and prove the base fails. If the intended contract is that unknown tables remain the catalog-owned 404 rather than a planner diagnostic, that is a specification correction rather than implementation acceptance.

### BHV-002 — DRIFT — the scalar-subquery correction is attached to the Wyrd live leaf instead of the DataFusion filter-pushdown invariant

- **Violated obligation:** the owner's direction requires real subquery support rather than a support-specific hack and asks that DataFusion-specific behavior be placed in the owned fork.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/live.rs:395-423` and `509-523`; dependency source `datafusion/physical-plan/src/filter.rs:718-735` and `datafusion/physical-plan/src/filter_pushdown.rs:55-63` at revision `fbe115d2...`.
- **Evidence:** the Wyrd leaf recognizes a concrete DataFusion expression by downcast and removes it after DataFusion has offered it for pre-optimization pushdown. DataFusion's `Pre` phase contract says expressions referencing execution plans cannot be pushed then, but `FilterExec::gather_filters_for_pushdown` forwards all predicate conjuncts without applying that invariant. `ScalarSubqueryExpr` reads a result container populated by the surrounding `ScalarSubqueryExec`; DataFusion proto decoding likewise permits it only under that owner. The invalid state is therefore produced by DataFusion filter pushdown, before Wyrd sees it.
- **Observable consequence:** this live leaf works for today's concrete scalar expression, but every other pre-pushdown consumer must independently know and reject the same DataFusion implementation type; another execution-plan-backed expression can repeat the failure. Wyrd is also coupled to a dependency-private planning shape it does not own.
- **Required testable correction:** enforce the pre-phase execution-plan-reference rule once in the owned DataFusion fork, with a DataFusion optimizer test proving a scalar-subquery predicate remains with its `ScalarSubqueryExec` and is not offered below it. Bump Wyrd's pinned revision and delete `reads_scalar_subquery` and its Wyrd-specific filter. Retain the Wyrd live, mixed, published, and remote-live journeys as end-to-end closure proof. `datafusion-distributed` is not the source: the invalid pushdown is produced before distributed stage planning.

## Non-blocking observations

- The logical error mapper's authorization argument is supported by source: providers are registered only after the complete resolved table set has passed permission checks, and each query gets a fresh catalog list. The changed unauthorized-table journeys exercise the important cross-schema case.
- The expression walk does not introduce a row-path cost. The performance concern is ownership and repeated consumer-specific classification, not measured execution throughput in this candidate.
- The larger analytical fixture is broad but directly exercises the requested joins/subqueries across published, live, mixed, and remote-live states. It is not proposed for deletion here.

## Verification limits

- No `verify:bifrost`, formatting, lint, codegen, or complete journey result was supplied to this reviewer.
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'` passed (1 test, 829 skipped).
- Capacity benchmarks are intentionally outside the regular gate and were not run. Source analysis confines the new work to planning and preserves ordinary conjunct pushdown, but this review has no before/after latency measurement.

## Overall result

**FAIL**

REQ-002's unknown-table obligation remains unchanged and unproved, and the live scalar-subquery behavior is implemented as a Wyrd consumer guard around a violated DataFusion pre-pushdown invariant. The remaining covered error mappings, authorization ordering, tenant isolation, cross-language projections, and live/published/mixed/remote subquery results are supported by the cumulative source and journeys.

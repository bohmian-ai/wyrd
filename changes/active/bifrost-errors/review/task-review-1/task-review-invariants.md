# Invariant review: Bifrost caller errors and live-table subqueries

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md`
- Original task: no task artifact exists; the approved specification plus the owner's direction to fix Bifrost error mapping and add scalar, `IN`, and correlated `EXISTS` support without regressing ordinary-query performance is the review authority.

The candidate commit matched the stated candidate at the start and end of this review.

## Invariant trace

### SQL failure producer to public error

`OraclePlanner::validate_query` parses and extracts all referenced tables, and `OraclePlanner::read_and_materialize` resolves the tenant-scoped cut and authorizes the resolved identities before object IO (`crates/vala/vala-bifrost-redux/src/oracle/planner.rs:40-62`, `:82-153`). `Oracle::build_physical_root` then registers only those already resolved cuts in the planning session before calling `plan_logical` (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2735-2755`). Consequently, the planner cannot suggest columns from an unregistered or unauthorized table.

`Oracle::plan_logical` maps SQL lowering, parameter binding, and logical optimization failures through `map_sql_planning_error` before the payload-column authorization gate (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3000-3034`). `caller_planning_fault` unwraps DataFusion's `Context`, `Diagnostic`, `Shared`, and first-error `Collection` wrappers to the typed SQL/plan/schema/not-implemented cause, matching DataFusion's own `Error::source` and display semantics for `Collection`. Resource exhaustion is checked first. The SQL mapper exposes only that typed cause with its backtrace removed, while physical-plan construction continues to expose a fixed repair action and all other errors retain the stable general mapping (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3746-3797`). This preserves the invariant that caller SQL faults are repairable 4xx errors while execution, dependency, and resource faults are not relabeled or disclosed.

The public HTTP query path is shared by the Rust, Python, and TypeScript SDKs; MCP calls the same Bifrost client. No language-specific mapping was added. The Rust journey exercises single-table, three-way join, and subquery suggestions plus type mismatch and ambiguity (`sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:320-392`); Python and TypeScript exercise the same shared wire errors (`sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py:130-191`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts:151-203`); MCP additionally covers missing table and function cases (`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs:298-398`). The scoped-reader journeys verify that a query naming an unauthorized table is rejected before planning and that neither the misspelling nor the real column name reaches the detail.

### Logical plan to live Scribe fragment

DataFusion 55 represents an uncorrelated scalar subquery with a `ScalarSubqueryExec` that owns and populates a process-local `ScalarSubqueryResults`; `ScalarSubqueryExpr` reads the corresponding slot. A serialized expression without its surrounding executor therefore has no populated result container. The pinned `datafusion-distributed` revision documents the generic stage-boundary form of the same failure in `src/metrics/task_metrics_collector.rs:221-228`.

The changed path is distinct from generic DataFusion-distributed stage cutting. `LiveScribeExec::handle_child_pushdown_result` is a Wyrd-owned optimization: it reports every filter unsupported so DataFusion keeps the authoritative `FilterExec` on the leader, but duplicates safe static filters into the separately constructed Scribe fragment for remote filtering and route pruning (`crates/vala/vala-bifrost-redux/src/oracle/live.rs:382-423`). That fragment consists only of Wyrd's remote-source placeholder and the duplicated filter; the generic distributed planner cannot keep a `ScalarSubqueryExec` beside it after the fact. The correct owner for deciding whether a leader expression is safe to duplicate is therefore `LiveScribeExec`, not either DataFusion fork.

The candidate excludes only expression trees containing `ScalarSubqueryExpr` (`crates/vala/vala-bifrost-redux/src/oracle/live.rs:401-406`, `:509-523`). The leader filter remains because the leaf still reports every filter unsupported. Ordinary static filters retain the existing fragment pushdown and route pruning; DataFusion splits top-level `AND` conjuncts before presenting them to the leaf, so a scalar-dependent conjunct does not suppress independent static conjuncts. Dynamic filters retain their prior exclusion. The existing focused test proves ordinary filter duplication remains (`crates/vala/vala-bifrost-redux/src/oracle/live.rs:970-1025`), while the live user journeys prove scalar, `IN`, ordinary correlated `EXISTS`, and managed-`run_id` correlated `EXISTS` results against equivalent joins (`sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:246-318` and equivalent Python, TypeScript, and MCP journeys).

The broader analytical comparison exercises published, live, and mixed data against plain DataFusion, and the peer journey places every live row on a remote Scribe so a lost or incorrectly local fragment changes the answer (`sdks/wyrd-sdk-rust/tests/integration/analytical_correctness.rs:28-132`, `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs:19-106`). Tenant identity and authorization remain in the existing frozen-cut, provider, and fragment bindings; the change neither adds a source nor widens a binding.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002: caller SQL planning faults are 4xx and carry the planner detail across single-table, joins, and subqueries | Separate logical SQL mapper plus wrapper unwrapping at `oracle/mod.rs:3746-3782`; shared client/HTTP path unchanged | Rust/Python/TypeScript journeys and MCP matrix cover suggestion, type mismatch, ambiguity, missing function, and missing table | PASS |
| REQ-002: internal/resource/physical failures retain safe mappings | Resource-first and typed-cause classification at `oracle/mod.rs:3746-3797`; physical planner retains fixed detail | Unit matrix covers context, diagnostic, collection, shared, internal, execution, and resource variants at `oracle/mod.rs:4475-4568` | PASS |
| REQ-002 / INV-001: planner details disclose no unauthorized schema | Cuts are resolved and authorized before session registration and SQL planning; only authorized providers enter the session | Scoped-reader journeys in Rust, Python, and TypeScript plus existing MCP authorization path | PASS |
| REQ-006: scalar, `IN`, and `EXISTS` over live tables match joins, including `run_id` correlation | Runtime-bound scalar filters stay beside `ScalarSubqueryExec` on the leader; decorrelated `IN`/`EXISTS` use normal physical plans | Rust/Python/TypeScript/MCP live-table matrices; broad live/mixed/published DataFusion oracle comparison | PASS |
| REQ-006: remote live execution and tenant isolation remain intact | Only Wyrd's optional duplicated predicate is narrowed; signed fragment assignment and tenant-bound provider paths are unchanged | Remote-Scribe analytical comparison proves rows cross the private fragment path; scoped authorization journeys prove rejection before planning | PASS |
| Owner direction: no Wyrd-specific workaround where a DataFusion owner exists | `LiveScribeExec` itself creates the reduced fragment outside generic stage planning, so it owns duplicability of its filter; no fork patch can reconstruct the missing leader-owned runtime value inside this Wyrd fragment | Source trace against pinned DataFusion and datafusion-distributed revisions | PASS |
| Owner direction: no regression to ordinary-query performance | Only scalar-subquery expression trees are excluded; ordinary and independent conjunct filters still ship and prune routes | Existing focused static-filter pushdown test plus broad analytical queries | PASS |
| AC-001: user journeys on every shipped surface and MCP/HTTP agent surface | Shared server implementation; no SDK-local behavior | Rust, Python, TypeScript, and MCP journeys added; SDK and MCP paths use public HTTP | PASS |
| AC-002 as applicable to this split: unauthorized Bifrost tables reveal nothing | Authorization occurs before provider registration and logical planning | Rust/Python/TypeScript scoped-reader journeys | PASS |
| AC-003: format, lints, codegen, and owning Bifrost lanes | N/A | No completed verification results were supplied. A focused two-test nextest invocation was attempted but could not acquire the already-held shared Cargo build lock and was cancelled without running tests. | NOT VERIFIED |

The Jev and malformed-`traceparent` phrases in AC-002 have no corresponding requirement or changed surface in this split specification; treating them as Bifrost acceptance criteria would broaden the owner-directed change. They were not reviewed as implementation obligations here.

## Proposed findings

None.

## Verification limits

This pass validated the complete base-to-candidate diff, current callers, the pinned DataFusion/DataFusion-distributed behavior, and the test source. It did not obtain runtime results for the ignored Postgres-backed journeys or the repository format/lint/codegen/Bifrost lanes. The final orchestrated verdict should require those existing gates before change completion under AC-003.

## Overall result

**PASS**

The invariant and ownership review found no implementation defect. The remaining uncertainty is execution of the candidate's verification lanes, not a source-level finding.

# Maintainer review — cumulative remediation candidate

## Immutable subject and authority

- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`.
- Specification: `changes/active/bifrost-errors/spec.md`; no original task existed.
- Remediation: `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`, including the owner's compact common-query and CTE-join coverage directions.
- Inspected the cumulative diff, not merely the optimizer-pass fix; used the earlier maintainer report to identify closure hypotheses rather than inherit conclusions.
- Authorities: `AGENTS.md` §§3–6, 11, 15–16; `architecture/agent-rules.md`; the complete maintainer-style, Rust-core, testing-workflows, errors, and DataFusion references; relevant Oracle/distributed-query, client-boundary, and error-catalog sections of Bifrost design, Wyrd design, and doctrine. CodeGraph was the first production-symbol navigation mechanism.

## Changed-surface coverage

| Surface | Source and consumers inspected | Assessment |
|---|---|---|
| Logical and physical planner refusals | `oracle/mod.rs::Oracle::plan_logical`, `caller_planning_fault`, both planning mappers, general mapper/resource precedence, error unit test and language/MCP journeys | PASS. Small stateless classification helpers stay beside the Oracle mapping boundary; no alternate public error catalog or SDK mapper. The logical-versus-physical disclosure distinction is explicit in names, documentation, and tests. |
| Live predicate ownership and runtime serialization | Full bodies of `LiveScribeExec::new`, `resolved`, `handle_child_pushdown_result`, `fragment`, `execute`, `LiveFragmentRead::into_stream`; `OracleTableProvider` live-leaf construction, execution bindings and existing route selection | PASS. The existing leaf retains the runtime predicate; its per-stream read owner creates a temporary static leaf. Route pruning reuses `LiveTableRoutes::selected_by`, recursive conversion reuses DataFusion snapshotting, fragment encoding reuses the existing codec. No scalar interpreter, operator allowlist, alternate planner, or reusable resolver framework was added. |
| Session optimizer configuration | `resources.rs::OracleSessionShape::apply`, leader physical-root construction, `analytical.rs::planning_session`, shared runtime/stage configuration consumers | PASS. Two optimizer passes are configured at the established shared session owner; there is no per-query SQL heuristic or competing join planner. The explanatory rustdoc names the rule-order dependency and the affected consumers. |
| Fork and revision cone | `Cargo.toml`, lockfile, codec revision; DataFusion `fbe115d2..5697cfa` complete diff and scalar snapshot tests | PASS for placement/reuse. Fork change is only `ScalarSubqueryExpr::snapshot` and its tests, built on `Literal`, `FieldMetadata`, and the dependency's existing snapshot contract. All patched DataFusion crates and the peer codec pin advance together. |
| Remote execution observations | `state.rs::Scribe` fields, constructor, record/access methods; `peer_service.rs` execution and footer producer; `NativeOutputTally::rows`; `PeerCluster::scribe_fragment_output` caller | PASS. Extends the existing Scribe activity observation instead of constructing another transport or oracle. Plan bytes and aggregate row retention are feature-gated test-support state, with the actual footer tally as producer. No new shipped SDK, HTTP, MCP, or generated contract. |
| Shared managed-run fixture | `BifrostWriter::enqueue` and `write`, `prepare_oracle_query_fixture`, their callers | PASS. Correlation is passed to the existing writer and server-stamped managed column. Existing write convenience method still supplies `None`; fixture-specific rows use typed `RunId`. No fixture-owned ingestion implementation. |
| Focused cross-surface journeys | Complete added Rust, Python, TypeScript and MCP query/error cases and their surrounding fixtures | PASS. Uses real public clients and existing server lifecycle owners. Assertions check explicit expected rows as well as equivalent joins; negative cases check stable codes, meaningful planner detail and authorization redaction. Runtime-specific tests remain in their owning language. |
| Compact analytical corpus | `query_coverage.rs` complete fixture, `Case`, `cases`, writer/write helpers; integration module and mise lane registration | PASS. Sixteen explicit cases use ten small fixed rows in two tables, published then live, with hand-written expected JSON results. Covers CTE-to-second-table join, joins, aggregate/scalar/correlated shapes, null-sensitive membership/existence and window ties. Small local helpers retain caller-owned dependencies; no public workload API, generated model or secondary DataFusion oracle. |
| Remote-live proof | Complete `peer_network/live_subquery.rs`, `LiveTables`, seeding and query helpers, layout registration extension and peer journey registration | PASS. Separate selective scalar and representative join assertions use the existing peer cluster. Exact fragment-count and footer-row assertions directly distinguish the old remote over-transfer from correct results alone. The fixture owns real cluster/table state and its two small pure time/id methods earn their placement. |
| Persistent docs and declarations | New/materially changed Wyrd Rust functions, fields, aliases, constants and tests; Python/TS typing; review/spec evidence documents | PASS for substantive Wyrd rustdoc and unchanged public declarations. New private helpers and tests describe their role; fallible Wyrd methods document errors and panic-bearing tests document panics. No generated declaration was hand edited. |

## Prior maintainer hypotheses and closure

- The old request to replace the caller-fault walk with `find_root` was rejected by prior independent validation. Current source still intentionally stops at typed caller-planning wrappers; traversing arbitrary external/internal causes would change disclosure policy. This is a focused Wyrd classification boundary, not gratuitous duplication of generic source traversal.
- The scalar-specific exclusion and its silent remote over-transfer are gone. `resolved` snapshots into an execution-local leaf and the focused unit/journey tests now observe predicate materialization, static pruning, reset/re-execution, route uniqueness, and actual remote returned rows. The original plan keeps its stateful expression.
- The public generated support workload and embedded oracle are absent in the cumulative result. `wyrd-testing` keeps DataFusion as a dev-dependency; common-query coverage now stays in one Rust SDK integration module, as the owner requested. The focused corpus is intentional coverage, not independently discovered drift.

## Material findings

None proposed by this maintainer audit. No new source-local finding IDs.

## Non-blocking evidence notes

- The remote scalar journey's opening rustdoc says Scribe “reads and returns no more than” one event. Its explicit assertion measures returned footer rows, not decoded/scanned source rows. The body makes the intended network-transfer property clear, and the distinction should remain explicit when interpreting this evidence; it does not establish one-row physical IO.
- The task recommendation and criterion 3 describe runtime narrowing before partition dealing, while actual code deliberately deals the immutable planned route list first and narrows each share. The code's rustdoc explains the ownership invariant and the unit test proves the union of selected shares. Acceptance should judge that invariant rather than require the obsolete private sequencing.

## Verification limits and result

This was a read-only source/maintainer review. I inspected the committed evidence table (focused fork and Wyrd tests, remote scalar/join journeys, mixed corpus, Oracle lane, formatting/lints/generated/boundary checks); did not rerun benchmarks, heavy tests, or broad gates. Green evidence does not substitute for source review. The recorded earlier 604-byte MCP release observation remains an unresolved evidence limit for the system reviewer; this report does not diagnose it or claim it is harmless. General scalar cardinality and dynamic-filter semantics are subject to the independent behavior/domain audits.

**Overall result: PASS.** Existing owners cover the runtime and test boundaries, the correction adds no competing planner/evaluator/framework, and the common-query coverage is compact and understandable.

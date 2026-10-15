# Independent maintainer review — round 5

## Subject and result

**PASS** for maintainer review. No material maintainer findings.

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`8a54d81c346db464b624e81ebe8958c63bb7e228`, and the cumulative
DataFusion fork `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`8c3f18e9806ae23e8840a710af775c058dfad66a`. The latest native delta
was located using previous fork `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`.
The subject remained unchanged at the final source check.

This is an independent maintainer assessment of source, ownership, readability,
and test shape; it does not substitute for the behavior, invariant, standards,
system, or domain acceptance audits.

## Authority and navigation

Applied AGENTS.md, architecture/agent-rules.md, the reference router,
maintainer-style.md, spec-driven-development.md, Rust/error/DataFusion
references, applicable Wyrd client/server doctrine, Bifrost query and distributed
execution authority, TESTING.md, approved revision-1 spec, and the existing
remediation authority. CodeGraph exploration preceded my Wyrd symbol/caller
navigation; source and Git diffs supplied the immutable cumulative comparison.
No other current discovery report or intended verdict was consulted.

## Changed-surface coverage

| Changed owner or surface | Source and surrounding callers inspected | Maintainer assessment |
|---|---|---|
| Native `Filter::is_scalar` and `is_fixed_value` | `datafusion/expr/src/logical_plan/plan.rs:2893–2987`, existing unique-dependency loop, `max_rows` Filter branch, `logical_plan/invariants.rs` correlated-scalar consumer, both scalar rewrite call sites | Small synchronous proof classifier is adjacent to its sole direct caller. Existing Filter retains proof ownership; no new proof framework, evaluation, planner recursion, state, or dependency. Positive expression cases and fallback can be followed in one match. |
| Existing native expression contracts | `expr.rs:2137–2173` (`any_column_refs`, `is_volatile_node`, `is_volatile`); `expr-common/src/signature.rs:46–89` | Existing volatility contract is reused directly for scalar functions. The existing negative traversal helpers do not independently establish invariance of opaque expression forms, so they are not a complete replacement for the positive classifier. |
| Native scalar cardinality | `max_rows` join/grouping/union changes; `scalar_subquery_to_join.rs` Filter and Projection branches, registry lookup, complete `build_join`; `functions-aggregate/src/single_value.rs`; aggregate default registration and expression export | One shared rewrite boundary applies one native aggregate. Partial/final row-count state stays with the native accumulator and uses ordinary aggregate codec support. No Wyrd duplicate cardinality path. Conservative bounds are documented where produced. |
| Native scalar snapshot and codec | `physical-expr/src/scalar_subquery.rs` snapshot and existing execution container; native proto round-trip test | Implements the existing `PhysicalExpr::snapshot` extension point; preserves metadata and typed NULL. No new plan node or serialization format. |
| Wyrd planning errors | `oracle/mod.rs` complete caller-fault classifier and logical/physical/general mappers, logical planning caller and focused mapping test | A narrow local classifier walks native wrappers; separate logical and physical mapping names expose the disclosure boundary. Existing catalog variants and general mapper remain the public contract. Tests state exact returned detail and internal-error opacity. |
| Live fragment pushdown and read lifecycle | `oracle/live.rs` `LiveScribeExec`, `resolved`, `fragment`, pushdown propagation, `execute`, `LiveFragmentRead`; provider construction in `oracle/exec.rs` | Retains existing leaf/read owners. First-poll snapshot, reusable runtime predicate, and deal-before-prune route ownership are explained at the methods that enforce them. No extra transport, reader, scheduler or lifecycle abstraction. |
| Session and compatibility pins | `resources.rs` `OracleSessionShape`; Cargo.toml/lock and `oracle/codec.rs` revision | Two-pass rationale describes optimizer order and observable join effect. Dependency/codec revision remains explicit and synchronized. |
| Scribe journey observations | `NativeOutputTally::rows`, peer fragment executor, `state.rs` Scribe observation methods, `PeerCluster::scribe_fragment_output` | Existing Scribe owns counters and test-only latest plan. Footer row observation comes from the actual output tally. The plan mutex and bytes are test-support gated; no shipped second query observation owner. |
| Existing fixture extension | `BifrostWriter::enqueue`, both callers in query fixture/write, `prepare_oracle_query_fixture`, `PeerCluster::register_table_with_layout` and existing registration method | Existing writer and server fixtures are extended with managed correlation/layout rather than duplicated. Convenience registration delegates to the same registration path. Materially modified Wyrd operations document intent and errors. |
| Remote journeys | Entire `peer_network/live_subquery.rs`, its module wiring and reused peer support | `LiveTables` owns meaningful topology/table state. Focused helpers remove repeated setup and retain visible SQL/assertions. Literal/footer proof, remote join and scalar refusals share native fixtures. Latest grouped-count cases extend the existing refusal journey and reuse one public client/key. |
| Rust SDK corpus and query journeys | Entire `query_coverage.rs`, changed `query_bifrost.rs`, integration main registration and mise lane | Compact fixed two-table data and exact expected results are readable without generated workloads or an external query oracle. Includes published/live mixture, NULL-sensitive joins/subqueries, CTE joined to another table, window and scalar refusal controls. Existing deployment/client is reused. |
| Python and TypeScript query journeys | Changed modules, fixtures and public SDK calls; names/types and corresponding Rust cases | Language runtime fixtures use public Bifrost clients and preserve typed results/errors. Parameterized query shapes remain explicit. No SDK implementation/export/stub/declaration change requires parity work. |
| MCP tests and native SQL evidence | Changed MCP query tests, `subquery.slt`, aggregate documentation/config changes, proto controls | Assertions expose intended result/refusal rather than merely successful invocation. Wyrd test rustdoc includes relevant Errors/Panics. Native SQL cases document the physical-scalar flag boundary and preserve genuine singleton controls. |
| Review/spec packet | Spec and previous remediation/evidence context, current cumulative changed-file list | Review history remains in the designated change packet; it introduces no permanent code-level task mechanism. |

## Reuse and complexity assessment

The latest correction needs a proof stronger than “not a bare column.” Its local
classifier has no retained dependencies or state, so a free function beside
Filter is appropriate under the repository's struct-centered rule. A utility
struct, trait, configurable expression registry, or new constancy evaluator
would introduce ownership that this operation does not need. The existing
native `Volatility` enum supplies the scalar-function rule.

Cumulatively, the implementation extends DataFusion's existing scalar snapshot,
logical row-bound proof, scalar-to-join rewrite and aggregate registry. Wyrd
extends its live scan leaf, current session shape and stable error mapping.
The added tests extend established server and SDK fixtures. I found no new
parallel workflow whose behavior should instead be folded into another
existing owner.

## Findings and limits

Material maintainer findings: **none**. No naming, layout or private-shape
preference is promoted to a blocking issue.

A behavior question concerning conservative opaque expressions in correlated
scalar queries was forwarded to the orchestrator for the independent acceptance
review: `invariants.rs` also consumes `max_rows`, whereas the implementation
packet describes conservative fallback chiefly as runtime validation. That is
a compatibility assessment, not a maintainer-style finding or a conclusion
that the current classifier is unsound.

I assessed the recorded implementation proof and actual assertions; I did not
rerun builds, SQL suites, journeys, lint/codegen lanes, benchmarks or synthetic
load. This report establishes maintainability of the inspected implementation,
not exhaustive SQL correctness or a measured performance guarantee.

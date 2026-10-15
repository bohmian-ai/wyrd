# Round-6 immutable subject and navigation map

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Wyrd candidate: `95b350d078226e811ac298230c233bf935975d42`.
- Integration baseline: `945a1548c47897dc94583169a6ba04163a73a749`.
- Original task base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Previous reviewed candidate: `8a54d81c346db464b624e81ebe8958c63bb7e228`.
- Review complete cumulative integration-baseline → candidate diff; original
  history/main integration is preserved in earlier subjects. No new merge
  since round 5. Latest delta locates fixes, not acceptance scope.
- DataFusion cumulative original: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- Previous fork: `8c3f18e9806ae23e8840a710af775c058dfad66a`.
- Current fork: `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`.
- Fork worktree: `/home/thorrester/Documents/GitHub/datafusion-scalar-snapshot`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

## Authority and evidence

Approved revision-1 `changes/active/bifrost-errors/spec.md`; no original task
existed. Read prior verdicts/ledgers/remediations in task-review-1/2/3,
task-review-4-resumed and task-review-5 as cumulative authority and hypotheses.
Latest task: `../task-review-5/bifrost-errors-R5-preserve-fixed-correlated-expressions.md`.
Its appended exact-command/evidence table is available for assessment against
actual source/assertions. Do not rely on the user's implementation summary.

Owner directions: correct caller planning errors and native live subqueries;
compact common-query coverage including joins, subqueries and joined CTE;
safe proof construction at its native owner, preserve valid queries and fix
root causes across sibling consumers; no benchmarks or synthetic host load.
Applicable AGENTS, agent-rules, reference router, spec-driven-development,
maintainer guidance, Wyrd design/doctrine, Bifrost/testing and native domain
authority govern. No authority file changed in latest Wyrd delta. CodeGraph
was consulted before Wyrd navigation this round; each reviewer expands the map.

## Starting map (not exhaustive)

- Latest fork: Filter::is_scalar/is_fixed_value in expr logical_plan/plan.rs;
  positive native pure family, volatility and native Expr::apply_children;
  proof test and subquery.slt controls. Trace all children/scopes and sibling
  consumers: max_rows, correlated analyzer invariant, shared scalar-to-join
  filter/projection build_join, functional-dependency producers.
- Cumulative native: ScalarSubqueryExpr snapshot/metadata/reset, single_value
  native aggregate/partial-final state/registry/codec, FULL JOIN/grouping/checked
  arithmetic bounds, distributed scalar flag, native hash readiness/filter.
- Wyrd: oracle/mod.rs logical/physical/general mapping and planning; analytical
  planning_session; resources OracleSessionShape two passes; live leaf Pre/Post
  pushdown, first-poll snapshot, planned partition route ownership and fragment
  encode; dispatcher/peer fragment service/state instrumentation and auth.
- Pins/lock/ORACLE_DATAFUSION_REVISION move together; existing codec fence/tests.
- Remote live_subquery journeys: literal/two-fragment/two-footer evidence,
  six scalar refusals, join now includes grouped nonnullable-id correlated CASE
  positive with unmatched row. One public client reused for both join/CASE.
- Compact Rust query_coverage and Rust/Python/TS query_bifrost; MCP query tests;
  query fixture/write correlation, peer_cluster helper and mise registration.
- Prior stable findings 1–5 are hypotheses for source-validated closure, not
  assumed open/closed. Repeated manifestations sharing one cause consolidate.

## Roles and execution limits

Fresh independent behavior, invariants, standards, maintainer, system,
DataFusion and tenancy roles, then claim comparison, conditional focused
follow-up and always a fresh independent structured Ponytail validator.
Discovery must not read other current discovery reports or receive a preferred
verdict. Root orchestrates and packages validated findings only.

Candidate code stays immutable; write only assigned report in this new
directory. No build/heavy lane/benchmark/raised concurrency. Assess recorded
execution with stated limits; request root coordination before any necessary
focused execution so shared Cargo/test activity stays serialized. No commit,
merge, push or deploy. Report source HEAD movement immediately.

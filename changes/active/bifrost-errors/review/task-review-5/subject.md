# Round-5 immutable subject and navigation map

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Wyrd candidate: `8a54d81c346db464b624e81ebe8958c63bb7e228`.
- Integration baseline: `945a1548c47897dc94583169a6ba04163a73a749`.
- Original task base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Previous reviewed candidate: `17697dc8c2740213565e5f913cde1edfb0cb8ce5`.
- Full task diff: integration baseline → candidate, retaining cumulative Bifrost
  change while excluding the previously integrated main work. Original history
  and merged seam review remain in round 4; no new merge since that subject.
- DataFusion original: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- Previous fork: `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`.
- Current fork: `8c3f18e9806ae23e8840a710af775c058dfad66a`.
- Fork worktree: `/home/thorrester/Documents/GitHub/datafusion-scalar-snapshot`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

## Authority and evidence

Approved revision-1 spec: `changes/active/bifrost-errors/spec.md`.
No original task existed. Read all prior verdicts/ledgers/remediations in
`review/task-review-{1,2,3,4-resumed}` as cumulative authority and hypotheses,
including the latest `bifrost-errors-R4-require-invariant-filter-values.md`.
Its appended evidence table contains exact commands/results; assess actual
assertions and source, not the user's completion summary. Owner additions:
compact tests for common joins/subqueries, including joined CTE; no benchmarks;
safe proof construction at its native owner, no consumer query-shape hacks;
preserve established valid queries. Source is read-only; only review reports
in this new directory may be written.

Read AGENTS, agent-rules, spec-driven-development, maintainer-style, reference
router and applicable Wyrd design/doctrine/Bifrost/testing/domain authority.
CodeGraph preceded Wyrd navigation at this round; each reviewer must expand
the map, follow repository rules, and independently inspect current source.

## Starting map, not an exhaustive boundary

- Latest fork delta: expr logical_plan/plan.rs Filter::is_scalar and fixed-value
  proof/tests; subquery.slt negative/positive controls. Siblings: max_rows,
  correlated scalar invariant checker, both ScalarSubqueryToJoin build_join
  callers, native constant/volatility contracts and unique-key dependencies.
- Cumulative fork: ScalarSubqueryExpr snapshot; single_value aggregate and
  native codec; FULL JOIN/grouping-set/overflow row bounds; distributed session
  flag and native hash join readiness/dynamic-filter producer.
- Wyrd oracle/mod.rs logical/physical/execution error mapping and planning;
  analytical.rs planning_session; resources.rs OracleSessionShape two passes;
  live.rs LiveScribeExec Pre/Post pushdown, resolved route ownership and
  LiveFragmentRead first-poll; dispatcher and peer fragment service/state.
- Cargo pins/lock and oracle/codec.rs ORACLE_DATAFUSION_REVISION move together.
- Existing peer_network/live_subquery.rs remote scalar literal/footer, join and
  scalar refusal journeys (latest adds grouped count/abs filter/projection).
- Query fixture/write helpers and peer_cluster; Rust query_bifrost and compact
  query_coverage, Python/TS query_bifrost, MCP query assertions; mise task wiring.
- Prior findings: FIND-1 selective predicate transfer, FIND-2 broad workload,
  FIND-3 scalar cardinality/proof bypass, FIND-4 rustdoc. Source-validate closure
  of each; do not infer disposition from prior verdict or green tests.

## Review topology and verification limits

Fresh separate behavior, invariant, standards, maintainer, system, DataFusion
and tenancy discovery reviewers. No discovery reviewer receives another's
current conclusions or intended verdict. Compare claims, use focused follow-up
only if needed, then fresh independent structured Ponytail validation even if
the proposed union is empty.

No benchmark, synthetic load, increased concurrency or heavy verification
lanes during review. Recorded implementation execution evidence may be assessed
with explicit limits; do not claim a rerun. Request orchestration before any
necessary focused execution to serialize shared Cargo lanes. Do not edit
candidate code, merge, commit or push. Report HEAD movement immediately.

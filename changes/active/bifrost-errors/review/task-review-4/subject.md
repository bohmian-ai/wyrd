# Immutable task review subject, round 4

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Original Wyrd base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Wyrd candidate: `7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`.
- Previous candidate: `dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`.
- Spec: `changes/active/bifrost-errors/spec.md`, approved revision 1.
- Original task: none; specification and owner directions govern.
- Prior verdicts/ledgers and remediation tasks: `review/task-review-1`, `-2`, `-3` under this change.
- Current remediation: `review/task-review-3/bifrost-errors-R3-make-scalar-cardinality-bounds-sound.md`, including committed evidence.
- DataFusion original: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- DataFusion candidate: `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`.
- DataFusion previous: `f059acf6c58b2c5fd4bc36b3358e8a56ac283476`.
- Cached fork: `/home/thorrester/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/b2d37bc`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`, `/home/thorrester/.cargo/git/checkouts/datafusion-distributed-615cde49e12a7772/8fac2a9`.

## Starting navigation map, not complete coverage

- Fork latest delta: `expr/src/logical_plan/plan.rs::LogicalPlan::max_rows` and direct bound test; `sqllogictest/test_files/subquery.slt` flag-off expansion refusal cases.
- Bound siblings: complete closed LogicalPlan match, `Filter::is_scalar`, functional dependencies/unique-key propagation, scalar invariant validator and shared scalar-to-join shortcut.
- Cumulative fork: scalar snapshot; native single_value aggregate accumulator/state/merge/registry; Filter/Projection scalar rewrite and aggregate shortcut; native partial/final distribution/codec and SQL/proto tests/docs.
- Wyrd latest: Cargo pins/lock and codec revision; existing remote_two_row_scalar_subquery_is_refused extended with FULL/GROUPING input over registered outer table.
- Wyrd cumulative: Oracle logical/physical/runtime error mapper, authorization and CTE discovery; resources shared two-pass session; analytical planning configuration and stage collection; live Pre/Post filtering, lazy first-poll snapshot, planned route assignment and signed fragment encoding.
- Test-support footer/plan observations and peer service/dispatcher/PeerCluster consumers; shared query fixture/RunId writer support; compact mixed-state Rust corpus and retained Rust/Python/TS/MCP journeys; mise registrations.

## Authority, evidence and scope

Read AGENTS, agent-rules, spec-driven-development, maintainer-style and full applicable authorities selected by reference router. Use full wyrd-task-review. Audit cumulative Wyrd original-to-candidate and pinned fork original-to-candidate, with latest diffs only locating closure. No implementation summary or another discovery conclusion establishes acceptance. Prior FIND-1 through FIND-4 are closure hypotheses; source/tests must support them.

Owner steering requires sound native singleton proof: skip cardinality validation only with a guarantee; otherwise runtime validation. Correct owning producer, not query-shape guards/new framework. This does not promise no future faulty code can ever be written. Prior validation rejected broad range-filter optimization; do not reintroduce it as required scope. Runtime opaque 500 refusal is preserved unless approved planning-error authority actually requires otherwise.

Committed exact-command evidence is in the R3 task (and earlier tasks); reconcile its credibility against actual assertions. Cumulative diff-check is clean at review start. No benchmarks, synthetic load, raised concurrency or heavy Cargo/environment lanes in discovery. Read-only candidate: write only your assigned report, preserve others and all prior artifacts. The orchestrator owns any focused execution and final comparison/validation. Later change review owns the full integrated capability gate.

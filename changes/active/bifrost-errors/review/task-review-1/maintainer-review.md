# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8`
- Candidate: `f1e1be815`
- Specification: `changes/active/bifrost-errors/spec.md`
- Diff: `git diff 526355fb8..f1e1be815`

The candidate was still at `f1e1be815069b23fa9920a61c1198524ecaad514` when this report was written.

## Changed-surface coverage

| Surface | Symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Oracle logical/physical planning errors | `Oracle::plan_logical`, the retained physical build, `caller_planning_fault`, `map_sql_planning_error`, `map_query_planning_error`, `map_datafusion_error`, `datafusion_resources_exhausted`, the error unit test, and the Rust/Python/TypeScript/MCP journeys | The public mapping belongs in Oracle: Wyrd owns the stable error catalog while DataFusion supplies the caller-facing planner diagnostic. Separating logical-plan messages from redacted physical-plan refusals is clear. The new wrapper traversal unnecessarily duplicates DataFusion's installed root-cause mechanism; see `MAINT-001`. |
| Live Scribe filter pushdown | `LiveScribeExec::handle_child_pushdown_result`, `reads_scalar_subquery`, live fragment construction/serialization, DataFusion's filter-pushdown conjunction splitting, `ScalarSubqueryExpr`/`ScalarSubqueryExec`, and the distributed planner's session configuration | The check belongs beside `LiveScribeExec`: this Wyrd-owned leaf copies a leader expression into a separately serialized Scribe fragment, and only this owner can decide not to copy an expression whose result container remains on the leader. Moving this guard into Apache DataFusion core would impose Wyrd's fragment boundary on ordinary in-process providers; moving it into `datafusion-distributed` would miss Wyrd's independent live-fragment RPC. The concrete downcast is a small, readable use of DataFusion's public physical-expression type, not a new abstraction. A focused owner-level proof of the performance-preserving behavior is missing; see `MAINT-002`. |
| Required SDK and MCP journeys | Rust `query_bifrost.rs`, Python `test_query_bifrost.py`, TypeScript `query-bifrost.test.ts`, MCP `query.rs`, shared `prepare_oracle_query_fixture`, and `BifrostWriter::enqueue` callers | The fixtures use the existing real-server journeys and test scalar, `IN`, `EXISTS`, managed-`run_id` correlation, planner suggestions, type/ambiguity diagnostics, and unauthorized-table redaction. Names and assertions describe caller outcomes. The optional `RunId` added to the test writer is narrow and all Bifrost-writer callers were traced. |
| Added analytical qualification harness | `support_workload.rs`, its in-module oracle tests, Rust published/live/mixed journeys, the remote-Scribe peer journey, module exports, and the promoted `datafusion` dependency | This is a separate broad qualification suite rather than focused proof for the approved behavior. It materially enlarges the test API and gated runtime; see `MAINT-003`. |
| Declarations, docs, and manifests | `wyrd-testing/Cargo.toml`, module registrations, Rust item/field rustdoc, Python typing, and TypeScript declarations | No public SDK contract changed, so generated declaration parity is unaffected. Materially changed Rust items have substantive rustdoc. The `datafusion` dependency promotion exists only to support the broad harness in `MAINT-003`. |

## Material findings

### MAINT-001 — Use DataFusion's root-cause traversal instead of maintaining a second wrapper list

- Location: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3739-3760`
- Governing principle: Ponytail's installed-dependency step and the maintainer guide's requirement that helpers isolate a real rule rather than duplicate dependency behavior.
- Evidence: `caller_planning_fault` manually unwraps `Context`, `Diagnostic`, `Shared`, and the first member of `Collection`. The pinned DataFusion version already exposes `DataFusionError::find_root`, whose implementation follows `Error::source`, handles those same wrappers (including the first collected error), crosses other DataFusion source-bearing variants, and returns the deepest typed `DataFusionError`. The repository already uses `find_root` for DataFusion classification in `crates/vala/vala-bifrost-redux/src/resources.rs`.
- Concrete maintenance cost: every new or newly reachable DataFusion wrapper requires Wyrd's private list and its tests to change in lockstep. A wrapper omitted here turns a repairable caller planning failure into a generic 500 even though DataFusion can already find the typed root. The helper name hides that it is a partial reimplementation of an installed API.
- Smallest correction: classify `error.find_root()` as SQL/Plan/Schema/NotImplemented after the existing resource-exhaustion precedence check, and delete the manual loop. Keep the SQL-versus-physical public mapping methods where they are. Retain a small test covering the public classification; wrapper mechanics belong to DataFusion's own tests.

### MAINT-002 — The performance-preserving half of the live-subquery fix has no focused proof

- Location: `crates/vala/vala-bifrost-redux/src/oracle/live.rs:395-423` and `:509-523`
- Governing principle: maintainer-style tests must prove the caller-relevant outcome, and `architecture/references/languages/testing-workflows.md` reserves a focused unit/integration test for a seam that a full journey would make noisy.
- Evidence: the implementation filters each pushed conjunct independently, which is the right shape: the `ScalarSubqueryExpr` conjunct remains on the leader while unrelated static predicates can still populate `LiveScribeExec::predicate` and prune routes. The new journeys prove correct rows and that a remote fragment ran, but none observes the resulting leaf predicate or route selection. They would still pass if a later edit retained every filter on the leader, preserving correctness while silently removing live-tail filtering and time-partition pruning.
- Concrete maintenance cost: a maintainer cannot change this DataFusion-coupled hook or upgrade the fork with direct evidence that the exclusion stays narrow. The user's explicit regression boundary is that subquery support must not cost ordinary predicate pushdown performance.
- Smallest correction: add one module-local plan/pushdown test beside `LiveScribeExec` that supplies a scalar-subquery conjunct and an independent ordinary/time conjunct, then asserts the rebuilt leaf excludes the scalar-dependent expression and retains the independent predicate (and, where the fixture permits, its route selection). Reuse existing live fixtures and DataFusion expression constructors; do not add another journey or harness.

### MAINT-003 — Delete the unrelated general analytical qualification framework

- Location: `crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:1-687`, `sdks/wyrd-sdk-rust/tests/integration/analytical_correctness.rs:1-133`, and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs:1-107`
- Governing principle: the task-review Ponytail ladder requires deleting machinery the approved behavior does not need; `architecture/references/languages/testing-workflows.md` says not to broaden tests into slow integration gates unless the touched behavior requires it.
- Evidence: the candidate already has focused real-server scalar/`IN`/`EXISTS` journeys on Rust, Python, TypeScript, and MCP, including managed-`run_id` correlation and live rows. In addition, it adds a public 687-line generated support-domain model, three tables, ten queries covering CTEs, windows, aggregates, null membership, orphan rows, and unrelated joins, plus three SDK deployment states and a multi-pod run. That is roughly forty additional Bifrost queries over hundreds of generated rows and promotes `datafusion` from a dev dependency to a normal `wyrd-testing` dependency solely for the oracle.
- Concrete maintenance cost: future schema, Arrow display, DataFusion, publication, peer-cluster, or test-lane changes must maintain a second analytical product suite whose failures do not isolate REQ-002 or REQ-006. Its broad public fixture API and deterministic generator create more code to review than the production fix, while the focused required journeys already close the user-facing contract.
- Smallest correction: remove the support-workload module, its `datafusion` dependency promotion, the published/live/mixed analytical-correctness module, and the remote support-workload peer module. Keep the focused language/MCP journeys and add only the owner-level pushdown proof from `MAINT-002`. If broad analytical qualification is independently desired, specify and land it as its own change with an owned performance/qualification lane.

## Maintainer preferences (non-blocking)

- `reads_scalar_subquery` could import `ScalarSubqueryExpr` at module scope to shorten the downcast, but the existing qualified path makes the dependency explicit and has no concrete maintenance cost.
- The scalar-subquery exclusion is correctly local to the Wyrd live-fragment boundary. No DataFusion or `datafusion-distributed` fork change is required for this task based on the inspected call path.

## Verification limits

No verification results were supplied. I inspected the complete base-to-candidate diff, relevant production owners, dependency implementations from the pinned DataFusion and `datafusion-distributed` checkouts, callers, fixtures, and tests. I ran `cargo metadata --no-deps --format-version 1` only; I did not run format, lints, unit tests, or the Postgres-backed language/MCP/peer journeys.

## Overall result

**FAIL**

`MAINT-001` leaves a dependency-coupled duplicate error walker, `MAINT-002` leaves the promised no-performance-regression boundary without focused proof, and `MAINT-003` adds substantial unrelated qualification machinery. The production ownership and method shape are otherwise clear, and the Wyrd-local scalar-expression guard is the correct placement.

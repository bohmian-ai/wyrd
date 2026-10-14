# Structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md`, revision 1
- Original task: no task artifact exists. The approved specification and the
  owner's direction to require correct Bifrost error mapping plus principled,
  performance-safe subquery support at the correct Wyrd/DataFusion boundary are
  the task authority.
- DataFusion fork: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
- `datafusion-distributed` fork:
  `8fac2a978bdcf8e06457238ebfe9c7f35023c123`

The candidate remained at the supplied commit before this report was written.
Review artifacts are untracked and outside the immutable candidate.

## Validation coverage

This pass read the complete cumulative diff, all discovery and follow-up
reports, the applicable repository and Bifrost authorities, the changed
production owners, their callers and tests, and the pinned source of both
DataFusion forks. It independently traced:

- query text through table discovery, tenant-bound cut resolution,
  authorization, logical planning, the stable `BifrostError` catalog, HTTP,
  SDKs, and MCP;
- scalar-subquery state from DataFusion's physical planner through
  `ScalarSubqueryExec`, the shared `ScalarSubqueryResults` writer,
  `ScalarSubqueryExpr`, physical filter pushdown, `LiveScribeExec`, fragment
  serialization, Scribe execution, and the retained leader `FilterExec`;
- sibling published and hot inputs, ordinary and dynamic filter handling,
  time-route pruning, local and remote Scribe dispatch, and the existing
  physical-expression snapshot mechanism; and
- every new support-workload consumer, module export, test registration, and
  manifest change.

## Proposed-finding dispositions

| Proposal | Disposition | Source validation |
|---|---|---|
| `BHV-001` | **REJECTED** | Missing tables are resolved by Oracle's planner before DataFusion session registration and return the existing actionable `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` 4xx with the requested FQN (`oracle/mod.rs:1685-1709`, `catalog/bifrost_catalog.rs:366-397`, `wyrd-spec/src/vala/error.rs:423-434`). The specification does not require a DataFusion-origin message or replace that stable code. The MCP journey exercises this case. Making DataFusion diagnose an absent provider would weaken authorization-before-diagnostics and would require a specification revision; it is not required by the approved text. |
| `BHV-002` | **REVISED** into `FIND-bifrost-errors-1` | `FilterExec` merely offers split conjuncts and `LiveScribeExec` reports all of them unsupported, so no generic DataFusion pushdown incorrectly removes the leader filter. Wyrd owns the separate Scribe-fragment copy. The concrete-type exclusion is still incomplete because it discards a predicate whose runtime value is available before the live input executes. DataFusion owns the generic state-to-static-expression operation; Wyrd owns invoking it at its fragment boundary. |
| `MAINT-001` | **REJECTED** | `DataFusionError::find_root` follows every `Error::source` edge, including `ArrowError`, Parquet, object-store, IO, `ExecutionJoin`, and `External` (`datafusion/common/src/error.rs:455-488,502-537`). Reusing it at this disclosure boundary could expose a nested `Plan` or `SchemaError` from an internal dependency. `caller_planning_fault` intentionally traverses only logical-planning wrappers before accepting a caller-fault leaf, while resource classification correctly uses the broader root walk elsewhere. |
| `MAINT-002` | **REVISED** into `FIND-bifrost-errors-1` | The candidate lacks direct proof that independent predicates remain pushed, but that assertion alone would still allow every scalar-dependent query to scan the full live tail. It is retained as one part of the focused performance closure proof for the shared defect. |
| `MAINT-003` | **CONFIRMED** as `FIND-bifrost-errors-2` | The 927-line general qualification framework and dependency promotion are used only by their new broad journeys. The focused Rust, Python, TypeScript, and MCP journeys already cover every specified query and error form. The broad framework does not observe predicate materialization, remote rows, bytes, or route pruning. |
| `SYS-001` | **REVISED** into `FIND-bifrost-errors-1` | The full-tail transfer and enlarged failure window are reachable and source-proven. The correction is narrowed to DataFusion scalar snapshotting plus Wyrd's execution-time fragment boundary; it does not require new Wyrd scalar evaluation or a general distributed-planner change. |
| `DF-001` | **REVISED** into `FIND-bifrost-errors-1` | Its Wyrd/DataFusion diagnosis is confirmed. Its requested `datafusion-distributed` stage-planner repair is removed: that fork has a related ignored test, but Wyrd builds this private Scribe fragment outside that planner and after the scalar result is ready. No reviewed Wyrd path makes the separate stage-cut defect part of REQ-006. |
| `FUP-001` | **CONFIRMED** as `FIND-bifrost-errors-1` | DataFusion's existing `PhysicalExpr::snapshot` contract explicitly covers converting runtime state before remote serialization; `ScalarSubqueryExpr` is the missing implementation. `ScalarSubqueryExec` populates the slot before it invokes `LiveScribeExec::execute`, which is the exact Wyrd-owned point where the recursive snapshot, route selection, and fragment encoding can use a literal. |
| `FUP-002` | **CONFIRMED** as `FIND-bifrost-errors-2` | Every consumer and manifest change exists solely for the broad support workload. Deleting them preserves all required focused cross-surface journeys and leaves room for the one narrow remote-fragment performance proof actually needed. |

## Final deduplicated finding ledger

### FIND-bifrost-errors-1 — CONFIRMED — INCORRECT: scalar support trades the decode failure for an unbounded live-tail transfer

- **Discovery sources:** `BHV-002`, `MAINT-002`, `SYS-001`, `DF-001`,
  `FUP-001`.
- **Violated obligation:** REQ-006 together with the owner's explicit
  requirement that subquery support be principled and introduce no avoidable
  query-path performance regression.
- **Locations:**
  `crates/vala/vala-bifrost-redux/src/oracle/live.rs:401-422,473-505,509-523,617-652`;
  DataFusion
  `datafusion/physical-expr/src/scalar_subquery.rs:134-181`,
  `datafusion/physical-plan/src/scalar_subquery.rs:214-240,372-390`, and
  `datafusion/physical-expr-common/src/physical_expr.rs:390-448,943-985` at
  `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- **Producer-to-consumer evidence:** DataFusion creates one
  `ScalarSubqueryResults` container shared by `ScalarSubqueryExec` and every
  dependent `ScalarSubqueryExpr`. `execute_subqueries` is the only production
  writer and fills every slot before `ScalarSubqueryExec` invokes the main
  input's `execute`. The candidate removes each scalar-dependent conjunct when
  `LiveScribeExec` records its fragment predicate. `fragment` can serialize
  only that recorded predicate, so Scribe never sees the resolved scalar. The
  original `FilterExec` remains on Oracle because the leaf reports every
  offered filter unsupported, preserving returned rows while forcing Oracle to
  filter all surviving live rows after transport.
- **Sibling and owner evidence:** Published and hot sources continue through
  their existing DataFusion/Iceberg paths. Independent top-level conjuncts are
  split before the leaf and may still prune, while a selective scalar-dependent
  conjunct never does. Dynamic filters remain a separate, intentionally
  leader-owned case. `PhysicalExpr::snapshot` is the installed generic contract
  for converting runtime-bound expressions before remote serialization, but
  `ScalarSubqueryExpr` does not implement it. Wyrd owns the separate
  Scribe-fragment boundary and route selection; DataFusion owns conversion of
  its runtime expression into a serializable built-in expression.
- **Observable consequence:** A query such as
  `WHERE id = (SELECT max(id) ...)` scans and transfers every live row surviving
  unrelated predicates, then filters at Oracle. Network bytes, Scribe work,
  Arrow decode, leader CPU, and the interval exposed to terminal mid-stream
  Scribe loss grow with the full live tail rather than the qualifying rows.
- **Decision-complete smallest correction:** In the owned DataFusion fork,
  implement `ScalarSubqueryExpr`'s existing snapshot contract so a populated
  slot becomes a metadata-preserving `Literal` and an unpopulated slot returns
  a typed error. In Wyrd, retain scalar-dependent conjuncts on
  `LiveScribeExec`; when `execute` is invoked after `ScalarSubqueryExec` has
  completed, recursively snapshot the predicate, use that static expression
  for runtime route selection and fragment encoding, and keep the leader
  `FilterExec` as the authoritative correctness check. Delete
  `reads_scalar_subquery`; do not add a Wyrd scalar evaluator, expression
  allowlist, second planner, or `datafusion-distributed` repair. Bump Wyrd's
  pinned DataFusion revision through the repository's normal dependency path.
- **Focused closure proof:** Add DataFusion unit coverage for pending and ready
  scalar snapshots, recursive replacement inside a compound predicate, and
  preserved field metadata. Add one narrow Wyrd remote-live test with a large
  outer live set and a highly selective scalar predicate; prove the dispatched
  fragment contains a materialized literal and no `ScalarSubqueryExpr`, and
  assert bounded Scribe output rows or transport bytes. Include one independent
  ordinary/time conjunct to prove it still reaches the fragment and retains
  route pruning. Retain the existing cross-surface row-equivalence journeys.

### FIND-bifrost-errors-2 — CONFIRMED — DRIFT: the general analytical qualification framework is not needed by this change

- **Discovery sources:** `MAINT-003`, `FUP-002`.
- **Violated obligation:** the task-review Ponytail requirement to retain only
  machinery needed for the approved error-mapping and live-subquery behavior.
- **Locations:**
  `crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:1-687`,
  `sdks/wyrd-sdk-rust/tests/integration/analytical_correctness.rs:1-133`,
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs:1-107`,
  their three module registrations, and
  `crates/wyrd/wyrd-testing/Cargo.toml:57-64,133-140`.
- **Reachability and reuse evidence:** The new public support model is consumed
  only by the new Rust published/live/mixed suite and remote-peer suite. It adds
  three schemas, generated data, a plain-DataFusion oracle, and ten queries over
  CTEs, windows, unrelated aggregates and joins, NULL membership, and orphan
  data. The focused Rust, Python, TypeScript, and MCP journeys already exercise
  the required scalar, `IN`, correlated `EXISTS`, managed-`run_id`, planner
  detail, and authorization-redaction behavior through the real server. The
  broad suite neither detects the scalar predicate's removal from the fragment
  nor bounds remote work.
- **Observable consequence:** The candidate promotes DataFusion from a dev
  dependency to a normal `wyrd-testing` dependency and adds roughly forty
  gated integration queries whose failures may come from unrelated
  publication, peer, SQL, Arrow-formatting, window, or fixture behavior. This
  increases the compile/test cone and creates a public test API larger than the
  production fix without proving either remaining risk.
- **Decision-complete smallest correction:** Delete the support-workload
  module, its two broad journey modules, their registrations, and the manifest
  dependency promotion. Keep the focused cross-surface journeys and add only
  the narrow remote-fragment performance proof required by
  `FIND-bifrost-errors-1`. The `RunId` support added to the existing query
  fixture remains required by the focused correlated journey.
- **Focused closure proof:** Run the existing focused Rust, Python, TypeScript,
  and MCP query journeys plus the new narrow remote-fragment test. Confirm the
  `wyrd-testing` manifest returns DataFusion to dev-dependency scope and no
  support-workload symbol or registration remains.

## Specification decision

Neither retained correction changes approved behavior, a public contract,
security or tenancy semantics, persistent data, or service ownership. The
DataFusion snapshot implementation extends an existing generic runtime-state
contract; Wyrd invokes it at its existing private fragment boundary. Therefore
the validated ledger requires bounded remediation and does **not** require
`SPEC_REVISION_REQUIRED`.

## Validation completeness and verification limits

The independently validated ledger contains two material findings. No other
proposal survives source validation. The supplied verification update reports
that `mise run verify:bifrost` has passed formatting, workspace-hack,
all-feature Clippy, dependency checks, tenant isolation, Rust and TypeScript
units, and `vala-bifrost-redux` integration, and that the focused mapper
nextest passed. Server and language journeys were still running with no failure
reported when this validation began. Those green checks establish broad build
and regression evidence but cannot close `FIND-bifrost-errors-1`, because the
candidate contains no assertion for materialized remote scalar pushdown or
bounded remote work.

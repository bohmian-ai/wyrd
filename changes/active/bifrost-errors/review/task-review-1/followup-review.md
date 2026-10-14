# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md`, revision 1
- Original task: no task artifact exists; the approved specification and the
  owner's direction to require correct error mapping plus principled,
  performance-safe subquery support are the task authority.
- DataFusion fork: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
- `datafusion-distributed` fork:
  `8fac2a978bdcf8e06457238ebfe9c7f35023c123`

The candidate was still the stated commit before this report was written.

## Source paths inspected

- Candidate production paths:
  `crates/vala/vala-bifrost-redux/src/oracle/live.rs`,
  `crates/vala/vala-bifrost-redux/src/oracle/mod.rs`,
  `crates/vala/vala-bifrost-redux/src/oracle/exec.rs`,
  `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs`, and
  `crates/wyrd-spec/src/vala/error.rs`.
- Candidate focused and broad proof paths:
  `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs`,
  `sdks/wyrd-sdk-python/tests/integration/test_query_bifrost.py`,
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts`,
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query.rs`,
  `crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs`,
  `sdks/wyrd-sdk-rust/tests/integration/analytical_correctness.rs`, and
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs`.
- DataFusion fork:
  `datafusion/physical-plan/src/filter.rs`,
  `datafusion/physical-plan/src/filter_pushdown.rs`,
  `datafusion/physical-plan/src/scalar_subquery.rs`,
  `datafusion/physical-expr/src/scalar_subquery.rs`,
  `datafusion/physical-expr-common/src/physical_expr.rs`,
  `datafusion/expr/src/physical_planning_context.rs`, and
  `datafusion/proto/src/physical_plan/{from_proto.rs,mod.rs}`.
- `datafusion-distributed` fork:
  `src/metrics/task_metrics_collector.rs` and the stage-boundary planning and
  encoding paths under `src/distributed_planner` and `src/coordinator`.
- Every discovery report in
  `changes/active/bifrost-errors/review/task-review-1/`.

## A. Ownership, execution timing, and performance

### What the current plan does

For the uncorrelated scalar case, DataFusion creates one
`ScalarSubqueryExec` around the main input. Its stream waits for all subqueries,
stores each `ScalarValue` in `ScalarSubqueryResults`, and only then calls
`input.execute(...)` (`physical-plan/src/scalar_subquery.rs:205-235`). The
outer filter's `ScalarSubqueryExpr` reads that shared slot
(`physical-expr/src/scalar_subquery.rs:134-148`). A serialized expression
cannot be decoded by itself because the proto decoder obtains its result
container only from the scope of a surrounding `ScalarSubqueryExec`
(`proto/src/physical_plan/from_proto.rs:350-361`).

DataFusion's pre-pushdown pass splits a `FilterExec` predicate into conjuncts
and offers them to its child (`physical-plan/src/filter.rs:718-735`). Offering
a filter is not a claim that the child handled it. `LiveScribeExec` returns
`all_unsupported`, so the authoritative `FilterExec` remains on the leader,
while the leaf uses the callback as the existing hook for optionally copying
safe conjuncts into its separately built Scribe fragment
(`oracle/live.rs:382-423`). This private fragment is not a child stage produced
by DataFusion or `datafusion-distributed`; Wyrd constructs and serializes it in
`LiveScribeExec::fragment` (`oracle/live.rs:279-344`). Therefore BHV-002's claim
that a change to `FilterExec` alone is the complete correction is rejected.
Withholding the scalar conjunct in DataFusion would avoid the decode error, but
it would also leave the same full live-tail transfer and would not satisfy the
owner's performance requirement.

The candidate filters out every conjunct containing `ScalarSubqueryExpr`
during planning (`oracle/live.rs:401-406`, `509-523`). This restores result
correctness because the leader filter remains, and independent top-level
conjuncts are still considered separately. It also guarantees that the scalar
conjunct can never prune live routes or filter at Scribe. A selective predicate
such as `id = (SELECT max(id) ...)` consequently transfers every outer live row
that survives other conjuncts. The cross-surface journeys prove returned rows;
the ordinary-filter unit test proves a wholly static predicate ships. None
proves that a scalar-dependent predicate is materialized and shipped, or bounds
remote rows/bytes. SYS-001 and DF-001 are correct on this observable
consequence.

### Exact ownership boundary

The two owners have complementary responsibilities:

1. DataFusion owns the runtime state and the generic state-to-static-expression
   conversion. `PhysicalExpr::snapshot` expressly exists to simplify dynamic
   expressions before a plan portion is serialized for remote execution
   (`physical-expr-common/src/physical_expr.rs:395-448`), and
   `snapshot_physical_expr` already performs the recursive rebuild
   (`physical_expr.rs:956-986`). `ScalarSubqueryExpr` currently implements no
   snapshot even though, after its slot is populated, its value is a normal
   serializable literal. The missing `ScalarSubqueryExpr` snapshot belongs in
   the DataFusion fork, including pending/ready behavior and preservation of
   its field metadata through `Literal::new_with_metadata`.
2. Wyrd owns the private fragment boundary and route pruning. By the time
   `LiveScribeExec::execute` is called under `ScalarSubqueryExec`, the slot is
   populated. Wyrd should retain the offered predicate, invoke DataFusion's
   recursive snapshot at that execution/fragment boundary, select routes from
   the static result, and encode that result above the remote placeholder. The
   advertised partition count may remain fixed: the constructor already
   documents that later pruning may leave every route, and therefore some or
   all output partitions, empty (`oracle/live.rs:237-264`). No Wyrd-owned scalar
   evaluator or concrete expression allowlist is needed.

This is smaller and more general than either disputed one-sided correction.
Changing only DataFusion filter pushdown fixes correctness but not performance;
keeping only Wyrd's concrete downcast fixes correctness by disabling the
optimization. The current candidate therefore violates the human
no-performance-impact/no-support-hack criterion for scalar-dependent live
predicates.

The ignored `datafusion-distributed` test records a real but separate stage
scope defect: its planner may encode a stage containing `ScalarSubqueryExpr`
without the owning `ScalarSubqueryExec`
(`task_metrics_collector.rs:221-228`). Wyrd's Scribe fragment is built outside
that planner and after the scalar result is available. The candidate does not
change the generic stage-cutting algorithm, and the inspected Wyrd live/mixed
topology supplied no path requiring that separate planning-time defect to be
fixed for REQ-006. It should not be bundled into this task's correction.

### Proposed finding FUP-001

- **Classification:** INCORRECT
- **Violated obligation:** REQ-006 plus the owner's explicit requirement that
  live subquery support be principled and not introduce a query-path
  performance regression.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/live.rs:401-421` and
  `:509-523`; DataFusion
  `physical-expr/src/scalar_subquery.rs:134-181` and
  `physical-expr-common/src/physical_expr.rs:395-448,956-986`.
- **Evidence:** the candidate deletes scalar-dependent conjuncts before storing
  the fragment predicate. `LiveScribeExec::fragment` can serialize only that
  stored predicate, although `ScalarSubqueryExec` populates the value before
  the live input executes. The generic snapshot contract exists for this
  remote-serialization case but the scalar expression does not implement it.
- **Observable consequence:** selective scalar predicates scan and transfer
  the complete outer live tail and filter it on Oracle, increasing Scribe work,
  network bytes, leader decode/filter work, and exposure to terminal mid-stream
  Scribe loss in proportion to live data volume.
- **Testable correction:** implement ready-state scalar snapshotting in the
  owned DataFusion fork; at Wyrd's live-fragment execution boundary, use the
  recursive generic snapshot for fragment encoding and route selection while
  retaining the leader filter. Prove a ready scalar becomes a serializable
  literal, a pending scalar fails typed, a selective remote-live scalar filter
  reaches Scribe and bounds transferred/output rows or equivalent fragment
  evidence, and an independent static/time conjunct remains pushed and usable
  for pruning.

## B. Unknown-table behavior

BHV-001 is rejected as an implementation finding. REQ-002 requires a 4xx
`WyrdError` whose detail carries the planning message for caller SQL faults; it
does not require every fault to use `QueryInvalidSql` or require DataFusion to
originate the text. Oracle must resolve and authorize the complete table set
before registering providers and invoking DataFusion. An absent table therefore
fails during `pin_cut` (`oracle/mod.rs:1685-1709` and
`catalog/bifrost_catalog.rs:366-397`) as the existing typed
`WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`, whose detail is
`bifrost table not found: <caller-supplied fqn>`
(`wyrd-spec/src/vala/error.rs:422-435`). It is a safe, actionable 4xx planning
diagnostic and is shared by HTTP, all SDKs, and MCP. The MCP journey pins that
stable code (`wyrd-mcp/tests/bifrost/mcp/query.rs:298-321`).

AC-001 requires a base-failing/candidate-passing journey for each requirement,
not for every parenthetical example. REQ-002 has such cross-surface journeys
for the changed logical-planner mapping. The unchanged unknown-table case does
not establish a missing implementation.

If the intended contract were specifically to register a synthetic missing
provider and expose DataFusion's unknown-table wording, or to replace the
catalog 404 with `QueryInvalidSql`, that would conflict with the approved
authorization-before-diagnostics boundary and the established public error
code. That alternate outcome would require a specification revision. The
current approved observable wording does not require it.

## C. Maintainer claims

### MAINT-001 — rejected

`caller_planning_fault` is not equivalent to `DataFusionError::find_root`.
`find_root` follows every `Error::source` edge and retains the deepest
DataFusion error, including through Arrow, Parquet, object-store, IO,
`External`, and execution-join wrappers
(`datafusion/common/src/error.rs:452-498,517-545`). That breadth is appropriate
for resource classification, which is how Wyrd already uses it. It is not an
appropriate disclosure boundary: an internal dependency error can contain a
nested `Plan` or `SchemaError`, and `find_root` would relabel and expose it as
caller SQL. The candidate deliberately traverses only the wrappers produced by
logical planning (`Context`, `Diagnostic`, `Shared`, and the first
`Collection` member), then accepts a typed caller-fault leaf
(`oracle/mod.rs:3739-3760`). This small allowlist enforces the spec's 5xx and
non-disclosure requirement and should not be replaced by the broader helper.

### MAINT-002 — revised and consolidated into FUP-001

The claim that no focused test proves an independent conjunct remains eligible
is true. The source currently filters each already-split conjunct independently,
so this is a proof gap rather than a second implementation cause. Its proposed
test is also insufficient for the owner's full performance requirement because
it would pass while the scalar-dependent conjunct still causes a full remote
scan. FUP-001 retains the useful independent-conjunct assertion and adds the
necessary proof that the resolved scalar predicate itself reaches the remote
fragment and bounds work.

### MAINT-003 — confirmed

The candidate's focused Rust, Python, TypeScript, and MCP journeys already
exercise the required scalar, `IN`, correlated `EXISTS`, and `run_id` forms over
live rows through the real server. The additional 687-line public support model
adds three schemas, generated data, ten general analytical queries, a
plain-DataFusion oracle, published/live/mixed SDK deployments, and a multi-pod
run. It tests CTEs, windows, unrelated aggregates and joins, NULL membership,
and orphan data outside REQ-002 and REQ-006. It also promotes `datafusion` from
a dev dependency to a normal `wyrd-testing` dependency solely for that library
fixture. Despite its breadth, it does not observe scalar fragment pushdown,
route pruning, transferred rows, or bytes, so it does not close FUP-001.

### Proposed finding FUP-002

- **Classification:** DRIFT
- **Violated obligation:** the task-review Ponytail requirement to keep only
  machinery needed for the approved behavior and the specification's bounded
  live-subquery/error scope.
- **Location:**
  `crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:1-687`,
  `sdks/wyrd-sdk-rust/tests/integration/analytical_correctness.rs:1-133`,
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/support_workload.rs:1-107`,
  their module registrations, and the `wyrd-testing` manifest dependency
  promotion.
- **Evidence:** the focused cross-surface journeys already prove the specified
  user behavior; the general suite covers many unrelated SQL features and
  deployment states but no performance boundary introduced by the production
  fix.
- **Observable consequence:** the change adds a large public test API, a wider
  normal dependency cone, and roughly forty additional integration queries
  whose failures can arise from unrelated publication, peer, SQL, Arrow
  display, or schema changes without isolating either approved requirement.
- **Testable correction:** remove the general support-workload framework,
  analytical SDK module, remote support-workload module, registrations, and
  dependency promotion. Retain the focused language/MCP journeys and add only
  the narrow remote-fragment/performance proof required by FUP-001.

## Resolution

**RESOLVED.** The ownership and performance conflict resolves to a shared
DataFusion-snapshot/Wyrd-fragment correction, not either proposed one-sided
fix. The unknown-table proposal is rejected under the current observable
contract. MAINT-001 is rejected, MAINT-002 is revised into FUP-001's closure
proof, and MAINT-003 is confirmed as FUP-002.

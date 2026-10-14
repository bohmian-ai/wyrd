# Maintainer review — round 3

## Subject and authority

Reviewed Wyrd `526355fb8c58dae6d67d516f98dddcd5c02b0e79` →
`dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`, and its pinned DataFusion
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`. The original implementation
task is absent; the approved spec, owner-directed common-query coverage, both
prior remediation packets and committed verification evidence are the inputs.
The latest remediation diff guided navigation; the cumulative runtime and test
changes remain in scope.

Authorities: `AGENTS.md`, `architecture/agent-rules.md`, the reference router,
`languages/maintainer-style.md`, `languages/spec-driven-development.md`,
`languages/rust-core.md`, `languages/testing-workflows.md`,
`domain/datafusion.md`, `domain/olap-serving.md`, and the relevant Oracle and
public client sections of `bifrost-design.md` and `wyrd-design.md`.
Used CodeGraph before Wyrd symbol/caller inspection. The fork's native
AggregateUDF and optimizer conventions govern its implementation shape;
Wyrd-only documentation rules are not mechanically imposed on dependency code.

## Changed-surface coverage

| Surface | Source/caller/test coverage | Assessment |
|---|---|---|
| Fork scalar cardinality | Full `SingleValue`, accumulator methods, registration/export; full shared `ScalarSubqueryToJoin::rewrite` and `build_join`; `ReplaceDistinctWithAggregate` registry lookup precedent; native scalar execution behavior; SQL and proto additions | Cohesive native owner, no second evaluator, physical node, protocol or Wyrd workaround |
| Fork runtime scalar snapshot | `ScalarSubqueryExpr::snapshot`, metadata-preserving Literal construction; ready/pending/null/reset/compound tests | Implements existing expression contract; retained dynamic source remains available for reuse |
| Wyrd live predicate lifecycle | `LiveScribeExec` state, construction, pushdown hook, `resolved`, `fragment`, `execute`; `LiveFragmentRead::into_stream`; provider/exec caller and Analytical planning session | Snapshot and fragment work stay with their existing owners; partition ownership and first-poll timing are explained |
| Optimizer session shape | `OracleSessionShape::apply`, local/analytical session consumers | A shared two-pass configuration fixes rule interaction without another planner or per-query branch |
| Error projection | logical-plan caller and both mapper bodies, narrow wrapper traversal, physical/general error mapping; mapper unit and public query journeys | Exposed versus opaque diagnostics are explicit; no parallel public error metadata |
| Observations for remote proof | `NativeOutputTally::rows`; peer fragment executor and Scribe recording/accessors; `PeerCluster::scribe_fragment_output` | Extends existing footer/activity observations; plan storage and row counter are test-support gated |
| Fixtures and helpers | BifrostWriter enqueue/write and query fixture callers; PeerCluster layout registration and public-key provisioning; full remote journey module | Required run correlation and route layout use existing writer/cluster capabilities; no generated support workload or new public test oracle |
| Common SQL corpus | Full `query_coverage.rs`, integration module registration and owning mise selector | Small fixed rows and explicit expected answers make joins, CTE join, nulls, subqueries and window behavior understandable without a second DataFusion oracle |
| SDK and MCP journeys | Cumulative Rust/Python/TypeScript/MCP query changes and fixture reuse | Public surfaces compare exact results, preserve planner suggestions and demonstrate unauthorized diagnostic redaction |
| Pin and packet changes | Workspace patch set, lock/revision identity, cumulative diff and R1/R2 evidence | Native dependency universe remains coordinated; review evidence stays outside runtime source |

## Reuse and correction boundary

The cardinality fix belongs at the scalar rewrite producer. Both Filter and
Projection call the same `build_join`, which wraps only uncorrelated inputs
whose `max_rows()` cannot prove at most one row. The wrapper is an ordinary
no-group aggregate. `SingleValueAccumulator` owns exactly the state needed:
one typed value and a row count, updated across batches and merged partial
states. There is no new network representation or execution lifecycle.

The existing alternative `first_value` cannot enforce the requirement: it
selects one result rather than refusing a second row. Native physical scalar
execution already enforces cardinality, but the distributed configuration
deliberately selects the join rewrite. Therefore extending that rewrite using
the normal registered aggregate mechanism is justified. This is a better fit
than requiring the previous task's private cardinality node literally.

The registry lookup follows an existing optimizer pattern. If the aggregate
is unavailable, the rule leaves the scalar expression unrewritten rather than
generating successful set semantics. The default aggregate registration and
normal proto lookup make the same implementation available to worker decode.
SQL exposure is documented in the fork's aggregate reference and uses the same
registration path as other aggregates; it does not introduce a Wyrd public API
or configuration surface.

R1 retains the dynamic predicate on the original leaf and resolves a clone on
first poll. The original planning-selected route order fixes partition shares
before runtime narrowing. The `resolved` documentation explains why this order
prevents duplicated/omitted routes and why retaining the original expression
prevents stale re-execution. Accepting Post-phase dynamic filters extends the
existing hook rather than creating another pushdown owner.

## Prior maintainer finding and failure diagnosis

`FIND-bifrost-errors-4` is closed in source: the materially modified mapper
unit and MCP error journey now have substantive `# Panics` sections naming
fixture/assertion failures. The new peer refusal journey and corpus helpers
also document their errors or panics.

The committed unique-key failure diagnosis is consistent with the source:
the generic peer query helper bootstraps the fixed `live-subquery` service name
each time, while the new two-query refusal journey provisions once and reuses
one client for Filter and Projection. Its assertions still require the
execution-failure code; no retry, timeout, assertion or existing test was
weakened. The single-call scalar/join journeys retain their prior helper.

## Material findings

None. No maintainer correction is required by this review.

## Non-blocking calibration

The selective remote journey's prose says Scribe "reads and returns no more"
than the picked row. Its footer assertion bounds rows returned and its
fragment count establishes route pruning; these observations do not establish
that only one physical row was decoded within H2. Interpret that prose narrowly
when describing performance evidence. This wording does not invalidate the
required bounded-transfer proof or justify another remediation round.

## Verification limits and result

Inspected committed R2 Red/Green evidence, exact commands and assertions; did
not execute benchmarks, heavyweight Cargo lanes, or external-environment
journeys. No implementation source or prior review was changed. Runtime and
domain acceptance remain the independent reviewers' responsibility.

Overall: **PASS**.

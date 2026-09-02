# SCRAPPED — pre-revision-3 T2 route and activation plan

Non-authoritative history. Approved specification revision 3 removed this
task's public EXPLAIN, exact-routing-facts subsystem, automatic retry,
exhaustive operator matrix, and repeated replica-count obligations. Its active
successor is `../tasks/02-r3-activate-production-analytical-query.md`.

# T2 — Define, route, activate, and prove production Analytical execution

Status: Planned
Repository origin: github.com/bohmian-ai/wyrd
Repository revision: 3978aa7a2bbe49299fc8cd50e36c2e3c8e6e9d09
REPO_ROOT: $REPO_ROOT
PLAN_PATH: $PLAN_PATH
TASK_PATH: $TASK_PATH
Plan: ../plan.md
Milestone: M2
Requirements: R1, R2, R4, R5, R6, R7, R8
Decisions: D2, D3, D5, D6
Depends on: T1

## 2026-09-01 peer-network test amendment

T2 depends specifically on the implemented and FINAL-reviewed T1 peer
remediation, not the original T1 candidate. That remediation adds
BifrostProcessCluster and Tier-2 separate-process peer-network cases inside the
existing Oracle journey target.

T2 MUST reuse that harness for its held-out Tier-1 public journeys. At minimum,
the public Analytical journey starts distinct leader Oracle, follower Oracle,
and Scribe child processes, then proves:

~~~text
real Rust/HTTP/generated-gRPC client
  -> leader child public listener
  -> production Analytical route selection
  -> leader child private peer listener/client
  -> follower child private peer listener
  -> follower operators and exchange
  -> leader result and terminal evidence
  -> client
~~~

The journey MUST assert distinct PIDs, exact PostgreSQL membership
addresses/fences, accepted peer certificate fingerprints, remote physical
operators, and post-drain zero ownership. Existing same-process
WyrdTestCluster tests remain supporting regressions and cannot substitute for
this public multi-process journey.

Oracle replicas remain horizontally interchangeable. Interactive and
Analytical are query execution choices, not pod roles. The Tier-1 journeys MUST
run the same Oracle child configuration at replica counts one, two, three, and
six. The one-Oracle case proves local execution, listener/membership/readiness,
and cleanup. At replica counts two and above, the journeys send public queries
to more than one Oracle endpoint and prove that each Oracle can coordinate
Interactive and Analytical requests and follow work for another coordinator.
T2 MUST NOT add query-type-specific deployments, labels, Services, membership
roles, or peer transports.

Do not add a Cargo target or `mise` task. The canonical command remains
`mise run test:bifrost:journey:oracle`; focused iteration may use an exact
nextest name selector through `mise exec` with the repository-managed
PostgreSQL fixture already running.

No task in this plan launches Kubernetes. The separate-process test proves the
server and peer-network behavior Kubernetes schedules into pods; checked-in
deployment manifests statically prove the port, headless-Service,
advertisement, secret, and NetworkPolicy configuration.

## Objective

Move the unshipped contract directly to its final `QueryExecutionPath` shape,
add conservative optimized-plan routing and path-keyed admission, activate T1's
proven handle in Oracle, and prove Interactive and Analytical behavior through
real Rust, HTTP, and generated-gRPC clients with terminal-safe post-drain
evidence.

Required skill: `wyrd-implement`.

## Context

`Oracle::query_sql` currently prepares a cut and a `PlannedSqlCut`, derives
`QueryClass`, then enters the existing query lifecycle. `wyrd-spec::vala::api`
owns request/terminal and generated contract inputs; server query and gRPC
modules own public transport; `vala-sql` owns durable policy/query schema;
`BifrostProcessCluster` owns the required separate-process public Analytical
journey, while `WyrdTestCluster` retains fast same-process regressions. T1
supplies an inactive `AnalyticalExecutionHandle`, stage authority,
resource/lifecycle ownership, and post-drain evidence. T2 is the sole
activation consumer.

T1 also supplies the extended production `OracleTelemetry` lifecycle and its
existing `WyrdTestCluster` production-capture seam. T2 must use that same owner
for both Interactive and activated Analytical routing; it must not introduce a
path-specific test harness.

Wyrd/Bifrost authority fixes one raw-SQL contract, Oracle ownership,
WAL-before-rows audit, a real exchange, Interactive default, no migration for
the unshipped schema, and first-class public journeys. OLAP/DataFusion fix
exact-fact routing; security/errors fix auth and stable failures; analytical
reliability fixes admission, cancellation, and readiness.

## Required changes

1. Define the final public request, EXPLAIN, execution-path, route/fallback,
   stage-DAG, terminal-evidence, and stable-error contracts at their sources;
   regenerate OpenAPI/schema/protobuf/descriptors.
2. Change existing unshipped SQL definitions, queries, checks, and fixtures
   directly so admission and recorded execution use `QueryExecutionPath`.
3. Add synchronous `OracleExecutionRouter` and `OracleRoutingFacts` over the
   optimized plan, pinned cut, exact post-pruning facts, and prospective
   Interactive grant.
4. Require distributed physical planning to produce a real streamed exchange
   before Analytical admission. Permit only the two pre-selection fallbacks.
5. After exchange validation and irreversible Analytical selection, allocate
   one internal `DataFusionQueryId` and bind it to the existing
   client-visible `PublicQueryId`; allocate none on Interactive fallback.
6. Inject and activate T1's handle, make Oracle readiness depend on it, and
   preserve one resource/audit/cancellation/shutdown hierarchy.
7. Add authenticated HTTP and unary generated-gRPC EXPLAIN through the same
   Oracle planning owner with no row IO, resource acquisition, running-query
   registration, or successful read acceptance.
8. Install production journeys before activation, then make the smallest
   routing/lifecycle changes needed to turn them green.
9. Instrument every activated Interactive and Analytical hot path through
   `OracleTelemetry`, and make Rust/HTTP/generated-gRPC journeys assert the same
   production traces, structured logs, and bounded metrics used for operations.

## Non-goals

- Python, TypeScript, or MCP projections; T3 owns them.
- SQL-syntax routing, mutable statistics, sampling, client hints, persisted
  NDV, or broad new optimizer policy.
- A compatibility route, migration, backfill, dual read/write, or old-node
  negotiation.
- A second telemetry owner, test-only signal path, or client-visible debugging
  contract.

## Allowed scope

- `crates/wyrd-spec/src/vala/**` and derive-backed error owners.
- `crates/vala/vala-bifrost-redux/src/oracle/**`, `src/resources.rs`, and
  nearest tests.
- `crates/vala/vala-sql/**` and current schema-source/fixture owners.
- `crates/vala/vala-sdk/src/query.rs` for Rust client projection.
- `crates/wyrd/wyrd-server/src/query/**`, `src/grpc/query.rs`, boot/readiness,
  and generated source inputs.
- `crates/wyrd/wyrd-tonic/**` source protos/generator inputs.
- `crates/wyrd/wyrd-testing/src/bifrost/**` and Oracle/server journey modules.
- Generated files only through canonical regeneration commands.

## Prohibited changes

- Public caller fields selecting path, class, stage, or physical plan.
- `QueryClass` controlling capacity, routing, storage, or durable path truth.
- Operational engine unavailability represented as a successful fallback.
- Evidence truncation, partial stream success, a second audit event for a
  retry, or stage-owned read audit.
- A new service/listener/resource/storage/readiness/audit owner.
- Hand-edited generated artifacts.
- High-cardinality identities or SQL in metric labels, and journey assertions
  satisfied by emitting surrogate test telemetry.

## Target paths and symbols

- Existing `crates/wyrd-spec/src/vala/api.rs` or its current split:
  `BifrostQueryExplainRequest`, `BifrostQueryExplainResponse`,
  `QueryExecutionPath`, `QueryRouteReason`, `QueryFallbackReason`,
  `BifrostQueryStage`, `QueryExecutionEvidence`, and terminal evidence.
- New `crates/vala/vala-bifrost-redux/src/oracle/router.rs`:
  `OracleExecutionRouter`, `OracleRoutingFacts`, `OracleRouteDecision`.
- Existing `oracle/planner.rs`: optimized logical plan and exact provider facts;
  distributed physical-plan candidate/exchange proof.
- Existing `oracle/mod.rs`: `Oracle::query_sql`, new `Oracle::explain_sql`, and
  active `AnalyticalExecutionHandle` field.
- Existing `oracle/mod.rs`: the same `OracleTelemetry` owner for routing,
  admission, planning, Interactive execution, Analytical stages, retry,
  cancellation, terminal settlement, and resource release.
- Existing `oracle/admission.rs` and `resources.rs`: path-keyed admission and
  protected floor under one aggregate root.
- Existing `wyrd-server/src/query/service.rs` and `grpc/query.rs`: public query
  and EXPLAIN handlers.
- New journey files
  `tests/bifrost/oracle/analytical_public.rs` and
  `tests/bifrost/server/query_explain.rs`, included by current target roots.
- Existing `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` and its
  child-node support binary: reuse without creating another cluster harness.

## Required types and interfaces

```rust
pub enum QueryExecutionPath { Interactive, Analytical }

pub struct OracleRoutingFacts {
    pub selected_rows: Option<ExactRowCount>,
    pub grouping_state_bytes: Option<u64>,
    pub contains_live_tail: bool,
    pub schema_exact: bool,
}

pub enum OracleRouteDecision {
    Interactive { reason: QueryRouteReason },
    AnalyticalCandidate { reason: QueryRouteReason },
}

impl OracleExecutionRouter {
    pub fn route(
        &self,
        optimized: &LogicalPlan,
        facts: &OracleRoutingFacts,
        interactive_grant_bytes: u64,
    ) -> OracleRouteDecision;
}

impl Oracle {
    pub async fn explain_sql(
        &self,
        context: AuthorizedQueryContext,
        request: BifrostQueryExplainRequest,
    ) -> Result<BifrostQueryExplainResponse, BifrostError>;
}
```

Public route reasons are `NoExchangeRequired`,
`CorrectnessRequiresExchange`,
`EstimatedHashStateExceedsInteractiveGrant`, `EstimateUnavailable`, and
`UnsupportedAnalyticalShape`. Pre-admission fallbacks are exactly
`DistributedPlanningFailed` and `DistributedCandidateHasNoExchange`.
Operational unavailability is not a fallback.

The public request, running-query, cancellation, audit, error, and terminal
surfaces project only `PublicQueryId`. `DataFusionQueryId` remains private to
Oracle and its workers. Oracle's active-query owner maps one public lifecycle
to its one selected DataFusion graph so public cancellation drains the complete
private graph without exposing a client control.

Analytical v1 supports multi-input joins, nonempty partitioned windows,
fixed-width COUNT/SUM/MIN/MAX grouping whose checked conservative state bound
strictly exceeds the Interactive grant, supported decorrelated subquery plans,
and representative cross-source deduplicating sets. Strings, nested/variable
keys, unsupported dictionaries/states/UDAFs, unresolved widths, schema
conflict, live tail, overflow, or still-correlated/unsupported shapes remain
Interactive.

Stage DAG validation enforces fixed count/byte ceilings, checked IDs, sorted
unique parents, no missing/self/cyclic edges, and exactly one head. Invalid or
oversized Analytical projection remains Interactive before admission; never
truncate.

## Implementation guidance

**Test-driven design is required for this task.**

For every public journey, take a scoped baseline from the installed production
telemetry capture before the request and assert deltas afterward. Interactive
and Analytical must share metric families and lifecycle semantics where the
operation is common, with closed path/route/outcome/reason labels where they
differ. Preserve public/private query and stage correlation in scrubbed traces
and structured logs, never in metric labels. Every active gauge returns to its
baseline on all terminal paths.

Write the public journey expectations and pure router tests first. Keep routing
and arithmetic synchronous and IO-free. Keep planning and provider facts in the
existing Oracle planner; do not create a second optimizer. Make route selection
and path admission two explicit steps so only planning/no-exchange can fall back.

Calculate fixed-width grouping state with checked arithmetic from exact selected
rows and pinned Arrow field widths. The physical distributed candidate must be
inspected for the exchange node produced by the pinned engine, not inferred from
stage count or SQL shape.

EXPLAIN shares auth, authorization, cut, planning, deadline, and route logic but
branches before resource acquisition, WAL acceptance, execution, or row IO.
Readiness becomes false when the handle, peer ingress, stage authority, or
supervisor is unhealthy. Existing accepted work drains under its owner.

## Control flow and pseudocode

```text
Oracle::query_sql
  -> validate/auth/authz/deadline
  -> pin cut and optimize once
  -> derive exact OracleRoutingFacts
  -> OracleExecutionRouter::route
  -> Interactive: admit path and run existing plan
  -> AnalyticalCandidate:
       build distributed physical plan
       if planning failed: record pre-admission fallback; Interactive
       if no real exchange: record pre-admission fallback; Interactive
       else select Analytical permanently
       allocate one DataFusionQueryId and bind it to PublicQueryId
       admit path under aggregate root/floor
       on refusal or engine loss: stable terminal error
       fsync one logical read acceptance
       AnalyticalExecutionHandle::execute
  -> validate terminal + post-drain evidence
  -> release path admission and relay audit
```

EXPLAIN follows the same flow only through route/distributed-plan validation and
returns the bounded DAG without admission, execution, rows, running state, or a
successful read acceptance.

## Failure and edge cases

- Missing or inexact facts, checked overflow, unsupported type/state, live tail,
  schema conflict, still-correlated subquery, or invalid DAG selects
  Interactive with a typed reason.
- Planning failure/no exchange falls back before Analytical admission and is
  visible in EXPLAIN/terminal evidence.
- Missing/unhealthy handle removes readiness; unexpected supervisor death does
  the same. Loss after selection returns a derived stable service error.
- Post-selection resource refusal is
  `WYRD_VALA_429_BIFROST_QUERY_RESOURCES_EXHAUSTED`; counters remain unchanged
  on refusal and no Interactive fallback occurs.
- Cross-tenant authority rejects before planning. Retry never duplicates audit
  or rows. Retry preserves both query identities while incrementing only the
  typed attempt. Deadline/cancellation/slow consumer settle every descendant.
- Invalid/missing terminal is never success on HTTP or generated gRPC.

## Acceptance criteria

- AC1: final Rust/JSON/protobuf/OpenAPI/schema contracts agree and contain no
  compatibility or migration behavior.
- AC2: route arithmetic and supported/unsupported shapes are deterministic from
  optimized plan plus exact immutable facts.
- AC3: only candidates with a real streamed exchange become Analytical;
  fallback exists only before selection.
- AC4: path-keyed admission preserves one aggregate cap and the non-borrowable
  Interactive floor; `QueryClass` is telemetry-only.
- AC5: query audit, retry, cancellation, deadline, backpressure, terminal, and
  six-node cleanup remain correct on both paths; the public identity resolves
  exactly one private DataFusion graph and the private identity is never
  projected to clients.
- AC6: authenticated EXPLAIN agrees with terminal execution evidence and has no
  query resource, row IO, running-query, or successful-read-audit side effect.
- AC7: real Rust/HTTP/generated-gRPC journeys enter a leader child and prove
  actual Analytical peer execution in follower child processes, plus
  Interactive behavior, denial, failure, fallback, and cleanup.
- AC8: the same production `OracleTelemetry` owner covers Interactive and
  activated Analytical hot paths, and Rust/HTTP/generated-gRPC journeys prove
  expected production metric deltas, correlated trace/log lifecycle, and
  balanced gauges for success, refusal, fallback, failure, cancellation, and
  shutdown.

## Required tests

| Slice | Exact test and file | Tier / production seam | First RED | Smallest GREEN owner | Required mutation RED | Exact focused command |
|---|---|---|---|---|---|---|
| Contract | `query_execution_path_contract_round_trips_all_generated_surfaces` in `wyrd-spec/src/vala/api.rs` | unit/codegen; source DTO → serde/schema/proto conversion | final types/conversions absent | spec types + source proto conversions | swap/omit a route reason or deadline bound; round-trip fails | `mise run codegen:check` |
| Router | `router_uses_exact_fixed_width_state_and_rejects_unknown_or_overflowing_facts` in `oracle/router.rs` | unit; optimized plan/facts → route | router absent | synchronous router and checked state calculator | treat `None`/overflow as Analytical; case fails | `mise run test:bifrost` |
| Supported shapes | `router_requires_exchange_for_join_window_subquery_and_distinct_set_candidates` in `oracle/router.rs` | unit; logical candidate → physical exchange proof | supported decision/exchange check absent | route classification + distributed planner inspection | accept a no-exchange candidate; test fails | `mise run test:bifrost` |
| Admission | `path_admission_preserves_interactive_floor_and_refusal_is_counter_neutral` in `oracle/admission.rs` | unit/integration; real capacity lock | path-keyed state absent | path admission under aggregate owner | let Analytical consume floor or increment on refusal; test fails | `mise run test:bifrost` |
| Production telemetry | `public_paths_emit_shared_production_oracle_telemetry` in `tests/bifrost/oracle/analytical_public.rs` | journey; Interactive + Analytical raw SQL → installed production recorder/capture | production signal coverage is incomplete across both paths | extend existing `OracleTelemetry` at routing/execution owners | add a path-local/test-only recorder, omit a hot-path terminal, or leak an active gauge; production capture assertion fails | `mise run test:bifrost:journey:oracle` |
| EXPLAIN | `public_explain_authenticates_before_planning_and_has_no_query_side_effects` in `tests/bifrost/server/query_explain.rs` | journey; public HTTP/gRPC → Oracle planner | route absent | server handler + `Oracle::explain_sql` | acquire a slot or append acceptance; inspection fails | `mise run test:bifrost:journey:server` |
| Public paths | `public_raw_sql_routes_interactive_and_executes_analytical_follower_operators` in `tests/bifrost/oracle/analytical_public.rs` | Tier-1 journey; real Rust/HTTP client → leader child → private peer network → follower child | Analytical journey fails before activation | inject handle and activate route | use one PID, force leader-only operator, or report zero exchange; evidence assertion fails | `mise run test:bifrost:journey:oracle` |
| Fallback/refusal | `public_analytical_fallback_is_preselection_and_postselection_failures_are_terminal` in the same file | journey; planning/no-exchange and engine/resource faults | final semantics absent | route transition + stable errors | silently fall back after selection; path/terminal assertion fails | `mise run test:bifrost:journey:oracle` |
| Retry/terminal | `public_analytical_retry_cancel_deadline_and_slow_consumer_are_terminal_safe` in the same file | journey; public stream and fault controller | activation/lifecycle absent | Oracle lifecycle using T1 supervisor | allow post-egress retry or duplicate rows/audit; test fails | `mise run test:bifrost:journey:oracle` |
| Generated gRPC | `generated_grpc_query_and_explain_prove_both_paths_denial_and_cleanup` in `tests/bifrost/server/query_explain.rs` | journey; actual tonic listener | EXPLAIN/path projections absent | gRPC adapters/conversions | bypass auth or omit terminal evidence; journey fails | `mise run test:bifrost:journey:server` |

The public Analytical journey must independently assert follower join,
partial/final aggregate, window, supported subquery result operator,
repartition/dedup operator, positive exchange, follower pushdown, qualified
spill, exact result/terminal, and six zero-owner snapshots. Route labels,
EXPLAIN, planned stages, or remote scans alone fail the test.
It MUST also prove that leader and follower are different child PIDs and that
the follower accepted the exact fenced private address selected from
PostgreSQL membership.

## Required features

- Current default/test-support/server feature unions selected by the canonical
  Bifrost, SQL, codegen, proto, Oracle, and server journey tasks.
- No new Cargo feature.
- Generated schemas/protobuf/stubs are regenerated from their owning sources.

## Focused verification

Run sequentially:

```bash
mise run check
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run test:e2e
mise run codegen:check
mise run check:proto-drift
mise run check:client-tier
mise run fmt
mise run lints
```

## Commands explicitly excluded

- `mise run gate`; the integrated plan closeout owns the broad release
  aggregate.
- Python, TypeScript, and MCP lanes; T3 owns them.
- Bare whole-crate Cargo commands and zero-match filters.
- Direct plan/provider injection as journey acceptance evidence.

## Stop and escalate if

Stop if correct routing needs mutable/inexact facts, SQL syntax, or a client
hint; schema correctness appears to require migration/compatibility despite the
unshipped status; activation needs a second owner/listener; a post-selection
failure must fall back; tenant/audit identity becomes ambiguous; a new public
error/field beyond this packet is required; or actual follower and cleanup
evidence cannot be made deterministic.

## Completion evidence

Return AC1-AC8 mapping, source/generated contract diff, schema change proof,
named RED/GREEN/mutation results, exact route/admission matrices, audit/readiness
and six-node inspections, scoped production telemetry deltas and correlated
trace/log evidence, all sequential verification outputs, features, final diff
audit, and any bounded private-path adaptation. Record the commit that T3 may
consume; it may not start from a partial unintegrated contract.

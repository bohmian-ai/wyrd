---
task_id: BIFROST-R3-T2-PRODUCTION-ACTIVATION
title: Activate conservative Analytical routing through the one public query contract
kind: reconcile
status: proposed
approved_spec: SPEC-bifrost-distributed-analytics-engine
approved_revision: 3
parent_task: pre-r3 T2 route and activation plan
frozen_candidate: f1ac4cb01
dependencies: [BIFROST-R3-T1-INACTIVE-CLOSEOUT]
mapped_requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-011]
mapped_invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
mapped_acceptance: [AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
---

# Superseded task — Revision 3 production activation

## Disposition and outcome

Replace the pre-revision-3 Task 2. Retain its one-public-contract activation,
path admission, terminal evidence, telemetry, and real public journeys. Scrap
its public EXPLAIN API, specialized exact-routing-facts subsystem, automatic
retry, public stage DAG, persisted execution-path migration, exhaustive
operator family, and repeated one/two/three/six-replica journeys.

After this task, the existing authenticated raw-SQL operation routes
conservatively to Interactive or to Task 1's settled Analytical handle. A
caller cannot select the path. Analytical becomes irreversible only after a
supported distributed physical plan with a real exchange is built and path
admission succeeds. Every later failure is terminal and never reruns locally.

Required execution skill: `$wyrd-implement`.

## Owners and scope

- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs` remains the cohesive
  prepare, route, admit, execute, audit, cancel, and terminal owner.
- The existing Oracle planner/classification modules own candidate
  classification and distributed physical-plan inspection. Do not add an
  `OracleRoutingFacts` subsystem or second optimizer.
- `oracle/admission.rs` and `resources.rs` own path-keyed slots beneath one
  aggregate capacity root and the Interactive floor.
- `oracle/query_stream.rs` owns terminal-safe settlement for both paths.
- `crates/wyrd-spec/src/vala/api.rs`, existing proto sources,
  `wyrd-tonic` conversions, `wyrd-server` HTTP/gRPC query adapters, and
  `vala-sdk` own the language-neutral/Rust projection.
- Existing process-cluster and `WyrdTestServer` owners provide journeys. No new
  cluster, listener, scheduler, query-job service, or persisted SQL owner.

Python, TypeScript, and MCP are Task 3. Public EXPLAIN is out of scope.

## Design closure

### 1. Public contract

Keep `BifrostQueryRequest` unchanged: SQL, visibility, freshness, and optional
deadline only. Add the closed `QueryExecutionPath::{Interactive, Analytical}`
to the server-derived terminal contract and generated HTTP/gRPC/Rust surfaces.
Do not expose query class, worker set, graph, stage plan, or path hint.

Admission failure before a stream remains a structured request error. A
started stream ends with exactly one validated terminal containing its selected
path and success/failure outcome. Frames before a failed terminal are not a
successful result. The private DataFusion graph identity never leaves Oracle.

No durable Postgres schema or migration is required: selected path is
request-lifecycle state and terminal evidence, not a durable query-job record.

### 2. Candidate classification and supported plan

Reuse the `PlannedSqlCut` classification produced from the existing optimized
logical plan and immutable pinned cut. `QueryClass::Analytical` may nominate a
candidate but never selects the engine or controls path capacity.

Add one pure closed physical-plan validator beside the existing planner. It
accepts only the revision-3 baseline reachable in this task:

- filtered/projected scans over the pinned source cut;
- fixed-width grouped `COUNT`, `SUM`, `MIN`, and `MAX` states;
- multi-input equi-join;
- streamed exchange produced by the pinned distributed planner; and
- `SortExec` as the qualified spill-capable operator using the Task 1 scratch
  owner.

Unknown widths/states, live-tail dependence, schema conflict, overflow,
unsupported join/aggregate/operator shape, planning failure, or a plan without
a real network exchange remains or falls back to Interactive before selection
when Interactive supports it. Do not add exact selected-row, NDV, grouping-byte,
or stage-DAG contracts solely to route v1.

### 3. Selection and admission state machine

Oracle owns this closed transition:

```text
prepared
  -> Interactive candidate -> admit Interactive -> selected Interactive
  -> Analytical candidate -> build and validate distributed physical plan
       -> unsupported/planning failure/no exchange -> admit Interactive
       -> supported exchange -> admit Analytical
            -> refusal: structured resource error, no fallback
            -> success: selected Analytical, execute Task 1 handle
selected -> success | failed | cancelled | deadline | shutdown -> joined terminal
```

Selection is recorded only after path admission succeeds. Once selected, no
resource, peer, transport, protocol, execution, cancellation, deadline, or
cleanup failure can enter Interactive or start another attempt.

### 4. Path admission and resource protection

Extend the existing Oracle admission owner with distinct Interactive and
Analytical slot queues/counters under one checked total. Reserve the configured
Interactive floor before Analytical is admitted. Interactive may use otherwise
idle capacity; Analytical may never consume the floor, including during
failure cleanup or shutdown. A refused request changes no running counter or
another grant.

Each selected query passes its one admitted query envelope into the chosen
engine. Both paths use one query-local DataFusion memory pool, the shared
aggregate root, finite Wyrd-owned queue/fan-out/task/result bounds, one scratch
allocation, one deadline, and one cancellation tree. Do not precharge exchange
bytes or introduce per-path memory roots.

### 5. Audit, cancellation, cleanup, and internal callers

Reuse the existing one logical read-audit WAL acceptance. It must fsync before
rows on either path; distributed stages append no read audit. Preserve the
authenticated tenant, permission digest, pinned cut, sensitive-column decision,
request ID, and deadline when handing off to Analytical.

HTTP/gRPC stream drop, explicit lifecycle cancellation, deadline, peer loss,
server shutdown, and internal scheduled cancellation signal the same Oracle
cancellation tree and await Task 1 settlement before the terminal/release is
called clean. The server-owned drift/evaluation/scheduled caller invokes the
same Oracle query operation and receives the same terminal; it gains no private
path selector.

Use the existing `AppState::query_sql(AuthorizedQueryContext,
BifrostQueryRequest)` internal adapter for the scheduled/internal journey. It
already enters Gate/Oracle with the same typed request and is the production
server-owned seam; do not add a scheduler, alternate query method, or direct
Analytical handle call merely for Journey C.

### 6. Production telemetry and topology

Extend the existing `OracleTelemetry` owner only. Bounded labels cover path,
candidate/selection/fallback, admission/refusal, active queries, worker fan-out,
memory current/peak, exchange activity reported by the dependency, spill,
cancellation/deadline/failure category, cleanup, and terminal outcome. IDs and
SQL remain scrubbed trace/log fields, never metric labels.

Use the smallest topology for each journey:

- UI/Interactive behavior may run against one Oracle plus its required Scribe
  source.
- Public Analytical, scheduled Analytical, peer-loss, and pressure behavior use
  the three-Oracle plus one-Scribe process topology already justified in Task 1.
- Do not repeat the same public journey at other replica counts.

## Rejected alternatives

- A new routing-facts struct populated by a second statistics pipeline: not
  required by the approved conservative classifier.
- Public EXPLAIN or stage DAG: explicitly deferred.
- SQL hints, request fields, endpoints, or pod roles selecting the path:
  violates server authority.
- Fallback after Analytical admission/selection: risks duplicated or partial
  results.
- Persisting selected path in a new query-job schema: v1 streams synchronously
  and owns no durable query job.

## Ordered TDD scenarios

1. **Contract terminal path.** Generated Rust/JSON/protobuf/OpenAPI surfaces
   carry the closed selected path without adding any request selector.
2. **Conservative plan selection.** Existing classification nominates a
   candidate; only the supported physical baseline with a real exchange can
   proceed; all other safe cases run Interactive before selection.
3. **Interactive floor.** Analytical saturation leaves the floor serviceable;
   admission refusal is counter- and grant-neutral.
4. **Public path journeys.** A real public client receives exact Interactive
   and cross-process Analytical results and matching terminal paths.
5. **Pre-/post-selection failure.** Unsupported/no-exchange planning falls back
   before selection; resource/peer/transport failure after selection produces
   one failed Analytical terminal and no rerun.
6. **Scheduled caller and cancellation.** The server-owned caller uses the same
   operation; cancellation/deadline joins the complete graph.
7. **Audit and telemetry.** One read acceptance precedes rows, stages add none,
   all production signals are bounded, and every gauge returns to baseline.

For each scenario record the first behavioral RED, implement the smallest
GREEN in the named owner, then REFACTOR without widening the accepted plan.

## Named tests and exact focused commands

Pure plan and admission tests:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features "$WYRD_REDUX_TEST_FEATURES" \
  -E 'test(=oracle::tests::analytical_selection_requires_supported_physical_exchange)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features "$WYRD_REDUX_TEST_FEATURES" \
  -E 'test(=oracle::admission::tests::analytical_never_consumes_interactive_floor)'
```

Public Oracle journeys:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::public_raw_sql_selects_both_paths_and_never_falls_back_after_selection)'"
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::analytical_pressure_preserves_interactive_and_scheduled_queries_share_the_route)'"
```

Generated gRPC terminal-contract journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey -E 'test(=query::generated_grpc_query_projects_selected_path_failure_and_cleanup)'"
```

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run test:e2e
mise run codegen:check
mise run check:proto-drift
mise run check:client-tier
mise run check:bifrost-resource-governance
git diff --check
```

Do not run `mise run gate` for this task slice.

## Completion evidence

- Generated-source diff proving no request path selector or EXPLAIN surface.
- Supported/unsupported physical-plan table and real-exchange mutation RED.
- Path admission/floor/refusal state snapshots.
- Public UI, data-scientist, scheduled, failure, cancellation, and pressure
  journeys with exact terminals and zero ownership.
- One-audit acceptance evidence and no stage audit rows.
- Production telemetry deltas, bounded-label inspection, and balanced gauges.
- All focused/broader command results and final diff audit.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if production correctness requires a path hint,
public EXPLAIN, automatic retry, a new optimizer/statistics authority, broader
operator compatibility, a durable query job, or changed tenant/audit semantics.

## Authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/errors.md`
- `architecture/references/languages/testing-workflows.md`

---
id: BIFROST-R5-T04A-PRODUCTION-ACTIVATION
title: Complete production query activation through pre-distribution selection
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 5
depends_on: [BIFROST-R4-T03A-CONTENTION-QUALIFICATION]
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
parent_task: BIFROST-R4-T04-PRODUCTION-ACTIVATION
frozen_implementation_candidate: b1634b93ba79d5c8a9f8194ddcbb86523b40e824
blocker_evidence: 726c7ffa6e7209528654c3ea9a66b9ca8bbc55ad
planning_base: 5d3cc09f75d0d3583164baeb481179ed92358808
---

# Production query activation successor

## Outcome and value

The existing authenticated raw-SQL operation conservatively selects
Interactive or the qualified Analytical engine without caller hints. Oracle
first builds a locally executable physical plan and applies Task 2's existing
closed support predicate. Only a supported plan is rebuilt through the pinned
`datafusion-distributed` planner, and only a surviving real exchange selects
Analytical and transfers the query envelope into the Analytical graph. Every
pre-selection refusal retains the local Interactive route; every
post-selection failure is terminal.

Required execution skill: `$wyrd-implement`.

This single successor owns the remaining Task 04 outcome because selection,
query-resource transfer, running-query retirement, transport settlement, and
the public journeys mutate one shared query-lifecycle seam. Splitting them
would leave an independently unreviewable state in which the public terminal
could disagree with its cleanup owner.

## Reconciliation amendment

### Frozen and current state

- The original Task 04 remains unchanged at
  `tasks/04-production-query-activation.md`, including its partial execution
  history.
- Scenario 1 implementation is frozen at `b1634b93b`: the request stayed
  closed and every terminal now carries `QueryExecutionPath::{Interactive,
  Analytical}` through the language-neutral contract, proto conversion,
  Oracle stream, server transports, and `vala-sdk`.
- The implementation blocker is frozen at `726c7ffa6`: the original Oracle RED
  location could not compose an Oracle, and post-distribution validation could
  not safely reuse the resulting plan for local execution.
- Before writing this successor on 2026-09-03, HEAD was
  `5d3cc09f75d0d3583164baeb481179ed92358808` on `oracle-distributed`.
  `git status --porcelain=v2 --branch`, `git diff --stat`,
  `git diff --cached --stat`, and `git ls-files --others --exclude-standard`
  showed no tracked, staged, or untracked changes.
- During final verification, unrelated concurrent Task 05 replacement state
  appeared: tracked `tasks/05-mcp-query-projection.md` was deleted and
  `tasks/05-pre-mcp-buildout.md` was untracked. Both were excluded from this
  reconciliation and left untouched.
- Approved specification revision 5 is unchanged. It explicitly leaves Task
  04's Rust production activation outcome unchanged while correcting later MCP
  ownership.

### Retained

- Completed Scenario 1 and its passing evidence at `b1634b93b`.
- `OraclePlanner` classification, immutable `PlannedSqlCut`, authenticated
  query routes, audit-WAL-before-rows acceptance, terminal-safe stream,
  lifecycle cancellation service, and `AppState::query_sql`.
- Task 2's existing `splitter::validate_supported` contract, query envelope,
  graph lifecycle, participant reservation/publication barrier, and two-second
  pending TTL.
- Task 3's pinned `datafusion-distributed` physical baseline and real
  three-Oracle/one-Scribe qualification.
- Task 3A's class/tenant admission, aggregate capacity root, and protected
  Interactive floor.

### Invalidated

- The original Scenario 2 Oracle RED in a
  `vala-bifrost-redux --lib` test. Oracle composition belongs to the existing
  `wyrd-testing/tests/bifrost/oracle/` real-server topology.
- Validation after `datafusion-distributed` has already transformed the plan.
  Task 2's closed predicate validates the locally executable plan before the
  dependency introduces `DistributedExec`, `NetworkShuffleExec`, or
  `NetworkCoalesceExec`.
- Any fallback ordering that first transfers the query envelope into an
  Analytical graph and then tries to execute locally.
- Task 04's task-only requirement that unsupported/no-exchange routing perform
  one physical build. The selected implementation deliberately performs a
  local build and, only for a supported Analytical candidate, a second build
  through the pinned distributed planner.

### Unfinished

- Revised Scenarios 2–6 below: production selection, query-resource ownership,
  Rust stream settlement, public UI/distributed journeys, pre-selection stale
  replacement versus post-selection terminal failure, and audit/internal
  scheduled lifecycle coverage.

### Deleted or replaced

- Delete every instruction that retains a provisional Analytical graph for an
  unsupported, distributed-planning-failed, or no-exchange Interactive
  fallback.
- Delete every instruction that opens a `DistributedExec` or its network
  children through the local `stream_physical` path.
- Replace "lease Analytical before physical planning" with the exact
  plan/validate/replan/select/activate sequence in Scenario 2.
- Replace graph-held running-query ownership for Interactive fallbacks with the
  ordinary graphless Interactive stream owner. Only selected Analytical work
  transfers that owner to the supervisor graph.
- Replace original stale-retry cleanup of a provisional fallback graph with
  direct graphless Interactive settlement.

## Owners, scope, consumers, and non-goals

- `vala-bifrost-redux::Oracle` owns local planning, support validation,
  distributed planning, irreversible selection, execution, and settlement.
- `oracle/splitter.rs` remains the pure closed Task 2 support predicate. This
  task calls it earlier and does not change its accepted operator matrix.
- `oracle/analytical.rs` and `oracle/analytical_supervisor.rs` own the selected
  graph's query-resource transfer, runtime, exchange registry, participant
  cell, attempt, reservation lifecycle, and joined cleanup.
- `oracle/admission.rs` retains the one already-admitted local query envelope
  until selection. Candidate class admission continues to use Task 3A's
  qualified counters and floor; it is not a participant reservation and does
  not make the execution path Analytical.
- `oracle/query_stream.rs` owns the terminal and graphless Interactive running
  entry. `vala-sdk` owns reusable Rust-client decoding, cancellation, deadline,
  and settlement behavior.
- `wyrd-server` owns audit acceptance, HTTP/gRPC projection, internal scheduled
  invocation, listener composition, readiness, and shutdown.
- `wyrd-testing` owns the real-server and process journeys. The focused
  selection RED uses the existing `WyrdTestServer` composition through
  `WyrdTestCluster`; cross-process evidence continues to use the existing
  `BifrostProcessCluster`.

Do not modify Tasks 2 or 3. Cite and consume their current contracts. Do not
extend `validate_supported` to accept dependency-distributed nodes. Do not
route selected Analytical execution through `plan_distributed_split`. Do not
add a capturing `QueryPlanner`, second optimizer, planner fork, scheduler,
shuffle service, query job, path selector, EXPLAIN surface, listener, runtime,
resource registry, or automatic retry. The two physical builds are the selected
minimal implementation; revisit capture only if production profiling later
shows replanning is material.

## Ordered implementation scenarios

### Retained Scenario 1 — One request and one typed terminal path

**Behavior.** Preserve the completed language-neutral terminal path without a
request selector. Maps REQ-001, REQ-008, INV-001, INV-008, AC-006, AC-008.

**Evidence.** Commit `b1634b93b` is retained unchanged. Its focused test remains:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::tests::query_terminal_projects_path_without_request_selector)'
```

It is already GREEN. During every later scenario, rerun it after any terminal
contract or conversion edit. Do not manufacture another RED or rewrite the
completed implementation.

### Scenario 2 — Select before Analytical resource transfer

**Behavior.** Interactive remains the selected-path default. For an Analytical
candidate, Oracle builds one locally executable physical plan, validates it
with the existing pre-distribution `validate_supported`, and only then performs
the dependency-owned distributed build. Unsupported shape, distributed-planner
failure, or no surviving exchange executes the retained local plan as
Interactive without graph registration, query-resource transfer, follower
reservation, participant publication, or peer/source dispatch. A usable
`DistributedExec` makes selection irreversible; all activation, reservation,
dispatch, execution, and cleanup failures after that point are terminal. Maps
REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, INV-001, INV-002, INV-004,
INV-006, INV-007, INV-008, AC-002, AC-003, AC-004, AC-008.

**RED.** Add
`analytical_activation::analytical_selection_requires_supported_physical_exchange`
under `crates/wyrd/wyrd-testing/tests/bifrost/oracle/` and register that module
in the existing `oracle` target. Use `WyrdTestCluster`'s existing
`WyrdTestServer` nodes, catalog, Postgres, storage, authentication, Oracle
composition, and control probes; add no in-crate Oracle harness. Drive both
`Oracle::query_sql` and the forwarded
`Oracle::query_sql_with_participant_cut` leader entry.

Exercise a non-candidate, an unsupported but locally executable candidate, a
supported candidate whose pinned planner produces no exchange, a supported
exchange, and a distributed-planning refusal. Add one test-support-only,
one-shot `Oracle::fail_next_analytical_plan_for_test` flag checked immediately
before the second `create_physical_plan` call; it returns the same mapped
planning error as that call and neither replaces nor wraps the pinned planner.
Assert:

- non-candidate, unsupported, planner-refused, and no-exchange outcomes return
  an Interactive terminal and the exact local result;
- unsupported is rejected by the unchanged pre-distribution predicate, before
  the pinned distributed planner is invoked;
- supported candidates may perform the selected two builds; no assertion caps
  the attempt to one physical build;
- every fallback keeps the original admission, pinned cut, deadline, session
  runtime, and local physical plan and performs no graph registration,
  `take_query_resources`, attempt spawn, `ReserveNodeSlots`, participant
  publication, or Analytical peer/source IO before the retained local plan
  begins execution;
- a real distributed root alone selects Analytical, transfers one exact query
  envelope, publishes the frozen participant cut once immediately before first
  dispatch, and returns an Analytical terminal; and
- neither entry reauthorizes, re-pins, re-admits, allocates a second public
  query identity, or invokes `plan_distributed_split`.

Expected RED: current `admit_and_lease_attempt` moves resources into an
Analytical graph before planning, and `execute_analytical_session` validates no
local plan before the dependency transformation.

Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::analytical_selection_requires_supported_physical_exchange)' --run-ignored=all"
```

Task 2's genuinely crate-local predicate regression remains independently
owned and must stay green; do not relocate or rewrite it:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::supported_analytical_plan_accepts_only_the_v1_baseline)'
```

**GREEN.** Preserve the existing request-local `QueryExecutionPath::Interactive`
default and converge both production entries on `Oracle::run_sql_attempt`.
Derive the exact `AnalyticalAttemptContext` there from the existing
`PlannedSqlCut`, participant-cut attempt UUID, aggregate snapshot digest, and
permission digest, but treat it only as immutable candidate identity until
selection.

Change `admit_and_lease_attempt` to admit once, retain physical projections,
build only the ordinary query-owned local `SessionContext`, and register the
running query. It must not call `AnalyticalExecutionHandle::lease_session` or
move `OracleQueryResources`. Audit and drain remain before any rows or peer IO.
Provider registration remains on the local session so the first physical plan
is executable against the exact pinned cut.

Inside `execute_sql_cut`, build that local physical plan first and retain its
session, plan root, schema, and scan statistics. For a non-candidate, stream it
directly. For a candidate, call the existing
`splitter::validate_supported(local_plan, source_groups)` before constructing a
distributed session. Treat only its closed unsupported result as Interactive
fallback; preserve other security, tenant, catalog, corruption, or invariant
errors as failures.

For a supported local plan, have the existing `AnalyticalExecutionHandle`
compose one planning-only `SessionState` from
`SessionStateBuilder::new_from_existing(local_session.state())` plus the same
handlers, codec, frozen worker resolver, and `with_distributed_planner()`
configuration currently installed by `leader_session`. The planning session
does not need a graph-backed channel resolver: the pinned planner consults the
worker resolver while transforming, while channel resolution is read from the
execution `TaskContext`. The pinned planner performs its own second default
physical build and transformation. Planning performs no supervisor mutation,
resource move, participant publication, or peer IO. Do not capture the
planner's internal first plan and do not call `plan_distributed_split`.

If planning fails or `exec::is_distributed_plan` finds no surviving
`DistributedExec`, drop the planning session and candidate plan and stream the
retained local plan through the original local session. The admitted guard
still owns the query envelope; no cleanup detour through Analytical exists.

When and only when the distributed root survives, set the selected path to
Analytical and activate through the existing `AnalyticalExecutionHandle`
lease/session path. That path moves the existing `OracleQueryResources` from
the admitted guard exactly once, registers the one graph and exchange
registry, spawns attempt zero, starts the existing supervisor-owned lifecycle,
and builds the authoritative leader execution session with the signing channel
resolver. Execute the already-built distributed root with that session's
`TaskContext`; do not perform a third physical build and do not create a second
registry or lifecycle.

All validation and distributed planning occur before
`take_query_resources`. If graph registration refuses after the take, use its
existing returned-envelope error contract to restore the complete resources to
the same admitted permit before the common terminal release. If attempt or
lifecycle activation fails after graph registration, settle through the
existing graph guard/lifecycle rollback and return the failure terminal. No
error after the take may execute the local plan.

Call `publish_participants` only after activation and immediately before
`execute_stream` on the distributed root. Reservation refusal and every later
error use the selected Analytical terminal and joined graph settlement. Return
the actual selected path with the stream instead of hard-coding Interactive in
`AttemptOutput`.

**REFACTOR.** Candidate class, local plan support, distributed exchange
presence, selected path, and graph ownership remain distinct facts. Keep the
orchestration on `Oracle` and distributed composition on the existing
`AnalyticalExecutionHandle`; add no routing state machine or general planner
abstraction.

### Scenario 3 — Shared Rust stream settlement follows actual graph ownership

**Behavior.** HTTP and gRPC expose the exact absolute query deadline in
`x-wyrd-query-deadline-ms`; `vala-sdk` settles healthy incomplete streams by
cancelling once, draining the existing body under that deadline, and validating
the terminal. After decode or transport failure it preserves the original
error, cancels once, and polls the existing status route until not-found proves
cleanup. All Interactive streams, including unsupported and no-exchange
fallbacks, are graphless. Only selected Analytical streams transfer the running
entry to the graph and retire it after joined cleanup. Maps REQ-007, REQ-008,
REQ-010, INV-003, INV-004, INV-008, AC-004, AC-006.

**RED.** Add `query::tests::query_result_stream_settles_every_incomplete_exit_once`
in `vala-sdk`, covering normal terminal, healthy explicit close, bounded-result
overflow, protocol/decode failure, and transport failure. Assert checked
deadline parsing, at-most-once cancellation, healthy terminal drain, broken
body status polling to `WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND`, original-error
preservation, and no blocking or spawned settlement from raw `Drop`. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-sdk --lib -E 'test(=query::tests::query_result_stream_settles_every_incomplete_exit_once)'
```

Add the crate-local regression
`oracle::query_stream::tests::running_query_terminal_owner_drop_retires_untransferred_failure`:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::query_stream::tests::running_query_terminal_owner_drop_retires_untransferred_failure)'
```

Add the Oracle process journey
`analytical_activation::transport_drop_retains_running_status_until_cleanup_joins`.
Exercise direct Interactive, unsupported fallback, no-exchange fallback, and
selected Analytical over HTTP and gRPC. The three Interactive cases must show
no graph and synchronously drop local stream/admission owners. Pause cleanup
only for selected Analytical, then prove running status and graph ownership
remain until the lifecycle join completes. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::transport_drop_retains_running_status_until_cleanup_joins)' --run-ignored=all"
```

**GREEN.** Carry the participant cut's checked nonnegative Unix epoch
millisecond deadline on `OracleQueryStream` and project it unchanged through
HTTP, gRPC, private forwarding, and `vala-sdk`. A missing or invalid response
deadline is a protocol failure; add no request field, proto field, polling API,
or completed-query store.

Make `vala_sdk::QueryResultStream` retain its `QueryClient`, request ID,
deadline, body, and one settlement state. Healthy cancellation/overflow drains
that body and validates its terminal. A broken body is never read again; retain
the originating `ValaSdkError`, cancel once, poll status every fixed 100 ms
clipped to the absolute deadline, and report scrubbed unconfirmed-settlement
telemetry if proof remains unavailable.

Keep `RunningQueryTerminalOwner::Drop` as the pre-transfer fail-safe. Direct and
fallback Interactive attempts retain it in `OracleQueryStream`; normal terminal
finishes it after stream settlement, and drop cancels, destroys frame and
admission owners, then retires it failed. After Scenario 2 selects Analytical,
move it exactly once into `AnalyticalGraphState` before reservation or dispatch.
The graph lifecycle finishes it only after attempt, exchange, participant,
runtime, scratch, admission, and IPC cleanup join. Cleanup failure retains the
graph, owner, cancelling status, and readiness failure.

Reuse the original Task 04 one-shot `AnalyticalCleanupPause` design only for
the selected graph lifecycle, immediately before clean graph release. Reuse
the existing server schema stall for HTTP positioning and the first schema
frame for gRPC; do not add a second transport stall or generic gate. Project
the pause through the existing process-cluster control protocol. HTTP and gRPC
retain and poll the complete `OracleQueryStream`; adapters own no cleanup task.

**REFACTOR.** Retirement follows actual graph ownership, never candidate class
or terminal label. `vala-sdk` remains the sole external Rust settlement owner.

### Scenario 4 — Public UI and distributed Rust journeys

**Behavior.** A real UI query selects Interactive while Analytical capacity is
occupied; a supported data-scientist query selects Analytical without a hint.
Both return exact results and terminal paths, finite result pressure fails
safely, and every owner returns to baseline. Maps REQ-001, REQ-002, REQ-003,
REQ-006, REQ-008, REQ-009, REQ-011, AC-002, AC-003, AC-004, AC-005, AC-006,
AC-007.

**RED.** Add
`analytical_activation::public_query_selects_both_paths_and_preserves_interactive_floor`
to the Oracle process journey. Start exactly three Oracle children and one
Scribe child, provision one public API key, and use `vala-sdk` against the
coordinator's public HTTP address. Assert distinct PIDs, real peer-socket work,
no request path field, exact deadline/terminal/result agreement, Interactive
service under Analytical saturation, bounded result refusal, production
telemetry, and zero retained ownership. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::public_query_selects_both_paths_and_preserves_interactive_floor)' --run-ignored=all"
```

**GREEN.** Project Scenario 2's selected path through the existing Oracle
stream and consume Task 2's envelope, Task 3's qualified handle, and Task 3A's
admission/floor owner unchanged. Extend only the existing bounded
candidate/selection/fallback/path/outcome telemetry. Do not repeat their
operator, memory, or contention matrices.

**REFACTOR.** The journey observes existing owners; it creates no production
probe or duplicate physical matrix.

### Scenario 5 — Interactive stale replacement versus Analytical terminal failure

**Behavior.** Unsupported, distributed-planning-failed, and no-exchange
candidates remain graphless Interactive before selection. The existing typed
stale-Iceberg first-batch detector may replace only attempt zero before output
escapes from an Interactive path. After Analytical selection, stale data, peer
loss, transport/resource refusal, cancellation, deadline, or cleanup failure is
terminal and never reruns locally. Sensitive-column denial remains before
planning, peer, or source IO. Maps REQ-002, REQ-007, REQ-008, INV-001, INV-002,
INV-003, INV-004, AC-003, AC-004.

**RED.** Add
`analytical_activation::fallback_is_preselection_only_and_failure_is_terminal`.
Drive an unsupported and a no-exchange Interactive fallback, including typed
stale replacement on attempt zero; then drive the same stale condition and an
injected peer loss after Analytical selection plus an under-privileged query.
Assert selected path, attempt count, terminal, audit, zero unauthorized IO, no
Analytical rerun, and zero ownership. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::fallback_is_preselection_only_and_failure_is_terminal)' --run-ignored=all"
```

**GREEN.** Add the immutable selected path to `StaleReplacementGate`. Permit
the existing one replacement only for attempt zero, Interactive selection, and
no escaped output. Interactive fallbacks release their ordinary stream-local
owners before the existing re-pin/re-admit loop. A selected Analytical stale or
failure settles its graph and returns one failed terminal; remove every edge
from selected Analytical back to `execute_session`, `stream_physical`, or
`plan_distributed_split`.

**REFACTOR.** Keep one terminal constructor for both paths; only selected path
and typed outcome vary.

### Scenario 6 — Audit, scheduled caller, and server lifecycle

**Behavior.** One Oracle read-audit acceptance fsyncs before rows and stages
append none. The server-owned scheduled caller uses the same operation and
selection sequence, and cancellation/deadline joins the complete graph. Public
gRPC errors use the canonical Wyrd problem metadata. Maps REQ-008, REQ-009,
REQ-010, REQ-011, INV-001, INV-002, INV-003, AC-003, AC-005, AC-006, AC-007.

**RED.** Add
`query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`
to the existing `server` journey target. Assert one accepted/relayed logical
read, no stage audit, gRPC initial deadline metadata, selected terminal path,
canonical problem fields, the production scheduled adapter's use of
`AppState::query_sql`, cancellation, private peer isolation, readiness, and
zero ownership. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey -E 'test(=query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup)' --run-ignored=all"
```

**GREEN.** Preserve audit-WAL acceptance before either execution path can emit
rows and pass the authenticated context, permission digest, pinned cut, request
ID, and deadline unchanged. Add the original Task 04's concrete
`query::ScheduledQueryCaller` adapter: it owns its server-authorized context
and calls only `AppState::query_sql`, consuming and settling the returned stream
under the caller's cancellation token and original deadline. It owns no clock,
queue, job, loop, path selector, or alternate operation.

Replace the partial gRPC error mapping with the existing canonical
`wyrd_tonic::wyrd_error_to_status` mapper while retaining retry metadata only
for existing retryable pre-stream capacity errors. Keep peer services private,
ready only with their security dependencies, and joined during ordered
shutdown.

**REFACTOR.** Add no scheduler, alternate Analytical method, audit writer, or
second gRPC envelope.

## Cross-scenario decisions and invariants

The authoritative order is:

```text
authorize and pin cut
  -> admit one local query envelope
  -> audit and drain the cut
  -> register providers on the local session
  -> build a locally executable physical plan
  -> validate_supported(local plan)
  -> unsupported: execute retained local plan as Interactive
  -> supported: rebuild through pinned datafusion-distributed planner
       -> planning failure/no surviving exchange: execute retained local plan as Interactive
       -> surviving DistributedExec: select Analytical
          -> transfer resources/register graph/start lifecycle
          -> reserve and publish participants immediately before dispatch
          -> execute once and settle terminally
```

The local and dependency builds use the same authorized logical statement,
pinned providers, deadline, and query-owned runtime. Replanning is private
execution work, not a second optimizer or new routing-facts subsystem. No
fallback path may observe an `AnalyticalGraphState`; no selected Analytical path
may recover by opening the retained local plan.

IDs and SQL remain scrubbed trace fields, never metric labels. Cross-process
acceptance uses distinct child processes and real public/private sockets.
Public contracts remain source-generated. Revision 5 MCP work is downstream
and outside this successor.

Material authority and inherited contracts:

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/operations/deployment-and-release.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `tasks/02-analytical-query-envelope.md`
- `tasks/03-physical-analytical-baseline.md`
- `tasks/03a-oracle-contention-qualification.md`

## Broader verification

Run named tests scenario by scenario, then:

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run test:bifrost:journey:sdk
mise run test:e2e
mise run codegen:check
mise run check:proto-drift
mise run check:client-tier
mise run check:bifrost-resource-governance
git diff --check
```

`mise run gate` remains CI-owned; this successor does not change shared CI or
build infrastructure.

## Completion evidence

- Retained Scenario 1 test and commit evidence remain green.
- Real-server selection matrix proves local validation precedes dependency
  planning and every fallback precedes graph/resource/participant ownership.
- Source and runtime evidence proves exactly two builds only for supported
  candidates reaching dependency planning, with no custom capturing planner.
- Selected Analytical evidence proves one envelope transfer, one graph,
  attempt zero, one participant publication, real remote exchange, and no
  fallback after selection.
- Rust SDK, HTTP/gRPC, UI, data-scientist, scheduled, stale, authorization,
  transport-drop, cancellation, peer-loss, pressure, audit, readiness, and
  shutdown evidence all settle to zero ownership.
- Diff audit proves Tasks 2 and 3, the approved spec, request shape, dependency
  pins, and public path selection remain unchanged.

## Material stop conditions

Return `SPEC_REVISION_REQUIRED` if completion requires a public path hint,
EXPLAIN, automatic retry, broader operator promise, changed audit/tenant
semantics, a second listener, or a changed accepted outcome.

Return `PLAN_BLOCKED` if the pinned planner cannot build and retain a
pre-activation distributed plan over the existing query-owned runtime and
frozen resolver inputs without registering a graph or issuing peer IO. Do not
respond by extending `validate_supported`, using `plan_distributed_split`,
capturing an internal plan with a custom `QueryPlanner`, or falling back after
resource transfer.

## Execution evidence

### Scenario 1 — retained, GREEN

`b1634b93b` already ships one typed terminal path with no request selector.
Re-verified: `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E
'test(=vala::api::tests::query_terminal_projects_path_without_request_selector)'`.

### Scenario 2 — mechanism GREEN, ordering BLOCKED

Implemented on the local session in `oracle/mod.rs`:
`select_analytical` builds one locally executable physical plan, applies
Task 2's unchanged `splitter::validate_supported`, rebuilds only supported
candidates through `analytical::planning_session` (pinned
`with_distributed_planner()`, no channel resolver, no supervisor mutation, no
peer IO), and selects Analytical only when `exec::is_distributed_plan` holds.
Resource transfer, graph registration, participant reservation and publication
all move after selection. `analytical::lease_session` now returns the envelope
to its permit via the new `admission::restore_query_resources` when graph
registration refuses. `execute_analytical_session` and
`lease_analytical_session` are removed. Outcomes are counted on
`oracle_query_analytical_selection_total{outcome}`.

New journey
`crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_activation.rs::analytical_selection_requires_supported_physical_exchange`
(four-node `three_oracles_one_scribe`) passes for: non-candidate → Interactive;
supported candidate with an armed planner failure → Interactive with the flag
consumed; no-exchange candidate → Interactive with the exact local result;
grouped aggregate → Analytical; the same via `query_sql_with_participant_cut`
→ Analytical. Each case settles to zero retained ownership.
`Summary [ 4.919s] 1 test run: 1 passed, 17 skipped`.

### TASK_REVISION_REQUIRED — the mandated pre-distribution predicate rejects
### plans the pinned planner can distribute

`mise run test:bifrost:journey:oracle` regresses two journeys that pass on
`5d3cc09f7`:

- `capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`
- `peer_network::analytical::inactive_baseline_executes_join_group_spill_and_interchangeable_topology`

Server evidence for the first:

```
oracle_query_analytical_selection_total{outcome="unsupported"} = 1
Error during planning: unsupported distributed Oracle plan:
  aggregate mode outside Partial/PartialReduce/Final/FinalPartitioned
Oracle query released after failure phase="execution rejection"
  error=QueryExecutionFailed
```

Two independent gaps, both structural:

1. **Domain mismatch.** `validate_supported` was written for post-distribution
   plans. `splitter.rs:686-706` accepts only `Partial`, `PartialReduce`,
   `Final`, and `FinalPartitioned`, and its own comment states "every
   distributed aggregate carries a `Partial` layer". A locally executable plan
   carries `AggregateMode::Single`/`SinglePartitioned`, so the task's required
   order — validate the local plan with the unchanged predicate before the
   pinned build — refuses candidates the pinned planner distributes correctly
   today. The task forbids extending `validate_supported`, so this cannot be
   resolved inside the task.
2. **The unsupported fallback is unreachable.** The task requires an
   unsupported candidate to "return an Interactive terminal and the exact local
   result". It cannot:
   - `codec.rs::RemoteSourcePlaceholderExec::execute` delegates to an
     `EmptyExec`, and `exec.rs::OracleTableProvider::scan` emits that
     placeholder on every node that owns a `fragment_dispatcher`. Streaming the
     retained plan silently returns zero rows.
   - Falling through to the existing Interactive path re-enters
     `splitter::split_physical_plan_with_context`, which calls the same
     `validate_supported` and refuses again — confirmed above.
   - Remote `ScribeFollowerSource` rows are not readable locally, so any
     genuinely local rebuild silently drops the live tail under live-inclusive
     visibility.

Two materially different reachable designs:

- **A — second, non-distributed provider registration.** Register a local-only
  provider set, rebuild a third physical plan, and execute it as Interactive.
  Adds a second provider-registration owner, a third physical build, retained
  cuts, and requires an unstated live-tail visibility restriction.
- **B — leaf-only Interactive split.** Keep unsupported operators on the leader
  and distribute only the leaf scans. Correct and cheap, but modifies Task 2's
  splitter, which this task forbids.

Both alter ownership and test topology, so the choice is not an implementer
decision. Scenarios 3-6 are not started pending it.

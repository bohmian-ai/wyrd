---
id: BIFROST-R4-T04-PRODUCTION-ACTIVATION
title: Activate server-owned Analytical selection through the one public query contract
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T03A-CONTENTION-QUALIFICATION]
requirements: [REQ-001, REQ-002, REQ-003, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
parent_task: BIFROST-R3-T2-PRODUCTION-ACTIVATION
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Production query activation

## Outcome and value

The existing authenticated raw-SQL operation conservatively chooses
Interactive or Task 3's qualified Analytical path without caller hints.
Analytical selection occurs only after a supported distributed physical plan
with a real exchange and successful path admission; every later failure is
terminal. Rust, HTTP/gRPC, and internal scheduled callers receive the same
stream, selected-path terminal, errors, audit, cancellation, and cleanup.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

Retain `OraclePlanner`'s optimized-plan classification, immutable
`PlannedSqlCut`, current authenticated query routes, audit-WAL acceptance,
terminal-safe stream, lifecycle cancellation service, and internal
`AppState::query_sql` seam. Consume Task 3A's qualified class/tenant admission,
Interactive floor, and aggregate resource owner without modifying them. Do not
create exact routing facts, public EXPLAIN, path hints, a query-job schema, or
automatic retry. Replace the current Interactive-only dispatch after the
qualified Analytical handle is integrated.

## Owners, scope, consumers, and non-goals

- `vala-bifrost-redux::Oracle` owns prepare, candidate classification, physical
  planning/validation, path admission, selection, execution, and settlement.
- `oracle/admission.rs` owns the Task 3A-qualified path counters/queues and
  Interactive floor; this task only consumes them during path selection.
- `oracle/query_stream.rs` owns one terminal contract.
- `wyrd-spec::vala::api` and proto source own the closed terminal execution path;
  `wyrd-tonic`, `wyrd-server`, and `vala-sdk` project it.
- `vala-sdk` is the one shared Rust client implementation for request
  construction, incremental decoding, terminal/error validation, cancellation,
  deadline handling, and settlement. Every language or agent client consumes
  this implementation through a thin boundary; none reimplements its logic.
- `wyrd-server` owns audit-WAL-before-rows, public/private listener composition,
  lifecycle cancellation, readiness, shutdown, and internal scheduled calls.
- `wyrd-testing` owns Rust/HTTP/gRPC and multi-process journeys. Evidence mapped
  to REQ-003 or AC-002 uses the existing `BifrostProcessCluster` with three
  `ProcessNodeTarget::Oracle` children and one `ProcessNodeTarget::Scribe`
  child, drives `ProcessNode::http_addr` or `grpc_addr`, and authenticates with
  `BifrostProcessCluster::provision_public_api_key`. `WyrdTestCluster` remains
  available for in-process supporting checks but cannot prove a process or
  socket boundary. Do not add a cluster abstraction or modify `WyrdTestServer`
  ownership.

MCP belongs to Task 5 as a thin projection over this Rust client, not an
independent client implementation. Python and TypeScript are outside revision
4's task packet and acceptance lanes. Do not repeat Task 3's physical operator
qualification here.

## Ordered implementation scenarios

### Scenario 1 — One request and one typed terminal path

**Behavior.** The request remains SQL, visibility, freshness, and optional
deadline only. A successful or failed started stream carries exactly one closed
`Interactive | Analytical` selected path; no graph, class, workers, plan, or
selector becomes public. Maps REQ-001, REQ-008, INV-001, INV-008, AC-006, AC-008.

**RED.** Add source/schema tests proving request closure and terminal path:
`vala::api::tests::query_terminal_projects_path_without_request_selector` in
`wyrd-spec`. Exact:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::tests::query_terminal_projects_path_without_request_selector)'
```

It fails because the current terminal has no selected path.

**GREEN.** Add `QueryExecutionPath::{Interactive, Analytical}` to the
language-neutral terminal source, derive required schema/proto conversions, and
thread the server-selected value through `vala-sdk`. Do not alter
`BifrostQueryRequest`. Pre-stream admission errors remain structured request
errors; started streams terminate explicitly and preceding failure frames never
form a successful result.

**REFACTOR.** Generated OpenAPI/schema/stubs come only from owners; no hand edits
or persisted query-path row.

### Scenario 2 — Conservative request-local selection

**Behavior.** Interactive is default. Existing classification nominates an
Analytical candidate and the participant-cut leader derives one exact
provisional graph identity from the already-pinned request facts. The candidate
is admitted and leases that graph once; Task 2's supported physical-plan result
plus a real exchange then irreversibly selects Analytical. Unsupported shape,
safe planning failure, or no exchange falls back before selection while the
provisional graph remains owned and settles normally. Unsupported and
no-exchange outcomes execute their already-built leader plan as Interactive;
distributed-planning failure uses the existing Interactive planning/execution
path. Maps REQ-002, REQ-003, INV-001, INV-002, INV-008, AC-002, AC-003, AC-008.

**RED.** Add
`oracle::tests::analytical_selection_requires_supported_physical_exchange`.
Drive both `Oracle::query_sql` and the forwarded
`Oracle::query_sql_with_participant_cut` leader path. Assert each Analytical
candidate leases exactly one graph whose public ID equals the participant-cut
attempt UUID, whose `DataFusionQueryId` is allocated once and differs from that
public ID, and whose snapshot and permission digests equal the existing audit
helpers' results over the exact `PlannedSqlCut` and authorized context. Mutate
the supported predicate, exchange presence, and admission result. A supported
exchange must publish participants and select Analytical; an unsupported or
no-exchange plan must issue no reservation, execute the same already-built
leader physical plan as Interactive, and settle the provisional graph. Inject a
`plan_distributed_split` failure before participant publication and assert the
same session, admission, pinned cut, and provisional graph continue through
`Oracle::execute_session` without a reservation, returning Interactive on
success and its stable error if Interactive planning also fails. Assert no
re-admission, cut re-pin, or graph re-lease on any fallback; assert one physical-
plan build and no optimizer rerun only for unsupported and no-exchange outcomes.
It fails while both production entries pass no Analytical context and no
production transition leases and then settles a provisional graph. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::tests::analytical_selection_requires_supported_physical_exchange)'
```

**GREEN.** Keep the request-local selected path initialized to `Interactive`.
Both production entries already converge on `Oracle::run_sql_attempt`; derive
the candidate's `AnalyticalAttemptContext` there, after
`plan_or_reuse_attempt` returns the exact `PlannedSqlCut` and before
`admit_and_lease_attempt`. Set `public_query_id` with
`PublicQueryId::from_uuid(participant_cut.attempt_id().as_uuid())`, allocate one
`DataFusionQueryId::allocate()`, set `snapshot_digest` from
`aggregate_audit_digest(planned.cuts.iter().map(|cut|
cut.snapshot_digest.as_str()))`, and set `permission_digest` from the existing
`audit_digest(&context.permission)`; copy only each digest's string value into
the context. The participant cut remains the deadline and participant/fence
authority passed to `AnalyticalExecutionHandle::lease_session`. Keep the
explicit context accepted by `query_sql_inactive_analytical` solely as the
test-support override; production `None` now means derive this context, not use
an Interactive session. Update the stale comments on `SqlAttemptInput` and
`DataFusionQueryId` to describe provisional candidate allocation.

Admit once and lease the existing Analytical session/graph once before physical
planning. Change the existing Analytical execution result to return its selected
`QueryExecutionPath` with the schema, stream, and statistics. Extend the
existing physical-planning seam to return its one already-built leader
`Arc<dyn ExecutionPlan>` together with Task 2's supported-plan verdict and
exchange presence before any stream is opened. A supported plan with a real
exchange publishes participants, opens the distributed stream, and returns
`Analytical`. A rejected supported-plan verdict or absent exchange publishes no
participants; when that returned leader plan is locally executable, open that
same `Arc` through the existing `stream_physical` path and return `Interactive`
while retaining the provisional `AnalyticalAttemptOwnership` in
`AdmittedQueryGuard` for normal settlement. If `plan_distributed_split` fails
before participant publication and therefore returns no reusable plan, call
`Oracle::execute_session` with the original session, SQL, and logical selected
bytes. Keep the same admission guard, `PlannedSqlCut`, participant cut, and
provisional `AnalyticalAttemptOwnership`; a successful Interactive execution
returns `Interactive`, while failure of that existing Interactive path returns
its stable error. Do not re-admit, rebuild or re-pin `PlannedSqlCut` or its
participant cut, or allocate another graph ID. The no-second-optimizer rule
applies only when unsupported or no-exchange planning already returned a
reusable leader plan; distributed-planning failure intentionally delegates its
fresh local plan to `execute_session`. Admission refusal is pre-selection
failure; reservation and every failure after participant publication are
terminal Analytical failures.

**REFACTOR.** Keep the selected value private and immutable after Analytical
assignment. Candidate class, actual graph ownership, and selected terminal path
remain separate facts. Do not introduce a routing state-machine type; query
class does not become the execution-path authority.

### Scenario 3 — Shared Rust stream settlement

**Behavior.** HTTP and gRPC initial response metadata return the server's exact
absolute wall-clock deadline as Unix epoch milliseconds in
`x-wyrd-query-deadline-ms`; `vala-sdk` retains that value with the request ID
and response body. A healthy stream, including explicit cancellation or a
bounded-collection overflow before transport failure, requests cancellation at
most once, drains the existing body under that deadline, and validates its
terminal before reporting settlement. If decode or transport has already
broken the body, the client preserves the original error, issues the same
existing cancellation request, and polls the existing status route until the
stable `RunningQueryNotFound` 404 proves server cleanup completed; this path
does not claim terminal validation. Raw Rust `Drop` only drops the body and
signals cancellation; it never blocks, spawns client work, or claims awaited
settlement. Language and MCP projections consume this owner instead of
implementing server lifecycle policy. Maps REQ-007, REQ-008, REQ-010, INV-003,
INV-004, INV-008, AC-004, AC-006.

**RED.** Add
`query::tests::query_result_stream_settles_every_incomplete_exit_once` in
`vala-sdk`. Use deterministic HTTP lifecycle gates for a normal terminal,
healthy explicit close, result-limit overflow, protocol/decode failure, and
transport failure. Assert the client retains the response deadline, rejects a
missing, malformed, negative, or non-representable deadline header before
returning a stream, sends at most one cancellation, and never exceeds that
deadline. The healthy cases must drain the same body and validate its terminal;
the broken cases must return the original `ValaSdkError` with unchanged stable
code, status, and detail after cancellation and status polling reaches the stable
`WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND`, with no terminal-validation claim.
Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-sdk --lib -E 'test(=query::tests::query_result_stream_settles_every_incomplete_exit_once)'
```

Add
`oracle::query_stream::tests::running_query_terminal_owner_drop_retires_untransferred_failure`
in `vala-bifrost-redux`. Insert one real running entry, drop its untransferred
terminal owner, and assert one failed retirement and no remaining entry; then
explicitly finish a second owner and assert its later drop is a no-op. This is a
verification-only regression lock before the refactor and must remain green
while the post-registration audit, execution, first-batch, telemetry, and schema
error paths continue to rely on that destructor. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::query_stream::tests::running_query_terminal_owner_drop_retires_untransferred_failure)'
```

Add the focused Oracle journey
`analytical_public::transport_drop_retains_running_status_until_cleanup_joins`.
For HTTP, arm the existing server schema stall, wait until the body reaches it,
and drop the response body. For gRPC, read the schema frame and then drop the
receiver; do not add a second transport stall. Start
`BifrostProcessCluster::start` with
`[ProcessNodeTarget::Oracle, ProcessNodeTarget::Oracle,
ProcessNodeTarget::Oracle, ProcessNodeTarget::Scribe]`, provision one shared
public API key, and drive the coordinator child's HTTP/gRPC addresses directly.
Exercise graphless Interactive, no-exchange Interactive fallback with its
provisional Analytical graph, and selected Analytical. For each graph-owning
case, arm the new one-shot cleanup pause on the coordinator before the query;
after transport drop, await that pause, assert the running status is still
cancelling and graph ownership is retained, then release it and assert the same
public status route returns `WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND`. For
graphless Interactive, assert drop synchronously destroys local batch,
admission, and resource owners before its running entry disappears. In every
case assert transport drop only signals the existing query cancellation token;
also assert the no-exchange terminal selects Interactive and HTTP/gRPC deadline
metadata equals the active summary deadline. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::transport_drop_retains_running_status_until_cleanup_joins)' --run-ignored=all"
```

**GREEN.** Carry the participant cut's existing `DateTime<Utc>` deadline on
`OracleQueryStream` and project its checked nonnegative `timestamp_millis()`
through HTTP response headers, gRPC initial metadata, and the existing private
Oracle forwarding adapter without recomputing it at any transport hop. Document
the HTTP header in the generated OpenAPI source. A missing or malformed deadline
on either server-to-server forwarding or the public Rust client is a protocol
failure; no proto field, query request field, polling endpoint, or completed
result is added.

Make `vala_sdk::QueryResultStream` retain `QueryClient`, request ID, the raw
deadline milliseconds, body, and one settlement state. Explicit close and
owned overflow/error paths move the stream through that state exactly once.
While the body remains decodable, cancellation drains only that body and
accepts settlement only after a structurally valid terminal; it returns the
caller's original overflow error after that proof. Once decode or transport
fails, save the original error, issue `QueryClient::cancel` once, and call
`QueryClient::status` at a fixed 100 ms interval clipped to the remaining
absolute deadline until its stable not-found code proves removal. Transient
cancel/status failures do not
replace the original stream error; if proof is still unavailable at the
deadline, return that original error and record scrubbed unconfirmed-settlement
telemetry. Do not read or validate the broken body again.

Keep `RunningQueryTerminalOwner::Drop` as the fail-safe failed retirement for
every post-registration error before ownership transfer; do not rewrite those
early returns. Choose retirement ownership from actual graph ownership, never
from the selected terminal path. Immediately after `register_running_query`
inside `admit_and_lease_attempt`, inspect the returned
`AdmittedQueryGuard::analytical`. When it contains
`AnalyticalAttemptOwnership`, move the running owner into a new
`running_query: Option<RunningQueryTerminalOwner>` field on that graph's
existing `AnalyticalGraphState` before audit, drain, physical qualification, or
first-batch polling. This includes a provisional graph whose supported-plan or
no-exchange result later selects Interactive. Return
`AttemptOutput::running_query` as `None` for every graph-owning attempt. Only a
graphless Interactive attempt keeps `Some(owner)` through the pre-stream error
paths and moves it into `OracleQueryStream` after first-batch validation and
schema encoding succeed.

Implement the graph transfer as
`AnalyticalAttemptOwnership::retain_running_query`, delegating through its
existing signals to `AnalyticalSupervisor::retain_running_query` and following
the existing `retain_admission` returned-owner pattern. Reject a second owner
without replacing or dropping the first. A refused transfer returns the new
owner unchanged; keep it alive while signalling a failed terminal and awaiting
the same graph lifecycle settlement, then release admission and let its
fail-safe `Drop` retire the entry. Widen
`RunningQueryTerminalOwner::finish` only to `pub(super)` for the sibling
supervisor. The existing
`AnalyticalGraphLifecycle` explicitly finishes the accepted graph-held owner
after its cleanup join as part of clean graph release. After the distributed
join, close the local IPC stream before signalling Analytical settlement, while
admission remains held, so IPC-close failure is included in the exact public
outcome. Change the existing internal signal only: make
`AnalyticalGraphControl::Terminal` carry both its existing
`AnalyticalAttemptOutcome` and the final `QueryTerminalOutcome`, thread both
through `AnalyticalGraphSignals::terminal` and `AnalyticalGraphGuard::settle`,
and store the latter once in
`AnalyticalGraphState::running_query_outcome: Option<QueryTerminalOutcome>`.
`Success` and `Degraded` remain distinct; execution failure, cancellation,
deadline, or `AnalyticalGraphSignals::Drop` supplies `Failed`. The first
terminal signal wins both values. If later cleanup fails, the retained graph is
not retired and any eventual post-deadline release uses `Failed`. This adds no
wire field or second signal channel. Add one supervisor method
for that leader path,
`AnalyticalSupervisor::release_graph_and_finish_running_query`: after the
existing live-attempt and child-idle checks, it removes the graph entry,
invalidates the runtime, drops the released graph resources, and then calls
`RunningQueryTerminalOwner::finish` with the mapped terminal outcome. If
invalidation fails, reinsert the unchanged graph entry so its terminal owner
and cleanup remain observable. Follower releases continue through the existing
ownerless `release_graph`. Post-deadline eventual release uses the leader method
with a failed outcome. A timeout or cleanup failure before release keeps the
owner on the observable draining graph, so the status remains cancelling and
readiness/shutdown report the retained cleanup.

Keep only graphless Interactive retirement on `OracleQueryStream`. Make the
stream retain its frame generator and optional graphless terminal owner
separately. Its normal terminal polling path finishes that owner only after
`settle_and_finish_stream` has joined distributed work and released admission.
An Interactive fallback whose guard still contains
`AnalyticalAttemptOwnership` carries no stream-local terminal owner; its graph
lifecycle receives the Interactive stream's final public outcome and retires
the graph-held owner after cleanup. Make the frame generator private, expose one
inherent `next_frame` operation, and update Gate, server query service, HTTP,
gRPC, and current direct tests to consume that operation so no caller can bypass
retirement. Its synchronous `Drop` first signals the existing cancellation
token, then takes and drops the frame generator so local batch, admission, and
resource owners are destroyed, and only then drops any graphless terminal
owner; the existing fail-safe destructor retires it as failed. Normal
completion explicitly finishes that owner first, making its later drop a
no-op. HTTP and gRPC retain and poll the complete
`OracleQueryStream` instead of destructuring out `frames`; transport drop adds
no cleanup owner and starts no task—it only causes that existing cancellation
signal and Oracle-owned retirement order. Explicit `cancel` uses the same
stream polling/settlement path.

Under `test-support`, add one one-shot `AnalyticalCleanupPause` in
`oracle/analytical.rs`, owned through `oracle/analytical_supervisor.rs` by the
existing `AnalyticalSupervisor` and taken by the next
`AnalyticalGraphLifecycle`. Reuse the existing `AnalyticalExecutePause`
`AtomicBool`/`Notify` handshake shape rather than adding a generic gate. The
supervisor's test-only `Mutex<Option<Arc<AnalyticalCleanupPause>>>` accepts one
arm only while empty, and lifecycle start takes and clears that slot. `Arm`
installs that single pause; `Await` waits until the lifecycle has joined the
attempt, released participants, and observed idle graph children; and `Release`
lets the lifecycle call
`AnalyticalSupervisor::release_graph_and_finish_running_query`. The pause sits
immediately before that clean release, delays only the production lifecycle,
and makes no cleanup or terminal decision.

Project the pause through the existing `BifrostProcessCluster` control protocol
in `wyrd-testing/src/bifrost/process_cluster.rs` and its `child.rs`
with `ArmAnalyticalCleanupPause`, `AwaitAnalyticalCleanupPaused`, and
`ReleaseAnalyticalCleanupPause` requests;
`CleanupPauseArmed`, `AnalyticalCleanupPaused`, and `CleanupPauseReleased`
responses; and matching `ProcessNode::arm_analytical_cleanup_pause`,
`await_analytical_cleanup_paused`, and `release_analytical_cleanup_pause`
methods. Add `ArmQuerySchemaStall` and `AwaitQuerySchemaStall` requests,
`QuerySchemaStallArmed` and `QuerySchemaStalled { query_id }` responses, and
matching `ProcessNode` methods that delegate to
`WyrdTestServer::stall_next_query_after_schema` and
`wait_query_schema_stall`; they create no second stall. The child retains only
the armed test handles, and the parent uses the schema stall only for the HTTP
case to position transport drop before awaiting the independent lifecycle
pause.

**REFACTOR.** `vala-sdk` is the sole shared client settlement owner. MCP only
bridges its tool-return and ceiling events to this API. Client-neutral behavior
discovered while implementing a projection must move into `vala-sdk` first and
receive Rust coverage there; the projection then exposes it without
duplication. Keep the existing running-query route and Analytical supervisor;
do not add a client background task, completed-query storage, polling API, or
lifecycle framework.

### Scenario 4 — Public UI and distributed Rust journeys

**Behavior.** A real UI query selects Interactive while Analytical capacity is
full; a real data-scientist query selects Analytical without a hint; exact
results and selected terminals return with zero ownership. A finite result-
transport pressure case refuses/cancels and joins without unbounded buffering.
Maps REQ-001,
REQ-002, REQ-003, REQ-006, REQ-008, REQ-009, REQ-011, AC-002–AC-007.

**RED.** Add
`analytical_public::public_query_selects_both_paths_and_preserves_interactive_floor`
to the Oracle journey target. Start one `BifrostProcessCluster` with three
Oracle targets and one Scribe target, provision one public API key, and drive a
coordinator's public HTTP address through `vala-sdk` for both the UI and
Analytical cases. Assert the child PIDs are distinct, remote work crosses a
peer socket, the raw request has no path field, the retained HTTP deadline
equals the server's active-query deadline, terminals differ, results match the
trusted local result, UI service survives Analytical pressure, result transport
refuses finitely, and every process-owned gauge/owner returns to baseline.
Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::public_query_selects_both_paths_and_preserves_interactive_floor)' --run-ignored=all"
```

**GREEN.** Wire the selected path into the existing public Oracle stream; reuse
Task 2's admitted guard, Task 3's handle, and Task 3A's qualified contention
owner. Extend only existing production telemetry for candidate/selection/
fallback/path terminal and ensure all active gauges settle through the joined
owner. Do not repeat Task 3A's admission or memory load matrix.

**REFACTOR.** Do not copy the physical operator matrix; consume Task 3 evidence.

### Scenario 5 — Interactive stale replacement versus Analytical failure

**Behavior.** Unsupported/no-exchange candidates fall back before Analytical
selection. The existing typed stale-Iceberg first-batch detector may replace
only the first selected Interactive attempt before output escapes. Typed stale
or injected peer/transport/resource/cancellation/deadline/cleanup failure after
Analytical selection returns one failed Analytical terminal, with no
replacement, rerun, or successful partial rows. An under-privileged
sensitive-column request is refused before selection and peer/source IO. Maps
REQ-002, REQ-007, REQ-008, INV-001, INV-002, INV-003, INV-004, AC-003, AC-004.

**RED.** Add
`analytical_public::fallback_is_preselection_only_and_failure_is_terminal`.
Drive a no-exchange execution that remains Interactive and encounters the
existing typed stale-Iceberg first-batch condition on attempt zero; assert one
replacement before output. Drive the same typed condition after Analytical
selection, an under-privileged sensitive-column query, and an injected
post-selection peer loss; assert path/attempt/terminal/audit/ownership, no
Analytical replacement or Interactive rerun, and zero peer/source IO for the
authorization denial. Use the existing `BifrostProcessCluster` with three
Oracle child processes and one Scribe child process, its shared public API key,
and a coordinator's public endpoint. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_public::fallback_is_preselection_only_and_failure_is_terminal)' --run-ignored=all"
```

**GREEN.** Carry the locked selection into `query_stream`; remove every error
edge from selected Analytical to Interactive. Await joined cleanup before the
failed terminal. Keep the current first-batch typed-stale detector. Add the
immutable selected path to `StaleReplacementGate` and allow its existing one
replacement only when `retry_ordinal == 0`, the selected path is Interactive,
and no output has escaped. A typed stale first batch on Analytical follows the
ordinary terminal settlement path; it never returns `Ok(None)` to
`Oracle::run_sql_query_attempt_loop`. When that eligible Interactive attempt
owns a provisional Analytical graph, signal failure and await its joined graph
settlement—including retirement of the graph-held running owner—before
returning `Ok(None)` for the existing re-pin/re-admit retry. Preserve
authorization before physical dispatch and stable Wyrd error mapping at
HTTP/gRPC/Rust boundaries.

**REFACTOR.** One terminal constructor serves both paths; only the selected path
and typed outcome vary.

### Scenario 6 — Audit, internal scheduled caller, and lifecycle cancellation

**Behavior.** One audit WAL acceptance fsyncs before rows; stages append none.
The server-owned scheduled caller uses the same query operation, selects
Analytical for a supported global query, and cancellation/deadline settles the
whole graph. Maps REQ-008, REQ-009, REQ-011, INV-001, INV-002, AC-003, AC-005,
AC-007.

**RED.** Add
`query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup`
to the server journey. Assert one accepted/relayed logical read, no stage audit,
generated gRPC initial deadline metadata, terminal path, and canonical problem fields (`code`, HTTP
`status`, `title`, `detail`, `remediation`, `details`, and request instance),
the production scheduled adapter's same `AppState::query_sql` route,
cancellation, private peer isolation, readiness, and zero ownership. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey -E 'test(=query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup)' --run-ignored=all"
```

**GREEN.** Preserve the existing audit acceptance before either engine can emit
rows; pass the authenticated context, permission digest, pinned cut, request ID,
and deadline unchanged into Analytical. Add one production
`query::ScheduledQueryCaller` dependency-owning adapter that accepts a server-
authorized context and `BifrostQueryRequest`, calls only
`AppState::query_sql`, and consumes/settles the returned stream under the
caller's cancellation token and original deadline. It owns no clock, loop,
queue, durable job, path selector, or alternate query API; server job owners can
invoke its `run` method, and this journey invokes that exact production method.
Replace `grpc::query::query_status`'s partial `ErrorInfo` mapping with the
existing canonical `wyrd_tonic::wyrd_error_to_status` problem+json metadata
mapper and retain retry metadata only for the existing retryable pre-stream
capacity classes. Keep peer services private and included in readiness/ordered
shutdown. The journey starts
`BifrostProcessCluster::start` with three Oracle targets and one Scribe target,
uses `provision_public_api_key`, and drives the selected child's existing public
HTTP/gRPC addresses. The process cluster retains every child listener and
shutdown owner; the journey adds no `WyrdTestServer` composition.

**REFACTOR.** No scheduler, alternate internal analytical method, audit writer,
or second gRPC error envelope.

## Cross-scenario decisions and authority

Selection is request-lifecycle state, not durable SQL state. The real-exchange
check occurs on the built physical plan. Production telemetry labels use closed
path/reason/outcome values only; IDs and SQL stay scrubbed traces. Rust is the
client implementation authority: wire parsing, validation, errors, deadlines,
cancellation, and settlement must not diverge by language.

The deadline header is the existing participant-cut deadline represented as a
base-10 Unix epoch millisecond integer. `RunningQueryNotFound` proves only that
the active server owner retired after cleanup; it is not a recovered terminal.
Healthy streams validate terminals, broken streams preserve their originating
error and validate settlement only through active-entry disappearance.
Transport adapters retain the complete Oracle stream and own no retirement
logic. Only graphless Interactive retirement follows synchronous stream-local
destruction. Whenever `AdmittedQueryGuard` contains
`AnalyticalAttemptOwnership`, including an Interactive pre-selection fallback,
retirement belongs to the existing supervisor before any transport can receive
the stream and occurs only after joined graph cleanup. Selected path describes
execution, not retirement ownership.
Cross-process acceptance evidence comes only from `BifrostProcessCluster` with
distinct child PIDs and real public/private sockets. `WyrdTestCluster` evidence
is supporting in-process coverage and cannot satisfy REQ-003 or AC-002.

Task 3A owns admission fairness, Interactive protection, and lowest-rung
contention qualification. This task may observe those behaviors through public
routing but must not redesign or duplicate their owners or evidence matrix.

Authority: `architecture/wyrd-design.md`, `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`,
`architecture/references/domain/olap-serving.md`,
`architecture/references/languages/errors.md`, and
`architecture/references/languages/testing-workflows.md`.

## Broader verification

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

## Completion evidence

- Source-generated request/terminal diff proving no selector or EXPLAIN.
- Selection mutation matrix and path admission/floor snapshots.
- Exact public/graph/snapshot/permission/deadline identity and single-lease
  evidence for both production leader entries, including same-plan unsupported/
  no-exchange fallback, `execute_session` planning-failure fallback, and retained
  provisional-graph cleanup.
- UI, data-scientist, scheduled, failure, cancellation, and pressure journeys.
- HTTP/gRPC deadline metadata equality, healthy terminal-drain, broken-stream
  status settlement, fail-safe pre-stream retirement, and deterministic
  graphless/no-exchange/Analytical response-drop ordering evidence from the
  lifecycle pause.
- Distinct child-PID and public/private socket evidence from the existing
  three-Oracle/one-Scribe `BifrostProcessCluster`.
- Single audit acceptance/no stage audit evidence.
- Production telemetry deltas and zero owner/readiness/shutdown snapshots.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if activation requires a path hint, EXPLAIN,
automatic retry, exact routing-facts subsystem, broader operator promise,
durable query job, new listener, or changed tenant/audit semantics.

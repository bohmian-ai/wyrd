---
id: TASK-004
kind: implementation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 10
requirements: [REQ-014, REQ-015, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-029, REQ-030, REQ-032, REQ-032A, REQ-033, REQ-034, REQ-034A, REQ-034B, REQ-034C, REQ-036A, REQ-038, REQ-039, REQ-041, REQ-042, REQ-043, REQ-045, REQ-048, REQ-050, REQ-052, INV-001, INV-005, INV-006, INV-008, INV-009, INV-010, INV-010A, INV-011, INV-012, INV-013, INV-018, INV-019, INV-020, INV-021, INV-022, INV-023, AC-004, AC-008, AC-009, AC-010, AC-011A, AC-012, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-025, AC-027, AC-028]
depends_on: [TASK-001, TASK-002, TASK-003]
---

# Bounded accepted Workflow jobs on the server

Implementation skill: `$wyrd-implement`.

## Outcome and Value

A registered Workflow can be accepted once, execute after its request/token
ends, be inspected/cancelled by a newly authenticated owner, and terminate with
a consistent bounded snapshot. Internal gateway calls and built-in tools retain
captured scopes and attribution while their owners enforce current admission.
This is a process-local host around the existing executor, not another engine.

## Owners, Scope, Consumers, and Prohibited Changes

Server owns run lifecycle/admission/config/HTTP and a private in-process gateway
caller; runtime permission/builtin-role owners own `workflows:run`; Cards and
Bifrost owners retain resource permission/audit. Skald remains tenant/SQL-free.
Use `Caller::from_authenticated` as the verified context precedent; `Caller`
already retains principal/scopes/delegation but no bearer token. Capture only
that trusted attribution/authority and the spec's graph/deadline bound.

No durable queue/table/lease/recovery, new remote language/MCP surface, runtime
Invoke policy, bearer retention/renewal, widening grant refresh, second audit
writer, client-owned tenancy, arbitrary tool registration or Skald tool loop.
Gateway credential/deployment governance remains live and gateway settlement
remains gateway-owned. Shared Vault/Operators and Native transport are unchanged.

## Approach

1. Wire exact configuration/defaults, typed permission and builtin-role grants,
   one server run owner, route registrations and composition/boot/shutdown.
2. Implement the normative authenticated/audited create→tracked preparation→
   atomic acceptance sequence, exact graph/input/reserve admission and replay.
3. Bind the accepted context to the existing in-process gateway and two read
   tools; keep dynamic object authorization and canonical audit under owners.
4. Host Skald execution with short complete snapshot replacements and terminal
   compare-and-set after required drain; implement GET/cancel/retention.
5. Prove security, races, lifecycle, route matrix, resource bounds and sibling
   service responsiveness through production-shaped real-client journeys.

### Cross-owner seams and correctness-sensitive ordering

The packet's native Run/request/call DTO stubs are in TASK-001 and shared client
methods in TASK-003. This host consumes those types without wrappers: POST
`/v1/workflow-runs` returns queued native Run with 202, matching replay 200;
GET `/v1/workflow-runs/{run_id}` returns 200 current native Run; POST
`/v1/workflow-runs/{run_id}/cancel` returns 200 winning terminal native Run.
Run lookup is `(verified tenant, effective principal, WorkflowRunId)`; every
unknown/foreign/expired/evicted/lost/non-owning-replica lookup is
`WYRD_WORKFLOW_404_RUN_NOT_FOUND`. Preparation failure returns canonical HTTP
problem and creates no run. Runtime failure is a normal terminal Run value.

Add `WyrdServerConfig.workflow: ServerWorkflowConfig`, deny unknown fields and
manual table-valued Default, serde container defaults:

| Field | Default |
|---|---:|
| `default_timeout_seconds` | 1800 |
| `max_timeout_seconds` | 7200 |
| `max_concurrency_per_run` | 8 |
| `max_active_global` / `max_active_per_tenant` | 32 / 4 |
| `max_retained_global` / `max_retained_per_tenant` | 128 / 32 |
| `max_steps_per_run` / `max_dependency_edges_per_run` | 1024 / 4096 |
| `max_resolved_graph_bytes` | 8 MiB |
| `max_input_bytes` / `max_step_result_bytes` / `max_run_bytes` | 1 / 1 / 4 MiB |
| `external_gateway_bindings` | empty keyed map |

Timeout fields are `u64`, count/byte fields `usize`; bindings map is keyed by
`CredentialBindingName`, value `ServerExternalGatewayBindingConfig { tenant:
DataTenantId, binding: ExternalGatewayBindingConfig }` with binding flattened,
not nested in TOML. Native pure binding DTO is `{protocol, origin,
secret_headers: BTreeMap<String, SecretRef>}`. Server stores refs, resolves only
selected tenant-qualified binding during preparation; no plaintext config or
cross-tenant fallback. Reject zeros, default timeout above maximum, per-tenant
above global active/retained bounds, step-result above aggregate. Retention is
fixed 24 hours, not another setting; release graph/dependencies on terminal.

`Resource::Workflows`, existing `Action::Run`, `PermissionScope::All` and
`Permission::workflow_run()` produce `workflows:run`; writer/agent gain it,
admin wildcard covers it, reader/runtime_admin do not. Every create/replay/get/
cancel fresh authenticated permission decision audits before run lookup.
Capture principal, resource permissions/Card scope, credential attribution and
verified delegation from that accepted submission; do not retain bearer,
refresh or API-key secret. Token expiry/later grant changes do not cancel/widen
the accepted job; current gateway deployment/credential/limit rules stay live.

- In-process caller: `GatewayInvocation::run` in
  `components/gateway/invocation.rs` takes `Caller`, `GatewayCallRequest`, the
  existing authorized flag and separate cancellation. Set the approved typed
  request/dialect, stored fallback and remaining timeout. Do not claim workflow
  acceptance already authorized a model: each call keeps the gateway's decision.
  Pass `authorized = false` so the ordinary per-call admission is not bypassed.
  Child cancellation signals gateway-owned tracked settlement; Workflow drain
  neither owns nor waits for its accounting/capture/audit tasks.
- Cards tool: the exact audited UID precedent is
  `components::cards::routes::get_card_for`. CardRef reads must reach that same
  authorized/audited boundary (or its shared exact-ref equivalent) after safe
  identity resolution; `service::get_card_by_ref` alone does not audit/auth.
  Omitted tool space inherits executing Agent space, not untrusted input tenancy.
- Query tool: `query::service::stream_query` is the service seam; bounded argument
  and complete-result precedent is `mcp::bifrost::QueryArguments` and
  `ResultCollector` in `src/mcp/bifrost.rs`. Lift/reuse the narrow collection
  owner if necessary rather than copy another decoder. MCP and AgentTool retain
  their own surface projections, but share terminal-safe/result accounting.
  Successful JSON is the existing `{columns, rows, terminal}` shape; no rows
  without one trustworthy successful terminal and no truncation.
- Audit: use `crate::audit::authorize` in `src/audit/mod.rs` for Workflow create/get/cancel,
  transactional fail-closed. Gateway and Oracle use their canonical tracked
  non-blocking paths; no extra terminal audit when no permission is evaluated.
- Lifecycle: no IO under the one run-state lock. Auth/audit precede key lookup
  and existence disclosure; tracked preparation owns reservation before handler
  suspension; graph/tool/binding suitability precedes `202`; promotion transfers
  capacity atomically; all failed/shutdown preparations release/wake exactly once.
  Replay audits fresh authority but never replaces accepted graph/context.
  Request hash is BLAKE3 of JCS request; key scope is `(tenant, principal,
  IdempotencyKey)`. Reserve one tenant/global active slot before resolution.
  Preparation failure/shutdown creates neither public run nor cached failure;
  promotion rechecks shutdown and atomically transfers that slot to queued run
  and dedup entry. Release executor start signal only after acceptance is
  published to waiters. Creator response 202, matching waiters/replays 200.
- Shutdown: close Workflow admission/signal/drain before closing gateway tracked
  settlement or Bifrost/query services needed by in-flight steps. Reuse the single
  remaining shutdown deadline in `app::server::BoundServer::run`; no independent
  fresh timeout per subsystem and no workflow mutation after terminalization.
- Resource admission: count steps/edges before DAG construction; count unique
  native bodies incrementally before retention; stack-safe pure validation outside
  state lock uses bounded repository-native blocking/cooperative execution.
  Reserve full terminal metadata and one bounded error per step/run before
  acceptance; terminalization cannot fail for lack of budget.

### Query ownership across step abort

An admitted Bifrost tool query's response/collector and settlement continuation
are owned by tracked server work, not by the abortable Skald step or AgentTool
wait future. Establish that owner, register its per-run join/cancellation
responsibility, and transfer the response before exposing it to an AgentTool
waiter. The tracked owner owns the bounded row/result collection; the waiter
only receives its completed value/error. A waiter drop or step abort signals
the owner's cancellation synchronously and cannot destroy its response or
`cancel_and_settle` future. No untracked query task or second query engine is
introduced; use existing process tracking and Oracle controls.

Normative ordering on explicit cancel, deadline, parent interruption, or
shutdown: signal all admitted read-tool owners; abort/drain Skald futures;
await each read-tool owner's collection/settlement completion; only then
commit the server terminal WorkflowRun and release its active capacity.
Ordinary tool errors also await the same owner before returning their error.
No result can mutate the Workflow after that boundary. The trusted captured
context is retained only for this run's admitted operations/cleanup and is not
a new token or permission grant. Gateway settlement remains separately
gateway-owned under its already-specified rule; this query join must not make
Workflow await gateway accounting/capture/audit.

Preserve `RunningQueryControls::cancel_and_settle` in
`src/oracle/lifecycle_controls.rs`: signal the exact query immediately; route
authenticated remote lifecycle cancellation for forwarding; retain/drain the
response under its original query deadline. The shared shutdown deadline may
shorten, never restart or extend, that bound. A control acknowledgment or owner
absence is not terminal proof. A valid failed terminal may carry cleanup
residue; successful terminal proof requires clean EOF. Deadline, transport,
protocol, or shutdown loss records the owner's existing incomplete/unavailable/
protocol result and must not be reported as confirmed cleanup. Preserve the
Workflow's cancelled/timed_out outcome rather than inventing a successful tool
result or cancellation error field; record settlement failure through existing
safe query diagnostics/probes. Capacity may release after the owner has
completed either validated settlement or that bounded, honestly unconfirmed
outcome, never merely because the AgentTool waiter vanished. Once the original
deadline is exhausted, do not add a fresh cleanup timeout or retry ambiguous
remote cancellation. The existing Oracle owner remains responsible for any
reported residue; shutdown must not claim a clean drain on timeout.

Private guard/channel/task placement is implementer-owned. This requirement
fixes cross-owner lifetime and ordering, not a new Workflow queue or durable
cleanup service.

Built-in query arguments are closed `{sql, deadline_ms?, max_rows?, max_bytes?}`:
65,536 SQL UTF-8 bytes, deadline `1..=u32::MAX`, positive rows default 1000/max
10,000, bytes default 4 MiB/max 16 MiB, further constrained by remaining
deadline/Workflow budget. No visibility/freshness/source/class/path selector.
Cards arguments are one exact `{kind, name, version, space?, uid?}` CardRef.
Both consume captured context and expose no tenant/principal/endpoint selection.
Tool failures are redacted and non-retryable; unavailable declarations fail
before 202. A retry may repeat completed read calls and their separate audits.

## Ordered Implementation Scenarios

All following names are **planned** top-level tests in new
`crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs`. They require existing
`test-support` feature for controlled lifecycle/fault probes; use real
WyrdTestServer and shared/Rust client with local deterministic upstreams.
Do not replace the journey with an engine fixture. Every GREEN reruns prior
scenario tests; every REFACTOR retains them. The owning family lane provisions
Postgres, and each focused command explicitly does so below.

### Scenario 1 — Admission is authenticated, audited and bounded

**Behavior.** REQ-014/017/029–034/050: exact config defaults/validation and
writer/agent grant (admin wildcard; reader/runtime_admin denied); active exact
registered graph only, no Native or unavailable declared tools, no route/input
override, no secret exposure. Auth/audit/resolve/validation/input/capacity/binding
failure creates no run and causes no provider/tool call. Audit failure refuses
before acceptance; allow/deny audit is canonical and redacted.

**RED.** Add `admission_is_audited_and_side_effect_free_on_refusal`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=admission_is_audited_and_side_effect_free_on_refusal)"'`.
Expect absent route/owner/permission. Audit-append fault injection may use an
owner unit proof additionally when no cross-boundary safe injection exists;
record that reason, never weaken the real ordinary denial journey.

**GREEN.** Exact config/permission and production composition, canonical
fail-closed decisions, tenant-qualified bindings and graph suitability.

**REFACTOR.** Reuse current config/role/Caller/audit patterns; no authorization
cache or redundant policies.

### Scenario 2 — One preparation and one accepted job per scoped key

**Behavior.** REQ-030/034C/050 and AC-021: concurrent same-key requests share one
preparer/graph/run, creator receives 202 and replays 200; changed request conflicts;
tenant/principal scope isolates keys. Failure releases capacity/no cached failure;
creator disconnect and waiter cancel cannot strand/cancel work. Shutdown during
preparation wakes all waiters/releases exactly once. Lost acceptance response
recovers original run without duplicate observations/provider calls.

**RED.** Add `tracked_preparation_replay_and_disconnect`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=tracked_preparation_replay_and_disconnect)"'`.
Expect missing tracked preparation/idempotency. Synchronize at owned lifecycle
boundaries; count resolutions, accepted events and provider calls, not just equal output.

**GREEN.** Follow the exact spec create order/start signals/reservation promotion
and exact-once cleanup; no IO under lock and no persistent spawn-failure record.

**REFACTOR.** One cohesive lifecycle owner, no detached preparation or framework
of reservations/actors that changes the public statuses.

### Scenario 3 — Accepted authority is independent of request-token lifetime

**Behavior.** REQ-032A and AC-027: close submit connection, expire token, alter
grants/credential after acceptance; later calls still use original scopes and
attribution without token renewal. No scope widening; inaccessible Card/table/
model remains denied. New get/cancel/replay requests need valid current auth and
permission; replay cannot refresh context. Card mutation cannot alter pinned
graph; live gateway eligibility/credential/limits refusal still applies.

**RED.** Add `accepted_authority_outlives_submission_only`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=accepted_authority_outlives_submission_only)"'`.
Use controlled token issuance/time and deterministic step gates, not a five-minute
sleep. Expect absent capture boundary or ordinary request authentication leaking
into future accepted execution.

**GREEN.** Immutable trusted principal/scope/credential attribution/delegation
context bounded to run graph/deadline; no bearer/refresh/key secret retained.

**REFACTOR.** Reuse verified Caller projection and resource owners; no live grant
lookup substitute that changes approved accepted-job semantics.

### Scenario 4 — Built-in read tools preserve object authorization

**Behavior.** REQ-052 and AC-025/020: actual checked-in Agent declaration calls
both tools via existing Agent loop and outputs seeded query/Card results;
reads/repeated retry reads authorize and audit individually. Undeclared/unavailable
names, bad/unknown args, non-SELECT/oversized SQL, inaccessible Card/table,
foreign tenant, insufficient tool grants, bad terminal/result ceiling, deadline
and cancellation yield no unauthorized data and redacted non-retryable errors.

**RED.** Add `declared_tools_use_captured_scopes_and_owned_services`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=declared_tools_use_captured_scopes_and_owned_services)"'`.
Use real tenant Bifrost data and registered Cards, controlled model tool calls,
and canonical audit evidence. Existing query terminal fault seams are supporting
negative proof, not an alternate query engine.
Include an actually admitted forwarded Oracle query held after schema/midstream,
then cancel the Workflow before the original query deadline. Assert the remote
authenticated cancellation path, retained response, validated terminal outcome
(including residue rather than falsely claiming clean cleanup), tracked owner
join before Workflow capacity release, and continued sibling query availability.
Repeat with deadline expiry and transport/terminal loss: bounded completion
reports honest incomplete/unavailable/protocol settlement, never partial rows
as success. Use existing cluster/forwarding fixtures and lifecycle probes rather
than a fake stream that bypasses admitted ownership; prove selection through the
same focused scenario selector once the cases are added.

**GREEN.** Two run-bound AgentTool adapters with exact schemas/current query
bounds and authorized Cards/query services; no MCP transport inside the DAG.

**REFACTOR.** Reuse complete-result collection/permission owner, not duplicate
SQL floor, decoder, registry or tool loop; preserve existing MCP behavior.

### Scenario 5 — Gateway and external routes keep their owners

**Behavior.** AC-014–017/011A: server WyrdGateway uses in-process boundary with
isolated fallback, remaining duration and separate cancellation; all supported
dialect/capability combinations including Vertex work or reject pre-upstream.
Server ExtGateway goes directly through tenant-qualified bound egress, never
gateway credential/governance transport. Production origin/DNS/rebind/header/
redirect/timeout/response-limit failures disclose no secrets.

**RED.** Add `server_routes_keep_gateway_and_external_ownership`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=server_routes_keep_gateway_and_external_ownership)"'`.
Expect absent adapter/binding composition. Count direct versus gateway upstream
requests and assert separate canonical invocation decisions and redacted payloads.

**GREEN.** Bind exact graph routes and verified run context to approved dependencies;
server fixtures explicitly opt into existing local profile for mock loopback,
while production-screening negatives use production policy itself.

**REFACTOR.** Typed gateway-owned translation only; retain existing settlement
and credential owners, no recursive public HTTP or second ingress semantics.

### Scenario 6 — Races, eviction and shutdown produce complete terminal state

**Behavior.** REQ-018–023/030/034A–C/048/050 and AC-018/022: completion-versus-
cancel/deadline first terminal CAS wins; GET sees whole snapshots; no pending/
running terminal step; failure preserves peers/unstarted steps. Cancel signal
survives its request disconnect, drains Skald and releases capacity; later gateway
settlement cannot mutate run or retry. Tenant-first then global oldest eligible
terminal eviction, 24-hour lazy expiry and matching key removal; active/not-drained
work is never evicted. Shutdown cancels preparation/queued/running work under one
budget. Restart/non-owning replica returns common 404 and never resumes calls.

**RED.** Add `lifecycle_races_retention_and_shutdown`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=lifecycle_races_retention_and_shutdown)"'`.
Use deterministic gates/time and the real BoundServer shutdown path; expect absent
run lifecycle/drain. Cross-tenant/different-principal/expired/evicted/lost run
responses must be indistinguishable.
Hold an admitted forwarded read-tool response while cancelling or shutting down.
Prove the query owner survives Skald abort, preserves authenticated cancellation/
terminal semantics, and joins under the original query/shared shutdown deadline
before run capacity releases. If the bound expires, assert an honestly recorded
unconfirmed settlement and no false clean-drain claim. No tracked query owner or
retained response is silently abandoned by aborting the AgentTool waiter.

**GREEN.** Exact normative lifecycle and ordering; no independently reset shutdown
budget or terminal projection before owned tasks drain.

**REFACTOR.** One short-held state lock and shared terminal boundary; no actor,
durable recovery or cross-replica lookup workaround.

### Scenario 7 — Graph preparation and terminalization stay bounded

**Behavior.** REQ-017/045/050 and AC-028: exact step/edge/unique-body/counting
budgets (including duplicate edges), incremental byte rejection, stack-safe deep
graphs and mandatory terminal reserve. Reject oversized graph before provider/
tool calls without leaked reservation/retained oversized bodies. Full metadata/
error accounting always allows terminalization near budget. Concurrent ordinary
gateway, Cards and Bifrost requests remain serviceable during bounded preparation.

**RED.** Add `graph_and_snapshot_limits_preserve_sibling_services`;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_workflow_runs -E "test(=graph_and_snapshot_limits_preserve_sibling_services)"'`.
Use normal lane concurrency and deterministic admission/preparation gates,
not synthetic host load; expect missing admission/reserve. Assert exact graph,
input, step-result and full-run 413 codes and no oversized observable payload.

**GREEN.** Enforce counts before traversal, bytes before retention, pure bounded
stack-safe preparation outside state lock/reactor monopolization; release graph/
execution dependencies on terminal completion.

**REFACTOR.** Reuse native accounting/validation; no new CPU pool/scheduler or
transport-limit surrogate for actual graph limits.

## Acceptance Criteria

All exact scenarios pass against real server and shared/Rust client. HTTP bodies/
codes/status fields match spec, no permission or audit owner bypass, no bearer
retention or grant widening, no duplicate/untracked work, no IO under lock,
bounded graph and complete snapshots, serviceable siblings, proper shutdown
before gateway/Bifrost drain. Affinity/restart loss is documented, not concealed.

## Expected Write Set and Consumer Closure

`crates/shared/wyrd-runtime/src/{permission,builtin_roles}.rs`;
server config/state/boot/route composition and new cohesive Workflow owner;
Cards authorized-ref read seam; query bounded collector seam shared with MCP;
in-process caller and existing gateway interfaces; server testing composition/
controlled probes and PG tests/served OpenAPI. Only narrow existing-workspace
dependencies needed for approved interfaces; no new feature/crate/third-party
package or Workflow persistence migration. Builtin grants are updated at their
canonical definition/seed boundary, not hardcoded role-name checks in handlers.

## Verification and Evidence

Run exact selectors sequentially; then `mise run test:wyrd`,
`mise run test:shared`, `mise run test:principals:unit`,
`mise run test:principals:integration`, `mise run test:gateway:journey`,
`mise run test:bifrost`, `mise run codegen:check`,
`mise run check:client-tier`, `mise run check:tenant-isolation`,
`mise run check:unwrap-audit`, `mise run fmt`, `mise run lints`,
`git diff --check`. These support this mixed owner closure; final plan aggregate
is TASK-005. Exact PG target runs with test-support because required probes need
it, not `--all-features`. Config/docs/generated registrations have static and
served-document/regression proof, not fabricated RED. Assert all named planned
selectors select tests after addition; no claim of current runtime proof.

## Material Stop Conditions

Stop for changed authority lifetime/revocation, skipped resource checks/audit,
durable run machinery, expanded tool catalog/remote language surface, different
size/count defaults/accounting or gateway credential/settlement ownership.

## Authority Links

- [Approved Revision 10](../spec.md); TASK-001, TASK-002, TASK-003
- `AGENTS.md`; `architecture/agent-rules.md`
- `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`
- `architecture/bifrost-design.md` §§Query contract, Read audit and terminal
- `architecture/references/languages/{agent-harness,errors,spec-driven-development,implementation-execution,testing-workflows}.md`

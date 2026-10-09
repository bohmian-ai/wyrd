---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verification-closeout
spec_revision: 2
requirements: [REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, INV-001, AC-004, AC-005, AC-006]
depends_on: [TASK-001, TASK-002]
provenance: TASK-017-R4
---

# R4: Canonical support-desk closeout

## Outcome and Value

One support-desk example proves that an authenticated user can deploy and run
an Agent-backed Service, correlate every signal of a request, execute continuous
and realtime verification, and explain the result through public surfaces in
Rust, Python, and TypeScript.

## Owners, Scope, Consumers, and Prohibited Changes

- Extend existing Card registration, verification, gateway, telemetry, client,
  and example owners. Do not introduce replacement orchestration.
- Realtime results use Task 2's `ScribeOutbox`; this task does not create
  another result sink or publisher.
- Principal authority, Card attribution, saved-login behavior, and stock-client
  authentication come from Task 1 and are not reimplemented.
- Preserve server-owned durable behavior and language-parity through the shared
  client and generated contracts.
- Exclude Agent tools, structured output, Workflow correlation, new MCP tools,
  table evolution/drop controls, telemetry metrics/log setup, and UI work.

## Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Declared tables | Card registration and Bifrost table registration | card registration and Bifrost integration tests | A Service cannot declare and ensure a dataset | Extend registration to validate and ensure through the existing catalog owner | None |
| Binding activation | verification binding projector, scheduler, observation enqueue | card and Eval integration tests | Activation is required and unbound writers enqueue nothing | Permit no activation and reuse subject binding selection for unbound writers | None |
| Result correlation | result projection, Eval report, direct verification | result and route tests | Results carry verifier-run identity or are absent | Carry the application Run and send canonical batches through Task 2 | None |
| Gateway correlation and judging | gateway ingress/capture and judge invoker | gateway and verification journeys | Calls lack Run/Card correlation; judge bypasses gateway | Extend existing gateway request facts and use the existing gateway caller | A bounded principal cache is retained only if queued judging cannot recover caller authority otherwise |
| Agent invocation | existing Agent runtime and gateway adapter | gateway inference and workflow tests | No public one-Agent Run invocation | Expose the existing single-Agent gateway path through Run | None |
| Telemetry scope | existing SDK telemetry/export helpers and Run types | OTLP and observation journeys | No consistent setup/scope in all SDKs | Compose existing exporter and context mechanisms | Language runtime adapters only |

## Approach

1. Add declared Service tables and optional verification activation using the
   current Card, registry, catalog, scheduler, and observation owners.
2. Carry application-Run identity through continuous and direct result
   projection; record direct results through Task 2 without creating a durable
   verification run.
3. Add gateway Run/Card correlation, route judge and single-Agent invocation
   through the existing gateway, and preserve the initiating principal.
4. Complete the shared client and three SDK telemetry/Run-scope surfaces.
5. Build one checked-in support-desk example and matching three-language
   journeys that prove the complete public workflow and its key refusals.

## Required Contract Detail

The public types and surfaces are fixed by the spec:

- `ServiceTable { name, schema }`; `ServiceSpec.tables` defaults empty and is
  omitted when empty.
- `VerificationBinding.runs_on` becomes optional and is omitted when absent.
- `ExecuteVerificationRequest.run_id` is optional and omitted when absent.
- Gateway correlation uses `wyrd-run-id` (non-empty, at most 128 bytes) and
  `wyrd-card-uid` (a Card UID); both or neither. The UID is checked against
  the caller's signed Card scope, or the tenant registry for an unbound
  caller, before dispatch; a refusal is the fieldless scope denial.
- The gateway-call subject carries typed Run and Card UID identities while existing
  Workflow steps continue to pass none.
- Python, TypeScript, and Rust expose idiomatic `Run.invoke`, telemetry setup,
  and Run scope; `observe.verify` keeps its public shape while sending Run ID.

The example layout is retained:

```text
examples/support-desk/
  README.md
  service/
    support-desk.yaml
    support-agent.yaml
    support-prompt.yaml
    answer-quality.yaml
    judge-prompt.yaml
    no-refund-promise.yaml
  python/support_desk.py
  typescript/support-desk.ts
  rust/support_desk.rs
  rust/main.rs
```

Each implementation exports `deploy`, `serve`, `wait_for_verdicts`, and
`explain` (idiomatic casing only), plus a runnable main. `deploy` ensures the
four-field `vala.datasets.tickets` table, metadata capture, and model
deployment. `serve` handles 100 requests inside correlated Agent Runs. Ten
questions contain “refund” and receive a refund promise. `wait_for_verdicts`
waits through the public verification surface. `explain` uses only MCP
`bifrost.query` with a fresh token.

Required journey assertions are:

| Proof | Expected result |
|---|---|
| declared table | `tickets` exists with the declared four fields before Bifrost starts |
| Agent invocation | receiver answer returned; captured call has the application Run and Agent Card |
| span correlation | `support-desk.request` carries the Run; Eval observation shares its trace |
| joined explanation | observation, service record, span, gateway call, continuous verdict, and realtime verdict join for passing and failing requests |
| verdict totals | 100 `answer-quality` passes; 90 `no-refund-promise` passes; 10 failures |
| schema refusal | conflicting tickets schema returns `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` before writes |
| authorization refusal | `viewer` Agent invocation returns `WYRD_PERMISSION_403_DENIED_RBAC` before upstream IO |

The stable refusal contract also includes unsupported declared schema,
insufficient table permission, unavailable Scribe role, half/malformed gateway
correlation, unresolved/out-of-scope Card, invalid Agent invocation, denied
judge gateway access, missing judge model, and foreign telemetry provider.

## Ordered Implementation Scenarios

### Scenario 1 — Registration and activation support the example

**Behavior.** Service registration validates and ensures declared tables before
activation. Bindings without automatic activation are realtime-only, while an
unbound writer's observation activates matching subject bindings.

**RED.** Extend the existing Card-registration and Eval integration targets
with declared-table success/conflict, realtime-only binding, and unbound-writer
activation cases. Current contracts fail these cases.

**GREEN.** Extend the existing Card specs, registration/catalog workflow,
binding projection, scheduler, and observation enqueue selection.

**REFACTOR.** Share current schema validation and table-ensure behavior; remove
only branches made unreachable by optional activation.

### Scenario 2 — Every request and verdict is correlated and recorded

**Behavior.** Gateway calls accept authorized Run/Card correlation. Continuous
and realtime result rows carry the application Run, Verifier Card, subject,
and stable result identity. Direct execution returns independently and records
through `ScribeOutbox` without creating a verification run or dispatch.

A direct task Verifier judgment records exactly one `vala.verification.results`
row and no detail-table row: `implementation = task`, `execution_status =
completed`, its `passed`/`failed` verdict, exact `verifier_version` and
`subject_card_uid`, null `owner_card_uid`/`binding_id`/`trigger_identity`/
`window_*`, `source_record_id` set to the application Run record identity (the
`execution_id` when none is supplied), the execution interval as
`started_at`/`ended_at`, and the canonical JSON of its one `AssertionResult` in
the sensitive `details` column. No new table or record type is added. The
task3 merge already builds this row in `ResultPayloadBuilder::build`; this
scenario proves it and carries the application Run identity into it.

**RED.** Extend existing gateway-capture, result-projection, and direct-route
tests with correlated continuous and direct cases plus malformed, partial, and
unauthorized correlation refusals. Add a `verification::results` unit test that
builds a task result and asserts its single `vala.verification.results` batch
and row values, and extend the direct-route test to read the task row back by
`result_id = execution_id` and to refuse `details` without the payload
permission.

**GREEN.** Carry the existing typed identities through gateway facts and result
projection, and use Task 2's shared outbox for completed direct judgments.

**REFACTOR.** Remove obsolete documentation and assertions that direct
execution writes no result; add no second result-writing path.

### Scenario 3 — Agents, judges, and telemetry use the public runtime

**Behavior.** A Run invokes its Agent through the gateway; LLM judges use the
same gateway as the initiating principal; each SDK installs telemetry and
scopes spans to the Run and Card. Existing gateway authorization, accounting,
capture, and audit apply.

**RED.** Extend the existing verification, gateway, OTLP, and SDK observation
targets with gateway-owned judge calls, one-Agent invocation, Run-scoped span
attributes, token refresh, shutdown flush, and foreign-provider refusal.

**GREEN.** Expose and compose the existing gateway adapter, issuer, telemetry
exporter, and runtime context mechanisms through the shared client and SDKs.

**REFACTOR.** Delete the separate judge-provider plumbing once all judge calls
use the gateway; keep language-specific code limited to runtime integration.

### Scenario 4 — One support-desk journey proves the closeout

**Behavior.** Rust, Python, and TypeScript run the same checked-in support-desk
story against a real server. The journey deploys the declared table and Agent,
invokes requests, records telemetry and observations, produces continuous and
realtime verdicts, and queries joined evidence through MCP. It proves one
passing request, one policy failure, schema-conflict refusal, and
under-privileged invocation refusal.

**RED.** Add the same support-desk journey name and fixture story to the three
existing SDK integration targets. The story fails until Scenarios 1–3 and Tasks
1–2 are integrated.

**GREEN.** Add the smallest example, fixtures, public-SDK journey code, and mise
entrypoints required to run that story identically in all three languages.

**REFACTOR.** Consolidate only shared checked-in Card inputs and existing test
harness setup; keep each language example idiomatic and readable.

The three journey files are fixed at:

- `sdks/wyrd-sdk-python/tests/integration/test_support_desk.py`;
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/support-desk.test.ts`;
- `sdks/wyrd-sdk-rust/tests/integration/support_desk.rs`.

Their focused commands are:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_support_desk.py"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/support-desk.test.ts"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(/^support_desk::/)'"
```

### Scenario 5 — Repair shared Scribe delivery and membership ownership

**Behavior.** The process has one live membership snapshot poller through the
bounded Scribe outbox drain. Each registered Scribe or Oracle role owns one
readiness heartbeat through its own final shutdown; setting `ready=false`
does not stop that heartbeat. The shared outbox selects its local Scribe or
Scribe peer directly from process dependencies and does not require an Oracle
runtime to submit to a peer.

**RED.** Prove the Oracle-only pod can start before the Scribe-only pod, stage
on the shared outbox, discover the late Scribe, and deliver through shutdown.
Add focused checks for peer routing without an Oracle runtime, one poller on a
mixed-role pod, continued unready heartbeat after `deactivate`, and bounded
task cleanup on normal shutdown and abort. Reproduce each remaining Scribe,
Forge, and Oracle journey failure with tracing before changing its assertion.

**GREEN.** In `cluster/mod.rs`, make readiness heartbeat and snapshot poller
starters return handles that own their cancellation tokens and tasks. Keep
`RegisteredRole` as the existing cloneable identity value. Scribe and Oracle
store their own heartbeat handles; `Bifrost` stores the one process poller.
Stop heartbeats at final role shutdown/abort and the poller after role drain or
on Bifrost abort. Rename the engine/process token `role_shutdown` to
`process_drain`; remove the temporary per-role `membership` tokens and duplicate
pollers. Keep `deactivate` limited to publishing `ready=false`.

In `boot/mod.rs`, construct the outbox with the existing local Scribe
implementation when present, otherwise with the existing peer route using
process `ClusterRegistry` and `BifrostPeerTls`. In `scribe_outbox.rs`, store that
route directly in `ScribeSink`; remove `RouteSlot`, `ScribeRouteBinding`, and
Oracle-derived route selection. Remove late binding from `Bifrost::assembled`,
update `reaches_scribe` and the ownerless test shell, and adjust the direct
outbox test constructors. Preserve the current peer mTLS identity, channel
cache, `IngestCapture` RPC, batching, stable retry IDs, and accepted in-memory
loss window.

For tests that count Scribe rows, Forge membership, queue slots, or telemetry,
assert the intended table or the actual process-wide aggregate. Bind one-shot
faults to the intended operation when needed. Keep audit decisions on the
shared outbox; do not add a general audit opt-out. Fix each independently
diagnosed live-rewrite or Oracle failure at its own root cause.

**REFACTOR.** Delete duplicate lifetime state and late route binding. Add no
new TLS pool, peer RPC, route config, or delivery system.

### Scenario 6 — Test Postgres bootstraps with its container client

**Behavior.** The shared Postgres test wrapper runs `roles.sql` with `psql`
from the Compose container it started. A host `psql` 14 or no host `psql` does
not prevent role bootstrap or the requested test command. Bootstrap errors
stop the wrapper before that command and still tear down its Compose project.

**RED.** Extend `scripts/postgres/test-contract.sh` to reject host `psql` for
bootstrap, assert that the container client receives `roles.sql` and the
configured role passwords, and prove failure propagation and cleanup. Run a
repository-managed Postgres lane with host `psql` 14 first on `PATH` or a fake
host `psql` that refuses bootstrap. Preserve the idempotency proof in
`scripts/postgres/test-roles.sh`, whose own `roles.sql` rerun currently uses
host `psql`.

**GREEN.** In `scripts/postgres/with-test-postgres.sh`, feed the local
`roles.sql` file to `docker compose exec -T postgres psql` through stdin;
keep the existing role variables, add `ON_ERROR_STOP`, and preserve the
exported host URLs and isolated Compose lifecycle. Make the roles test's
rerun use the container client as well. Do not change the deployment contract
of `roles.sql` or require a host `psql` for wrapper bootstrap.

**REFACTOR.** Remove the host-version `PATH` workaround from closeout
instructions once the contract and roles tests pass.

## Acceptance Criteria

- AC-004, AC-005, and AC-006 pass.
- No task-local result sink, gateway transport, auth model, or telemetry
  provider duplicates Task 1 or Task 2 ownership.
- Direct execution stages exactly one result through `ScribeOutbox` and creates
  no durable verification run, observation, or Operator dispatch.
- A direct task Verifier judgment is recorded as one `vala.verification.results`
  row (`implementation = task`) whose `AssertionResult` is in `details`, read
  back by `result_id = execution_id` and refused without the payload
  permission; no detail table or new record type exists for it.
- The support-desk journeys use the same story and observable outcomes in all
  three SDKs and no test-only publication hook.
- Public contracts regenerate cleanly and documentation matches the proved
  workflow.
- A late Scribe joins an Oracle-only pod's live membership view and receives
  its staged outbox writes before the bounded shutdown drain ends; peer
  routing has no Oracle runtime dependency.
- A mixed-role pod has one process membership poller and one heartbeat per
  registered role. `deactivate` preserves the unready heartbeat until final
  role shutdown. Normal shutdown and abort stop every owned task.
- Scribe, Forge, and Oracle journey lanes and the final broad gate pass with
  audit, gateway capture, and Verifier results still using the shared outbox.
- The Postgres wrapper and role-idempotency test run `roles.sql` with the
  container client; a host `psql` 14 cannot block bootstrap, and SQL failure
  prevents the requested command while cleanup still runs.

## Expected Write Set and Consumer Closure

- Contracts/generated surfaces: Service and Verifier specs, verification and
  correlation requests, generated schemas, OpenAPI, Python stubs, and
  TypeScript declarations.
- Server: Card registration, Bifrost table ensure, verification binding/
  observation/result/direct owners, gateway ingress/invocation/capture, judge
  integration, and server state only where caller authority is retained; the
  remediation also touches `boot/mod.rs`, `state.rs`, `scribe_outbox.rs`, and
  process shutdown.
- Cluster runtime: `ClusterRegistry` task handles in
  `vala-bifrost-redux/src/cluster/mod.rs` and their direct tests.
- Shared test infrastructure: `scripts/postgres/with-test-postgres.sh`, its
  contract and role tests, and any related mise task description.
- Shared client and SDKs: Run invocation, verify correlation, telemetry setup
  and scope, plus idiomatic Rust/Python/TypeScript projections.
- Evidence: existing focused server tests, `fixtures/cards/support_desk`, one
  support-desk journey per SDK, the example and its concise documentation, and
  existing mise example/journey membership.

## Verification and Evidence

- `mise run test:cards:integration`
- `mise run test:principals:integration`
- `mise run test:gateway:journey`
- `mise run test:bifrost:journey:observe`
- `mise run test:bifrost:journey:otlp`
- `mise run test:bifrost:journey:mcp`
- `mise run check:examples`
- `mise run codegen:check`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `mise run fmt`
- `mise run lints`
- `mise run py:lints`
- `mise run py:typecheck`
- `mise run docs:check`

Use focused exact selectors and the Scribe, Forge, and Oracle journey leaves
during repair. Run `mise run test:postgres:contract` and
`mise run test:postgres:roles` for the wrapper change. This remediation
crosses shared cluster runtime, server boot, shutdown ownership, and test
infrastructure, so `mise run gate` is the final aggregate. Do not repeat its
component lanes as separate final gates unless a required lane is outside
that aggregate.

## Material Stop Conditions

- Registration cannot ensure a table in supported deployment topologies using
  an existing server-to-Bifrost boundary.
- A queued judge cannot recover the initiating principal without introducing a
  new durable credential or weakening authorization.
- Application-Run identity cannot reach result projection without changing the
  documented result meaning beyond this specification.
- A first-class language runtime cannot propagate Run scope through its native
  async context or a required stock client lacks the needed extension hook.

## Authority Links

- `../spec.md` revision 2: REQ-006..010, AC-004..006
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`;
  `architecture/bifrost-design.md`;
  `TESTING.md` (definitive Wyrd guide for test ergonomics,
  understandability, structure, ownership, and lane selection)

## Implementation Evidence (r4 closeout)

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Late Scribe receives an Oracle-only pod's staged writes; peer routing has no Oracle dependency | `scribe_outbox.rs` selects `Local`/`Peer` at boot from `ClusterRegistry` + `BifrostPeerTls`; `RouteSlot`/late binding removed | `scribe_outbox::tests::a_peer_route_needs_no_oracle`, `an_unreachable_route_drops`; `query::oracle_only_pod_retains_audit_staged_before_its_drain`; `test:bifrost:journey:server` | PASS |
| One process membership poller, one heartbeat per role; `deactivate` keeps the unready heartbeat; shutdown/abort stop every task | `cluster/mod.rs` `ClusterTask`; poller owned by `Bifrost`; `state.rs` `process_drain`, `stop_role_task` | `cluster::tests::stop_joins_the_task_once`, `stop_aborts_a_task_past_its_deadline`, `abort_and_drop_end_the_task`; `server::owner_inspection::unready_roles_heartbeat_until_teardown_ends_membership_tasks` | PASS |
| Gateway Run correlation by Card UID: paired `wyrd-run-id` + `wyrd-card-uid`; UID checked against signed scope or tenant registry before dispatch; authorized UID captured; `wyrd-card-ref` and CardRef lookup removed, no alias | `wyrd-spec` `CARD_UID_HEADER`; `ingress.rs` `requested_subject`; `invocation.rs` `GatewayCallSubject { run_id, card_uid }`, `attribute`; `capture.rs`; client `PublicWyrdGatewayCaller::with_subject(run, card_uid)`, `Run::invoke` | `ingress::tests::subject_headers_decode_only_as_a_valid_pair` (malformed); `pg_invocation_tests::gateway_correlation_is_authorized_before_dispatch_and_captured` (out-of-scope, unregistered, capture); `workflow_transport::public_gateway_call_carries_run_and_card_uid`; `pg_openapi_contract::gateway_inference_ingress_publishes_the_fallback_header`; support-desk journeys (`call_card_uid`) | PASS |
| Public scope error | `CardScopeDenied` fieldless; `<server-validation>` removed; mappings and contracts regenerated | `codegen:check`; wyrd-client/gate error mapping tests | PASS |
| Scribe, Forge, Oracle lanes pass with audit, capture, and Verifier results on the shared outbox | Table-scoped Scribe faults/backlog; aggregate-meaning metric assertions; table-scoped Forge commit-uncertainty fault; Forge self-reclaim on supervised restart; follower admit-after-settle guard; cancel-safe cleanup pause; grant-order observation | `test:bifrost:journey:scribe` 28/28; `test:bifrost:journey:forge` 22/22; `test:bifrost:journey:oracle` 50/50 | PASS |
| Postgres wrapper uses the container client | `with-test-postgres.sh`, `test-contract.sh`, `test-roles.sh` | `test:postgres:contract`; `test:postgres:roles`; `mise run gate` | PASS |
| Final broad gate | — | `mise run -c gate` exit 0 (2026-10-09), no FAIL lines | PASS |

### Diagnoses

Common origin: `70a6f4a4e` made `vala.system.audit_log` an always-active second
table on the same Scribe, which every audited call writes.

- **Scribe pod-global faults and probes.** Symptom: one-shot persistence faults
  and staging backlog fired or counted on `audit_log`. Cause: `PersistenceFaults`
  and `StagingAssembler::backlog` were process-wide. Fix site:
  `scribe/persistence.rs` `target_table_for_test`, `assembly.rs` `backlog_of`;
  all fault callers route through `targets()`.
- **Scribe pod-wide counters.** Symptom: telemetry/budget equalities off by audit
  rows. Cause: assertions treated process counters as table counts and read
  before the outbox settled. Fix site: the tests; settle with
  `await_audit_retained`, assert aggregate meaning, prove the table count by a
  strict read.
- **Forge membership and Shutdown attempt.** Symptom: schedule sizes +1 and a
  `Shutdown` failure on restart. Cause: `audit_log` joins recovered membership;
  a stopping node's pre-effect attempt is a shutdown, not a failure. Fix site:
  `production_closeout.rs` (audit-aware sizes; `failures()` excludes shutdown).
- **Forge uncertainty fault.** Symptom: rewrite never saw its injected commit
  uncertainty. Cause: process-global fault consumed by an `audit_log` commit.
  Fix site: `forge_harness.rs` `CommitUncertaintyCatalog::target_table`.
- **Forge restart reclaim (production).** Symptom:
  `failed_worker_restarts_while_the_api_serves` waited out its 2-minute bound.
  Cause: a supervised restart keeps its owner id but `reclaim_expired_attempts`
  only took expired claims, so readiness waited for the 15-minute lease. Fix
  site: `forge_tasks.rs` reclaims `claimed_by = previous_owner`;
  `worker.rs` `drain_recoverable_work` passes its own owner. Proof:
  `pg_forge_tasks::previous_owner_reclaims_its_unexpired_attempt`.
- **Oracle admit-after-settle (production).** Symptom:
  `selected_peer_failure_is_terminal` leaked a follower graph. Cause:
  `GraphLease::settle` copied an empty attempt set while a reused lease
  admitted a late `SetPlan`. Fix site: `analytical.rs` marks and copies under
  the attempt lock; `admit_attempt` refuses once settled. Proof:
  `oracle::analytical::tests::a_plan_after_settlement_admits_no_attempt` fails
  without the guard, passes with it.
- **Oracle cleanup pause.** Symptom: `transport_drop_retains_running_status_until_cleanup_joins`
  saw the running entry retired while paused. Cause: the leader's `settle`
  consumed the one-shot pause, was dropped by the transport, and `reclaim`
  passed straight through. Fix site: `AnalyticalCleanupPause::hold` re-arms
  when dropped unreleased; `wait_entered` registers before reading. Independent
  diagnostician concurred.
- **Oracle queue order.** Symptom: `queued_tenants_are_granted_fifo_and_rotated`
  saw `second` before `first-older` in the lane. Cause: the test read
  completion order; production releases the slot before the terminal frame, so
  completions race. Fix site: the test now parks each grant on a schema stall
  holding the only unit and records grant order. Production ordering unchanged
  (independent diagnostician: correct).
- **Forge failover inherited state.** Symptom:
  `one_leader_failover_volatile_state` saw the successor's schedule sizes
  `(1,1,0)`, not `(0,0,0)`. Cause: the stopped leader's shutdown drained audit
  rows, so the successor's first pass recorded a real `audit_log` commit; the
  claim was wrong, not production. Fix site: the test asserts that the rewrite
  key has no inherited track and that only the audit track is present.
  Independent diagnostician concurred.
- **Oracle shutdown memory read (production).** Symptom:
  `memory_refusal_preserves_oracle_health_and_next_query` panicked "Oracle
  shutdown retained admission, resource, or peer state". Cause:
  `OracleAdmission::shutdown` waited only for `active_queries == 0`, then read
  shared pool bytes once. A query's slot returns before its torn-down child
  tasks drop their reservations. Fix site: `GovernedMemoryRoot` notifies on
  every shrink, and `shutdown` waits for both counts to reach zero (both wakeups
  enabled before each check), still bounded by the deadline. The residual
  warning now includes memory. Every Oracle shutdown routes through this owner.
  Independent diagnostician identified it.
- **Rust SDK journey selected identity-lane tests.** Symptom: `verify:rust-sdk`
  failed `signed_in_development::saved_login_completes_the_workflow_past_token_expiry`
  with an OIDC discovery 503. Cause: its `--run-ignored=all` filter excluded
  only `saved_user_auth::`. The Keycloak-dependent `signed_in_development::`
  tests then ran after `test:identity:journey` had removed Keycloak. Fix site:
  `mise.toml` `verify:rust-sdk` excludes both modules, which the identity lane
  owns. Python (marker) and TypeScript (file exclude) were already excluded.
  Independent diagnostician identified it.
- **UI replacement settings replica expiry.** Symptom:
  `production-auth.integration.test.ts` "production provider replacement
  settings" got no redirect on replica 0 after replica 1 ended the session.
  Cause: each BFF replica caches its own access token. JWT `exp` has
  whole-second resolution, so replica 0's token (issued a second later under
  load) outlived replica 1's. The test assumed both expire together. Fix site:
  the test polls replica 0 with `expectSessionEnds` as it already did for
  replica 1. BFF behavior unchanged. Independent diagnostician identified it.

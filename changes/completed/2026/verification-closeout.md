# Verification closeout

- Change: `verification-closeout`
- Specification: `SPEC-verification-closeout`, approved revision 3
- Completed: 2026-10-09
- Reviewed target: `b987cf910` (base `7f79fb341` for the TASK-003 closeout)
- Completion authority: explicit human approval by the repository owner on 2026-10-09; no final change-review `PASS` was recorded
- Delivery reference: not supplied

This change closes the remaining TASK-017 verification gaps. It brings one principal-centred access model, one non-blocking write path into Scribe, and one canonical support-desk journey that proves the whole workflow in Rust, Python and TypeScript.

## Shipped behavior

- **Principals and roles.** Role assignments are subresources of `principal_id`, and the Card-addressed grant route is gone with no alias.
  - `GET /v1/principals` gives exact-match discovery with UUIDv7 keyset paging.
  - Grant and revoke are idempotent `PUT`/`DELETE` calls on `/v1/principals/{principal_id}/roles/{role}`.
  - User assignments carry an `idp` or `direct` source, and IdP sync replaces only `idp` rows.
  - Exactly four built-in roles ship, nested `viewer` ⊂ `workload` ⊂ `editor` ⊂ `admin`. Card-bound Service and Agent principals receive `workload` on first projection only.
  - The CLI and all three SDKs expose the same principal surface.
- **Frictionless authenticated operation.** Unbound users and tenant admins may attribute observations to any registered observation-target Card in their tenant; Card-bound principals keep their declared scope.
  - SDK token adapters call `access_token()` on every request.
  - Local development uses only the setup admin key, and the enterprise flow uses a saved login.
  - No journey issues a Card key or calls the test-only Bifrost flush.
- **Task Verifiers.** `VerifierImplementation::Task(TaskVerifierSpec)` wraps one flattened `Assertion` or `LlmJudge` task.
  - Explicit invocation requires `verifier:run` on that Verifier's UID, or an all-Verifiers grant.
  - Every allowed and denied decision is audited.
- **One Scribe outbox.** Every server-internal Scribe write enters `ScribeOutbox = Outbox<ScribeSink>` as a logical `ScribeWrite`. That covers audit decisions, gateway captures, and queued and realtime Verifier results.
  - The sink groups each tenant slice by destination into Arrow frames with content-derived batch ids, and routes them to the local or peer Scribe.
  - Retries resubmit the identical slice. Terminal rejections are logged, counted and consumed.
  - `vala.audit_staging`, `AuditPublisher`, the audit hash-chain fields and `wyrd.verifier_run_results` are retired.
  - Observation-run enqueueing remains its own PostgreSQL-backed `Outbox<ObservationRunSink>`.
- **Verification results and correlation.** `VerificationBinding.runs_on` is optional; absence means explicit-only execution.
  - An unbound writer's Eval observation activates matching bindings.
  - Result rows carry the application Run as `run_id`.
  - Services declare Bifrost tables, which are validated and ensured before activation and never dropped.
  - Gateway ingress requires the paired `wyrd-run-id`/`wyrd-card-uid` headers. It authorizes the Card UID against the principal's scope or the tenant registry before upstream IO, and captures calls by Card UID.
- **Gateway-owned execution.** LLM judges call the gateway as the evidence-producing principal, with no fallback principal or separate provider registry. `run.invoke` runs one tool-free Agent with string variables.
- **Telemetry.** `start_telemetry` installs OTLP tracing once using refreshing credentials. It refuses a foreign provider with `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`. Run scope uses native context propagation.
- **Support desk.** `examples/support-desk` exports `deploy`, `serve`, `wait_for_verdicts` and `explain` in each language. Each SDK journey imports that example; the workload is 100 requests, 10 of which mention a refund.
  - The run yields 100 continuous `answer-quality` passes, 90 realtime `no-refund-promise` passes and 10 failures.
  - `explain` joins the observation, service record, span, gateway call and both verdicts through MCP `bifrost.query`.
  - `deploy` refuses unless the exact gateway provider/model deployment exists, and names the remediation when it is missing.

## Lasting invariants

- Tenant isolation, permission checks, audit coverage and non-enumerating identity failures hold on every path.
- Producers never wait for Scribe, and a Scribe failure never changes the originating operation.
- Before Scribe acknowledges a write, it lives only in process memory. It may be lost on abrupt process death or when the shutdown deadline expires.
- Only `ScribeSink` chooses the delivery route and owns retry. Only existing Scribe ingress admits items and bytes.
- Oracle shutdown drains only Oracle-attributed memory, meaning governed query bytes plus Oracle infallible headroom. Sibling roles' memory never delays it.
- A Forge worker fast-reclaims its own unexpired Claimed, Running or Prepared attempts only when its previous invocation joined every plan and heartbeat. After an interrupted invocation, those attempts are recovered once their leases expire.

## Material decisions

- The in-memory, best-effort Scribe path replaced PostgreSQL staging (revision 2). A durable outbox is a separate future change, justified only by operational evidence.
- Single-task Verifiers and Verifier-UID-scoped `verifier:run` authority were approved in revision 3.
- Gateway and observation correlation identify the Card by UID rather than by `CardRef`. The active design documents were updated to match.

## Revisions and deviations

- Spec revisions 1 to 3 were approved on 2026-10-08.
- Task reviews of TASK-001 and TASK-003 returned `FIX_REQUIRED` several times, and the remediations R1 and R2 were implemented.
- The last re-review of candidate `e2d324a91` validated no production defect. It asked for a stronger FIND-7 test proof:
  - interrupt startup only once a reconciliation heartbeat or plan is live;
  - observe a completed recovery pass, not a 2-second window.
- The repository owner approved completion without that proof correction. The existing test `interrupted_startup_waits_for_its_prepared_lease` stays as the recorded evidence.

## Acceptance closure

| Criterion | Evidence |
|---|---|
| AC-001 principals and roles | `pg_principal_roles` integration tests; SDK principal journeys |
| AC-002 local and saved-login flows | `sdks/wyrd-sdk-rust/tests/integration/local_development.rs` and its Python and TypeScript counterparts; `test:identity:journey` |
| AC-003 shared Scribe outbox | `scribe_outbox` and `outbox` unit tests; `pg_verification_runtime`; `test:bifrost:integration:server` and `test:bifrost:integration:redux` |
| AC-004 declared tables, bindings, run identity, correlation, judges | `pg_verification_routes`, `pg_verification_runtime`, gateway `pg_invocation_tests` |
| AC-005 support-desk journey in three SDKs | Rust, Python and TypeScript `support_desk` journeys |
| AC-006 contracts, boundaries, docs, examples | `codegen:check`, `check:deps`, `check:tenant-isolation`, `docs:check`, `check:examples`, `fmt`, `lints` |
| AC-007 task Verifier authority | Verifier YAML and real-time endpoint grant/refusal tests |

## Current owners

- [Wyrd design](../../../architecture/wyrd-design.md), [Bifrost design](../../../architecture/bifrost-design.md), [security posture](../../../architecture/wyrd-security-posture.md), and [testing guide](../../../TESTING.md).
- [Principal routes](../../../crates/wyrd/wyrd-server/src/components/principals/routes.rs) and [principal role tests](../../../crates/wyrd/wyrd-server/tests/integration/pg_principal_roles.rs).
- [Scribe outbox](../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs) and [runtime outbox](../../../crates/shared/wyrd-runtime/src/outbox.rs).
- [Verification runner](../../../crates/wyrd/wyrd-server/src/verification/runner.rs), [Verifier spec](../../../crates/wyrd-spec/src/card/verifier.rs), and [Service spec](../../../crates/wyrd-spec/src/card/service.rs).
- [Gateway ingress](../../../crates/wyrd/wyrd-server/src/components/gateway/ingress.rs) and [gateway capture](../../../crates/wyrd/wyrd-server/src/components/gateway/capture.rs).
- [Resource governor](../../../crates/vala/vala-bifrost-redux/src/resources.rs), [Oracle admission](../../../crates/vala/vala-bifrost-redux/src/oracle/admission.rs), [Forge worker](../../../crates/vala/vala-bifrost-redux/src/forge/worker.rs), and [Forge task SQL](../../../crates/vala/vala-sql/src/queries/forge_tasks.rs).
- [Support-desk example](../../../examples/support-desk) and journeys: [Rust](../../../sdks/wyrd-sdk-rust/tests/integration/support_desk.rs), [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_support_desk.py), and [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/support-desk.test.ts).

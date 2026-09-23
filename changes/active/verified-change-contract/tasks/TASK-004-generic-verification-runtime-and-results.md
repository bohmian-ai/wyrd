---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 33
requirements: [REQ-061, REQ-062, REQ-063, REQ-078, REQ-079, REQ-081, REQ-082, REQ-085, REQ-086, REQ-087, REQ-096, REQ-097, REQ-100, REQ-115, REQ-119, REQ-121, REQ-122, REQ-135, REQ-136, REQ-137, REQ-145, REQ-146, INV-004, INV-007, INV-010, INV-011, AC-013, AC-015, AC-020, AC-023, AC-024, AC-028, AC-030]
depends_on: [TASK-002, TASK-003]
---

## Outcome and Value

One supervised, durable VerificationRuntime admits scheduled or manual work,
claims and retries exact runs, publishes immutable result batches remotely
through Bifrost under a tenant SYSTEM writer, settles only after required ACKs,
and exposes binding/run status through one HTTP/SDK/MCP contract. Drift and Eval
plug into this closed runner without owning claims, results, dispatch, or
process-local state.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-sql` owns run/dispatch control persistence; `wyrd-server` owns the
runtime, HTTP handlers, audit boundaries, supervision, limits, health, and
telemetry. Existing auth crates and the tenant machine-principal store own the
internal `system` principal and its token issuance.
`wyrd-client::Bifrost`, Gate, and Scribe own remote result admission/durability;
Oracle remains the read path. Shared client plus SDK/MCP surfaces project the
typed status/manual API.

Do not add a broker, direct/local Scribe write, result HTTP endpoint,
process-local run registry, new RBAC permission, cross-table recovery protocol,
or claim atomic analytical visibility. Internal mechanics do not emit auth
audit rows.

## Approach

1. Add durable run/dispatch claim, lease, retry, settlement, cursor, and status
   operations with token fencing and UUIDv7 identities.
2. Provision and verify one credentialless UUIDv7 SYSTEM tenant principal per
   tenant through the existing identity store and issuer; enforce Gate's closed
   table matrix and every public-path refusal.
3. Supervise one bounded runtime with scheduler, generic runner, and later
   Operator worker capability slots, health, telemetry, and drain behavior.
4. Publish reusable result/detail batches through the existing Bifrost facade
   and preserve sealed payload identity for unacknowledged retries.
5. Expose the three Verification operations and their shared-client/SDK/MCP
   projections with normal permissions, idempotency, tenancy, and audit.

## Ordered Implementation Scenarios

### Scenario 1 — Provisioned SYSTEM can write only result tables

**Behavior.** Tenant provisioning creates one stable credentialless UUIDv7
`system` principal in the existing tenant machine-principal store. It is the
sixth wire/runtime principal kind but remains internal-only and absent from
public principal listings and lifecycle. Per publication, the existing tenant
issuer mints a five-minute-or-shorter token with no credential, delegation
chain, root Card, or roles and exactly one UID-bearing Verifier scope.
Verification rejects every malformed or public creation, credential, refresh,
workload, delegation, impersonation, or management path. Gate permits only
that principal on the three result tables and denies it everywhere else;
every other kind, including wildcard admin, is denied those tables. Internal
minting emits no authorization audit; Gate records exactly one canonical write
decision.

**RED.** Add provisioning, public-list omission, create/get/update/delete and
credential/refresh/workload/delegation refusal, token verification, Gate
matrix, audit-cardinality, scope-forgery, and tenant-forgery tests. The current
principal model and Gate lack this path.

**GREEN.** Extend the existing principal kind, tenant machine-principal store,
tenant issuer, token verifier, and Gate owners; reuse the existing JWT format,
`bifrost_record:write`, signed scope, and canonical Gate audit.

**REFACTOR.** Delete any parallel issuer, identity store, special transport
credential, or table permission; SYSTEM remains one intrinsic internal tenant
principal path.

### Scenario 2 — Manual requests durably enqueue and preserve requester identity

**Behavior.** Authenticated `POST /v1/verification/runs` accepts one bounded
Drift window and binding/direct target, audits `evals:run` plus exact scope,
uses normal Idempotency-Key semantics, returns `202 {run_id}`, and freezes the
caller separately from nullable binding owner/ID. Invalid windows, targets,
tenants, scope, or readiness fail before enqueue.

**RED.** Add handler/SQL and real HTTP cases for binding/direct success,
request replay/conflict, identity columns, and every refusal.

**GREEN.** Compose existing caller, audit, idempotency, Card/binding resolution,
and run insert in the owning transaction.

**REFACTOR.** Share one enqueue path with scheduler/Eval origins while keeping
origin-specific required fields exhaustive.

### Scenario 3 — Scheduler creates one exact window without catch-up

**Behavior.** One scheduler task claims a due active/ready binding briefly,
creates at most one unique run for the stored half-open window, and advances
the cursor in the same transaction. Inactive/unready/missed occurrences create
no run and are never backfilled; the next future boundary becomes the cursor.
Analysis runs after the lock is released.

**RED.** Add controllable-clock concurrency tests for duplicate ticks,
inactive/unready/missed periods, restart, cursor movement, and fixed windows.

**GREEN.** Reuse the existing skip-locked/fenced SQL patterns and approved
activity/readiness queries.

**REFACTOR.** Keep cron calculation synchronous and IO only in the owner that
claims/persists work.

### Scenario 4 — Claims, retries, and terminal states survive restart

**Behavior.** The runner acquires global/per-tenant permits before claim,
settles only with its lease token, reclaims expiry, retries engine failures with
the same run/input, and distinguishes completed verdicts from cancelled,
timed_out, and errored execution. Restart never duplicates a run or loses
visible state.

**RED.** Add lease expiry, stale settlement, retry exhaustion, cancellation,
permit fairness, and restart cases against Postgres.

**GREEN.** Implement bounded claims on the durable store and one typed closed
dispatch to the two real engines.

**REFACTOR.** Centralize lifecycle transitions on the runtime owner; engines
return typed outcomes and do not mutate run rows.

### Scenario 5 — Result publication requires every non-empty ACK

**Behavior.** Every non-empty detail batch precedes the summary; all rows share
result event time and exact run/Verifier/subject/owner/binding identities. Zero
details sends no empty batch. Only all required Scribe ACKs permit completed
settlement and failed-binding dispatch insertion. Identical unacknowledged
sealed payload retry deduplicates; fresh writes do not. Partial rows may remain
visible but never authorize dispatch.

**RED.** Add multi-server gRPC/Scribe tests for success, zero-detail, duplicate
sealed replay, fresh batch, detail-success/summary-failure, crash, and no-local-
Scribe worker.

**GREEN.** Use `wyrd_client::Bifrost` and preserve sealed batch/table/bytes
inside the bounded attempt until ACK or terminal failure.

**REFACTOR.** Keep analytical payload construction separate from transport
mechanics and add no repair coordinator.

### Scenario 6 — Status is one typed control-plane projection

**Behavior.** Binding GET reports exact identities, active/readiness reasons,
schedule and last activation/run. Run GET reports execution/requester/result
pointer/error and independent dispatch statuses without copying verdict/detail
data from Bifrost. Rust/Python/TypeScript and MCP call the same shared client;
Cards and Bifrost retain baseline/result reads.

**RED.** Add HTTP and first-class client/MCP journeys for polling, permissions,
cross-tenant denial, and direct versus binding delivery state.

**GREEN.** Add typed `wyrd-spec` wire shapes, handlers, shared Verification
client capability, thin language projections, and MCP tools.

**REFACTOR.** Remove any duplicate result/status transport or language-owned
state machine.

### Scenario 7 — Runtime limits, audit, health, and shutdown are enforced

**Behavior.** Global 16/per-tenant 4 Verifier capacity prevents one tenant
starving another. Shutdown stops claims, drains 30 seconds, then makes durable
work reclaimable; a crashed required task restarts and health is degraded until
present. Metrics/traces expose bounded queue/work/attempt/failure/latency.
Only public/Gate permission decisions audit; claims, retries, commits, and
worker mechanics do not.

**RED.** Add deterministic permit, shutdown/restart, supervisor, telemetry,
and audit-cardinality tests.

**GREEN.** Compose existing server supervision, bounded permit, clock,
telemetry, and audit owners around the durable workers.

**REFACTOR.** Keep one runtime owner and no process-local work registry.

## Acceptance Criteria

- Status/verdict independence and nullable direct-run ownership match the spec.
- Remote result writes obey SYSTEM/Gate/Scribe identity and ACK semantics.
- SYSTEM reuses the existing tenant identity/JWT machinery, stays unreachable
  through public principal and credential operations, and creates no duplicate
  issuance audit.
- Manual/scheduled work, claims, retries, concurrency, shutdown, and audit
  satisfy `AC-015`, `AC-023`, `AC-028`, and runtime portions of `AC-030`.
- Drift/Eval/Operator tasks can consume this runtime without a second lifecycle.

## Expected Write Set and Consumer Closure

Likely owners: `wyrd-spec` verification/auth/error contracts, `wyrd-runtime`
principal/permissions, shared tenant auth issue/verify/provisioning,
`wyrd-sql` migrations and
queries, `wyrd-server` routes/runtime/state/health, `vala-bifrost-redux` Gate,
shared Bifrost/Verification clients, SDK bindings, MCP tools, and production-
shaped server/Bifrost tests.

## Verification and Evidence

```bash
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:shared
mise run test:wyrd
mise run test:vala
mise run test:bifrost:integration:server
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:server
mise run test:bifrost:journey:mcp
mise run test:e2e
mise run py:test:integration
mise run py:typecheck
mise run ts:test:integration
mise run ts:typecheck
mise run codegen:check
mise run check:tenant-isolation
mise run check:client-tier
mise run check:unwrap-audit
mise run fmt
mise run lints
git diff --check
```

New named tests require exact focused commands after their targets and names
exist; do not invent selectors during implementation.

## Material Stop Conditions

Stop for changed result schemas, stronger atomicity/recovery promises, new
permissions, another run/status API, direct Scribe access, a second identity or
token hierarchy, a public SYSTEM lifecycle or credential, different
concurrency limits, or changed audit cardinality.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `changes/active/verified-change-contract/architecture/logic/table_schema.md`
- `architecture/wyrd-security-posture.md`
- `architecture/bifrost-design.md`
- `architecture/references/languages/implementation-execution.md`
- `AGENTS.md`

## Implementation Evidence

Commits: `9431906ee..HEAD` on `verified-change-contract` (SYSTEM principal and Gate
matrix; `verifier_runs`/`operator_dispatches` and wire types; the supervised
`VerificationRuntime`; HTTP/`wyrd-client`/Rust-Python-TS SDK/MCP surfaces; and
the closing gap commits for sealed-batch replay proof, the 30-second drain,
SYSTEM public-route proof, and configuration docs).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Status/verdict independence and nullable direct-run ownership match the spec | `crates/wyrd/wyrd-sql/migrations` (`verifier_runs`, `operator_dispatches`), `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs`, `crates/wyrd/wyrd-server/src/components/verification/{routes,service}.rs`, `crates/wyrd-spec` verification wire types | `wyrd-sql` `pg_verifier_runs` (`test:sql`); `pg_verification_routes::manual_runs_enqueue_replay_and_read_back` (requester frozen apart from nullable owner/binding) (`test:cards:integration`); `pg_verification_runtime::cancellation_and_deadline_settle_without_a_verdict` | PASS |
| Remote result writes obey SYSTEM/Gate/Scribe identity and ACK semantics | `crates/wyrd/wyrd-server/src/verification/{publisher,results,runner}.rs` publish through `wyrd_client::Bifrost::write_batch`, which seals each batch once and resends the identical table/batch ID/frame while its ACK is ambiguous; Gate result-table matrix in `vala-bifrost-redux` | `pg_verification_runtime::lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates` (identical table, batch ID, bytes; one attempt; one durable summary row), `::unacknowledged_summary_retries_with_a_fresh_result` (detail ACK + summary failure: no completion, inspectable `retrying`/`result_publication_failed`; the fresh attempt's batch has a new ID and both attempts' detail rows stay durable), `::completed_run_publishes_details_then_summary_and_records_metrics`, `::unscored_drift_publishes_only_the_summary`; `pg_grpc_ingest_smoke::system_writer_alone_writes_verification_results`; `execution_lanes` `system_writer_correlates_only_its_signed_verifier`; `wyrd-testing` `verification_runtime::runner_without_local_scribe_publishes_through_the_ingest_endpoint` | PASS |
| SYSTEM reuses tenant identity/JWT machinery, is unreachable publicly, no duplicate issuance audit | `wyrd.provision_system_principal()` migration and `service_accounts.rs` public-query exclusions; `wyrd-auth` `issuance.rs` `issue_system_token`; tenant provisioning call | `pg_admin_principals::{system_principal_provisioning_is_idempotent_and_stable, system_principal_is_absent_from_public_principal_paths, system_principal_upgrade_backfills_existing_tenants_idempotently}`; `issuance::pg_tests::{issue_system_token_round_trips_one_verifier_scope_without_audit, issue_system_token_refuses_bad_scopes_and_missing_or_malformed_writers, public_grants_refuse_the_system_principal}` (`test:principals:integration`); real-router HTTP `pg_verification_routes::system_writer_is_unreachable_through_public_principal_routes` (credential list/issue/revoke and principal revoke all `404`, row stays active and keyless) | PASS |
| Manual/scheduled work, claims, retries, concurrency, shutdown, and audit satisfy AC-015, AC-023, AC-028, runtime portions of AC-030 | `verification/{scheduler,runner,permits,health,clock}.rs`; `RuntimeLimits::default()` 16/4 permits and 30 s drain; `ShutdownConfig::drain_ms` default raised to 35 000 ms and manifests' `terminationGracePeriodSeconds: 45` so the 30 s drain is never clipped or killed | `pg_verification_runtime::{schedulers_create_one_run_per_occurrence_across_ticks_and_restart, expired_lease_is_reclaimed_and_the_stale_holder_is_fenced, retryable_engine_failures_exhaust_to_errored, permits_cap_each_tenant_and_share_the_process, crashed_runner_restarts_and_reclaims_without_duplicates, shutdown_releases_runs_still_in_flight_after_the_grace, shutdown_drains_runs_that_finish_within_the_grace, bound_server_composes_the_runtime_and_reports_it_ready}`; `verification::tests::default_server_budget_keeps_the_thirty_second_drain`; audit cardinality in `completed_run_publishes_details_then_summary_and_records_metrics`; route allow/deny audit in `pg_verification_routes`; Rust/Python/TS/MCP journeys `starts_a_keyed_manual_run_and_reads_its_status`, `test_service_starts_a_keyed_manual_run_and_reads_its_status`, `verification-run.test.ts`, `an_agent_discovers_starts_and_observes_a_manual_run` | PASS |
| Drift/Eval/Operator tasks can consume this runtime without a second lifecycle | `verification/engines.rs` closed typed dispatch (engines return outcomes, never mutate runs); Operator worker slot in `RuntimeCapability`; dispatch rows inserted at completed-failed settlement | `pg_verification_runtime::unavailable_engine_errors_without_publishing`; `pg_verifier_runs` dispatch-settlement cases | PASS |
| Scenario 1 SYSTEM writes only result tables | as above | as above | PASS |
| Scenario 2 manual enqueue with requester identity | `components/verification` | `pg_verification_routes::{manual_runs_enqueue_replay_and_read_back, manual_run_refusals_fail_before_enqueue, verification_state_is_tenant_isolated}` | PASS |
| Scenario 3 scheduler one exact window, no catch-up | `verification/scheduler.rs` | `schedulers_create_one_run_per_occurrence_across_ticks_and_restart`; `pg_verifier_runs` cursor cases | PASS |
| Scenario 4 claims/retries/terminal states survive restart | `verification/runner.rs` | lease, retry-exhaustion, cancellation, crash-restart tests above | PASS |
| Scenario 5 publication requires every non-empty ACK; sealed replay dedups, fresh write does not | `verification/publisher.rs` | `lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates`, `unacknowledged_summary_retries_with_a_fresh_result`, `unscored_drift_publishes_only_the_summary`, `runner_without_local_scribe_publishes_through_the_ingest_endpoint` | PASS |
| Scenario 6 one typed status projection | `wyrd-spec` verification types, `wyrd-client` `Verification` handle, SDK/MCP projections | Rust/Python/TS/MCP journeys above; `codegen:check` | PASS |
| Scenario 7 limits, audit, health, shutdown | `verification/{mod,permits,health}.rs`, `config.rs`, `deploy/kubernetes/bifrost/*.yaml`, `docs/.../self-hosting/configuration.svx` | permit, shutdown, crash, metrics tests above; `default_server_budget_keeps_the_thirty_second_drain`; `docs:check` | PASS |

Focused commands for the tests added while closing gaps (Postgres through the
repository wrapper):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E "test(=lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates) | test(=unacknowledged_summary_retries_with_a_fresh_result)"'
WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_verification_routes -E "test(=system_writer_is_unreachable_through_public_principal_routes)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::tests::default_server_budget_keeps_the_thirty_second_drain)'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_router_smoke -E "test(=coordinator_standby_pass_is_not_ready)"'
```

All four exited 0.

Verification commands (all exit 0 after the last code change):

| Command | Exit |
|---|---|
| `mise run test:principals:unit` | 0 |
| `mise run test:principals:integration` | 0 |
| `mise run test:sql` | 0 |
| `mise run test:shared` | 0 |
| `mise run test:wyrd` | 0 |
| `mise run test:vala` | 0 |
| `mise run test:bifrost:integration:server` | 0 |
| `mise run test:bifrost:journey:sdk` | 0 |
| `mise run test:bifrost:journey:server` | 0 |
| `mise run test:bifrost:journey:mcp` | 0 |
| `mise run test:cards:integration` (derived substitute for `test:e2e`) | 0 |
| `mise run py:test:integration` | 0 |
| `mise run py:typecheck` | 0 |
| `mise run ts:test:integration` | 0 |
| `mise run ts:typecheck` | 0 |
| `mise run codegen:check` | 0 |
| `mise run check:tenant-isolation` | 0 |
| `mise run check:client-tier` | 0 |
| `mise run check:unwrap-audit` | 0 |
| `mise run docs:check` | 0 |
| `mise run fmt` | 0 |
| `mise run lints` | 0 |
| `mise run py:format` | 0 |
| `mise run py:lints` | 0 |
| `git diff --check` | 0 |

Command derivation: the task lists `mise run test:e2e`, which does not exist.
The Rust SDK verification journey
(`wyrd-sdk-rust --test verification_run starts_a_keyed_manual_run_and_reads_its_status`)
and the HTTP verification route suite run in `mise run test:cards:integration`,
so that lane is the derived substitute; no alias task was added. The SYSTEM
public-route HTTP proof lives in `pg_verification_routes` (that lane) because
no principal server test target exists in `test:principals:integration`; the
public surface has no principal list/get/update route, so list omission is the
SQL-level `system_principal_is_absent_from_public_principal_paths`.

While running the lanes, `docs:check` failed because `docs:generate` invoked a
bare `python` (now `uv run --no-project python`, the repository convention) and
the schema inventory lacked the new verification schemas (regenerated);
`test:wyrd` exposed a race in `pg_router_smoke::coordinator_standby_pass_is_not_ready`
that drove passes before observing the boot pass (now awaits it, as its sibling
tests do).

Non-goals confirmed excluded: no broker; no direct or local Scribe write (the
runner publishes only through `wyrd_client::Bifrost` over gRPC; the test-support
fault transport wraps that same gRPC transport); no result HTTP endpoint; no
process-local run registry (the durable queue is the only record); no new RBAC
permission (`evals:run` and `bifrost_record:write` reused); no cross-table
recovery protocol; no authorization audit from internal claims, retries,
Scribe commits, minting, or worker mechanics.

Material limits:

- Drift and Eval engine arms return terminal `implementation_unavailable`
  until TASK-005/TASK-006, so a real run cannot reach `completed` with a real
  verdict yet; completed paths are proven with the test-support engine script.
- PSI/SPC readiness is fail-closed until TASK-005 adds `drift_baselines`.
- The Operator worker capability slot is declared; dispatch delivery is
  TASK-007.
- A run-level retry after a failed publication mints a fresh `result_id`, so
  earlier-attempt detail rows may remain as accepted partial rows (REQ-086).
- Sealed-batch replay is bounded by the facade's identical-resend budget
  inside one publication attempt; an attempt that exhausts it retries the run
  with a fresh result rather than replaying across attempts.

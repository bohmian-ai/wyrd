# TASK-004 Task Implementation Review

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Review scope: complete base-to-candidate diff; implementation evidence prose was not used to infer the verdict.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK outcome: one supervised durable runtime admits, claims, retries, publishes through Bifrost, settles after ACK, and exposes one status/manual contract | `wyrd-server/src/verification/{mod,scheduler,runner,publisher}.rs`; `wyrd-sql/src/queries/verifier_runs.rs`; `wyrd-server/src/components/verification`; `wyrd-client/src/verification.rs` | `pg_verification_runtime`; `pg_verification_routes`; Rust/Python/TypeScript/MCP journeys | PASS |
| REQ-061: Drift and Eval map to the common Verification Result | `verification/engines.rs::VerifierReport`; `verification/results.rs::ResultPayloadBuilder` | result payload unit cases and `pg_verification_runtime` completed-result cases | PASS |
| REQ-062: results bind exact run/Verifier/subject/input and nullable direct owner/binding identities | `verifier_runs.rs::ClaimedRun`; `results.rs::ResultRun` and batch construction | `pg_verifier_runs`; `pg_verification_routes::manual_runs_enqueue_replay_and_read_back`; result publication tests | PASS |
| REQ-063 / INV-004: execution status and verdict are independent; non-completed execution cannot pass | SQL status/result constraints in migration `20260601000029_verifier_runs.sql`; `EngineOutcome`, `Transition`, and `complete` settlement | cancellation/deadline/retry-exhaustion and unavailable-engine tests | PASS |
| REQ-078: Postgres owns runnable control state while Bifrost owns result data | run/dispatch migrations and queue; result rows only through `ResultPublisher`; Run GET returns a result pointer, not verdict/details | SQL queue tests, route contract tests, gRPC result tests | PASS |
| REQ-079: run rows freeze exact identities/input; claims are leased and fenced; retries retain the run/input | migration columns/checks; `VerifierRunQueue::{insert,claim,retry,terminate,release}`; `ClaimedRunRow::into_claimed` | `pg_verifier_runs` identity, expiry, stale-token, retry, and terminal-state cases | PASS |
| REQ-081: scheduling uses a short locked transaction, unique occurrence, exact window, cursor advance, and no catch-up | `DUE_BINDING_SQL`; `schedule_next_due`; `BindingSchedule::occurrence`; scheduler commits before execution | `schedulers_create_one_run_per_occurrence_across_ticks_and_restart` and SQL cursor cases | PASS |
| REQ-082 / REQ-136: bounded manual Drift window; binding/direct targets; 202 durable enqueue; requester, idempotency, scope, readiness, and refusal behavior | typed request validation in `wyrd-spec/src/verification.rs`; `VerificationControl::start_run`; keyed SQL enqueue; HTTP route | `pg_verification_routes` success, replay/conflict, invalid window/target, permission, tenancy, and readiness cases | PASS |
| REQ-085 / REQ-119 / REQ-121 / REQ-122: canonical result/detail rows carry exact split identities, one event time, required IDs, schema, and nullability | `verification/results.rs`; verification `DomainTable` definitions; Gate table handling | result mapping unit tests, gRPC ingest schema/matrix tests, generated schema checks | PASS |
| REQ-086 / REQ-087: details precede summary, no empty details, every non-empty batch must ACK, ambiguous sealed replay is identical, fresh attempt is fresh | `ResultPublisher::publish`; existing `Bifrost::write_batch` sealed-owner transport; completion only after publisher success | `lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates`; `unacknowledged_summary_retries_with_a_fresh_result`; zero-detail and completion cases | PASS |
| REQ-096 / INV-011: one Trigger→run→Verifier→result→failed-only dispatch lifecycle, with no implementation-owned scheduling or notification | closed engine dispatch; `VerifierRunQueue::complete` inserts dispatches only for failed binding runs | SQL settlement/dispatch tests and completed-result runtime cases | PASS |
| REQ-097: failed completed binding runs create durable dispatch rows only after result ACK; direct runs never dispatch | `COMPLETE_RUN_SQL` plus `VerifierRunQueue::complete`; publisher precedes settlement | result-publication failure and dispatch-settlement tests | PASS |
| REQ-100 / REQ-135: exactly three Verification HTTP operations expose typed binding/run/manual projections without duplicating analytical results | verification router/service; `VerificationBindingStatus`; `VerificationRunStatus`; OpenAPI registration | route integration tests and served OpenAPI contract | PASS |
| REQ-115: one bounded supervised runtime, durable state, restart, recovery, drain, health, telemetry | `VerificationRuntime`; `VerifierRunner`; health capability bits; metrics declarations; durable queue | crash/restart, lease reclaim, shutdown, bound-server readiness, and metrics assertions in `pg_verification_runtime` | PASS |
| REQ-137: Rust, Python, TypeScript, and MCP project the shared Verification capability | `wyrd-client/src/verification.rs`; thin SDK bindings; `mcp/verification.rs` | Rust, Python, TypeScript integration journeys and MCP verification journey | PASS |
| REQ-145 / INV-007: existing permissions, tenant isolation, and canonical audit apply at public/Gate boundaries; mechanics do not audit | `VerificationControl` authorization/audit; RLS tables; Gate authorization path; SYSTEM issuance has no audit append | route allow/deny and cross-tenant tests; Gate matrix/audit tests; SYSTEM issuance tests | PASS |
| REQ-146 runtime portion: one scheduler, 16 global/4 tenant Verifier permits, 30-second drain, task restart, health and telemetry | `RuntimeLimits::default`; `VerifierPermits`; runtime supervision and health | permit fairness/cap tests; shutdown/crash/readiness tests; focused default-drain test | PASS |
| INV-010: Bifrost is authoritative for results and Postgres for runnable/operational state | result publication and status boundaries above; no result payload persisted in Postgres | route status and Bifrost publication/query evidence | PASS |
| AC-015: duplicate occurrence/record/batch, lease expiry, retry exhaustion, partial publication, fresh write, and zero-detail behavior | unique indexes/fences and publisher/settlement flow | corresponding SQL, Scribe, and runtime integration cases are present | PASS |
| AC-020 runtime persistence portion: real Postgres queue/readiness/claim/settlement behavior | migrations and tenant/operator query paths | repository-managed Postgres tests in `pg_verifier_runs` and `pg_verification_runtime` | PASS |
| AC-023 runtime portion: worker without local Scribe publishes remotely as tenant SYSTEM writer; identity and table matrix are closed | explicit ingest endpoint, `ResultPublisher`, SYSTEM issuer, Gate matrix | `runner_without_local_scribe_publishes_through_the_ingest_endpoint`; `system_writer_alone_writes_verification_results`; token/Gate tests | PASS |
| AC-024 runtime schema/publication portion: exact verification result schemas and shared event-time partition key | table declarations and `ResultPayloadBuilder` | schema assertions in gRPC ingest tests and generated catalog checks | PASS |
| AC-028 runtime/status portion: typed manual/status API across HTTP, SDKs, and MCP, including binding/direct ownership behavior | service, routes, shared client, language projections, MCP | HTTP binding/direct cases and first-class client/MCP journeys | PASS |
| AC-030 runtime portion: tenant fairness, bounded Verifier execution, authorization audit boundaries, shutdown, restart, health, metrics | permits, audit/service boundaries, supervision, shutdown flow | permit, audit cardinality, multi-tenant, crash/restart, shutdown, readiness, and metrics cases | PASS |
| Scenario 1: stable internal-only tenant SYSTEM principal, scoped short token, closed Gate result-table matrix, no issuance audit | SYSTEM migration/provisioning; auth issue/verify; public query exclusions; Gate | principal SQL/auth tests, public-route test, Gate execution-lane tests | PASS |
| Scenario 2: manual enqueue preserves requester and refuses before enqueue | `VerificationControl` and `enqueue_manual` transaction | `pg_verification_routes` and `pg_verifier_runs` | PASS |
| Scenario 3: scheduler creates one exact occurrence without catch-up | scheduler and queue schedule methods | scheduler restart/concurrency tests | PASS |
| Scenario 4: claims/retries/terminal states survive restart | claim/settlement SQL and runner | expiry, stale holder, exhaustion, cancellation, deadline, restart tests | PASS |
| Scenario 5: publication requires every non-empty ACK | publisher/result builder/settlement order | remote Scribe, lost-ACK, partial failure, fresh-attempt, zero-detail tests | PASS |
| Scenario 6: one typed control-plane projection | `wyrd-spec`, shared client, SDK and MCP projections | HTTP/client/MCP contract journeys | PASS |
| Scenario 7: limits, audit, health, telemetry, and shutdown | permits, supervision, health, metrics, drain | runtime integration suite and focused drain test | PASS |
| Constraint: Drift/Eval engines return outcomes and do not own run mutation; later engine/Operator tasks can plug in without a second lifecycle | `engines.rs` closed dispatch and runtime-owned transitions; `RuntimeCapability::OperatorWorker` reserved slot | unavailable-engine and scripted-engine runtime tests | PASS |
| Non-goal: no broker, direct/local Scribe write, result HTTP endpoint, process-local work registry, new permission, or cross-table repair/atomicity protocol | complete diff retains Postgres queue, remote `wyrd_client::Bifrost`, existing permissions, and only the three specified routes | route/OpenAPI, client-tier, Gate/Scribe tests | PASS |
| Non-goal: internal claims, retries, commits, minting, and worker mechanics emit no authorization audit | only public `VerificationControl` and Gate call canonical authorization audit; SYSTEM mint and queue transitions do not | audit-cardinality and issuance tests | PASS |
| Ponytail scope: no parallel issuer, queue, transport, status engine, result API, or compatibility path | existing identity/JWT, queue, Bifrost facade, shared client, and routes are reused | boundary and journey evidence above | PASS |

## Proposed findings

None. I found no reachable task-acceptance failure, prohibited alternate path, or scope expansion that warrants a TASK-004 implementation finding.

The production Drift and Eval engine bodies intentionally return terminal `implementation_unavailable`; the original task makes their engines consumers of this runtime and assigns their implementation to dependent follow-up tasks. The completed-result paths are therefore exercised with the test-support engine script without misrepresenting those later engines as shipped by TASK-004.

## Verification limits

- The task records successful broad Rust, SQL, server, Bifrost, SDK, MCP, codegen, boundary, formatting, lint, and docs lanes. I inspected their mapped source and tests but did not rerun the full heavy suite during this review.
- I independently ran `git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8..49ad24707de47378b9df51764034c49a129c6b8b` successfully.
- I independently ran `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::tests::default_server_budget_keeps_the_thirty_second_drain)'`: 1 passed.
- The task names `mise run test:e2e`, but no such repository task exists. The available evidence uses `test:cards:integration`, which directly runs the Rust SDK Verification journey and the real HTTP route suite. This is a command-name gap in the task artifact, not an uncovered acceptance behavior in the candidate.
- Candidate identity remained `49ad24707de47378b9df51764034c49a129c6b8b` through review.

## Overall result

**PASS**

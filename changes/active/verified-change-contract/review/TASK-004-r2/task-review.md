# TASK-004 Task Implementation Review — R2

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior verdict and findings: `changes/active/verified-change-contract/review/TASK-004-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md`
- Review scope: the complete cumulative base-to-candidate diff, including closure of all seven prior stable findings. Implementation summaries were used only as pointers to evidence, not as proof.

The candidate remained at the stated commit throughout this review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK outcome: one supervised durable runtime admits, claims, retries, publishes through Bifrost, settles only after required ACKs, and exposes one status/manual contract | `wyrd-server/src/verification/{mod,scheduler,runner,publisher}.rs`; `wyrd-sql/src/queries/verifier_runs.rs`; `wyrd-server/src/components/verification`; `wyrd-client/src/verification.rs` | `pg_verification_runtime`; `pg_verification_routes`; Rust/Python/TypeScript/MCP journeys recorded in the task | PASS |
| REQ-061: Drift and Eval map to the common Verification Result | `verification/engines.rs::VerifierReport`; `verification/results.rs::ResultPayloadBuilder` | result-payload unit cases and completed-result runtime cases | PASS |
| REQ-062: results bind exact run, Verifier, subject, input, and nullable direct owner/binding identities | `verifier_runs.rs::ClaimedRun`; `results.rs::ResultRun` and named batch construction | `pg_verifier_runs`; route read-back; runtime publication cases | PASS |
| REQ-063 / INV-004: execution status and verdict remain independent; incomplete or terminal execution cannot pass | verifier-run constraints; `EngineOutcome`, `Transition`, and fenced settlement | cancellation, timeout, retry-exhaustion, and unavailable-engine cases | PASS |
| REQ-078: Postgres owns runnable control state while Bifrost owns analytical results; tenant work uses the sanctioned Postgres owner | run/dispatch migrations and queue; `VerificationScheduler`, `VerifierRunner`, `ResultPublisher`, and `VerificationFixture` hold `WyrdPostgres`; cross-tenant discovery retains `OperatorPool` | SQL/runtime/journey lanes recorded in remediation evidence; source search finds no raw `PgPool`, `TenantConn::acquire`, or `RunnerPools` in the verification runtime/fixture | PASS |
| REQ-079: durable rows freeze exact identities/input; claims are leased and token-fenced; retries retain run/input | migration constraints; `VerifierRunQueue` claim and transition operations; `ClaimedRunRow::into_claimed` | identity, lease-expiry, stale-token, retry, and terminal SQL/runtime cases | PASS |
| REQ-081: scheduling uses a short locked transaction, unique occurrence, exact window, cursor advance, and no catch-up | `schedule_next_due`; `VerificationScheduler`; cancellation drops an uncommitted occurrence transaction | scheduler duplicate/restart cases and `cancelled_scheduler_rolls_back_its_blocked_occurrence` | PASS |
| REQ-082 / REQ-136: bounded manual Drift window; binding/direct target; `202` durable enqueue; requester, idempotency, scope, readiness, and refusal behavior | typed verification request; `VerificationControl::start_run`; keyed enqueue; HTTP route | route success, replay/conflict, invalid target/window, permission, tenancy, and readiness cases | PASS |
| REQ-085 / REQ-119 / REQ-121 / REQ-122: canonical summary/detail rows carry the exact split identities, required IDs, one event time, schema, and nullability | `ResultPayloadBuilder`; table-owned `arrow_fields`; named-column `assemble` rejects missing, duplicate, and unexpected fields | independently rerun reordered and negative name-mapping unit tests; result schema/publication cases; role-separated identity journey | PASS |
| REQ-086 / REQ-087: details precede summary, zero details emits no batch, every non-empty batch must ACK, ambiguous sealed replay is identical, and a fresh attempt is fresh | `ResultPublisher::publish`; shared `Bifrost::write_batch`; completion follows publication success | lost-ACK replay/dedup, partial failure, fresh-attempt, zero-detail cases, and detail-ACK/summary-unknown crash case | PASS |
| REQ-096 / INV-011: one Trigger-to-run-to-Verifier-to-result-to-failed-only-dispatch lifecycle | closed engine dispatch; queue completion inserts dispatches only for failed binding-created results | settlement and dispatch tests, including crash recovery before dispatch | PASS |
| REQ-097: failed binding result dispatches only after acknowledged completion; direct runs never dispatch | `COMPLETE_RUN_SQL`; publisher-before-settlement ordering | publication-failure, partial-result, completion, and direct-run cases | PASS |
| REQ-100 / REQ-135: exactly three typed Verification HTTP operations expose binding/run/manual control state without duplicating Bifrost result data | verification router/service and wire status types | route and served OpenAPI tests | PASS |
| REQ-115: one bounded supervised runtime, durable recovery, drain, health, and telemetry | `VerificationRuntime`, health capabilities, runner/scheduler supervision, metrics | crash/restart, lease reclaim, drain, readiness, and metrics cases | PASS |
| REQ-137: Rust, Python, TypeScript, and MCP project the shared Verification capability | shared `Verification` client plus thin SDK/MCP projections | first-class language and MCP journeys recorded in task evidence | PASS |
| REQ-145 / INV-007: existing permissions, RLS tenancy, and canonical audit apply at public/Gate boundaries; mechanics do not audit | route authorization; RLS; `Gate::authorize_record_write`; SYSTEM mint and queue mechanics contain no parallel audit | route allow/deny, cross-tenant, SYSTEM issuance, Gate matrix, and audit-cardinality cases | PASS |
| REQ-146 runtime portion: one scheduler, 16 global/4 per-tenant Verifier permits, 30-second drain, restart, health, and metrics; shutdown closes runner claim admission | runtime limits, permit owner, cancellation-aware runner claim transaction, fenced late-claim release/refund | permit fairness; drain/restart cases; `cancelled_runner_rolls_back_its_blocked_claim`; `claim_committed_after_cancellation_is_released_unexecuted` | PASS |
| INV-010: Bifrost remains authoritative for result rows and Postgres for runnable/operational state | result transport and status boundaries; Postgres stores only result pointers | route status plus Bifrost publication/query cases | PASS |
| AC-015: duplicate occurrence/batch, lease expiry, retry exhaustion, partial publication, fresh write, crash interleaving, and zero-detail behavior | queue fences and publisher/settlement ordering | corresponding SQL, Scribe, and runtime cases, including `crash_after_detail_ack_reclaims_the_same_run_before_dispatch` | PASS |
| AC-020 runtime persistence portion: real Postgres queue/readiness/claim/settlement seams | tenant and operator query paths | repository-managed Postgres tests recorded in the task/remediation evidence | PASS |
| AC-023 runtime portion: no-local-Scribe worker publishes remotely as tenant SYSTEM writer; result/detail identity and Gate scope are closed | explicit ingest endpoint, tenant SYSTEM issuer, Gate reservation and mandatory frame scope | extended role-separated non-empty binding result/detail journey; authenticated absent/null/malformed/foreign/exact scope gRPC test; Gate table-matrix tests | PASS |
| AC-024 runtime schema/publication portion: exact result schemas and shared event-time identity | table definitions and name-bound result construction | schema cases and role-separated summary/detail shared-time join | PASS |
| AC-028 runtime/status portion: typed manual/status API across HTTP, SDKs, and MCP, including binding/direct ownership | service, routes, shared client, SDKs, MCP | HTTP binding/direct and first-class client journeys | PASS |
| AC-030 runtime portion: fairness, bounded execution, audit boundaries, shutdown admission closure, restart, health, and metrics | permits, audit/service boundaries, supervision, scheduler/runner cancellation boundaries | permit/audit/crash/drain/readiness cases and the three lock-controlled cancellation tests | PASS |
| Scenario 1: stable internal-only tenant SYSTEM principal, short exact-Verifier token, closed result-table matrix, and no issuance audit | SYSTEM provisioning/migration; existing issuer/verifier; public exclusions; Gate's combined decision calls `require_frame_card_scope` before its one audit append | principal/auth/public-route tests; Gate unit cases; authenticated gRPC scope/audit test | PASS |
| Scenario 2: manual enqueue preserves requester and refuses invalid work before enqueue | `VerificationControl` and tenant enqueue transaction | route and queue cases | PASS |
| Scenario 3: scheduler creates one exact occurrence without catch-up and stops admitting an uncommitted occurrence at cancellation | scheduler and queue schedule operations | restart/duplicate cases plus lock-controlled cancellation | PASS |
| Scenario 4: claims, retries, and terminal states survive restart; cancellation rolls back or fenced-releases late claims | runner/queue transition owner | expiry, stale holder, exhaustion, crash/restart, and lock-controlled cancellation cases | PASS |
| Scenario 5: publication requires every non-empty ACK; partial rows never authorize completion/dispatch | publisher/result builder/settlement order | remote role-separated path, lost ACK, partial failure, zero detail, and crash after durable detail ACK | PASS |
| Scenario 6: one typed control-plane projection across HTTP/shared client/SDK/MCP | `wyrd-spec`, shared client, SDK/MCP projection | route, language-client, and MCP journeys | PASS |
| Scenario 7: fixed limits, public/Gate-only audit, health, telemetry, immediate admission closure, and bounded drain | permits, supervision, health, metrics, cancellation-aware durable boundaries | permit, audit, crash/restart, cancellation, and drain cases | PASS |
| Prior `FIND-TASK-004-1`: remove raw application pool ownership | runner pool wrapper deleted; runtime and fixture use `WyrdPostgres::tenant_conn`; `OperatorPool` remains only for authorized cross-tenant reads | source search plus affected lanes in remediation evidence | PASS |
| Prior `FIND-TASK-004-2`: bind result values to Arrow fields by name | builders return named columns; `assemble` orders from the table authority and validates the complete name set | two independently rerun focused unit tests passed | PASS |
| Prior `FIND-TASK-004-3`: use top-level imports and bare names in cited Rust fields/signatures/bounds | cited modules use imported bare types; no behavioral abstraction or module churn added | formatting/lint and affected lanes recorded in remediation evidence; source inspection | PASS |
| Prior `FIND-TASK-004-4`: exact Verifier scope must participate in the canonical Gate decision | `authorize_record_write` folds `require_frame_card_scope` into the reserved SYSTEM/result-table decision before exactly one audit append; Scribe's validation/stamping remains | Gate unit tests and authenticated real-gRPC scope/audit/durable-row test | PASS |
| Prior `FIND-TASK-004-5`: shutdown closes durable scheduler/runner claim admission | scheduler pass races cancellation; runner races pre-commit work and fenced-releases a commit-race claim without execution | three lock-controlled Postgres tests plus existing drain cases | PASS |
| Prior `FIND-TASK-004-6`: role-separated proof covers non-empty binding summary/detail and exact identities | existing no-local-Scribe journey now schedules a binding-created two-feature Drift result and queries summary/details through Oracle | journey asserts SYSTEM, Verifier, subject, owner, binding, run, result join, shared event time, and no local ingest | PASS |
| Prior `FIND-TASK-004-7`: prove crash after detail ACK and unknown summary outcome | existing publication hang and runner crash controls exercise the interleaving without a recovery coordinator | runtime test proves one durable partial detail, no completion/dispatch, same-run reclaim, and dispatch only after later full ACK | PASS |
| Constraint: Drift/Eval engines return typed outcomes and do not mutate run rows; later engines/Operator worker can consume the runtime without a second lifecycle | closed engine dispatch; runtime-owned transitions; reserved Operator capability slot | unavailable-engine and scripted-engine cases | PASS |
| Non-goals: no broker, local/direct Scribe write, result HTTP endpoint, process-local run registry, new permission, repair coordinator, cross-table atomicity claim, or alternate identity/token hierarchy | complete cumulative diff retains the existing queue, Bifrost facade, permission, identity, and route owners | source/diff inspection and boundary evidence | PASS |
| Ponytail scope: remediation reused the existing Postgres owner, table schema authority, Gate/Scribe scope validator, cancellation token, fault seam, and role-separated harness | remediation diff adds no replacement framework, transport, permission, dependency, or persistent protocol | source/diff inspection | PASS |

## Proposed findings

None. The candidate closes `FIND-TASK-004-1` through `FIND-TASK-004-7`; I found no reachable remaining task-acceptance failure, prohibited alternate path, regression, or task-external scope expansion.

The production Drift and Eval engine arms still return the intentional unavailable outcome. TASK-004 establishes their shared lifecycle and assigns their substantive engine work to dependent tasks; the completed-result paths are therefore correctly exercised through the existing test-support engine seam.

## Verification limits

- I inspected the complete cumulative diff and the source and tests mapped above, but did not rerun the full Postgres, multi-server, SDK, MCP, codegen, lint, and boundary matrix during this review. The original task and remediation task record those lanes as passing at the candidate.
- I independently ran `git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8..2af4cc3ff95a609d1df682be4f633345f96934e1`; it passed.
- I independently ran the two focused name-mapping unit tests through `mise exec -- cargo nextest run --locked -p wyrd-server --lib`; both passed.
- The lock-controlled cancellation, authenticated gRPC scope/audit, role-separated journey, and crash-after-detail-ACK proofs were inspected but not independently rerun because they require repository-managed Postgres or the serialized multi-server journey lane. Their exact commands and successful results are recorded in the remediation task.
- The original task names `mise run test:e2e`, which is not a current repository task. Its recorded substitute remains the capability-specific real-server journey coverage; this is an artifact command-name limitation, not a missing TASK-004 behavior.

## Overall result

**PASS**

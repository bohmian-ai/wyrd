# TASK-004 Task Implementation Review — R3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `29721b7e33854633b025b25948fd5d2eaebe7bfd`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior reviews: `changes/active/verified-change-contract/review/TASK-004-r1/` and `TASK-004-r2/`
- Remediation tasks: `TASK-004-R1-close-validated-runtime-gaps.md` and `TASK-004-R2-close-scheduler-ordering-and-source-shape.md`

This review covers the complete cumulative base-to-candidate range. Prior
implementation and review records were used as evidence pointers, not as a
substitute for inspecting the candidate source, callers, tests, and final
remediation diff. The candidate remained unchanged throughout this review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK outcome: one supervised durable runtime admits scheduled/manual work, claims and retries exact runs, publishes through Bifrost, settles only after required ACKs, and exposes one status/manual contract | `wyrd-server/src/verification/{mod,scheduler,runner,publisher}.rs`; `wyrd-sql/src/queries/verifier_runs.rs`; `wyrd-server/src/components/verification`; `wyrd-client/src/verification.rs` | `pg_verification_runtime`; `pg_verification_routes`; recorded Rust/Python/TypeScript/MCP journeys | PASS |
| REQ-061, REQ-062: Drift/Eval use the common result and freeze exact run, Verifier, subject, input, and nullable direct-run owner/binding identities | `verification/engines.rs::VerifierReport`; `verification/results.rs::ResultPayloadBuilder`; `ClaimedRun` | Result construction tests, SQL identity tests, route read-back, and role-separated publication journey | PASS |
| REQ-063, INV-004: execution state and verdict remain independent; incomplete, errored, cancelled, or timed-out work cannot pass | run constraints; `EngineOutcome`; fenced `Transition` settlement | cancellation, timeout, retry exhaustion, unavailable-engine, and completion cases | PASS |
| REQ-078: Postgres owns durable control state and Bifrost owns analytical results; tenant work enters through the sanctioned owner | run/dispatch migrations and queue; runtime owners hold `WyrdPostgres`; cross-tenant discovery uses `OperatorPool` | SQL/runtime lanes recorded in R1 evidence; cumulative source inspection | PASS |
| REQ-079: run rows freeze identities and inputs; claims are leased/token-fenced; retry retains the run and input | migration constraints; `VerifierRunQueue` claim/transition paths | lease-expiry, stale-holder, retry, and terminal SQL/runtime cases | PASS |
| REQ-081: scheduling uses a short locked transaction, unique occurrence, exact fixed window, atomic cursor advance, and no catch-up | `schedule_next_due`; `VerificationScheduler::pass` and `schedule_tenant` | duplicate/restart test; pre-commit cancellation rollback test; independently rerun deferred-commit test | PASS |
| REQ-082, REQ-136: manual Drift accepts one bounded window and binding/direct target, returns a durable `202`, freezes requester, and applies idempotency/readiness/scope checks before enqueue | verification request types; `VerificationControl::start_run`; keyed enqueue | route success, replay/conflict, invalid target/window, readiness, permission, and tenancy cases | PASS |
| REQ-085, REQ-119, REQ-121, REQ-122: summary/detail rows carry exact split identities, one event time, authoritative schema, and required nullability | named-column `ResultPayloadBuilder`; table-owned `arrow_fields`; `assemble` validation | reordered/missing/duplicate/unexpected mapping tests; result schema and role-separated identity cases | PASS |
| REQ-086, REQ-087: detail batches precede summary, empty details emit nothing, every non-empty batch must ACK, ambiguous replay is identical, and a new attempt is a fresh write | `ResultPublisher::publish`; shared `Bifrost::write_batch`; settlement occurs only after publication succeeds | lost-ACK dedup, partial failure, new-attempt, zero-detail, and detail-ACK/summary-unknown crash cases | PASS |
| REQ-096, REQ-097, INV-011: Trigger-to-run-to-result-to-failed-only-dispatch is one lifecycle; direct runs never dispatch | closed engine dispatch; completed failed binding settlement inserts dispatches | dispatch settlement, publication failure, crash/reclaim, and direct-run cases | PASS |
| REQ-100, REQ-135: exactly three typed Verification HTTP operations expose control state without copying analytical result data | verification routes/service and wire types | route and served OpenAPI tests | PASS |
| REQ-115: one bounded supervised runtime has durable recovery, drain, health, telemetry, and task restart | `VerificationRuntime`; capability health; scheduler/runner supervision; metrics | crash/restart, reclaim, drain, readiness, and metrics cases | PASS |
| REQ-137: Rust, Python, TypeScript, and MCP project the shared Verification client capability | shared `Verification` handle and thin SDK/MCP projections | recorded first-class language and MCP journeys | PASS |
| REQ-145, INV-007: existing permissions, tenant RLS, and canonical audit guard public/Gate decisions; mechanics do not audit | route authorization; tenant connections; Gate combined decision; SYSTEM issuance and worker paths | allow/deny, cross-tenant, Gate matrix/scope, issuance, and audit-cardinality cases | PASS |
| REQ-146 runtime portion: one scheduler, 16 global/4 per-tenant Verifier permits, 30-second drain, restart/health/metrics, and known shutdown admission ordering | runtime limits and permit owner; cancellation-aware scheduler/runner transaction boundaries | fairness/drain tests; four focused scheduler/runner ordering tests independently rerun and passed | PASS |
| INV-010: Bifrost remains authoritative for analytical rows and Postgres for runnable/operational status | publisher and status boundary; Postgres stores result pointers only | status plus Bifrost publication/query cases | PASS |
| AC-015: duplicate occurrence/batch, lease expiry, retry exhaustion, partial publication, fresh write, zero-detail, and crash interleaving are covered | queue fences and publisher/settlement ordering | corresponding SQL, Scribe, and runtime cases | PASS |
| AC-020 runtime persistence portion: real Postgres covers queue, readiness, scheduling, claim, and settlement seams | tenant/operator SQL paths | repository-managed Postgres tests; focused scheduler suite rerun in this review | PASS |
| AC-023 runtime portion: a runner without local Scribe publishes through shared Bifrost as tenant SYSTEM, with exact Verifier scope and identity split | explicit ingest endpoint; tenant SYSTEM issuer; Gate reservation/scope check | authenticated gRPC scope/audit test and role-separated non-empty result/detail journey | PASS |
| AC-024 runtime portion: canonical result schema and shared event-time identity are preserved | table definitions and name-bound payload construction | schema cases and summary/detail event-time join | PASS |
| AC-028 runtime/status portion: typed manual/status API spans HTTP, SDKs, and MCP, including nullable direct ownership | service/routes, shared client, SDK/MCP projections | binding/direct route cases and first-class client journeys | PASS |
| AC-030 runtime portion: fairness, audit boundaries, immediate claim closure, bounded drain, recovery, health, and telemetry | permits, service/Gate audit boundaries, supervision, cancellation-aware transactions | permit/audit/crash/drain/readiness cases plus focused ordering suite | PASS |
| Scenario 1: stable internal-only SYSTEM principal, short exact-Verifier token, closed result-table matrix, and no issuance audit | provisioning migration; existing issuer/verifier; public exclusions; Gate combined decision | principal/auth/public-route, Gate, and authenticated gRPC cases | PASS |
| Scenario 2: manual enqueue preserves requester identity and refuses invalid work before enqueue | `VerificationControl`; tenant enqueue transaction | route and queue cases | PASS |
| Scenario 3: scheduler creates one exact occurrence without catch-up and gives cancellation/commit a known order | scheduler and queue schedule owner; selected commit is awaited non-cancellably | `scheduler_awaits_its_selected_commit_before_closing` and retained cancellation/restart tests | PASS |
| Scenario 4: claims, retries, fencing, and terminal states survive restart | runner and queue transition owner | expiry, stale-holder, exhaustion, crash/restart, and cancellation cases | PASS |
| Scenario 5: every required ACK gates completion/dispatch; partial analytical rows authorize neither | publisher, payload builder, settlement order | remote publication, lost ACK, partial failure, zero-detail, and crash-after-detail-ACK cases | PASS |
| Scenario 6: one typed status projection is shared by HTTP/client/SDK/MCP | `wyrd-spec`, shared client, thin language/MCP surfaces | route, SDK, and MCP journeys | PASS |
| Scenario 7: fixed limits, audit boundary, health/telemetry, immediate admission closure, and bounded drain | permits, health, metrics, supervision, scheduler/runner cancellation handling | fairness, audit, crash/restart, drain, and lock-controlled ordering tests | PASS |
| R1 findings `FIND-TASK-004-1` through `-7` remain closed | sanctioned Postgres owner; named result mapping; bare imports; exact Gate scope; cancellation boundaries; role-separated identity proof; crash/reclaim proof | R1 focused and broader evidence plus cumulative source inspection | PASS |
| R2 `FIND-TASK-004-5`: cancellation must not discard an unknown scheduler commit | `scheduler.rs:135-204`: pre-selection work races stop; once selected, `conn.commit()` is awaited before closure; no later occurrence starts after stop | Independently rerun deferred-commit, pre-commit rollback, runner late-claim, and duplicate/restart tests: 4/4 passed | PASS |
| R2 `FIND-TASK-004-8`: changed Rust fields/signatures/bounds use top-level imports and bare types | corrected server/state/publisher/ID/Gate/error/gRPC-test sites; cumulative remediation diff inspection | recorded `fmt`/`lints` and affected lanes; source inspection | PASS |
| Owner-directed reuse cleanup: remove uncalled `VerifierRunQueue::new` and `ResultPublisher::endpoint` without deleting a required contract | final commit deletes only the two methods; repository-wide Rust search finds no callers; production and tests construct the queue with `Default` and publisher with `ResultPublisher::new` | Focused runtime suite compiles and passes at the final candidate | PASS |
| Constraint/non-goals: no broker, direct/local Scribe write, result endpoint, process-local registry, new permission, repair coordinator, cross-table atomicity claim, second identity/token hierarchy, or implemented later engine | cumulative diff retains the existing queue, Bifrost, permission, identity, and route owners; production Drift/Eval arms intentionally remain unavailable for their dependent tasks | source/diff inspection | PASS |
| Ponytail scope: corrections reuse existing transaction, cancellation, fault, schema, Postgres, Gate/Scribe, and journey mechanisms | R1/R2 diffs and final deletion add no replacement framework, dependency, persistent protocol, or speculative public surface | source/diff inspection | PASS |

## Proposed findings

None. The cumulative candidate satisfies the TASK-004 obligations, closes the
R1 and R2 remediation findings, preserves the prohibited boundaries, and adds
no task-external runtime behavior. The final deletion is valid reuse cleanup:
both methods had zero callers and neither was a public contract required by the
task or specification.

The production Drift and Eval engine arms still return the intentional
unavailable outcome. TASK-004 establishes their shared runtime and durable
lifecycle; the substantive engines remain assigned to dependent tasks, so this
is not an acceptance gap.

## Verification limits

- Independently ran the repository-managed Postgres focused command for
  `scheduler_awaits_its_selected_commit_before_closing`,
  `cancelled_scheduler_rolls_back_its_blocked_occurrence`,
  `claim_committed_after_cancellation_is_released_unexecuted`, and
  `schedulers_create_one_run_per_occurrence_across_ticks_and_restart`; all four
  passed at candidate `29721b7e33854633b025b25948fd5d2eaebe7bfd`.
- The full SQL, multi-server, SDK, MCP, codegen, lint, and boundary matrix was
  not rerun during this review. Its successful commands are recorded in the
  original task and both remediation records; relevant source and tests were
  inspected.
- `git diff --check base..candidate` reports one extra blank line at EOF in the
  required prior-review evidence file
  `review/TASK-004-r2/findings-validation.md`. This does not change or weaken
  TASK-004 behavior and is below the material task-finding threshold; the
  repository-standards review independently owns source/artifact compliance.
- The original task names `mise run test:e2e`, which is not a current task in
  `mise.toml`; the recorded capability-specific real-server journey lanes are
  the available proof for the affected behavior.

## Overall result

**PASS**

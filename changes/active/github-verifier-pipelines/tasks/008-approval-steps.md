---
id: TASK-008
status: proposed
approved_spec_revision: 5
requirements: [REQ-010, REQ-011, REQ-016]
acceptance: [AC-006, AC-007, AC-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-007, TASK-010]
---

# Accept approval decisions for exact pending occurrences

Carry **approved-deferred-inputs.md → Approval and deadlines** and
**approved-runtime-and-github.md → Durable waits and resumption** through task
007's canonical state/park/continuation owners. **contracts.md → Human approval**
is approved; the earlier recent-login/five-minute proof restriction is superseded.
It requires task 010's memory execution runtime (spec revision 7 REQ-017).

## Input → pending occurrence

An exact registered Verifier declares `action: {kind: approval, message: ...}`.
There is no approval-specific expiration setting. Add that closed action/schema/
validation at the existing spec/engine action owner. Task 006's accepted source/authority and task
007's restored logical execution provide its context; no request can expand that
accepted authority.

### Revision — reconcile approval-only eligibility

Do not implement the former instruction to add a separate pre-step approval
guard. [010-R1](010-R1-restore-direct-verification-baseline.md) introduces a
server-derived `queue_only` flag checked on cold registry load; task 005 adds
pull-request extraction detection and task 007 adds deferred-input detection.
Approval is a separate action and does not automatically declare a deferred
input. The existing approval-is-queued-only requirement therefore is not fully
expressed by the maintainer's three specified registration conditions.
Resolve this approval-only eligibility conflict with the maintainer before
implementing that behavior. Do not infer a fourth classification condition,
invent deferred inputs, add per-request guards or silently allow direct approval.
This issue belongs to task 008 and does not block publish-only remediation or
the extraction/deferred-input registration extensions. Other approval behavior
below remains unchanged.

When the action becomes pending, the sole state owner creates a server-issued
approval_id for its logical step/item/iteration occurrence, distinct across
repeat/fanout. Attempts do not silently mint another occurrence. Every pending
approval uses the execution's one original expiration timestamp. Do not calculate
or store another approval deadline, add `expires_after_seconds`, introduce a clock
anchor, or query PostgreSQL when an approval step is reached. Checkpoint/recovery
and continuation preserve the same execution expiration; they never restart it.
The pending identity/message enters live state; inspection and decision
acceptance resolve it only from accepted Bifrost state. Independent branches
continue; when all block, use task 007's background park handoff (compute
released immediately, parked state ACKed before delivery retirement), not another
wait service or deadline loop.

## Decision acceptance and same-execution continuation

The caller selects the pending ID from execution inspection and uses
`POST /v1/verification/executions/{execution_id}/approvals/{approval_id}` with
approved/rejected and optional reason. Resolve the exact tenant/execution/
Verifier/occurrence; check normal authentication, the distinct Verifier approve
action at its exact scope through normal permission/
audit owners. No login-age cutoff, interactive proof, special approval token,
quorum or credential-type shortcut is authorized.

Extend the existing canonical execution control-fact acceptance owner used by
007 to conditionally accept actor/decision/reason for this pending, current
occurrence while its execution has not expired. Record approved/rejected/expired
with server actor/time/reason
(expiry has null actor). Same actor/decision/reason retry replays receipt;
conflicting, expired, stale, terminal or superseded occurrence refuses as approved.
Accept durable decision ACK before positive receipt; a process-local notification
or speculative checkpoint is not acceptance. The execution's single state owner
consumes the accepted fact, including on restart; do not race a second full-state
writer against live independent branches.

Accepted decision queues task 007's stable same-ExecutionId continuation.
Restore completed predecessors, original deadline, remaining attempts/spend and
accepted authority. An ACK→queue gap must recover without asking the user again.
Rejection is returned data, not crash/cancellation; authors may branch. Supply the
standard requirement helper that passes only approved and fails rejection/expiry.
Completion of the action alone cannot satisfy an authored approval requirement.
Discarded speculative/obsolete occurrences and an older head's approval never
authorize current work. Revocation/cancellation/deadline apply while parked.

## Output and shipped projection

Inspection returns pending/status occurrences with ID, step/item/iteration and
message; expiration is the existing execution expiration, not another per-approval
field. Decision receipt is exactly approval_id/status/actor_id/
decided_at/reason from the contracts. Task 009 reads pending reason and accepted
progress for GitHub status without changing verdict. Shared client and thin
Rust/Python/TypeScript, CLI, HTTP/MCP approve/read operations, scopes, typed errors,
permission projection and focused authoring/approval/expiry docs ship here.
Amend **wyrd-design.md → Verifier**, **wyrd-security-posture.md → Permission
model / Permission check** for approve and exact Verifier scope, and the Bifrost
canonical decision/ACK contract at its owner. No UI or alternate login flow.

## Proof

**AC-006 (approval):** Drive task 002 connect→003 declare→006 admit and task 007's
actual park lifecycle, with independent work and a pending approval. Read its ID
through SDK/CLI/MCP and approve/reject as an authorized second principal. Observe
exact receipt, same execution continuation and authored require-approved behavior.
The execution's expiration expires pending approvals rather than passing/crashing;
decisions after it are refused. Assert no approval-specific duration/deadline and
no new SQL call when an approval becomes pending. Unauthorized, foreign tenant,
wrong Verifier scope, conflicting retry, stale occurrence and old-head decision
fail safely. Repeat/fanout have distinct occurrence IDs; discarded speculation
cannot authorize retained work. Observe independent progress and capacity release
through the same resource path already proved by 007.

**AC-007 (decisions):** Restart while an execution waits for a decision, then
recover actual pending occurrence and accepted fact through 007. A positive
receipt survives without another decision or replay of settled computation;
unknown/unavailable canonical state is not a newly generated approval. Verify
recovery and continuation preserve the same execution expiration. Race
expiry, current authority loss and task 006 supersession with acceptance and
observe no stale release. This extends 007's recovery journey with real accepted
decisions, rather than a synthetic checkpoint claiming approval.

**AC-011 (approval):** A reader follows docs using ordinary credentials and
permission, inspects pending identity and decides through each shipped surface.

Tier 1: matched SDK stories and MCP journeys through WyrdTestServer/Postgres,
existing runners in README; tier 2: conditional fact/checkpoint/queue fences in
existing server and SQL integration owners via
`mise run test:bifrost:journey:server` and `mise run test:sql`; tier 3: action,
permission coverage and standard helper behavior at their owners. Record new
focused commands when tests exist. Apply README codegen/typing/OpenAPI/MCP,
format/lint, dependency and tenant checks. No authentication-age tests are added.

## Obligations

Maintainer clarification (2026-11-13): approval-only eligibility is settled by
spec REQ-015 ("PR extraction, deferred inputs and approval are queued-only") and
the registration-derived `queue_only` owner, whose contract already says later
queued-only forms extend that one derivation. A Verifier with an approval step
is therefore marked `queue_only` at registration; the §Revision caution is
resolved, not a fourth inferred rule.

| ID | Promise (source) | Planned proof |
|---|---|---|
| REQ-010a | `action: {kind: approval, message}` is a closed action with validation; no approval expiration setting; registration derives `queue_only`; direct verify refuses before any step (§Input, §Revision, REQ-015) | spec validation unit tests; declaration route test; direct refusal test |
| REQ-010b | A reached approval occurrence becomes pending with a server-issued approval_id per logical step/item/iteration, stable across attempts and distinct across fanout/repeat; no per-approval deadline, clock anchor or SQL call when pending; independent branches continue; all-blocked parks through 007's park handoff (§Input → pending) | engine unit tests (pending, independent progress, fanout/repeat distinct occurrences); runtime park test |
| REQ-010c | `POST /v1/verification/executions/{execution_id}/approvals/{approval_id}` with approved/rejected + optional reason: normal auth, exact `verifier:approve` at Verifier scope only, audited; resolves only from accepted state; durable decision ACK before receipt `{approval_id,status,actor_id,decided_at,reason}`; identical retry replays; conflicting/expired/stale/terminal/superseded/unknown refuse (§Decision acceptance, contracts → Human approval) | server pg journey: authorized second principal, unauthorized, foreign tenant, wrong Verifier scope, conflict, replay, terminal/expired, old head |
| REQ-011 | The accepted decision continues the same ExecutionId through 007's queue continuation; completed predecessors, deadline, attempts/spend and authority are restored; rejection is step data; standard helper passes only approved; ACK→queue gap recovers by park recheck; supersession/revocation/deadline fence parked work (§Decision acceptance) | pg journey (approve → pass, reject → fail via helper), deadline expiry, restart while waiting (AC-007) |
| REQ-016 | Inspection lists approval occurrences (id, step/item/iteration, message, status) using the execution's expiration; shared client, Rust/Python/TS SDKs, CLI, MCP read/decide, scope, typed errors, docs; architecture amended (§Output) | SDK journeys (Rust/Python/TS), MCP journey, CLI test, codegen/docs checks |
| INV-003/004 | Positive receipt follows durable acceptance; unavailable canonical state is never a new or absent approval (AC-007) | restart/unavailable tests |
| INV-006 | Superseded/old-head and discarded-speculation occurrences never authorize current work (AC-006) | old-head decision refused; occurrence IDs bind the execution |

## Evidence

Server journeys: `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner; mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_approvals::/) | test(=pg_card_declaration_route::registration_derives_queue_only_from_each_queued_only_form) | test(/pg_deferred_evidence::/) | test(/pg_openapi_contract::/)"'`.
Paths: `pg_approvals.rs` = `crates/wyrd/wyrd-server/tests/integration/pg_approvals.rs`.

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| REQ-010a: closed `approval` action with `message`, no expiration field; registration derives `queue_only`; direct verify refuses | `wyrd-spec/src/card/verifier/{mod,validate}.rs`; one added condition in the existing `queue_only()` derivation, `components/cards/service.rs:1951` | `validate.rs:1593` `approval_states_a_message_and_outputs_its_receipt`; `pg_card_declaration_route.rs:1737` (`qo-approval` case); refusal of any `queue_only` Verifier by `verify` is the shared flag proven at `pg_card_declaration_route.rs:2023` | nextest `-p wyrd-spec`, server journey above | PASS |
| REQ-010b: pending occurrence with a server-issued id per step/item/iteration, stable across attempts, distinct across fanout/repeat/execution; no per-approval deadline or SQL when pending; independent branches continue; all-blocked parks via 007 | engine `Waiting{approvals}` (`vala-verify/src/blocks/engine.rs`); `approval_id` derivation `verification/state.rs:93`; park reuses 007's runner handoff | `engine/tests.rs:4382` `approvals_hold_only_their_dependents_until_decided` (independent progress, one occurrence per fanout item, rejection is data); `state.rs` `each_approval_occurrence_has_its_own_stable_id`; `pg_approvals.rs` decide test (CI evidence progresses independently while approval waits) | `cargo nextest run -p vala-verify --lib -E 'test(=blocks::engine::tests::approvals_hold_only_their_dependents_until_decided)'`; `-p wyrd-server --lib -E 'test(=verification::state::tests::each_approval_occurrence_has_its_own_stable_id)'`. The engine crate has no SQL dependency, so reaching an approval cannot issue a query; the occurrence carries no deadline field (`wyrd-spec/src/verification/state.rs` `ApprovalOccurrence`) | PASS |
| REQ-010c: decide route; normal auth; `verifier:approve` at exact Verifier scope only, audited; durable ACK before receipt; identical retry replays; conflict/expired/superseded/unknown refuse | `components/verification/service/approvals.rs`, `verification/approvals.rs` (fenced Scribe batch per approval_id), `routes.rs`; `Permission::verifier_approve{,_any}`; editor holds approve_any, workload does not | `pg_approvals.rs` `a_pending_approval_is_decided_once_and_resumes_the_same_execution` (viewer 403, misscoped Verifier 403, foreign tenant 404, unknown id 404, approve, replay, 3 conflicts 409, scoped approver rejects, audit rows denied/allowed under `verification.approval.decide`); `an_unacknowledged_decision_is_unavailable_and_its_retry_replays`; `permission.rs` `verifier_run_is_uid_scoped` approve cases; `builtin_roles.rs` workload never approves | server journey; `cargo nextest run -p wyrd-runtime --lib` | PASS |
| REQ-011: same ExecutionId continues; rejection is data failing an authored requirement; ACK→queue gap recovers; supersession/deadline fence parked work | `wake_decided` + 007 park recheck; requirement uses the standard `assert` check on `steps.<id>.output.status == approved` (no new helper: the existing check passes only approved and fails rejected; an expired occurrence never produces output and the execution ends `timed_out`) | `pg_approvals.rs`: decide test (approve → same id passes, reject → fails), `an_approval_expires_with_its_execution` (superseded old head and timed-out head both `expired`, decisions 409), `a_decision_made_while_stopped_finishes_after_a_restart` (ACK with no runtime, then restart passes), `a_decision_racing_supersession_releases_nothing` (accepted approval then supersession → cancelled, not passed) | server journey | PASS |
| REQ-016: inspection lists occurrences; shared client, Rust/Python/TS SDKs, CLI, MCP, OpenAPI, errors, docs, architecture | `wyrd-client verify/execution.rs::decide`; Python `PyExecution.decide` + `.pyi`; TS `Execution.decide`; CLI `wyrd execution approve|reject`; MCP `verification.decide_approval` gated by `may_approve`; docs `build-and-verify/verify/approve-a-pull-request.md`; `architecture/{wyrd-design,wyrd-security-posture,bifrost-design}.md` | Rust SDK `deferred_evidence.rs:294`; Python `test_deferred_evidence.py:114`; TS `deferred-evidence.test.ts:104`; CLI `github_journey.rs:666` + guide test `:753`; MCP `wyrd-mcp/tests/bifrost/mcp/github.rs:652`; `pg_openapi_contract.rs:1625` | each test by exact selector (journey profile for SDK/CLI/MCP); `mise run codegen:check`; `mise run docs:check`; architecture by content review | PASS |
| INV-003/004: positive receipt only after durable acceptance; unavailable state is never absent or a new approval; recovery after restart | fenced-only read: a fenced decision not yet visible is `ServiceUnavailable`, never absent (`verification/approvals.rs::decided`) | `an_unacknowledged_decision_is_unavailable_and_its_retry_replays`; `a_decision_made_while_stopped_finishes_after_a_restart` | server journey | PASS |
| INV-006: old-head, superseded or discarded occurrences never authorize current work | ids bind the execution (`approval_id` hashes the ExecutionId); decisions resolve only from the named execution's accepted state | `an_approval_expires_with_its_execution` (old head 409, new head has its own id); `a_decision_racing_supersession_releases_nothing`; `each_approval_occurrence_has_its_own_stable_id` | server journey; lib test | PASS |

Checks: `mise run fmt && mise run lints` (clean), `mise run py:format && mise run py:lints`, `mise run check:deps`, `mise run check:tenant-isolation` (after adding the sibling module's runtime-query justification to `wyrd-sql/.../verifier_executions/approvals.rs`), `mise run codegen:check`, `mise run docs:check`, `mise run test:sql`, `git diff --check`.

Deviations and exceptions:
- Decision custody: the 007 fenced Scribe acceptance was generalized to a sibling fenced-only table, `bifrost.verification.approvals`, rather than adding rows to the execution control-fact path; same ACK and fence semantics.
- RED was not captured: the server, SDK, CLI and MCP tests were written after the production code. This is a process exception. The occurrence-id and race tests prove behavior that was already correct.
- Accepted risk: a decision accepted in the same instant the deadline passes may show `approved` on an execution that ends `timed_out`; the verdict is not passed.
- Diagnosis (own new test): `an_unacknowledged_decision_is_unavailable_and_its_retry_replays` first observed `running` where it asserted `waiting`. Cause: the test published CI after the park, and that publication's wake resumed the execution. Fix site: the test now publishes before the pull request opens, so only the approval holds it. The same run exposed that a decision replay did not make the execution due, so it waited for the one-minute park recheck (63s). A replay now wakes it, as a publication replay does (`service/approvals.rs`), and the test settles in about 5s.
- Not separately proven: a concurrent revocation of the approver's grant racing an accepted decision. Authority is checked at decision time (misscoped 403), and the execution resumes under its own accepted authority, never the approver's.
- Authorization correction (maintainer direction): verification authorization is RBAC alone, and a caller's Card scope attributes and never authorizes. `VerificationControl::authorize_verifier` (renamed from `authorize_subject`) drops the Card-scope and root-required refusals for execute, enqueue, deferred evidence and approval. Decide loads the execution through the inspection reader with `Action::Approve`, so `verifier:approve` on the exact Verifier is its only permission, audited; it no longer needs `verifier:read` or `cards:read`. REQ-010, the design note, the security posture, the design, the route, MCP, SDK and error texts state this. Proof: `pg_approvals.rs` `a_pending_approval_is_decided_once_and_resumes_the_same_execution` (the scoped approver holds only `verifier:approve`); `a_service_account_decides_on_a_users_behalf` (RED before the correction: 403 "the caller's Card scope does not cover the verified subject"; now the delegated Service decides, the receipt and inspection name it, its retry replays, and the user directly and a second actor get 409); `pg_verification_routes.rs` `enqueue_is_authorized_by_rbac_alone` and `direct_execution_is_authorized_by_rbac_alone`; MCP `verification::pg_tests::direct_execution_is_authorized_by_rbac_alone`; `pg_verification_runtime.rs` `accepted_authority_survives_account_and_context_changes`. These replace the tests that asserted the removed check. Commands: the server integration selectors above plus `pg_approvals::` and `pg_deferred_evidence::` (22 passed); `-p wyrd-mcp --test mcp --run-ignored all` (2 passed); `-p wyrd-runtime -p wyrd-server --lib` verification, approvals and builtin-roles filter (87 passed); `lints`, `codegen:check`, `docs:check`, `check:tenant-isolation`, `git diff --check` clean.

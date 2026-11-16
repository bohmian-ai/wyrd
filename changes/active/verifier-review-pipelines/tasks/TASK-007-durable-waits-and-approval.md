---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-verifier-review-pipelines
spec_revision: 21
github_consumer_amendment_revision: 24
supplement: SUPPLEMENT-verifier-waits-and-approvals@4
contract_status: pending_remaining_decisions
depends_on: [TASK-001-R4, TASK-003, TASK-006]
req: [REQ-060, REQ-061, REQ-062, REQ-063, REQ-064, REQ-065, REQ-066, REQ-067, REQ-068]
inv: [INV-020, INV-021, INV-022, INV-023, INV-024, INV-025, INV-026]
ac: [AC-052, AC-053, AC-054, AC-055, AC-056, AC-057, AC-058, AC-059, AC-060]
---

# Durable delay, wait, approval and suspended execution

## Outcome and Value

A queued review pipeline can finish its currently runnable steps, suspend for a
human decision or time/action condition, and release its worker resources.
An authorized approval or due timer creates new work through SubmissionSink;
a worker restores the same execution from Bifrost and continues its eligible
steps. Completed reviewers are not run again.

For example, two reviewers finish and publish their reports. The Verifier waits
for human approval. No worker slot, renewing execution lease, provider connection
or sleeping task remains merely to await that approval. After an eligible human
signs in recently and decides, a new queue item references the original
ExecutionId. The next worker restores the reports and decision, then proceeds
through ordinary conditions and requirements.

This task implements the new capability requested by the user; it does not
reopen TASK-001 or absorb its remediation. All TASK-001 follow-ups remain in
R4. Delay/time and approval/action are triggers of an existing execution's
resumption, not new Trigger Card kinds or new workflow instances.

## What Changes and What Must Stay True

**GitHub consumer alignment (revision 24):** use the verified installation and
Check association supplied by [replacement TASK-004](TASK-004-installable-github-app.md).
Wait/resume must not repeat installation/onboarding, obtain authority from a
GitHub callback, or move Wyrd workflow logic into bohmian-code-review. GitHub
user consent is not Wyrd recent-human-login proof and cannot satisfy Verifier
approval. Recheck the existing connection/repository authority before resumed
source IO or Check delivery. No new GitHub policy decision is introduced here;
this clarification does not approve the unrelated wait/approval draft below.

**Approved settlement rule:** persist and ACK the canonical suspended checkpoint
before retiring the current queue delivery and releasing all active resources.
Keep stopped computation distinct from durable pending delivery. Suspension is
nonterminal for the workflow; the future trigger admits new delivery through
SubmissionSink. Reuse the R4 delivery boundary, without keeping a parked worker
or selecting any unrelated open wait/approval policy.


The user approved SubmissionSink as the shared admission owner and a fresh
queue work item for a continuation of the same execution. A work-item identity
identifies one delivery; ExecutionId continues to identify the entire workflow.
Retries of one delivery preserve its identity. A new genuine continuation has
another work-item identity. Adapt existing queue coordination and migrations;
there is no second queue service or PostgreSQL execution-state store.

| Owner | Responsibility |
| --- | --- |
| Existing Verifier planner/walker/RunState | Record a pending logical wait, keep peers concurrent, return suspension when no work remains active/runnable, and restore/resolve accepted work. |
| Existing canonical Bifrost records/shared writer | Retain wait/timing, continuation input and approval provenance, resolved decisions and compatible checkpoints. Preserve exact File custody and authoritative recovery. |
| Server control/auth owner | Authenticate the command's caller, enforce recent interactive human proof and current approval authority, persist the decision/input and stage admission. Never substitute approver authority for execution authority. |
| SubmissionSink/SubmissionOutbox | Commit initial or resume queue admission. It does not execute Verifiers or decide their policy. Bifrost does not insert PostgreSQL rows. |
| Existing scheduler/queue/claim loop | Discover due coordination, admit/claim eligible work with normal bounds, and prevent ordinary concurrent deliveries of one execution. |
| Runner/HeldLease/LeaseRenewal | Run admitted work; release permit, active future and renewal registration when suspended. Renew only active work. |

Only the pending step's dependents wait; independent ready work continues. A
fully suspended execution retains durable evidence and bounded coordination,
not active execution resources. Preserve original deadlines, accepted authority,
spend and computation attempts across every delivery. Normal suspension and
resumption are not failed attempts; actual failures keep existing retry bounds.

Resume admission cannot move an initial schedule/group cursor, create another
execution, duplicate source admission or independently create another Operator
or GitHub outcome. Existing final-result delivery remains independent.
Cancellation/revocation/supersession and expiry prevent unauthorized work and
apply to parked coordination as well as active deliveries.

No new SQL checkpoint head/writer/revision authority, payload mirror, workflow
service, writer, lease scheduler, unbounded event/history scan or per-wait actor
is permitted. Full snapshots remain the selected checkpoint representation;
continuation facts are inputs to that state, not another checkpoint head.
Preserve the explicitly accepted rare delayed-old-worker overlap and current
Bifrost retained-volume failure domain. No universal exactly-once external-effect
or permanent-volume-loss guarantee is added.

The supplement records remaining material decisions. This task is proposed,
not ready while its contract_status is pending. Neither the implementer nor
this task may silently choose an entry-mode restriction, rejection policy,
new public permission/route schema or stronger ownership fence.

## Approach

1. Extend existing Verifier and execution contracts for pending/resolved waits,
   timing and authenticated continuation facts, with versioned recovery and
   explicit supported entry modes from the approved supplement.
2. Extend the walker and host transition so waiting branches release their
   capacity and fully suspended work ends the active runner/lease lifecycle.
3. Extend SubmissionSink and the existing queue for initial/resume delivery
   identities, due-time activation and coherent claim/cancel/settlement behavior.
4. Connect authenticated action/approval receipt and timer eligibility to
   canonical Bifrost state and resume admission, including bounded recovery of
   interrupted handoff and stable retry identity.
5. Project inspection/control through existing HTTP/client/SDK/CLI/MCP owners,
   recent-login flows and code-review status; add complete examples and focused
   user journeys plus negative and crash-boundary proofs.

## Ordered Implementation Scenarios

### Scenario 1 — Suspend a timed step and free the worker

**Behavior.** A declared timer wait becomes pending with one fixed wake time.
Ready peers continue. When all runnable work finishes, the engine records its
suspended state and the runner releases its permit, lease renewal, active future
and transient execution resources. It does not select a terminal result.
Restart preserves that wake time and the original deadline. Already due timers
become eligible once; suspension never restarts the full duration.

**RED.** Add the planned [timer journey](#p1-timer-and-resource-release) through
the public client and real server, supported by controlled clock/runner seams.
Observe both a waiting branch beside a running peer and the fully parked
execution. Assert released capacity/renewal and no terminal result; the missing
wait action/transition supplies RED. Confirm the registered selector first.

**GREEN.** Extend existing action validation, logical state/walker and runner
transition. Persist suspension before releasing ownership, and retain bounded
coordination so process loss before parking converges after restart.

**REFACTOR.** Reuse ordinary dependency readiness and admission. Do not replace
the wait with a sleeping worker or add another graph executor. Keep existing
concurrent-step and direct fast-return contracts green.

### Scenario 2 — Admit a new delivery for the same execution

**Behavior.** Due time or an accepted continuation passes through SubmissionSink,
which inserts a new queue work item for the existing execution. Claiming it
restores accepted state and continues only eligible work. Matching queue retries
and an ambiguous COMMIT response preserve one logical delivery. Separate
continuations cannot intentionally run the workflow concurrently. Initial
idempotency, schedule/group occurrence and cursor behavior remain unchanged.

**RED.** Add the planned [queue integration proof](#p2-resume-delivery-identity)
using the real SQL owner and extend the public timer journey to assert stable
ExecutionId and unchanged predecessor call counts. Current admission deduplicates
by ExecutionId and cannot represent the required new delivery. Confirm selectors
before RED; never use a new execution as a workaround.

**GREEN.** Extend existing admission/queue types, ordered migrations and runner
claim/settlement together. Preserve execution-wide controls and scope lease
checks to the actual delivery. Keep current initial admission behavior and
retained installed data; do not rewrite applied migrations.

**REFACTOR.** Keep one SubmissionSink, one queue and one runner path; remove
assumptions that queue-row identity and logical ExecutionId are interchangeable.

### Scenario 3 — Resolve human approval through current human authority

**Behavior.** An eligible human with the selected exact-Verifier approval grant
and recent interactive login can submit a decision for the pending logical
step. User API keys, stale login, delegated/service automation without human
proof, wrong grants and foreign tenants cannot satisfy approval. The decision
and actor remain attributable to the original evidence/head. Application outcome
on rejection follows the supplement's selected policy once agreed; no
unapproved self-approval or automatic-cancellation rule is introduced.

**RED.** Add the planned [approval journey](#p3-human-approval) and
[browser-authority seam](#p6-http-and-human-proof) through the actual server
session/auth path. Assert both continuation and credential/tenant/current-grant
negatives. No bearer-to-human test shortcut is allowed. Confirm selectors first.

**GREEN.** Extend existing control and browser-auth owners, feature permissions
and canonical decision facts. Stage resume admission through SubmissionSink;
the resumed execution retains its originally accepted authority and budgets.

**REFACTOR.** Share current authorization, audit and recent-login checks rather
than inventing a second human-presence token or approval-specific identity store.

### Scenario 4 — Survive interruption between accepted state and queue admission

**Behavior.** Crash after pending-wait acceptance but before parking; after
continuation acceptance but before queue COMMIT; and after queue COMMIT before
reply. Restart restores canonical work and makes accepted continuation eligible
without a second human approval, duplicate delivery or rerun of settled work.
Temporary source unavailability cannot imply absence. Retry of an applied
command returns its recorded outcome; conflicting commands and expiry/cancel
races follow the selected contract under existing controls and accepted limits.

**RED.** Add the planned [handoff recovery journey](#p4-interrupted-handoff),
using current writer/queue fault seams and retained-volume restart. Stop the
process at each boundary, not only after a successful end-to-end response.
The existing in-memory post-acceptance handoff cannot supply this proof.

**GREEN.** Complete bounded server-owned recovery/delivery through the selected
SubmissionSink/queue mechanism. Preserve stable command/delivery identities,
source completeness and bounded tenant-fair work. A callback alone, requiring
another approval, or an unbounded full-history scan does not satisfy recovery.

**REFACTOR.** Reuse the existing outbox/queue retry and clock owners. Keep payload
custody in Bifrost; do not fill the gap with a SQL event/outcome mirror or a
new distributed checkpoint ownership protocol.

### Scenario 5 — Resume within original authority, limits and composition

**Behavior.** Repeated deliveries retain original deadline/spend/retry bounds;
normal suspension consumes no failed-computation attempt. Cancellation,
revocation, original deadline and a superseding PR head prevent unauthorized
continuation, including after parking. Fanout/repeat targets retain their exact
logical occurrence and compatible state survives restart. Unsupported state
fails explicitly, without dispatch or fresh reset. Apply the supplement's chosen
entry-mode, early-input and composition behavior rather than invent exclusions.

**RED.** Add the planned [lifecycle/control journey](#p5-lifecycle-and-controls)
and controlled engine/schema cases. Exercise both queued waiting and a ready
peer, expired/cancelled/superseded work, retry ceiling, wrong occurrence, and
new/old format boundaries. Confirm selectors and expected missing assertion.

**GREEN.** Extend existing lifecycle/cancellation/readiness and compatibility
owners across new queue deliveries. Enforce existing bounds at re-admission
and protected work, with no silent deadline extension or budget reset.

**REFACTOR.** Keep one source of terminal facts and existing authority checks;
remove obsolete single-delivery assumptions rather than add bypasses.

### Scenario 6 — Operate and inspect waiting reviews through shipped surfaces

**Behavior.** A client submits a review, sees pending waits and their evidence,
causes authorized action/human continuation through the chosen shared contract,
and reads the final result under the same execution ID. HTTP, Rust/Python/TS,
CLI and MCP expose equivalent server behavior; the optional UI is a projection.
GitHub remains nonterminal while required approval is pending and publishes
its existing exact-head terminal outcome independently. Examples cover delay,
action wait and human approval, restart, timeout and rejected decisions.

**RED.** Add the planned [Rust public story](#p7-public-client-story) and focused
[Python/TS/MCP closures](#p8-language-and-mcp-closure). Extend existing
HTTP/OpenAPI and UI/docsite cases for selected routes/statuses. Assert actual
server state and negatives, not only schema availability. Confirm selectors.

**GREEN.** Complete pure contracts, thin client/native projections, discovery,
errors and docs/examples together. Reuse server-owned recent-login behavior;
UI/session convenience cannot become execution authority.

**REFACTOR.** Use one authored fixture story across language surfaces and extend
existing inspection/delivery owners. TASK-006 supplies the existing inspection/education baseline;
this task owns all new capability-specific consumer gaps and its integrated story.

## Acceptance Criteria

| Obligations | Proof and violation |
| --- | --- |
| REQ-060/061; INV-020; AC-052/053 | P1/P5: fixed-time restore, peer progress and full worker release; retained future/permit/renewal or reset duration violates suspension. |
| REQ-068; INV-025/026; AC-060 | P2/P1: new delivery through SubmissionSink, stable ExecutionId and deduplicated COMMIT retry; a new workflow or skipped resume insert violates identity. |
| REQ-062; INV-021; AC-054 | P3/P6: actual recent human proof and exact approval authority; accepting an API key/delegation or using approver privileges for work violates approval. |
| REQ-063/064; INV-022/023; AC-055/056 | P4/P2: restart at every cross-owner boundary without another decision or repeated settled computation; orphaned canonical continuation or SQL payload authority violates recovery. |
| REQ-065; INV-024; AC-057 | P5: original bounds, cancellation/revoke/supersession and honest compatibility; refreshed budgets/authority or revival violates controls. |
| REQ-066/067; AC-058/059 | P7/P8/P6 and targeted UI/docs: same control/inspection across surfaces, original result identity and GitHub lifecycle; UI-only authority or completed status while required waits remain violates the contract. |

## Implementation Reference

Source inspected on candidate cd905f4cd. Revalidate after prerequisites integrate;
R3 removed rejected SQL-head assumptions; R4 completes uniform terminal settlement before this task extends the owners.

| Existing owner/symbol and consumers | Precise missing behavior and selected reuse |
| --- | --- |
| wyrd-spec card/verifier Action, Step, verification::ExecutionState and execution projections | Actions currently extract/agent/code; checkpoints contain invocation/publication/terminal state. Extend these contracts for the approved waits/decisions with versioned custody, not another workflow model. |
| vala-verify VerificationEngine::with_checkpoints, walk/RunLedger/Scope; server RunState | Existing restore skips settled blocks. Extend readiness/settlement for pending/resolved waits and host suspension without an indefinite awaited worker. Engine scripted tests support controlled clocks/order; public journeys still required. |
| server verification::SubmissionSink::write, SubmissionOutbox, Queued; ScribeSink::write | Current server post-input-ACK handoff stages Submitted/Scheduled/Grouped only and lives in process memory. Extend this shared admission owner for continuation references and bounded recovery; Bifrost itself does not enqueue. |
| wyrd-sql VerifierExecutionQueue::accept_submissions and INSERT_SUBMISSIONS_SQL | Current replay is keyed by ExecutionId; existing rows cannot represent independent resume deliveries. Extend the existing coordination schema/queries and all claims/cancel/complete/group consumers coherently. Forced RLS, database clock and immutable ordered migrations remain mandatory. |
| server ClaimLoop::round, VerifierRunner::process/execute, Transition, HeldLease::drop, LeaseRenewal | Claim retains semaphore through process; runner renews lease throughout attempt; existing Transition has no Suspend. Extend lifecycle so fully parked work ends the active future and renewal registration, with ordinary bounded re-admission. |
| server components::verification::VerificationControl/routes; shared client verify::Execution | Existing control supports status/cancel/inspection. Extend selected operations here and project them to SDKs/CLI/MCP rather than a UI-owned control path. |
| server auth::browser::RecentHumanLogin; principal routes using that proof | Existing password/SSO proof is at most five minutes and excludes bearer/API-key/recovery sessions. Reuse this owner with current origin/CSRF protections, selected grant and actual interactive flow. |
| server scheduler/groups/dispatch; TASK-004 repository automation; TASK-006 inspection/examples | Initial occurrence/cursor and delivery effects must not repeat on resume. Extend existing status/read/delivery projections and canonical examples; add no new Trigger/Operator kind or special review engine. |

Existing canonical execution_events currently describes full checkpoints, not a
proven independently writable continuation inbox. TASK-007 must extend canonical
record/input semantics explicitly, preserving checkpoint revision meaning and
command provenance. It must not pretend command receipt is already implemented
or overwrite a full checkpoint from an idle HTTP handler. The approved persisted
representation and command/ACK semantics must be fixed in the supplement before
GREEN; a new storage owner requires source-backed specification authority.

## Verification and Evidence

No runtime proof is claimed during planning. All selectors below are **planned**;
existing packages/targets/setup were checked from Cargo.toml, module roots,
TESTING.md and mise tasks. Register new thematic modules in the existing target,
then list/confirm nonzero exact selectors before RED. Do not add a test binary,
feature or full-suite requirement merely to host these cases. Store named
commands/results here as implementation evidence.

### P1 — timer and resource release

Planned real-server/public-client journey in the existing wyrd-testing server
target. Controlled signals and time establish active resource release; no
wall-clock hours or synthetic host load.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_waits::timer_wait_suspends_and_resumes_same_execution)"'
```

### P2 — resume delivery identity

Planned SQL seam plus real P1/P3 consumer closure. The test asserts separate
queue deliveries under one execution, normal mutual exclusion, initial-source
idempotency and ambiguous COMMIT retry.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_verifier_executions::resume_items_keep_execution_identity_and_deduplicate_delivery)"'
```

### P3 — human approval

Planned journey in the same server binary. It uses real recent-login/session
proof and the selected approval policy, including negative authority cases.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_waits::human_approval_resumes_the_suspended_review)"'
```

### P4 — interrupted handoff

Planned multi-process/fault-controlled journey in the existing server target;
use WyrdTestCluster/repository restart controls and preserve storage.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_waits::accepted_continuation_recovers_after_admission_interruption)"'
```

### P5 — lifecycle and controls

Planned focused server seam for timeout/revoke/cancel/supersession/attempts and
composition. Pure engine/schema cases complement this real seam; register their
exact actual selectors before running them, without inventing a private API.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::suspended_work_preserves_original_limits_and_authority)"'
```

### P6 — HTTP and human proof

Planned route/auth seam in the existing integration target; browser source and
existing pg_browser_sessions prove this target owns cookie/recent-login checks.
Selected contract and exact stable errors must be asserted, including OpenAPI.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_routes::approval_requires_current_grant_and_recent_human_login)"'
```

### P7 — public client story

Planned Rust SDK story in the existing wyrd-ai integration target. Use checked-in
Cards and public SDK handles; compare final ExecutionId and provider call counts.
Existing CLI entry points drive the same story; register their exact selectors
when extending that owner rather than shelling out from the SDK test.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=durable_verifier_work::review_waits_and_continues_under_the_same_execution)"'
```

### P8 — language and MCP closure

Planned focused cases in inspected existing integration files/targets. They
exercise the selected action control and human-interaction requirement, including
API-key refusal; they must not emulate interactive human login inside Rust.
Run existing build/setup prerequisites once from verification.md for changed
native/browser owners, then the exact cases below.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_durable_verifier_work.py::test_waiting_review_resumes_the_same_execution'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/durable-verifier-work.test.ts -t "^waiting_review_resumes_the_same_execution$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::waiting_review_resumes_the_same_execution)"'
```

Use only applicable shared owner checks in verification.md: fmt/lints,
codegen, dependencies/tenant isolation, language/native typing and targeted
examples/docs checks for the actual write set. No broad gate or complete
language suites per task. No performance claim is made without measurements.
Task-local UI/example inspection extends existing browser/docsite controls;
TASK-007 owns the added integrated story after the TASK-006 baseline, without
assigning missing feature APIs or examples back to that earlier task.

## Material Stop Conditions

This task is not ready while the supplement has unresolved public/persisted,
entry-mode, authorization/approval-policy or cross-owner recovery decisions.
Do not turn open decisions into implementation defaults. In particular:

- Do not add a new Trigger kind, arbitrary event-name bus, separate queue/lease
  service, SQL checkpoint head, payload mirror or per-wait worker.
- Do not widen accepted execution authority, accept automation as recent human
  login or silently select self-approval/rejection semantics.
- Do not treat an in-memory callback as durable activation or change Bifrost's
  failure domain/accepted rare-worker limit to make a test pass.
- Report any owner limitation requiring a new public/persistent boundary with
  concrete source evidence; keep reversible private mechanics implementation-owned.

## Authority Links

[Parent specification](../spec.md) · [Wait/approval contract](../wait-and-approval.md) ·
[Public contracts](../public-contracts.md) · [Task navigation](README.md) ·
[Shared verification](verification.md) ·
[Existing R4](../review/task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md) ·
[Wyrd design](../../../../architecture/wyrd-design.md) ·
[Bifrost design](../../../../architecture/bifrost-design.md) ·
[Security](../../../../architecture/wyrd-security-posture.md) ·
[Implementation execution](../../../implementation-execution.md) ·
[Testing](../../../../TESTING.md).

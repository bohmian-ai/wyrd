---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 8
supporting_spec: SPEC-authorization-model
supporting_spec_revision: 9
requirements: [REQ-005, REQ-008, REQ-009, REQ-014, AC-004, AC-005, AC-008, AC-009]
depends_on: [TASK-002]
---

# Deferred inputs and exact approval decisions

## Outcome and activation

A reader can distinguish missing inputs from pending human decisions, inspect
one reached occurrence while peers run, and, when authorized, approve or reject
that exact occurrence. The recorded receipt and continuation belong to the
same execution. Read access alone does not permit approval.

Approved revision 8 records REQ-008's execution-owned expiry, exact-Verifier
approve plus parent read, definition-independent occurrence selection and the
selected action presentation. Record the integrated inspection/decision inputs
and their actual dependency proof before implementation; approval is not
runtime qualification. Do not use superseded approve-only instructions.

The selected [resumption design](../design/resume-005-007.md#proposed-task-006-revision-inspect-and-decide-exact-approvals)
extends TASK-002's inspector and host action/state owners. TASK-005's shared
definition-unavailable selection can be reused when integrated, but PR-set
delivery and TASK-007's backend correction are not prerequisites. If this task
runs first, finish only its occurrence-selection seam at the shared owner;
do not duplicate the general inspector.

## Inputs, owners and connected implementation

The existing execution-status operation supplies `waiting` and `approvals`
without completed details. A reached occurrence has its `approval_id`, step,
item/iteration, message, pending/approved/rejected/expired state and recorded
actor/time/reason. It has no requested-at or expiry timestamp. The execution's
original deadline, cancellation or supersession owns expiry; the UI states that
relationship and displays recorded expiry, without a countdown or new deadline.

Consume the existing HTTP
`POST /v1/verification/executions/{execution_id}/approvals/{approval_id}`,
TS `Execution.decide()` and MCP `verification.decide_approval`. Rust's
`VerificationControl::decide_approval` and retained authorization owner require
`verifier:approve` on the exact Verifier plus read on the trusted execution
parent (Repository for PR work). Referenced-Verifier definition access is
independent. Current grants, credential ceilings and parent visibility remain
server-enforced on each action; no role-name gate or copied permission evaluator.

Extend the existing inspector, typed catalog emits and host-action handling:
component → validated exact occurrence intent → host forwards the existing
session/SDK/MCP write → stable receipt/problem → authoritative occurrence/status
readback. Reuse cancel's forwarding/action-state pattern and the established
safe problem projection and Origin/CSRF/session boundary. Writes never enter
the read executor. Keep credentials with hosts; honor existing MCP write scopes
and discovery/exposure rules. An unsupported host reports unavailable and emits
no write.

Use available host action support and informational operation hints for
presentation only. An absent approve operation can explain read-only state;
its presence cannot establish exact target authority. Without a reliable hint,
a supported host can submit and show the server refusal. Do not add a capability
endpoint or promise every scoped denial is known before submission. No fresh
login ceremony is introduced.

Guard pending submission by execution + occurrence, not a step name alone.
A durable receipt acknowledges one decision, not completed continuation.
Resolve an ambiguous answer by reading that occurrence before an explicit
same-decision/reason retry; never automatically issue another decision or
transfer a response to a newer selection. Reuse authoritative cancel/terminal
handling and retain accepted outputs. No runner, approval dashboard, browser
resumption/deadline loop, server/SDK field, migration or durable backend change.

## Ordered acceptance scenarios and evidence

Add actual assertions, exact selectors, RED/GREEN and limits to each entry.
Current backend behaviors may supply already-green prerequisite proof; new UI
behavior remains test-first.

1. **Inspect reached waits without completed details.** Actual public
   verification creates a missing input, pending approval and running peer.
   Load status through shared Node/MCP readers and select the occurrence in
   the inspector, including without a readable definition. Message, exact
   ID/step/item/iteration and recorded state appear independently of details.
   When every runnable branch parks, waiting remains nonterminal/no verdict.
   **Map:** REQ-005/008/014, AC-004/008. **Tier/owner:** existing shared component,
   real-server library and BFF/browser integration owners. Missing occurrence
   rendering/selection should produce RED.
   - Evidence. [I1] `ui-library.integration.test.ts:2485` "TASK-006: each reached approval occurrence is read without the Verifier…" (fixture `fixtures/cards/approvals/repeated.yaml`, `repeated-repository.yaml`): a real pull-request execution parks on a repeated
     `review` approval and a `release` approval while `ci` is deferred
     (`status: waiting`, `waiting: ['ci']`, no `result`). A Repository viewer
     with no Verifier read loads it through Node and MCP: `definition` is not
     populated, and `approvals` lists `[review, 0, 'Reviewed?', pending]` and
     `[release, null, 'Release?', pending]`. Rendered, the "Recorded without
     the definition" group shows `Step release` as waiting, and
     `[data-approval=<id>]` reads `<execution> · step review · iteration 0`.
     [U1] `ui-library.test.ts:3213` "TASK-006: a waiting execution shows each reached approval occurrence…" (definition refused through `fixtureRecording`): the summary fact
     "Waiting for input" names `eval_report` apart from the approvals. Recorded
     steps read `gather succeeded`, `audit started`, `release ◷waiting`,
     `review ◷waiting`. Each occurrence shows its status, message, exact
     occurrence/step/item/iteration and lifecycle wording. Status stays
     `waiting` with no verdict. [B1] `ui-pages.integration.test.ts:2643` "TASK-006: an approver reads each waiting approval in the execution page…": the page reads "no verdict recorded" and
     "Waiting for input".
   - RED: [U1] failed on the missing "Waiting for input" label (missing
     behavior). GREEN: [U1], [I1], [B1] pass.
   - Limit: the running peer is covered by the [U1] fixture (`audit started`);
     the real Verifier in [I1] has approvals and a deferred input but no
     long-running peer step.

2. **Submit one exact decision and consume its receipt.** As a reader with
   exact-Verifier approve plus parent read, approve one occurrence with a reason
   and reject another without one, including distinct repeated/fanout occurrences.
   UI/host dispatch carries the selected IDs/body, guards duplicate pending
   submission and shows the durable actor/time/reason receipt. Authoritative
   readback then shows continuation under the same execution/accepted authority;
   receipt arrival alone does not claim completion or retained progress.
   **Map:** REQ-008/014, AC-004/008. **Tier/owner:** unit intent/dispatch checks
   and real public backend → Node/MCP/custom/built host action → browser journey
   using existing UI test owners. Missing decision emit/BFF dispatch should
   produce RED; backend resumption/durability is reused.
   - Evidence. [U1] `ui-library.test.ts:3213` "TASK-006: a waiting execution shows each reached approval occurrence…": Approve emits exactly `{name: 'approval.decide', input:
     {execution_id, approval_id, decision, reason?}}`. `validateIntent` accepts
     it, the catalog says `completion: 'host-action'`, and `decision: 'expired'`
     is refused. While the action is pending, every decide control is disabled
     (one decision in flight per page) and a second click emits nothing. The
     host's answer shows only under the occurrence last sent (`decidedFor`).
     Reject with no reason sends no `reason`. Without `onaction`, Approve is
     disabled, so the MCP App artifact and `examples/ui-app` issue no write.
     [P1] `ui-pages.test.ts:739` "TASK-006: an execution page decides one exact approval occurrence once…": one `fetch(here + '/approvals/apo_release_0', {method: 'POST',
     body: {decision: 'approved', reason: 'metrics hold'}})`. A second click
     sends nothing. The receipt shows "Recorded approved by pr_me at … The same
     execution continues from this decision; its status is read again.", then
     `invalidateAll` runs once. [I1] `ui-library.integration.test.ts:2485` "TASK-006: each reached approval occurrence is read without the Verifier…" (fixture `fixtures/cards/approvals/repeated.yaml`, `repeated-repository.yaml`): with both grants, rejecting
     `review` iteration 0 without a reason returns `200 {status: 'rejected',
     reason: null, actor_id}`. The same execution then asks again as a new
     occurrence, iteration 1. Approving that occurrence with a reason records
     it, its `reviewed` requirement is judged `passed`, and status stays
     `waiting` on `ci` with `release` pending under the same
     `execution_id`. [B1] `ui-pages.integration.test.ts:2643` "TASK-006: an approver reads each waiting approval in the execution page…": a real browser types a reason, clicks Approve on
     `review` and sees the receipt line and `approved`. It then rejects
     `release` from the keyboard (focus plus Enter). The server holds exactly
     `[review approved 'reviewed the diff'], [release rejected null]`.
   - Host route: `approvals/[approvalId]/+server.ts` validates the body
     (strict `decision` enum, `reason` 1–4096) and forwards it unchanged
     through the session to `POST /v1/verification/executions/{id}/approvals/
     {approval_id}`. Cancel and decide share `relayAnswer` in
     `server/problem.ts`.
   - RED: [U1] (missing emit) and [P1] (`approve()` never became disabled:
     no host dispatch). GREEN: all four.

3. **Refuse insufficient or changed authority.** Exercise a parent reader
   without approve, exact-Verifier approver without parent read, wrong-Verifier
   approve, both grants through a custom role, foreign IDs and grant removal
   after the page loads. Rust refuses unauthorized writes with the appropriate
   stable problem and makes no decision; the UI exposes no hidden occurrence or
   inferred authority. Supported hosts display denials, coarse hints do not
   bypass final checks, and unavailable hosts issue no write.
   **Map:** REQ-008/014, AC-004/008; authorization-model REQ-004/006.
   **Tier/owner:** actual scoped-grant server/HTTP inputs through existing
   library/host and production-browser integration owners. New UI refusal/
   availability wiring should produce RED; unchanged server checks may pass.
   - Evidence. [I1] `ui-library.integration.test.ts:2485` "TASK-006: each reached approval occurrence is read without the Verifier…" (fixture `fixtures/cards/approvals/repeated.yaml`, `repeated-repository.yaml`), through the host route as each principal's browser
     session: Repository viewer without approve gets 403
     `WYRD_PERMISSION_403_DENIED_RBAC`. Exact-Verifier `approver` without
     Repository read gets 403. `approver` on another Verifier plus Repository
     viewer gets 403. Another tenant's admin gets 404
     `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`. An invalid body gets 400
     `WYRD_SPEC_400_VALIDATION` from the host and is never forwarded. The
     occurrence stays `pending` after each refusal. A custom role
     `verifier_approver` (`verifier:approve`) on the exact Verifier plus
     Repository viewer may decide. After the page loaded, unbinding that
     principal's Repository viewer refuses its next decision with 403 in the
     same session, and `release` stays pending. [P1] `ui-pages.test.ts:739` "TASK-006: an execution page decides one exact approval occurrence once…": a 403 shows the stable
     code under the occurrence, which stays readable and `pending`. [U1] `ui-library.test.ts:3213` "TASK-006: a waiting execution shows each reached approval occurrence…": a
     denied note appears only under the occurrence that was sent.
   - Decision: no permission hint. The session's permission strings are not a
     verified contract, so the UI infers no authority. A supported host submits,
     and the server's refusal is the answer, as REQ-008 allows. Hosts without
     `onaction` keep the controls disabled.
   - RED: the refusal wiring is in the [P1] RED above. The server checks
     already existed and passed on first run.

4. **Read back ambiguous, stale and conflicting answers.** Let the real backend
   commit a decision while its response is lost. The host reads that exact
   occurrence before an explicit retry; one receipt/continuation is observed.
   Same actor/decision/reason replay is stable; another decision/actor, expired
   or superseded work is explained through authoritative readback/problem.
   Select another occurrence while a response is pending: it cannot acquire
   the prior receipt. No new execution or automatic decision is submitted.
   **Map:** REQ-008/014, AC-004/008. **Tier/owner:** controlled transport around
   real backend decisions plus component/host/browser tests. Missing ambiguous
   response and exact-selection handling should produce RED.
   - Evidence. [P1] `ui-pages.test.ts:739` "TASK-006: an execution page decides one exact approval occurrence once…": on 409 the server's problem is shown and the status is
     read again. On a lost answer (fetch rejects), the reread finds the
     occurrence still pending: "No decision is recorded for apo_release_0;
     submit it again to retry." Nothing is resent. If the reread itself fails,
     the result is "Whether apo_release_0 was decided is unknown … Refresh
     before deciding again." and the control is released. An explicit retry
     sends the same decision and reason. On 503, the reread shows the server
     already recorded it: "The answer was lost; the server records approved by
     pr_me at …". Approve disappears, and there are 6 fetches in total, none
     automatic. [I1] `ui-library.integration.test.ts:2485` "TASK-006: each reached approval occurrence is read without the Verifier…" (fixture `fixtures/cards/approvals/repeated.yaml`, `repeated-repository.yaml`): a decision is committed and its answer discarded. The
     reader's reread shows it approved with the reason, and an explicit retry of
     the same decision and reason returns the identical receipt
     (`actor_id`, `decided_at`). Replaying the same rejection gives an equal
     response. A different decision gets 409. [U1] `ui-library.test.ts:3213` "TASK-006: a waiting execution shows each reached approval occurrence…": while one decision is
     pending, the other step's controls are disabled. The answer is the mock's
     page notice under the header, naming the occurrence it was sent for.
   - RED: [P1] (no dispatch or readback). The Wyrd Factory hook found two
     defects, both fixed before commit. A rejected `invalidateAll` left the
     action pending (the reread now settles, and cancel shares the fix). An
     unreadable status was reported as pending (it is now reported as unknown).
     GREEN: [P1] 23/23 in the file.

5. **Cancel or finish parked work and retain evidence.** Cancel actual parked
   work, race continuation/completion and reopen retained outputs. Server-owned
   terminal state wins, expired occurrences cannot be decided, accepted evidence
   remains inspectable and unavailable detail/definition never hides readable
   occurrences. Compare the approved inspector in both themes/viewports with
   keyboard/touch, long messages, pending/read-only/denied/error states.
   **Map:** REQ-005/008/009/014, AC-004/005/008/009. **Tier/owner:** existing
   real-server UI host/browser owners and component variants. Reuse cancel/
   backend race proof; add RED only for missing connected UI behavior. Record
   visual differences, including lifecycle wording replacing illustrative mock
   timestamps. Record AC-009 comparison evidence and get the maintainer's ruling
   on remaining mock differences under
   [wyrd-ui](../../../../.agents/skills/wyrd-ui/SKILL.md#matching-an-approved-mock).
   - Evidence. [I1] `ui-library.integration.test.ts:2485` "TASK-006: each reached approval occurrence is read without the Verifier…" (fixture `fixtures/cards/approvals/repeated.yaml`, `repeated-repository.yaml`): cancelling the parked execution through the cancel host
     route returns `cancelled`. `release` becomes `expired`, deciding it gets
     409, the invocations stay populated, and the rendered occurrence reads
     `expired` with no Approve. [U1] `ui-library.test.ts:3213` "TASK-006: a waiting execution shows each reached approval occurrence…": after `timed_out`, the occurrence reads
     ⧗ expired with no controls. Prerequisites:
     `pg_approvals::a_pending_approval_is_decided_once_and_resumes_the_same_execution`
     and `pg_approvals::an_approval_expires_with_its_execution` 2 passed (command
     under Verification).
   - AC-009 comparison: the mock's `selfcheck.mjs --shots` (`exec-approval`
     light, dark, mobile) beside [B1]'s `captureRepositories` of the pending and
     decided page at 1440 and 390, light and dark, with no sideways scroll. The
     first comparison was rejected by the maintainer, and the UI was realigned
     to the mock's source (`Inspector.svelte`, `StepNode.svelte`,
     `DepEdge.svelte`, `app.css`):
     - Header: the lifecycle badge plus dashed tags "waiting for input **ci**"
       and "waiting for approval". The verdict reads "none while running" (or
       "no recorded verdict" once settled).
     - Graph: each waiting node has the dashed warn border. A step's state sits
       on its second row ("◷ waiting for approval" / "◷ waiting for input")
       under its name. The missing input reads "not provided yet", and its edge
       is dashed.
     - Approval box: "Human approval" with the boxed state, the message, then
       `dl.dkv` rows (Occurrence, For, Expires, Decided) in 12px mono, stacked
       under 900px. Then "Reason (optional)", Approve and Reject, and
       "Submitting for X…" while busy.
     - Read-only and refused readers see the viewer note instead of the
       controls.
     - Decisions report on the page as the mock's `.banner` notices: "Accepted
       by the server: … no new work was started.", "Refreshed: X already
       rejected Y. Your approval was not applied.", "Refreshed: Y has expired.
       Nothing was applied."
     - Waitnotes on a step that waits for an input and on the missing input's
       panel.
     Asserted in [U1], [P1] and [B1] (`data-blocked`, `data-decision-notice`,
     `data-viewer-note`, `g.dep.awaited`, "not provided yet").
   - Differences that remain, with the maintainer's ruling (2026-11-24):
     1. Expires reads "with this execution · at its deadline, on cancellation,
        or when newer work supersedes it". There is no Requested row. The wire
        (`ApprovalOccurrenceState`) carries neither timestamp.
     2. For reads `… · step review · iteration 0` (or `item N`), not
        `attempt 1`. Occurrences carry no attempt number.
     3. The header badge reads the server's lifecycle `waiting`, where the mock
        shows `running`. The mock's header layout (title, description, Copy
        link / ← Verify / Cancel row) is TASK-005's panel and is unchanged here.
     4. The step panel head (name, kind, waiting tag, Overview/Outputs tabs,
        and Action / Depends on / Invocation rows) is the earlier tasks' step
        inspector, which shows "Step review · Recorded invocations" and "No
        invocation of this step is recorded yet."; unchanged here.
     Ruling: 1 and 2 are accepted as wire limits; no server change is made in
     this change. 3 and 4 are follow-up fixes to the tasks that own the header
     (TASK-005) and the step inspector (TASK-004/005), not TASK-006 scope.
   - Not reproduced: the integrated `development environment lifecycle`
     journey refuses to start while another local service holds port 8000.
     Every other journey in both integration files passed (24/24 and 21/22).
   - Limits: long messages and the touch layout are checked only at 390 in the
     captures, with no separate long-message fixture.

## Verification and handoff

Read `wyrd-ui` before edits. Use `ui-library.test.ts`, `ui-pages.test.ts`,
`ui-library.integration.test.ts` and `ui-pages.integration.test.ts`, with their
existing fixtures and runners. Add new focused selectors before RED, preserving
supported Node/MCP/custom/built-artifact writes and refusal/readback evidence.

Focused existing backend prerequisite regressions (repository-managed Postgres):

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_approvals::a_pending_approval_is_decided_once_and_resumes_the_same_execution) | test(=pg_approvals::an_approval_expires_with_its_execution)"'
```

Prepare `mise run ts:build`, `ts:build:testing`, `build:server` and the UI
build before focused UI integration under the managed Postgres wrapper with
`WYRD_UI_INTEGRATION=1`. Run `mise run ui:check`, UI build, catalog regeneration
and `mise run codegen:check` for changed schemas/catalog, plus affected BFF/
session owner checks including `mise run test:identity:ui` for new forwarding.
Do not add benchmarks or unrelated broad gates.

Run (2026-11-24): `mise run ui:check` exit 0 (117 passed); `mise run
codegen:check` exit 0; `mise run test:identity:ui` exit 0; `git diff --check`
clean; backend prerequisites 2 passed. Focused integration under
`scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm exec
vitest run <file> -t "TASK-006"`: [I1] and the cancel journeys 2 passed; [B1]
1 passed with `WYRD_UI_CAPTURE_DIR` set.

Planning runs no implementation or runtime verification. Backend approval/
recovery is consumed, not reimplemented; integrated change review owns complete
shipped-surface journey qualification.

## Revision 9 continuation — 2026-11-24

The maintainer approved server-owned task-local input-wait reporting in spec
revision 9. TASK-006A (`tasks/TASK-006A-task-local-input-waits.md` in this packet)
implements REQ-022/AC-015 and supplies the remaining simultaneous-state proof.
Earlier backend/architecture exclusions are superseded only for that work;
completed approval/idempotency and Origin evidence remains intact. The explicit
no-TDD exception applies only to TASK-006A. Prior recorded evidence and this
task's original revision metadata remain historical, not proof of 006A delivery.

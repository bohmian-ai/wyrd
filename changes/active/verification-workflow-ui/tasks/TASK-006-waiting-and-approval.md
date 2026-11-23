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

Planning runs no implementation or runtime verification. Backend approval/
recovery is consumed, not reimplemented; integrated change review owns complete
shipped-surface journey qualification.

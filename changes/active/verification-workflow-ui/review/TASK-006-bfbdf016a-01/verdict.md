# TASK-006 verdict: FIX_REQUIRED

Three independently validated findings remain within approved behavior. TASK-006 is not accepted yet. No specification change is required.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verification-workflow-ui-resume`.
- Branch: `task/verification-workflow-ui/TASK-005`.
- Base: `e6d7d2681e588f118ec7a8e9bbcfab24757a654b`.
- Candidate: `bfbdf016a5487cac17e8dc6fbb73cc8ae17a147f` (eight commits); review reads `git diff <base>...<candidate>` directly.
- Approved contract: `changes/active/verification-workflow-ui/spec.md` revision 8, supported by authorization-model revision 9.
- Original task: `tasks/TASK-006-waiting-and-approval.md`.
- Governing references: root AGENTS, `changes/spec-driven-development.md`, `TESTING.md`, `agent_docs/maintainer-style.md`, `wyrd-ui`, `architecture/wyrd-design.md:1794–1811`, and `architecture/wyrd-security-posture.md:195–203,697–714`. SQL AGENTS applies to the specialist's inspection of unchanged production query capabilities.
- First review; no prior TASK-006 findings or remediation rounds to carry forward. No `.codegraph/` exists. Existing TASK-005 review files were preserved.

## Independent coverage

`behavior_rev` performed discovery for assigned behavior, scenario coverage, consumer paths, test proof, recovery and visual/process evidence (`task-review-behavior.md`). `domain_rev` separately inspected exact occurrence, current permission and tenant boundaries, session/Origin forwarding and unchanged durable decision behavior (`task-review-domain.md`). Fresh `ponytail_rev` independently traced every proposed finding and reconciled coverage (`findings-validation.md`). The orchestrator did not act as a reviewer.

A separate domain reviewer is necessary because a new cookie-authenticated approval mutation crosses a sensitive authorization boundary. No separate invariant/system reviewer is needed: this diff changes client action state, not durable shared lifecycle or process recovery; primary and validator cover its pending/unknown/readback transitions. No broad repository or maintainer specialist is needed: no production Rust/SQL owner, crate layering, storage contract or substantial abstraction change is made. Existing backend and sibling forwarding consumers were inspected through the assigned primary/domain boundaries.

## Reconciled acceptance matrix

Paths are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless qualified. Findings are recorded once in `findings-validation.md` and referenced here.

| Requirement / original scenario | Implementation and assertion / execution evidence | Result |
|---|---|---|
| REQ-005/008/014, AC-004/008; scenario 1 | Component `VerificationExecution.svelte:69–85,256–289,363–382`, graph `execution.ts:300–320`; component test `ui-library.test.ts:3213`; real Node/MCP test `ui-library.integration.test.ts:2485` proves parked input/approvals without definition/details. Running peer remains fixture-only: FIND-TASK-006-2. | FAIL (evidence) |
| REQ-008/014, AC-004; scenario 2 | Closed `schemas.ts:735` intent/catalog, component exact ID emit, page dispatch and BFF route. Unit exact request/duplicate-pending assertions, real repeated-iteration reject/approve and durable receipt replay, browser `ui-pages.integration.test.ts:2643` approve with reason and keyboard reject. Same execution remains authoritative. | PASS |
| REQ-008/014, AC-008 and authorization-model REQ-004/006; scenario 3 permissions | Existing backend `VerificationControl::decide_approval` and retained inspection check trusted exact Verifier plus parent. Fresh real-server test proves parent-only/approve-only/wrong-Verifier refusal, foreign tenant 404, custom role success and grant removal in existing session; refused state remains pending. Unsupported host issues no write. | PASS |
| Task Origin/CSRF preservation and security posture 708–712; scenario 3 session boundary | Route requires session and validates body, but incoming missing/hostile Origin is replaced by trusted forwarding Origin. Source trace and specialist production-BFF sentinel HTTP proof: FIND-TASK-006-3. | FAIL |
| REQ-008/014, AC-004; scenario 4 | Conflict and successful recorded readback paths pass; real lost-reply receipt read/replay uses exact occurrence. Page unknown-result branch enables retry before successful readback; focused page test confirms it: FIND-TASK-006-1. | FAIL |
| REQ-005/008/009, AC-005; scenario 5 affected cancellation/expiry | Fresh library integration cancels parked execution, reads expired occurrence, rejects later decision, preserves recorded progress. Component timeout variant suppresses actions. Backend prerequisite results and unaffected terminal-race coverage are implementation-recorded/predecessor proof. | PASS for bounded affected behavior |
| REQ-014, AC-009; shared component/visual acceptance | Shared pure-prop component and host action; browser test uses real approval execution and captures 1440/390 light/dark with overflow check and keyboard reject. Maintainer explicitly accepts wire-limited timestamps/iteration wording and defers header/general step-panel differences to TASK-004/005. | PASS within accepted scope |

## Finding references and remediation

- FIND-TASK-006-1: unresolved decision resubmission before authoritative readback.
- FIND-TASK-006-2: missing connected running-peer evidence; no implementation defect asserted.
- FIND-TASK-006-3: incoming Origin replacement violates the explicit cookie-mutation boundary; browser CSRF exploitability is not established.

One bounded remediation task: [TASK-006-R1-approval-recovery-and-boundaries.md](TASK-006-R1-approval-recovery-and-boundaries.md). Preserve approved wire, durable ownership and maintainer mock rulings. No implementation or follow-on review was launched.

## Verification and process

Fresh on the candidate: `mise run ui:check` exit 0 (117/117 tests, package and host svelte-check zero errors/warnings); `mise run codegen:check` exit 0; `git diff --check e6d7d2681...bfbdf016a` clean. Specialist reran the exact managed-Postgres TASK-006 library integration: 1 passed / 23 filtered. Validator reran the exact TASK-006 page test: 1 passed / 22 filtered. The latter verifies current behavior, including the incorrect retry expectation; a green test does not close that finding.

Behavioral verdict: **FIX_REQUIRED**. TDD compliance: **PASS on recorded evidence**, with within-commit execution order not independently reconstructible; no contrary history or process violation established. Task records expected missing-label/emit/host-dispatch REDs and subsequent GREEN. This does not repair the behavioral findings or require historical test churn.

Implementation-recorded browser/identity/backend prerequisite runs were inspected but not all rerun. Full integration-suite lifecycle startup failure at occupied port 8000 was not independently reproduced or diagnosed here; it is not a focused TASK-006 pass and is not made a new bounded remediation obligation. No independent pixel recapture or separate long-message/touch fixture proof was obtained; user comparison/rulings and existing browser/component evidence bound visual acceptance. No benchmark or broad aggregate was requested. No new/moved backend hot-path wait arises from this client diff.

Review source and HEAD remained at the candidate; only new review reports/tasks were written. The task remains open until these findings close and the maintainer initiates remediation review.

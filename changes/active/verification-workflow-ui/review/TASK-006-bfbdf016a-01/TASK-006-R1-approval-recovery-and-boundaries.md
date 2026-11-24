---
id: TASK-006-R1
kind: remediation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 8
supporting_spec: SPEC-authorization-model
supporting_spec_revision: 9
parent_task: TASK-006
remediates: [FIND-TASK-006-1, FIND-TASK-006-2, FIND-TASK-006-3]
requirements: [REQ-005, REQ-008, REQ-009, REQ-014, AC-004, AC-008]
depends_on: []
---

# Resolve ambiguous decisions and preserve the approval request boundary

## Outcome and reviewed subject

A reader whose decision answer and status read both fail must refresh successfully before sending another decision. Cookie-backed approval forwarding must refuse missing or mismatched incoming Origin. One real verification journey must demonstrate a pending approval and missing input beside a running peer.

Original task: `../../tasks/TASK-006-waiting-and-approval.md`; approved verification-workflow-ui revision 8 and authorization-model revision 9. Reviewed base `e6d7d2681e588f118ec7a8e9bbcfab24757a654b`, candidate `bfbdf016a5487cac17e8dc6fbb73cc8ae17a147f`, branch `task/verification-workflow-ui/TASK-005`. Findings and evidence: `findings-validation.md`; acceptance decision: `verdict.md`.

## Bounded correction

**FIND-TASK-006-1 — recovery ordering.** REQ-008 / AC-004 and original scenario 4 require authoritative occurrence readback before explicit retry. The existing execution page marks an unknown result as error after failed refresh, which enables another decision immediately; `ui-pages.test.ts:783–803` asserts that behavior. Correct the existing page action/refresh availability owner and its shared control consumption so an unresolved occurrence cannot submit again until a successful authoritative read establishes its state. Keep refresh available and evidence readable. If still pending after readback, allow explicit retry; if decided/expired, display that recorded state. Preserve exact execution/occurrence identity, optional reason, duplicate in-flight guard, no automatic resend and server-owned first-decision finality.

**FIND-TASK-006-3 — incoming Origin.** Security posture 708–712 and the original task's Origin/CSRF boundary require refusal before forwarding a cookie mutation with missing/mismatched Origin. The new approval route calls `BrowserSession.api`, whose shared transport generates a trusted Origin without first validating the incoming header. Enforce the existing rule at the shared authenticated mutation request/forwarding owner, covering the approval route and affected sibling callers. Preserve tenant cookie selection, trusted public-origin configuration, read-only operations, bound callback exceptions and Rust permission checks. Update legitimate test requests as needed to represent valid browser Origin. No permission preflight, copied evaluator, new authentication API or session redesign is needed. The finding is the explicit boundary violation; do not claim a demonstrated browser CSRF exploit.

**FIND-TASK-006-2 — connected proof only.** REQ-008 / AC-004 and original scenario 1 require actual public verification with a running peer beside approval and missing input. `repeated.yaml` parks every runnable branch, while `audit started` appears only in the component fixture. Extend an existing focused real-server library journey using a controlled in-progress peer. Feed actual status/progress to the existing shared loader/component and assert the distinct states and absent verdict. This finding needs evidence, not production code without a separately established defect. Do not add definition-denial or decision-while-running combinations merely to close this gap; those behaviors have separate proof.

Primary owners are the conventional execution page, shared action presentation, existing authenticated BFF request/forwarding boundary, and existing `ui-pages.test.ts`, `ui-library.integration.test.ts`, and production-BFF browser/HTTP integration owners. Reuse their fixtures and public verification operations. Existing cancel consumers of the forwarding boundary remain covered by focused affected checks.

## Observable closure scenarios

| Scenario | Finding / requirement | Narrowest credible proof |
|---|---|---|
| Send one exact decision; lose its response and fail the occurrence read. Attempt both decisions and change the reason: no extra POST occurs. Successfully refresh; recorded decision/expiry removes the action, or pending status permits explicit retry. | FIND-TASK-006-1; REQ-008, AC-004, original scenario 4 | Update existing page test for dispatch/availability; one controlled transport journey around a real server decision through the existing UI integration owner proves committed-readback ordering. Assert request count and recorded occurrence. |
| With a live authorized tenant cookie, POST JSON approval with missing and mismatched Origin: stable refusal, no decision operation forwarded and no durable decision. Matching exact Origin reaches the existing operation and records the selected decision. Reads remain readable; affected sibling mutation forwarding keeps its boundary. | FIND-TASK-006-3; security posture 708–712, original Inputs/owners; REQ-008/014 | Focused production-BFF HTTP/browser integration using the existing session setup and real server; add a small boundary check if useful, but direct handler mocks alone do not prove the incoming request boundary. |
| Public verification has a pending exact approval, missing deferred input and an actual in-progress peer. Shared readers/component consume its recorded state and display separate wait reasons, running progress and no final verdict. | FIND-TASK-006-2; REQ-005/008/014, AC-004, original scenario 1 | Extend existing managed-Postgres real-server library integration. Controlled peer lifetime, no synthetic host load or new framework. |

Evidence (paths relative to `crates/wyrd/wyrd-server/wyrd-ui/`; commands under Execution and checks):

- **Row 1 — closed by maintainer ruling (2026-11-24): no client guard.** The server already makes the decision idempotent: the first decision is final, the same actor, decision and reason replay the receipt with 200, and anything else is refused `WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING` (`architecture/wyrd-design.md` approval steps; `crates/wyrd/wyrd-server/src/components/verification/routes.rs` `decide_approval` responses). A resend after a lost answer therefore cannot record a second decision, and the page's existing 409 reread already reports a decision someone else recorded. The page, component and `ActionState` contract are unchanged from the task commit. [B] `src/lib/ui-pages.integration.test.ts` TASK-006 browser test: after the keyboard Reject is recorded, resending the same decision with this session returns 200 `rejected` and the occurrences still read one rejection with no reason.
- **Row 2 — PASS.** Owner: `src/hooks.server.ts`, the one request owner every BFF route passes through before a session is read or `BrowserSession.api`/`client()` forwards the trusted Origin. Any method other than GET/HEAD/OPTIONS without exactly `event.url.origin` gets the stable `WYRD_AUTH_403_ORIGIN_REFUSED` (Rust's existing `WyrdError::OriginRefused`, added to `gen_schemas.rs` problem examples and regenerated). `/auth/*` keeps forwarding the incoming Origin unchanged to Rust, which enforces it and owns the bound callback. This covers the approval, cancel, GitHub connection and `ui/load` endpoints; form actions already had SvelteKit's check. [B] same browser test: with this session's cookie, a JSON approval and a cancel with no Origin and with `http://evil.test` each get 403 `WYRD_AUTH_403_ORIGIN_REFUSED`; the execution stays `waiting` with both occurrences `pending`; a GET of the page is 200; the matching-Origin Approve button then records `approved` with its reason. RED: `expected [200, undefined] to deeply equal [403, 'WYRD_AUTH_403_ORIGIN_REFUSED']` (missing Origin reached the decision). GREEN: 1 passed / 21 skipped. [Unit] `src/lib/server/auth/session.test.ts` "request hook refuses a mutation without this exact public Origin before any session is read": missing, foreign and other-port Origins give 403 with no session cookie read and `resolve` not called. The existing hook test's POST now carries its valid browser Origin. `problem.test.ts` checks the new example's metadata against `error.rs`.
- **Row 3 — PARTIAL; needs a ruling.** [I] `src/lib/ui-library.integration.test.ts` "TASK-006: a pending approval waits beside a step still running, then beside the missing input once it parks, with no verdict". New fixtures: `fixtures/cards/approvals/release-desk.yaml` (Service), `beside-review.yaml` (Verifier: `release` approval, `review-notes` agent step, deferred `ci`) and `notes-reviewer.yaml` (Agent). The server's provider holds the model answer until the test releases it. A Service-family viewer, with no Verifier read, reads through `loadExecution` over Node: status `running`, no result, `release` `pending`, and invocations `[['review-notes','started']]`. The component shows "waiting for approval", verdict "none while running" and recorded steps `Step review-notes started` and `Step release ◷waiting for approval`. After release, the same execution parks: `waiting`, `waiting: ['ci']`, `release` still pending, no result. The component then shows "waiting for input ci" and "waiting for approval" separately, still with no verdict. GREEN: 1 passed. No RED, because this is evidence only and no production code changed. **Gap:** the three states never appear together on the wire. `wyrd-spec` `ExecutionStatus.waiting` / `ExecutionState.waiting` lists deferred inputs only "when no runnable step remained; empty while it runs", and the server reports `waiting: undefined` while `review-notes` runs. A missing input beside a *running* peer would need either a server field or the UI deriving missingness from the definition. The first is excluded here (no backend fields); the second re-derives server meaning. The maintainer must rule: accept this as a wire limit, like absent timestamps, or route a server change through `$wyrd-spec`.

## Execution and checks

Read `wyrd-implement` and `wyrd-ui`; use the repository's task-first/RED→GREEN discipline for changed runtime behavior. Execute new exact selectors after confirming their names; record expected missing behavior and actual results in these scenarios. Existing baseline selectors are:

```sh
# From crates/wyrd/wyrd-server/wyrd-ui:
mise exec -- pnpm exec vitest run src/lib/ui-pages.test.ts -t 'TASK-006: an execution page decides one exact approval occurrence once and reads it back'

# From repository root, using existing built prerequisites:
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.integration.test.ts -t TASK-006
```

Run (2026-11-24, sequential):

- `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.integration.test.ts -t 'TASK-006|cancels once'`: RED (Origin), then GREEN; after the row 1 ruling, 3 passed (TASK-006, the sibling cancel journey and the Observe support journey whose gate teardown shares the file).
- `… ui-library.integration.test.ts -t TASK-006` under the same wrapper: 2 passed (new running-peer journey plus the existing decision journey).
- `mise run codegen:regen`, then `mise run codegen:check`: exit 0. `mise run ui:check`: exit 0, 118 passed, both svelte-checks 0 errors. `mise run test:identity:ui`: exit 0 (BFF journeys 5 passed; `identity_ui_e2e::production_ui_bff_journey` passed). `git diff --check`: clean.
- TDD: row 2 recorded RED before its production change; row 1's guard was removed by ruling. Row 3 is evidence only. Mechanical deviation: row 3 adds a test case beside the existing TASK-006 library journey rather than editing it, because it needs a held model provider that the existing journey's server lacks.

Use the existing production-BFF integration runner with the exact added/extended selector for Origin and controlled recovery evidence; prepare only its documented SDK/server/UI prerequisites. Run `mise run ui:check`, affected BFF/session tests including `mise run test:identity:ui` if the shared session boundary changes, `mise run codegen:check` only if schemas/catalog/generated owners change, and `git diff --check`. Coordinate Cargo-backed commands sequentially. No full journey suites, broad aggregate or benchmarks are required for this remediation; integrated change review owns them.

## Preserved scope

Keep durable approval/expiry/resumption in Rust, the existing endpoint and receipt wire shape, exact-Verifier approve plus retained-parent read, current grant checks and unsupported-host no-write behavior. Preserve accepted absent timestamps and iteration/item labels. Header and general step inspector mock differences remain TASK-004/005 follow-up. Do not modify architecture, add backend fields/migrations, redesign the inspector, or repair unrelated lifecycle port occupancy. Return implementation evidence; do not initiate review, commit, push or deploy.

## Revision 9 continuation — 2026-11-24

The maintainer approved server-owned task-local input-wait reporting in spec
revision 9. TASK-006A (`tasks/TASK-006A-task-local-input-waits.md` in this packet)
implements REQ-022/AC-015 and supplies the remaining simultaneous-state proof.
Earlier backend/architecture exclusions are superseded only for that work;
completed approval/idempotency and Origin evidence remains intact. The explicit
no-TDD exception applies only to TASK-006A. Prior recorded evidence and this
task's original revision metadata remain historical, not proof of 006A delivery.

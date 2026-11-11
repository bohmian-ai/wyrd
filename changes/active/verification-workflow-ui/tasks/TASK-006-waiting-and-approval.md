---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-008, REQ-009, REQ-014, AC-004, AC-005, AC-008, AC-009]
depends_on: [TASK-002]
---

# Deferred inputs and exact approval decisions

## Outcome and prerequisite

The existing execution page explains missing input versus approval waits while
peers run. An authorized user decides one exact pending occurrence and observes
continuation of that same execution; a viewer can inspect but cannot approve.

002 provides execution/instance inspection and host action/cancel behavior.
Backend GitHub tasks 007/008 must deliver waiting inspection, exact approval
occurrences/readback, approve scope, decision receipts/errors and same-execution
continuation. Their earlier backend dependencies are their responsibility;
this UI does not repeat that engine work or wait for provider delivery task 009.
Record actual integrated operations and real dependency proof, not task status.

## Flow and existing owner

Extend 002's typed props/loaders and shared inspector with generated pending
input names and approval occurrence identity, step/item/iteration, message,
expiry and recorded decision. Waiting remains nonterminal/no final verdict and
coexists with running peers. No separate approval dashboard or wait framework.

Use existing host action intent/state/session forwarding for the approved
`POST /v1/verification/executions/{execution_id}/approvals/{approval_id}`
decision shape. Preserve actor/decision/reason through the domain receipt;
optional reason is not mandatory. Current exact Verifier approve authorization
governs on each action; no fresh-login ceremony or browser-created authority.
Guard duplicate pending submission, refresh accepted state, and show stable
denied/error/expired/conflict explanations. An ambiguous response is resolved
through recorded occurrence readback, not a blind new decision or replacement
execution. Obsolete source/occurrence cannot authorize newer work.

Reuse 002's cancel action on parked work, keeping evidence and allowing terminal
completion to win. Superseded reads and later publications cannot switch the
selected approval/producer. Authoritative expiry/continuation remains server
behavior; do not implement a second browser deadline or resubmission loop.

## Observable proof

Through real backend/SDK-created parallel work, inspect a running peer alongside
missing input and pending approval, then observe all-blocked state without a
verdict. Public approval actions drive approved/rejected/expired outcomes,
stale/conflicting occurrence, denied viewer and foreign ID cases. Read back an
ambiguous response and assert a single recorded decision and continuation of
the same execution under its accepted authority. New source cannot inherit an
old decision. Backend durability/resumption proof is consumed, not simulated.

Exercise those actions through the production browser and existing supported
Node/MCP/custom/built-artifact host dispatch. Assert exact occurrence request,
optional reason, pending guards, authoritative receipt and safe error states.
Cancel parked work and race continuation/completion; reopen retained outputs.
Use existing owner tests and the verification commands below.
Compare waiting/approval inspector regions with the approved mock in light/dark,
mobile/desktop and keyboard interaction. No fixture action can replace a real
authorized decision and downstream continuation.

## Execution and verification

This task is proposed against draft spec revision 2. Activation requires
explicit spec approval, the applicable scope transfer, and only this task's
named contract/authority and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. Record the integrated source revisions
and actual dependency journeys it consumes; upstream task status is not proof.

Read the repo-local `wyrd-ui` skill before UI edits. Reuse the shared component,
loader, catalog, Node/MCP executor and host action owners. Test shared components
in `src/lib/ui-library.test.ts` and pages in `src/lib/ui-pages.test.ts`;
authorized host/built-artifact proof belongs to `ui-library.integration.test.ts`
and production BFF/Chromium/public-login proof to `ui-pages.integration.test.ts`.
Record actual focused selectors for added tests before their first run; follow
the existing TDD discipline without inventing test names in this task.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

Narrow test files with actual relevant selectors. The integration command needs
the existing `ts:build`, `ts:build:testing`, `build:server` and UI-build
prerequisites; inspect `mise.toml` and the managed Postgres wrapper before
running it. Missing prerequisites are not passing or skipped proof.

Extend normal `mise run dev:full` public setup where needed to demonstrate this
task's real inputs. Independently compare each introduced component/page with
the approved mock on desktop/mobile, light/dark, keyboard/focus/touch, long
content and error states. Record reference regions, screenshots and individual
human acceptance. Fixtures and automated checks alone do not prove fidelity.

Regenerate changed catalog/contracts through their owning generators and run
`codegen:check` when applicable. Changed Rust/SDK owners require their narrow
format/lint/type and real boundary journeys; preserve their existing tenancy,
Python stub and N-API checks. Do not add a runner or substitute fixtures for
an unavailable backend output. Integrated change review joins the delivered
flows and runs the required full shipped-surface journey lanes.

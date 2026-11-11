---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-009, REQ-014, AC-005, AC-008, AC-009]
depends_on: [TASK-002, TASK-005]
---

# Independent GitHub delivery status

## Outcome and dependency

An engineer viewing PR verification can distinguish the recorded verification
judgment from GitHub publication state. A provider retry changes delivery state,
not the execution or its verdict. This separate dependency-bound addition lets
the core viewer/PR navigation finish without waiting for report delivery.

Consume 002's execution page and 005's PR context, plus backend GitHub task 009's
redacted authorized delivery inspection attached to exact execution/provider
identity. Require delivered normal SDK/MCP projections and real provider
reconciliation proof. Backend owns report conventions, provider writes, retry
and recovery; this task consumes them and adds no retry controller or action.

## Flow and proof

Extend the existing shared execution/context loader props and supported host
adapters with the generated delivery projection. Show its state/reason safely
beside the execution context, separate from verdict. Unavailable/denied delivery
inspection is not failed verification or empty success. Every retained output
remains an equal ordinary publication; optional standard provider report support
does not create a primary output or a specialized finding UI.

Use retained outputs from actual admitted executions and backend-controlled
provider failure/retry/recovery. Through real public reads and the browser,
observe a stable selected result/verdict while delivery changes and retained
evidence remains readable. New head/history selection keeps exact delivery
association; foreign/denied reads reveal no provider details. Assert no UI
execution retry, recomputation or provider write. Use existing shared host and
production browser owners plus the verification commands below.
Compare the existing execution/PR context presentation with the approved mock;
do not invent a new delivery dashboard or management workflow.

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

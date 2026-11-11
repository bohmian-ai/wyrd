---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 2
requirements: [REQ-010, REQ-011, REQ-013, REQ-014, AC-006, AC-008, AC-009]
depends_on: [TASK-001, TASK-003]
---

# Repository Cards and GitHub connection settings

## Outcome and required backend outputs

The separate Repositories workspace exposes normal versioned Repository policy
and verification activity; Settings manages provider access. An accessible
provider repository can have no Card, while a registered Card can lack access.
Neither viewing a version nor connecting the App declares or selects policy.

Activate after backend GitHub task 002 supplies verified connect/read/disconnect
and redacted access projections, and task 003 supplies native Repository kind,
declaration receipts, selected policy and exact bindings. Require their normal
SDK/MCP operations, authorization and real journeys, with owning architecture
amendments delivered. 001 supplies definition rendering and 003 supplies filtered
Verify. Backend task numbers refer to the approved revision-4 packet identified
in [the specification](../spec.md#dependencies-authority-and-scope-transfer), not this packet's IDs.

## Flow and reuse

Reuse existing exact Card header/facts/version/relationship reads and typed
shared loaders. Extend Repository presentation from the normal Card envelope;
combine bounded authorized Card/access projections by stable provider identity
for display. Do not create another registry, browser authority or declarations.
Card and integration permission remain independent, and unavailable access
never becomes a guessed ready/inactive state.

Add Repositories beside Cards in the tenant navigation; leave existing Cards
browsing intact. Reuse tenant route/session conventions for index/detail,
preserving index search/filter state on return. Show accessible-but-undeclared
instructions, registered/ready, registered/access-lost and empty-bindings cases.
Detail shows viewed version separately from selected policy, exact Verifier
bindings/activation, read-only YAML/TOML and verification activity. Label
reconstructed TOML as a projection. Link definitions through 001 and existing
execution/filtered Verify through 003. 005 adds exact PR review-set navigation.

Settings → Integrations → GitHub consumes the singular existing connect flow,
provider navigation and server callback-completed association for Wyrd-first
and GitHub-first entry. Show safe configured host/App/account/access state,
provider manage-access navigation and authorized confirmed disconnect. Use
existing host ActionIntent/ActionState and current browser session; no component
credential, App-secret upload, second confirmation protocol or UI permission.
Pending/denied/error/readback comes from authoritative operations. Explain
affected unfinished cancellation and retained Card/evidence; do not delete or
hide authorized Cards after disconnect/access loss. Missing optional App
configuration makes only GitHub integration unavailable.

## Observable proof

Public backend connection → native declaration → typed shared loader → actual
Repository page must use the real predecessor receipts, not independent fake
records. Test both onboarding entry paths and normal auth callback behavior
through the existing real server/BFF lifecycle with the provider transport
controlled by the backend test owner. No production GitHub credential is needed
where the owner supplies its local provider substitute.

In the browser, start from Verify, visibly navigate Repositories, inspect all
registration/access/readiness cases, change viewed policy without changing
selection, open an exact Verifier/execution or filtered Verify, return with
index state and follow Manage connection into Settings. Exercise authorized
disconnect, pending guards, refusal/access-loss and foreign IDs; verify Cards
and retained evidence survive and backend cancellation is reflected. Assert
no secrets in props/HTML and no dependent sensitive read after denial.

Use existing shared host/built-artifact, UI unit and browser test owners plus
the verification commands below.
Compare Repository index/detail/no-Card and Settings regions with the sole
approved mock in both themes/viewports and keyboard/touch use. No folder-path
editor, activate/pause/rollback control or broader Changes UI is included.

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

---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-006, REQ-007, REQ-008, REQ-009]
depends_on: [TASK-001]
---

# Administer scoped access in the UI and prove connected journeys

An administrator creates a local User or family-bound Service, assigns scoped
access and can see where that access comes from. The person or workload then
uses the same account and grants through the CLI/SDK. A user who needs access
can copy a safe request, and an administrator can grant it without requiring
the user to sign in again.

## Approach and dependency

Consume TASK-001's working account/role/group/binding administration, validated
scope and access-summary contracts, local device approval/refresh, current
authority and filtered data reads. Use those actual outputs: no UI-local role
resolver, alternate credential exchange, inferred scope or duplicated filtering.
TASK-001 already proves backend enforcement; this task proves that the UI drives
it correctly and presents the resulting authority accurately.

Follow [administration and connected journeys](../design/notes.md#administration-and-the-connected-user-journey),
[the agreed task split](../design/notes.md#logical-task-grouping) and
[the spec](../spec.md). Apply `$wyrd-ui` for production UI work. Extend the
existing tenant Settings routes, server loads/actions and Rust-owned browser
session/device flow in `crates/wyrd/wyrd-server/wyrd-ui`, reusing its components,
theme tokens and error handling. Shared presentation belongs in `packages/ui`
only when another host consumes it. The existing Settings/integration/account
pages provide composition; they do not yet provide Access administration.

Deliver Settings → Access for Users, Service accounts, Roles and optional
Groups, account creation/invitation and existing credential lifecycle controls,
subject/role/explicit-scope forms and direct/group/provider source summaries.
Runtime-family selection is separate from operation grants and has no release
version selector. Built-ins are read-only; custom roles select the closed
operation list. Show applicable operations and future-version family coverage.
Edit inherited authority at its source; removing a direct binding must not
claim that broader or inherited access disappeared.

Render local device approval through the existing human login flow. Add Copy
access request guidance using only the authenticated identity, requested action
and supplied link/identifier. Nothing is sent or persisted. Respect server
denials at the GitHub configuration and Access pages/actions; Repository reads
remain usable without configuration rights. Update affected presentation tests
and user-facing documentation. No new backend authority, persistence purpose,
request queue, notification system or UI-only workflow is authorized here.

## Acceptance and implementation evidence

Execute these three cycles in order. Begin with the smallest UI action and
extend its connected journey. Add assertion locations, actual focused commands,
observed RED/GREEN, production captures and material limitations in each entry.
Record exact new test selectors after adding them and before RED.

### 1. Admin creates Alice's access; Alice uses it in the browser and CLI

Maps AC-001 to REQ-002, REQ-006, REQ-007 and REQ-008.
Start a production-built BFF and real Wyrd server with repository PostgreSQL,
an administrator and payments/another Repository with retained reviews. Configure
no IdP. Admin uses Settings → Access to create/invite local Alice and assign
viewer at payments family. First assert the saved server binding and the UI's
matching account/role/scope/source summary. Expected RED: the Access page/form
does not yet let an admin submit this scoped assignment.

Alice completes local password setup/sign-in, then approves a real CLI user code
in that UI. Use the actual redeemed CLI login and SDK clients, and Alice's
browser session, to observe the same payments review sets/member executions and
evidence. Another Repository contributes no items/pages/counts. Opening an
ungranted referenced Card is refused; GitHub setup/inventory and Access direct
URLs/actions remain denied. An unavailable server is distinguishable from
ordinary denial. Do not assert only a toast or inspect isolated form fixtures.

Exercise direct and optional-group assignments, custom roles, family versus
exact-version selection and current source summaries through the same forms.
Verify invalid/inapplicable/foreign scopes cause no binding, missing invitation
scope causes no enrollment, built-ins cannot be edited and last-admin removal
is refused. Inherited access remains accurately shown after direct-binding
removal. Existing provider-login presentation remains correct using the backend
provider regression path; local approval never requires a synthetic connection.

Proof: production browser → BFF → Rust → PostgreSQL, followed by real CLI and
public Rust/Python/TypeScript/MCP consumers of those created grants. Extend the
existing server integration UI host and production UI test owners. Component
tests cover form validation/presentation, not authorization substitutes.

### 2. Create a runtime Service in the UI and reuse it for two releases

Maps AC-002 to REQ-003 and REQ-008.
Start with checkout Service release fixtures and dependencies. Admin creates
checkout-runtime through Access, selecting Service family checkout and separate
operation bindings. Inspect that exact account through HTTP/CLI, then use its
issued credential with the public SDK workload-root path for each release.
Assert the same principal/key works for both and actual retained data names
the correct workload_uid, owning Card, subject, Verifier and actor chain.
Expected RED: UI provisioning cannot yet submit/display the separate runtime
family and scoped operation bindings.

Inspect a programmatically created Service in the UI as the reverse variant.
The form must not require a release UID or suggest account/key recreation for
a new release. Preserve one-time secret handling and existing credential
rotation/revocation controls. Try an ungranted operation and a different runtime
family using that credential; neither produces unauthorized work. Displayed
family restriction must not imply access to every dependency.

Proof: production browser form → TASK-001 provisioning contract → actual CLI/SDK
credential consumer → retained attribution query. Reuse the backend Service
journey fixtures and production UI host; disconnected UI mocks cannot prove
that the saved credential and grants are usable.

### 3. Request and change access without replacing the existing session

Maps AC-003 to REQ-008 and REQ-009; observes TASK-001's REQ-004/REQ-005 behavior.
Start with signed-in Alice denied access to a supplied target and an admin in
a separate session. Alice copies the request and hands the text through the
test's existing communication step; it contains her identity, requested action
and supplied identifier only. Admin grants through HTTP/CLI. Alice's same browser
session and ordinary CLI/SDK token gain access after normal propagation.
Expected RED: the copy guidance/current access presentation is missing or still
requires re-login to reflect the saved binding.

Admin removes/changes the assignment through Access. Observe actual subsequent
browser and programmatic reads lose the affected access within the healthy
bound. A remaining inherited/broader binding stays visible and effective;
restricted/delegated tokens never exceed their backend frozen limits. Confirm
the UI mutation reaches the same server revision and filtered-read path already
proved by TASK-001. Do not reproduce backend cache fault injection in Node.

Compare copy guidance for unknown and inaccessible identifiers: no hidden name,
owner, administrator roster or installation inventory is disclosed. Copying
causes no request record, notification or message. Existing sessions remain
Rust-owned; the browser receives no API key/token beyond the existing intended
one-time credential display. Permission/unavailable responses remain faithful
to server problems rather than promising an access change succeeded.

Proof: real browser interaction and clipboard observation, server-backed admin
mutation, then browser/CLI/SDK reads using existing sessions/credentials. Assert
returned data and refused operations, not only changed labels. Component tests
pin safe copy contents and form states; backend outage/replica and recovery
proof remains in TASK-001's third entry.

## Verification and handoff

Reuse `wyrd-server/tests/integration/identity_ui_e2e.rs` production BFF host,
`wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts`, and
`wyrd-ui/src/lib/ui-pages.integration.test.ts` for real Chromium page actions.
The latter's existing runner is `mise exec -- scripts/postgres/with-test-postgres.sh
-- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test`,
after `ts:build`, `ts:build:testing` and `build:server`; use its actual focused
file/selector when recording evidence. Component tests remain in the existing
UI page/library owners. The production identity host runs via server's
`--test integration`; package tests use Vitest, browser interactions use its
installed Playwright. The existing `mise run test:identity:ui` builds/runs the
provider variant. Extend the existing host/runner with a local-only path as
needed so the local journey proves no IdP dependency; reuse repository Postgres
and production UI build setup instead of a second application harness. Record
its actual focused command before RED, without inventing a selector in this plan.

Run `mise run ui:check` and the focused production journeys; use backend/CLI/SDK
runner setup identified in TASK-001 for downstream assertions. Check keyboard
operation, visible focus, responsive layout and both light/dark modes on the
affected production pages under `$wyrd-ui`. Run `codegen:check` for any touched
generated-contract source, required format/lints for touched languages and
`git diff --check`. Changes to reusable component contracts must regenerate
their existing catalog rather than hand-edit generated files.

No backend hot-path change is expected in this task. If one is necessary within
approved behavior, apply the spec's wait inventory and full clean VALID
benchmark evidence requirement to that change; presentation proof cannot replace
it. Integrated change review owns the full combined surface journeys. Neither
task implementation nor this plan initiates review or claims approval.

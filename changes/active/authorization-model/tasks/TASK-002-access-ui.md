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

**Evidence.** Behavioral verification: PASS for the cases listed; limitations
below. Commands, recorded once and referred to by the other entries:

- C1 (pages): `mise exec -- scripts/postgres/with-test-postgres.sh -- env
  WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest
  run src/lib/ui-pages.integration.test.ts`, after `mise run ts:build`,
  `mise run ts:build:testing` and `pnpm --filter wyrd-ai-ui build`. Final run
  at `6a11468a8`: 19 passed of 19, default stack and runtime settings.
- C2 (library): the same with `src/lib/ui-library.integration.test.ts`:
  17 passed of 17.
- C3: `mise run ui:check`, `mise run codegen:check`, `mise run docs:check`,
  `mise run lints`, `cargo fmt --all --check`, `git diff --check`: all exit 0.

Assertions, all in `crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-pages.integration.test.ts`:

- `Alice is given payments in Settings and sees only payments in her browser,
  CLI and SDK, then gains and loses ledger without signing in again` (line
  2650): invitation with viewer at the payments family and no IdP; the saved
  binding and the page's account, role, scope and source; removal of the
  tenant administrator's own admin binding refused; Alice's index holds
  payments only; ledger reads as no such Card; GitHub setup and Access
  administration refused; CLI code approved in her browser; CLI login and SDK
  client read payments only.
- `an administrator manages direct, group and custom-role grants in Access,
  and a refused grant saves nothing` (line 2824): invitation without a scope
  enrolls nobody; family versus exact version told apart; inapplicable role,
  unknown Card and malformed name save nothing; built-in roles not editable;
  custom role from operations; group as source; the group's grant stays
  shown and effective after the direct binding is removed.
- `Access tells an unavailable server apart from a refusal` (line 3068).
- `Access works from the keyboard and fits a phone and a desktop in light and
  dark` (line 3117): 1280 and 390 px, both modes.

Limitations. The RED observations of the original cycles were not written
here when they were made and are not reconstructed: TDD compliance for this
entry is unverified. Python and MCP consumers of grants created in the UI
are not driven by this journey; TASK-001's SDK, CLI and MCP journeys prove
those consumers against the same server contract. Provider login:
`mise run test:identity:ui` against real Keycloak and Dex passes at
`29a9d276f` (`identity_ui_e2e::production_ui_bff_journey` and its 5 UI
tests).

Remediation diagnoses (review findings; each fix site is the shared owner):

- Symptom: C1's worker died with SIGSEGV. Evidence: native stack through
  `OraclePlanner::validate_statement` into `sqlparser::Parser::parse_joins`.
  Cause: `parse_joins` recurses without the crate's stack guard, so a nested
  derived-table query exhausts the stack before the recursion limit refuses
  it. Fix site: the parser, pinned through `[patch.crates-io]` (`d2f507ee9`);
  the UI query no longer nests (`2a7d70c72`). See the shared prerequisite
  below.
- Symptom: three Access journeys refused connection. Evidence: the shared
  host exited on `ENOENT` for an asset of the first build after a later
  journey rebuilt `build/`. Cause: each journey rebuilt the UI under a
  file-scoped running host. Fix site: `buildUi` builds once per file
  (`6a11468a8`). Ceiling: another process rebuilding `build/` mid-run can
  still break a host.
- Symptom: `development environment lifecycle` never reported its batch.
  Cause: `scripts/postgres/dev-server.sh` passed the removed `--role admin`.
  Fix: `--grant admin:all`; `dev:backend`, `dev:both` and `dev:full` share it.
- Symptom: stale expectations after scoped authorization. Cause and fix: three
  ordinary-table query sites and the trace refusal answer
  `WYRD_VALA_403_QUERY_FORBIDDEN`; judged, traced and drift fixtures hold
  `executions:read`, see judgments, inputs and drift windows, and are refused
  the separate Verifier definition; the GitHub settings refusal offers only
  `Copy access request`.

Shared backend prerequisite (not UI evidence), required for C1 and C2 to run:

- Parser: sqlparser 0.63.0 plus one guard on `parse_joins`, pinned by rev.
  Tests `oracle::tests::nested_verify_discovery_query_validates_on_constrained_stacks`,
  `nested_derived_tables_and_joins_validate_on_constrained_stacks` and
  `over_nested_and_malformed_sql_are_refused_on_constrained_stacks` in
  `vala-bifrost-redux` pass (`cargo nextest run --locked -p vala-bifrost-redux
  --lib -E 'test(/constrained_stacks/)'`); they failed by stack overflow
  before the pin. Byte ceiling and recursion limit unchanged.
- Oracle parse boundary (`075be2429`): the deadline is taken before parsing
  and CPU admission; parsing runs through `CpuLanes::dispatch_blocking` under
  `Workload::Bifrost` with tenant attribution; the same-process path reuses
  the parsed statement; a remote Oracle validates independently and the wire
  request is unchanged; the lane is released before IO. Tests
  `parse_takes_one_lane_inside_the_deadline_and_returns_it` and the lanes
  dispatch test pass. TDD: no RED was observed for the dispatch helper.
  Wait inventory: no PostgreSQL, Scribe, object-storage or Vault wait is
  added or moved; the one new wait is CPU-lane admission, inside the existing
  CPU-resource boundary. No benchmark was run, by maintainer decision.
- Discovery query (`2a7d70c72`): result equivalence is asserted by
  `consolidated Observe bundles return the original results; EXPLAIN ANALYZE
  compares their work` in C2. Executed work for Verify with
  `WYRD_QUERY_DIAGNOSTICS`: 1 statement and 5 scans over 40 rows, against 9
  statements and 37 scans over 288 rows for the original reads. The
  population names five table scans, so it is read once.
- Card not-found concealment (`93b0f4470`) is TASK-001 remediation: exact
  reference, UID, absent name, kind mismatch and another tenant all answer
  `WYRD_REGISTRY_404_CARD_NOT_FOUND` with one message
  (`principal_roles::group_custom_and_exact_version_bindings_scope_reads`,
  RED observed with seven distinct messages); a readable reference with a
  hidden UID over MCP
  (`a_family_bound_agent_reads_only_that_family`, written after the fix, no
  RED); operational errors keep their code (unit test in `cards/service.rs`).
- Bifrost server journeys (`29a9d276f`), TASK-001 fixtures: `mise run
  test:bifrost:journey:server:inner` under `with-test-postgres.sh`: 63 of 66,
  then `visible_trace_spans_preserve_binding_ownership` fixed and passing
  alone. Causes: inspection contexts lacked `executions:read`; the
  result-layout fixture published without a registry, leaving subject kind
  and name unstamped; status was read through a revoked submitter; a
  cancelled status row now names its subject; tokens carry no `permissions`
  claim; a trace read without a table grant answers `query forbidden`.
  Open: `isolated_capacity::connected_work_waits_out_pressure_with_its_own_attribution`
  and `isolated_capacity::standalone_call_bodies_count_as_gateway_work_while_the_call_runs`
  fail asserting zero Bifrost workload bytes. Independent of this change:
  since `6e7db5c25` the Scribe memtable is charged to Bifrost by design.
  Commit `5290a7c14` on `task/github-verifier-pipelines/TASK-012-integrate-code-mode`
  already corrects both tests to assert only the request workloads; it is
  not in this branch, and the two tests pass once it is merged.

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

**Evidence.** Behavioral verification: PASS (C1). `an administrator creates a
runtime Service account in Access, and its one key runs two releases of the
family` (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-pages.integration.test.ts`, line 2943) asserts the Service family and separate operation
bindings, the one-time key, and that key running both releases with the
retained workload attribution; an ungranted operation and a Card outside the
family are refused; a second key is shown once and the first revoked by its
prefix; an account created through the SDK reads the same way in Access. The
form asks for no release version or UID. TDD: RED not recorded when made;
unverified.

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

**Evidence.** Behavioral verification: PASS (C1). In the Alice journey
(`crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-pages.integration.test.ts`, lines 2771 to 2823): the copied request read back from the clipboard
names her account, the tenant, the action and the link, and sends nothing; a
Repository that does not exist gives the same problem code and request; a
grant made over HTTP reaches her same browser session, CLI login and SDK
client without signing in again; removal in Access takes ledger away and
keeps payments; her browser holds no script-readable credential. Copy
guidance is present for a reader and absent for an administrator at lines
3094 and 3103; its text is pinned in `src/lib/access.test.ts`. TDD: RED not
recorded when made; unverified.

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
approved behavior, apply the spec's wait inventory. Integrated change review
owns the full combined surface journeys. Neither task implementation nor this
plan initiates review or claims approval.

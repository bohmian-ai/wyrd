---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 3
requirements: [REQ-010, REQ-011, REQ-013, REQ-014, AC-006, AC-008, AC-009]
depends_on: [TASK-001, TASK-003]
---

# Repository Cards and GitHub connection settings

## Outcome and required backend outputs

The separate Repositories workspace exposes normal versioned Repository policy
and verification activity; Settings manages provider access. An accessible
provider repository can have no Card, while a registered Card can lack access.
Neither viewing a version nor connecting the App declares or selects policy.

Requires delivered connection/read/disconnect and redacted access operations,
plus native Repository declaration/selected policy/exact binding operations and
their owning authority amendments. Record the actual integrated capability and
contract revision, not another packet's task status.

TASK-001A's integrated baseline (`cd204b404`) contains these server owners;
record their real journey proof at activation:

| Need | Existing owner |
|---|---|
| Connect, read, disconnect, setup and callback | `/v1/integrations/github/connect`, `/v1/integrations/github` (GET, DELETE), `/setup`, `/callback` in `wyrd-server/src/components/github/routes.rs` |
| Redacted connection and accessible repositories | `GithubConnectionView` (`wyrd-spec/src/github_connection.rs:99`): host, App, state, installation and `repositories` |
| Selected policy and access on a Card | `Status.repository` / `RepositoryStatus` (`wyrd-spec/src/envelope.rs:497`): `selected`, `selected_version`, `full_name` |
| `Repository` Card kind | `AGENTS.md` (13 kinds) and the registry's normal Card reads |

Accessible-but-undeclared rows come from `GithubConnectionView.repositories`
joined to Repository Cards by stable provider identity; selection comes from the
server-derived Card status, never browser inference. TASK-001 supplies definition
rendering and TASK-003 supplies filtered Verify. Missing backend capabilities
remain external dependencies; this task adds only UI presentation/adapters.

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
Activity reuses TASK-003's existing-read queries and opens TASK-002's inspector;
do not hydrate detailed outcomes in the Repository loader or add a read service.
A missing execution detail must not disable readable policy/access/activity.

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

This task is against approved spec revision 3, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. This packet consumes existing read/action
capabilities and adds no underlying server changes; a missing required capability
remains an explicit external dependency. Record the integrated source revisions
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

Regenerate changed UI catalog/contracts through their existing generator and run
`codegen:check` where applicable. Existing backend/SDK operations are dependency
inputs, not a Rust/SDK implementation assignment here. Do not add a runner or
substitute fixtures for unavailable inputs. Integrated change review joins the
delivered flows and runs the required shipped-surface journey lanes.

## Execution evidence

Integrated source revision: the branch contains `cd204b404`; the consumed server
operations are `/v1/integrations/github/connect`, `/setup`, `/callback`,
`/v1/integrations/github` (GET, DELETE), normal Repository Card reads and
`wyrd declare`. No server, Rust or SDK file is changed.

Commands, run from the repository root:

```bash
mise run ts:build && mise run ts:build:testing && mise run build:server
mise run ui:check
mise run codegen:check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 WYRD_LOG=warn WYRD_UI_CAPTURE_DIR=<dir> pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
git diff --check
```

`ui:check` passes (0 type errors, 81 tests, including the task's
`ui-library.test.ts` and `ui-pages.test.ts` command and `check`);
`codegen:check` and the build pass; both integration files pass (28 tests).

| Requirement | Proof |
|---|---|
| REQ-014: the connection read through the shared contract and both executors | `ui-library.test.ts`: "the GitHub connection read runs as the existing SDK and MCP operations" |
| REQ-010: index joins Cards and access by repository id; registration, selection, access and readiness words; undeclared rows; connection denied keeps Cards | `ui-library.test.ts`: "the Repositories index joins Repository Cards and GitHub access by repository id" |
| REQ-011: viewed version apart from selected policy, exact bindings and activation, read-only YAML, TOML labelled a projection, removed access and empty policy | `ui-library.test.ts`: "a Repository Card shows the viewed version apart from the selected policy, its exact bindings and a read-only declaration" |
| REQ-010: no-Card instructions; unknown, foreign or already-registered ids name nothing | `ui-library.test.ts`: "an accessible repository without a Card shows how to declare one, and an unknown id shows nothing" |
| REQ-013: host, App, installation and state; suspended or removed access; manage-access link; confirmed disconnect; denied and unconfigured App issue no Card read | `ui-library.test.ts`: "the GitHub integration shows safe connection state and asks before disconnecting" |
| REQ-010, REQ-011: nav entry, index URL state kept on return, exact version links, activity, Manage connection | `ui-pages.test.ts`: "the Repositories workspace keeps its index state, opens exact versions and leads to the connection" |
| REQ-013: one connect and one disconnect through the host, pending guard, denied and error state, read-back | `ui-pages.test.ts`: "Settings connects and disconnects GitHub once through the host and reads state back" |
| REQ-014, AC-006, AC-008, real server: Node and MCP agree; minimal, changed, empty and undeclared repositories; an older version is not selected; ids 1 and 999 not found; no secrets; a viewer is denied the connection, keeps Cards and issues no Card read after denial; another tenant sees nothing; disconnect leaves Cards with access `none` and readiness `inactive` | `ui-library.integration.test.ts`: "Repository Cards join GitHub access by repository id through the Node and MCP hosts" |
| AC-006, AC-008, AC-009, REQ-010, REQ-011, REQ-013, Chromium against the production BFF: Verify → Repositories → Settings; Wyrd-first connect through the server's setup and callback; every index case; URL state into detail and back; an old version viewed without changing selection; TOML projection; activity → execution → filtered Verify → Verifier definition; no-Card page; foreign and unknown ids; viewer denied on the page and on direct POST and DELETE; modal confirmation (focus, Escape, Cancel sends nothing); one DELETE while pending; Cards and executions kept, executions cancelled; GitHub-first reconnect; no sideways page scroll at 1440 and 390 | `ui-pages.integration.test.ts`: "a tenant connects GitHub from Settings, reads its Repository Cards apart from GitHub access, and disconnects without losing them" |

Not proven:

- AC-006 "access-lost" on a real server is proven only by disconnect. The
  TypeScript test owner exposes no suspend, removal or installation event, so
  suspended and removed access are proven with fixtures at unit tier only.
- AC-009 development journey: `mise run dev:full` is not extended. It starts the
  server with no `[github]` configuration, so Settings reports the App as not
  configured and Repositories has no rows there. A developer's own
  `WYRD_GITHUB_*` variables already reach the dev server.
- AC-009 individual human acceptance of the mock comparison is not recorded.

TDD: VIOLATED. The connection-read test was written first but its failing run
was not captured. The four component tests first ran after their components
existed. The two page tests were written in the same step as the pages. Both
real-server tests were written after the behaviour.

Corrected mechanics: captures are taken at 1440 and 390 wide (the mock's own
widths); the integration command adds `WYRD_LOG=warn` and `WYRD_UI_CAPTURE_DIR`;
the disconnect and connect relay lives at
`settings/integrations/github/connection` because a `+server.ts` beside a page
answers a non-HTML GET with 405.

Comparison with the approved mock, against its source (`Repositories.svelte`,
`RepoDetail.svelte`, `Undeclared.svelte`, `Settings.svelte`, `app.css`) and
captures of index, detail, no-Card, Settings, the disconnect dialog and the
disconnected index in light and dark at 1440 and 390. Remaining differences,
each awaiting a ruling:

1. Index has no "Latest execution" and "Status · verdict" columns.
2. Activity is the shared RecordTable (Time, Verifier, Card version, Status,
   Verdict; its Wrap lines, Columns, CSV, Open in Query and paging controls),
   not the mock's Execution, Pull request, Head, Verifier, Policy table with a
   "Latest per PR" toggle. Sort, page and query there answer "Open Verify to
   sort, filter or page these executions." Pull request columns belong to
   TASK-005.
3. Activity names the Verifier without `@version` (version on hover).
4. Card YAML is the whole returned envelope (uid, spec hash, relationships,
   status), not the mock's short authored YAML.
5. Detail header has no description line.
6. Settings shows no Account fact: the connection read returns no account name.
7. The disconnect dialog gives no count of unfinished executions.
8. Search applies on Enter.
9. In-page navigation is drawn as link-styled buttons that emit intents.
10. Extra access words "No access", "Not selected" and "Unavailable"; a
    `repository_id` tag where GitHub gives no full name; a Host fact while not
    connected.
11. The sidebar shows the host's search box and tenant switcher, which the mock
    omits.
12. The mock's phone capture exists in dark only; light at 390 was compared
    with the desktop light capture.

External dependencies:

- The server callback answers JSON and nothing returns the browser to Settings.
  The flow completes in the browser only where the UI and `/v1` share an
  origin; Settings shows the authoritative state on its next load.
- Reading the connection needs `identity_connections:write`, so a viewer sees
  the connection as denied while Cards stay readable.
- The connection read carries no account name and no manage URL; the manage
  link is `https://{host}/apps/{slug}/installations/new`.

User decisions: none yet.

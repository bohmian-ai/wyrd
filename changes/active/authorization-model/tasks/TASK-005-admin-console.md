---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 12
requirements: [REQ-021]
acceptance: [AC-016]
depends_on: [TASK-004]  # TASK-004's code, not its review or paused live smoke
---

# Make workspace and platform administration discoverable

After this task, an administrator opens Settings, sees every administration
section, and can open Invite people without hunting through introductory text.
People, sign-in, stored secrets and the platform console show current server
state before controls. Every User can open Your account from their email in the
sidebar name block; provider sign-ins store and refresh the person's display
name, and labels use display name, then username, then email. Social Users no
longer see a missing username or a principal ID as their identity.

## Contract, inputs and existing owners

Use `changes/active/authorization-model/spec.md`, approved revision 12 on
2026-11-24 as amended that day: REQ-021 (including its read-projections
table), AC-016, Persistence rows "User display name (revision 12)" and "Role
description (revision 12)" and Scope and delivery's "Revision 12: admin console". The finalized
`design/admin-console.md` sections "Proposed implementation", "Wire and storage
additions", "Page pattern", "Decisions and alternatives" and "Standards
conflicts" carry the chosen approach. The maintainer clarification of 2026-11-24 withdraws the earlier
stored-display-name and migration exclusions and approves the provider marks.
Paths under `design/` here are relative to `changes/active/authorization-model/`.

Read `design/admin-mock/src/`, particularly `Login.svelte`, `Mark.svelte`,
`Shell.svelte`, `Admin.svelte`,
`People.svelte`, `Services.svelte`, `Roles.svelte`, `SignIn.svelte`,
`Secrets.svelte`, `Integrations.svelte`, `Account.svelte`, `Platform.svelte`,
`app.css` and `data.js`. These fix look, labels, column order and presentation
states, not sample identities, hardcoded numbers or new server behavior. The
approved spec governs where illustrative mock behavior differs: Your account
stays at `settings/account`; the company proof window stays five minutes;
session/device totals, usage counts and age warnings are not invented from
sample data. Show server-owned values or omit unsupported metrics. Record
resulting visual/copy differences for maintainer ruling, without silently
expanding the server contract to match a sample.

TASK-004 supplies company-login-policy reads and mutations, the lock-out count,
fresh company proof and revision-conflict handling, and connected production
browser/BFF owners. Inspect its actual outputs and evidence limitations; this
dependency is on TASK-004's code being present, not on its review passing or
its paused live smoke; it is not a claim that all of its verification has passed. Reuse earlier
account, invitation, Role, credential and platform administration. TASK-004's
paused live Google/GitHub smoke remains there, including steps 3–5; it is not
part of this task's delivery or evidence.

Existing owners and the missing behavior:

- `crates/wyrd-spec/src/auth/tenant_principals.rs`: `CurrentUserProfile` lacks
  email/display name and says "never an email"; `PrincipalSummary` already has
  email/name, status and restriction, but lacks display name, authentication
  and scoped Role assignments;
  `PrincipalPage` has only principals and continuation, not status counts;
  `PrincipalQuery` has no `role` filter.
- `crates/wyrd-spec/src/auth/access.rs`: `RoleView` lacks `description` and
  `member_count`; `PutRoleRequest` lacks an optional `description`.
  `components/principals/access.rs` (`list_access_roles`, the
  `/access/roles/{role}` PUT) and `wyrd-sql/src/queries/auth/roles.rs`
  (`list_roles`) own them. `member_count` counts distinct Users and service
  accounts holding the Role at any scope, directly or through a Group, each
  once, in the same list query; the built-in descriptions are server-set
  using the mock's `data.js` wording and are read-only.
  Extend these contracts over stored data, including the new provider-sourced
  display name. Authentication uses the same `AccountAuthentication` shape as `/me`; do not infer a provider from an
  email address or add identity/linking semantics for presentation.
- `crates/wyrd/wyrd-server/src/components/principals/accounts.rs`:
  `current_user` and `current_user_profile` own HTTP/MCP self-profile reads.
  `components/principals/routes.rs` owns `list_principals` and `summary`.
  Extend their existing tenant-scoped SQL read owners for these projections,
  preserving authorization, keyset pagination and Role scope/source meaning.
  The current directory query is
  `wyrd-sql/src/queries/auth/principal_directory.rs`; self-profile currently
  reads `local_username` from `queries/auth/users.rs`.
  Counts describe the requested filter set, not the rows on one page. No new
  inventory endpoint or alternate authorization service.
- `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql` and the existing
  `queries/auth/users.rs` owner supply nullable `wyrd.auth_users.display_name`,
  and `queries/auth/roles.rs` nullable `wyrd.auth_roles.description`.
  REQ-021 and the two revision-12 Persistence rows explicitly
  authorize adding those columns in place to this unshipped migration and the
  resulting local database reset; no new migration, table or index is needed.
  Extend the existing successful social/company sign-in workflows to store
  Google ID-token `name`, GitHub profile `name` (else `login` when blank), and
  company OIDC `name` on every successful sign-in. An omitted provider value
  leaves the stored name unchanged; password Users have none. Keep forced RLS
  and the existing sign-in ownership; no user-editable field or linking flow.
- `crates/shared/wyrd-client/src/principals/handle.rs` and its re-exports own
  typed client transport. Carry fields through Rust SDK re-exports, Python
  `sdks/wyrd-sdk-python/src/principals.rs` and authored stubs/exports, and
  TypeScript `sdks/wyrd-sdk-ts/native/src/principals.rs` and generated/public
  types. `wyrd-server/src/mcp/principals.rs` projects the self-profile through
  `principals.me` and shared summary types where consumed; update shipped
  projections/catalog schemas, without inventing a new administrative tool.
- Under `crates/wyrd/wyrd-server/wyrd-ui`, revise the existing host:
  `src/routes/t/[tenantKey]/settings/{+page,access/+page,account/+page}.svelte`
  and their server loads/actions, tenant layout, and `src/routes/platform/ui/*`.
  Replace the single SSO landing page and combined Access presentation with
  the agreed section subroutes: `people`, `service-accounts`, `roles`,
  `sign-in`, `secrets`, `integrations`. Keep GitHub's existing connection
  workflow. Your account remains outside the admin section list and retains
  existing self-service behavior. No parallel old admin implementation.
- Existing `settings/access/+page.server.ts` already performs invitations,
  Role/binding and Service/key actions; `settings/+page.server.ts` performs
  company setup/policy actions; `settings/account/+page.server.ts` consumes
  `/me` and own credentials. Reuse these workflows through the existing
  `BrowserSession` boundary. Platform loads/actions use `platformSession` and
  `consoleChange`; preserve their separate cookie plane and fresh proof.
- Compose tenant Secrets in the host from `/v1/admin/gateway/provider-credentials*`,
  `/v1/operator-connections*`, `/v1/identity/oidc/connections*` and
  `/v1/admin/trusted-issuers*`. Their server owners retain replacement,
  encryption renewal and revoke/deactivate semantics. Offer each action only
  where that owner supports it; never expose stored secret values.
- `packages/ui/src/lib/components/` already provides built `StatStrip`,
  `RecordTable`, `StatusPill`, `EmptyState`, `Drawer`, `Panel`, `ActionButton`;
  their contracts are in `brand/components.json`. Reuse them with the approved
  composition, applying `wyrd-ui`, `brand/DESIGN.md` and existing theme/mode
  ownership. The approved fixed Google/GitHub SVG marks are the narrow
  exception: add one sentence to `brand/DESIGN.md` permitting provider marks,
  Google's official colors and GitHub `currentColor`, as settled in the
  design's "Standards conflicts". No new component system or dependencies.

Excluded: editable display names, rotation-age threshold, per-method usage
counts, Groups UI beyond today's, new administration capabilities, migration
changes beyond the one expressly authorized auth-column edit, and live
Google/GitHub smoke. The Rust server still owns all workflow, counts,
thresholds, sorting, pagination and aggregation. Follow the relevant browser,
platform, access-administration and stored-secret sections of
`architecture/wyrd-security-posture.md`; no credential-bearing browser storage
or weaker proof/permission channel is introduced.

## Ordered acceptance and implementation evidence

All entries exercise REQ-021 and AC-016; entries 5–8 also preserve the existing
access, company-SSO and platform safeguards referenced there. Order is delivery
order, not RED/GREEN ordering for presentation refactoring. Each entry is its
own evidence record: append actual assertion locations, focused commands,
results and limitations in place. No entry requires test-first ordering or a
recorded failing test. Storage/refresh and read additions still require
contract tests and connected journeys; record their passing results. Existing correct behavior needs regression proof, not an
invented failing test.

### 1. Reconcile the self-profile authority before implementation

First step; maintainer approval of wording required. Propose the change to the
`GET /v1/principals/me` paragraph in `architecture/wyrd-design.md`, replacing
its first sentence with the wording REQ-021 records: "`GET /v1/principals/me`
returns the calling User's safe profile: principal, tenant, username, display
name, the User's own email, authentication method and effective permissions.
It never returns another principal's email." Obtain explicit maintainer
approval of that edit and reconcile the paragraph before implementing any part of
this task. Preserve existing secret exclusions and self-only authorization.
Then align `CurrentUserProfile` rustdoc with the approved paragraph. Record the
approved wording/approval and changed paragraph location here. This is a
document prerequisite, not a runtime test or authorization to revise other
architecture boundaries. This planning task does not itself edit architecture.

### 2. Successful provider sign-ins store and refresh the display name

Start with the repository-managed Postgres database created from the revised
unshipped auth migration, real server sign-in workflows, controlled Google and
GitHub provider responses, and the existing company OIDC fixture. Sign in with
Google ID-token `name`, GitHub profile `name`, GitHub blank `name` plus `login`,
and company OIDC `name`. Each successful sign-in stores the supplied name; a
later successful sign-in with a changed provider name refreshes it. A later
response omitting the name preserves the stored value. Password Users retain
no display name, and no account control accepts edits to the provider name.

Read back through `/me` and the authorized principal directory after each
sign-in, using the typed clients exercised in entry 3; reload the production
name block, Your account and People to prove those consumers use persisted
values. Assert display-name → username → email fallback and email on the
second line. With distinct names in another tenant, neither self-profile,
directory nor rendered pages disclose those names. Reads do not update names,
grant access or change authentication methods.

Tier 2: extend the owning SQL Postgres tests to prove nullable storage, refresh,
omitted-value preservation and RLS under the revised migration. Tier 1: extend
the existing identity journeys and production UI fixture through successful
social/company sign-in → storage → typed profile/directory → production UI.
Controlled provider responses exercise production parsing and callbacks; these
are not TASK-004's paused live-provider smoke. Record actual assertion
locations, focused commands and passing results. Fresh fixture
creation and the authorized local reset must apply the edited migration without
an added migration, table or index.

### 3. Real clients receive the safe read projections

Start with real repository Postgres and `WyrdTestServer`: password, Google,
GitHub and company Users, active/suspended Users, Service accounts, scoped Role
assignments and another tenant. Authenticated callers read their own `/me`;
admins list principals with supported filters and bounded pages. Self email
and display name match the stored caller, including social Users without
usernames; list items include display name and preserve authentication and
actual Role assignments with scope; status
counts reflect the filter set across pages, including zero matches. The `role`
filter returns principals holding that Role at any scope, directly or through a
Group. Role list items carry `description` and `member_count` matching the
same membership rule; a custom Role's description is set and cleared through
the Role PUT, and a built-in's description cannot be changed. Another
tenant contributes neither rows, Role assignments, members nor counts. Service accounts
do not acquire a human authentication method. Existing unauthenticated,
non-User self-read and non-admin directory refusals still return their catalog
codes and disclose no protected data. Reads create no account, grant or secret.

Tier 2: extend server principal/read and served OpenAPI contract owners
`tests/integration/pg_principal_roles.rs` and `pg_openapi_contract.rs`, plus the
owning SQL/contract tests needed for filter/count semantics. Tier 1: real Rust,
Python and TypeScript clients read every addition through the
server and assert typed fields and tenant isolation; extend the existing
`principal_roles.rs`, `test_principal_roles.py` and `principal-roles.test.ts`
journey owners under their SDK integration trees. MCP journeys assert the
fields and schemas on the existing tools that project them, including
`principals.me`, through the actual MCP consumer. No raw HTTP substitute for
SDK journeys. Record actual new test names and exact focused commands once added.

### 4. Settings and Your account lead with a usable identity

From a real signed-in tenant page, open Settings and reach all six named
sections from the visible section list. Each section loads its own server
data and follows title/purpose/primary action, status, content. Opening a page
alone changes nothing. Password, Google and GitHub Users see their own stored
name and email in the sidebar and Your account, using display name, else
username, else email, with email as the second line; company Users follow the
same label order. An ordinary User reaches `settings/account` from the name block without admin privileges.
No placeholder, principal ID, fabricated name or editable display-name control
replaces that identity. Existing account/key controls remain usable under their existing restrictions.
Tier 1: production UI → BFF → real Rust profile and Postgres; prove the page
consumes the outputs of entries 2–3 rather than hardcoded mock data. Component
checks and light/dark captures pin shell/section hierarchy and account wording.

### 5. People, Service accounts and Roles use existing administration

With mixed Users, pending invitations, Services and scoped grants from the
server, an admin opens People, selects Invite people, enters recipients plus
Role/scope, then sends: opening the drawer and submitting are the two actions
from People. From Settings, People and Invite people are two navigation clicks.
Reload shows real pending invitations with delivery and expiry; resend/revoke
use their existing owner, and one-time links appear only in the issuance result.
The people table and person drawer show server display name (else username,
else email), email, authentication, roles/scopes, status and supported key metadata; filters/status strip consume
server results, never a count of the currently loaded rows. Role removal keeps
other sources of access; suspension and key actions retain existing proof,
last-admin and credential restrictions, rather than the mock's illustrative
unrestricted "Issue key" behavior.

Open Service accounts to inspect restriction, keys and status and create an
account through its existing endpoint. Open Roles to read each Role's server
description and `member_count`, expand actual operations, and create/edit a
custom Role, including its optional description, through the existing owner;
built-ins remain read-only. Selecting a member count opens People filtered by
that Role through the `role` filter. Do not calculate counts in the browser or
by downloading all pages, and do not substitute binding-row counts for members. Non-admin reads/writes and cross-tenant
targets refuse without changing accounts, grants, invitations or credentials.
Tier 1: extend the production UI fixture for these page/form → server → reload
journeys, preserving existing SDK account/Role regressions. Add focused component
checks only where needed to prove table, drawer and state presentation.

### 6. Sign-in presents the existing company-SSO lifecycle and policy

From no company connection, the admin sees actual available methods and enters
the four-step setup: register callback, enter client details, test sign-in,
activate. Read-only machine values support Copy. Activation still requires the
existing tested revision and recovery checks. With an Active connection, the
Require company SSO switch opens impact confirmation using the Rust
login-policy lock-out count. Cancel changes nothing; explicit confirmation uses
the observed policy/connection revisions and fresh company-admin proof.
Stale proof directs the admin to sign in again and explicitly confirm; stale
revisions reload/refuse without mutation or automatic replay. Required state
and deactivate controls reflect the actual server policy. No API-key-only
fallback claim, invented session total or ten-minute proof window is shown.
Tier 1: extend the production UI company-policy journey over the existing real
identity-provider fixture and two replicas; retain TASK-004's server regression
proof. The UI refactor must not weaken non-admin, stale-proof or conflict cases.

### 7. Tenant Secrets and Integrations call their existing owners

With tenant-owned provider credentials, Operator connections, company SSO and
trusted-issuer secrets, an admin opens Secrets and sees one inventory composed
from those owners' redacted metadata: state and available rotation metadata,
with no secret plaintext or UI-derived age warning. Add provider key and
Replace submit write-only values to the right owner; Renew encryption uses
that owner's existing renewal action; Revoke/deactivate is offered only where
supported and confirms the owner's actual impact. Cancel writes nothing;
reload reflects accepted changes. Non-admin requests and another tenant's
identifiers refuse without changing secrets, even when submitted directly.
Failures remain safe problems, not false success. Integrations presents and
uses today's GitHub connection workflow without new capability.
Tier 1: production UI/BFF → actual owner endpoints → reload, including refusal
and cancellation; reuse owner integration tests for secret lifecycle and
redaction. Do not replace these connections with a static inventory fixture.

### 8. The platform console shares the presentation and keeps its authority

A real platform operator reaches Overview, Workspaces, Sign-in providers,
Mail, Admins and Stored secrets from the same shell pattern, and changes mode
from the name/menu control using the existing persisted mode. Maintenance
status/results, provider/mail/admin and workspace actions use existing platform
endpoints and fresh-proof rules. Stale proof refuses until explicit
reauthentication and resubmission; tenant sessions cannot authorize platform
operations, and platform cookies never authorize tenant actions. Navigating or
changing mode mutates no platform configuration. Mock totals are not production
data. Tier 1: extend the real platform production-page journeys and their
server fixtures; prove navigation plus existing mutations/refusals, not links
alone.

### 9. Provider marks identify sign-in choices and methods

Using `design/admin-mock/src/Login.svelte` and `Mark.svelte` as the
look-and-wording source, show the fixed official Google "G" and GitHub mark on
"Continue with Google/GitHub" buttons for root/workspace signup and tenant
sign-in, and "Join with Google/GitHub" on invitation acceptance. The same marks
identify Google/GitHub in People and Sign-in method displays. Google's "G"
retains its official colors; GitHub uses `currentColor` in light and dark.
Add the single approved provider-mark exception sentence to the production
`brand/DESIGN.md`; the rest of Evidence Thread styling remains authoritative.

Presentation proof: capture the mock and production sign-in, signup,
invitation, People and Sign-in method states in light and dark at the matching
widths used in entry 10. Verify official SVG artwork, visible labels and the
specified color behavior in both modes. Exercise the existing provider buttons
to confirm their sign-in/join destinations still use the existing workflows;
rendering a mark alone never starts sign-in or changes account state. Reuse the
production UI journey fixture for these interactions and matched screenshots
for appearance. Presentation work needs no manufactured RED/GREEN ordering.

### 10. Production presentation matches the agreed states

Compare mock and production at 1280 and 390 pixels in both light and dark,
including the mock's current states and reachable empty, denied, error and unavailable
states. Use the mock's `selfcheck.mjs --shots` and production captures at matching
widths. Check labels, columns, primary actions, settings rows, drawers, destructive
confirmations, copy fields, keyboard/focus behavior and readable status glyphs/
words. Navigation/actions remain reachable and the document does not scroll
sideways; dense tables may use labeled horizontal scroll regions. Reused
components retain Evidence Thread tokens, 1px rules and 2px radius. No preview
strip, sample identity or fake live state ships. Record every remaining mock
difference and obtain the maintainer's ruling, including approved-contract
corrections noted above. Visual comparison supplements the connected journeys;
it does not substitute for entries 2–9.

## Verification and handoff

Use existing runners; add assertion locations, selectors and results to the
entries above, without a second coverage ledger. For newly added tests, record
their real focused commands before the first run; do not invent selectors now.

- Production UI test owner:
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts`,
  launched by `crates/wyrd/wyrd-server/tests/integration/identity_ui_e2e.rs`'s
  `production_ui_bff_journey` and its `WYRD_UI_JOURNEY` fixture. Run
  `mise run test:identity:ui`; it provisions Postgres, Keycloak/Dex and the
  production build, then selects that test and the onboarding companion.
  Its exact existing selector is
  `mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'`,
  executed only inside the same lane's managed provider/Postgres lifecycle,
  after its `ts:build` and production UI build. Use `WYRD_UI_FILTER` with an
  actual Vitest title when focusing a new scenario; standalone Vitest without
  the Rust fixture does not prove production behavior.
- `mise run test:principals:integration` owns principal SQL, served OpenAPI,
  server Role and Rust-client journey coverage. Run focused added tests within
  that task's managed Postgres lifecycle. Python and TypeScript journeys use
  `mise run py:test:integration` and `mise run ts:test:integration`; use
  `mise run test:identity:journey` where saved human-login/provider fixtures are
  required. MCP assertions extend
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs` and the catalog checks
  in `discovery.rs`, using `mise run test:bifrost:journey:mcp`; record actual
  focused selectors with the added assertions.
- Required affected lanes: `mise run ui:check`, `mise run codegen:check`,
  `mise run test:wyrd`, and `mise run docs:check` where documentation is touched.
  Regenerate consumed types/catalog outputs from their sources; Python stubs
  remain authored. Run Rust `mise run fmt` and `mise run lints`, and the owning
  Python/TypeScript format, lint, typing and N-API drift checks for changed
  projections. Run Cargo-backed checks sequentially and finish with
  `mise exec -- git diff --check` and scope inspection.

Handoff records the architecture wording approval, connected assertions for
every read projection on shipped surfaces, production journey results, matched
captures (including provider marks in light/dark) and maintainer rulings on
remaining differences. Include storage/refresh and tenant-isolation evidence
for the provider-sourced name, Role descriptions and the authorized in-place
migration/local reset. No further migration, new administration workflow or TASK-004
live-provider smoke is required by this task.

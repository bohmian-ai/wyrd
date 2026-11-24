---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 11
requirements: [REQ-002, REQ-004, REQ-005, REQ-011, REQ-012, REQ-013, REQ-014, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020]
acceptance: [AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015]
depends_on: [TASK-003-carryover]
---

# Enforce company SSO and verify the connected launch journey

After this task, a workspace administrator can require company SSO through tenant
Authentication settings, and the real UI, CLI and clients enforce that choice.
The remaining launch flows have connected proof against real dependencies and
Google/GitHub compatibility evidence. This task owns former carryover cycles 19
and 20. Self-service linking, password addition and method removal (17–18) are
deferred; they are not dependencies or acceptance criteria here.

## Contract, inputs and existing owners

Use [approved spec revision 11](../spec.md), especially REQ-012/016 and AC-006,
and the [finalized remaining-flow design](../design/remaining-identity-flows.md).
The maintainer approved the narrowed scope and recommendations for 19–20 on
2026-11-23. Do not execute the deferred linking design or historical cycle text.

The dependency supplies completed carryover **cycles 1–16**: actual setup/signup,
invitations, login methods, contacts/reset, shared provider settings, platform
sessions and production pages. It does not assert they are all complete now.
TASK-001/002 supply scoped grants, Access administration and existing client
resource journeys. Before implementation, read their actual outputs and preserve
recorded limitations. This task closes missing retained-scope consumer/projection
proof; it does not rebuild their provisioning, operator UI or authentication.

Company adoption needs no account linking. Configure/test/activate the existing
company connection using its current management/recovery requirements; sign in
through its existing company first-login provisioning. Obtain tenant-admin access
through verified group mapping or existing explicit grant administration. Then
that **company User** freshly signs in and enables enforcement. Password/social
Users stay separate. State this limitation in the warning and user/operator docs;
matching email never joins Users or transfers grants, keys or attribution.

Reuse these owners and consumers:

- `HumanConnections`, `AuthorizationCodeExchange`, `BrowserSessions` and
  `CliLogins`/refresh in `wyrd-auth`: company trust, provider proof, browser/device
  lifecycle and original credential anchors. Extend their complete workflows;
  no new policy service, session engine or generic reauthentication framework.
- `wyrd-sql` identity, login-policy, browser, device/refresh and authorization-
  revision queries: tenant-RLS reads, atomic policy mutation and durable
  invalidation. `login_policy.rs` currently supplies a read only. Company
  `ensure_user_identity` supplies first-login identity creation, while verified
  group-role mapping or existing grants supply admin authority.
- `wyrd-server/src/components/admin/identity.rs`, `auth/browser.rs`,
  `auth/social.rs` and callback/device handlers: typed HTTP operations and
  enforcement. `SocialSignIn`'s existing methods evaluator remains the one
  availability/policy calculation; do not reimplement it in Node or each handler.
- The existing `t/[tenantKey]/settings/+page.server.ts` and `+page.svelte`
  company-connection page: add the approved policy read, warning count and
  Enable/Disable confirmation using existing components and theme conventions.
  Apply `$wyrd-ui`; the maintainer's approval authorizes these policy controls,
  not another mock approval round or another operator console.
- Existing `wyrd-client`, SDK, CLI and MCP owners project the safe metadata and
  authorized management required by REQ-011/017 and the shipped-surface contract.
  Preserve their single Rust transport/auth owner and current write scopes.

## Connected contracts and boundaries

An admin-only policy read returns current required state/revision, current company
connection ID/revision and readiness, and the number of **active Users** without
a usable identity for that connection. Exclude disabled Users and Services.
Do not expose a new user directory, provider secrets or user identifiers in the
warning. The warning is informative; unlinked Users do not block enablement.

Enable submits the desired state and observed policy/connection revisions. The
Rust mutation rechecks tenant-admin authority and company proof through that
same current tested Active connection; changes since the read cause conflict
and reload, without changing policy or credentials. Ordinary Disable also requires
fresh company-admin proof. An old proof requires another existing company login,
then a return to settings and explicit confirmation; never automatically replay
a pending write. The current tenant five-minute successful-sign-in semantics
apply, not a new forced upstream password/MFA guarantee. Connection-test and
recovery/API-key sessions alone are not the needed human proof.

Human proof and policy mutations stay in their deliberate Rust-owned HTTP/browser
channel with existing Origin/cookie protections. Safe permitted reads project to
typed consumers; existing management operations retain their protocol scopes.
Do not expose cookies on MCP or a new protocol, add a password-bearing CLI command,
or treat a bearer credential/MCP scope as human proof. Do not implement an
unrestricted policy setter on another surface to get around these checks.

The approved persistence owners are spec **Persistence and authority changes**:
existing tenant login-policy state/revision and identity lookup, browser sessions,
device approvals/refresh anchors and existing authorization revision/invalidation
state. The policy table already exists. Use existing columns/query mechanisms
where sufficient. Any necessary migration must implement an explicitly named
property there; cite it, add a new migration and prove RLS/idempotence. Never edit
an applied migration. A new persistence purpose returns SPEC_REVISION_REQUIRED.
Do not store evidence or audit history in PostgreSQL.

## Ordered acceptance and implementation evidence

Each entry is the evidence record. Before new behavior, add/select its actual
consumer assertion, record its focused command and expected missing-behavior RED,
then implement and record GREEN plus affected regressions. Record actual paths,
assertions and limitations here. Existing correct behavior needs connected
regression proof, not invented RED or a replacement implementation.

### 1. An administrator sees current policy and the account warning

Maps to REQ-011/016; AC-006/011. Start with a real tenant containing a tested
Active company connection, a company-authenticated admin, password/social Users,
a company User, a disabled User and a Service. Through production settings and
typed HTTP, read state/revisions/readiness and the active-User warning count from
Rust. The other tenant's Users never contribute. Non-admin reads refuse and
anonymous method reads reveal no count or account directory. Missing read/warning
is the expected RED. Tier 1: extend server identity/browser integration and
production UI journey owners; tier 2 pins tenant isolation/count semantics.

Evidence (Rust/HTTP):

- Runner: `t.sh` = repository test Postgres + Mailpit + Keycloak/Dex env, then
  `mise exec -- cargo nextest run --locked "$@"`. Selector:
  `-p wyrd-server --features test-support --test integration -E
  'test(=platform_admin_e2e::company_sso::a_company_administrator_reads_the_policy_and_its_account_warning)'`.
- RED: `GET /v1/identity/login-policy` from the company admin's browser
  session returned 401 `WYRD_AUTH_401_UNAUTHENTICATED` (no such route).
- GREEN: 1 passed. Assertions are in `platform_admin_e2e/company_sso.rs`:
  the exact view (not required, revision 0, Active id/revision, ready, count);
  403 RBAC for a password viewer; no count or user keys in anonymous
  `/auth/browser/methods`. The count is 3: the fixture's seeded admin, a
  password User and a Google User. It excludes two company Users, a suspended
  User, the headless Service and another tenant's User, which proves tier-2
  tenant/count semantics through RLS.
- Owners: `HumanConnections::login_policy`, `wyrd_sql::queries::auth::login_policy`
  (`users_without_identity` matches identities by the Active issuer),
  `components/admin/identity.rs::read_login_policy`, wire types
  `LoginPolicyView`/`LoginPolicyConnection` in `wyrd-spec`.
- Mechanical deviation: the controlled company IdP lives in
  `platform_admin_e2e/company_sso.rs` (test-support build with Google
  fixtures), not `identity_e2e.rs`, which compiles without test-support.
- Production settings UI: pending.

### 2. A company administrator enables and disables enforcement

Maps to REQ-016, preserving REQ-002/004/005; AC-006. Through real company login
and group mapping or existing grant administration, establish the company admin
without linking a password/social User. From settings, inspect the warning,
explicitly enable, reload and see persisted policy plus company-only methods.
Unlinked accounts do not block confirmation. Ordinary Disable with fresh company
proof permits new allowed-method login. Non-admin, stale proof, password/social
or recovery-only proof, missing/unusable/untested connection and changed policy
or connection revisions refuse without changing state or credentials. If proof
expires, sign in again and explicitly confirm; login alone performs no mutation.
Missing human policy write/confirmation is the expected RED. Tier 1: production
UI → Rust → repository Postgres, including a second replica; tier 2 pins mutation
atomicity and revision conflict. Preserve existing connection activation recovery
checks rather than bypassing them for the fixture or UI.

Evidence (Rust/HTTP):

- Selector: scenario 1's runner with
  `-E 'test(=platform_admin_e2e::company_sso::a_company_administrator_enables_and_disables_required_sso)'`.
- TDD VIOLATED: `HumanConnections::set_login_policy` and
  `PUT /v1/identity/login-policy` were written with scenario 1, before this
  test existed. Its first run passed, so there is no RED.
- GREEN: 1 passed. Refusals that change neither the policy nor the password
  session: company member (403 RBAC); headless bearer, API-key recovery
  session and password admin session (403 `RECENT_LOGIN_REQUIRED`); stale
  policy revision (409 `policy_changed`); changed connection revision (409
  `connection_changed`); a company proof aged past five minutes (403).
  Accepted changes: a fresh group-mapped company admin enables while 2
  unlinked Users remain; a second replica reads the identical view; the
  methods report offers `company_sso` only. A stale proof cannot disable. A
  new sign-in alone leaves the policy unchanged. A confirmed Disable moves to
  revision 2, and password sign-in works again.
- Mutation atomicity: `set_login_policy` checks proof, connection, revision
  and readiness under `lock_human_connection_slot` plus the policy row
  `FOR UPDATE`, then writes and sweeps in one transaction. A refusal commits
  only its audit decision.
- Missing/unusable connection: see scenario 5. Production settings UI: pending.

### 3. Required SSO cannot be bypassed at a later login step

Maps to REQ-002/011/012/013/016/020; AC-006/011/015. Start password/social
browser/device/invitation flows while optional, enable through scenario 2, then
finish them. Rust refuses disallowed starts and racing completions, creates no
unauthorized session/User/grant, and grants no device authority. Company sign-in,
company-policy-compliant invitations and explicit company device approval work.
The UI consumes current methods, but direct HTTP still refuses hidden methods.
Keep sign-in start admission and its 429/503/no-side-effect controls intact.
Expected RED is an unconnected completion/policy consumer. Tier 1: production
login/invitation/device pages, actual HTTP and real CLI consumer with controlled
providers and repository Postgres; carry forward valid cycle-5 evaluator proof.

Evidence (Rust/HTTP):

- Selector: scenario 1's runner with
  `-E 'test(=platform_admin_e2e::company_sso::required_sso_refuses_every_later_password_and_social_step)'`.
- RED: a Google sign-in begun while optional and finished after enablement
  returned 303 with a session (`company_sso.rs`, "returning Google sign-in").
- GREEN: 1 passed, with no session and no User for:
  - Google sign-in and Google invitation enrollment begun while optional;
  - password invitation signup (403);
  - password sign-in (403 `method_unavailable`);
  - a new Google start (403).
  A device code approved by password before enablement redeems no token, and
  a password session cannot approve. A company-approved device code redeems,
  and company sign-in works.
- Enforcement owners:
  - `insert_browser_session` refuses a password/social row in the same
    statement while required, and `create_session` maps that refusal to
    `MethodUnavailable`; the enclosing enrollment rolls back.
  - `live_browser_session` refuses password/social sessions while required.
  - `issue_human_access` refuses a Password anchor while required, which
    covers device redemption and refresh renewal.
  - Starts use the cycle-5 evaluator unchanged.
- Tier 2: `mise exec -- cargo nextest run --locked -p wyrd-sql --test
  integration -E 'test(/^pg_login_policy::/)'` (same env) passed 1.
  `pg_login_policy::pg_tests::required_sso_ends_racing_password_sessions_for_good`
  commits a session inserted before the policy commit only after the
  policy's sweep. That session is not live while required; an insert under
  the policy records nothing; Disable revives neither it nor the session
  swept at enable; a new session works. This pin was written after the
  guard (TDD VIOLATED for this tier-2 case).
- Invitations: every invitation redemption names a social provider or a
  password, so none completes under the policy. Company first-login
  provisioning is the compliant path. Production login/invitation/device
  pages and the real CLI consumer: pending.

### 4. Existing credentials stop, and toggling policy cannot revive them

Maps to REQ-002/004/005/012/016; AC-006. Before enablement issue real browser
sessions and approved device refresh families through password, social and company
methods. Enable through settings and attempt use/renewal on another replica:
password/social credentials refuse under established freshness; company credentials
remain usable. Disable, then test both previously refused credentials and old
credentials **never presented while enforcement was enabled**: neither revives.
New permitted-method sign-in succeeds. Preserve ordinary short-lived access-token
rules, principal/access/keys and refresh replay/client binding; no method switching
or instantaneous global bearer revocation guarantee. Expected RED is missing
connected invalidation or resurrection. Tier 1: browser and actual CLI/client
renewal consumers; tier 2 pins original-anchor and durable invalidation semantics.

### 5. Outage, replacement and recovery preserve the tenant boundary

Maps to REQ-002/004/005/011/012/016/018; AC-006/013. With enforcement enabled,
replace/deactivate the company connection or interrupt the provider. Required
policy stays enabled; affected in-flight completion and original-connection
credentials refuse, and no password/social fallback appears. Ordinary Disable
cannot substitute a weaker proof. Exercise the existing independent operator
recovery procedure through its actual owner, not a new recovery system; prove
existing personal/Service API-key policy and platform login remain independent.
Another tenant is unaffected, and replacement never transfers an issuer/subject
between Users. Do not assert IdP-side removal alone immediately revokes Wyrd.
Tier 1: actual connection management, browser/device/renewal and recovery consumers,
including replica continuity. Expected RED is missing policy enforcement at those
boundaries; reuse already-correct connection lifecycle proof.

Also retain regression proof that shipped operator social-provider replacement/
disablement and password disablement refuse affected pending completions,
browser use and renewal across replicas, without method substitution or new
grants. Reuse valid existing proof where correct; this is shipped-control
correctness under REQ-012, not deferred self-service removal.

### 6. Setup, invitation and signup outputs reach the actual clients

Maps to REQ-002/004/005/011–014/017–020; AC-004/005/007–015. Use the real setup,
signup/invitation and scoped grants supplied by carryover, through production
onboarding/login, explicit CLI device approval, and actual Rust/Python/TypeScript
resource calls. Include a no-IdP/no-SMTP local journey and a company-policy journey;
do not fabricate a linked account to connect them. Observe permitted resources,
denied resources/administration, safe HTTP/MCP metadata and management denials,
and selected renewal refusal from scenario 4. Preserve contacts/reset, private
defaults, source admission, stored-secret protection, platform/tenant isolation
and credential ceilings at affected boundaries. Use existing tests/evidence for
unchanged branches; close actual consumer gaps, including absent required safe
methods/settings projections. Missing connected inputs or projections are the
expected RED, not an excuse to rebuild owners. Tier 1: existing production UI,
CLI, SDK and MCP journey owners against WyrdTestServer/repository Postgres,
Mailpit and controlled provider fixtures. Generated/exported contracts agree with
Rust; Node never owns token exchange or credential storage.

### 7. Real Google/GitHub compatibility and instructions match delivery

Maps to REQ-011/012/014/017/018; AC-004/005/007/013 and shipped-surface safeguards.
On an operator-configured test deployment, follow the actual registration/setup
instructions and complete real Google and GitHub sign-in/signup as applicable.
Record secret-free configuration and observed consent, callback and account-
selection results. Review docs against actual actions, limitations and refusal
messages, including no automatic login-method transfer. Controlled-provider tests
cannot supply this compatibility evidence. No hosted SMTP smoke is needed.
This is live compatibility/static documentation proof, not manufactured TDD.
Unavailable provider access is a material verification limitation: finish
unaffected work, disclose it and leave this entry unfinished, never claim smoke
or complete task acceptance.

## Verification and handoff

Test owners: `wyrd-server/tests/integration/{identity_e2e.rs,identity_ui_e2e.rs,
pg_browser_sessions.rs,platform_admin_e2e.rs}` and their in-owner modules;
`wyrd-sql` identity/policy/session integration owners; existing CLI saved-login,
Rust SDK, Python `tests/integration/` and TypeScript `wyrd/tests/integration/`
identity consumers; existing `wyrd-mcp` scoped journey owners. Keep tests requiring
services in their existing pg/provider lanes. Do not invent new test selectors in
planning; implementation records the exact focused selector before each RED.

Existing runner families inspected in `mise.toml`:

- `mise run test:identity:journey` includes real Keycloak/Dex, production UI,
  CLI and Rust/Python/TypeScript saved-login/resource consumers. Its constituent
  `mise run test:identity:ui` is useful while iterating; do not rerun both as a
  final duplicate checklist. Extend controlled social support in the test owner;
  Keycloak/Dex alone are not Google/GitHub smoke.
- `mise run test:platform:journey` owns operator/setup/signup/mail/platform
  regressions; `mise run test:principals:integration` owns applicable durable
  identity/grant seams; `mise run test:sql` covers new/changed SQL integration
  cases not selected by those narrower owners. Inspect actual selectors so the
  chosen command executes the added assertions.
- Run `mise run ui:check`, `mise run codegen:check`,
  `mise run check:tenant-isolation`, `mise run check:deps`, `mise run fmt` and
  `mise run lints` for the changed boundaries. Apply SDK owners' format/lint/type
  checks when their projections change and `mise run docs:check` for guide edits.
  Follow repository migration lifecycle only if an approved migration is needed.

All commands use the repository `mise` lifecycle; no raw Cargo command or missing-
environment workaround. This planning packet names test owners/runners, not new
test names. Integrated change review owns the full change-wide aggregates and
unchanged journeys; the connected consumers required above remain this task's
proof. Do not run `gate` just for task handoff or add benchmarks without the
maintainer's explicit request. Enumerate added/moved PostgreSQL, Scribe,
object-store or Vault waits if a defined hot path changes; match each to its
sanctioned boundary. No new onboarding hot-path wait or blocking audit is approved.

Return evidence in the entries above, behavioral verification and TDD compliance
separately, and any real external limitation. Do not claim the entire change or
deployment is complete from this task alone. No automatic review, deployment,
staging, commit or push is authorized.

Planning check (2026-11-23): one bounded independent reader inspected the current
spec, finalized design, this task and carryover ownership markers without code
or author conversation. It found no material comprehension gap. Local packet
links, requirement references and diff whitespace were checked. This is planning
evidence, not runtime verification or an implementation-readiness verdict.

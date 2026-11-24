---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 5
requirements: [REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017]
depends_on: [TASK-001, TASK-002]
---

# Social sign-in, workspace signup and team invitations

**Revision-11 ownership update (2026-11-23):** original cycles 7–8 (carryover
17–18) are deferred from launch; original cycles 9–10 (carryover 19–20) belong
only to [TASK-004](TASK-004-company-sso-and-launch-verification.md). The carryover
retains cycles 1–16. This file is historical evidence, not remaining-work authority.

Remaining-work owner: [TASK-003-carryover](TASK-003-carryover.md), approved spec
revision 6. Cycles 1–5 are completed/committed according to the maintainer; cycle
6's server slice is staged, uncommitted evidence. All unchecked/open items and
cycles 7–10 below are transferred to that task. This record retains historical
scope/evidence (including prior TDD limitations); it assigns no remaining work.
Where revision-3/4/5 text below differs from revision 6, the approved spec and
carryover govern. Do not execute the historical instructions independently.

An operator enables Google and GitHub sign-in and configures application email.
A tenant administrator invites a team with scoped access through single-use
links, copied or emailed. Each recipient opens their link, proves an allowed
sign-in method and enters that tenant with the intended access. A public visitor may instead create a new workspace, when enabled, and
becomes its administrator. Existing Users can connect another sign-in method
without losing their access or keys, and tenants can require company SSO.

## Scope, approach and dependencies

Implement AC-004 through AC-006 from [approved revision 3](../spec.md), preserving
the completed authorization behavior. TASK-001 supplies scoped assignment and
invitation validation, current grants, local device approval/refresh and account
administration. TASK-002 supplies the Access/account UI and connected client
journeys. Consume those outputs through their real contracts; do not rewrite
their revision-2 task records or substitute fixture-only grants for their output.
The maintainer has reported both predecessor tasks complete.

Follow [the finalized social design](../design/social-login.md), particularly
Availability and provider ownership, One callback workflow with an explicit
purpose, Returning login and invitation acceptance, New-tenant signup, Explicit
linking and removal, Require company SSO, and Email delivery. The owning
[security contract](../../../../architecture/wyrd-security-posture.md#social-admission-account-linking-and-application-mail),
[browser-session contract](../../../../architecture/wyrd-security-posture.md#first-party-browser-sessions)
and [human sign-in flow](../../../../architecture/wyrd-design.md#human-sign-in)
govern the implementation. Apply `$wyrd-ui` when editing the production UI.

Extend the existing owners rather than introduce another identity or onboarding
system:

| Existing owner | Missing behavior and connected consumers |
|---|---|
| `wyrd-server/src/components/platform/identity.rs`, platform routes and configuration | Reuse platform authorization, sealed secrets and provider test management for shared social clients and SMTP/signup/password settings. Add their operator UI projections. Tenant company SSO retains its own single connection slot and lifecycle. |
| `wyrd-auth/src/connections.rs`, `callback.rs`, `passwords.rs`; server `src/auth/` routes | Reuse provider verification and login completion. Bind the social purpose before the existing unknown-identity provisioning/group-sync branch: returning social login must not create a User or replace company bindings. Google uses OIDC first; GitHub then needs a concrete OAuth user-lookup adapter because it supplies no ID token. |
| `wyrd-auth/src/browser_sessions.rs`, `cli_logins.rs`, `refresh.rs` and SQL login/session owners | Carry exact provider revision and linked identity through browser login, explicit device approval and refresh. Enforce current method availability and tenant SSO policy through these existing lifecycle owners. |
| Existing identity store and `wyrd-sql/src/queries/auth/local_passwords.rs` | Multiple identities per User already exist. Add explicit attachment/removal and addressed invitation links, resend and send outcomes without email-based User resolution. Extend existing scoped consumption and issuer-authority checks. |
| `wyrd-server/src/components/platform/provisioning.rs::TenantProvisioning` and SQL platform provisioning | Extend its directory claim, builtin/SYSTEM/table initialization and promotion/failure workflow with human-owner initialization. The existing privileged flow returns a bootstrap credential; public signup instead initializes its proven User/admin binding and returns an ordinary tenant session. Do not duplicate orchestration or fabricate a platform caller. |
| UI login/signup, Settings → Access and account routes; `src/lib/server/auth/` | Render Rust's allowed methods, addressed bulk invitation results, account linking and SSO policy. Node forwards browser context; Rust owns exchange, proof, secrets and durable behavior. |

Operator clients/settings, tenant administrator recipient/access selections,
provider responses and the browser-bound signup/linking attempts are the real
inputs. Persist approved coordination metadata through `wyrd-sql`; return safe
management results, per-recipient outcomes and ordinary tenant credentials to
the UI and existing CLI/SDK consumers. SMTP is the one new, optional delivery capability:
use the existing secret handling and a minimal generic SMTP implementation;
there is no existing mail workflow to reuse. Send after commit, outside the
tenant transaction. No provider-specific email implementation is needed.

Update typed `wyrd-spec` contracts/stable errors, Rust server composition,
affected shared-client/CLI/SDK/MCP projections, generated exports, fixtures and
user/operator documentation together where the delivered surface exposes them.
Keep programmatic management equivalent to UI management; a provider redirect
remains human proof, not a new noninteractive credential grant.

Schema work is limited to the revision-3 purposes in
[Persistence and authority changes](../spec.md#persistence-and-authority-changes).
Cite that section for each migration change. Add migrations; never edit an
applied one. Tenant tables lead with `data_tenant_id`; genuine platform/pre-tenant
records use the approved platform distinction. Every new PostgreSQL table enables
and forces RLS. Preserve PostgreSQL-owned coordination time. Physical names and
private representation remain implementation-owned; additional persistence
purposes or SQL functions require `SPEC_REVISION_REQUIRED`.

Exclusions remain those of the spec: no cross-tenant person, account merging,
public password signup, SCIM, generic provider framework, continuous upstream
offboarding, provider-token vault, mail queue/automatic retries, CAPTCHA or
operator-wide creation caps. Resend is simply a compatible SMTP service. Preserve
API-key policy and independent operator recovery; do not add email notifications
to the existing copy-access-request flow.

## Ordered acceptance and implementation evidence

Implement these cycles in order, beginning with the smallest observable operator
action. Each entry owns its evidence: append actual assertion locations, focused
commands, observed RED failures, GREEN results and material limitations in place.
Record new selectors after adding tests and before RED. Already-correct behavior
needs regression/static proof, not a manufactured failure or production change.
Use real Rust endpoints and repository Postgres for delivered behavior; controlled
OIDC/OAuth and SMTP servers supply only external-provider responses.

### 1. An operator configures and tests Google beside company SSO

**Maps to:** REQ-011; AC-004.

Start with an operator session, a tenant administrator and an existing company
connection. Through the platform management API, the operator supplies Google
client metadata and a secret, receives the exact registration callback and tests
the configuration through provider sign-in before enabling it. Read it through
another replica and the operator UI: the same tested revision appears, without
the secret. Rust's method listing and the tenant login page offer Google beside
company SSO; with no social configuration they offer no social button. Password
starts enabled and public signup starts disabled. A tenant admin's direct shared
configuration write is denied, while their own company connection remains
manageable. A platform session alone does not authorize tenant settings changes.

**Proof:** Tier-1 platform HTTP and production UI journeys, extending platform
identity and UI test owners below. Initially fail on absent shared provider
management/method projection. Inspect sealed storage and secret-free responses;
provider setup must not replace the tenant connection or create tenant grants.

**Evidence (HTTP, SQL):**

- Owners extended: platform authorization/audit helpers in
  `components/platform/identity.rs` (`authorize`, `authorize_read`,
  `commit_decision`), the sealing keyring, and the relying party already used
  for company and platform OIDC. The new pieces are the `SocialSignIn` owner in
  `wyrd-auth/src/social.rs`, the platform routes in `components/platform/social.rs`,
  and the anonymous `/auth/social/{provider}/callback` and `/auth/browser/methods`
  in `wyrd-server/src/auth/social.rs`. Contracts live in
  `wyrd-spec/src/auth/social.rs`, and `[social]` endpoint overrides are refused
  in production (`config.rs::SocialConfig`). The migration is
  `20261121000000_social_providers.sql` and cites spec "Persistence and
  authority changes". It adds platform provider records, operator test state and
  login settings, each with ENABLE+FORCE RLS and an `operator_access` policy. It
  adds no SQL function.
- Journey: `platform_admin_e2e::an_operator_configures_and_tests_google_beside_company_sso`
  (`tests/integration/platform_admin_e2e.rs`, end of file). It asserts:
  - Before configuration, methods are `{password:true, company_sso:true, providers:[], signup:false}`.
  - Every provider is unconfigured, and the defaults are password enabled and signup disabled.
  - Configuring returns the exact callback `http://wyrd.test/auth/social/google/callback`
    with revision 1, untested, and no secret.
  - The stored envelope does not contain the plaintext.
  - Enabling an untested revision returns 409, and testing a stale revision returns 409.
  - The authorization URL has the configured client and callback, and scope `openid profile email`.
  - The controlled-OIDC callback passes, and a replay returns 401.
  - After enabling, a second replica lists revision 1 as enabled with no secret, and methods list
    `["google"]` beside company SSO.
  - A tenant admin's shared write returns 401, while their `/v1/identity/oidc/connections`
    read still returns 200 with the Active connection.
  - A platform session's tenant candidate write returns 401.
  - The `wyrd.auth_role_bindings` count is unchanged.
- SQL: `pg_platform_social::pg_tests::{a_test_of_a_replaced_revision_certifies_nothing,
  the_tenant_role_cannot_reach_shared_providers}` (`wyrd-sql/tests/integration/pg_platform_social.rs`).
  They show that a replaced client gets a new untested revision and that a stale stamp
  is refused. A tenant session sees no provider and changes no login setting.
- Commands:
  - `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(/^pg_openapi_contract::/) | test(/^platform_admin_e2e::/)" --no-fail-fast'`:
    GREEN, 66 passed. Migration idempotence also passed.
  - `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E "test(/^pg_platform_social::/)"'`:
    GREEN, 2 passed.
  - `mise run fmt`, `mise run lints`, `mise run codegen:check`,
    `mise run check:tenant-isolation` and `git diff --check`: clean.
- RED: not observed. Production code was written before this test, so cycle 1
  is a **TDD process violation**. The journey was GREEN on its first run.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 14/18; historical open operator UI evidence:** no platform administration UI exists, and approved
  browser sessions are tenant-only ("First-party browser sessions"; Node owns
  no credential). The spec requires a "platform UI" (REQ-011), but a browser-held
  operator credential would be a new credential purpose. The operator-UI half
  of this entry and its UI journey therefore remain **FAIL** pending a maintainer
  decision.

### 2. Google proves a bound login without admitting a stranger

**Maps to:** REQ-012; AC-004.

With the tested Google client enabled, an unknown subject starts login from a
tenant page and completes the real callback using controlled valid OIDC claims.
Wyrd refuses with invitation/signup guidance and creates no User, identity,
binding or session. Vary email to match an existing User: the result stays refused.
Exercise wrong issuer/audience/signature/nonce, expired or replayed transaction,
wrong browser/origin, purpose/tenant substitution and provider outage through the
same endpoints. Disable or replace the provider between start and callback;
completion on another replica also refuses. Request only the agreed Google scopes.

**Proof:** Tier-1 server/provider journey plus OIDC validation regression proof.
Initially fail because shared purpose-bound social login is absent; the existing
company JIT path must not be used to make this pass. Assert no local side effects
or provider-token retention, while retaining company test/device behavior.

**Evidence (HTTP, SQL):**

- Owners extended:
  - `POST /auth/browser/login` gains an optional `provider`
    (`wyrd-spec/src/auth/browser.rs::BrowserLoginRequest`). Absent, it keeps
    company SSO.
  - `wyrd.auth_login_state` gains `flow = 'social'` rows. They bind the purpose,
    the exact platform provider revision and client, the browser digest, PKCE
    and the nonce. The migration is `20261121000001_social_login_state.sql` and
    cites "Persistence and authority changes: wyrd.auth_login_state". It adds
    no SQL function and reuses `wyrd.auth_login_state_tenant`.
  - `insert_social_login_state` and `consume_social_login_state` sit beside the
    GitHub flow in `queries/auth/login_state.rs`.
  - `BrowserSessions::{begin_social, complete_social}` live in
    `wyrd-auth/src/social.rs`. They reuse `verify_response_issuer`,
    `provider_refusal`, `audit_authorization_code_failure`, `user_id_by_identity`
    and the server's `login_refused`. The company `/auth/callback` path, with its
    `ensure_user_identity` JIT and group sync, is not called.
  - `/auth/social/{provider}/callback` tries the operator test first, then a
    tenant sign-in.
  - Each callback clears only its own `wyrd_login` cookie path: `/auth/callback`
    or `/auth/social`.
- Journey: `platform_admin_e2e::google_proves_a_bound_login_without_admitting_a_stranger`
  (`tests/integration/platform_admin_e2e.rs`, end of file). It asserts:
  1. The begin request has the configured client, the social callback and scope
     `openid profile email`, with no `access_type`. The cookie has `Path=/auth/social`.
  2. A stranger with valid claims gets a 303 to `/t/{slug}/login?login=no_account`.
     User, identity, session and role-binding counts are unchanged, and a replay
     returns 400.
  3. A pre-seeded User with the same email (`ops@example.com`) still gets
     `no_account`.
  4. These are each refused with `login=failed`: no cookie, another browser's
     cookie, a foreign `iss`, a wrong nonce, a wrong audience, and an unknown
     signing key.
  5. A Google state presented at `/auth/callback` returns 400.
  6. Google disabled between start and callback, completed on a second replica,
     is refused. So is a client replaced between start and callback.
  7. A provider outage returns 503. No row in `wyrd.auth_login_state` or
     `platform.social_login_state` contains the provider access token.

  A temporary local probe panic (reverted, not committed) read the trace. It
  confirmed each refusal's cause in order: `INVALID_TOKEN` for no_account and
  email, "did not begin it" twice, `INVALID_TOKEN` for the issuer,
  `INVALID_NONCE`, the audience mismatch, the unmatched key,
  `INVALID_TOKEN` for disabled and replaced, and `503 VERIFY_UNAVAILABLE`.
- RED: `scripts/postgres/with-test-postgres.sh -- bash -lc 'cargo nextest run --locked -p wyrd-server --test integration -E "test(=platform_admin_e2e::google_proves_a_bound_login_without_admitting_a_stranger)"'`.
  Provider setup passed, then begin failed with
  `400 WYRD_SPEC_400_VALIDATION ... unknown field \`provider\``. Social login
  was absent.
- Diagnosis of a failure during GREEN work:
  - **Symptom:** the stranger got `login=failed`, not `no_account`.
  - **Evidence:** trace `browser sign-in refused code="WYRD_AUTH_400_INVALID_NONCE"`.
  - **Cause:** wiremock answers with the first mounted matching mock. The
    test's `mount_platform_token` appended a `/token` reply behind
    `enable_google`'s reply, which carried the provider-test nonce.
  - **Fix site:** the test fixture. Both appending calls now reset through
    `mount_platform_provider`. An independent read-only diagnostician
    confirmed the cause and found the second site, at step 6. Production
    verification was correct.
- Cycle-1 journey replay expectation changed from 401 to
  `400 WYRD_AUTH_400_INVALID_STATE`. The replay is now refused as the company
  callback refuses an unknown state; the assertion is equally strict.
- GREEN: the RED selector plus
  `test(=platform_admin_e2e::an_operator_configures_and_tests_google_beside_company_sso) | test(/^pg_browser_sessions::/) | test(/^pg_openapi_contract::/) | test(/callback|github/)`:
  passed. `cargo nextest run --locked -p wyrd-auth --lib`: 136 passed.
  `-p wyrd-server --lib -E "test(/^auth::/) | test(/^components::auth::/) | test(/^components::platform::/)"`:
  62 passed. These keep company browser, device and test-callback behavior.
- Diagnosis of a lane failure:
  - **Symptom:** in the `wyrd-sql` integration lane,
    `pg_migration::pg_tests::{federated_identity_roundtrips_and_rejects_cross_tenant_user_id,
    nullable_oidc_user_email_roundtrips}` failed at cleanup with `23503` on
    `auth_authorization_revisions_data_tenant_id_fkey`.
  - **Cause:** `insert_user` advances the tenant authorization revision
    (`queries/auth/users.rs`), and the test cleanup helpers deleted tenants
    without deleting that row.
  - **Fix site:** `cleanup_auth_test_rows` and `cleanup_refresh_token_test_rows`
    in `pg_migration.rs`, the second as a latent instance of the same defect.
    An independent read-only diagnostician confirmed the cause; this diff does
    not cause the failure. `cargo nextest run --locked -p wyrd-sql --test integration`:
    142 passed.
- Limitation: a linked social identity cannot sign in yet. It is refused with
  `login=failed` until the invitation/identity anchor in entry 4 exists.

### 3. An administrator invites a team by copied and emailed links

**Maps to:** REQ-013, REQ-017; preserves REQ-001/REQ-008; AC-004.

Without SMTP, a tenant admin pastes at least ten recipient addresses into
Access/API and chooses the predecessor's explicit role/scope assignments. The
response returns one link per recipient (tenant invitation page, code in the URL
fragment) with "not emailed" outcomes, and the UI explains that Wyrd does not
email them. Pending entries list recipient, assignments, expiry and last
outcome, never a code; resend returns a new link and voids the old one; revoke
voids it. Storage holds only code verifiers.

The operator then configures the local SMTP test server (Mailpit) through
generic host/port/TLS, optional credentials and sender settings, and performs
test-send. New invitations and resends are emailed separately after commit; the
server receives one correctly addressed message per recipient whose link matches
the returned link. The tenant admin cannot read/change relay credentials, and
reads omit secrets. With relay failure, report failure rather than delivery
beside the returned link, retain pending state, create no active User and allow
resend after recovery. Relay acceptance is shown as acceptance, never inbox
delivery. A stalled SMTP exchange must not hold the tenant transaction; no raw
code is stored as a mail spool or logged.

**Proof:** Tier-1 management/UI → Rust → Postgres → Mailpit journey; SQL
integration proof for coordination and transaction boundaries as needed.
Initially fail on absent recipient/bulk/link/delivery contracts. Consume actual
returned and sent links in the next cycle rather than constructing unrelated
invitation rows.

**Evidence (copied-link slice; SMTP pending):**

- Reuse: extends `wyrd.auth_user_invitations`, `wyrd-auth::passwords` issue/
  consume and the existing `POST`/`DELETE` invitation routes; signup redemption
  (`/auth/browser/password/signup`) is unchanged. Migration
  `20261121000003_invitation_recipients.sql` cites spec "Persistence and
  authority changes": adds `recipient` and `delivery`, removes recipient-less
  rows (nothing shipped).
- Journey `crates/wyrd/wyrd-server/tests/integration/pg_browser_sessions.rs`
  `an_administrator_invites_a_team_by_copied_links`: ten recipients yield ten
  distinct fragment links marked `not_emailed`, no-store; list shows recipients
  and never codes; storage holds no code; a link enrolls once; resend replaces
  and voids the old link; revoke voids and leaves the list; resend of a revoked
  invitation is 404; empty, malformed and repeated recipient lists are 400 and
  issue nothing.
- RED: `scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p
  wyrd-server --test integration -E
  'test(=pg_browser_sessions::an_administrator_invites_a_team_by_copied_links)'`
  failed at the first issue with `400 … unknown field 'recipients'`.
- GREEN: `-E 'test(/^pg_browser_sessions::/)'` 9/9;
  `-E 'test(/pg_openapi_contract|pg_browser_sessions/)'` 34/34; `wyrd-sql
  --test integration` 142/142; `wyrd-auth --lib` 137/137 including
  `passwords::tests::invitation_recipients_are_distinct_addresses`; CLI
  `-p wyrd-cli --test cli -E 'test(/principal_user_cli_journey/)'`; SDK
  `-p wyrd-ai --test integration -P journey --run-ignored=all -E
  'test(=principal_roles::administrators_create_and_invite_users)'`; UI
  `WYRD_UI_INTEGRATION=1 pnpm … test src/lib/ui-pages.integration.test.ts -t
  'Alice is given payments|an administrator manages direct, group and
  custom-role|Access tells an unavailable|signup|invitation'` 4/4 (Access
  pastes two addresses, shows two links, the not-emailed notice and a pending
  row; Alice and Bob enroll by opening the copied link); `mise run ui:check`;
  `fmt`, `lints`, `codegen:check`, `check:tenant-isolation`, `git diff --check`.
- Diagnosis (UI journeys after the first production edit). Symptom: the signup
  page left the code empty, then the post-signup redirect missed `waitForURL`.
  Evidence: probed browser console showed `Cannot call replaceState(...) before
  router is initialized` from both `$effect` and `afterNavigate`; the probed
  final URL kept `#invitation=…`. Cause: SvelteKit refuses `replaceState`
  during hydration, and browsers carry a fragment across a 303 when the form
  posts to the fragment-bearing URL. Fix site: the signup page only — read the
  fragment in `onMount` and post to `action="?"`; no history rewrite, which the
  spec does not require (fragments reach no server or referrer).
- Process note: the fix removed this cycle's own unrequired assertion that the
  address bar loses the fragment, without a diagnostician; recorded here for
  review. TDD: RED observed before production code for this slice.

**Evidence (SMTP slice):**

- Reuse: the relay joins the existing operator-only `platform.login_settings`
  record (migration `20261121000004_application_mail.sql`, citing spec
  "Persistence and authority changes": platform application-mail/signup
  settings records), its password sealed with the deployment keyring as
  provider secrets are. Platform `authorize`/`commit_decision` govern
  `GET`/`PUT /platform/mail` and `POST /platform/mail/test`
  (`components/platform/social.rs`). New: `wyrd-auth/src/mail.rs::ApplicationMail`
  over `lettre` (rustls/aws-lc with the platform verifier, as reqwest uses);
  no SMTP code existed. Invitation create/resend commit, then send
  (`accounts.rs::email_invitations`) and record each outcome in a second short
  transaction keyed by code verifier. No queue, retry, spool or message log.
- Test relay: Mailpit (`docker-compose.yml`, pinned digest) starts beside
  Postgres in `scripts/postgres/with-test-postgres.sh`, which exports
  `WYRD_TEST_MAILPIT_SMTP`/`WYRD_TEST_MAILPIT_API`; the test panics, never
  skips, without them.
- Journey `platform_admin_e2e::an_operator_relays_invitations_through_smtp`:
  tenant admin GET/PUT of the relay is 401; reads return `password_set`, never
  the password, and the stored envelope lacks the plaintext; test-send arrives;
  three invitations each produce exactly one Mailpit message to only that
  recipient, from the sender, naming the workspace and carrying the returned
  link, marked `accepted`; resend mails the replacement link; with the relay
  down the link is still returned beside `failed`, listed pending as `failed`,
  no User is created, and resend after recovery is mailed; with a relay that
  accepts TCP and never answers, the invitation is listed (committed) while the
  send is still waiting, then reports `failed`.
- RED: `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run
  db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test
  integration -E
  "test(=platform_admin_e2e::an_operator_relays_invitations_through_smtp)"'`
  failed at the first relay read: `401 … token is not a tenant access token`
  (no `/platform/mail` route).
- GREEN: same selector; `-E 'test(/^pg_openapi_contract::/) |
  test(/^platform_admin_e2e::/) | test(/^pg_browser_sessions::/)'` 77/77;
  `wyrd-sql --test integration` 142/142 (migration idempotence); `fmt`,
  `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`,
  `docs:check`, `git diff --check`.
- Limitation (transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 14/18): the production UI has no operator relay page (same open
  operator-UI decision as entry 1); the tenant Access page already labels
  `accepted` as "accepted by the mail relay" and `failed` as "email failed".
  The relay is managed over platform HTTP only, as are social providers.
- TDD: PASS for this slice.

### 4. The addressed recipient redeems scoped access, then returns with Google

**Maps to:** REQ-012, REQ-013; preserves REQ-001/REQ-008; AC-004.

Use a link produced by cycle 3. Its holder completes Google enrollment
(a differing Google email does not refuse), receives one tenant-local User,
identity, exactly the invited assignments and session, and consumes the invitation
atomically. Use that session against an allowed resource and a second resource
outside the invited scope. Log out and return with the same tenant/issuer/subject:
the same User and access return. Social sign-in must leave direct, local-group
and company-derived assignments intact; no provider groups are needed.

Also redeem a copied link through invited password enrollment. Opening a link
spends nothing. An invalid, expired, revoked, superseded or replayed link,
changed issuer assignment authority or invalid role/scope cannot grant access. An already-owned provider
identity refuses new-User enrollment with sign-in/access-request guidance.
Concurrent redemption has one winner; failed consumption leaves none of the
User/identity/bindings/session writes committed. Inactive Users/tenants cannot
return. Reuse one provider identity in a second tenant and observe distinct
Users/grants/credentials, with no cross-tenant credential use.

**Proof:** Tier-1 invitation email → enrollment → real authenticated resource
calls, including production UI; SQL integration fault/concurrency assertions
support atomicity. Initially fail on missing link-based social/password enrollment
and social returning login. Matching email alone must never resolve an existing User.

**Evidence (server slice; UI slice below):**

- Reuse: invitation consumption, issuer revalidation and binding moved from
  `password_signup` into one `BrowserSessions::redeem_invitation`
  (`wyrd-auth/src/passwords.rs`), which password and Google enrollment both
  call, so every refusal and the single-winner `UPDATE … RETURNING` apply to
  both. Sessions reuse `create_session`/`TenantGrant::OidcLogin`; identities
  reuse `wyrd.auth_user_identities` (tenant/issuer/subject primary key).
- Migration `20261121000005_social_identity_anchors.sql` (spec "Persistence and
  authority changes": tenant identity store, `wyrd.auth_login_state`,
  `wyrd.auth_browser_sessions`): identity `id`; `social_invitation_hash`
  required exactly for purpose `invitation`; session `social_provider`,
  `social_provider_revision`, `social_identity_id` (FK `ON DELETE CASCADE`),
  counted in the one-authority constraint. Device and refresh anchors stay
  with entry 8 (`human_anchor()` is `None` for a social session).
- Contract: `BrowserLoginRequest.invitation_code` (optional, needs `provider`);
  `BrowserCredential::Social`; callback reason `login=account_exists`.
- Test `platform_admin_e2e::an_invited_person_enrolls_with_google_and_returns`:
  1. beginning Google from the link spends nothing;
  2. a differing Google email enrolls one User, one identity and exactly
     `viewer` on `Prompt/payments`, and consumes the invitation;
  3. the session (`credential: social`) reads `Prompt/payments` (200) and
     not `Prompt/ledger` (404 `WYRD_REGISTRY_404_CARD_NOT_FOUND`);
  4. after a new sign-in with a changed email, the same User returns and no
     User, identity or grant is added;
  5. spent, revoked, superseded and expired links give `login=failed` and
     create no User; an owned identity gives `login=account_exists`, sets no
     cookie and spends no link;
  6. in a second tenant the identity is `no_account` until invited, then a
     distinct User; a session relabeled to the other tenant gets 401;
  7. a suspended User cannot return.
- RED: `unknown field invitation_code, expected tenant or provider` (400 at
  begin). GREEN: `cargo nextest run --locked -p wyrd-server --test integration
  -E 'test(=platform_admin_e2e::an_invited_person_enrolls_with_google_and_returns)'`
  passed under `scripts/postgres/with-test-postgres.sh`.
- Shared-owner proof (password path, same `redeem_invitation`):
  `pg_browser_sessions::local_signup_is_tenant_bound_and_rate_limited`
  (concurrent redemption has one winner with no partial User, expiry, other
  tenant), `pg_browser_sessions::scoped_invitations_enroll_exactly_their_assignments`
  (inapplicable scope, deleted role, former issuer), and
  `pg_browser_sessions::an_administrator_invites_a_team_by_copied_links`
  (password enrollment from a copied link). Atomicity: every Google refusal
  above happens before commit on one `TenantConn`, and step 5 asserts no User
  remains. Inactive tenant: the shared `TenantTokenIssuer::verify` status check.
- Lanes: `mise run fmt`, `mise run lints`; wyrd-server integration 463/463
  (one earlier full run failed
  `pg_github_pull_request_extraction::an_extracted_review_that_fails_is_the_next_baseline`
  once under load; it passed alone 3 times and in the rerun; diagnosis below);
  wyrd-sql integration 142, wyrd-auth lib 137, wyrd-spec lib 813.
- Diagnosis of the one-off full-lane failure:
  - **Symptom:** `pg_github_pull_request_extraction::an_extracted_review_that_fails_is_the_next_baseline`
    failed once (9.65 s, 206/463) in a full `wyrd-server` integration run; no
    panic was captured. It passed alone three times and in a full rerun.
  - **Evidence:** a read-only Codex diagnostician was given the command and
    the diff. It found `Deployment::settled()`
    (`pg_github_pull_request_extraction.rs:441`) returns when the result is
    visible. The runner stores the result (`verification/runner.rs:1713`)
    before settling the queue row (`:1620`). The next head submission
    (`pg_github_pull_request_extraction.rs:1071`) cancels a still-`running` row
    as superseded (`verifier_executions/pull_requests.rs:148`), and baseline
    selection (`:284`) needs `status = 'completed'`.
  - **Cause (unconfirmed without the original output):** the test helper
    races queue settlement under load. This diff does not touch that sequence.
  - **Fix site:** `Deployment::settled()` should also wait for the committed
    `completed` queue row; callers in `pg_github_pull_request_extraction.rs`
    and `pg_github_pull_requests.rs`. Outside this task's write set, so it is
    reported, not changed.

**Evidence (UI slice):**

- Reuse: the login and signup pages render the providers Rust's
  `GET /auth/browser/methods` returns, and begin through the existing BFF
  `browserSessions.begin`, which now forwards `provider` and
  `invitation_code`; the callback stays Rust's `/auth/social/google/callback`
  behind the BFF hook. The journey's server uses the existing
  `with_social_endpoints_for_test` (exposed to the TS harness as
  `googleIssuer`, with `SocialConfig` re-exported from `wyrd-testing`) and
  `initialize_platform_root` (exposed as `initializePlatformRoot`). Google is a
  loopback fake inside the test file that serves discovery, an Ed25519 key
  set, an authorize step that returns at once, and an EdDSA-signed ID token,
  as `platform_admin_e2e` mounts with wiremock.
- Test `ui-pages.integration.test.ts` "an invited person joins with Google
  and returns with Google": the operator configures, tests and enables Google
  over HTTP; the administrator invites through Access with `viewer` on
  `Prompt/payments`; the recipient opens the link and uses **Join with
  Google** with a differing email, and lands in the tenant holding exactly
  that binding; after **Sign out**, **Continue with Google** returns to the
  same single User; a second link opened with that Google account ends at
  `login=account_exists` with its notice; an uninvited Google account ends at
  `login=no_account` with its notice and adds no User.
- TDD: `VIOLATED` for this slice. The page edits were written before the
  journey; the journey's first run failed only on a test locator (sign-out
  lives in the identity menu) after enrollment had already succeeded.
- GREEN: `mise exec -- scripts/postgres/with-test-postgres.sh -- env
  WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
  src/lib/ui-pages.integration.test.ts -t 'joins with Google'` (1 passed).
- Docs: `docs/src/content/docs/operate/add-people.md` describes **Join with
  Google**, returning with **Continue with Google**, and the second-link
  refusal; `llms-full.txt` regenerated.
- Lanes: `mise run ui:check`, `ts:format`, `ts:lints`, `ts:typecheck`,
  `ts:build:testing`, `fmt`, `lints`, `docs:check`, `git diff --check`.

### 5. GitHub follows the same admission flow using its stable account ID

**Maps to:** REQ-011, REQ-012, REQ-013; AC-004.

The operator registers/tests/enables GitHub through the same management journey.
Invite a recipient and complete code/S256-PKCE/state exchange, authenticated user
lookup through controlled GitHub responses (no email API access). Redeem and return through real endpoints. Rename the GitHub login
while retaining its numeric ID: Wyrd returns the same User. Another numeric ID
with a matching email/login does not inherit it. Repeat the common invitation,
purpose, revision and stranger refusals through this adapter. Provider failure
must not create a User or fall back to another method.

**Proof:** Tier-1 OAuth → invitation → session → authorized resource journey.
Initially fail on the absent OAuth identity adapter. Inspect requested scopes
and transient-token handling: no repository/organization access, fabricated ID
token, retained provider token or social role mapping. Reuse common flow proof
where it exercises both adapters; do not substitute Google-only proof for GitHub.

**Evidence:**

- Reuse: GitHub joins the existing `SocialSignIn` branches (`authorize`,
  `issuer`, `verify_code`), so begin, login-state binding, invitation
  redemption, returning sign-in, refusals and session creation are the code
  Google runs. The gap was the OAuth exchange itself, which the OIDC relying
  party cannot do (GitHub issues no ID token). `GithubOAuth`
  (`wyrd-auth-oidc/src/github.rs`) sits beside `RelyingParty` and sends both
  requests through its screened, redirect-free, bounded `ScreenedHttp::send`
  (now `pub(crate)`), with `openidconnect`'s PKCE and state generators. No
  migration: the login state keeps a random nonce nothing checks.
- Config: `[social] github_url` / `github_api_url` (refused in production)
  feed `SocialEndpoints.github`; identities always use the fixed
  `GITHUB_NAMESPACE` (`https://github.com`). The operator's registration
  instructions no longer mention `user:email`.
- Test `platform_admin_e2e::an_invited_person_enrolls_with_github_and_returns`:
  1. the operator configures, tests (a real callback through the mock) and
     enables GitHub;
  2. the authorization URL carries `client_id`, `state`, `code_challenge`,
     `code_challenge_method=S256` and the GitHub callback, and no `scope`;
     the mock token endpoint only answers a form carrying `code_verifier`
     and the client secret, and the user API only answers that token;
  3. enrollment creates one User and one identity under `https://github.com`
     with subject `4242`, exactly `viewer` on `Prompt/payments`, and a
     `social` session that reads `Prompt/payments`;
  4. a renamed login with the same ID returns to the same User, adding no
     User, identity or grant;
  5. ID `9999` with the same login and email gets `login=no_account`;
  6. a spent link (`failed`), an owned identity (`account_exists`) and a
     revision replaced mid-sign-in (`failed`) admit no one;
  7. a `502` from GitHub's token endpoint answers a server error, sets no
     session and creates no User.
- Unit `wyrd-auth-oidc github::tests::a_refused_code_reads_no_user`: a
  `200 {"error": …}` token answer is rejected and the user API is never
  called.
- Inspection: no scope is requested; the access token lives only inside
  `GithubOAuth::identity` and is dropped on return (no column stores it, no
  refresh-token vault); no JWT is built from GitHub data; `groups` is empty
  and enrollment grants only the invitation's assignments.
- RED: the test failed at `test begins` with `404 WYRD_SPEC_404_NOT_FOUND`
  "github sign-in is not configured". GREEN: same selector passed (1.7 s)
  under `scripts/postgres/with-test-postgres.sh`. The unit test was written
  after the adapter (supplementary to the journey's RED).
- Lanes: `mise run fmt`, `lints`, `codegen:check`, `check:deps`,
  `check:tenant-isolation`; `wyrd-auth` + `wyrd-auth-oidc` lib 185/185;
  wyrd-server integration `platform_admin_e2e::*` and `pg_browser_sessions::*`
  54/54; native-testing `cargo check`.

### 6. A visitor creates a workspace and becomes its administrator

**Maps to:** REQ-011, REQ-012, REQ-014; AC-005.

The operator enables public signup. A visitor starts signup with an enabled
Google/GitHub client, proves identity, supplies a valid new workspace name/slug
and confirms. Drive the existing provisioning owner through claim, initialization
and promotion. The returned ordinary tenant session can administer that usable
workspace, whose builtin roles, SYSTEM and tables exist. Inspect its User/admin
binding; the response contains no platform principal/authority or bootstrap key.
Use that admin session to invite a teammate through cycles 3–4.

Signup disabled at start or completion, invalid slug, existing tenant slug,
invalidated provider, shared source/identity rate-limit refusal or unavailable
admission state creates no active tenant. Across replicas, race/replay the same
attempt, interrupt initialization and retry as its owner: no second tenant or
active ownerless tenant appears. Another identity cannot adopt its failed claim.
Lose the successful session response, then sign into the completed tenant rather
than creating it again. The same provider account can create a second independent
tenant; email/domain/provider organization gives no access to an existing one.

Revision 5 abuse controls (REQ-014):

- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** Trusted source: behind a configured trusted ingress, two visitors with
  distinct client-address headers get separate windows; that header from an
  untrusted peer is ignored; password-login source admission uses the same
  source. A request with no determinable source is refused, not admitted.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** IPv6: two addresses in one `/64` share a source window; IPv4-mapped
  addresses normalize to IPv4.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** No pre-proof row: starting signup writes no `platform` row; the
  round-trip state rides a sealed, short-lived, HttpOnly cookie. A replayed
  callback records no second proof (state uniqueness at proof insert).
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** Initiation admission: a flood of signup starts from one source is
  refused before provider discovery.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** `Retry-After`: every signup `429` carries the header in whole seconds,
  matching `details.retry_after_seconds`.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** Pseudonymous, bounded state: stored admission keys hold no plain
  issuer, subject or address; a background batch purges expired windows and
  expired/completed attempts; no public request runs unbounded deletion;
  a completed attempt keeps no email.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** Challenge (when configured): a missing, invalid, wrong-hostname,
  wrong-action or reused token creates no tenant; the provider being
  unreachable refuses creation; admission runs first; no challenge when
  unconfigured. Uses a local mock Siteverify server.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** Owned-workspace cap: at the cap, creation is refused with a stable quota
  error (not 429), atomically across concurrent completions; a retired
  workspace no longer counts; the count survives attempt purge.
- **Transferred to [TASK-003-carryover](TASK-003-carryover.md), cycles 7–13:** Crash recovery: an attempt left `creating` past the stale bound is
  resumed by its own completion holder and refused to anyone else.

**Proof:** Tier-1 public UI/API → existing provisioner → tenant administration,
with real Postgres failure/retry/concurrency proof. Initially fail on missing
human-owner provisioning and public admission. Prove reuse of the existing
directory lifecycle without claiming a cross-plane atomic transaction.

**Evidence (server/API slice):**

- Persistence: migration `20261121000006_workspace_signup.sql` adds
  `platform.signup_attempts`, `platform.signup_admissions` and
  `platform.tenants.signup_attempt_id`, as the spec's "Persistence and
  authority changes" table names ("New platform pre-tenant signup records and
  admission counters"; "platform.tenants and existing provisioning state").
  Both tables enable and force RLS. They are platform records with no
  tenant key. No provider token is stored. Queries live in
  `wyrd-sql/src/queries/platform/signup.rs`, plus
  `provisioning.rs::claim_signup_tenant`, which adopts only a `failed` or
  stale claim that names the same attempt.
- Reuse: `TenantProvisioning` keeps one owner. `claim_for_signup` plus
  `provision_for_signup` share `complete`, `retire`,
  `establish_and_promote`, `grant_admin`, the builtin-role/SYSTEM seeding,
  `builtins.ensure_tenant` and `mark_tenant_active`/`failed` with the operator
  flow. The human owner is a `User` and an `oidc` identity bound to `admin`
  at tenant scope. No service account or key is created, and no platform
  caller is fabricated. The two planes stay separate transactions: claim on
  the platform pool, initialization in one tenant transaction, then
  promotion.
- Public surface: `POST /auth/signup` (begin), the shared social callback
  dispatching a signup state to `SocialSignIn::prove_signup` (303 to
  `/signup/workspace` with an HttpOnly `wyrd_signup` cookie on
  `/auth/signup`), and `POST /auth/signup/complete`. Completion settles the
  attempt before opening the session, so a lost response is recovered by
  returning login and never by re-creation. There is a new stable code,
  `WYRD_AUTH_429_SIGNUP_ATTEMPTS`; its TS error code is regenerated.
- Test `platform_admin_e2e::a_visitor_creates_a_workspace_with_google`
  covers steps 1–10:
  1. signup off refuses begin with `WYRD_AUTH_401_INVALID_TOKEN`;
  2. an invalid slug returns 400 and an existing slug returns 409, and the
     existing tenant is never adopted;
  3. `acme` becomes active with 1 User, 1 identity, 1 user `admin` binding,
     0 API keys, 0 non-SYSTEM service accounts, builtin roles and SYSTEM.
     The body is exactly `{"tenant":"acme"}` and no platform principal is
     added. The session invites a teammate (201);
  4. a replay returns 409 naming `acme`, opens no session and creates no other
     slug; returning Google sign-in reaches `/t/acme`;
  5. one proof raced on two replicas returns one 200 and one 409, with one
     active tenant;
  6. a staged interrupted claim, made by SQL fault injection with a `failed`
     row and attempt, is refused (409) to another identity and stays
     `failed`. Its own attempt resumes it under the same tenant id to
     `active`;
  7. the same Google account creates `acme-two` as a second, distinct User
     and gains no identity in the fixture tenant;
  8. turning signup off, or disabling Google after proof, refuses
     completion (401) and creates no tenant;
  9. after 4×5 completions from one source, the identity window refuses a
     6th from another source and the source window refuses a 21st identity
     (429, `retry_after_seconds` > 0), while another source is admitted;
  10. with admission state unavailable (table renamed) completion returns
      503 and creates no tenant, then succeeds once the state returns.
- RED: the selector failed at signup begin with
  `401 WYRD_AUTH_401_UNAUTHENTICATED` (no `/auth/signup` route). The
  migration, SQL queries and `SocialSignIn` signup methods had been written
  before this RED, which is a TDD process violation for those layers. The
  provisioning human-owner path and routes followed the RED.
- Diagnosis (step 9): **Symptom:** a 409 was expected inside the flood loop
  but a 429 arrived. **Evidence:** `WyrdTestServer::raw_call`
  (`wyrd-testing/src/server.rs`) inserted `ConnectInfo(127.0.0.1:0)` on every
  request, overwriting the test's peer. **Cause:** every completion shared
  one source window. **Fix site:** `raw_call` now inserts loopback only when
  the request names no peer. A read-only codex diagnostician confirmed the
  cause and fix site and checked the callers (`oneshot`,
  `oneshot_authenticated`, `browser_json`, `token`). Requests without a peer
  are unchanged.
- GREEN: the same selector passed (3.5 s). Lanes: `fmt`, `lints`,
  `codegen:regen`/`codegen:check`, `check:deps`, `check:tenant-isolation`,
  `git diff --check`. wyrd-server integration `platform_admin_e2e::*`,
  `pg_browser_sessions::*` and `provision*` passed 56/56. `wyrd-auth`,
  `wyrd-auth-oidc` and `wyrd-sql` lib passed 279/279.
- The GitHub token decoder now reads `access_token` as `String` and wraps it
  immediately. It had compiled only when another crate enabled `secrecy`'s
  serde feature.
- **Transferred:** the UI signup page and journey, and the docs → [TASK-003-carryover cycle 14](TASK-003-carryover.md#14-approved-mock-precedes-new-pages-and-remaining-cycle-6-ui-delivery).

### 7. An existing User explicitly connects another sign-in method

**Transferred; do not execute here:** [TASK-003-carryover cycle 15](TASK-003-carryover.md). Intent below is retained as history.

**Maps to:** REQ-012, REQ-015; AC-006.

Start with a password User holding direct grants, local memberships and keys.
From account settings, freshly prove its allowed current method within five
minutes, then Google, GitHub or company SSO in the bound linking transaction.
Sign in through the added method and use the same scoped resources/key: principal
ID, existing grants, groups, keys and attribution remain. Company linking updates
only that connection's verified group assignments. Repeat same-User attachment
without creating a duplicate row.

A stale cookie, recovery-only proof, swapped User/tenant/browser/purpose, expired
proof or identity belonging to another User refuses attachment without moving
identities/grants. Concurrent claims have one winner. Existing duplicate Users
remain separate. From a social-only User, freshly prove the current method and
add a validated unique username/password, then log in with it. Matching email or
a reset request alone cannot add that password.

**Proof:** Tier-1 production account UI and Rust HTTP → existing User → actual
resource/key use; integration assertions for uniqueness races. Initially fail on
absent linking/password-add flow. Reuse current credential/grant consumers to
observe preservation, not just identity row counts.

### 8. Removing or disabling a method invalidates its original credentials

**Transferred; do not execute here:** [TASK-003-carryover cycle 16](TASK-003-carryover.md). Intent below is retained as history.

**Maps to:** REQ-011, REQ-012, REQ-015; preserves REQ-002; AC-006.

Give the linked User browser sessions and explicitly approved CLI refresh families
from different allowed methods. Freshly prove a remaining method and remove one
identity. Its anchored session and renewal refuse; the other method and its
credentials remain usable. Last-usable-method removal and removal without fresh
remaining-method proof refuse. Preserve last-usable-admin safeguards.

Then disable/replace a shared provider, or disable password, through operator
management while affected callbacks and sessions exist. Across replicas, affected
completion/use/renewal refuses under the established rules, even if an affected
User has no remaining method. No automatic method substitution or grant appears;
operator recovery and existing personal/Service API-key behavior remain intact.

**Proof:** Tier-1 browser and real CLI/SDK device/renewal consumers, backed by
Postgres. Initially fail on missing social identity anchors and availability
enforcement. Observe both selected invalidation and surviving credentials.

### 9. A tenant adopts company SSO without a password or social bypass

**Transferred; do not execute here:** [TASK-003-carryover cycle 17](TASK-003-carryover.md). Intent below is retained as history.

**Maps to:** REQ-016; preserves REQ-002/REQ-004/REQ-005; AC-006.

With linked Users and some unlinked accounts, a tenant admin freshly proves the
tested Active company connection, sees the unlinked-account warning and enables
require-company-SSO. Enablement without that proof or usable tested connection
refuses. Allowed-method UI changes and direct password/social starts, racing
callbacks and invitation enrollment cannot bypass policy. Existing password/social
browser sessions and CLI renewal become unusable under established freshness;
company sign-in and policy-compliant invitations still work.

Replace/deactivate the company connection or interrupt it: policy remains enabled
and login refuses without password/social fallback. Preserve company revision
and issuer/subject ownership rules. Disabling policy does not resurrect revoked
sessions. Repeat through another replica; tenant policy never affects another
tenant or platform login. Exercise existing API-key/recovery paths unchanged.
Do not assert immediate revocation from upstream IdP removal alone.

**Proof:** Tier-1 tenant settings/login and device renewal journeys, including
production UI, against real policy persistence. Initially fail on the absent
policy and its completion/session/renewal checks; hidden buttons alone do not pass.

### 10. Delivered surfaces and real providers complete the same journey

**Transferred; do not execute here:** [TASK-003-carryover cycle 18](TASK-003-carryover.md). Intent below is retained as history.

**Maps to:** AC-004–AC-006 and the spec's shipped-surface safeguards.

Using the operator setup, sent invitation, tenant User and grants produced above,
complete production UI login, explicit CLI device-code approval and subsequent
real Rust/Python/TypeScript client resource access. Exercise shipped HTTP/MCP
management/read boundaries through their own typed consumers and denial behavior;
do not make cookie authentication available on additional protocols. Verify
generated/exported contracts match the Rust owners and Node retains no exchange
or credential authority. Preserve scoped permissions/ceilings, local/company
login, recovery and authorization regression cases at touched boundaries.

Perform real Google and GitHub operator smoke tests for registration instructions,
consent and callback compatibility. Record the configured flow and observed
result without secrets; controlled-provider tests do not establish this external
compatibility. No hosted SMTP provider test is required. Read updated operator
and user instructions against the actual delivered actions and refusal messages.

**Proof:** Tier-1 production UI/CLI/SDK/MCP journeys using their existing runners,
plus live-provider smoke and contract checks. This connects earlier cycle outputs
to actual consumers; disconnected fixtures or build success alone cannot pass.
Record missing live-provider access as a material verification limitation rather
than claiming those smoke tests ran.

## Verification owners and handoff

Remaining execution and verification belong to [TASK-003-carryover](TASK-003-carryover.md); the original recipes below are retained for evidence continuity.

Extend the existing `wyrd-server` integration target for platform management,
browser/password and identity journeys (`platform_admin_e2e.rs`,
`pg_browser_sessions.rs`, `identity_e2e.rs`, `identity_ui_e2e.rs`). Keep controlled
provider/SMTP fixtures local to those owners, using `WyrdTestServer` and repository
Postgres; no hosted Google/GitHub credentials are needed for deterministic tests.
Reuse `wyrd-auth` inline tests and `wyrd-sql` integration tests for pure validation,
RLS, atomicity and shared coordination. Extend the installed CLI and saved-login
Rust/Python/TypeScript journey owners selected by `test:identity:journey` for
their delivered behavior, and existing MCP journeys for projections it ships.

Existing focused module runners, once repository Postgres is provisioned by
the owning lifecycle, are:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_browser_sessions::/)'
mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^platform_admin_e2e::/)'
```

Record exact new selectors and required setup in each cycle before RED. Use the
existing `test:principals:integration`, `test:platform:journey`,
`test:identity:ui` and `test:identity:journey` lifecycle/setup as applicable;
do not pretend the existing Keycloak/Dex scripts already supply social or SMTP
fixtures. Add the smallest in-owner support for those controlled dependencies.
Integrated change review owns full shipped-surface journey lanes; this task
records focused delivered-flow proof and its affected regressions.

Run `mise run db:migrate` for new migration/idempotence proof and focused SQL
RLS/coordination assertions, `mise run check:tenant-isolation`,
`mise run check:deps`, `mise run codegen:check`, `mise run ui:check`,
`mise run fmt` and `mise run lints` for affected boundaries. Apply the language
owner's format/lint/type checks to changed SDK projections and `docs:check` to
changed documentation. Do not run the broad `gate` merely for task handoff.

If enforcement touches a defined verification/gateway/Bifrost hot path, enumerate
every added/moved PostgreSQL, Scribe, object-storage or Vault wait and match each
to a sanctioned wait. Review PASS additionally requires a full, clean
`mise run bench:l1:<engine>` run with verdict VALID, compared with the newest
same-experiment, same-host record under `benchmarks/history/`. Smoke, `--steps`
and `--allow-dirty` runs do not satisfy that condition. Authentication/control
plane changes alone do not make every such benchmark applicable. Never block
an operation on audit delivery or add per-request hot-path client construction.

No product or storage decision is open. Implementation chooses private names,
helpers, representations and fixture seams within the approved owners. A new
material contract/persistence purpose returns `SPEC_REVISION_REQUIRED`; missing
external smoke credentials are evidence limitations, not a reason to redesign.
This proposal contains no implementation or execution result, and does not
authorize staging, committing or pushing.

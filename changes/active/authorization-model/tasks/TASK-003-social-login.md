---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 3
requirements: [REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017]
depends_on: [TASK-001, TASK-002]
---

# Social sign-in, workspace signup and team invitations

An operator enables Google and GitHub sign-in and configures application email.
A tenant administrator invites a team with scoped access. Each recipient proves
their identity and invited address, then enters that tenant with the intended
access. A public visitor may instead create a new workspace, when enabled, and
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
| Existing identity store and `wyrd-sql/src/queries/auth/local_passwords.rs` | Multiple identities per User already exist. Add explicit attachment/removal, addressed invitation coordination and recipient proof without email-based User resolution. Extend existing scoped consumption and issuer-authority checks. |
| `wyrd-server/src/components/platform/provisioning.rs::TenantProvisioning` and SQL platform provisioning | Extend its directory claim, builtin/SYSTEM/table initialization and promotion/failure workflow with human-owner initialization. The existing privileged flow returns a bootstrap credential; public signup instead initializes its proven User/admin binding and returns an ordinary tenant session. Do not duplicate orchestration or fabricate a platform caller. |
| UI login/signup, Settings → Access and account routes; `src/lib/server/auth/` | Render Rust's allowed methods, addressed bulk invitation results, account linking and SSO policy. Node forwards browser context; Rust owns exchange, proof, secrets and durable behavior. |

Operator clients/settings, tenant administrator recipient/access selections,
provider responses and the browser-bound signup/linking attempts are the real
inputs. Persist approved coordination metadata through `wyrd-sql`; return safe
management results, per-recipient outcomes and ordinary tenant credentials to
the UI and existing CLI/SDK consumers. SMTP is the one new delivery capability:
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

### 3. An administrator emails separate invitations to a team

**Maps to:** REQ-013, REQ-017; preserves REQ-001/REQ-008; AC-004.

The operator configures the local SMTP test server through generic host/port/TLS,
optional credentials and sender settings, then performs test-send. A tenant admin
pastes at least ten recipient addresses into Access and chooses the predecessor's
explicit role/scope assignments. The UI/API returns separate invitation and send
results; the SMTP server receives separate correctly addressed messages tied to
the committed invitations. Pending entries can be inspected, resent and revoked.
The tenant admin cannot read/change relay credentials, and reads omit secrets.

With missing SMTP, the UI explains emailed invitation unavailability. With relay
failure, report failure rather than delivery, retain pending state, create no
active User and allow explicit resend after recovery. Relay acceptance is shown
as acceptance, never inbox delivery. A stalled SMTP exchange must not hold the
tenant transaction; no raw code is stored as a mail spool or logged.

**Proof:** Tier-1 management/UI → Rust → Postgres → controlled SMTP journey;
SQL integration proof for coordination and transaction boundaries as needed.
Initially fail on absent recipient/bulk/delivery contracts. Consume actual sent
messages in the next cycle rather than constructing unrelated invitation rows.

### 4. The addressed recipient redeems scoped access, then returns with Google

**Maps to:** REQ-012, REQ-013; preserves REQ-001/REQ-008; AC-004.

Use a message produced by cycle 3. Its recipient completes Google enrollment
with appropriate verified recipient email, receives one tenant-local User,
identity, exactly the invited assignments and session, and consumes the invitation
atomically. Use that session against an allowed resource and a second resource
outside the invited scope. Log out and return with the same tenant/issuer/subject:
the same User and access return. Social sign-in must leave direct, local-group
and company-derived assignments intact; no provider groups are needed.

Also redeem through invited password enrollment, and use a separate mailbox
challenge when provider proof cannot establish the recipient. A forwarded
invitation alone, wrong/unverified recipient, expired/replayed/wrong-browser
mailbox proof, invalid/expired/revoked invitation, changed issuer assignment
authority or invalid role/scope cannot grant access. An already-owned provider
identity refuses new-User enrollment with sign-in/access-request guidance.
Concurrent redemption has one winner; failed consumption leaves none of the
User/identity/bindings/session writes committed. Inactive Users/tenants cannot
return. Reuse one provider identity in a second tenant and observe distinct
Users/grants/credentials, with no cross-tenant credential use.

**Proof:** Tier-1 invitation email → enrollment → real authenticated resource
calls, including production UI; SQL integration fault/concurrency assertions
support atomicity. Initially fail on missing addressed social/password proof and
social returning login. Matching email alone must never resolve an existing User.

### 5. GitHub follows the same admission flow using its stable account ID

**Maps to:** REQ-011, REQ-012, REQ-013; AC-004.

The operator registers/tests/enables GitHub through the same management journey.
Invite a recipient and complete code/S256-PKCE/state exchange, authenticated user
lookup and, when needed, verified mailbox lookup through controlled GitHub
responses. Redeem and return through real endpoints. Rename the GitHub login
while retaining its numeric ID: Wyrd returns the same User. Another numeric ID
with a matching email/login does not inherit it. Repeat the common invitation,
purpose, revision and stranger refusals through this adapter. Provider failure
must not create a User or fall back to another method.

**Proof:** Tier-1 OAuth → invitation → session → authorized resource journey.
Initially fail on the absent OAuth identity adapter. Inspect requested scopes
and transient-token handling: no repository/organization access, fabricated ID
token, retained provider token or social role mapping. Reuse common flow proof
where it exercises both adapters; do not substitute Google-only proof for GitHub.

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

**Proof:** Tier-1 public UI/API → existing provisioner → tenant administration,
with real Postgres failure/retry/concurrency proof. Initially fail on missing
human-owner provisioning and public admission. Prove reuse of the existing
directory lifecycle without claiming a cross-plane atomic transaction.

### 7. An existing User explicitly connects another sign-in method

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

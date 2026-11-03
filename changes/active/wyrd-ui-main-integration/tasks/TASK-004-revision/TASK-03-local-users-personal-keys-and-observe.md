---
id: TASK-004-REV-03
kind: implementation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 19
requirements: [REQ-007, REQ-015, REQ-016, REQ-017, REQ-020, REQ-021, REQ-030, REQ-033, REQ-034, REQ-038, REQ-039, REQ-041, REQ-042, REQ-043, REQ-044, AC-024, AC-025, AC-026, AC-027, INV-001, INV-002, INV-004, INV-007]
depends_on: [TASK-003, TASK-027, TASK-004-REV-01]
parent_task: TASK-004
remediates: []
---

# Task 03 — Real local users, personal keys and an aligned Observe workflow

## Outcome and Value

A developer runs `mise run dev:full`, receives a real local username/password
and usable UI URL before the workload finishes, logs in without an external
identity provider, and investigates actual retained evidence in the approved
Traces, Logs, Metrics and waterfall UI. Local and SSO users have ordinary account
Settings where they can create a purpose-scoped API key once, use it with a
first-class SDK and revoke it. Password authentication and keys are Rust-owned
capabilities, not frontend-only behavior or a development fixture shortcut.

**Ready for implementation.** The caller explicitly approved specification
revision 19 and this task's contracts on 2026-11-02. The
approved admission model is protected workspace provisioning followed by
initial-owner enrollment and later invitations. Self-hosted setup normally
creates one tenant; SaaS onboarding provisions a distinct tenant for each team
through the existing platform owner. Both use the same local/SSO User and
session implementation. This approval covers the admission/security/persisted
contracts, typed public contracts and palette; no anonymous
membership/admin access or public platform provisioner is implied.

The current baseline is `010ef9bab1932b299b8d72aae8f05fcfd7a3e15f`, including
R2 source and default 500. Preserve that work and prior proven behavior. This
is the new requested delivery scope, not a restart or an instruction to recreate
all original Observe work. Existing Task 02/R1/R2 and immutable review reports
remain acceptance context; this task owns the new local-human/own-key workflow
and the specific remaining audit gaps. Under approved revision 19, Task 02's
unfulfilled delivery/visual obligations transfer here; Task 02 PASS is not a
prerequisite for the very work that closes those obligations. No historical PASS or screenshot grants
current visual acceptance.

## What Changes and What Must Stay True

### Local signup, identity and passwords

The approved admission model separates workspace provisioning from enrollment.
Reuse TenantProvisioning through the protected platform API: self-hosted operator
setup usually provisions one tenant; SaaS onboarding provisions a tenant per
team. The authorized bootstrap establishes initial-owner enrollment; subsequent
administrators create 24-hour, revocable, single-use invitations through the
existing principal/human administration owner. No deployment-specific auth fork
or Development/Production profile switch decides membership. Permission is `users:write`;
assigning roles additionally obeys the existing role-grant authority. Invitation
records bind tenant, issuer and selected existing Role IDs. Signup accepts an
enrollment code, username and password; the server determines tenant/roles
from that authority, revalidates issuer/tenant/role eligibility and atomically
consumes the invite, creates the User/credential/grants and creates a fresh
browser session. Invalid, expired, revoked, reused or concurrently consumed
invitations never leave a partial User or grant. Public anonymous SaaS workspace creation is separate platform onboarding and
is not implemented in this task. It requires its own registration authority and
verified-address channel; do not fabricate a PlatformCaller or add a public
wrapper around platform-admin APIs.

Fix the local username contract: tenant-local canonical ASCII handle, 3–64
characters, first alphanumeric, remainder alphanumeric/dot/underscore/hyphen;
trim surrounding whitespace and lowercase. Store the canonical form under a
tenant-local unique constraint for local users. Preserve immutable User ID;
username rename is not included. Add the local identifier without globally
restoring email uniqueness or changing issuer/subject SSO identity. Existing
OIDC users do not acquire a password because a username/email happens to match.

Passwords are 15–128 Unicode characters after consistent NFC normalization,
with a 1024-byte raw-input ceiling before costly processing. Preserve password case/spaces and reject control characters. Check the full password against a local common-password
blocklist; no composition rule, security question, scheduled rotation or
remote breach lookup on the critical path. Supply password-manager autocomplete,
paste, a visible policy explanation and an accessible reveal control.

Reuse workspace Argon2 0.5/password-hash 0.5 at the owning auth crate. Store one
encoded Argon2id PHC verifier in the existing password_hash owner, fresh random
salt, baseline memory 19456 KiB/iterations 2/parallelism 1. Use library verification
and rehash-on-success for obsolete policy. Existing encryption derive_key is not
a login verifier. Declare the already-resolved Unicode normalizer at its narrow
owner if used. No plaintext/reversible password store, new vault or auth framework.

Bound password work and guessing: at most four KDF operations per process;
tenant/normalized-name failure window 10 attempts per 15 minutes and trusted
source window 100 per 15 minutes. Apply these limits to every password verification path, including password
change and a new local login for key creation. Use shared Postgres-backed auth state,
atomic updates and statement_timestamp-derived windows; a process-local map
must not bypass limits across replicas. Fixed window end, no permanent
attacker-triggered lock, no raw/untrusted forwarded source identity. Unknown,
wrong, suspended or missing-local-credential failures use the same public
credential refusal and comparable verification path; rate refusal is stable
429 with retry information. Expire bounded attempt state through the existing
auth maintenance lifecycle. Validate sizes/rate admission before KDF allocation.

### Local sessions, recovery and SSO coexistence

Extend the existing typed `/auth/browser` owner with password signup/login/
change/reset operations; keep the existing SSO-begin operation and recovery-key
operation. The packet-local Public Contracts section below fixes the selected
wire shapes alongside spec revision 19. New types are pure wyrd-spec with scrubbed secret fields and stable
Wyrd errors. Server-side BFF forms forward to that owner;
Node never verifies passwords, owns a password database or creates user grants.

Use the existing User row and BrowserSessions workflow. A local password has a
monotonically increasing credential revision. A forward migration extends the
existing session authority constraint to exactly one of local User/revision,
OIDC connection/revision, or recovery-key ID. Do not create a second session
store. Session validation checks current User/tenant status and that revision.
Keep opaque host-only HttpOnly cookies, production Secure/SameSite=Lax, exact
Origin validation, selected-tenant and explicit-bearer precedence, no-store,
fixed 12-hour expiry and normal logout/revocation across replicas.

Authenticated password change verifies the current password. Initial forgotten-
password recovery is operator-assisted: users:write plus existing tenant-admin
grant authority creates a 15-minute, single-use, purpose/User/tenant-bound reset code; the user enters it with their
new password. Tokens use 256-bit randomness and verifier-only storage. Codes
are delivered separately from fixed tenant signup/reset URLs, never in URLs or
logs. Reset does not expose/set the user's chosen password to the administrator,
does not automatically sign in, and invalidates reset codes and all local
password sessions by revision. No email delivery/verification workflow is
promised without an approved verified-email channel. UI explains how to contact
the operator instead of offering a nonfunctional email-reset form.

Local and SSO users remain real User principals with the same role and session
owners. SSO-only accounts show provider-managed authentication in Settings and
cannot add/change a password through this slice. Do not auto-link by email or
username, enable OAuth grant_type=password, change device/refresh grants,
weaken provider checks, or extend cookie authentication to MCP/gRPC/OTLP.
Suspension/revocation uses existing User authority; no deletion of evidence
identity. Password change/reset leaves independently owned API keys intact;
explicit revoke/User suspension controls programmatic access.

### Personal Settings and user-owned API keys

Provide `/t/[tenantKey]/settings/account` for ordinary signed-in Users. It shows
safe identity/authentication metadata, local password-change when appropriate,
and own-key metadata/create/revoke. Existing tenant SSO administration remains
separate and permission-gated; lack of identity_connections:write must not deny
personal Settings. Point ordinary account navigation to the personal page.

Extend the existing principal owner with server-derived self identity, rather
than granting service_accounts:write to every User or trusting a submitted
principal ID. Use GET `/v1/principals/me` for safe User profile and extend the existing
GET/POST `/v1/principals/{principal_id}/credentials` plus DELETE
`/v1/principals/{principal_id}/credentials/{credential_id}` with the explicit
real-User self case. UI derives the target from its Rust session; the server
compares it with verified caller ownership. Reuse existing clients and MCP
metadata/revoke tools, rather than adding /me/credentials aliases. Preserve
existing administrator-targeted credential behavior; other-User issuance
requires tenant-admin grant authority. The approved explicit capabilities personal_credentials:read/write are
self-only for real User principals. Add them at the existing permission/authority
owner before applying credential restrictions; Service principals gain neither.
Read lists own metadata; write permits own revoke and, with recent human proof,
creation. Enforce capability plus derived ownership through the configured
permission/principal owner, not an undocumented check bypass. A restricted
User key needs the corresponding ceiling to list/revoke; an ordinary User
session has those self capabilities even without principal administration.

Create request: name (trimmed nonempty, 1–64 characters), a nonempty list of
existing parsed permission expressions as its ceiling, and explicit positive
expires_in seconds up to 365 days. UI preselects 30 days, shows expiry and starts
with no administrative/all-permissions checkbox selected. Server validates
scope against both caller effective authority and current User grants. Store
normalized name, permission ceiling and expiry with the existing credential
metadata. On each exchange intersect the ceiling with current User grants;
new roles never expand an old key beyond its ceiling. Keep existing short-lived
tenant JWT snapshot behavior: revoke/expiry blocks new exchanges, already-issued
JWTs expire normally. Do not introduce per-request remote IdP introspection.

Personal creation requires a recent human password/SSO login, session age at
most five minutes; older sessions repeat their normal login. API-key recovery
alone is not recent human proof and cannot mint a new master credential chain.
Freshness is Rust session creation after password verification or normal bound
SSO completion; an existing IdP session may satisfy it. Reusing a Wyrd cookie
or API key does not refresh it. Do not add forced provider password entry or a
new auth_time/step-up profile. This protects creation with recent Wyrd login
without asking an SSO-only person to invent a local password. Metadata/revoke remain available under normal valid
User authority, bounded by credential restrictions. This is not MFA assurance.

Reuse existing 256-bit key generation, prefix, SecretBearer and SHA-256 verifier
storage. Show the create-response secret once in a no-store action result with
copy/dismiss. This explicitly approved transient POST response is the only
exception to the no-browser-secret rule: never initial SSR/PageData, history,
local/session storage, URLs, logs/traces/analytics or screenshots. Dismiss or
navigation discards it; later list/reload cannot recover it. Metadata contains
name, prefix, ceiling, created/expiry/last-use and revocation state, with bounded
pages (default 50, maximum 100, opaque continuation cursor); revoke is
owner-constrained and clearly reports success. No second key format or secret
store, and no password KDF applied to random API keys.

The backend extension is the complete existing chain, not just a UI/route check:

1. Forward-migrate the existing key table so a credential has exactly one
   tenant-bound relational owner in existing auth_users or existing
   auth_service_accounts. Preserve every old ID/verifier/prefix/expiry and real
   service/admin owner; no surrogate Service, parallel credential table or
   weakened foreign-key integrity.
2. Extend existing insert/lookup/verified-credential types to retain that owner
   kind. Current SQL joins/FK only service accounts and must actually support
   Users; route-level require_principal alone is insufficient.
3. Extend ExchangeApiKey/TenantTokenIssuer's one resolution/grant workflow to
   resolve the verified User, current roles/status, stored ceiling and credential
   provenance. Never infer all API-key grants are Services or copy delegation
   as a second identity resolver. Keep admin/workload keys and restriction
   behavior unchanged.
4. Project safe own-profile/list/revoke and key exchange/use through wyrd-client
   and thin Rust/Python/TypeScript clients. Secret creation uses the interactive
   browser-auth HTTP protocol; do not add a password cache or cookie/session
   authority to SDKs. Existing administrator creation stays available through
   its existing client path. MCP remains metadata/revoke only—no created key
   secret in model transcripts. Preserve its required mutation scopes.

This self-service contract and scope metadata are new, approved revision-19
public/persisted behavior. Implementer cannot silently choose a weaker default
or treat an SSO User as a Service just to make a key exchange pass.

### Public contracts

These normative field tables close readiness finding PRR-001. They select the
public wire contract, not private Rust layout. Define shared types in wyrd-spec;
HTTP/OpenAPI, MCP schemas and SDK projections must agree. Objects below reject
unknown request fields. All fields are required unless marked `?`; optional
request fields are omitted, not null. `T | null` means a required nullable field.
UUIDs and existing PrincipalId/DataTenantId/TenantSlug use their existing string
encodings. Timestamps are RFC 3339 UTC strings. Secret fields are JSON strings
with redacted in-memory handling; they never appear in error details.

#### Password, enrollment and reset

| Shared type | JSON fields and constraints |
|---|---|
| PasswordSignupRequest | `tenant: TenantSlug`, `invitation_code: secret string`, `username: string`, `password: secret string`; username/password policies above; no principal, email or role input |
| PasswordLoginRequest | `tenant: TenantSlug`, `username: string`, `password: secret string` |
| PasswordChangeRequest | `tenant: TenantSlug`, `current_password: secret string`, `new_password: secret string` |
| PasswordResetRequest | `tenant: TenantSlug`, `reset_code: secret string`, `new_password: secret string` |
| CreateUserInvitationRequest | `role_ids: UUID[]`; nonempty, distinct existing tenant Role IDs, all authorized for the issuer to grant; fixed 24-hour lifetime, no caller-selected tenant/issuer/expiry |
| UserInvitationView | `id: UUID`, `tenant: TenantSlug`, `role_ids: UUID[]`, `created_at: timestamp`, `expires_at: timestamp`; Role IDs ordered by UUID |
| IssuedUserInvitation | `invitation: UserInvitationView`, `invitation_code: secret string`, `signup_url: AbsoluteUrl`; the fixed tenant signup URL contains no code |
| UserPasswordResetView | `id: UUID`, `tenant: TenantSlug`, `principal_id: PrincipalId`, `created_at: timestamp`, `expires_at: timestamp`; fixed 15-minute lifetime |
| IssuedUserPasswordReset | `reset: UserPasswordResetView`, `reset_code: secret string`, `reset_url: AbsoluteUrl`; the fixed tenant reset URL contains no code |

Invitation/reset codes encode the already-selected 256 random bits as unpadded
base64url (43 characters); malformed codes refuse before expensive processing.
An invitation response returns the code once to its authorized issuer, and a
reset response once to the authorized operator. Both are no-store responses;
no read/list operation recovers codes. Neither is an MCP tool result. The
existing CLI setup consumes the same enrollment owner under operator authority,
without adding a public provisioning endpoint.

| HTTP operation | Request / authority | Success |
|---|---|---|
| POST `/auth/browser/password/signup` | PasswordSignupRequest; valid enrollment authority | 201 BrowserSessionView and existing opaque session cookie |
| POST `/auth/browser/password/login` | PasswordLoginRequest | 200 BrowserSessionView and fresh cookie |
| POST `/auth/browser/password/change` | PasswordChangeRequest; selected local User session | 204 empty body; invalidate local revision sessions and clear selected cookie |
| POST `/auth/browser/password/reset` | PasswordResetRequest; valid reset authority | 204 empty body; invalidate reset/local sessions, no login |
| POST `/v1/principals/users/invitations` | CreateUserInvitationRequest; users:write plus existing role-grant authority | 201 IssuedUserInvitation |
| DELETE `/v1/principals/users/invitations/{id}` | UUID path; tenant-bound administration, no body | 204 empty body |
| POST `/v1/principals/users/{id}/password-reset` | PrincipalId path; users:write plus tenant-admin grant authority, no body | 201 IssuedUserPasswordReset |

BrowserSessionView retains every existing field: `tenant`, `tenant_id`,
`tenant_name`, `principal_id`, `principal_kind`, `credential`, `permissions`,
`expires_at`. Extend BrowserCredential only with wire value `password`; retain
`sso` and `api_key`. Successful signup/login return kind `user`, credential
`password`, current effective permission strings and the fixed session expiry.
The response contains no cookie identifier, bearer or password. Existing SSO
and recovery request/result shapes remain unchanged. BFF forms submit these
typed requests server-side and redirect after success; no native SDK password
login/session authority is introduced.

#### Safe profile and named User credentials

| Shared type | JSON fields and constraints |
|---|---|
| CurrentUserProfile | `tenant: TenantSlug`, `tenant_id: DataTenantId`, `principal_id: PrincipalId`, `principal_kind: "user"`, `username: string | null`, `authentication: "password" | "sso"`, `permissions: string[]` |
| CreateUserCredentialRequest | `name: string`, `permission_ceiling: string[]`, `expires_in: integer`; normalized name 1–64 characters, nonempty distinct existing permission expressions, lifetime 1–31536000 seconds; no null/default/unrestricted User form |
| IssuedCredential (existing) | Unchanged: `id: UUID`, `credential: SecretBearer` string, returned once; metadata is read through the list operation |
| CredentialMetadata (extended) | Existing required `id: UUID`, `prefix: string`, `created_at: timestamp`, and nullable `expires_at`, `revoked_at`, `last_used_at`; add `name?: string`, `permission_ceiling?: string[]` |
| CredentialListQuery (new) | `limit?: integer` in 1–100, `cursor?: string` (nonempty, at most 2048 bytes); no filters; query parameters have the same names as these fields |
| CredentialListResponse (extended) | `credentials: CredentialMetadata[]`, `next_cursor?: string`; omit next_cursor at end, never null; never a credential secret |
| ListCredentialsArgs (extended MCP input) | `principal_id: PrincipalId`, `limit?: integer`, `cursor?: string`; same bounds and page semantics as HTTP |

GET `/v1/principals/me` returns 200 CurrentUserProfile derived from verified
caller identity and current User state. Username is canonical and nonnull for
local accounts, null for SSO-only accounts. Authentication describes the User's
account method, not whether this request carried a key. Permissions are sorted,
distinct, caller-effective strings including applicable self capabilities after
credential restrictions; they are informational, never authorization input.
There is no email, password verifier, provider subject, session identifier or
key secret in this projection. A non-User caller is refused, not converted into
a human profile. Existing suspension/tenant checks still apply.

POST `/v1/principals/{principal_id}/credentials` returns 200 IssuedCredential,
preserving the existing status and shape. A User target requires
CreateUserCredentialRequest for both self and authorized administrator issuance;
self creation additionally requires the recent human session above. Reject an
absent body for User targets. Service/Admin targets retain the existing no-body
issuance, expiry and authority behavior; reject the new User request body for
those targets instead of silently ignoring scope fields. Name and
permission_ceiling are always present for newly issued User keys, ceiling is
stored/returned in canonical sorted order, and expires_at is nonnull. These
additions are omitted for existing Service/Admin and platform credentials;
absence means their unchanged principal-bound authority, not an unrestricted
User key. Preserve existing nullable timestamp fields and metadata values.
Do not change platform credential requests, statuses or response fields merely
because platform handlers share CredentialMetadata/IssuedCredential types.

DELETE `/v1/principals/{principal_id}/credentials/{credential_id}` keeps its
existing 204 empty HTTP response. MCP keeps RevokeCredentialArgs and
CredentialRevoked `{revoked: true, credential_id: UUID}` unchanged. Ownership,
permission, selected-tenant and existing stable-error boundaries apply on every
operation. All new invalid wire values use the existing validation problem
contract; no success-shaped error or alternate error envelope is introduced.

#### Pagination and client compatibility

Paging is an explicit mode on the existing list route, selected by supplying
`limit` or `cursor`. A cursor alone uses limit 50. The new page client operation
always sends a limit (default 50, maximum 100), requests **one page**, and returns
CredentialListResponse including its continuation. It never traverses secretly.
Clients request subsequent pages with that cursor and the same principal.
An empty page has `credentials: []` and no next_cursor; the final nonempty page
also has no next_cursor. Never emit a cursor when no more rows remain.

Preserve the existing no-query administrator list as a complete, newest-first
listing with `{credentials: [...]}` and no next_cursor. Existing administrator
HTTP calls, `list_credentials(principal_id)` and MCP calls containing only
principal_id keep that behavior and their existing authorization. An ordinary
self-service caller must use page mode; an unpaged self-only call fails with
the existing validation error explaining the required limit/cursor, rather
than silently truncating at 50. Settings always uses page mode, even for an
administrator viewing their own keys. The existing no-body issue client remains
valid for Service/Admin targets; it refuses User targets explicitly.

Pages include revoked and expired credentials, as the existing list does, and
order by created_at descending with credential UUID descending as a stable
tie-break. A cursor resumes strictly after the last returned row and binds the
tenant and target principal; malformed or mismatched cursors fail validation.
Recheck current authorization/ownership on every page; a cursor grants no
authority. There is no frozen snapshot across pages: newly created keys appear
on a fresh first page, while revocations update metadata. Cursor encoding and
private query helpers remain implementation-owned; no cursor service is needed.

Rust Principals gains `me()` and
`list_credentials_page(principal_id, CredentialListQuery)`; Python and TypeScript
project those same operations/types using their established naming conventions.
Omitted page limit is normalized to 50 by the page operation before transport.
The old list method keeps its full-list administrator semantics. No SDK creates
human-session authority to call interactive personal issuance. Existing
`principals.list_credentials` MCP adds the optional limit/cursor fields and the
same response continuation; existing `principals.revoke_credential` stays
unchanged. Add read-only `principals.me` with strict empty-object input and
CurrentUserProfile output. Do not expose password, invitation/reset secret or
personal key creation tools. Read discovery and mutation scopes retain the
existing MCP policy. All language/generated-schema projections originate at
the shared types rather than independently handwritten response shapes.

### One real seeded development workflow

Extend existing `wyrd-server setup` with optional `--local-user <username>` and
`--local-password-stdin` for the legitimate operator-controlled initial human
bootstrap. Require both together, read the secret only from stdin and preserve
normal setup when absent. Use the same User creation/hashing/grant owner as
signup; refuse to overwrite an existing local credential through setup.
Extend the existing dev-server/process owners. Seed a real local User
`dev` through the same creation/hashing owner, with a new random 20-character
password per ephemeral lifecycle and existing tenant-admin grants established
by authorized bootstrap. No direct test-table insert, fixture session or
committed default password. Keep a private 0600 dev credential record under
existing config ownership for the lifecycle/browser test, removed on owned
cleanup and excluded from review/CI artifacts; deliberately print
the dev username/password, UI URL and reset warning once to the dev console.
Do not print other recovery/admin keys or secrets into review artifacts.

`dev:full` waits for API, BFF/login and seeded-local-user readiness and prints
`http://127.0.0.1:<ui-port>/t/dev/login` before app setup/batch completion. API
8080 and UI/BFF 3000 remain separate process listeners behind the logical
application; use existing routing/public-origin owners and readiness precedent,
not a new environment controller. Require matching trusted origins and a
loopback-safe development listener. Force inherited WYRD_UI_LOCAL_AUTH fixture
mode off for this primary journey. Optional local Keycloak remains SSO test
coverage, never a dependency for password login.

Preserve default 500/concurrency 8, explicit small-run overrides, pinned canonical
source/current production wheel, normal OTLP export, exact request membership,
failed setup/batch inspectability and app-first graceful drain. A browser must
log in and inspect/use the UI while the batch is running; waiting for batch
completion is not a login-readiness test. Appended dev:batch adds evidence
without restarting the session/environment. Existing ephemeral reset disclosure
remains explicit; no new persistence/reset contract.

### Observe alignment and visual authority

Use the complete gap table in `references/task-03-observe-audit.md` as inspected
source evidence, with the actual Main/Metrics/Logs/Spans/TraceDrawer/TimePicker
`.dc.html` boards and approved reference PNGs. Their region layout, spacing,
density and color roles are required. Current candidate screenshots and a list
of deviations are not a new reference. Preserve existing shared Chart,
RecordTable, FacetRail, TraceWaterfall, RecordDetail, Drawer and TimePicker owners;
extend presentational seams instead of rebuilding them in page markup.

Explicit closure work:

- Metrics: chart card and separate selectable-series summary card as in the
  board; remove nested generic-chart framing and the extra default raw-points
  table. Retained structures remain inspectable through an explicit recorded-
  points inspector using the existing Drawer, RecordTable and metric list loader;
  open it only on user action, with bounded reads and normal authorization.
  Do not add a new metric detail service or hide/drop recorded structures.
  Keep line/summary selection, exact supported summaries and generic Chart
  consumers; no fabricated percentiles.
- Logs: selected detail aside belongs in the same records work region as the
  list, with the responsive equivalent, not below the entire split. Keep the
  compact severity timeline, severity key, Wrap/Columns and exact trace return.
- Main/Spans: reproduce chart density/baselines and explicit p95 lime/p50
  periwinkle mapping. Preserve roots-versus-spans, typed grouping, live holds,
  new rows, Columns/CSV, common URL/range/predicate and real SSR props.
- Waterfall: preserve current 884px drawer, header/actions and 300/110/flex/64
  grid with 28px rows; use the approved series/error palette and noncolor cues
  driven by actual recorded identities/roles, not invented operation-name rules.
- Canonical palette: adopt spec revision 19's explicit nav/selected/severity/
  series dark values and stated light counterparts in brand/palette.json and
  the owning brand guidance; generate theme/catalog projections through their
  owners. Do not copy hardcoded board hex into consumers or use default Chart
  cycling/opacity that reverses or blends the named reference colors. Keep
  bundled fonts and canonical mark assets, current schema fields and real data.

There is no approved light canvas board. Use the approved canonical light-token
mapping and obtain separate human acceptance. Every light proof must first
assert the selected and computed theme is light; a dark image named light is
failure. Generated captures/diffs stay outside the tracked repository. Never
capture password/API-key/enrollment/reset secrets.

## Approach

1. Apply approved revision 19's provision-then-enroll admission and contracts; amend its named architecture
   and brand owners with the selected contracts before incompatible behavior lands.
2. Extend existing User/browser/auth SQL and password/session lifecycle with
   stable errors and forward migrations; keep existing SSO/recovery semantics.
3. Extend the current credential owner/lookup/issuer chain and self-service
   authorization, and close safe public client projections and own-key Settings.
4. Integrate real local signup/login/recovery/account forms into the existing
   SvelteKit BFF and reusable shell, with no credential storage in Node.
5. Extend normal dev bootstrap/readiness to seed and expose a usable human
   before the 500-request workload, retaining current lifecycle and source/wheel.
6. Correct shared palette/composition once, then inspect shell/TimePicker,
   Traces/waterfall, Logs and Metrics as separate live visual checkpoints.
7. Complete focused real-server/browser/client proof and individual acceptance;
   do not substitute data plumbing, old evidence or a completion summary.

## Ordered Implementation Scenarios

### Scenario 1 — Signup obeys the selected tenant admission

**Behavior.** Under the approved invitation model, an authorized administrator
issues an invite with existing Role IDs. The person supplies code/username/
password and receives a real local User/session in that tenant. Expired,
revoked, reused, concurrent, wrong-tenant, malformed or role-escalating requests
are refused atomically; canonical-username collisions do not create a second
account. SSO same-email identity remains separate. The initial-owner case uses
a real already-provisioned tenant in both deployment models and does not test
or claim public anonymous SaaS tenant creation.

**RED.** Extend planned V1 and browser V4 with valid/negative admission and
case-normalized duplicate cases. Observe absent local signup before claiming
RED; setup/compile/no-selection failures are not behavioral proof.

**GREEN.** Extend existing auth/User/role/session owners and the forward migration
with the approved contract; no anonymous administrator grant or Node authorizer.

**REFACTOR.** Keep one transaction/credential owner and reuse stable error and
Origin/tenant boundaries; do not add a second signup/account framework.

### Scenario 2 — Local login, change and reset preserve credential authority

**Behavior.** Correct credentials establish the standard opaque browser session;
wrong/unknown/suspended and guessing attempts refuse safely. Local change/reset
invalidates old local sessions across replicas. SSO/recovery-key and existing
bearer precedence stay correct. A User can recover through the actual approved
operator path; there is no fake email flow.

**RED.** Planned V1/V2 and V4 cover policy/blocklist/NFC, verifier-only storage,
rate-window bounds, safe failures, expired/reused reset, missing/wrong Origin,
old-session rejection and unchanged SSO identity. Change and reset at replica A,
then present the old local-session cookie at replica B and require refusal;
reuse WyrdTestServer::start_replica. Use static schema/error/docs
proof for nonexecutable contract changes, not artificial RED.

**GREEN.** Reuse installed password libraries, BrowserSessions, User authority,
TenantConn/SQL and auth maintenance; extend the local authority binding once.

**REFACTOR.** Consolidate shared verification/session checks at their owner and
preserve app/auth availability under bounded hashing work; no Redis or ad hoc
Tokio runtime.

### Scenario 3 — An ordinary local or SSO User owns usable personal keys

**Behavior.** Both human login methods reach account Settings without tenant
OIDC-admin permission. Recent human login creates a named scoped expiring key
once; metadata/reload never returns it. Its ordinary SDK exchange remains
principal kind User with current grants intersected with ceiling. Own revoke,
expiry/suspension and wrong-owner/tenant/escalation cases behave as specified;
existing Service/Admin rotation still works.

**RED.** Planned V3/V4/V6–V9 fail the current self-admin gate, service-only FK/
lookup/issuer assumption and absent fields/page. Test real User key use on
allowed and denied calls, metadata secrecy and restriction after role changes;
MCP validates metadata/revoke without secret creation. V3/V6–V9 assert the
selected profile/metadata fields and paging contract with more than 100 keys,
including equal creation timestamps: follow every cursor without duplicates
or omissions, enforce the 100 maximum, reject wrong-target cursors, and observe
the final omitted next_cursor. Ordinary unpaged self calls refuse explicitly;
existing administrator no-query HTTP/client/MCP lists still return all records,
with unchanged Service/Admin issuance and platform metadata shapes. Compare
served schemas with actual JSON, including optional versus nullable fields.

**GREEN.** Extend existing table/verified owner/TenantTokenIssuer and principal
API once, then thin client projections and the server-rendered Settings form.
Use the approved no-store transient creation-result exception; no key cache.

**REFACTOR.** Keep generation, verification, expiry, audit and role resolution in
their existing owners; avoid a personal-key service or broad permission shortcut.

### Scenario 4 — One dev command exposes normal human login before its batch

**Behavior.** Default dev:full seeds and reports username/password and ready UI
before 500 workload calls complete. A browser logs in normally, investigates
real data, reaches Settings, appends a batch, logs out and closes owned children.
A failed app setup/batch leaves healthy UI/server available. No API-key or
fixture-auth shortcut counts as the primary journey.

**RED.** Extend existing V5 lifecycle with an explicit small batch and control
its ordering to assert usable human login before batch completion, a real User
session and normal retained readback; inspect default-500 configuration
separately. Do not manufacture synthetic host load or use a sleep as readiness.

**GREEN.** Extend existing setup/bootstrap/process/readiness owners and print
safe dev-only credentials early; retain current canonical source/wheel/drain.

**REFACTOR.** Reuse the existing CLI/BFF readiness pattern and current test
process owner; no alternate environment service or test-only production mode.

### Scenario 5 — Main, Logs, Metrics and waterfall match the approved UI

**Behavior.** Actual populated Main/Spans/TraceDrawer/TimePicker/Logs/Metrics
regions match their named boards and exact approved dark tokens, with a verified
light counterpart and 390px controls. Metrics has the correct two-card layout,
Logs detail is beside records, and duration/severity/waterfall colors agree with
the canonical mapping. Retained fields, honest unsupported operations and
explicit exemplar deferral stay intact.

**RED.** Extend V10 and affected existing V11 browser/component assertions to
check region order/placement, computed colors/opacity, density, bounds, series
visibility and selected theme. The current source-backed gaps must fail their
specific checks; a green row-count assertion is not RED/GREEN for design.

**GREEN.** Correct the existing shared components/palette and direct conventional
composition; regenerate from the canonical owner. Keep meaningful query/detail/
structure access without unapproved extra default panels.

**REFACTOR.** Keep reuse and generic Chart/Table callers intact. Inspect and
present shell/TimePicker, traces/waterfall, Logs and Metrics separately before
moving to the next dependent visual slice; existing human checkpoints remain
required. Fix detected deviations rather than record them as accepted.

## Acceptance Criteria

| Spec obligation | Required observation and proof |
|---|---|
| REQ-041; AC-024 | V1/V2/V4 real signup/login follows the approved admission/identifier/password contract, creates no partial/mis-scoped User, stores verifier only and correctly rejects abusive/cross-tenant/stale sessions/reset. V12 retains SSO/replica behavior. |
| REQ-042; AC-025 | V3/V4 create/list/use/revoke for ordinary local and SSO Users; V6–V9 prove actual User SDK identity, permission ceilings, safe metadata, profile and explicit page continuation. More-than-100/equal-timestamp cases prove complete traversal; existing admin no-query lists remain complete and existing Service/Admin/platform wire shapes remain unchanged. |
| REQ-043/038; AC-026 | V5 proves early normal login/readiness and append/cleanup on a small actual workload. Final normal dev:full runs default500 and provides inspectable UI with known local credentials, production SDK/source and actual retained signals. |
| REQ-044/007/015–017/020/030/034/039; AC-027 | V10/V11 plus per-board current external evidence close every named audit gap in actual desktop/mobile light/dark rendering, keyboard/touch and long-content states. Components' empty/loading/denied/error/partial states use their existing prop-test owners plus applicable real-browser paths. Individual human acceptance is recorded separately for each board/component. |
| INV-001/002/004/007 | Source/consumer/migration/generated-contract audit preserves tenancy, roles, credential provenance, Origin/privacy, existing platform and SDK boundaries, one owner, stable errors and required rustdoc. No secret appears in page loads, logs, URLs or captures. |

## Implementation Reference

Read the complete [auth research](../../references/task-03-auth-research.md) and
[current UI audit](../../references/task-03-observe-audit.md); they distinguish
actual source, primary guidance and approved policy. Paths below are guidance,
not a private-method/file allowlist.

| Workflow | Inspected existing owner/callers | Selected extension and missing behavior |
|---|---|---|
| Typed browser auth | `crates/wyrd-spec/src/auth/browser.rs`; server `src/auth/browser.rs` | Extend password/signup/change/reset contracts and handler family, with secret-safe stable errors/OpenAPI; preserve SSO/recovery. |
| User credentials | `wyrd-sql/src/queries/auth/users.rs`, row types/migrations; workspace Argon2/password-hash | Add canonical local username/revision and password verification policy using existing password_hash; forward migration, no email merge. |
| Browser authority | `wyrd-auth/src/browser_sessions.rs::BrowserSessions/create_session`; SQL browser_sessions | Add exactly-one local credential revision; retain existing session/cookie/status/expiry/Origin owner. |
| Invite/reset/abuse | Existing auth+TenantConn/maintenance, Permission::users_manage and role grant owners | No complete existing workflow found. Earn narrow purpose-bound one-use auth records/attempt windows in these owners; no duplicate credential/session store. |
| Personal key storage/lookup | `wyrd-sql/src/queries/auth/service_accounts.rs::{insert_api_key,api_key_by_prefix}`, `queries/auth/api_keys.rs` | Extend current service-only relational owner/lookup with real User ownership and label/ceiling metadata. Preserve old verifiers and lifecycle. |
| Key verification/issuance | `wyrd-auth/src/exchange_api_key.rs::ExchangeApiKey`; `issuance.rs::{TenantGrant,TenantTokenIssuer}` | Carry verified owner kind and ceiling through one resolver/grant builder; preserve credential attribution, current user roles and automation behavior. |
| Self API/client | server `components/principals/routes.rs`; `wyrd-client/src/principals/handle.rs`; native/Python/TS projections | Add explicit self profile/metadata/revoke plus interactive create, configured authorization and exact projections. No users granted service_accounts:write merely to enable Settings. |
| UI auth/settings | `src/lib/server/auth/browser-sessions.ts`, tenant login/settings routes and direct BFF load/action pattern | Add local forms and ordinary account page; never hold durable password/key/session state in Node. |
| Workspace provisioning | server `components/platform/provisioning.rs::TenantProvisioning`; POST /platform/tenants, Platform::create_tenant, CLI setup | Reuse complete claim/provision/builtins/active/retry owner for operator/SaaS-controlled provisioning. Initial human enrollment is distinct from the existing headless admin and never grants platform permission. |
| Dev lifecycle | `mise.toml`, `scripts/postgres/dev-server.sh`, existing server setup and CLI DevServer readiness | Seed through normal local capability, force fixture mode off, wait/report UI before batch; no new process controller. |
| Observe | `observe/+layout.{server.ts,svelte}`, `+page.{server.ts,svelte}`; shared components/loaders; `brand/palette.json`/generator | Repair exact audited regions and canonical color roles once. Preserve current SSR/shared query owners and drawer/waterfall corrections. |

Update owning architecture sections named in spec 19 alongside the new contract;
source does not override them. Public wire roots and stable errors must project
through served schemas and generated TS contracts. Authored Python stubs follow
native additions; napi declarations come from their generator. No hand-editing
generated artifacts, no additional server-heavy SDK dependencies.

## Verification and Evidence

All new tests below are **planned**, not existing PASS evidence. Existing
packages/targets/setup owners are confirmed; confirm actual new selectors after
implementation and before RED. Use the existing Vitest/Playwright Node API,
WyrdTestServer/Postgres and SDK fixtures. No new runner, family aggregate or
unrelated full SDK suite. The SDK tests use public clients and permitted
non-Service principal credential fixtures; browser creation is covered in V4,
not raw HTTP embedded in Python/TS client tests.

**V1 — planned local signup/session lifecycle, existing server integration owner**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::local_signup_login_and_password_lifecycle)"'
```

**V2 — planned tenant/admission/abuse negatives, same owner**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::local_signup_is_tenant_bound_and_rate_limited)"'
```

**V3 — planned User credential integrity/permission lifecycle, same owner**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle)"'
```

**V4 — planned rendered local signup/account keys, existing browser owner**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^local signup and personal settings$"'
```

V4 uses a real local User/session and needs no identity provider. Rendered SSO
Settings/key creation belongs to V12's existing configured provider/BFF owner.
Do not invent an SSO session in page JavaScript to reuse this local-only setup.

**V5 — existing normal dev lifecycle, changed to primary local login**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^development environment lifecycle$"'
```

**V6–V8 — planned first-class own-key metadata/revoke/use journeys, existing principal role files**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=principal_roles::users_manage_personal_keys)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_principal_roles.py::test_users_manage_personal_keys'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/principal-roles.test.ts -t "^users manage personal keys$"'
```

**V9 — planned MCP own metadata/revoke without secret issuance, existing module**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=principals::pg_tests::users_manage_personal_key_metadata)"'
```

**V10 — planned real rendered reference/theme/geometry matrix, existing browser owner**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^observe boards match approved canvas$"'
```

Set/verify actual theme and computed fills before capture; use 1440px and 390px,
real seeded-human data and named region assertions from the audit. External
captures/text comparisons plus separate human inspection close visual proof;
recording image width alone does not prove controls stay visible.

**V11 — existing affected Observe and shared component behavior**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated (traces and spans|logs investigation|metrics investigation)$"'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts
```

**V12 — existing real SSO/replica regression owner**

```bash
mise run test:identity:ui
```

That existing owner currently drives production auth HTTP/form flows across
replicas and Keycloak/Dex. Extend its existing provider/BFF bootstrap and called
Vitest production-auth integration file with Playwright Node API rendering of
Account Settings/key creation after a genuine provider login. Keep the existing
HTTP/replica assertions; do not claim they alone already render the UI. Reuse the
same runner/dependency and real SSO cookie/session, never a fixture identity or
new provider service. This supplies SSO-user parity for AC-025; V4 remains the
local-browser proof and V10 the Observe visual matrix.

Use existing setup prerequisites: `mise run build:server`, `mise run ts:build`,
`mise run ts:build:testing`, Python testing wheel via `mise run py:setup`, frozen
UI pnpm install, production BFF/library build and its test-only Chromium. The
canonical app uses its separate production wheel as before. Cargo commands
sharing a target directory run sequentially across agents.

Run the checks for their actual changed write sets through existing owners:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:tenant-isolation
mise run check:deps
```

Use the existing auth migration/SQL and served OpenAPI contract owners for the
new contracts; codegen alone does not prove HTTP/MCP behavior. V1–V3 cover the
forward-migrated auth/tenant/key seams against real Postgres. Pure password
policy/verifier tests belong inline in their auth owner and need no credentials;
record their actual exact selector before RED rather than inventing another
runner or pretending a planned name is already selected. Do not require full family/user-journey aggregates or gate
as this task's checklist. At integrated change review, run normal default-500
dev:full once and complete cross-task journeys, not another 3000-request replay.

Record TDD results, static forward-migration/authority/generated-projection
checks, actual browser URL and dev login mechanism (never actual secrets in
evidence), per-board/component light/dark desktop/mobile results, and individual
human acceptance separately. If an audit gap remains or Metrics light was not
actually light, the task is incomplete; a green data test cannot waive it.

## Material Stop Conditions

Specification 19's admission/security/visual contract was approved on 2026-11-02.
Stop for a necessary new tenant-creation/billing flow, unapproved
email/MFA/linking behavior, altered key/JWT lifetime policy, unsupported
cross-service/credential authority or a genuine conflict with the amended
architecture. Provide the exact source/caller consequence and smallest existing-
owner alternative. Private methods, local control flow, module/fixture details
and equivalent selectors are implementation-owned; do not use those as scope
exceptions. Do not revive deferred exemplar precision or alter Bifrost integer
serialization to satisfy a UI display preference.

## Authority Links

- [Approved revision 19](../../spec.md), [order](../../plan.md).
- [Auth and product research](../../references/task-03-auth-research.md).
- [Current Observe audit](../../references/task-03-observe-audit.md).
- [Original Task02](TASK-02-live-observe-workflow.md) and prior review/R1/R2 context.
- Root AGENTS, SQL/Python/TS nested AGENTS, changes/spec-driven-development.md,
  implementation-execution.md and TESTING.md.
- architecture/wyrd-design.md Human sign-in/Client model; security posture API
  keys/browser sessions/permissions/tenant isolation; `.agents/skills/wyrd-ui`.

Planning changes no code, migration, authentication behavior, credentials or
visual acceptance. Do not commit/push unless explicitly asked for this packet.

### Evidence — Scenario 5 (Observe visual)

Implemented on branch `task/wyrd-ui-main-integration/TASK-004-observe-visual`
(commits 527b67587, a2b0989b3, 296117b72). Browser proofs ran against the
production BFF build, `WyrdTestServer` and repository Postgres, with real
OTLP-seeded evidence. Sign-in used the file's existing API-key `signIn` helper.
No secrets were captured. Captures were written to the session scratchpad
through `WYRD_UI_CAPTURE_DIR` and are not tracked.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Metrics chart and series cards (audit row 1) | `Chart` takes a host `heading` snippet. With it, the plot frame and a separate series-summary frame render, with no generic header or SQL footer. The Observe route supplies the metric heading with Calculation and `by`. The default raw RecordTable was removed. **Recorded points** (URL `points=1`, Metrics only) opens the same bounded list read in the existing `Drawer` (884px) with `RecordTable`. The precision caveat stays in the heading's type line. | Unit: `Observe canvas colour roles and series composition` checks two `.chart > .frame`, the legend only in the second frame, and no footer. V10 checks the plot card above the series card, 28px legend rows, no default `[data-panel=list]`, and the inspector opening at `min(884, viewport)` and closing on Escape. V11 metrics opens Recorded points before the list and structure assertions. | Closed |
| Selected Log detail (row 2) | The `.work` flex-wrap region holds facets (`0 0 220px`), records (`999 1 560px`) and the record aside (`1 1 360px`, max 400px). Below 899px the regions stack. The aside body is padded. | V10 at 1440px: facets, records and record are side by side with equal tops and the record at most 400px. At 390px they stack in that order, inside the viewport, with no page overflow. V11 logs passes. | Closed |
| Traces duration colours (row 3) | `DURATIONS = { p95: var(--observe), p50: var(--declare) }`. Explicit host colours override position. | Unit checks the polyline strokes and key swatches. V10 checks the computed p95 and p50 strokes against the palette values per mode. | Closed |
| Overview density (row 4) | The compact Chart plot is exactly 56px. There is no SVG axis text, a baseline sits on the bottom edge, and HTML time labels sit below the plot. The Logs band is 64px through a host rule. | V10 checks heights `[56, 56, 56]` with one shared bottom at 1440px, stacked inside the viewport at 390px, and the Logs band at 64px. | Closed |
| Logs severity colours (row 5) | `--level-info` was added. warn and error use `--warn` and `--danger`; unset uses `--muted`. Segment opacity blending was removed. | Unit checks the fills and the absence of opacity attributes. V10 checks the computed fill (`--danger`), opacity `1`, and the key swatches info, warn, error, unset. | Closed |
| Waterfall bars (row 6) | Bars are coloured by recorded `service_name` through the series roles, and errors by status (`--danger`) with the existing `✕`. A Services key lists real services and the error cue. At narrow widths only the end axis labels show. | Unit checks two services plus errors. V10 checks the computed bar colours by recorded service and error, and the key `['ui-agent', '✕ error']`. | Closed |
| Navigation surface and selected borders (row 7) | `--nav-surface` and `--selected-border` were added to `brand/palette.json` and projected by `gen-theme.mjs`. The tenant rail, WHERE, badge, pressed segments and open Filters consume the tokens. | `node --test brand/gen-theme.test.mjs` went RED on the missing `--nav-surface` light role, then GREEN. V10 checks the computed rail background (212px desktop), the WHERE border and the pressed Calculation border per mode. | Closed |
| Extra series hues (row 8) | `SERIES_COLORS` is declare, observe, `--series-3/4/5` (dark `#4d5ef0`/`#a9b4ff`/`#8fae2a`, with the stated light counterparts). Spans bars use `--series-3`. | Unit checks the six-series stroke order; it was RED against the earlier cycle. V10 checks the Spans fill `--series-3` and the Metrics strokes declare then observe. | Closed |
| Drawer and waterfall grid (row 9) | Unchanged 884px drawer, header, Link, Expand and Escape. The grid is still 300/110/flex/64 with 28px rows. | V10 checks the drawer at `min(884, viewport)`, every row at 28px, and the columns `[300, 110, …, 64]` at 1440px. V11 traces passes. | Closed |
| Trace list controls (row 10) | No change. Columns and CSV stay in the trace listbar, and Logs keeps Wrap and Columns in its records header. | V11 traces and logs pass. | Closed |
| Light theme (row 11) | Light counterparts for every new role are in the palette. | Before any colour check, V10 sets the `wyrd-mode` cookie and asserts `.app-root[data-mode]` and that the computed `--bg` equals the palette value, with every channel above 200 in light and below 40 in dark. All boards are checked in both modes at 1440px and 390px. | Closed pending human acceptance |
| V10 | `observe boards match approved canvas` | `… -t "^observe boards match approved canvas$"`: 1 passed. It was authored after the GREEN component change, so its first runs failed only on test authoring (collapsed tree, unset key). Executable RED for design came from the unit and palette tests. | Pass |
| V11 | Localized edits to the metrics journey only | `… -t "^authenticated (traces and spans\|logs investigation\|metrics investigation)$"`: 3 passed. | Pass |
| Unit | `ui-pages.test.ts`, `ui-library.test.ts` | 16 passed. | Pass |
| Check and build | `pnpm check`, `pnpm build` | 0 errors and 0 warnings; build exit 0. | Pass |
| Codegen | `mise run codegen:check` | exit 0. | Pass |

Per-board observations (inspected real captures):

- **Shell / TimePicker.**
  - Dark and light, 1440px: the rail is 212px on `--nav-surface`, with the selected destination on `--declare-soft`. The picker panel is on `--surface`.
  - 390px: the rail becomes a top bar. The picker presets, the custom fields, UTC/Local and Apply range all fit inside the viewport.
- **Traces / Spans.**
  - Dark and light, 1440px: the three 56px tiles share one baseline. Spans are cobalt `--series-3`, errors are red, and the duration key reads p95 lime / p50 periwinkle. With the single seeded bucket, duration draws as a short mark.
  - 390px: the tiles stack, and the controls wrap inside the screen. The list table scrolls horizontally inside its own panel.
- **Waterfall.**
  - Dark and light, 1440px: 884px drawer, Services key, `ui-agent` bars in `--declare`, and the error span red with `✕`.
  - 390px: full-width drawer showing tree, track and duration. The overprinted middle tick was fixed so only the end ticks show.
- **Logs.**
  - Dark and light, 1440px: the 64px severity band in `--danger`, with the key info / warn / error / unset. Facets, records and detail sit in one row. The unpadded detail body was fixed.
  - 390px: the regions stack in order. The detail fits the screen.
- **Metrics.**
  - Dark and light, 1440px: a 240px rail ending in Recorded points, then the chart card above the series card. Lines are declare then observe, with distinct markers.
  - 390px: the rail, chart and series cards stack. Calculation wraps, and the summary names truncate inside their cells.
  - The Recorded points drawer lists the four seeded points at 884px, or at the viewport width on mobile.

Remaining notes:

- The Recorded points rail link is an addition to the Metrics board, needed for the required explicit inspector.
- Compact tiles each draw their own time labels rather than one shared Main time row.
- Exemplar precision stays deferred.
- Human acceptance of every board in dark and light, at desktop and mobile, remains pending.

### Evidence — Scenarios 1–4 and 6 (local users, personal keys, dev login, SSO parity)

Implemented on branch `task/wyrd-ui-main-integration/TASK-004` (commits
e1f1ef19d through the closing evidence commit). Every browser proof ran
against the real BFF, Wyrd server and repository Postgres. No password, API
key, invitation code or reset code appears in evidence, logs or captures. The
dev login mechanism is the local User `dev`, signed in on the normal password
form at `http://127.0.0.1:3000/t/dev/login`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Local signup, login, change and reset; verifier-only storage; sessions end on change/reset | `wyrd-auth/src/passwords.rs`, `wyrd-server/src/auth/browser.rs` `/auth/browser/password/*`, `wyrd-sql` local password queries and migration | V1 `pg_browser_sessions::local_signup_login_and_password_lifecycle` | PASS |
| Tenant binding, admission and abuse limits (10 per account → 429 + Retry-After) | same owners | V2 `pg_browser_sessions::local_signup_is_tenant_bound_and_rate_limited` | PASS |
| User keys: one owner, ceiling ∩ current grants, recent-login creation, metadata pages, revoke | `components/principals/{routes,accounts}.rs`, `wyrd-runtime` `personal_credentials:*` | V3 `pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle` | PASS |
| UI: login, signup, reset and `/t/[tenantKey]/settings/account`; secret only in the no-store action result; 30-day default, no ceiling preselected, paged list, revoke; identity menu "Account"; no identity_connections:write needed | `wyrd-ui/src/routes/t/[tenantKey]/{login,signup,reset,settings/account}`, `lib/Entry.svelte`, `lib/server/form.ts`, `browser-sessions.ts` `password`/`resetPassword`/`changePassword` | V4 `local signup and personal settings` (editor User; the secret is absent from SSR, reload and console; wrong code, empty ceiling, old password and foreign tenant are refused) | PASS |
| Dev seeding: `wyrd-server setup --local-user --local-password-stdin`; `dev` with a random 20-character password; 0600 `dev-login.toml` removed on exit; login printed before the batch; fixture auth forced off | `wyrd-server/src/main.rs` `setup`/`local_user`/`create_local_admin`, `scripts/postgres/dev-server.sh`, `mise.toml` `dev:full` | V5 `development environment lifecycle`: "UI ready" before any batch line, record mode 0600, password login → Account → batch → traces/logs/metrics points → appended batch → sign-out, record removed after Ctrl-C | PASS |
| SDK profile and own-key pages (Rust, Python, TypeScript) | `wyrd-client` `Principals::{me,list_credentials_page}`, PyO3 and napi projections, authored `.pyi`, `WyrdTestServer::bootstrap_user_key` | V6 `principal_roles::users_manage_personal_keys`; V7 `test_users_manage_personal_keys`; V8 `users manage personal keys` | PASS |
| MCP own-key metadata and revoke, no secret issuance | `wyrd-server/src/mcp/principals.rs` | V9 `principals::pg_tests::users_manage_personal_key_metadata` | PASS |
| Observe regressions after the shell change | identity menu link only | V10 `observe boards match approved canvas`; V11 three journeys plus `ui-pages.test.ts`/`ui-library.test.ts` (16) | PASS |
| SSO User parity: Account Settings and key creation after a real Keycloak login | `production-auth.integration.test.ts` `production SSO account settings` (Playwright, viewer `bob`) | V12 `mise run test:identity:ui` (5 passed) | PASS |
| Architecture amended | `architecture/wyrd-design.md` Human sign-in; `architecture/wyrd-security-posture.md` API keys and browser sessions | review of the diff | PASS |
| Checks | — | `pnpm check`, `pnpm build`, `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:tenant-isolation`, `check:deps` | PASS |

Diagnosis (V5, first run). **Symptom:** the Observe list poll timed out for
`signal=metrics` only. **Evidence:** the page text showed the metric-name rail
with retained metrics. The route loads points only when `points=1`
(`observe/+layout.server.ts:51`) and renders them only in the Recorded points
drawer (`+layout.svelte:362`). **Cause:** a stale assertion from a2b0989b3,
which moved raw points into the inspector. **Fix site:** the lifecycle test now
opens `signal=metrics&points=1`, the same way the metrics journey does. An
independent read-only diagnostician confirmed this.

- The test harness gained `builtinRoleId`, because invitations name Role ids and no read surface returns them.
- The rail Settings link still opens tenant SSO administration. The identity menu's "Account" link opens personal Settings.
- The default 500-request `dev:full` run is left for integrated change review, as the task directs.

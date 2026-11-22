---
id: TASK-003-carryover
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 7
requirements: [REQ-002, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018]
acceptance: [AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013]
depends_on: [TASK-003]
---

# Complete onboarding, platform console and remaining social login

After this task, an operator can establish a usable human admin without an IdP,
visitors can create verified workspaces when enabled, and returning people can
find workspace links and reset existing passwords through verified contacts.
The delivered UI and clients consume the same Rust methods, provisioning and
session owners; linking, invalidation and required company SSO complete the
remaining social-login journey. Pre-registered operators can sign in to the
platform console without a browser-held bootstrap key. Nothing ships until all
cycles are complete; each capability is built once in its final form.

## Inputs, ownership and dependencies

Implement [approved spec revision 7](../spec.md), using finalized
[onboarding](../design/onboarding.md) (End-to-end journeys, HTTP contracts and
configuration, Proofs/persistence/abuse, all eight decisions) and
[social login](../design/social-login.md) (explicit linking/removal and company SSO).
Revision 7 records the maintainer's explicitly approved 2026-11-21 platform-console
and build-once decisions in REQ-018 and Revision history; those supplement the
finalized onboarding/social design without inventing retroactive design notes.
Authority is the spec's linked human sign-in, security and deployment sections.
Requirements are defined there once; the cycles below are observable proofs.

`depends_on: TASK-003` means its **completed cycles 1–5**, which the maintainer
reports committed. They supply tested/enabled operator Google/GitHub clients,
purpose-bound returning login, invitation issue/redemption, generic SMTP and
ordinary tenant social sessions. It does not depend on TASK-003's open work;
that work is owned only here. TASK-001/002's existing scoped grants, Access/account
UI, local password/device flows and client journeys are consumed through those
outputs. Historical partial UI limitations remain visible in TASK-003's evidence;
this task owns their unfinished delivery too.

TASK-003 cycle 6's server slice is implemented but uncommitted and staged in
`git diff --cached`. Inspect that diff before implementation; its evidence is
historical, not revision-7 acceptance. Reuse the current `TenantProvisioning`
human-owner path, `SocialSignIn`, `BrowserSessions`, password/reset validators,
`ApplicationMail`, shared admission and auth-maintenance owners and SQL query
owners; generalize local/external proof rather than duplicate orchestration.
The current provider-only start contract, pending-state SQL and email handling
must reach revision 7. Keep the earlier TDD violations visible; they cannot be
relabelled RED/GREEN. New missing behavior follows test-first cycles.

The staged migration `20261121000006_workspace_signup.sql` is **uncommitted and
unapplied anywhere**. Revise it in place to its final revision-7 shape, rather
than stacking a correction. Cite the spec's Persistence and authority changes
for every table/column/index/policy change. Proven email/consumption metadata,
tenant contacts/contact revisions, minimal discovery projection, shared admission
and creator/claim purposes are explicitly approved there. Revision 7 additionally
names platform-user password credentials, platform browser sessions and bound
platform login/re-authentication metadata. Reuse `PlatformLogin`,
`PlatformLoginRequest`, `PlatformSessions` current principal checks and Rust
`BrowserSessions`/SQL lifecycle mechanisms; tenant session rows cannot represent
tenantless operators without violating tenant RLS. Extend the existing owners;
do not fork login, session, admission or provisioning orchestration. Force RLS everywhere;
tenant tables lead with `data_tenant_id` and tenant-qualified references, genuine
platform/pre-tenant records use only the approved platform-owner exception.
Never edit an applied migration or introduce another persistence purpose/SQL
function without `SPEC_REVISION_REQUIRED`.

Real inputs are operator settings/setup stdin, recipient invitations, browser-bound
email/provider proof and current allowed methods. Rust validates and provisions
through platform claim → tenant initialization → promotion, or verifies existing
tenant contacts/reset state, and returns safe results to UI/HTTP/CLI/SDK consumers.
The discovery projection is needed because tenant-RLS connections cannot scan
all tenants at root: it is HMAC contact metadata plus current revalidation,
never a global account, grants or authorization substitute. Mail runs after
transactions; reset sends are bounded transient process work with generic public
responses, while operator/invitation delivery reports honest relay outcomes.

Keep independent tenant Users/passwords/grants; preserve bearer invitation join,
explicit linking, SSO policy and API-key/operator recovery. No global person,
automatic merge, OAuth password grant, mail queue, automatic retry, provider-token
vault, deployment-wide cap or mandatory Turnstile. Core defaults remain private.
No verification/gateway/Oracle/Scribe hot-path wait is authorized by onboarding.

## Ordered acceptance and implementation evidence

Execute each behavioral cycle test-first: add the real consumer assertion, confirm
its actual focused selector and setup, record the missing-behavior RED, implement
the smallest owner change, record GREEN and limitations here. Do not invent test
names/selectors or repeat already-correct work merely to manufacture RED.
All foundations and acquisition cycles prove real server/contract behavior first;
production page work waits for cycle 15's mock extension/approval. Existing code
has historical evidence; only new behavior can claim new test-first execution.
No cycle creates an interim contract, pending-row signup implementation or
provisioning/session engine for a later cycle to replace. Shared owners take the
final local/external proof union and persistence purposes from the outset;
later cycles add consumers/transport-specific proof, not replacement machinery.
The original TASK-003 cycles 7–10 remain the final four cycles here.

### 1. Trusted source, IPv6 /64 and missing-source refusal

**Reuses:** Existing trusted ingress/peer and shared password-admission owners; this establishes source derivation for all later cycles.

**Maps to:** REQ-014; AC-005.

Behind configured trusted ingress, two visitors' client-address headers get separate
windows; untrusted-peer spoofed headers are ignored. Password admission uses the
same derivation with separate windows. Two IPv6 addresses in one `/64` share the
signup window; mapped IPv4 normalizes to IPv4. With no determinable source, both
signup start and completion refuse without bypass. Use real ingress/peer request
context and shared Postgres across replicas.

**Proof:** tier-1 HTTP requests plus admission integration; RED is unsafe source
derivation/grouping/missing-source admission in the staged slice.

**Evidence (cycle 1):** GREEN — `platform_admin_e2e::signup::admission_keys_the_trusted_source`
(`tests/integration/platform_admin_e2e/signup.rs`) drives real `/auth/signup` starts,
completions and `/auth/browser/password/login` failures through `RequestSource` behind
`with_trusted_ingress_for_test(["10.0.0.0/8"], "CF-Connecting-IP")`: separate visitor
windows, untrusted-peer header ignored (peer spends only its own window), mapped IPv4,
shared/distinct IPv6 `/64`, `503 source_unknown` for start and completion with no
tenant row, and `wyrd.auth_password_attempts` source subjects keyed by the same
derivation. Unit: `auth::source::tests::only_a_trusted_peer_names_the_client`,
`admission::tests::ipv6_sources_share_their_64`,
`config::tests::trusted_ingress_needs_a_header_and_valid_entries`. Regression:
`platform_admin_e2e::a_visitor_creates_a_workspace_with_google` passes. Owner reuse:
`TrustedIngress` (deployment config `http.trusted_ingress`/`client_address_header`,
`WYRD_TRUSTED_INGRESS`/`WYRD_CLIENT_ADDRESS_HEADER`) is the one derivation; password
`Windows` and signup `admit` key through `wyrd_auth::admission::source_key`; the staged
`peer()` helper is deleted. Start admission (`start` scope in the revised staged
migration) runs before settings/provider work. **TDD: VIOLATED** — production code
was written before the journey's first run, so no RED was observed for this cycle.
Command: `cargo nextest run --locked -p wyrd-server --features test-support --test
integration -E 'test(/^platform_admin_e2e::signup::/)'` with the repository Postgres
lifecycle env.

### 2. Every signup throttle carries Retry-After

**Reuses:** Cycle 1 source derivation and existing shared counters; later consumers retain the same Retry-After projection.

**Maps to:** REQ-014; AC-005.

Exhaust start/completion source and completion identity windows through real
requests. Every 429 has whole-second `Retry-After` equal to
`details.retry_after_seconds`; quota/challenge failures keep their separate codes.
Unavailable shared admission refuses rather than admitting.

**Proof:** tier-1 HTTP header/problem assertions against real counters; RED is the
missing/mismatched header, not a private error-construction fixture.

**Evidence (cycle 2):** RED — `platform_admin_e2e::signup::every_signup_throttle_carries_retry_after`
failed first at `stated_wait`: the start 429 carried `details.retry_after_seconds: 3600`
and no `Retry-After` header. GREEN after the shared renderer
(`http/error.rs::wyrd_error_response_from_parts`) projects `SignupAttempts` exactly as
`PasswordAttempts`: start, completion identity and completion source throttles each
assert the header equals the problem field; renamed admission state refuses start with
`503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` (completion's equivalent remains in
`a_visitor_creates_a_workspace_with_google`). OpenAPI annotations document start 429/503
and both `Retry-After` headers. **TDD: PASS.** Same selector as cycle 1.

### 3. HMAC keys and bounded purge

**Reuses:** Cycles 1–2 admission and existing keyring/auth maintenance; define final retention before proof consumers.

**Maps to:** REQ-014, REQ-017; AC-005, AC-007, AC-009, AC-010.

Inspect actual admission/creator keys after source/identity/mail-purpose requests:
keyring-derived HMAC-SHA-256 pseudonyms contain no plain issuer, subject or address.
Expire windows and attempts/proof metadata, run existing maintenance batches and
observe bounded cleanup with spent identifiers retained until proof expiry. Public
requests never run unbounded deletion. Install the final retention policy for proof/attempt and creator metadata;
creator accounting survives attempt purge. The completion cycle later exercises
email clearing against this policy, retaining the separate verified tenant contact. Do not build another cleanup service.

**Evidence (cycle 3):** RED — `platform_admin_e2e::signup::signup_state_is_pseudonymous_and_purged_in_batches`
first failed at its key assertion: the `start` window key was the plain address
`198.51.100.40` (13 bytes). GREEN: start/source/identity keys are 32-byte
`SealingKeyring::pseudonym` HMACs holding no address, issuer host or subject; the
created tenant's `signup_creator_key` is 32 bytes; a public start leaves 601 expired
attempts in place; `SocialSignIn::purge_expired_signups` (the method the worker ticks)
removes `[1000, 201, 0]` across batches of at most 500 windows + 500 attempts, keeping
live windows and both live pending starts; after purge the tenant's
`signup_attempt_id` is NULL and its creator key remains. Unit:
`wyrd-crypt tests::pseudonyms_separate_scope_and_key` (scope/key separation).
Regression: the cycle 1–2 signup journeys and `a_visitor_creates_a_workspace_with_google`
pass (its staged interrupted claim now carries a creator key, required by the new
`tenants_signup_creator` check). Schema (staged migration revised in place, citing
revision 5's pseudonymous admission and creator-pseudonym purposes): admission `key`
is `BYTEA` of length 32, a `window_start` purge index, `platform.tenants.signup_creator_key`.
**Deviation:** no background auth maintenance existed — every auth table purged
inline on insert. The inline signup purge is removed and `ServerAuth::maintenance_worker`
(`auth_maintenance`, beside `refresh_worker` in `engine_workers`) is the one auth
maintenance owner, draining bounded batches each minute; worker wiring is source-cited,
the batch behavior runtime-proven. Mail-purpose HMAC scopes join the mail cycles
(10, 12, 13) through the same `pseudonym`. **Risk:** rotating the sealing write key
starts a fresh pseudonym space, so windows reset and cycle 8's cap cannot match
pre-rotation creator keys. Completion-email clearing remains cycle 7. **TDD: PASS.**
Same selector as cycle 1.

**Proof:** real SQL/maintenance integration and real admission HTTP requests; RED is a plaintext
key or missing/unbounded cleanup. Completion-email clearing is observed with
completion below, without replacing this maintenance owner.

### 4. Every local password path uses the 15-character minimum

**Reuses:** Cycles 1–3 admission and the existing password owner; every later password caller uses this final validator.

**Maps to:** REQ-014, REQ-015, REQ-017; AC-012.

Finish the shared validator and route every currently existing password caller
through it in this cycle. Through direct creation, invited enrollment, change
and existing reset redemption, submit fourteen normalized characters (refused),
fifteen-character passphrases (accepted unless blocklisted), 128-character Unicode
(accepted), and oversized input (refused without truncation). Preserve NFC,
Argon2id/blocklist and bounded blocking; no composition/periodic-rotation rule.
Observe no password/code/proof in logs or responses. Paste/password-manager UI
proof joins cycle 16. Preserve the same boundary cases for setup (cycle 11), local
signup (cycle 10), self-service reset (cycle 13) and password-add (cycle 17): those
cycles connect their new consumers to this already-final validator and record
their real integration proof there. This cycle does not implement those flows
or wait for their assertions.

**Proof:** shared-validator unit boundaries plus real owner API/setup integration
for existing consumers. Later consumer cycles own their shipped-surface
assertions. RED is the current eight-character acceptance;
no implementation-mirroring test or disconnected validator substitutes for callers.

**Evidence (cycle 4):** RED — `pg_browser_sessions::every_password_path_holds_the_fifteen_character_minimum`
first failed when direct creation (`POST /v1/principals/users`) answered `201` for the
fourteen-character `amber kestrel9`. GREEN after `wyrd_auth::passwords::PASSWORD_MIN_CHARS`
became 15. One journey drives every existing consumer, all of which already route through
`new_password`: direct creation, invited enrollment
(`/auth/browser/password/signup`), change, and administrator-reset redemption. On each
it refuses fourteen characters, fourteen after NFC (fifteen raw code points), 129
characters and 1026-byte input, each with `400 WYRD_SPEC_400_VALIDATION` naming the
field and never echoing the input. It admits fifteen characters and 128 non-ASCII
characters, and a reset password then signs in. Unit boundaries:
`passwords::tests::new_passwords_follow_the_policy`. NFC, Argon2id, the blocklist and
bounded derivation are unchanged and were already covered by
`local_signup_login_and_password_lifecycle`, which still passes along with the rest
of `pg_browser_sessions` (10/10). Callers' fixtures were updated: the Rust SDK
`principal_roles` journeys (11/11) and the existing tenant signup page copy (now
"15–128 characters"; its UI proof joins cycle 16). Logs are source-cited, not
runtime-captured: every password handler's `#[tracing::instrument]` skips the
request, and `SecretBearer`/`SecretString` redact. **TDD: PASS.** Commands:
`cargo nextest run --locked -p wyrd-server --features test-support --test integration
-E 'test(/^pg_browser_sessions::/)'`; `cargo nextest run --locked -p wyrd-ai -P journey
--run-ignored=all -E 'test(/principal_roles::/)'`.

### 5. Complete advertised methods govern every consumer

**Reuses:** Cycles 1–4 shared errors, admission and password policy plus completed TASK-003 settings; later methods consume one final contract.

**Maps to:** REQ-011, REQ-012, REQ-014, REQ-016, REQ-017; AC-011.

Finish the complete methods wire contract and its single shared availability/
policy evaluator here, using current settings and the approved required-company-SSO
state semantics. Exercise existing operator/tenant management inputs through their
real contracts; do not introduce a temporary methods shape.
Observe methods metadata for no providers, missing SMTP, password disabled,
required/unavailable company SSO and no usable methods. Required-SSO policy
calculation is final here; cycle 19 owns enabling that policy through the human
adoption flow and the connected proof of all live credential consumers. Separate returning and
acquisition providers; `signup` reports usable methods, never just a switch.
On already delivered start/completion paths, change settings after the response:
Rust refuses the now unavailable method, with no User/session/tenant side effect.
Cycles 6–14 and 17–19 connect their added consumers to this same evaluator and
record the equivalent race/refusal proof as those consumers become available;
this cycle does not require those later flows to exist. Check safe typed
HTTP/client projections and authorized MCP methods/settings metadata without
password/reset secrets or a new proof channel. Use the final typed email/local/external acquisition contract from the outset,
including proof-kind and capability distinctions. Rendered-page proof follows
cycle 16. Identity linking, invalidation and SSO cycles reuse this contract and
its policy checks, adding only their required behavior rather than rebuilding it.

**Proof:** tier-1 management → methods → existing real start/completion; controlled provider
and real Postgres. RED is incomplete capability metadata/enforcement. Contract/
export checks prove projection parity, not a substitute for refused execution.

**Evidence (cycle 5):** RED: `platform_admin_e2e::methods::methods_report_exactly_the_usable_capabilities`
first failed at `methods.rs:114` because the report had only `{password, company_sso,
providers, signup}`. The second RED,
`platform_admin_e2e::methods::delivered_paths_refuse_a_method_no_longer_offered`,
failed because password login answered `200` after the operator disabled password.

GREEN came after these changes:
- `LoginMethodsResponse` now carries the spec-named fields as three
  `#[serde(flatten)]` groups (`SignInMethods`, `EmailMethods`, `CreationMethods`),
  so the wire stays flat. The split meets clippy's limit of three bools per
  struct without an `#[allow]`.
- `SocialSignIn::login_methods(Option<TenantLogin>)` and its pure
  `available_methods` are now the one evaluator.
- `HumanConnections::tenant_login` reads the Active connection and the new
  `wyrd.auth_login_policies` policy. Migration `20261121000007_login_policy.sql`
  cites revision 3, "new tenant login-policy metadata", and forces RLS under
  `tenant_isolation` and `operator_access`.
- The server's `available_methods` is used by `GET /auth/browser/methods` and by
  every consumer.

The first journey covers these cases:
- defaults
- signup switched on without SMTP or a provider: still nothing to create
- SMTP added: local signup, discovery and reset
- Google enabled: separate `providers` and `signup_providers`
- signup off
- password off
- required SSO without an Active connection: deployment creation and discovery
  only, with no tenant fallback
- required SSO with an Active connection
- the root: never password or company SSO

The second journey changes settings after the page rendered. With password
disabled, then with company SSO required, it refuses password login, invited
enrollment, change and administrator-reset redemption. Under required SSO it
also refuses the returning Google start (`/auth/browser/login`). Each refusal
is `401 WYRD_AUTH_401_INVALID_TOKEN` with reason `method_unavailable`, sets no
session cookie and leaves the `auth_users` count unchanged. Lifting the policy
restores password login.

The delivered signup start and completion recheck now route through the same
evaluator (`SocialSignIn::require_signup_provider`). Their existing journeys
under `platform_admin_e2e::signup::` still pass.

The returning social callback recheck under required SSO joins cycle 19, which
owns the live-credential races. The UI `LoginMethods` type mirrors the wire shape.

**Limitation (open):** neither `wyrd-client` nor `wyrd-mcp` has a methods or
settings projection, so the MCP metadata check has nothing to inspect yet. HTTP
and OpenAPI are the only typed projections today.

`an_operator_relays_invitations_through_smtp` saw 2 messages where it expected 1.
- Diagnosis: the session's single Mailpit kept the previous run's probe
  addressed to the same deterministic `probe-test-tenant-1@example.test`, as
  Mailpit search showed.
- Cause: environment reuse. `with-test-postgres.sh` starts a fresh Mailpit for
  each invocation.
- Result: after `DELETE /api/v1/messages` it passes.

**TDD: PASS.** Commands:
- `cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(/^platform_admin_e2e::methods::/)'`
- `cargo nextest run --locked -p wyrd-auth -p wyrd-server --features wyrd-server/test-support --lib --test integration -E 'kind(lib) | test(/^platform_admin_e2e::/) | test(/^pg_browser_sessions::/) | test(/social|login|openapi/)'` (906/906)

### 6. Sealed cookies and start admission leave no pre-proof signup row

**Reuses:** Cycles 1–5 final source, counters, retention and methods; existing sealed-cookie, provider and ApplicationMail owners supply proof.

**Maps to:** REQ-012, REQ-014, REQ-017; AC-005, AC-007.

Start the final email/provider signup contract using actual browser cookies and
shared source windows. Exercise email through the existing mail owner and provider
proof through the existing provider owner. Produce the same bound proof-kind union
for completion; build no provider-only or pending-row intermediate implementation.
A flood is refused before provider discovery, SMTP or hashing. Unavailable admission
fails closed; only bounded counters may persist pre-proof. State/PKCE/nonce/provider
revision/redirect/browser context rides sealed short-lived HttpOnly path-scoped
cookies, not a pending platform signup row. Replay/concurrent callbacks insert at
most one proof by unique state digest. Email one-use identifier/spent retention
prevents replay across replicas and preserves same-browser lost-response recovery.

**Proof:** tier-1 HTTP/provider/mail and SQL integration assertions for absence,
uniqueness and pre-work refusal. RED is staged pending-row/start-admission behavior.

### 7. Final signup completion and interrupted-claim recovery

**Reuses:** Cycle 6 bound proof and cycles 1–5 foundations; extend the existing TenantProvisioning and tenant session owners once.

**Maps to:** REQ-012, REQ-014, REQ-017; AC-005, AC-007.

Consume cycle 6's actual proven local/external proof through the one final
`TenantProvisioning` claim → initialization → promotion path. A successful
completion yields the ordinary appropriate tenant session; completion clears
attempt email while the separately verified tenant contact remains. Lost-response,
replay and concurrent completion return the same successful tenant with no foreign
claim adoption. Reuse cycle 3's retention/accounting and cycle 4's password owner.

Interrupt a real claim in `creating`, advance beyond existing staleness bound and
resume only with that attempt's valid completion cookie. Another holder/identity
cannot adopt it; expired proof cannot restart it. Race retries across replicas:
one owned workspace and no active ownerless tenant, remaining failed/provisioning
until operator retirement if proof expires.

**Proof:** tier-1 provisioning/retry with real SQL fault/staleness seams; RED is
staged `creating` recovery behavior, not just failed-claim retry already evidenced.

### 8. Owned-workspace cap is atomic and survives proof purge

**Reuses:** Cycle 7 final claim/completion path and cycle 3 durable creator accounting; enforce quota at that same claim boundary.

**Maps to:** REQ-014; AC-005, AC-007.

At configured cap (default 3), new local-email/social identity claims refuse with
`WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`, not 429, including concurrent completions
across replicas. Failed/provisioning claims count until retired; retirement frees
capacity. Count survives attempt purge; resuming own claim and operator creation
are unaffected. Never merge identities by email to deduplicate humans.

**Proof:** tier-1 concurrent claims and SQL atomicity/retention assertions. RED is
missing creator/cap persistence and claim enforcement in staged code.

### 9. Optional Turnstile validates after admission and before claim

**Reuses:** Cycles 6–8 admitted completion and atomic claim; insert configured challenge validation before that claim, without a second completion path.

**Maps to:** REQ-014; AC-005, AC-011.

Configure through the existing operator setting and use local mock Siteverify.
Missing/invalid/expired/wrong-hostname/wrong-action/reused tokens create no tenant
and use `WYRD_AUTH_403_SIGNUP_CHALLENGE`; unreachable provider refuses creation.
Already-refused admission performs no challenge call; accepted validation checks
success, deployment hostname and `signup` action before claim. Unconfigured signup
needs no challenge, and SDK/CLI/MCP/API traffic is not challenged. UI token/refusal
and form-at-cap behavior connects cycle 16.

**Proof:** tier-1 browser completion → local Siteverify → actual provisioner;
RED is absent server-side optional challenge, not a page-only widget.

### 10. Built-in verified-email signup creates one tenant-local admin

**Reuses:** Cycles 1–9 final signup/provisioning/session machinery and existing provider/mail owners; connect local/social acquisition and contacts once.

**Maps to:** REQ-011, REQ-012, REQ-013, REQ-014, REQ-017; AC-005, AC-007.

Start with untouched settings: anonymous start advertises/refuses creation and
writes no tenant. Then enable password/signup/SMTP through real operator settings.
Through typed HTTP/Rust-client consumers request email signup; read the actual
Mailpit message, open without consuming, POST browser-bound proof, then submit
workspace name/slug and local username/password. Before proof no password is
collected and no pending User/tenant/signup row exists (bounded counters are allowed).
Promotion yields one usable workspace, verified contact, admin binding and ordinary
password session that invites a teammate; no platform principal/bootstrap key.
Claimed slug conflicts without reproving an unclaimed alternative. Repeat/replay,
race across replicas and lose verification/completion responses: one creation,
same successful tenant, no foreign claim adoption or active ownerless workspace.

Exercise social acquisition with validated Google verified email, GitHub's narrowly
scoped authenticated verified-email retrieval, and a provider lacking proof that
must complete the bound Wyrd email challenge. No SMTP permits social creation only
with provider-verified contact; social Users stay password-free. Returning/invited
social login does not gain a verification prerequisite or extra workspace.

**Proof:** tier-1 Rust/HTTP → WyrdTestServer + repository Postgres → real tenant
resource use, Mailpit and controlled providers; SQL integration for replay/claim
atomicity and RLS. Earlier cycles already supply final proof, admission, completion, cap/challenge
and retention. RED here is missing contact/acquisition integration or delivered
mail/provider variants, not a reason to rebuild earlier proven mechanisms;
staged provider-only signup is not proof of this outcome.

### 11. Setup creates a named human admin without SMTP or an IdP

**Reuses:** Cycle 7 generalized human-owner provisioning, cycle 10 contact integration and cycle 4 password validator; reuse existing operator setup/device flows.

**Maps to:** REQ-002, REQ-014; AC-008.

Use the real setup command against a fresh installation with admin username and
hidden prompt or secret stdin; optional email remains unverified. The returned
login URL/username allows password sign-in and explicit approval of a real CLI
device code, followed by actual scoped SDK resource access. Keep separate
bootstrap automation/recovery output, never echo the password. Noninteractive
missing human inputs claim nothing. Completed rerun neither duplicates admin nor
resets password; interrupt/recover provisioning without activating ownerless.
Machine-only administrative creation may still omit human-admin input.

**Proof:** tier-1 installed server/setup → real Rust auth/device → saved CLI/client,
repository Postgres and existing platform provisioning journey. RED is setup's
absent required human-admin input/result; use existing lifecycle fault seams.

### 12. Verified contacts and discovery return bounded workspace links only

**Reuses:** Cycles 10–11 tenant contacts, cycle 6 bound proof, cycle 3 HMAC/maintenance and cycle 5 methods; add authenticated contact verification/replacement and bounded discovery/revalidation.

**Maps to:** REQ-011, REQ-017; AC-009, AC-010.

After setup or copied invitation join, freshly authenticate an allowed method,
request/verify a contact from actual mail, then replace it: atomically advance
contact revision/directory intent, notify the old verified address, preserve
identity/grants/password and invalidate old-contact reset proofs. Join remains
usable without verification and SMTP; recipient metadata never verifies contact.

Create tenant-local Users/verified contacts through cycles 10–11 and authenticated
contact verification; allow duplicate email in/across tenants. On a new browser,
request known/unknown addresses and observe identical generic response and neutral
mail with no membership names. Consume the actual browser-bound proof to page
bounded deduplicated same-origin active login links. Select a link and still
require ordinary tenant authentication with independent passwords/grants. No
principal IDs, usernames, session, grant, User or identity link is returned/created.

Use actual tenant-contact revision notifications/reconciliation, then change/revoke
contacts, disable/delete Users, retire tenants, leave stale projection entries,
try cross-tenant references and interrupt validation. Current validation reveals
none of those associations, even before reconciliation; lag may omit a new result.
Other-browser, expired, wrong-purpose and unauthorized replayed proof grants no
directory read. Resubmission after a lost verification response may restore the
same browser's original read cookie/result within its original deadline, without
a second proof consumption, as REQ-017 permits; it grants no new read authority.
Without SMTP discovery is absent; direct keys/URLs and live-session enumeration
remain usable independently. Verify HMAC-only minimal projection and forced RLS.

**Proof:** tier-1 Rust/HTTP → real mail proof → directory → tenant login; SQL
integration for projection/RLS, reconciliation and unavailable-validation refusal.
RED is absent authenticated contact request/verify/replace, discovery proof/
pagination or bounded projection/revalidation. Build the contact revision and
directory-update owner once here; recovery below consumes its output.

### 13. Verified contacts support private self-service reset

**Reuses:** Cycle 12 completed contact verification/replacement and cycles 10–12 proof/directory intent, plus cycles 1–5 admission/password/methods; reuse reset redemption.

**Maps to:** REQ-002, REQ-013, REQ-015, REQ-016, REQ-017; AC-010.

Reuse cycle 12's completed authenticated contact request/verify/replace flow and
its current contact revision/directory intent. This cycle adds private reset
request/send integration and reuses existing redemption; it does not rebuild
contact verification or its revision owner.

At a password-permitting workspace, request reset with tenant + username for
known/unknown/unverified/ineligible Users. Compare generic responses and timing
distributions, including recipient throttling and relay failure; requests do not
lock accounts or disclose whether mail was sent. Source throttles may return 429.
Redeem a real mailed manual-input code: change only existing password, invalidate
local browser sessions/CLI renewal, preserve keys, notify contact and require login.
Expired/replaced/replayed/wrong-purpose/old-contact codes refuse. Social-only Users
cannot add passwords by reset; required SSO has no bypass. Bounded transient send
loss requires a new request, not automatic retry. Offline administrator-issued
reset still works; removing SMTP does not invalidate issued codes.

**Proof:** tier-1 Rust/HTTP/Mailpit → redemption → real local session/device renewal
and key use, plus tenant SQL one-use/contact-revision integration. RED is missing
reset-request/send isolation; existing contact and reset redemption are reused and regressed.

### 14. Platform operators obtain isolated browser sessions and re-authenticate

**Reuses:** Cycles 1–5 shared source/admission/password/method foundations and existing PlatformLogin/PlatformLoginRequest/platform principal and browser-session owners; add only the approved platform purpose.

**Maps to:** REQ-018; AC-013; preserves REQ-011, REQ-017.

Use the existing authorized platform principal/credential setup to register a
platform `user`; no anonymous enrollment. Through real Rust HTTP consumers sign
in with controlled platform OIDC, retaining pre-registration/issuer-subject
pinning, or a 15+ character platform-user password without an IdP. Unknown subjects,
invalid proof and a bootstrap key in the browser login path refuse without a
session. Verify regenerated opaque cookies with host-only Secure/HttpOnly/
SameSite=Lax and `Path=/platform`; Node and browser metadata receive no API token,
bootstrap key or provider token. Tenant cookies refuse platform operations and
manually replayed platform cookies refuse tenant routes and enumeration.

Across replicas, exercise 15-minute idle and eight-hour absolute expiry, activity
that cannot extend absolute expiry, logout, suspension, password/connection
revision invalidation and store-unavailable refusal. Try retiring a workspace and
changing providers/SMTP/signup with stale, other-operator or tenant proof: no
mutation. Fresh same-operator password or forced-fresh platform IdP proof within
the shared five-minute window permits the authorized action. IdP policy owns MFA;
CLI/API break-glass stays available separately and platform authority grants no
tenant resource access. Preserve ordinary API-token consumers.

**Proof:** tier-1 Rust/HTTP → existing platform login/session owners → repository
Postgres and controlled IdP, with SQL forced-RLS/expiry/revocation integration.
RED is missing platform browser purpose/local credential/fresh-proof enforcement;
current platform API-token login is reused, not presented as browser proof.
Production browser and host-routing proof follows cycle 16 after mock approval.

### 15. Extend and approve the complete mock before production pages

**Reuses:** Cycles 5–14 final contracts, journeys and platform session metadata; reuse the already approved signup visual direction.

**Maps to:** REQ-002, REQ-011, REQ-012, REQ-013, REQ-014, REQ-017, REQ-018;
AC-004, AC-005, AC-007–AC-013.

**Prerequisite:** extend the design mock to cover email signup, verification,
workspace discovery, reset request and setup, plus `/platform` entry and
`/platform/ui/*` sign-in, re-authentication, console/provider/SMTP/signup views,
and obtain maintainer approval of those pages **before production UI implementation**. The current signup direction
is already approved: narrow tenant sign-in card, disable/replace creation form at
the workspace cap, stable challenge-refusal code, root “Create a workspace” link
only when usable, and distinct “Create a workspace” / “Join with an invitation”.
Do not request reapproval of that settled direction. This planning change does not
edit the mock; extension/approval are future work owned by this cycle.

**Proof:** review the extended mock against advertised contracts and AC-013,
record maintainer approval before any new production page. This planning edit
neither changes the mock nor claims that future visual approval has occurred.

### 16. Deliver approved onboarding and platform console pages

**Reuses:** Cycles 1–14 final Rust outputs and cycle 15 approved mock; render the complete onboarding and platform surfaces once.

**Maps to:** REQ-002, REQ-011, REQ-012, REQ-013, REQ-014, REQ-017, REQ-018;
AC-004, AC-005, AC-007–AC-013.

After approval, drive production root/key/discovery/create, email-proof, workspace
confirmation, tenant login/invitation/device/contact/reset and platform console pages through Rust
outputs from earlier cycles. Render only advertised password/company/social/mail
methods, explain missing SMTP/offline recovery honestly, and support normalized
passphrases/paste. Opening a proof link consumes nothing; POST then clear its
fragment. Show quota/challenge refusals and recoverable retry without duplicate
creation. The local/signup-created admin invites and enrolls a teammate, returns
to the workspace, then explicitly approves CLI device login; no extra workspace
for invitation join, Node auth machine, token exchange or secret storage.

Carry TASK-003 cycle 6's open signup pages, connected UI journey and docs in full:
its provider creation journey still covers disabled/invalid slug/existing slug,
provider policy race, shared refusal/unavailable state, replay/concurrency,
initialization failure, owner-only retry and lost response → returning login.
Extend user/operator setup/deployment guides with secret stdin, safe reruns,
private defaults, missing SMTP, verified-contact/discovery/recovery limits and
create-versus-join wording. Ship equivalent HTTP/CLI/SDK projections where exposed;
MCP carries safe metadata/authorized management with explicit write scopes, never
password/reset secrets. Keep human proof/device approval in its auth channel.

**Proof:** tier-1 production browser → Rust → repository Postgres/Mailpit/provider
fixtures and actual CLI/client consumers. New page/journey missing-flow assertions
must fail before production edits; existing signup server evidence is reused, not
claimed as new RED. Documentation/contract checks complement journeys.


Deliver the platform entry headed **Platform admin**, platform sign-in and
same-operator re-authentication using cycle 14's Rust-owned session and safe
metadata; tenant headings remain **workspace admin**. Carry every open operator
provider/SMTP/settings view from TASK-003 cycles 1 and 3 here, using existing
test-provider/test-send output with secret omission and fresh re-authentication
for mutations. Prove the official host routes exact `/platform` and
`/platform/ui` plus its subtree to the BFF before its Rust `/platform/*` matcher;
existing APIs including `/platform/browser/*` retain their Rust responses. The
BFF forwards only the platform cookie, never a global key or tenant cookie.
This is the only new-page implementation cycle; later identity/policy cycles
extend these approved pages for their respective final behaviors.

### 17. An existing User explicitly connects another sign-in method

**Reuses:** Cycles 4–5 password/method owners, cycle 16 approved account pages and existing bound identity linking; preserve the same User/grants.

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

### 18. Removing or disabling a method invalidates its original credentials

**Reuses:** Cycle 17 linked methods, cycle 5 policy/availability contract and existing browser/CLI anchors; extend their original invalidation checks.

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

### 19. A tenant adopts company SSO without a password or social bypass

**Reuses:** Cycles 5 and 17–18 methods, linking and invalidation plus existing company-connection lifecycle; extend the same policy owner and pages.

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
production UI, against real policy persistence. Initially fail on the missing
human adoption flow and any unconnected completion/session/renewal consumers;
reuse cycle 5's final evaluator and cycles 17–18's anchors/invalidation rather
than introduce another policy implementation. If those checks already pass,
record their connected regression proof without inventing RED. Hidden buttons
alone do not pass.

### 20. Delivered surfaces and real providers complete the same journey

**Reuses:** All earlier cycles and completed TASK-003 outputs; verify the connected shipped surfaces and real-provider compatibility without rebuilding them.

**Maps to:** REQ-002, REQ-011–REQ-018; AC-004–AC-013 and the spec's shipped-surface safeguards.

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

Local SMTP requires no hosted provider credential; real Google/GitHub smoke
remains distinct and must honestly record unavailable external access. This
cycle verifies the operator views delivered in cycle 16; it builds no second
provider/SMTP/settings UI and introduces no new credential channel.

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

Revision 7 resolves the inherited operator UI credential-channel decision with
REQ-018 and AC-013. All eight onboarding decisions, including discovery, remain
settled.
Implementation chooses private names,
helpers, representations and fixture seams within the approved owners. A new
material contract/persistence purpose returns `SPEC_REVISION_REQUIRED`; missing
external smoke credentials are evidence limitations, not a reason to redesign.
This carryover contains no new implementation or execution result, and does not
authorize staging, committing or pushing.

Planning validation: the completed TASK-003 cycles remain dependency evidence;
all open cycle-6 work, revision-5 controls and original cycles 7–10 are owned here.
No code, migration, test, mock, stage, commit or push is part of this planning edit.
Mock extension approval and live-provider smoke access are future execution gates,
not unresolved product decisions. Platform sign-in semantics and route ownership
are approved; mock appearance approval and live-provider evidence remain future
execution gates. Nothing ships until every cycle is complete.


Documentation handoff (2026-11-21): the bounded independent comprehension pass
covered the spec and carryover together. Its forward-dependency and replay-recovery
wording findings were repaired; no new approval or implementation-readiness verdict
was issued. `git diff --check` passes. `mise run docs:check` was attempted but
stopped in `docs:install`: pnpm could not open its database. A temporary XDG-state
retry could not resolve tool downloads with restricted network access; no docs
build/test pass is claimed. The documentation gate remains an execution-environment
limitation, not an unresolved platform product decision.

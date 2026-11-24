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

**Evidence (cycle 6):** RED: `platform_admin_e2e::signup::signup_start_keeps_its_round_trip_in_a_sealed_cookie`
first failed because a social start left one `platform.signup_attempts` row
("a start writes no row", left 1). The second RED,
`platform_admin_e2e::signup::email_signup_proves_a_mailbox_in_the_browser_that_asked`,
failed because an email start answered `401 "email signup is not available"`.

GREEN came after these changes:
- Unapplied migration 000006 was revised in place. `platform.signup_attempts`
  now holds only proven attempts: `proof_kind` is `email` or `social`, and the
  table is unique on `proof_hash` (the state or identifier digest). A social
  proof requires provider, revision and identity; an email proof requires its
  address. Admissions gain an `address` scope.
- `SocialSignIn` seals a `SignupEnvelope` (`signup_social` or `signup_email`)
  under the deployment keyring into the cookie. The deadline comes from the
  database clock (`signup_clock`) and is checked before provider IO.
- Social: `wyrd_login`, path `/auth/social`, five minutes, carrying state, PKCE
  verifier, nonce, revision, redirect and binding.
- Email: `wyrd_signup_email`, path `/auth/signup`, fifteen minutes, carrying the
  proof's digest, canonical address and binding, never the proof.
- `insert_proven_signup` uses `ON CONFLICT (proof_hash) DO NOTHING`, so a replay
  or a concurrent return records nothing more. `renew_signup_completion` lets
  only the proving browser recover a lost response.
- `ApplicationMail::send_signup_proof` mails a fragment link
  (`/signup/verify#proof=…`) plus a code to paste.
- `EmailAddress::parse` trims and lowercases the domain only. Unit test:
  `mail::tests::addresses_compare_by_local_part_and_lowercase_domain`.
- New route `POST /auth/signup/verify` returns `{status:"proven"}`; an email
  start returns `202 {status:"check_email"}`.

The social journey proves these cases:
- no row after a start, and the cookie hides state and nonce
- a tampered cookie proves nothing
- two concurrent callbacks with one cookie record exactly one proof, and a
  replay records none
- a throttled start makes no provider discovery request
- admission state that cannot be read (table renamed) refuses with `503`,
  before discovery

The email journey proves these cases:
- without a relay, email signup is not offered and nothing is mailed
- a start stores nothing; delivery goes to the trimmed address as typed
- a wrong proof, or the proof without the flow cookie, gets
  `401 WYRD_AUTH_401_ONE_TIME_CODE_INVALID`
- the asking browser records one `email` proof under the canonical address
- a repeat from the same browser gets a fresh completion cookie and no second
  proof
- a later flow for the same address cannot use the spent proof
- the address budget refuses a start from fresh sources before SMTP: mail
  count equals admitted starts

`signup_state_is_pseudonymous_and_purged_in_batches` now seeds the
proven-attempt shape. Its assertion "both live pending starts remain" becomes
"starts stored no attempt to keep" (0), because starts no longer write rows.

The replica claim rests on shared PostgreSQL. The one-use digest is a database
unique constraint, and the concurrent-callback race ran on one server.

Completing an email proof is refused (`InvalidState`) until cycles 7 and 10
consume it. **TDD: PASS.** Commands:
- `cargo nextest run --locked -p wyrd-server --features test-support --test integration -E 'test(/^platform_admin_e2e::signup::/)'`
- `cargo nextest run --locked -p wyrd-auth -p wyrd-sql -p wyrd-server --features wyrd-server/test-support --lib --test integration -E 'kind(lib) | test(/^platform_admin_e2e::/) | test(/^pg_browser_sessions::/) | test(/social|login|openapi|signup/)'` (1010/1010)

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

**Evidence (cycle 7):** RED — `platform_admin_e2e::signup::completion_consumes_either_proof_and_recovers_its_own_claim`
(written whole before production code) first failed when the completion cookie answered
`Max-Age=1800`, not the five-minute window. Its later steps were not run RED separately: the
staged `creating` recovery was then mutation-checked. With the stale takeover in
`start_signup_creation` disabled, step 5's own-cookie resume fails `409 signup_in_progress`.
GREEN once five changes landed:
- `SIGNUP_COMPLETION_TTL` and the cookie became five minutes.
- `SignupCompleteRequest.local_credentials` is checked by `admit_signup` before admission or
  claim. An email proof requires it and a social proof refuses it, both
  `400 WYRD_SPEC_400_VALIDATION`, field `local_credentials`. The password goes through the
  cycle 4 `new_password`/`hash_password` owner.
- `SignupCredential` (social identity | local username/verifier) flows through the one
  `HumanOwner` → `claim_for_signup` → `provision_for_signup` path. That path creates a linked
  OIDC User or a local password User bound to tenant `admin`. On resume it keeps the earlier
  User, and a local one takes the retry's verifier.
- `open_signup_session` opens a social- or password-anchored session.
- `finish_signup` clears the attempt email on completion. The 000006 CHECK admits a null email
  only for `completed` rows (spec "Email proofs last fifteen minutes, signup completion proofs
  five minutes…" and REQ-012 one-use completion). `start_signup_creation` takes over its own
  `creating` row once it has made no progress for the existing 15-minute provisioning
  staleness bound, and only while the proof is live.

The journey asserts:
- each refusal creates no tenant;
- the email proof creates an active `harbor`;
- a password login as `ada` holds `wildcard:wildcard`;
- the attempt row reads `completed` with a null email;
- a replay answers `409` naming `harbor` with no session and no second row;
- a recent `creating` claim reads as in progress;
- a stale one refuses another proof, then resumes under its original tenant id with its own
  cookie;
- an expired proof answers `400 WYRD_AUTH_400_INVALID_STATE` and leaves its claim
  `provisioning`.

Cross-replica race and replay are covered by the existing
`a_visitor_creates_a_workspace_with_google` steps 4–6, still green.

Interpretation: "lost-response, replay and concurrent completion return the same successful
tenant" is read against REQ-012 one-use completion. A replay or the losing racer answers `409`
with `details.tenant` naming the one created workspace, and the creator recovers by signing in.
No second completion succeeds.

**Spec revision (maintainer-approved):** the five-minute completion proof always expired
before the old 15-minute staleness bound, so recovery was unreachable. The spec's
"Crash recovery" bullet now sets a two-minute signup staleness bound. It is applied in
`start_signup_creation` and in `claim_signup_tenant`'s adoption check. Operator
`insert_provisioning_tenant` keeps 15 minutes. The journey stages a claim three minutes old with
a live proof. Follow-up: a resumed local creation used to look its owner up by the retried username, so
a retry under another name added a second administrator. It now keeps the tenant's one local
User (`first_local_user`). Step 4b reopens the completed `harbor` as interrupted and retries as
`grace`. It asserts the only local User is still `ada` and that the retry's password signs
`ada` in. The fix preceded this assertion, so this sub-case is a **TDD process violation**.
Tenant contact retention joins cycle 10, which owns verified contact. **TDD: PASS** for step 1,
with mutation proof for step 5. Commands: the selector above, and
`cargo nextest run --locked -p wyrd-auth -p wyrd-sql -p wyrd-server --features
wyrd-server/test-support --lib --test integration -E 'kind(lib) | test(/^platform_admin_e2e::/)
| test(/^pg_browser_sessions::/) | test(/social|login|openapi|signup/)'` (1011/1011), clippy
`-D warnings`, `codegen:check`, `check:tenant-isolation`.

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

**Evidence (cycle 8):** RED — `platform_admin_e2e::signup::the_owned_workspace_cap_holds_atomically_across_purge`
first failed because `GET /platform/login-settings` had no `signup_workspace_cap`. GREEN after
these changes:
- Migration 000006 (unapplied) adds `platform.login_settings.signup_workspace_cap` (default 3,
  1–65535) and an index on `platform.tenants.signup_creator_key`, per spec "revision 5 adds the
  owned-workspace cap".
- `LoginSettings.signup_workspace_cap` is a `NonZeroU16` that defaults to 3 when omitted.
- `PUT /platform/login-settings` now maps a bad body through the shared `body_rejection`, so a
  cap of 0 answers `400 WYRD_SPEC_400_VALIDATION` instead of axum's bare `422`.
- `claim_signup_tenant` takes a per-creator `pg_advisory_xact_lock`, then admits only the
  attempt's own existing claim, or a creator below the cap. The count covers tenants with that
  creator key whose status is not `deleted`.
- The cap refusal is the new catalog error `WYRD_AUTH_403_SIGNUP_WORKSPACE_LIMIT`, carried
  through `ProvisionError::WorkspaceLimit`. The TS error codes and the docs error page were
  regenerated.

The journey asserts, with cap 1:
- a second claim gets `403` and creates nothing;
- the refusal holds after the attempt rows are deleted, and while the claim is `failed`;
- retiring the tenant (status `deleted`, set in SQL because no operator retirement surface
  exists yet) frees the slot;
- two completions of one identity raced on the server and a replica give `[200, 403]`, with
  one tenant;
- an own failed claim staged at the cap resumes to `active`;
- an email identity at the same address creates its own workspace, so there is no merging by
  email;
- operator `POST /platform/tenants` succeeds.

The login-settings shape assertion in `an_operator_configures_and_tests_google_beside_company_sso`
was extended to the new field. **TDD: PASS.** Commands: the cycle 7 lane command
(1012/1012 after the shape update), clippy `-D warnings`, `codegen:check`, `docs:check`,
`check:tenant-isolation`.

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

**Evidence (cycle 9):** RED — `platform_admin_e2e::signup::a_configured_challenge_gates_browser_creation`
first failed with `PUT /platform/login-settings` refusing the unknown field
`signup_challenge`. Before that I added only the test-config field
`SocialConfig.turnstile_siteverify_url`, so the test could compile. GREEN after these changes:
- `LoginSettings.signup_challenge {site_key, secret}` is accepted through the existing
  login-settings operation. The secret is write-only, sealed with the deployment keyring, and
  kept when a later write omits it. Writing `null` clears the challenge.
- Migration 000006 (unapplied) adds `signup_challenge_site_key` and `signup_challenge_secret`
  under a both-or-neither CHECK, per spec "the optional challenge site key with sealed secret".
- The new `wyrd_auth_oidc::turnstile::Turnstile` Siteverify client goes through the screened
  transport.
- `SocialSignIn::require_challenge` runs in `signup_complete` after `admit_signup` and before
  the claim. A refusal settles the attempt `failed` so its owner can retry.
- The new catalog error `WYRD_AUTH_403_SIGNUP_CHALLENGE` was added, and the TS codes and docs
  were regenerated.

The journey asserts:
- the stored secret is never returned and is not stored in plain text;
- a missing token, `invalid-input-response`, `timeout-or-duplicate` (already used), a wrong
  hostname and the `login` action each answer `403` and create nothing;
- a missing token makes no Siteverify call;
- a `503` from Siteverify refuses with `503 challenge_unavailable`;
- a valid token creates an active workspace and posts the stored secret, the token and the
  visitor's address;
- a source-less completion refused at admission makes no call;
- operator `POST /platform/tenants` is never challenged;
- clearing the setting removes the challenge.

Unit: `turnstile::tests::{siteverify_receives_the_form_and_decodes_a_refusal,
an_unavailable_siteverify_is_an_error}`; that client was written before those unit tests (TDD
process violation for the unit tier only). The expired-token case is Siteverify's same
`timeout-or-duplicate` refusal. The UI widget and the form-at-cap behavior join cycle 16.
**TDD: PASS** (journey). Commands: the cycle 7 lane command plus `-p wyrd-auth-oidc` and
`config` (1067/1067 after the login-settings shape update), clippy `-D warnings`,
`codegen:check`, `docs:check`, `check:tenant-isolation`.

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

**Evidence (cycle 10):** RED: `platform_admin_e2e::signup::every_signup_creator_gets_a_verified_contact`
failed at its first contact read because `wyrd.auth_user_contacts` did not exist. GREEN after
these changes:

- Migration `20261121000008_verified_contacts.sql` adds the revision-6 "tenant-local verified
  recovery contact" table. It leads with `data_tenant_id`, has a tenant-qualified FK to
  `auth_users`, forced RLS, and the tenant and operator policies. It has no address uniqueness,
  because duplicates are allowed.
- `SignupCredential` now carries the verified contact. Provisioning writes it in the
  User-creation transaction, so a resumed creation finds it already committed.
- At acquisition, Google keeps its email only under a validated `email_verified: true`.
- GitHub signup alone requests `user:email` and takes a verified address from `/user/emails`,
  primary first. Sign-in and invitations still request no scope; the existing GitHub
  invitation journey still passes.
- A provider attempt without a verified email refuses completion with `409 contact_required`
  and creates nothing.
- The same browser can start the email branch while holding its completion cookie. This needs
  signup and a relay, not password. Verification records the mailed address on that
  still-`proven` attempt once (`attach_signup_contact`) and rotates its completion.

The journey asserts:
- Google-verified contact without SMTP;
- an unverified Google email is refused with no tenant, and there is no email branch without a
  relay;
- with a relay, the bound branch creates the workspace with the canonical mailed address as
  contact, no password and the Google identity;
- the local admin's contact is its proven address, and its password session invites a teammate
  (`201`);
- GitHub takes the verified non-primary address over an unverified primary, and an account
  with no verified address gets `contact_required`.

Reused, not rebuilt: default-off advertisement and anonymous refusal, no pre-proof row,
password after proof, slug conflict, replay/race/lost-response single creation, and no foreign
claim adoption (cycles 6–8 journeys, still green in the lane). Returning and invited social
login gained no verification prerequisite (`an_invited_person_enrolls_with_github_and_returns`
and the Google login journeys, green).

Limitation: the Rust-client consumer for signup is not added. The journeys drive HTTP
directly. That is the same MCP/client projection gap cycle 7 recorded.

**TDD: PASS**. Commands: the cycle 9 lane command plus `github` (1138/1138; one earlier run
failed only because the test Postgres container ran out of storage and was restarted), clippy
`-D warnings`, `codegen:check`, `docs:check`, `check:tenant-isolation`.

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

**Cycle 11 evidence.** Journey
`platform_admin_e2e::setup::setup_creates_a_named_human_admin_without_smtp_or_an_idp`
(`crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e/setup.rs`), committed red in
b7206eafe.

- **RED:** step 1 (`setup.rs:180`): `wyrd-server setup --tenant acme` without
  `--admin-username` succeeded. That is the missing behavior, not a setup failure.
- **GREEN:** the same selector passes (1/1). It proves each case:
  - missing username (clap), no terminal without `--admin-password-stdin`, and a policy-refused
    password all fail, and `acme` has no directory row (`setup.rs:177-188`);
  - an activation fault leaves `acme` inactive with at most one User. The rerun with
    `WYRD_PLATFORM_CREDENTIAL` gives one active tenant, one admin-role User `founder` whose
    profile email is unverified and who has no `auth_user_contacts` row, prints
    `admin_credential`, `admin_username` and `login_url` (`/t/acme/login`), and never
    prints the password in any output (`setup.rs:190-250`);
  - password sign-in approves a real `wyrd-cli` device code, and the redeemed token reads
    `founder` from `/v1/principals/me` (`setup.rs:252-309`);
  - a completed rerun prints "already set up", discloses no key, adds no admin and keeps the
    original password (`setup.rs:311-327`);
  - machine-only `POST /platform/tenants` returns no `human_admin` (`setup.rs:329-345`).
- **Implementation:**
  - `CreateTenantRequest.human_admin: Option<TenantHumanAdmin>` and
    `CreateTenantResponse.human_admin: Option<ProvisionedHumanAdmin>` (`wyrd-spec`
    `tenant_admin.rs`). The response never carries the password.
  - `create_tenant` validates through `LocalAdmin::admit` before the claim:
    `canonical_username`, `new_password`, the shared `is_email_address` (extracted from
    `invitation_recipients`), then `hash_password`. Only a request with a human admin needs
    `RequestOrigin`.
  - The operator path (`TenantOwner::Bootstrap(Option<&LocalAdmin>)`) inserts the User and its
    admin grant in the bootstrap admin's tenant transaction. A resume keeps `first_local_user`
    without resetting its password. Activation stays last.
  - `insert_local_user` takes `email: Option<&str>`, and its 3 existing callers pass `None`.
  - Setup gains `--admin-username` (required), `--admin-password-stdin` and
    `--admin-email`. Input is checked with `LocalAdmin::check` before `root_credential`.
    Without the stdin flag, setup prompts with echo off (rustix `termios`, already in the
    resolved feature set; POSIX on macOS and Linux) and refuses when stdin is not a terminal.
- **Reuse:** the provisioning owner, its resume path, the password policy, the device flow and
  `RequestOrigin` are reused. There is no new owner and no migration.
- **Diagnosis (lane regression, fixed):**
  - Symptom: 40 `platform_admin_e2e` tests got `400` from `POST /platform/tenants`.
  - Evidence: the request metric shows `status="400"` with no `create_tenant` span. The test
    helper `platform_post` sends no `Host`.
  - Cause: the new `RequestOrigin` extractor refused the request before the handler ran.
  - Fix site: `create_tenant` takes `Result<RequestOrigin, _>` and requires it only for a
    human admin. No other caller is affected.
- **TDD: PASS.**
- **Commands:**
  - the selector above;
  - `cargo nextest run --locked -p wyrd-auth -p wyrd-sql -p wyrd-auth-oidc -p wyrd-cli -p
    wyrd-server --features wyrd-server/test-support --lib --test integration --no-fail-fast -E
    'kind(lib) | test(/^platform_admin_e2e::/) | test(/^pg_browser_sessions::/) |
    test(/social|login|openapi|signup|config|github/)'` (1232/1233 before the gateway test fix below);
  - `mise run fmt`, `mise run lints`, `codegen:check`, `docs:generate` (no diff),
    `docs:check` and `check:tenant-isolation` are clean.
- **Lane regression outside the write set (fixed):**
  - Symptom: `components::gateway::pg_administration_tests::gateway_managed_secret_renewal_keeps_the_key_and_loses_races`
    failed in both lane runs and in 1 of 3 isolated runs at `pg_administration_tests.rs:639`.
    The renewal returned `Ok` instead of losing to the replacement.
  - Evidence and cause (read-only diagnostician, from source and trace): the renewal's reseal
    and the replacement's seal both wait on the same 300 ms delayed transit fixture. The
    replacement starts right after the renewal's encrypt arrives, so the commit order depends
    on timer and wakeup order. Nothing in the test fixes it. The compare-and-swap in
    `renew_gateway_credential` is correct. Cycle 11's diff is not involved; the leg came in
    with 7c935655a.
  - Fix site: the test only. The replacement seals through a second, undelayed
    `TransitFixture`, so it commits inside the renewal's in-flight 300 ms, as the revoke leg
    already does. The sibling races that use `start_delayed` race writes that make no key-service
    call, so they stay deterministic.
  - Result: 6/6 isolated runs pass.

After cycle 11: cycles 12–20 as listed below; cycle 15 is the UI mock gate.

Notes for the final report:
- TDD violations so far: cycle 7 step 4b; the turnstile unit tests were written before the
  journey RED.
- The 409-replay reading of "same tenant".
- No tenant retirement surface exists; tests retire tenants through SQL.
- MCP and `wyrd-client` projections for signup are missing.

Environment: the test Postgres runs from `scripts/postgres/with-test-postgres.sh`. Its container
ran out of storage once during cycle 10 and was restarted. If tests fail with `PoolTimedOut`,
check `docker logs` first.

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

**Cycle 12 evidence (in progress).**

- Scenario 1, authenticated contact request, verify and replace:
  `platform_admin_e2e::contacts::a_signed_in_user_verifies_and_replaces_a_recovery_contact`
  (`crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e/contacts.rs`).
  It covers these cases:
  - without a relay, a request is refused with 403 `METHOD_UNAVAILABLE` and nothing is mailed;
  - a request with no session gets 401, and a session older than five minutes gets 403 `RECENT_LOGIN_REQUIRED`;
  - a fresh session gets 202 `check_email`, a flow cookie that does not hold the proof, and a mail with `#proof=`;
  - a wrong proof, or the right proof without the flow cookie, gets 401 and stores nothing;
  - the right proof gives 204 and stores the contact at revision 1; repeating it gives 204 with no change;
  - a replacement stores revision 2, a flow begun before it gets 401, and the old address receives one notice with no proof and no new address;
  - the password still signs in.
  - RED: 401 from the authentication middleware for the unmatched route `/auth/browser/email/request` (the route did not exist).
  - GREEN: 1 passed.
  - A first GREEN run panicked in the test's own body reader on the empty 204 body. I changed the local `browser` helper to read bodies leniently, the way `social_call` does. No assertion changed.
  - Before GREEN I dropped my own unrun assertion that the contact mail omits the slug. REQ-017 forbids workspace names only in discovery mail, and the contact link is `/t/{slug}/contact/verify`.
- Implementation:
  - A `ContactEmail` purpose in the sealed envelope carries the proof digest, tenant, User, address, the contact revision the flow began against, and a deadline. Nothing is stored before proof.
  - Verification is a compare-and-swap on that revision under `FOR UPDATE`, so each proof works once and a flow begun before a replacement verifies nothing. A lost-response replay succeeds only when the current contact is exactly that flow's result.
  - Owners: `SocialSignIn` (`wyrd-auth/src/social/contacts.rs`), the `verified_contact_for_update` and `replace_verified_contact` queries, and two `ApplicationMail` messages. `send_signup_proof` now shares their send path.
  - Routes: `POST /auth/browser/email/request` and `/verify`.
  - My reading: verify needs a live session of the same User, not a recent one, because the proof itself lives 15 minutes.
- First-contact race (raised by the commit hook before the first commit): `FOR UPDATE` locks nothing when no contact exists, so two flows begun at revision 0 could both succeed. `replace_verified_contact` now updates only `WHERE revision = expected` and returns `None` otherwise.
  - Proof: `pg_auth_contacts::pg_tests::contact_replacement_takes_effect_only_at_its_expected_revision` (`crates/wyrd/wyrd-sql/tests/integration/pg_auth_contacts.rs`).
  - Command: `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_auth_contacts::pg_tests::contact_replacement_takes_effect_only_at_its_expected_revision)"'`. GREEN: 1 passed.
  - Mutation check: without the guard it fails with `(Some(1), Some(2))` against `(Some(1), None)`.
  - TDD VIOLATED for this fix: the guard was written before its test.
- Scenario 2, bounded discovery with revalidation:
  `platform_admin_e2e::contacts::a_visitor_discovers_only_live_workspaces_for_a_verified_address`
  (same file). It covers these cases:
  - without a relay, a request gets 403 `METHOD_UNAVAILABLE`;
  - verified contacts reach the directory within the propagation bound (`within_propagation`);
  - known and unknown addresses both get exactly `202 {"status":"check_email"}` and a binding cookie, and the mail names no workspace;
  - the proof without its binding cookie gets 401;
  - in its own browser it gives 200 with exactly `tenant`, `display_name` and `login_url` per workspace. The links are fixed same-origin `/t/{slug}/login` paths, in route-key order, with no `next_cursor`, and a read cookie is set. An unknown address gets an empty list;
  - the same browser repeating a spent proof gets the same page;
  - replacing one contact, suspending one User and retiring one tenant (through SQL; there is no retirement surface) each drop that workspace at once, before reconciliation;
  - a listed workspace still needs its own sign-in (401 without a session).
  - RED: 401 from the middleware for the unmatched route `/auth/browser/workspaces/request`.
  - GREEN: 1 passed.
  - A first GREEN attempt saw an empty directory. Cause: an unbound `WyrdTestServer` starts only `authorization_cache` and leaves the other workers for `bind()`, so the new `workspace_directory` worker never ran under `oneshot`. Fix site: `WyrdTestServer::propagate_authority` (`crates/wyrd/wyrd-testing/src/server.rs`), which now also starts `workspace_directory`. Callers: every unbound test server.
- Implementation:
  - Schema: unshipped migration 0008 edited in place (spec revision 6 "Workspace discovery directory" and "Proven email … discovery … proof and one-use redemption"; maintainer approval: "nothing has shipped").
    - `platform.workspace_directory` holds tenant, User, the HMAC address key and the contact revision.
    - `platform.discovery_proofs` holds the proof digest, browser digest, read deadline and expiry.
    - Both force operator-only RLS.
    - A trigger on `wyrd.auth_user_contacts` notifies `wyrd_contact`.
  - Owners: `SocialSignIn` in `wyrd-auth/src/social/discovery.rs`, the queries in `wyrd-sql/src/queries/platform/discovery.rs`, `live_contact`/`tenant_contacts`, and a `workspace_directory` worker on the existing `NotificationListener`.
    - The worker reprojects a User on each notification and resyncs every live tenant on each (re)subscribe.
    - Each page revalidates the active User and the exact current contact and revision through the tenant's connection. Any failure omits the workspace.
    - Spent proofs are purged by the existing maintenance purge.
  - Routes: `POST /auth/browser/workspaces/request`, `POST /auth/browser/workspaces/verify` and `GET /auth/browser/workspaces?cursor=`.
- Lane regressions fixed: the full lane named below turned up two stale fixtures from merge 70a56a136, which predate operation-bundle roles. A read-only diagnostician found them; its report is summarized here.
  - `pg_approvals::a_decision_outlives_its_retry_receipt`: `editor` no longer approves (`builtin_roles.rs`), so the test now uses `["approver","viewer"]` like its siblings.
  - `pg_operator_connection_routes::renewal_keeps_revocation_and_refuses_without_authority`: a raw role row carried `"scope"`, which `Operation` refuses. The test drops it, like the same file's `:694`.
  - Both pass. Neither assertion changed.
- Not fixed, outside this task: `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees` fails intermittently (2 of 5 solo runs, and once in the lane).
  - Cause, from a read-only diagnostician: after the runner admits an attempt, its input read's Oracle admission joins the operation and waits out the 20 s execution deadline while pressure is `Critical` (`runner.rs:808-813`, `oracle/mod.rs:2342-2346`, `workload.rs:622-660`). The working-tree diff is not involved.
  - Fix site: the verification runner's nested admissions, which are on a hot path. Left for a maintainer decision.
- Scenario 3, email request budgets: `platform_admin_e2e::contacts::email_requests_are_admitted_before_any_mail_is_sent` (same file). It covers these cases:
  - one address asked about seven times always gets the same `202 {"status":"check_email"}`, but only five messages reach it (silent recipient throttle);
  - one source asking about many addresses is refused on its 21st request with `429 WYRD_AUTH_429_EMAIL_ATTEMPTS` and a `Retry-After` equal to `details.retry_after_seconds`. Nothing is mailed to the refused address, and another source is still served;
  - a signed-in User asking to verify address after address is refused on the 6th request with the same 429, and nothing more is mailed.
  - RED: `the address budget stops mail`, left 7, right 5 (every request mailed).
  - GREEN: 1 passed. The contacts and signup modules then passed twice in a row (14 tests).
  - Implementation:
    - `SocialSignIn::admit_email` (`wyrd-auth/src/social.rs`) runs before any mail, on the existing `platform.signup_admissions` windows.
    - Three new scopes: `email_source` (20 an hour), `email_account` (5 an hour), and `email_address` (5 an hour, keyed per purpose so a discovery flood cannot withhold a reset).
    - The scope CHECK is in the unshipped migration 0006, edited in place (spec "Apply separate shared source and address/account budgets, including resend, … for signup/contact verification, discovery and reset"; "Source throttles may return `429`"; maintainer approval: "nothing has shipped").
    - New stable code `WYRD_AUTH_429_EMAIL_ATTEMPTS` with `Retry-After`; projections regenerated (`codegen:regen`, UI catalog) and the errors reference updated.
    - The two request handlers take `RequestSource`. `request_contact` now takes the `ContactRequest` body (the argument limit).
  - First GREEN attempt: the earlier discovery journey failed because it asks about one address six times across contact and discovery. Cause: the address window was shared across purposes. It is now keyed per purpose. The 10-an-hour source limit then refused that journey's twelfth request from one browser; it was raised to 20 to match signup starts. The new test was adjusted before its first commit.
  - Unexplained, not reproduced: in one combined run, `platform_admin_e2e::signup::completion_consumes_either_proof_and_recovers_its_own_claim` failed once. Its panic was lost to output trimming. It then passed in 5 runs, 3 of them under clippy load. One Mailpit `connection refused` also happened once while the teammates' test stacks were starting, and passed on rerun.
- Scenario 4, a contact flow belongs to the User who began it: `platform_admin_e2e::contacts::a_contact_flow_belongs_to_the_user_who_began_it`. It covers these cases:
  - another recent User of the same tenant presenting the flow cookie and the real proof gets 401 `ONE_TIME_CODE_INVALID`, and nothing is stored;
  - the owner then completes the flow (204, one contact);
  - a session for `harbor` asking for `cove`'s contact gets 401, mails nothing and stores nothing.
  - No RED: the User and tenant checks shipped with scenario 1 and passed on first run. Mutation: dropping `user != principal.id` in `verify_contact` fails the test (204, expected 401). The code was restored.
- Scenario 5, discovery pagination: `platform_admin_e2e::contacts::a_discovery_read_pages_through_every_workspace`.
  - 23 workspaces for one address. Contacts are seeded through SQL because the per-purpose address window allows 5 verifications an hour; the projection still arrives through the real notification worker.
  - The first page lists 20 in route-key order with `next_cursor`. The read cookie and that cursor return the last 3 with no cursor. Without the read cookie the next page gets 401.
  - TDD VIOLATED: the paging code went in with scenario 2, before this test, which passed on first run. Mutation: returning no cursor from a full page fails the test (`a next cursor`). The code was restored.
- Scenario 6, purpose and read expiry: `platform_admin_e2e::contacts::only_a_live_discovery_proof_reads_workspaces`. It covers these cases:
  - a contact proof and flow cookie presented as a discovery flow get 401 and no read cookie;
  - a discovery proof presented to contact verification gets 401, and the contact is unchanged;
  - once the spent proof's read deadline passes (aged through SQL), the same browser's repeat gets 401.
  - TDD VIOLATED: the code came with scenario 2. Mutation: dropping `read_until > statement_timestamp()` from `spend_discovery_proof` fails the test (200, expected 401). The code was restored.
  - Not proven: expiry of the pre-proof sealed envelope. Its deadline is sealed in the cookie and checked by the shared `require_live` against the database clock, which no journey can advance. Signup has the same gap.
- Scenario 7, reconciliation and revalidation: `platform_admin_e2e::contacts::the_directory_heals_and_discloses_only_what_it_revalidates`. It covers these cases:
  - harbor's entry is deleted and a stale cove entry inserted, then the worker's LISTEN backend is terminated. The worker resubscribes and resyncs to exactly `["harbor"]`;
  - with `SELECT` on contacts revoked from `wyrd_tenant`, discovery lists nothing, and after the grant it lists harbor again.
  - TDD VIOLATED: the code came with scenario 2. Mutations: skipping the stale-entry removal fails the test (`["cove","harbor"]`), and treating a failed revalidation as valid fails it too. The code was restored.
- SQL structure: `pg_auth_contacts::pg_tests::discovery_tables_hold_only_digests_behind_forced_rls`.
  - `platform.workspace_directory` has exactly `data_tenant_id, user_id, address_key, contact_revision`.
  - `platform.discovery_proofs` has exactly `proof_hash, browser_binding_hash, read_until, expires_at`.
  - Both enable and force RLS. A tenant connection, each on its own transaction, is refused with exactly SQLSTATE 42501 ("permission denied"), while the platform owner writes them. An earlier draft accepted any SQL error; the commit hook flagged it and it was tightened.
  - Structural proof, so no RED applies. Command: `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E "test(/pg_auth_contacts/)"'`.
- Invalidating old-contact reset proofs landed with cycle 13 (`wyrd.auth_password_resets.contact_revision`; scenario `platform_admin_e2e::resets::a_reset_code_refuses_every_stale_or_foreign_use`), merged in c27c06f79.
- Cycle 12 status: IMPLEMENTED. Every case has a runtime or structural proof except expiry of the pre-proof sealed envelope (see scenario 6).
  - TDD VIOLATED for: the first-contact race fix, pagination (scenario 5), purpose and read expiry (scenario 6) and reconciliation and revalidation (scenario 7). In each, the code preceded its test, and a mutation check shows the test now catches the defect.
  - removing the old revision's reset proofs (the reset owner is cycle 13).
- Commands:
  - the Postgres journey command, with selector `test(=platform_admin_e2e::contacts::a_signed_in_user_verifies_and_replaces_a_recovery_contact)`;
  - `mise run fmt`;
  - `cargo clippy --locked -p wyrd-spec -p wyrd-sql -p wyrd-auth -p wyrd-server --features wyrd-server/test-support --all-targets -- -D warnings`;
  - `mise run codegen:check`;
  - `mise run check:tenant-isolation`.
  All are clean.
- TDD: PASS for scenario 1.
- Scenario 2, every stale or foreign use refuses and the offline paths stay usable:
  `platform_admin_e2e::contacts::a_reset_code_refuses_every_stale_or_foreign_use` (same file).
  It covers these cases:
  - a mailed code replaced by a newer request, then the newer one once expired (both aged
    16 minutes in SQL), each get `401 ONE_TIME_CODE_INVALID`;
  - a contact proof used as a reset code (wrong purpose), and a code mailed before the contact was
    replaced (old contact), each get `401`; the password is unchanged afterwards;
  - with company SSO required (policy row set in SQL), a request gets `403 METHOD_UNAVAILABLE` and
    a live mailed code gets `403` at redemption;
  - an operator reset for a social-only User (inserted in SQL) gets `404`, so no reset adds a password;
  - with the relay removed (SQL, as no relay-removal surface exists), a request gets
    `403 METHOD_UNAVAILABLE`, while a code mailed before and an operator-issued code both reset
    (`204`) and their new passwords sign in.
  - RED: none. The first run failed only in setup (the test's social-only insert used a retired
    `auth_type`; fixed to `oidc`). Every assertion before it already passed.
  - GREEN: 1 passed.
  - Mutation check: without the mailed-code supersession, or without the contact-revision check, the
    test fails with `204` where `401` is expected.
  - TDD VIOLATED for the replaced and old-contact cases: their production code (supersession in
    `insert_password_reset`, the contact-revision check in `password_reset`) went in with
    scenario 1, before this test. Expired, wrong-purpose, SSO, relay removal, operator offline reset
    and social-only are existing redemption behavior, proved here as regressions.
- Scenario 3, throttled without locking or disclosing delivery:
  `platform_admin_e2e::contacts::reset_requests_are_throttled_without_locking_or_disclosing_delivery`
  (same file). It covers these cases:
  - five requests for one User all get the generic `202`; once the account window shows five counted
    attempts, exactly three codes were mailed, and the password still signs in (no lock);
  - one source gets `202` thirty times, then `429 WYRD_AUTH_429_PASSWORD_ATTEMPTS` with
    `retry_after_seconds`; another source is still admitted;
  - with the relay pointed at a local listener that answers `554`, the request still gets the generic
    `202`. After the listener saw the attempt and the relay was restored, only a new request mailed a
    code (one message), that code reset, and the listener saw no second attempt (no retry).
  - The test polls Mailpit and the admission row with a 10-second deadline, because sends run after
    the answer. It has no fixed sleep.
  - RED: none; it passed on its first run.
  - GREEN: 1 passed. Mutation check: without the account window it fails with five messages against three.
  - TDD VIOLATED for the account and source windows and the bounded send: they went in with scenario 1.
- SQL integration: `pg_auth_contacts::pg_tests::mailed_resets_replace_mailed_codes_and_are_consumed_once`
  (`crates/wyrd/wyrd-sql/tests/integration/pg_auth_contacts.rs`). A mailed reset replaces the earlier
  mailed one and not the operator code, keeps its contact revision, and two racing consumptions of one
  code have one winner. GREEN: 1 passed (beside the cycle-12 test, 2/2). TDD VIOLATED: written after
  the query change.
  - Command: `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner &&
    cargo nextest run --locked -p wyrd-sql --test integration -E "test(/^pg_auth_contacts::/)"'`.
- Commands (all clean):
  - the Postgres journey command with each scenario's exact selector, and with
    `test(/^platform_admin_e2e::/) | test(/^pg_browser_sessions::/)` (90/90, the existing operator
    reset and browser-session regressions included);
  - `cargo nextest run --locked -p wyrd-auth -p wyrd-sql --lib -E 'not test(/pg_tests::/)'` (125/125);
  - `mise run fmt`; `cargo clippy --locked -p wyrd-spec -p wyrd-sql -p wyrd-auth -p wyrd-server
    --features wyrd-server/test-support --all-targets -- -D warnings`; `mise run codegen:check`;
    `mise run check:tenant-isolation`; `git diff --check`.
- Left open:
  - The timing comparison is a coarse median check (within the other classes' median, or 25 ms).
    The public path is the same code for every account, because all account work runs after the answer.
  - Social-only Users have no username, so a reset request cannot name them. The journey proves the
    operator path refuses them (`404`); the mailed path finds no local credential by construction.
  - No relay-removal or tenant-policy write surface was needed here; the tests set both in SQL.
  - MCP and `wyrd-client` projections of the reset request are not added (as for signup).

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

**Cycle 13 evidence.**

- Scenario 1, a verified contact resets a forgotten password:
  `platform_admin_e2e::contacts::a_verified_contact_resets_a_forgotten_password`
  (`crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e/contacts.rs`).
  It covers these cases:
  - three rounds of requests for a verified User, an unknown username, an unverified User, a
    suspended User with a verified contact and an unknown workspace all get `202`
    `{"status":"check_email"}`; the verified User's median answer time is within the others'
    median (or 25 ms) of theirs;
  - only the verified User's contact is mailed: three messages, each with the workspace reset page
    `/t/quay/reset` and one code to paste that is on no URL line; the suspended User's contact gets none;
  - the newest code gives `204` with no session cookie; the old browser session gets `401`, the
    CLI refresh token from a real device approval (which renewed once before) is refused, the
    personal API key still exchanges, the old password is refused and the new one signs in;
  - the contact receives one "password was reset" notice without the password; the same code then
    gets `401 WYRD_AUTH_401_ONE_TIME_CODE_INVALID`.
  - RED: `contacts.rs:710`, `401 WYRD_AUTH_401_UNAUTHENTICATED` for the unmatched route
    `/auth/browser/password/reset/request` (the route did not exist). All setup before it
    (contacts, Users, key, device sign-in and renewal) passed.
  - First run failed in setup: the test's own username `cy` is shorter than the 3-character
    minimum (`400` from `POST /v1/principals/users`). I renamed it `cyd`; no assertion changed.
  - GREEN: 1 passed; 4 more consecutive runs in one container passed.
  - In those reruns the cycle-12 journey failed at `contacts.rs:283` ("nothing was mailed")
    only because Mailpit keeps the first run's messages to the same address inside one container.
    Its first run in the container passed; a fresh container is the lane's setup.
- Implementation:
  - `POST /auth/browser/password/reset/request` `{tenant, username}` (`PasswordResetCodeRequest`).
    The handler admits the request in its per-source window (`429 PASSWORD_ATTEMPTS`), refuses
    with `403 METHOD_UNAVAILABLE` when the tenant's methods report does not offer
    `password_reset` (no relay, password off, or company SSO required), then answers `202`.
  - `SocialSignIn::request_password_reset` (`wyrd-auth/src/social/contacts.rs`) answers before any
    account read. A process semaphore (16 permits) bounds the spawned send; with none free the
    request sends nothing. The send resolves the tenant, counts the account window silently (3
    messages per hour), and mails only an active local password User with a verified contact.
    The code is issued through the existing `issue_password_reset`, bound to the password
    revision and the contact revision, and replaces the User's earlier mailed codes. Mail goes
    after the commit; a failed send is logged and not retried.
  - Redemption reuses `BrowserSessions::password_reset`. A mailed code also needs its contact
    revision to still be current, which is how a contact replacement invalidates old-contact codes.
    After the commit the verified contact is told.
  - Schema, both edited in place (unshipped, maintainer-approved): `platform.signup_admissions`
    scopes gain `reset` and `reset_account` (`20261121000006`), and
    `wyrd.auth_password_resets` gains a nullable `contact_revision` (`20261121000008`). Both cite
    the spec's revision-6 "Recovery and admission metadata" row and the "reset-contact revision
    validity" support property. No new table.
- TDD: PASS for scenario 1.

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

#### Cycle 14 evidence

Selector prefix: `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --no-fail-fast -E "test(=<selector>)"'`.

Schema (spec revision 9, REQ-018 / AC-013 platform-user password and platform
browser sessions): migration `20261123000001_platform_console_sessions.sql`
extends `platform.principals` (password verifier and revision, `user` only),
`platform.oidc_connection` (revision) and `platform.login_state` (purpose,
browser binding, re-auth session), and adds forced-RLS
`platform.browser_sessions`. Like every other `platform.*` table it has no
`data_tenant_id`: the platform plane is tenant-free by design.

Decisions: password attempts share the existing per-source `admit_sign_in`
window; "retiring a workspace" is the existing tenant suspension
(`PUT /platform/tenants/{id}/status`), as no other retirement surface exists;
fresh proof is enforced on browser-session callers only, so platform API tokens
(the CLI/API break-glass channel) are unchanged; a platform-user password counts
as a usable way in for the last-administrator guard.

1. `platform_admin_e2e::console::a_registered_operator_signs_in_with_a_platform_password`
   - RED: `POST /platform/admins {"name":"ops"}` returned 422 (`match_claim`
     required; no IdP-free registration or platform password existed).
   - GREEN: authorized registration without a claim, policy-checked
     `PUT /platform/admins/{id}/password`, `POST /platform/browser/password/login`
     (wrong/unknown/root-key-as-password 401 with no cookie, missing Origin 403),
     regenerated opaque host-only Secure/HttpOnly/SameSite=Lax `Path=/platform`
     cookie with token-free metadata, cookie use on platform reads and
     same-origin mutations, tenant cookie refused on platform routes, platform
     cookie refused on tenant routes and as a tenant cookie, logout revoking only
     its own session, API token unaffected. 1 passed.

2. `platform_admin_e2e::console::platform_console_sessions_end_on_every_replica`
   - RED: after suspending and reactivating the operator, its old session
     resolved again (`after active`: 200, expected 401). TDD note: steps 1–3
     (idle expiry, activity capped at the absolute deadline, logout across
     replicas) already passed because scenario 1's resolve statement checked
     both deadlines and revocation; they are regression proof, not new RED. Two
     earlier failures were test-setup bugs in how the test aged a session and
     are not counted as RED.
   - GREEN: suspension revokes the principal's platform browser sessions in its
     own transaction. Idle expiry and the absolute cap hold on both replicas;
     a password change ends earlier sessions and only the new password signs
     in; when the session store cannot be written, a platform cookie gets 503
     on session reads and platform routes, then works again. 1 passed.

3. `platform_admin_e2e::console::a_registered_operator_signs_in_to_the_console_through_platform_oidc`
   - RED: `POST /platform/browser/oidc/login` answered 401 "missing
     X-Wyrd-Access-Token header" (expected 403 without Origin); the route did
     not exist.
   - GREEN: `POST /platform/browser/oidc/login` (same-origin, counted in the
     per-source sign-in window, fixed `{origin}/platform/ui/callback`
     redirect, `wyrd_platform_login` binding cookie on `Path=/platform`) and
     `POST /platform/browser/oidc/callback`. The login state records purpose,
     binding digest and connection revision, and is consumed only with the
     matching binding. A missing or foreign binding, an unregistered subject,
     a replay, and swapping API-token and browser states each create no
     session. The registered subject gets an `oidc` session with no provider
     token in its body or cookies. Replacing the connection advances its
     revision and ends the sessions it created; the pinned subject signs in
     again. Pre-registration and issuer/subject pinning reuse the existing
     `PlatformSessions` owner: a first-login pin and its browser session
     commit in one transaction. 1 passed; the existing platform OIDC journeys
     (7) and `pg_platform_identity` (11) still pass.

4. `platform_admin_e2e::console::destructive_console_changes_need_fresh_password_proof` and
   `platform_admin_e2e::console::destructive_console_changes_accept_forced_fresh_idp_proof`
   - RED (both): with proof older than five minutes, `PUT
     /platform/tenants/{id}/status` (retiring a workspace) returned 200
     (expected 403). Nothing checked browser freshness.
   - GREEN: `PlatformCaller::require_recent_proof` runs first in tenant
     status, social provider configure/enable, login settings, mail relay,
     OIDC connection configure/remove, admin status and admin password. It
     refuses only browser sessions whose `fresh_at` is outside the shared
     five-minute window (`WYRD_AUTH_403_RECENT_LOGIN_REQUIRED`); platform API
     tokens pass unchanged. With stale proof, retirement, provider, signup
     settings and SMTP changes all refuse and the API-token readback is
     unchanged. `POST /platform/browser/password/reauth` checks only the
     session principal's own password: another operator's password, the
     bootstrap key or a request with no session refreshes nothing; the right
     password refreshes in place (no new cookie) and the changes land.
     `POST /platform/browser/oidc/reauth` begins a login bound to this session
     with `prompt=login` and `max_age=0`. The callback refreshes only that
     presenting session, only for a token whose `auth_time` is within five
     minutes and whose subject is already pinned to the same principal;
     another registered operator's identity and a 10-minute-old
     authentication refresh nothing. 2 passed.

5. `platform_admin_e2e::console::maintenance_removes_ended_platform_sessions`
   - RED: TDD deviation. The test did not compile because the platform
     session owner had no purge entry point (`PlatformLogin::sessions`,
     `purge_ended_browser_sessions`), so ended sessions would accumulate.
     This is a compile-time RED, not a behavioral one.
   - GREEN: the auth maintenance worker also drains ended platform browser
     sessions (revoked, idle-expired or past the absolute deadline) in
     batches of 1000 through `PlatformSessions::purge_ended_browser_sessions`.
     The journey removes 3 ended sessions in one batch, then 0, and the live
     session still resolves. Login states that reference a purged session
     cascade with it. 1 passed.

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

**Cycle 15 status: APPROVED (2026-11-23).** The maintainer approved the extended mock (commits 2d2cdbe26, 1c8773557) and all seven contract readings in the mock README: (1) the proof step reports its method, so the completion page does not infer it; (2) recovery-contact entry appears only when `email_discovery` is offered; (3) no current recovery address is shown; (4) a read exposes whether platform OIDC exists so its button appears only when usable; (5) sign-in on `/platform`, console under `/platform/ui`; (6) retirement is Suspend/Resume; (7) a blank relay password keeps the stored one, with explicit removal.

**Reserved signup slugs (spec revision 10 addendum).** RED: `platform_admin_e2e::signup::signup_cannot_claim_a_reserved_slug` failed with `admin is reserved: {"tenant":"admin"}` (200, expected 409). GREEN: `ProvisionOwner::claim_for_signup` refuses a reserved slug as `SlugTaken` before any write; all 12 signup journeys pass. TDD: PASS.

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

#### Cycle 16 evidence (Part B: tenant onboarding pages)

Journey host: `identity_ui_e2e::onboarding_ui_journey` (one bound server over
repository Postgres, a production `node build` BFF at a public origin, the
repository Mailpit as relay) runs `src/lib/server/auth/onboarding.integration.test.ts`
in real Chromium; it joins `test:identity:ui:suite`. Command:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E "test(=identity_ui_e2e::onboarding_ui_journey)"'`
after `pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`.

1. **Root offers create/find only when usable.** RED before production edits:
   with mail and signup enabled, `getByRole('link', { name: 'Create a workspace' })`
   timed out (no link). The same run also showed `/readyz` 503
   (`forge_coordinator_unavailable` right after bind), which the pages read as
   "server down"; the journey now waits for readiness before its first page, as
   an operator would. GREEN: private defaults show neither link and keep the
   tenant entry; after the operator configures the relay and signup the root
   page links `/signup` and `/workspaces`. The root load reads the no-tenant
   `/auth/browser/methods` (`signup`, `email_discovery`).


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

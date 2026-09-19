# TASK-007 / TASK-008 — validated finding ledger

Each finding was traced to source, its callers read in full, and its path proved
reachable and required by the approved task. The Ponytail ladder was applied to
every proposed correction: delete -> reuse repository behavior -> standard
library -> native platform -> installed dependency -> minimum new code. Where an
existing repository owner already solves the problem it is named, and the
correction is bounded to reusing it.

## TASK-007

### FIND-007-1 — MISSING — the primary proof named by the task was not written

- **Obligation**: TASK-007 §Verification — "The primary proof is a real-provider
  identity journey extending the existing `test:identity:journey` lane (Keycloak
  and Dex under `WYRD_IDENTITY_E2E`) to cover platform-connection configuration,
  pre-registration, first-login pinning, unknown-subject denial, scope
  separation across a platform and a tenant session for the same human, and
  credential administration while the provider is unavailable." Also AC-015,
  AC-016.
- **Location**: `crates/wyrd/wyrd-server/tests/identity_e2e.rs` — unchanged in
  `c5c2075..HEAD` (absent from `git diff --stat`), and `grep -c platform` over
  it returns `0`. `mise.toml` `test:identity:journey` (line 432) is unchanged.
- **Evidence**: the only journeys added are provider-free
  (`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:741`, `:899`). They
  configure and remove a connection pointing at `https://idp.example.com/...`
  that is never contacted, and never call `/auth/platform/login` beyond
  asserting a `404` before configuration (`platform_admin_e2e.rs:754-767`).
- **Consequence**: the entire federated sign-in path — discovery, code exchange,
  token verification, nonce binding, subject pinning, session minting — has
  never been executed against a real provider or in any other form. AC-015 and
  AC-016 have no evidence at all.
- **Correction**: extend `identity_e2e.rs` under the existing `e2e_enabled()`
  gate and the existing `test:identity:journey` lane, reusing
  `crates/wyrd/wyrd-testing`'s Keycloak/Dex fixtures rather than adding a
  provider harness. Cover, for one real provider: configure the platform
  connection -> register an administrator against that account's email -> first
  login pins and returns a usable platform session -> a second login with the
  same subject succeeds -> a token for an unregistered subject is refused and
  creates no principal -> the platform session is refused on a `/v1` tenant
  route and the tenant session is refused on `/platform/*` -> with the provider
  unreachable, the global credential still creates a tenant.
- **Status**: `CONFIRMED`.

### FIND-007-2 — MISSING — the core of TASK-007 has no test at any tier

- **Obligation**: AGENTS.md §11 ("every new user/agent-facing capability ships a
  user-journey test"), §12; spec `VER-001`.
- **Location**: `crates/wyrd/wyrd-auth/src/platform_login.rs` (350 lines, no
  `mod tests`); `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs`
  (`pin_platform_identity`, `take_platform_login_state`,
  `platform_identity_by_subject`, `purge_expired_platform_login_state`).
- **Evidence**: a repository-wide grep for each of those symbols outside its
  defining file returns only `components/platform/identity.rs`. No test file
  references any of them. `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs`
  covers principals and credentials only.
- **Consequence**: four load-bearing safety properties are asserted only in
  prose: single-use login state (the `DELETE ... RETURNING`), expiry enforcement
  (`.filter(|row| row.expires_at > Utc::now())`), one-time pinning (the
  `subject IS NULL` predicate), and nonce binding (`platform_login.rs:340-352`).
  Each would still compile and pass every existing lane if silently weakened —
  changing `take_platform_login_state` to a `SELECT` makes every login state
  infinitely replayable with no test failing.
- **Correction**: add Postgres-backed integration tests beside the existing
  `pg_admin_principals.rs` suite, exercising the query slots directly: a second
  `take_platform_login_state` on the same key returns `None`; an expired row
  returns `None`; two `pin_platform_identity` calls for the same claim return
  `Some` then `None`; `pin_platform_identity` never matches an already-pinned
  row; `platform_identity_by_subject` resolves only the pinned pair. Add a unit
  test for the nonce check. Distinct from FIND-007-1: the journey proves the
  path, these pin the invariants.
- **Status**: `CONFIRMED`.

### FIND-007-3 — INCORRECT — first-login pinning trusts an unverified email claim

- **Obligation**: REQ-044 ("An unknown subject at the platform plane MUST be
  denied; just-in-time provisioning of platform principals is prohibited"),
  INV-011 (unverifiable conditions deny), INV-004b.
- **Location**: `crates/wyrd/wyrd-auth/src/platform_login.rs:282` —
  `let Some(claim) = claims.email.as_deref() else { ... }`, feeding
  `pin_platform_identity` at `:285`. Claim mapping fixed at
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:46-52`.
- **Evidence**: `grep -rn email_verified --include=*.rs crates/` returns
  nothing — the claim is never read anywhere in the repository.
  `wyrd_auth_oidc::map_claims` (`claims.rs:47-50`) extracts `email` with no
  verification predicate. `ExternalClaims::email` is documented in
  `wyrd-auth-verify/src/lib.rs` as "never used as the identity key", which the
  platform path contradicts.
- **Falsifying scenario**: a deployment points the platform connection at its
  corporate IdP and pre-registers `ops@example.com` before that person's first
  login. Any other principal in that same issuer who can present an ID token
  carrying `email: "ops@example.com"` — a self-service realm registration, an
  unverified alias, a directory permitting duplicate mail attributes — reaches
  `/auth/platform/callback`, passes signature/audience/nonce verification, and
  `pin_platform_identity` durably binds *their* `sub` to the pre-registered
  principal. They are now that platform administrator, and the legitimate
  administrator can never be pinned because the row is consumed. This is exactly
  the "unknown subject acquires platform authority" outcome REQ-044 forbids, and
  it is one-way: nothing in the served surface can unpin the row (FIND-007-6).
- **Correction**: in `resolve_principal`, refuse the first-login pin unless the
  verified token asserts the email as verified — read `email_verified` from
  `claims.raw_claims` (already carried for the nonce check at
  `platform_login.rs:340-352`; no new plumbing) and require boolean `true`,
  returning `PlatformLoginError::NotAccepted` otherwise. This only narrows the
  claim match REQ-044 already prescribes and needs no new configuration surface
  or product decision. Already-pinned logins are unaffected: they resolve by
  subject before reaching this branch.
- **Status**: `CONFIRMED`.

### FIND-007-4 — VIOLATION — the platform connection bypasses the SSRF screening the task requires it to reuse

- **Obligation**: TASK-007 §Constraints — "Reuse the existing OIDC verification
  mechanics — discovery, JWKS, PKCE, nonce, state, **SSRF screening**, secret
  protection. Do not define a second verification implementation."
- **Location**:
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:129-178`
  (`configure_connection`) passes `request.issuer_url` and `request.jwks_uri`
  straight to `upsert_platform_oidc_connection` at `:157`.
- **Evidence**: the existing owner is
  `crates/wyrd/wyrd-server/src/components/admin/routes.rs` — `is_blocked_addr`
  (`:575`), `resolve_and_screen` (`:588`), `pinned_discovery_client` (`:620`),
  `discover_jwks_uri` (`:655`). That path screens every resolved address against
  the deployment profile, pins the discovery client to the screened addresses to
  defeat DNS rebinding, and **derives** `jwks_uri` from the issuer's discovery
  document instead of accepting it from the caller. `configure_connection` does
  none of this: it does not even parse the two values as URLs, so a malformed
  issuer surfaces only at login time as `PlatformLoginError::ConnectionUnusable`.
- **Falsifying scenario**: with the connection set to
  `issuer_url = "http://169.254.169.254/latest/"`, every unauthenticated `POST`
  to `/auth/platform/login` — an anonymous route by design
  (`identity.rs:64-71`) — drives `discover_authorization_endpoint`
  (`wyrd-auth/src/login.rs:145`) into an outbound fetch of that address from the
  server, with no rate limit and no caller identity. Separately, because
  `jwks_uri` is caller-supplied and never checked against the issuer's discovery
  document, the keys that verify platform-administrator tokens need not belong
  to the issuer named in `iss` — a property the tenant path structurally
  prevents.
- **Consequence**: the highest-privilege plane accepts a weaker issuer
  registration contract than a tenant administrator does, and converts it into
  unauthenticated server-side request forgery.
- **Correction**: route the write through the existing screening owner rather
  than adding a second one — validate `issuer_url` into `IssuerUrl`, screen it
  with the existing address policy under the deployment profile, and derive
  `jwks_uri` from discovery instead of accepting it on the wire, dropping the
  field from `ConfigurePlatformOidcRequest`. The screening helpers currently sit
  in `components/admin/routes.rs`; lift them to a shared module both callers
  use rather than copying them.
- **Status**: `CONFIRMED`.

### FIND-007-5 — INCORRECT — `register_admin` is not atomic and diverges from the authorization contract

- **Obligation**: AGENTS.md §"Audit records authorization decisions" (the
  decision and the operation commit together); `PlatformAuthorization::authorize`
  rustdoc — "the returned transaction already carries the allowance row, so the
  caller's operation and its authorization commit or roll back together. The
  caller owns that transaction and must commit it"; INV-006/INV-011 fail-closed.
- **Location**:
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:100-126` — the
  local `authorize` helper commits the transaction at `:116` and returns `()`.
  `register_admin` then performs two independent writes on the pool:
  `insert_platform_principal` at `:283` and `insert_platform_identity` at `:286`.
- **Evidence**: every other platform-plane caller carries the transaction into
  the operation — `components/platform/provisioning.rs:129-147` (`let mut tx =
  authz.authorize(...)`, writes with `&mut tx`, commits at `:147`),
  `components/platform/recovery.rs:75-86`, and
  `components/principals/routes.rs:218`, `:277`, `:301`, `:346`
  (`let mut conn = authorize(...)`). `identity.rs` is the sole exception. A
  transactional slot already exists and is unused here:
  `wyrd-sql/src/queries/platform/principals.rs:56`
  `insert_platform_principal_tx`.
- **Falsifying scenario**: already reproduced by the repository's own passing
  test. `platform_admin_e2e.rs:845-861` registers `ops-lead-again` with a
  `match_claim` already taken; `insert_platform_principal` succeeds,
  `insert_platform_identity` raises `UniqueViolation`, and the handler returns
  `409`. The `platform.principals` row for `ops-lead-again` is left behind with
  no identity: it can never sign in, it is invisible to any served listing
  (FIND-007-6), it permanently consumes that name against the principal-name
  unique constraint, and it is a grantable platform principal no federated login
  can ever reach. Any store failure between the two writes produces the same
  orphan.
- **Correction**: make `register_admin` use the transaction the platform plane
  already returns — bind `let mut tx = authz.authorize(...)`, write through
  `insert_platform_principal_tx` and a transactional sibling of
  `insert_platform_identity`, then commit once. Apply the same shape to
  `configure_connection` and `remove_connection` so the identity routes stop
  being the one place an allowance commits without its operation. Do not
  introduce a compensating delete.
- **Status**: `CONFIRMED`.

### FIND-007-6 — MISSING — platform principal listing and revocation do not exist

- **Obligation**: TASK-007 §Approach 3 — "Add platform principal
  pre-registration, listing, **and revocation**, authorized only from the
  platform plane." Supports INV-013 and the session guarantees the code claims.
- **Location**:
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:55-63` registers
  `GET/PUT/DELETE /platform/oidc/connection` and `POST /platform/admins` only.
  `crates/wyrd/wyrd-sql/src/queries/platform/principals.rs` exposes exactly
  three functions: `insert_platform_principal_tx`, `insert_platform_principal`,
  `platform_principal_by_id` — no list, no status update.
- **Evidence**: `PlatformSessions::issue_federated`
  (`platform_sessions.rs:166-183`) and `confirm_federated_session` (`:244-259`)
  both hinge on `principal.is_active()`, and their rustdoc states "suspending it
  ends every session it holds on the next request, which is the human equivalent
  of revoking a credential". No code path anywhere sets a platform principal to
  a non-active status.
- **Consequence**: the documented revocation story for human platform
  administrators is unreachable. Removing a compromised administrator requires
  direct SQL against `platform.principals`, which the spec's operator journey
  exists to eliminate. The `is_active()` checks are dormant guards whose green
  tests prove nothing about the property they describe. Combined with
  FIND-007-3, an administrator pinned by the wrong subject cannot be removed
  through any served surface.
- **Correction**: add the two authorized platform-plane operations the task
  names — list platform principals with their identity pin state, and revoke or
  suspend one — through the existing `PlatformAuthorization` transaction shape
  used by `provisioning.rs`, with the status write in the same transaction as
  its allowance. Reuse `platform.principals.status` rather than adding state.
- **Status**: `CONFIRMED`.

### FIND-007-7 — INCORRECT — a circular test claims a property it does not exercise

- **Obligation**: AGENTS.md §12 (do not produce proof weaker than its assertion);
  spec `VER-001` (evidence must be credible).
- **Location**:
  `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:245-256`,
  `platform_caller_carries_its_minting_credential`.
- **Evidence**: the fixture `caller()` at `:186-198` constructs
  `credential_id: Some(Uuid::now_v7())` by hand; the test then asserts
  `caller.credential_id.is_some()`. The second assertion,
  `caller.principal_id() == caller.context.principal_id()`, is a tautology:
  `PlatformCaller::principal_id` at `:57-59` is defined as
  `self.context.principal_id()`.
- **Consequence**: the docstring claims "The caller carries the credential that
  minted its session, so audit can name the credential as well as the
  principal." Nothing about minting, session confirmation, or audit is executed.
  `PlatformSessions::confirm` could stop propagating `credential_id` entirely and
  this test would still pass, so it is affirmative-looking coverage over an
  unguarded seam.
- **Correction**: replace it with a test of the actual seam —
  `PlatformSessions::confirm` on credential-minted claims yields
  `Some(credential_id)` and on federated claims yields `None`. The
  `platform_credentials::pg_tests` module in the same crate is the established
  home for that.
- **Status**: `CONFIRMED`.

### FIND-007-8 — MISSING — AC-011 has no evidence

- **Obligation**: AC-011 and TASK-007 acceptance — "A federated human and a
  machine credential produce the same authenticated context shape, and no
  downstream handler branches on the entry path"; "A human tenant administrator
  coexists with the tenant administrative principal; neither displaces nor
  requires removal of the other." TASK-007 §Approach 5.
- **Location**: no test added. `identity_e2e.rs::human_oidc_login_journey`
  predates the change and is untouched; it knows nothing about
  `PrincipalKind::TenantAdmin`, which this change introduced.
- **Consequence**: the coexistence requirement — the reason REQ-035 exists, so a
  tenant keeps a headless administrative path independent of its identity
  provider — is asserted nowhere. A regression that made a federated human login
  displace or conflict with the tenant administrative principal would be caught
  by nothing.
- **Correction**: extend `identity_e2e.rs::human_oidc_login_journey` (already
  provider-backed and in the named lane) so that, in a tenant that already has
  its administrative principal from provisioning, a federated human logs in,
  receives a token, and both identities operate concurrently; assert the
  authenticated context shape is the same for both.
- **Status**: `CONFIRMED`.

### FIND-007-9 — VIOLATION — a wire secret is carried in a plain `String` with a derived `Debug`

- **Obligation**: AGENTS.md §4 — "Use `secrecy::SecretString` for secrets and
  redacted custom `Debug` impls for secret-bearing structs." INV-002.
- **Location**: `crates/wyrd-spec/src/auth/platform_identity.rs:13-29` —
  `#[derive(Debug, Clone, Serialize, Deserialize, ...)] pub enum
  PlatformClientAuth { SecretBasic { secret: String }, SecretPost { secret:
  String }, Public }`, reached through
  `ConfigurePlatformOidcRequest::client_auth` at `:45`.
- **Evidence**: the repository already owns this exact problem.
  `crates/wyrd-spec/src/auth/secret_bearer.rs:36-40` defines `SecretBearer` with
  a redacting `Debug`, a serializer that still emits the raw value, and
  `into_secret_string()`; it is used for the outbound secret in
  `PlatformTokenResponse`. The inbound provider secret does not use it.
- **Consequence**: any `{:?}` of the request — a future handler, an extractor
  rejection, a middleware, a test helper — prints the provider client secret in
  cleartext. Today's single handler suppresses it with
  `#[tracing::instrument(..., skip(request))]`
  (`components/platform/identity.rs:128`), so the secret's protection rests on
  one attribute at one call site rather than on the type.
- **Correction**: change both `secret` fields to `SecretBearer` and use
  `into_secret_string()` at the one construction site (`identity.rs:132-140`),
  which already builds `SecretString` by hand. No new type and no serialization
  change: `SecretBearer`'s `Serialize`/`Deserialize` are string-transparent.
- **Status**: `CONFIRMED`.

## TASK-008

### FIND-008-1 — MISSING — no Python SDK surface

- **Obligation**: TASK-008 §Objective and §Approach 2 ("the Rust, Python, and
  TypeScript SDKs project it"), acceptance criterion 1, AC-014, REQ-036.
- **Location**: `sdks/wyrd-sdk-python/` is absent from
  `git diff --stat c5c2075..HEAD`. There is no `principals` module under
  `sdks/wyrd-sdk-python/python/wyrd/`, and no principal or credential symbol is
  exported from `__init__.py`.
- **Consequence**: AC-014 fails for Python. No `py:test:unit`, `py:typecheck`,
  or stub regeneration evidence exists because there is nothing to generate.
- **Correction**: add the Python projection of `wyrd_client::principals` under
  `sdks/wyrd-sdk-python`, following the existing `cards`/`bifrost` module shape
  (PyO3 wrapper + `python/wyrd/<module>` export + regenerated `.pyi`), and a
  journey against a real server following
  `crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs`.
- **Status**: `CONFIRMED`.

### FIND-008-2 — MISSING — no TypeScript SDK surface

- **Obligation**: same as FIND-008-1.
- **Location**: `sdks/wyrd-sdk-ts/` absent from the diff; no principal module
  exists under it.
- **Consequence**: AC-014 fails for TypeScript; the repository TypeScript
  integration task has no journey to run.
- **Correction**: project the same operations through `sdks/wyrd-sdk-ts`
  following its existing module and declaration shape, and add the TypeScript
  journey the task names.
- **Status**: `CONFIRMED`.

### FIND-008-3 — MISSING — no MCP projection and no scope gating

- **Obligation**: TASK-008 §Approach 3 and acceptance criterion 3 — "MCP
  administrative write tools are unavailable without their explicit scope and
  available with it; read tools remain available." REQ-036; AGENTS.md §2 ("MCP
  is first-class; read tools are always available, write tools require explicit
  scopes").
- **Location**: `crates/wyrd/wyrd-mcp/` is absent from the diff.
  `crates/wyrd/wyrd-server/src/mcp/` contains only `bifrost.rs`, `probe.rs`,
  `mod.rs`; no administrative tool is registered and no scope gate exists.
- **Consequence**: the agent-facing surface, which AGENTS.md §2 names a primary
  surface, does not expose the administrative contract at all. The acceptance
  criterion is not merely unproved, it is unimplemented.
- **Correction**: register the tenant principal and credential operations as MCP
  tools in `wyrd-server/src/mcp`, reusing `wyrd_client::principals` rather than
  re-implementing transport, with the write tools behind an explicit scope and
  the read tools ungated; prove both states with a focused test.
- **Status**: `CONFIRMED`.

### FIND-008-4 — MISSING — no platform-plane operation reaches any client surface

- **Obligation**: REQ-036 ("**Platform** and tenant administrative operations
  MUST be available headlessly ... CLI, SDKs, and MCP project that contract");
  TASK-008 acceptance criterion 2 ("A tenant-scope client cannot invoke a
  platform-plane operation through any SDK or MCP tool; the refusal is the
  stable contract error").
- **Location**: `crates/shared/wyrd-client/src/principals/handle.rs:60-135`
  exposes four `/v1/principals*` routes only. No client method exists for
  `/auth/platform/token`, `/platform/tenants`, `/platform/oidc/connection`,
  `/platform/admins`, or the recovery route.
- **Consequence**: half of REQ-036 is unprojected, and acceptance criterion 2 is
  vacuously unfalsifiable — a tenant client cannot invoke a platform operation
  because no client can. The criterion asks for a *proved refusal*, not an
  absence.
- **Correction**: add the platform-plane operations to `wyrd-client` as a
  sibling handle (the shape `Principals` already establishes), and add the
  cross-plane refusal journey asserting the stable contract error when a
  tenant-scoped client calls one.
- **Status**: `CONFIRMED`.

### FIND-008-5 — MISSING — the administrative routes are absent from the generated HTTP contract

- **Obligation**: REQ-036 — "available headlessly over the language-agnostic
  HTTP contract with typed bodies, stable `WyrdError` codes, and **generated
  artifacts**"; TASK-008 acceptance — "Generated types, stubs, schemas, and the
  OpenAPI contract regenerate cleanly with no hand edits"; `VER-006`.
- **Location**: `crates/wyrd/wyrd-server/src/http/openapi.rs:14-33` lists 18
  paths, none administrative. Both that file and the tracked `openapi.yaml` are
  unchanged in `c5c2075..HEAD`; `grep -n '/platform/\|/v1/principals' openapi.yaml`
  returns nothing.
- **Consequence**: a language-agnostic client — the stated reason the contract is
  HTTP-first — cannot discover any administrative operation from the published
  contract. Note that `codegen:check` would still pass: it compares generated
  output against the tracked file, and since nothing was registered there is no
  drift to detect. A green codegen lane is therefore not evidence here.
- **Correction**: register the administrative handlers in the `utoipa`
  `paths(...)` set and their request/response types in
  `components(schemas(...))`, then regenerate `openapi.yaml` through the
  existing generator (`crates/wyrd/wyrd-server/examples/gen_openapi.rs`). Do not
  hand-edit the yaml. Scope the correction to the routes this change added; the
  pre-existing partiality of the document is out of scope.
- **Status**: `CONFIRMED`.

### FIND-008-6 — VIOLATION — `bootstrap-key` and its fabricated identities are not gone

- **Obligation**: AC-013 — evidence must prove "that `bootstrap-key` and its
  fabricated identities are gone"; REQ-038; TASK-008 acceptance — "No document,
  example, or surface still describes a second identity model, a removed
  bootstrap path, or credential-keyed authorization."
- **Location**:
  - `crates/wyrd/wyrd-server/src/boot/bootstrap.rs` is still tracked at HEAD
    (`git ls-tree -r HEAD`), still defines `SYSTEM_OPERATOR_ID` at `:33` and the
    fabricated `bootstrap-admin` `CardRef` at `:96` and `:150`. `boot/mod.rs`
    merely stopped declaring the module, and the file was *edited* in this range
    (`:121`, `card_ref` -> `Some(card_ref)`) rather than deleted.
  - `docs/src/content/docs/self-hosting/running-the-server.svx:11,24,31,33,36,42`
  - `docs/src/content/docs/self-hosting/authentication.svx:29,75`
  - `docs/src/content/docs/self-hosting/local-development.svx:37`
- **Consequence**: the deliverable's own documentation instructs operators to run
  a subcommand that no longer exists (`main.rs` now offers only `Init`), so the
  published self-hosting path is broken. The removed identity model survives in
  tree as an orphaned source file containing the exact synthetic principal
  REQ-038 names, which INV-010 forbids.
- **Correction**: delete `crates/wyrd/wyrd-server/src/boot/bootstrap.rs`, and
  rewrite the three documentation pages onto `wyrd-server init` and the
  platform/tenant credential model. The `cli:bootstrap-key` mise task is already
  removed and needs no further action.
- **Status**: `CONFIRMED`.

### FIND-008-7 — MISSING — REQ-040 documentation was not written

- **Obligation**: REQ-040 — "Documentation MUST cover the three-command operator
  journey, the SaaS model in which Wyrd operates the global principal and the
  customer never receives it, credential rotation, and credential-loss
  recovery." Listed on both TASK-007 (§Relevant Surface,
  `docs/src/content/docs/`) and TASK-008 (obligations, §Approach 5).
- **Location**: `git diff --name-only c5c2075..HEAD` contains no path under
  `docs/`.
- **Consequence**: none of the four named topics is documented anywhere, and
  `mise run docs:check` — required by both tasks' Verification blocks — proves
  only that the unchanged site still builds.
- **Correction**: write the four topics into
  `docs/src/content/docs/self-hosting` alongside the FIND-008-6 rewrite, as one
  coherent operator model rather than four disconnected notes.
- **Status**: `CONFIRMED`.

## Considered and rejected

| Candidate | Why rejected |
|---|---|
| `cid` becoming `Option<String>` weakens the credential check | `REJECTED`. Tokens are signed by `IssuingKey`; the only producer of a `cid`-less token is `PlatformSessions::issue_federated` (`platform_sessions.rs:166`), reachable only after `PlatformLogin::complete` resolved a pinned principal. `exchange` (`:129`) always passes `Some(credential_id)`. `confirm` (`:200`) dispatches on `cid` and both arms re-read live state from the store. A credential holder cannot obtain a `cid`-less token, so the check cannot be dodged. The federated anchor is equivalent *in mechanism*; its weakness is that nothing can set a principal inactive, which is FIND-007-6, not a token-shape defect. |
| Duplicate `verify_nonce` in `platform_login.rs:340` vs `callback.rs:445` | `REJECTED`. The existing function takes `&LoginStateEntry`, a tenant-flow type the platform plane does not have. Both are short total comparisons with identical semantics; consolidating shuffles parameters without removing a real second implementation. |
| `client_auth_label` (`pg_resolvers.rs`) is a pass-through for `client_auth_discriminant` | `REJECTED`. One-line visibility adapter for a private helper; deleting it means widening the helper's visibility instead. No behavior, no cost. |
| Free-function handler helpers (`operator`, `authorize`, `login_service`) violate AGENTS.md §5 struct-centered style | `REJECTED`. Axum handlers are free functions by construction, and this matches `components/platform/routes.rs`, `recovery.rs`, and `principals/routes.rs`. AGENTS.md §16 requires following the existing pattern. The real defect in `authorize` is transactional (FIND-007-5), not structural. |
| `assert!(!format!("{before:?}").contains(...))` in `sdks/wyrd-sdk-rust/tests/principals.rs:97-102` is vacuous | `REJECTED` as a finding. `CredentialListResponse` has no secret field, so the type already guarantees it — the assertion is redundant rather than misleading, and its message is accurate. Noted, not reported. |
| `a_tenant_administrator_cannot_configure_platform_sign_in` asserts `401`, not the audited `403` denial | `REJECTED`. The `401` is the real behavior: a tenant access token is not a platform session token, so `PlatformCaller` rejects at authentication before authorization. The test's message matches what it proves. |
| Broad aggregates (`mise run gate`, `test:rust`, family lanes, `--all-features`) were not run | `REJECTED` per `VER-003`, which forbids requiring them and instructs the reviewer not to treat their absence as missing verification. |
| Pre-existing partiality of `openapi.yaml` (auth routes also absent) | Out of scope; FIND-008-5 is bounded to routes this change added. |

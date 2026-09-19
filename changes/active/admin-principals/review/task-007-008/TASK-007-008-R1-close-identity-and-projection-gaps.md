---
task: TASK-007-008-R1
title: Close the platform human-identity defects and the missing surface projection
spec: SPEC-admin-principals
spec_revision: 6
remediates: [TASK-007, TASK-008]
findings: [FIND-007-1, FIND-007-2, FIND-007-3, FIND-007-4, FIND-007-5, FIND-007-6, FIND-007-7, FIND-007-8, FIND-007-9, FIND-008-1, FIND-008-2, FIND-008-3, FIND-008-4, FIND-008-5, FIND-008-6, FIND-008-7]
---

## Inputs

- Approved spec: `changes/active/admin-principals/spec.md` (revision 6)
- Original tasks: `changes/active/admin-principals/tasks/TASK-007-platform-human-administration.md`,
  `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`
- Prior verdict and evidence:
  `changes/active/admin-principals/review/task-007-008/verdict.md`,
  `task-review.md`, `findings-validation.md`
- Candidate reviewed: base `c5c2075`, candidate `4225069` on
  `claude/admin-principals-spec-qfsmjc`

## Intended correction outcome

A human can sign in to the platform control plane through the deployment's one
OIDC connection, and that path is trustworthy, revocable, and proved against a
real provider. The administrative HTTP contract is reachable from the surfaces
the specification requires, and the deployment's documentation describes one
identity model.

## Issue diagnosis and decision-complete recommendation

Each item below states the violated obligation, the current behavior, why the
candidate's existing proof falls short, and the correction boundary. Where an
existing owner or mechanism solves the problem, it is named; reuse it rather
than writing a parallel one.

### 1. First-login pinning accepts an unverified email claim (FIND-007-3)

**Violated obligation**: REQ-044, INV-011, INV-004b.

**Current behavior**: `crates/wyrd/wyrd-auth/src/platform_login.rs:282` resolves
an unpinned pre-registration by matching `claims.email` against
`platform.principal_identities.match_claim`. `email_verified` is read nowhere in
the repository, and `wyrd_auth_oidc::map_claims` does not check it.

**Consequence**: a subject other than the intended administrator, presenting a
validly signed token from the same issuer that asserts the pre-registered email,
pins their own `sub` to the platform principal. The pin is one-way, so the
intended administrator can never claim it and (see item 4) nothing served can
undo it.

**Why the existing proof falls short**: there is no proof — nothing exercises
`resolve_principal` at any tier.

**Correction**: in `PlatformLogin::resolve_principal`, take the first-login
branch only when the verified token asserts the email as verified. The raw
claims are already on `ExternalClaims::raw_claims` and are already read a few
lines later for the nonce, so no new plumbing is needed; require the
`email_verified` claim to be boolean `true` and otherwise return
`PlatformLoginError::NotAccepted`, which already collapses to the single
indistinguishable rejection. Do not add a configuration switch: REQ-044 fixes
the pre-registration semantics, and this only narrows the match it prescribes.
The already-pinned branch is untouched, since it resolves by subject first.

### 2. The platform connection bypasses SSRF screening (FIND-007-4)

**Violated obligation**: TASK-007 constraint "Reuse the existing OIDC
verification mechanics — discovery, JWKS, PKCE, nonce, state, SSRF screening,
secret protection. Do not define a second verification implementation."

**Current behavior**: `components/platform/identity.rs:129-178` stores
`issuer_url` and `jwks_uri` verbatim with no parsing, no address screening, and
no discovery.

**Consequence**: unauthenticated `POST /auth/platform/login` drives an outbound
server fetch to whatever address the stored issuer names; and the JWKS that
verifies platform-administrator tokens need not belong to the issuer in `iss`.

**Why the existing proof falls short**: the e2e configures a connection that is
never contacted, so no screening behavior is observable.

**Correction**: reuse the existing screening owner rather than adding a second.
`crates/wyrd/wyrd-server/src/components/admin/routes.rs` already holds the
policy (`is_blocked_addr`, `resolve_and_screen`, `pinned_discovery_client`,
`discover_jwks_uri`) applied when a tenant administrator registers a trusted
issuer. Lift those helpers into a module both route families call — do not copy
them and do not weaken them. On the platform configure path: parse the issuer
into `IssuerUrl`, screen it under the deployment profile, and **derive**
`jwks_uri` from the issuer's discovery document, removing `jwks_uri` from
`ConfigurePlatformOidcRequest`. A connection whose issuer fails screening is
refused at configuration time with the stable contract error, not at login.

### 3. `register_admin` is non-atomic (FIND-007-5)

**Violated obligation**: the transactional-authorization rule in `AGENTS.md`,
the documented contract of `PlatformAuthorization::authorize`, INV-006.

**Current behavior**: the local `authorize` helper at
`components/platform/identity.rs:100-126` commits the allowance transaction and
returns `()`; `register_admin` then issues `insert_platform_principal` (`:283`)
and `insert_platform_identity` (`:286`) as two independent pool writes.

**Consequence**: a `UniqueViolation` on the second write leaves an unreachable
platform principal that permanently consumes its name. The candidate's own
passing e2e (`platform_admin_e2e.rs:845-861`) creates exactly such an orphan.

**Why the existing proof falls short**: the test asserts the `409` status and
never inspects the store, so the orphan is invisible to it.

**Correction**: adopt the shape every other platform-plane caller already uses —
`components/platform/provisioning.rs:129-147`,
`components/platform/recovery.rs:75-86`, and
`components/principals/routes.rs:218`. Bind the transaction
`PlatformAuthorization::authorize` returns, perform the writes in it, and commit
once. `wyrd-sql/src/queries/platform/principals.rs:56`
`insert_platform_principal_tx` already exists for this; add the matching
transactional form for the identity insert. Apply the same shape to
`configure_connection` and `remove_connection`. Do not add a compensating
delete, and do not leave the non-transactional `authorize` helper behind for
other callers to reuse.

### 4. Platform principals cannot be listed or revoked (FIND-007-6)

**Violated obligation**: TASK-007 Approach 3; INV-013 and the session
guarantees the code's own rustdoc asserts.

**Current behavior**: the identity router exposes only the connection triple and
`POST /platform/admins`. `queries/platform/principals.rs` has no list and no
status update. Nothing in the repository sets a platform principal inactive.

**Consequence**: `PlatformSessions::issue_federated` and
`confirm_federated_session` gate on `principal.is_active()`, a condition no
served operation can produce. Removing a compromised human platform
administrator requires direct SQL — the exact out-of-band step the spec's
operator journey exists to eliminate — and with item 1 unfixed, a mis-pinned
principal is unrecoverable.

**Why the existing proof falls short**: the `is_active()` guards are exercised
only for credential-minted sessions, where credential revocation is the live
mechanism.

**Correction**: add the two authorized platform-plane operations the task names:
list platform principals with their identity pin state, and revoke or suspend
one. Route both through `PlatformAuthorization` with the write in the same
transaction as its allowance, per item 3. Reuse the existing
`platform.principals.status` column; do not introduce new lifecycle state.

### 5. A wire secret is unprotected by its type (FIND-007-9)

**Violated obligation**: `AGENTS.md` §4; INV-002.

**Current behavior**: `crates/wyrd-spec/src/auth/platform_identity.rs:13-29`
carries the provider client secret as `String` inside a `#[derive(Debug)]` enum.
Protection rests entirely on one `#[tracing::instrument(..., skip(request))]`
attribute at the single current call site.

**Correction**: use the repository's existing owner. Change both `secret` fields
to `wyrd_spec::auth::SecretBearer`, which already redacts `Debug` and serializes
transparently, and call `into_secret_string()` at the one construction site in
`components/platform/identity.rs`, which builds a `SecretString` by hand today.
No new type.

### 6. A circular test (FIND-007-7)

**Violated obligation**: `AGENTS.md` §12; `VER-001` credibility.

**Current behavior**:
`components/auth/platform_extractor.rs:245-256` asserts that a fixture it
constructed with `Some(..)` is `Some`, and that a delegating accessor equals
what it delegates to, while claiming to prove that audit can name the minting
credential.

**Correction**: replace it with a test of the real seam —
`PlatformSessions::confirm` yields `Some(credential_id)` for credential-minted
claims and `None` for federated claims. The `platform_credentials::pg_tests`
module in `wyrd-auth` is the established home. Do not simply delete it and leave
the seam unproved.

### 7. The human-identity path has no evidence (FIND-007-1, FIND-007-2, FIND-007-8)

**Violated obligation**: TASK-007 §Verification (the real-provider journey is
named as the primary proof), AC-011, AC-015, AC-016, `VER-001`.

**Current behavior**: `crates/wyrd/wyrd-server/tests/identity_e2e.rs` and the
`test:identity:journey` lane are untouched and contain nothing about the
platform plane. `platform_login.rs` has no `mod tests`. No test references
`pin_platform_identity`, `take_platform_login_state`, or
`platform_identity_by_subject`.

**Consequence**: single-use login state, expiry enforcement, one-time pinning,
and nonce binding are all correct as written and all unguarded — each could be
silently weakened with every lane still green.

**Correction**, in two parts, both required:

- **Journey** — extend `identity_e2e.rs` inside the existing `e2e_enabled()`
  gate and the existing `test:identity:journey` lane, reusing the repository's
  Keycloak/Dex fixtures rather than adding a provider harness. Against one real
  provider: configure the platform connection, pre-register an administrator
  against that account's verified email, complete a first login that pins and
  returns a usable platform session, complete a second login for the same
  subject, refuse a token for an unregistered subject and show no principal was
  created, show the platform session is refused on a `/v1` tenant route and a
  tenant session on `/platform/*` for the same human, and show the global
  credential still administers the platform while the provider is unreachable.
  Also extend `human_oidc_login_journey` so a federated human tenant
  administrator and the tenant administrative principal operate concurrently
  with the same authenticated context shape, closing AC-011.
- **Invariants** — add Postgres-backed integration tests beside
  `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs` that exercise the query
  slots directly: a replayed `take_platform_login_state` returns `None`; an
  expired row returns `None`; concurrent `pin_platform_identity` for one claim
  returns `Some` then `None`; a pinned row is never re-matched by claim; and a
  unit test for the nonce comparison.

### 8. The administrative contract is not projected (FIND-008-1..5)

**Violated obligation**: REQ-036, AC-013, AC-014, TASK-008 acceptance criteria
1 through 4.

**Current behavior**: only `crates/shared/wyrd-client/src/principals/` and
`sdks/wyrd-sdk-rust` exist, covering tenant principal and credential operations
only. `sdks/wyrd-sdk-python`, `sdks/wyrd-sdk-ts`, `crates/wyrd/wyrd-mcp`, and
`crates/wyrd/wyrd-server/src/mcp` are untouched, and no administrative route
appears in `http/openapi.rs` or `openapi.yaml`.

**Why the existing proof falls short**: the Rust journey passes and proves the
Rust surface. `codegen:check` would also pass — because nothing was registered,
there is no drift to detect — so a green codegen lane is not evidence for the
generated-artifact obligation.

**Correction**:
- Add the platform-plane operations to `crates/shared/wyrd-client` as a sibling
  handle following the `Principals` shape already established there. Keep
  `wyrd-client` the sole Rust client surface; SDKs stay thin over it.
- Project the tenant and platform operations through `sdks/wyrd-sdk-python` and
  `sdks/wyrd-sdk-ts`, each following that package's existing module,
  declaration, and stub-generation shape. Regenerate stubs and types from
  source; do not hand-edit generated files.
- Register the administrative operations as MCP tools in
  `crates/wyrd/wyrd-server/src/mcp`, reusing `wyrd_client` for transport, with
  write tools behind an explicit scope and read tools ungated.
- Register the administrative handlers and their types in the `utoipa`
  `paths(...)` and `components(schemas(...))` sets in
  `crates/wyrd/wyrd-server/src/http/openapi.rs`, then regenerate `openapi.yaml`
  through `crates/wyrd/wyrd-server/examples/gen_openapi.rs`. Scope this to the
  routes this change added.
- Add one journey per language surface against a real server, following
  `crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs`, each
  including the cross-plane refusal: a tenant-scope client invoking a
  platform-plane operation receives the stable contract error.

### 9. The removed bootstrap path survives (FIND-008-6) and REQ-040 is undocumented (FIND-008-7)

**Violated obligation**: REQ-038, REQ-040, AC-013, and the TASK-008 criterion
"No document, example, or surface still describes a second identity model, a
removed bootstrap path, or credential-keyed authorization."

**Current behavior**: `crates/wyrd/wyrd-server/src/boot/bootstrap.rs` is still
tracked at HEAD with `SYSTEM_OPERATOR_ID` and the fabricated
`system/bootstrap-admin` CardRef; only its module declaration was removed.
`docs/src/content/docs/self-hosting/running-the-server.svx`,
`authentication.svx`, and `local-development.svx` still instruct operators to
run `wyrd-server bootstrap-key`, a subcommand `main.rs` no longer offers. No
documentation covers the three-command operator journey, the SaaS model,
credential rotation, or credential-loss recovery.

**Correction**: delete `crates/wyrd/wyrd-server/src/boot/bootstrap.rs`. Rewrite
the three self-hosting pages onto `wyrd-server init` and the platform/tenant
credential model, and write REQ-040's four topics there as one coherent operator
model rather than four disconnected notes.

## Constraints and preserved behavior

- Do not weaken, disable, delete, or `#[ignore]` any test to produce a passing
  result. Item 6 replaces a test with a stronger one; it does not remove
  coverage.
- Preserve the pin's existing safety properties: the `subject IS NULL` predicate
  on `pin_platform_identity`, the `DELETE ... RETURNING` consumption of login
  state, the expiry filter, the mid-flight issuer check in
  `PlatformLogin::complete`, and the single indistinguishable
  `NotAccepted` / `Unauthenticated` projection. Item 1 narrows the claim match;
  it must not widen any of these.
- Preserve `PlatformAccessTokenClaims::cid` as `Option<String>` and the
  federated principal-re-read anchor. This was examined and is correct.
- Preserve REQ-043: the global credential must continue to administer the
  platform when the connection is absent, removed, misconfigured, or its
  provider is failing. Item 2's screening must refuse *configuration*, never
  degrade credential administration.
- Keep `wyrd-spec` PyO3-free. Only `wyrd-sdk-python` enables Python features.
  Client-tier crates take no `sqlx`, cloud SDK, `datafusion`, or `deltalake`
  dependency.
- No compatibility route, alias, or legacy name.

## Non-goals

- A platform admin UI.
- Tenant OIDC configuration, claim mapping, or group-to-role mapping — owned by
  `SPEC-tenant-oidc-federation`.
- Making the OpenAPI document complete for pre-existing routes outside this
  change.
- Rewriting the tenant login path's discovery client; item 2 is bounded to
  platform connection configuration and the shared screening owner.
- Broadening `AGENTS.md` §11 verification beyond spec `VER-001`..`VER-006`.

## Acceptance criteria

| # | Criterion | Closes |
|---|---|---|
| 1 | A first login whose token does not assert `email_verified: true` is refused with the standard rejection and pins nothing; one that does, pins and succeeds. | FIND-007-3 |
| 2 | Configuring the platform connection with an issuer resolving to a blocked address is refused with the stable contract error, and `jwks_uri` is derived from discovery rather than accepted on the wire. | FIND-007-4 |
| 3 | A `register_admin` whose identity insert fails leaves no `platform.principals` row, and the same name registers successfully afterwards. | FIND-007-5 |
| 4 | A platform principal can be listed and revoked from the platform plane by an authorized caller; a revoked principal's live federated session is refused on its next request. | FIND-007-6 |
| 5 | `PlatformClientAuth`'s `Debug` output contains no secret material, proved directly. | FIND-007-9 |
| 6 | `PlatformSessions::confirm` is proved to yield `Some(credential_id)` for a credential-minted session and `None` for a federated one; the circular extractor test is gone. | FIND-007-7 |
| 7 | The real-provider identity journey covers configuration, pre-registration, first-login pinning, a repeat login, unknown-subject denial with no principal created, bidirectional scope separation for one human, and credential administration while the provider is unavailable. | FIND-007-1, AC-015, AC-016 |
| 8 | A federated human tenant administrator and the tenant administrative principal coexist and produce the same authenticated context shape. | FIND-007-8, AC-011 |
| 9 | Postgres-backed tests prove single-use login state, expiry rejection, one-time pinning under concurrency, and no re-match of a pinned row; a unit test proves nonce rejection. | FIND-007-2 |
| 10 | Each of the Rust, Python, and TypeScript SDKs performs its intended administrative operations against a real server, including receiving a once-returned credential and using it on a subsequent call. | FIND-008-1, FIND-008-2, AC-014 |
| 11 | A tenant-scope client is refused a platform-plane operation through every SDK and through MCP, with the stable contract error. | FIND-008-4 |
| 12 | MCP administrative write tools are unavailable without their scope and available with it; read tools remain available. | FIND-008-3 |
| 13 | The administrative routes appear in `openapi.yaml`, regenerated from source with no hand edits, and `codegen:check` passes. | FIND-008-5 |
| 14 | `crates/wyrd/wyrd-server/src/boot/bootstrap.rs` no longer exists, and no document or example references `bootstrap-key`, `SYSTEM_OPERATOR_ID`, or `system/bootstrap-admin`. | FIND-008-6, AC-013 |
| 15 | Documentation covers the three-command operator journey, the SaaS model, credential rotation, and credential-loss recovery. | FIND-008-7, REQ-040 |

## Verification

Scope remains `VER-001` through `VER-006`. Run only the lanes for the surfaces
touched, and run every named Rust test through its exact focused expression with
the repository-managed Postgres wrapper where required.

```bash
mise run fmt
mise run lints
mise exec -- cargo clippy --locked -p wyrd-auth-oidc -p wyrd-auth -p wyrd-server \
  -p wyrd-client -p wyrd-mcp --all-targets
mise run codegen:check
mise run check:client-tier
mise run check:sdk-client-tier
mise run check:sdk-pyo3-scope
mise run py:format
mise run py:lints
mise run py:typecheck
mise run py:test:unit
mise run docs:check
```

Focused and journey proof:

- `mise run test:platform:journey` — the platform administration journeys,
  including the new registration-atomicity and revocation assertions.
- `mise run test:identity:journey` — the real-provider identity journey
  (items 7 and 8). This is the primary proof for TASK-007 and must pass with
  `WYRD_IDENTITY_E2E=1` against both Keycloak and Dex.
- `mise run test:principals:integration` — extended with the login-state and
  pinning invariant tests (item 9).
- The Rust, Python, and TypeScript SDK journeys, each through its own lane;
  add a `mise` lane for any journey that does not yet have one.
- Every specifically named new Rust test additionally through
  `mise exec -- cargo nextest run --locked -p <crate> <target> -E 'test(=<name>)'`.

Do not run `mise run gate`, `test:rust`, family lanes, the storage matrix, or
any `--all-features` workspace lane: `VER-003` forbids them as evidence.

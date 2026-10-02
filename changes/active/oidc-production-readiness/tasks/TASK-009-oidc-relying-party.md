---
id: TASK-009
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-003, REQ-004, REQ-006, REQ-007, REQ-016, INV-001, INV-004, AC-002, AC-003, AC-006, AC-007, AC-008]
depends_on: [TASK-004]
---

# Server OIDC relying party on `openidconnect`

## Outcome and Value

Tenant login, the connection test sign-in, and platform-administrator login
use one vetted OIDC relying-party library instead of hand-written discovery,
PKCE, token redemption, and ID-token validation. Behavior seen by tenants and
operators does not change; the protocol code Wyrd maintains shrinks to the
SSRF-screened transport, a small per-issuer cache, and the RFC 9207 `iss`
check. This is report item T1 in
[`research/auth-standards-recommendation.md`](../research/auth-standards-recommendation.md).

## Owners, Scope, Consumers, and Prohibited Changes

Owners: `wyrd-auth-oidc` (`provider.rs`), `wyrd-auth` (`login.rs`,
`callback.rs`), and the platform login in `wyrd-server`
(`platform_login.rs`). Consumers: the tenant login and callback routes, the
connection test sign-in, platform-admin login, and the identity journeys.

Libraries and standards, exactly:

- `openidconnect = 4.0.1` (brings `oauth2 ^5`; `rsa 0.9` is already locked).
  Use `CoreProviderMetadata::discover_async`,
  `PkceCodeChallenge::new_random_sha256`, `CsrfToken`, `Nonce`,
  `exchange_code(...).set_pkce_verifier(...).request_async`, and
  `IdTokenVerifier` with `id_token.claims(&verifier, &nonce)`.
- Plug the existing screened transport (`screening.rs`: SSRF screening, DNS
  pinning, https-only, 1 MiB cap, no redirects, no proxy) into the library
  through one small `AsyncHttpClient` adapter over workspace `reqwest 0.13`.
  Do not enable the `oauth2` `reqwest` (0.12) feature.
- Standards the library and kept code cover: OIDC Core 1.0 §3.1 (authorization
  code flow) and §3.1.3.7 (ID Token validation), OIDC Discovery 1.0 §4,
  RFC 7636 §4.3–4.6 (S256), RFC 6749 §2.3.1 (`client_secret_basic` /
  `client_secret_post`), and RFC 9207 §2.4 (`iss` check).

Keep: `screening.rs`; the login-state table as domain state, now storing the
values `openidconnect` generates; the RFC 9207 `iss` check (about 15 lines, per
human direction FIND-TASK-003-1); the connection test by real sign-in; the
per-issuer metadata and JWKS cache on the existing `moka`, with exactly one
forced re-discovery when an ID token names an unknown `kid`. Workload RFC 7523
jwt-bearer verification stays on `jsonwebtoken` through `ExternalVerifier`.

Delete: the hand-written `ProviderMetadata` struct and parse and
`OidcProvider::discover`; `random_b64url` and the hand `Sha256` PKCE
challenge, state and nonce generation in `login.rs`; `TokenEndpointResponse`
and the hand token POST with its `client_secret_basic`/`post` code in
`callback.rs`; and the human ID-token path through `ExternalVerifier`.

Prohibited: provider-specific branches, a second HTTP client that bypasses
screening, following any redirect, changing login state semantics, tenant
selection, role mapping, issuance, or audit. Grant and session changes belong
to TASK-010; do not touch the BFF, device grant, or sealed completion here
beyond what the callback needs to compile.

## Approach

1. Add the screened `AsyncHttpClient` adapter and route all relying-party
   HTTP through it.
2. Replace discovery, PKCE/state/nonce generation, code redemption, and human
   ID-token validation with `openidconnect` for tenant login, connection test,
   and platform login.
3. Shrink the human issuer cache to discovered metadata plus JWKS per issuer
   with one forced re-discovery on an unknown `kid`; keep the RFC 9207 check.
4. Delete the replaced hand-written code and its tests; keep the negative
   journeys.

## Ordered Implementation Scenarios

### Scenario 1 — Login through the library against Keycloak and Dex

**Behavior.** A tenant user signs in through Keycloak and through Dex, a
connection test sign-in marks only the candidate tested, and platform-admin
login still works, all through `openidconnect` (REQ-003, REQ-006, REQ-007,
AC-002, AC-003, AC-006, AC-008).

**RED.** Delete the hand-written discovery, PKCE, token POST, and human
`ExternalVerifier` path first; the existing `identity_e2e` Keycloak and Dex
login journeys and the platform login tests fail to build or fail at login.

**GREEN.** Wire the `openidconnect` calls through the screened adapter until
the existing server identity journeys pass unchanged.

**REFACTOR.** Remove any now-unused types, error variants, and helpers left by
the deleted code.

### Scenario 2 — ID-token and RFC 9207 negatives fail closed

**Behavior.** A bad nonce, issuer, audience, algorithm, or expiry, and a
missing or wrong RFC 9207 `iss` authorization-response parameter, yield no
`User`, credential, or session (REQ-007, AC-007).

**RED.** Run the existing AC-007 negative journeys and RFC 9207 cases against
the Scenario 1 build; any case the library path does not yet reject fails.

**GREEN.** Configure `IdTokenVerifier` and the kept `iss` check until every
negative case refuses; rerun Scenario 1.

**REFACTOR.** Map library errors onto the existing Wyrd error codes without a
second error taxonomy.

### Scenario 3 — Unknown `kid` triggers exactly one re-discovery

**Behavior.** After the provider rotates signing keys, the first ID token with
an unknown `kid` causes one forced re-discovery and then succeeds; a token
whose `kid` is still unknown after that one re-discovery fails closed
(INV-004, AC-007).

**RED.** A journey that rotates the provider key between two logins fails
because the cached JWKS lacks the new `kid`, or re-discovers more than once.

**GREEN.** Add the single forced re-discovery to the per-issuer cache; rerun
Scenarios 1–2.

**REFACTOR.** Keep the cache to discovered metadata plus JWKS keyed by issuer.

### Scenario 4 — Unsafe destinations, redirects, and outages

**Behavior.** An unsafe discovery or JWKS URL is refused before any request; a
redirect response from the provider token endpoint is refused and never
followed (FIND-TASK-004-13, server side); an IdP outage fails closed without
trying another tenant or the platform connection (REQ-007, INV-004, AC-007).

**RED.** Negative tests point discovery/JWKS at a screened address, make the
token endpoint answer 307/308 to a second local origin, and stop the IdP;
before the adapter enforces screening and no-redirect, at least one passes a
request through or reaches the second origin.

**GREEN.** The adapter applies `screening.rs` and a no-redirect policy to
every relying-party call; the second origin observes zero requests. Rerun
Scenarios 1–3.

**REFACTOR.** One adapter, no per-call options.

## Acceptance Criteria

- `identity_e2e` Keycloak and Dex login journeys pass through `openidconnect`
  for tenant login, connection test sign-in, and platform login.
- AC-007 negatives refuse: bad nonce, `iss`, `aud`, `alg`, `exp`; unknown `kid`
  causes exactly one re-discovery; unsafe discovery or JWKS URL; a redirect
  from the token endpoint is refused and the redirect target receives no
  request (FIND-TASK-004-13, screened adapter); IdP outage; the RFC 9207 cases.
- No hand-written discovery, PKCE/state/nonce generation, token POST, or human
  `ExternalVerifier` path remains. `oauth2`'s `reqwest` feature is not enabled.

## Expected Write Set and Consumer Closure

`crates/**/wyrd-auth-oidc`, `crates/**/wyrd-auth` (`login.rs`,
`callback.rs`), `wyrd-server` platform login, workspace and crate
`Cargo.toml`, `Cargo.lock`, the workspace-hack if required, and the server
identity journeys.

## Verification and Evidence

Run every new or changed named test with its exact `mise exec --` selector and
setup. The server journeys run with
`mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=<test> mise run test:identity:journey`;
list the selection first with
`mise exec -- cargo nextest list --locked -p wyrd-server --test identity_e2e --run-ignored=all`.

Then run: `mise run test:identity:journey` unfiltered (targets `server`, `ui`,
`cli`, `rust`, `client`, `python`, `typescript`), `mise run test:shared`,
`mise run test:wyrd-sdk`, `mise run test:cli:journey`,
`mise run test:principals:integration`, `mise run py:test:integration`,
`mise run ts:test:integration`, `mise run test:wyrd`,
`mise run codegen:check`, `mise run docs:check`, `mise run fmt`,
`mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:test:unit`, `mise run py:typecheck`, `mise run ts:test:unit`,
`mise run ts:typecheck`, `mise run ts:napi:check`, and the boundary checks
`mise run check:client-tier`, `mise run check:pyo3-scope`,
`mise run check:unwrap-audit`, and `mise run check:workspace-hack`.

## Material Stop Conditions

Stop and report if a needed behavior has no vetted library and is not covered
by a named RFC section; never write custom protocol logic. Also stop if
`openidconnect` cannot run through the screened transport without enabling its
`reqwest` feature or following redirects, or if a standard provider would
need a provider-specific branch.

## Authority Links

[Approved spec](../spec.md);
[research report](../research/auth-standards-recommendation.md) §3.1 and T1;
[AGENTS.md](../../../../AGENTS.md);
[security posture](../../../../architecture/wyrd-security-posture.md);
[OIDC Core](https://openid.net/specs/openid-connect-core-1_0.html);
[OIDC Discovery](https://openid.net/specs/openid-connect-discovery-1_0.html);
[RFC 7636](https://www.rfc-editor.org/rfc/rfc7636);
[RFC 9207](https://www.rfc-editor.org/rfc/rfc9207).

# OIDC production readiness

- Change: `oidc-production-readiness` (`SPEC-oidc-production-readiness`, approved revision 11)
- Completed: 2026-10-03
- Reviewed base: `b245056712b1d252bab22b619898a6d9ebd2c095`
- Reviewed target: `291e7d15d5dd3f4033777d0990f304708a310b43`
- Delivery reference: not supplied

## Intent and value

Wyrd now supports optional production tenant federation through a tenant's
existing OpenID Connect provider without making provider tokens Wyrd API
authority. A person signs in once and uses the web app, CLI, and first-class
SDKs with short-lived Wyrd credentials. Services and agents keep independent
machine identities, and deployments without an identity provider retain their
existing operator and workload paths.

The same open-source server supports self-hosted and multi-tenant deployments.
Each tenant owns at most one active human connection, while different tenants
may use different standards-compliant providers on the same Wyrd service.

## Shipped behavior

- Tenant administrators can create, inspect in redacted form, test through a
  real sign-in, activate, replace, rotate, deactivate, and remove their OIDC
  connection through the headless API and web settings.
- Tenant login uses authorization code with PKCE, state, and nonce through the
  `openidconnect` Rust library. Provider discovery and JWKS traffic is bounded,
  DNS-screened, address-pinned, proxy-free, and fails closed.
- Wyrd resolves a human as the tenant-scoped `(issuer, subject)` identity and
  maps only that tenant's configured groups to existing roles. Email never
  links identities or grants membership.
- Wyrd is an OAuth authorization server for its clients. It exposes standard
  form-encoded authorization-code, device, refresh, revocation, RFC 8693 token
  exchange, RFC 7523 assertion, and RFC 8414 metadata surfaces with standard
  success and error bodies.
- The SvelteKit BFF uses `openid-client` as confidential client `wyrd-ui`. It
  stores the refresh token, or an operator recovery API key, only in one
  `jose`-encrypted Secure, HttpOnly, SameSite=Lax cookie. The Wyrd API server
  stores no browser session.
- `wyrd auth login` uses RFC 8628 and the platform `webbrowser` library. The
  shared Rust client stores and renews the resulting login for Rust, Python,
  and TypeScript SDKs, including explicit tenant selection, newest-login
  defaulting, locked refresh, and atomic replacement.
- Routine logout deletes local browser or saved-login state and attempts one
  best-effort RFC 7009 revocation. Already issued access tokens remain valid
  only until their bounded expiry.
- Enabling human SSO does not replace API-key exchange, workload assertions,
  platform administration, tenant RBAC, canonical audit, or Postgres tenant
  isolation.

## Lasting invariants and constraints

- Effective tenant identity comes from verified, server-bound state or a
  verified Wyrd credential, never from a path, host, arbitrary header, email,
  or unverified provider token.
- Platform administrators, tenant users, services, and agents remain separate
  identity and authorization planes. Browser, CLI, and SDK login converge on
  the existing tenant User, role, revocation, credential, and audit owners.
- Provider metadata is not trust. TLS, SSRF screening and address pinning,
  issuer/audience/key/time checks, replay protection, audit, and RLS fail
  closed.
- Provider secrets are encrypted under the deployment sealing keyring and are
  absent from responses, browser data, logs, traces, audit, and generated
  artifacts. Wyrd persists only one-way digests for API keys, refresh tokens,
  authorization codes, and device codes.
- Public-client refresh tokens rotate with family-reuse detection. The
  authenticated confidential BFF refresh token does not rotate and has a
  bounded absolute lifetime.
- One userinfo-free URL origin identifies a Wyrd server to clients. OAuth
  audience remains the `client_id`, and migration `20261002000001` remains
  part of the reviewed contract.
- Rust, Python, and TypeScript SDKs project the shared client implementation;
  no language-specific durable identity or token implementation was added.

## Material decisions

- Standard libraries own protocol mechanics: `openidconnect` for Wyrd's OIDC
  relying party, `oauth2` for Rust client grants, `openid-client` for the BFF,
  `jose` for its encrypted cookie, and `webbrowser` for native browser launch.
- API-key authentication uses RFC 8693 token exchange with
  `urn:wyrd:oauth:token-type:api_key`. Machine grants remain clientless; human
  grants identify either public client `wyrd-cli` or confidential client
  `wyrd-ui` as their RFC requires.
- Device authorization admission is an operator ingress responsibility for
  `POST /auth/device`; Wyrd adds no image-local or application-specific rate
  limiter.
- The BFF owns its portable encrypted cookie and the API server remains
  stateless for browser sessions. Cross-replica renewal reconstructs authority
  through the standard refresh or exchange grant.
- Logout revocation is best effort and redirect-free. Wyrd does not add retry
  state, a revocation journal, an access-token denylist, or IdP logout.
- Provider `error_description` is logged for diagnosis without changing the
  public OAuth error shape. Windows support is the installed `webbrowser`
  target; WSL-specific handling remains out of scope.

## Approved revisions and deviations

Revision 11 replaced bespoke protocol and session mechanisms with the vetted
libraries and conventional OAuth/OIDC behavior above. It deleted the private
BFF channel, server-side browser sessions, custom flow cookie and CSRF token,
and sealed login-completion handoff. Tokens are minted when an authorization or
device code is redeemed.

Earlier approved revisions established real interactive connection testing,
the standard RFC 8628 CLI flow, shared saved-login renewal, conventional
local-first logout, OAuth form and error envelopes, and provider-agnostic
qualification against Keycloak and Dex. Revision 11 also deferred SAML 2.0 and
SCIM 2.0 to later approved changes; neither is part of the shipped contract.

Human direction closed the final review of superseded TASK-001 and TASK-003
through direct integrated inspection, and the final documentation correction
through the same change review. No behavioral task-review exception or open
material finding remains.

## Acceptance and evidence closure

| Obligations | Evidence |
|---|---|
| Optional OIDC, tenant connection lifecycle, provider replacement, tenant isolation, and OIDC-off operation | Tenant SQL and auth owners; real-server Keycloak/Dex connection, replacement, refusal, and OIDC-off journeys. |
| OIDC trust, secret handling, stable user identity, role mapping, audit, and workload independence | Screened `openidconnect` relying party, sealed provider configuration, callback/issuance owners, security refusal tests, and machine journeys. |
| Standard authorization server, BFF, CLI, and shared SDK behavior | OAuth endpoint and client owners; two-BFF/two-Wyrd production journeys; CLI, concurrent-renewal, Rust, Python, and TypeScript journeys. |
| Public contracts, documentation, schemas, and non-goals | Served OpenAPI and code-generation evidence, documentation checks, direct final documentation inspection, and the integrated non-goal audit. |

The final integrated identity lane passed on candidate
`48f2e2423136942a11949a1c831c69811c4f45eb`. The reviewed target differs only
by the prior review artifacts and the binding human direction, so the final
review reused that runtime evidence without rerunning unchanged suites.

## Current authorities and owners

- [Wyrd design](../../../architecture/wyrd-design.md),
  [doctrine](../../../architecture/wyrd-doctrine.mdx), and
  [security posture](../../../architecture/wyrd-security-posture.md).
- [OIDC relying party](../../../crates/shared/wyrd-auth-oidc/src/relying_party.rs),
  [provider screening](../../../crates/shared/wyrd-auth-oidc/src/screening.rs),
  [tenant auth](../../../crates/wyrd/wyrd-auth/src/lib.rs), and
  [OAuth wire owner](../../../crates/wyrd/wyrd-server/src/auth/oauth.rs).
- [Shared client auth](../../../crates/shared/wyrd-client/src/auth.rs),
  [saved logins](../../../crates/shared/wyrd-client/src/saved_login.rs),
  [CLI login](../../../crates/wyrd/wyrd-cli/src/auth/login.rs), and
  [BFF browser sessions](../../../crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts).
- [Identity server journeys](../../../crates/wyrd/wyrd-server/tests/identity_e2e.rs),
  [production UI journeys](../../../crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs),
  [Python saved-login journey](../../../sdks/wyrd-sdk-python/tests/integration/auth/test_saved_user_auth.py), and
  [TypeScript saved-login journey](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/saved-user-auth.test.ts).

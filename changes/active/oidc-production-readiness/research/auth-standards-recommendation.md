# Standards-first auth: recommendation for `oidc-production-readiness`

Reviewer: ponytail-researcher (Ponytail ultra). Read-only review of committed `HEAD`
(`wyrd/oidc-production-readiness/TASK-004`, 214 commits, 466 files, +57.6k/-3.5k; ~29k of
that is review packets, ~28k is code/tests). Working-tree edits ignored. No builds run.

## 0. Bottom line

1. **Wyrd stays the OAuth authorization server.** That is already true on `main`. `/auth/token`,
   API-key exchange, refresh, revoke, RFC 7523 jwt-bearer and RFC 8693 delegation all predate this
   change. Wyrd tokens carry a permission snapshot, tenant, Card scope and delegation chain, and
   they are verified locally. No vetted Rust AS library exists. An external AS would need a token
   hook back into Wyrd plus Wyrd-written login/consent code. Wyrd therefore keeps the AS endpoints,
   written to the RFC sections listed below and nothing more.
2. **Everything on the relying-party and client side moves to vetted libraries:**
   `openidconnect` (server RP), `oauth2` + `webbrowser` (Rust client/CLI), and `openid-client`
   (SvelteKit BFF).
3. **Delete the invented protocols.** The custom BFF channel (`/internal/bff/v1/*` + `x-wyrd-bff-key`),
   server-side browser-session rows, and the sealed "login completion" handoff are a re-invented
   **authorization code grant**. Replace them with the real one: the BFF becomes a confidential
   OAuth client of Wyrd. Tokens are minted when the code or device code is redeemed, never stored
   sealed while they wait.
4. **Option A:** keep the branch and replace in place under a spec revision 11. About half the
   committed production code survives, including all of TASK-001's hardened domain code.
5. **Defer SCIM (TASK-006) and SAML (TASK-007) out of this change.** Grafana OSS ships neither.
   Argo CD gets SAML only through Dex, whose SAML connector is self-declared unmaintained. No vetted
   Rust SAML SP exists.

## 1. Library facts (verified 2026-10-02)

| Library | Version (date) | Adoption / maintenance | Notes |
|---|---|---|---|
| `openidconnect` | 4.0.1 (2025-07-06) | 14.3M total, 4.4M recent | Depends on `oauth2 ^5`, `rsa 0.9` (already in Cargo.lock 0.9.10), `p256`/`p384`/`ed25519-dalek`. Supports pluggable HTTP clients. |
| `oauth2` | 5.0.0 (2025-01-21) | 54.6M total, 15.1M recent | Supports auth code + PKCE, **device flow (RFC 8628)**, refresh, **revocation (RFC 7009)**, introspection. **No RFC 8693 / RFC 7523.** Optional `reqwest ^0.12`. Custom `AsyncHttpClient` trait. Docs: *"To prevent SSRF vulnerabilities, be sure to configure the HTTP client not to follow redirects."* Lock today holds only 4.4.2, transitively. |
| `oxide-auth` (AS) | 0.6.1 (2024-06-02) | 3.0M total; no release in 28 months | No device grant. **Rejected.** |
| `webbrowser` | 1.2.4 (2026-08-05) | 50.9M total | Replaces the hand-rolled `open`/`rundll32`/`xdg-open` launcher (`wyrd-cli/src/auth/login.rs:299-327`). |
| std `File::lock` | stable since Rust 1.89 | workspace `rust-version = 1.94.0` | `credentials_file.rs` already uses it. Keep. |
| `tower-sessions` | 0.15.0 (2026-02-01) | 3.9M total | Not needed: Wyrd renders no pages, and sessions live in the BFF. |
| `openid-client` (npm) | 6.8.8 | panva, OpenID-certified; deps `jose`, `oauth4webapi` | Covers BFF code+PKCE, refresh, revoke, and plain OAuth AS (non-OIDC). |
| `@auth/sveltekit` (Auth.js) | n/a | Joined Better Auth Sept 2025; security-patch-only | **Rejected:** maintenance mode, and refresh rotation is DIY. |
| `samael` | 0.0.22 (2026-07-07) | 749k total; pre-1.0; xmlsec1/libxml2 FFI | No vetted pure-Rust SAML SP. Dex's SAML connector: "unmaintained, likely vulnerable". |
| `scim_v2` | 1.0.0 (2026-09-20) | 98k total; 1.0 is 12 days old | Types and filter parser only. |
| `scim-server` | 0.5.3 (2025-09-21) | 155k total | Framework with its own storage model. Not vetted. |
| Ory Hydra / Zitadel / Keycloak | n/a | Hydra needs a Wyrd-written login+consent app. Zitadel has been AGPL-3.0 since v3 (2025-03-31). Keycloak means a realm per tenant on a JVM service. | **Rejected** as Wyrd's AS: each adds a stateful service and breaks REQ-001 (no IdP needed). |

## 2. What comparable projects do

| Project | UI SSO | CLI / API access | Issues own tokens? |
|---|---|---|---|
| Vault | OIDC RP | `vault login -method=oidc`: localhost:8250 listener; Vault redeems the code | Yes, a Vault token |
| Argo CD | Dex or direct OIDC | `argocd login --sso` (PKCE; optional `cliClientID`); project tokens "signed & issued by Argo CD" | Yes for local/automation; SSO uses the IdP JWT |
| GitLab | OmniAuth OIDC/SAML | `glab auth login`: Web (localhost:7171, PKCE) or Device; PATs | Yes (GitLab is an OAuth AS) |
| Harbor | OIDC | "CLI secret" tied to the OIDC refresh | Yes (CLI secret, robot accounts) |
| Grafana | Generic OAuth, `role_attribute_path` mapping | Service account tokens | Yes (own session + SA tokens). SAML and SCIM are Enterprise/Cloud only |
| MLflow | `mlflow-oidc-auth` plugin | Personal access tokens (basic auth) | Yes |

Pattern: every product with its own fine-grained authorization runs its own sessions and tokens
after IdP login. None writes its own OIDC RP mechanics. Wyrd should match that.

## 3. Verdicts per component

### 3.1 OIDC relying party (server)
| Component | Verdict | Detail |
|---|---|---|
| Discovery (`wyrd-auth-oidc/provider.rs` `ProviderMetadata`, `OidcProvider::discover`) | **REPLACE** → `openidconnect 4.0.1` `CoreProviderMetadata::discover_async` | Delete the hand-written metadata struct and parse. |
| PKCE / state / nonce generation (`wyrd-auth/login.rs` `random_b64url`, `Sha256` challenge) | **REPLACE** → `PkceCodeChallenge::new_random_sha256`, `CsrfToken`, `Nonce` | Keep the **login-state table** as domain state; it stores the values openidconnect generates. |
| Code redemption (`callback.rs` `TokenEndpointResponse` + POST; `client_secret_basic`/`post`) | **REPLACE** → `exchange_code(...).set_pkce_verifier(...).request_async` | Delete the hand-written token POST and response type. |
| Human ID-token validation (signature, alg, iss, aud, exp, nonce) via `wyrd-auth-verify` `ExternalVerifier` | **REPLACE** → `IdTokenVerifier` + `id_token.claims(&verifier, &nonce)` | Delete the human path through `ExternalVerifier`. Workload jwt-bearer verification stays on `jsonwebtoken` (resource-server verification; out of scope). |
| JWKS cache for human issuers | **KEEP (shrink)** | Cache discovered metadata+JWKS per issuer (existing `moka`), with one forced re-discovery on an unknown `kid`. About 50 lines. openidconnect does not cache. |
| HTTP transport `screening.rs` (SSRF, DNS pinning, https-only, 1 MiB cap, no redirects, no proxy) | **KEEP** | Hosted multi-tenant fetches customer URLs, and no crate does this. Plug it into openidconnect/oauth2 through a ~25-line `AsyncHttpClient` adapter over workspace `reqwest 0.13`. Do **not** enable `oauth2`'s `reqwest` (0.12) feature. |
| RFC 9207 `iss` check | **KEEP** | ~15 lines per human direction FIND-TASK-003-1. openidconnect does not do it. |
| Platform-admin OIDC login (`platform_login.rs`, on main) | **REPLACE** the same way | Same RP calls. |
| Connection test by real sign-in | **KEEP** | Domain logic per human direction; runs through the same openidconnect path. |

### 3.2 Browser / UI session (BFF)
| Component | Verdict | Detail |
|---|---|---|
| Private BFF channel `bff.rs` (`/internal/bff/v1/{login/options,sessions/*}`, `x-wyrd-bff-key`) | **DELETE** | Replaced by the standard confidential client `wyrd-ui` (RFC 6749 §2.3.1 `client_secret_basic`). The deployment configures the client secret where `WYRD_BFF_SERVICE_KEY_SHA256` is today. |
| `browser_sessions.rs` (1,569), `queries/auth/browser_sessions.rs`, migrations `20261001000001`, `20261001000003` | **DELETE** | Unreleased migrations; editing or deleting them is sanctioned (lead direction FIND-TASK-004-11). Wyrd's durable session is the **refresh-token grant row** it already has. |
| Sealed login completion (`auth_login_state.completion_sealed`, `/login/complete`) | **DELETE** → Wyrd **authorization code** | The code is hashed, ≤60 s, single-use, and bound to `client_id`, exact `redirect_uri`, PKCE S256 challenge, tenant and principal. Tokens are minted at `/auth/token`. |
| `server-sessions.ts` (372), flow cookie, custom CSRF token | **REPLACE** → `openid-client 6.8.8` (code+PKCE, refresh, revoke) + `jose` encrypted HttpOnly cookie | The cookie holds the Wyrd refresh token (or, OIDC-off, the operator API key; REQ-010) plus safe metadata. Access tokens are cached in BFF memory keyed by the token hash; a cache miss runs a refresh. CSRF uses SvelteKit's built-in `csrf.checkOrigin` + `SameSite=Lax`. |
| Refresh rotation for the BFF client | **CHANGE** | Non-rotating for the confidential client (RFC 9700 §2.2.2 requires rotation or sender-constraint only for public clients; RFC 6749 §10.4 binds confidential refresh tokens to client auth). This removes the multi-tab/multi-replica refresh race that drove the server session design. Public CLI tokens keep rotation and family revocation on reuse. |
| Logout | **REPLACE** | RFC 7009 revoke of this session's refresh token, then clear the cookie. This meets FIND-TASK-003-18 ("this login only") by construction. |
| tower-sessions / axum-login / Auth.js | **REJECTED** | The first two are for server-rendered Rust apps; Auth.js is in maintenance mode. |

### 3.3 Wyrd as authorization server (REWRITE-TO-RFC; no vetted Rust library)
| Endpoint / grant | Verdict | Exact RFC sections, nothing more |
|---|---|---|
| `GET /auth/authorize` (new; replaces `/auth/login` + handoff) | **REWRITE-TO-RFC** | RFC 6749 §3.1, §3.1.2 (exact `redirect_uri`), §4.1.1–4.1.2 (code, ≤10 min, single use), §4.1.2.1 errors; RFC 7636 §4.3 + §4.6 (S256 only); RFC 9700 §2.1. Tenant is one extension parameter `tenant=<key>` (RFC 6749 §3.1 permits extension parameters). It is pre-login routing context only (INV-001). |
| `authorization_code` at `/auth/token` | **REWRITE-TO-RFC** | RFC 6749 §4.1.3–4.1.4, §5.1, §5.2; RFC 7636 §4.5–4.6. |
| Device authorization (`cli_logins.rs`, `cli_login.rs`, device SQL) | **REWRITE-TO-RFC** (trim) | RFC 8628 §3.1–3.5, §5.1 (user-code entropy and rate limiting). **Mint at redemption:** the device row stores only "approved by principal P, tenant T, connection C"; delete the sealed-completion path. |
| `refresh_token` grant (`refresh.rs`, main) | **KEEP + REWRITE-TO-RFC** | RFC 6749 §6; RFC 9700 §4.14.2 (rotation + family revoke for **public** clients only). |
| Revocation `/auth/revoke` (main) | **KEEP + RFC wire** | RFC 7009 §2.1–2.2 (200 for unknown tokens). |
| RFC 8693 delegation, RFC 7523 jwt-bearer, API-key exchange (main) | **KEEP** | RFC 8693 §2.1–2.3; RFC 7523 §2.1, §3. API-key exchange as RFC 8693 `subject_token_type` per the existing contract. Wire format only (TASK-008). |
| Errors / wire (TASK-008) | **KEEP** | RFC 6749 §5.1/§5.2, `Cache-Control: no-store`, 401 + `WWW-Authenticate` for `invalid_client`. |
| AS metadata | **OPEN (rec: add)** | RFC 8414 §2–3: one static JSON document. openid-client discovery uses it, and the MCP authorization spec requires it for remote MCP clients. |
| Accept IdP tokens directly (resource-server only) | **REJECTED** | Spec non-goal. IdP access-token audiences are not Wyrd, and per-tenant refusal and group→permission mapping must be Wyrd's. |
| External AS (Hydra/Keycloak/Zitadel/Dex) | **REJECTED** | See §1. |

### 3.4 Client side
| Component | Verdict | Detail |
|---|---|---|
| `wyrd-client/src/auth.rs` `TokenExchange::{post,decode,device_authorization,revoke_refresh_token}` | **REPLACE** → `oauth2 5.0.0` (`default-features=false`, redirect-disabled adapter) | Device poll (`exchange_device_access_token`), refresh, and revoke come from the crate. RFC 8693/7523 keep one form POST through the same client. Keep `AuthMiddleware` caching and single-flight (main). |
| CLI browser launch (`login.rs:299-327`) | **REPLACE** → `webbrowser 1.2.4` | Deletes the Windows-launcher bug class. |
| `canonical_origin` | **KEEP** | Already `url::Url::origin()`. |
| Python / TypeScript | **KEEP** | Both reach `wyrd-client` through PyO3 / N-API (`sdks/*`); no duplicate token logic found. |

### 3.5 Tenant connection administration (TASK-001)
| Component | Verdict |
|---|---|
| `connections.rs`, `queries/auth/human_connections.rs`, `wyrd-spec/auth/human_connection.rs`, `admin/identity.rs`, migration `20260925000000`, UI settings | **KEEP**: Wyrd domain (tenant-owned connection, one active, candidate→test→activate, role mapping, User resolution, canonical audit). No standard covers it. |
| Provider secret sealing (`wyrd-crypt` keyring on `aes-gcm 0.10`; `sealing.rs` rewrap) | **KEEP, narrowed** to provider and workload-issuer client secrets. Delete the browser-session, completion and bootstrap-key columns from the rewrap. |

### 3.6 Local credential store
| Component | Verdict |
|---|---|
| `credentials.toml` (`credentials_file.rs`, `saved_login.rs`): 0600 fail-closed, std `File::lock`, atomic replace, preserve user content | **KEEP**. This is what gh, gcloud and aws do; it already uses std. |

### 3.7 Remaining tasks
| Task | Verdict |
|---|---|
| TASK-005 docs | **KEEP**, retarget to the new flows. |
| TASK-006 SCIM | **DEFER** (open decision 1). If kept: REWRITE-TO-RFC minimum: RFC 7644 §3.3, §3.4.1, §3.4.2.2 (`eq` on `userName`/`externalId` only), §3.5.1, §3.5.2 (`replace active`, Group `members` add/remove), §3.6, §4 static discovery docs; RFC 7643 §4.1–4.2 core attributes. `scim_v2 1.0.0` only for types, optionally. |
| TASK-007 SAML | **DEFER** (open decision 1). Enterprise IdPs (Okta, Entra, Google, Ping, OneLogin) all speak OIDC. SAML-only shops put Keycloak in front as a SAML→OIDC broker, which Wyrd already proves against. If ever built: `samael` behind `xmlsec`, isolated crate. |
| TASK-008 wire format | **KEEP**, merged into task T2 below. Add the `authorization_code` grant to its endpoint set. |

## 4. Path forward: Option A (keep branches, replace in place)

Evidence: production code on the branch splits roughly as follows (line counts from `git diff --numstat main...HEAD`).

| Bucket | ~Lines | Files |
|---|---|---|
| Survives unchanged (domain) | ~6.0k | connections, human_connections SQL/spec/admin/migration, UI settings + login page, credentials file + saved logins, keyring, issuance/refresh/revoke/pg_resolvers changes, Keycloak+Dex fixtures |
| Reworked onto libraries | ~4.5k | login, callback (server + auth), cli_logins/device, client `auth.rs`, CLI login |
| Deleted | ~2.9k | browser_sessions (Rust+SQL+2 migrations), `bff.rs`, `server-sessions.ts`, `/login/complete`, completion sealing |

Option B would re-derive the ~6k lines of TASK-001/004 domain code that survived 5–12 review
rounds, and would re-fight decisions the human has already made (one active connection, real
test sign-in, `iss`, credentials.toml, device grant). Option A deletes more than it adds.
Process: freeze the old review packets as superseded, add spec revision 11, and run the tasks
below on top of TASK-004. Merge to main as one squash per task after the final change review.

### Ordered tasks
| # | Outcome | Libraries | Delete | Proof (journeys must pass) |
|---|---|---|---|---|
| T1 | Server RP on openidconnect for tenant login, connection test and platform login | `openidconnect 4.0.1`; screened `AsyncHttpClient` adapter | Hand PKCE/nonce/state, token POST, `ProviderMetadata`, human `ExternalVerifier` path | `identity_e2e` Keycloak + Dex login; AC-007 negatives: bad nonce/iss/aud/alg/exp, unknown `kid` → one re-discovery, unsafe discovery/JWKS URL, redirect from the token endpoint refused, IdP outage; RFC 9207 cases |
| T2 | Wyrd AS standard grants + RFC wire (absorbs TASK-008 server side): `/auth/authorize`, `authorization_code`, device grant minted at redemption, refresh (rotate public only), RFC 7009 revoke, form bodies, RFC 6749 §5 errors, clients `wyrd-ui` (confidential) / `wyrd-cli` (public) | none new (`axum`, `jsonwebtoken`, `sqlx`) | `bff.rs`, `browser_sessions.rs` + SQL + migrations, `completion_sealed`, sealed rewrap of session columns, JSON request bodies | Off-the-shelf clients against a real server: `oauth2` crate device login + refresh; `openid-client` code+PKCE. Negatives: wrong/expired/replayed code, PKCE mismatch, wrong `redirect_uri`, wrong client secret → `invalid_client` 401, device `authorization_pending`/`slow_down`/`access_denied`/`expired_token`, CLI refresh reuse → family revoked, BFF refresh not rotated, JSON body → `invalid_request` |
| T3 | BFF on openid-client: login, callback, encrypted cookie session, tenant switch, OIDC-off API-key login, logout = revoke | `openid-client 6.8.8`, `jose` (its dependency) | `server-sessions.ts` protocol code, flow-cookie code, custom CSRF, `/login/complete` | UI journeys AC-002/003; two BFF replicas share a session (cookie); no token in page data or URL; old-connection session cannot refresh after replacement (REQ-016); logout revokes only this login (FIND-TASK-003-18 proof) |
| T4 | Client on oauth2 + webbrowser | `oauth2 5.0.0`, `webbrowser 1.2.4` | `TokenExchange` hand POST/decode, `open_browser` launcher | AC-004 Rust/Python/TS saved-login journeys; CLI journey; concurrent refresh under the file lock; `--no-browser` |
| T5 | Docs + architecture authority (TASK-005, retargeted) | — | Prose about the handoff, BFF channel and browser-session sealing | `docs:check`, `codegen:check`, full identity lane |

Order: T1 ∥ (T2 → {T3, T4}) → T5. T1 and T2 touch `callback.rs`, so sequence them if
running in one worktree.

## 5. Spec changes (revision 11, recommended wording)

| Where | Replace with |
|---|---|
| New constraint (Scope) | "Protocol mechanics use vetted libraries: `openidconnect` for the server relying party, `oauth2` for the Rust client, and `openid-client` for the BFF. Wyrd implements only the authorization-server endpoints no vetted Rust library provides, limited to the RFC sections named in REQ-021, plus its SSRF-screened provider transport." |
| REQ-005 | "A provider client secret is accepted only at an authorized server boundary, encrypted at rest under the deployment sealing keyring, and absent from responses, browser data, logs, traces, errors, audit and generated artifacts. Wyrd stores no recoverable access token, refresh token or API key; it stores only one-way digests. The keyring is required only when a provider secret is stored. Rotation keeps existing connections usable and is documented and tested." |
| REQ-009 | "The BFF is a confidential OAuth client of Wyrd. It signs a person in with the authorization code grant and PKCE (RFC 6749 §4.1, RFC 7636 S256) at Wyrd's authorize endpoint, which federates to the tenant's provider. Wyrd's authorization code is single-use, expires within 60 seconds, and is bound to the client, exact redirect URI and PKCE verifier. The browser holds only a Secure, HttpOnly, SameSite=Lax cookie encrypted by the BFF, containing the Wyrd refresh token (or, OIDC-off, the operator credential) and safe metadata. It never sees a token in page data, URL or JavaScript. Logout revokes that refresh token (RFC 7009) and clears the cookie; it does not end the IdP session." |
| REQ-011 | Keep. Add: "Wyrd mints the credential when the device code is redeemed; no issued credential is stored awaiting pickup." |
| REQ-012 | Add: "Refresh tokens issued to public clients (the CLI) rotate on every use and reuse revokes the family (RFC 9700 §4.14.2). Refresh tokens issued to the confidential BFF client do not rotate and have a bounded absolute lifetime." |
| REQ-021 | Add `authorization_code` and the authorize endpoint (RFC 6749 §3.1, §4.1; RFC 7636). List exact sections: RFC 6749 §2.3.1, §3.1, §3.1.2, §4.1, §5.1, §5.2, §6; RFC 7636 §4.3–4.6; RFC 7009 §2; RFC 8628 §3.1–3.5; RFC 8693 §2; RFC 7523 §2.1, §3; [RFC 8414 §2–3 if decision 3 is yes]. |
| Decision 7 | "A completed login reaches its client only through a standard grant: the authorization code grant for the BFF, and the device grant for the CLI. Tokens are minted at redemption. The deployment keyring protects provider secrets only." |
| REQ-019, REQ-020, AC-010, AC-011, decisions 8–9, INV-007 SAML/SCIM mentions | Move to "Deferred to a later change" (decision 1). |
| AC-007 | Replace "BFF session behavior across two serving replicas" with "a BFF cookie session works across two BFF replicas and two Wyrd replicas"; drop "absent or rotated sealing keys" for sessions, keep it for provider secrets. |
| Security posture "Access and refresh tokens" | "rotated on every successful use" → "rotated on every use for public clients". |

## 6. Open human decisions (each with a recommendation)

1. **Defer SCIM and SAML (TASK-006/007) from this change?** **Recommend yes.** No vetted Rust
   implementation exists. Grafana OSS ships neither, and Argo CD delegates SAML to an unmaintained
   Dex connector. Keycloak brokers SAML→OIDC today. Revisit when a paying tenant needs it, possibly
   in the commercial distribution.
2. **BFF session as an encrypted cookie plus non-rotating confidential-client refresh tokens,
   deleting the Wyrd-side browser-session table?** **Recommend yes.** Wyrd's refresh-grant row is
   the durable, revocable, audited session, and the cookie is a client cache, like
   `credentials.toml`. The alternative keeps ~2.3k lines of Wyrd-side session code and the refresh
   race it exists to manage.
3. **Publish RFC 8414 AS metadata?** **Recommend yes.** It is a static document. openid-client
   discovers from it, and the MCP authorization spec requires it for remote MCP clients. This
   reverses TASK-008's "no discovery document".

## Sources
- crates.io API: [openidconnect](https://crates.io/crates/openidconnect), [oauth2](https://crates.io/crates/oauth2), [oauth2 5.0.0 deps](https://crates.io/api/v1/crates/oauth2/5.0.0/dependencies), [openidconnect 4.0.1 deps](https://crates.io/api/v1/crates/openidconnect/4.0.1/dependencies), [oxide-auth](https://crates.io/crates/oxide-auth), [samael](https://crates.io/crates/samael), [webbrowser](https://crates.io/crates/webbrowser), [tower-sessions](https://crates.io/crates/tower-sessions), [scim_v2](https://crates.io/crates/scim_v2), [scim-server](https://crates.io/crates/scim-server)
- [oauth2 docs (flows, redirect/SSRF warning)](https://docs.rs/oauth2/5.0.0/oauth2/)
- [openid-client on npm registry](https://registry.npmjs.org/openid-client/latest)
- [Rust 1.89 File::lock](https://releases.rs/docs/1.89.0/)
- [Auth.js joins Better Auth](https://news.hada.io/topic?id=23316), [@auth/sveltekit](https://www.npmjs.com/package/@auth/sveltekit)
- [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html)
- [MCP authorization 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)
- [Argo CD user management](https://argo-cd.readthedocs.io/en/stable/operator-manual/user-management/), [Argo CD security](https://argo-cd.readthedocs.io/en/stable/operator-manual/security/)
- [Harbor OIDC](https://goharbor.io/docs/main/administration/configure-authentication/oidc-auth/)
- [Vault JWT/OIDC auth](https://developer.hashicorp.com/vault/docs/auth/jwt)
- [Grafana generic OAuth](https://grafana.com/docs/grafana/latest/setup-grafana/configure-security/configure-authentication/generic-oauth/), [Grafana service accounts](https://grafana.com/docs/grafana/latest/administration/service-accounts/), [Grafana Enterprise (SAML)](https://grafana.com/docs/grafana/latest/introduction/grafana-enterprise/)
- [GitLab CLI authentication](https://docs.gitlab.com/cli/authentication/)
- [mlflow-oidc-auth](https://github.com/mlflow-oidc/mlflow-oidc-auth)
- [Dex SAML connector warning](https://dexidp.io/docs/connectors/saml/), [Dex deprecation discussion](https://github.com/dexidp/dex/discussions/1884)
- [Ory Hydra](https://github.com/ory/hydra), [Zitadel AGPL](https://zitadel.com/blog/apache-to-agpl)

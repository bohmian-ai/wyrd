# OAuth/OIDC Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`

The candidate remained `e3a47a05d931c010f4c70c75edea2d23c447108b` throughout this review. The repository has no `.codegraph/` directory, so source navigation used repository-native `rg`, `git diff`, and direct source reads.

## Reviewed boundary

This review traced the security and credential-lifecycle statements added or retained by TASK-005 across:

- tenant-provider OIDC connection setup, callback binding, issuer/subject identity, group-to-role mapping, and provider-secret sealing;
- Wyrd's OAuth authorization-server surface: authorization code plus PKCE, RFC 8628 device authorization, public- and confidential-client refresh behavior, RFC 7009 revocation, RFC 8693 token exchange, RFC 7523 JWT bearer assertions, and RFC 8414 metadata;
- the `wyrd-ui` confidential-client boundary, encrypted cookie session, access-token cache, API-key recovery, tenant cookie binding, and logout;
- the `wyrd-cli` public-client device flow, saved-login selection, refresh rotation, and local-first best-effort logout;
- sealing/signing key operator guidance, tenant separation, machine/human principal separation, and OAuth error-wire documentation; and
- shared Rust, Python, and TypeScript client documentation for saved-login selection and selected-tenant mismatch behavior.

The standing decisions were treated as closed: RFC 8693 API-key exchange with `urn:wyrd:oauth:token-type:api_key`; ingress-owned rate limiting of `POST /auth/device`; best-effort RFC 7009 logout revocation; client base URLs reduced to their origin; and self-contained access tokens remaining valid until expiry after logout.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer evidence inspected | Result |
|---|---|---|---|
| Tenant OIDC trust and callback | Spec REQ-001, REQ-004–009, INV-001–004; `architecture/wyrd-security-posture.md` delegation/federation | `wyrd-auth` connection/callback owners, `wyrd-server/src/auth/{authorize,callback}.rs`, connection docs and provider setup | PASS |
| OAuth clients and grants | Spec REQ-009, REQ-011, REQ-021; RFC sections fixed by revision 11 | `wyrd-server/src/auth/{oauth,authorize,cli_login}.rs`, `components/auth/routes.rs`, `wyrd-auth/src/{issuance,refresh,cli_logins}.rs`, `wyrd-spec/src/auth/token.rs` | FAIL: documentation mismatches in SEC-002 and SEC-003 |
| BFF cookie and logout | Spec REQ-009–010; encrypted-cookie decision | `wyrd-ui/src/lib/server/auth/browser-sessions.ts`, login/callback/API-key routes, production auth integration tests, UI README and auth docs | FAIL: operator documentation mismatch in SEC-001 |
| CLI saved credential lifecycle | Spec REQ-011–012; local-first best-effort logout | `wyrd-cli/src/auth/login.rs`, `wyrd-client/src/{auth,saved_login,config}.rs`, CLI/client docs and SDK source documentation | PASS |
| Refresh-token profiles | Spec REQ-012, REQ-016; RFC 9700 §4.14.2 | `wyrd-auth/src/{refresh,issuance}.rs`, refresh-token SQL, security posture and concept docs | PASS |
| Machine identity and tenant binding | Spec REQ-013, INV-003–005 | JWT-bearer handler and verifier, `TokenRequest::JwtBearer`, API-key exchange docs, workload docs | PASS |
| Secrets and key lifecycle | Spec REQ-005, AC-007 | BFF HKDF/A256GCM cookie sealing, server sealing-key configuration and rewrap owner, self-hosting/configuration/Kubernetes docs | PASS |
| OAuth error exception | Spec REQ-021; TASK-005 approach and AC-009 | `OAuthForm`, `OAuthClients`, `OAuthError`, token/device/revocation handlers, generated error-doc source and rendered docs | FAIL: SEC-002 and SEC-003 |

## Security Audit

### Critical

None.

### High

None.

### Medium

- `SEC-001` — [`docs/src/content/docs/self-hosting/authentication.svx:11`](../../../../../docs/src/content/docs/self-hosting/authentication.svx) says there is no session cookie, while the shipped confidential BFF deliberately stores each tenant's refresh token or recovery API key in an encrypted Secure, HttpOnly, SameSite=Lax cookie. The implementation is explicit at `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:23-24,34,98-125,172-177`. This is not merely wording: it gives operators a false credential-custody and rotation model on the page that explains production key material, obscuring why `WYRD_UI_CLIENT_SECRET` confidentiality and rotation affect active web sessions. Fix the operator page to distinguish the stateless Wyrd API server from the BFF: Wyrd trusts a bearer token per API request, while the BFF owns the encrypted browser session cookie. Preserve the existing claims that the browser cannot read the cookie and that Wyrd has no server-side browser-session store.

- `SEC-002` — [`docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`](../../../../../docs/src/content/docs/self-hosting/sso-and-oidc.svx) and [`architecture/wyrd-security-posture.md:208`](../../../../../architecture/wyrd-security-posture.md) state or imply that successful token, platform-token, device-authorization, and revocation requests all use the RFC 6749 §5.1 JSON body. The shipped contracts differ by standard: token endpoints return the §5.1 token response; device authorization returns `DeviceAuthorization` under RFC 8628 §3.2 (`wyrd-server/src/auth/cli_login.rs:43-90`); successful revocation returns an empty `200` under RFC 7009 (`wyrd-server/src/auth/cli_login.rs:265-307`). A standards-based client or operator following the guide can therefore require a token-shaped JSON body where none is shipped. Correct the shared public and architecture descriptions to scope §5.1 to successful token responses and name the RFC 8628 device response and RFC 7009 empty revocation success separately. Do not invent another envelope.

- `SEC-003` — [`docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`](../../../../../docs/src/content/docs/self-hosting/sso-and-oidc.svx), [`architecture/wyrd-security-posture.md:210`](../../../../../architecture/wyrd-security-posture.md), [`docs/scripts/generate_api_docs.py:121`](../../../../../docs/scripts/generate_api_docs.py), and its generated [`docs/src/content/docs/api/errors.md:13`](../../../../../docs/src/content/docs/api/errors.md) promise that every OAuth refusal is logged under a Wyrd error code. Only refusals converted from `WyrdError` take the logging path in `OAuthError::from` (`wyrd-server/src/auth/oauth.rs:100-123`). Protocol-native refusals created directly by form parsing or client identification—such as repeated parameters, wrong content type, malformed Basic credentials, an unknown client, or a contradictory `client_id`—return `OAuthError` directly (`oauth.rs:162-229,294-352`) and have no Wyrd code to log. This false incident-response promise can cause operators to depend on a correlation field that is absent for hostile malformed requests and client-auth failures. Since TASK-005 is documentation-only and revision 11 does not require a new logging contract, remove or accurately qualify the blanket logging promise at its source and regenerate the error page. Do not add a parallel error identity or bespoke logging mechanism for this docs task.

### Low / Defense In Depth

None. No optional hardening or style preferences are proposed.

### Positive Controls

- The BFF uses `openid-client` for discovery, code redemption, refresh, and revocation, and `jose` A256GCM encryption for the HttpOnly cookie; it does not expose the session credential or access token through page data.
- Authorization requests use PKCE S256 and state; Wyrd fixes and exactly matches the `wyrd-ui` redirect URI and binds authorization codes to client, redirect, verifier challenge, tenant, and principal.
- The OAuth server stores only SHA-256 digests of the BFF client secret and supports a two-digest rotation overlap; the raw secret remains in the BFF.
- Public-client refresh tokens rotate and replay revokes the affected chain; confidential-client refresh tokens authenticate on every use, do not rotate, and have a fixed absolute lifetime.
- Logout clears local browser or CLI state first and performs RFC 7009 revocation best-effort without falsely claiming already issued access tokens disappear.
- Provider client secrets are sealed at rest and provider tokens never become Wyrd authorization. Tenant users resolve by verified issuer and subject, never email.
- The device approval form is origin-checked and frame-protected, and the documentation correctly assigns its brute-force rate limit to the public ingress per the approved deployment boundary.
- Machine API-key and JWT-bearer paths remain separate from human OIDC roles and refresh authority.

## Material proposed findings

### SEC-001 — INCORRECT: operator authentication guide denies the shipped BFF session cookie

- Violated obligation: TASK-005 outcome and AC-009; spec REQ-009 and REQ-018 require documentation of the confidential BFF's encrypted cookie session and correct credential ownership.
- Location: `docs/src/content/docs/self-hosting/authentication.svx:11`.
- Evidence: the text says “There is no session cookie”; `browser-sessions.ts:23-24,34,98-125,172-177` stores the refresh token or recovery API key in an encrypted Secure, HttpOnly, SameSite=Lax cookie.
- Observable consequence: an operator reading the production key guide receives the wrong browser credential-custody and session-rotation model.
- Required testable correction: qualify the statement to the Wyrd API server and state that the BFF owns the encrypted cookie. Prove closure with `mise run docs:check` and a static comparison against `browser-sessions.ts`; no journey is required.

### SEC-002 — INCORRECT: OAuth success bodies are documented as one RFC 6749 §5.1 shape

- Violated obligation: spec REQ-021 and TASK-005 AC-009 require the public docs and architecture to agree with the standard wire contracts.
- Locations: `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`; `architecture/wyrd-security-posture.md:208-212`.
- Evidence: device authorization returns the RFC 8628 `DeviceAuthorization` JSON object (`cli_login.rs:43-90`), revocation returns an empty `200` (`cli_login.rs:265-307`), and token endpoints return the token body.
- Observable consequence: a conventional OAuth client or operator can enforce the wrong success decoder for device authorization or revocation.
- Required testable correction: describe the three existing standard success contracts separately, retaining form encoding, RFC error JSON, and no-store behavior. Prove closure with `mise run docs:check`; run `mise run codegen:check` only if generated API wording is changed.

### SEC-003 — INCORRECT: docs promise a Wyrd code in logs for every OAuth refusal

- Violated obligation: TASK-005 AC-009 and the task's requirement that documentation describe shipped behavior accurately.
- Locations: `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`; `architecture/wyrd-security-posture.md:210-212`; `docs/scripts/generate_api_docs.py:121`; generated `docs/src/content/docs/api/errors.md:13`.
- Evidence: `OAuthError::from(WyrdError)` logs `wyrd_code` (`oauth.rs:100-123`), but `OAuthForm` and `OAuthClients` directly construct protocol errors (`oauth.rs:162-229,294-352`) without a Wyrd error or Wyrd code.
- Observable consequence: incident-response guidance promises a stable correlator that is absent for malformed and client-authentication refusals.
- Required testable correction: remove or narrow the promise to refusals that originate from a Wyrd catalog error, update the generator source rather than hand-editing generated output, and preserve the standard OAuth wire exception. Prove closure with `mise run docs:check` and `mise run codegen:check`.

## Verification limits

- Review was static and source-backed. I did not run user journeys or repository aggregates, in accordance with the caller's instruction that those run once at change review.
- TASK-005 records successful `docs:check`, `codegen:check`, `ts:napi:check`, formatting, lint, and language-format lanes. Those checks prove build/generation consistency, not the semantic accuracy issues above.
- No live IdP, network, database, or browser flow was executed in this domain pass.
- No dependency manifest or lockfile changed in the base-to-candidate range, so no new supply-chain surface was introduced by TASK-005.
- No production executable behavior was changed by the candidate; the material findings concern false or overbroad security/public-contract documentation, not a newly introduced exploit path.

## Overall result

**FAIL**

The implementation controls inspected are conventional and coherent, and no Critical or High implementation vulnerability was found in this documentation-focused candidate. The three material documentation inaccuracies remain within TASK-005's explicit obligation to describe the shipped OAuth/OIDC and credential-lifecycle behavior accurately.

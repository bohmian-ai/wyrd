# OAuth/OIDC and Browser Security Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `4d468b33e49de4dd9df30c5dd046a334569465bf`
- Candidate tree: `87acdce15e3ca6ea2b6016969695db398ab2d598`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-011-r1/TASK-011-R1-browser-session-standards-and-logout.md`
- Binding reversal: `changes/active/oidc-production-readiness/review/TASK-011-r1/lead-direction-FIND-TASK-011-2.md`

The candidate commit and tree matched the supplied immutable subject before and
after inspection. The repository has no `.codegraph/` directory, so source
navigation used the repository's existing search and Git tools. This report
changes no reviewed source.

## Reviewed boundary

This review covers the BFF's OAuth/OIDC and browser trust boundaries:
authorization-code initiation and completion, state and S256 PKCE, exact
redirect binding, confidential-client authentication, opaque refresh
credentials, RFC 8693 API-key recovery exchange, encrypted session cookies,
access-token caching, refresh and revocation, CSRF, tenant binding,
browser-visible data and errors, secret logging, dependency selection, and the
relevant production-journey assertions.

| Boundary | Authority and source coverage | Assessment |
|---|---|---|
| Authorization code, state, and PKCE | Revision-11 REQ-006, REQ-009, INV-001; RFC 6749 section 4.1; RFC 7636 sections 4.3-4.6; installed `openid-client` 6.8.8; `browser-sessions.ts:128-233`; login and callback routes | PASS. Library-generated state and verifier are held in the five-minute encrypted login cookie. `authorizationCodeGrant` validates the exact state, supplies the verifier, derives the callback URI from the configured SvelteKit origin, and processes OAuth errors. The cookie is removed before completion and Wyrd owns single-use code redemption. |
| Confidential client and transport | TASK-011; RFC 6749 section 2.3.1; `browser-sessions.ts:113-164`; `upstream.ts`; Wyrd authorization-server metadata and redirect validation | PASS. `ClientSecretBasic` is used through `openid-client`; only public-origin endpoints are routed to the deployment-controlled internal Wyrd origin. Non-loopback internal HTTP is refused. SvelteKit adapter-node's conventional `ORIGIN` fixes the public origin used by the real BFF processes, while Wyrd independently publishes and enforces its deployment public origin and exact callback. |
| Refresh-token opacity and renewal | RFC 6749 sections 1.5 and 6; `openid-client.refreshTokenGrant`; prior `FIND-TASK-011-1`; `browser-sessions.ts:249-311`; `browser-sessions.test.ts:69-92` | PASS. `establish` no longer decodes or otherwise interprets the refresh credential. The opaque string is authenticated-encrypted into the existing cookie and later passed unchanged to the library refresh operation. The ordinary twelve-hour application-session bound is independent of token representation, introduces no new setting or protocol mechanism, and Wyrd refusal remains the credential authority. |
| Cookie confidentiality and integrity | REQ-009; `jose` 6.2.12; `browser-sessions.ts:10-35,120-126,172-187,249-277,313-334` | PASS. Host-only Secure, HttpOnly, SameSite=Lax cookies use authenticated JWE encryption (`dir`/`A256GCM`) with an HKDF-SHA256 domain-separated key derived from the confidential-client secret. Forged, expired, undecryptable, or cross-tenant cookies fail closed. The browser receives no token in page data, URLs, or JavaScript. |
| API-key recovery and tenant authority | Binding lead decisions in TASK-011; REQ-010, INV-001, INV-005; `browser-sessions.ts:235-311`; API-key recovery route; hooks and API callers; production journey | PASS. Recovery remains separate from routine SSO. The route key is presentation context; the exchanged token's tenant and server-enforced permissions are the authority. No endpoint, claim, mapper, or route-tenant trust mechanism was added. Cross-tenant recovery remains confined by the server token. |
| CSRF and browser actions | TASK-011; SvelteKit 2.70.3 built-in `csrf.checkOrigin`; SameSite=Lax; `svelte.config.js`; login, switch, settings, and logout actions; production cross-site assertions | PASS. The candidate uses the framework's conventional origin check and cookie policy rather than a duplicate CSRF token. The real built-server proof includes cross-site form refusal for recovery sign-in and authenticated mutations. |
| Logout and revocation | REQ-009; RFC 7009 section 2; binding lead reversal of `FIND-TASK-011-2`; `browser-sessions.ts:336-358`; `browser-sessions.test.ts:94-113` | PASS. Logout always removes the selected local cookie and this replica's cache entry before best-effort library revocation. API-key logout never revokes the operator key. Revocation failure is contained to a structured warning with tenant and error class only; it exposes no token and does not keep the user signed in or surface a failed logout. This is the required conventional direction and the original retry-preservation finding is not reopened. |
| Tenant switch, cache, and bearer limits | REQ-015, REQ-016; approved TASK-011 limit; `browser-sessions.ts:264-383`; hooks, layout, and production journeys | PASS. Credential-hash cache keys do not become tenant authority; every protected call presents the server-issued access token to Wyrd. A target switch reads only that target's authenticated cookie. Logout and connection replacement do not claim to invalidate already issued self-contained access tokens before their bounded expiry. |
| Secret and error exposure | REQ-005, REQ-009, AC-004; `BrowserSession.#token`; callback failure handling; logout warning; safe page metadata; production secret scans | PASS. Credentials remain in server-only memory or authenticated-encrypted cookies. OAuth callback detail is reduced to a generic failure. The new warning emits only the route tenant and error name. No client secret, API key, refresh token, access token, provider response, or token hash is logged or returned in the inspected paths. |
| Dependencies and supply chain | `package.json`; `pnpm-lock.yaml`; installed `openid-client` 6.8.8, `oauth4webapi` 3.8.8, and `jose` 6.2.12 | PASS. The task-selected packages and transitive versions are exact and integrity-locked. Protocol and cryptographic operations use their documented native interfaces; no parallel hand-written OAuth or cryptography entered the candidate. Prior read-only audit evidence found no advisory in this added dependency cone. |

## Prior-finding closure

- `FIND-TASK-011-1` is closed. The only producer of refresh sessions now keeps
  the credential opaque, and every sibling consumer (`read`, `switch`,
  `metadata`, and `logout`) uses the authenticated cookie without inspecting a
  refresh-token payload. The focused test proves a non-JWT refresh string is
  sealed, forwarded unchanged to `refreshTokenGrant`, and cleared on a
  terminal server refusal.
- `FIND-TASK-011-2` is reversed by binding lead direction and is not a live
  finding. The candidate implements the approved conventional behavior:
  immediate local sign-out with best-effort RFC 7009 revocation. The focused
  test proves cookie and cache clearing on transport failure and proves the
  warning excludes the token.
- `FIND-TASK-010-1` remains closed: the cumulative production BFF uses
  `openid-client` for code plus PKCE, refresh, and revocation against Wyrd's
  standard endpoints.

## Verification evidence and limits

The remediation records successful UI check, full UI unit lane, Rust format
and lints, the filtered production identity journey, and both focused tests.
This reviewer independently reran the narrow focused proofs:

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts -t 'accepts an opaque refresh token and forwards it unchanged'` — 1 passed;
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts -t 'failed refresh-token revocation still signs out'` — 1 passed; and
- `git diff --check 7c48ac7c9..4d468b33e49de4dd9df30c5dd046a334569465bf` — passed.

Per current review direction, no full identity or every-language journey was
rerun in task review; those aggregate journeys belong at change review. This
is not a security acceptance gap for the remediation because the two changed
security properties have direct focused proof and the cumulative journey
evidence is recorded against the same runtime paths.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. Additional nonce or JWT verification, a second CSRF token, an origin
allowlist, revocation retry state, server-side browser sessions, refresh-token
parsing, or a new lifetime setting would duplicate existing standard or native
controls or contradict the approved task and standing direction.

### Positive Controls

- `openid-client` owns discovery, confidential-client authentication, code
  response processing, state, PKCE, refresh, and revocation.
- `jose` authenticated encryption keeps credentials portable across replicas
  without exposing them to JavaScript or adding a server-side session store.
- The refresh credential is now opaque end to end at the BFF boundary.
- Wyrd remains the sole tenant and permission authority; UI claim projection
  does not replace server verification on protected calls.
- Local logout is unconditional, revocation is best-effort, and failure logs
  are secret-free, matching the binding standards direction.
- SvelteKit's native CSRF behavior and SameSite cookie control have real
  cross-site refusal coverage.

## Material proposed findings

None.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed OAuth/OIDC and browser
security boundary. The opaque-refresh remediation closes the remaining live
security-domain finding, the reversed logout direction is implemented exactly,
and no additional standards or comparable-project mechanism is warranted.

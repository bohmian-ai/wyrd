# Security and identity domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved authority: `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md` (`SPEC-oidc-production-readiness`, revision 5)
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior review and remediation: `changes/active/oidc-production-readiness/review/TASK-003-r1/{verdict.md,findings-validation.md,TASK-003-R1-production-ui-remediation.md}`
- Replacing human authority: `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`, which replaces `FIND-TASK-003-1` and `R1-AC-01`

The candidate resolved to the same immutable object immediately before this report was written.

## Reviewed boundary

This pass traced the security, identity, and tenancy boundary through:

- callback query parsing, server-owned login-state consumption, connection revision binding, fresh provider discovery, conditional RFC 9207 issuer validation, token exchange, failure audit, and completion creation;
- OIDC provider metadata projection and the explicit human decision that non-advertising providers remain usable;
- BFF service-key admission, internal route mounting, upstream transport validation, secret-safe tracing, and typed session responses;
- opaque flow and session identifiers, tenant-definer lookups, RLS re-entry, authoritative tenant projection, CSRF/origin enforcement, chooser verification, tenant switching, authority retrieval, renewal, and logout;
- OIDC-off API-key admission through the shared fixed-cost verifier, including malformed, unknown-route, cross-tenant, unknown-prefix, wrong-secret, revoked/expired, and valid paths;
- browser-session ciphertext inventory, compare-and-swap rewrap, keyless boot refusal, concurrent renewal, K2-only recovery, and credential wiping on logout;
- two-provider and same-issuer multi-tenant behavior, replacement activation, old-connection cutoff, same-email non-inheritance, recovery authority, and redacted settings projection;
- browser/page/URL/log exposure and the real server/two-BFF security journeys.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Callback issuer binding | Human direction replacing `FIND-TASK-003-1`/`R1-AC-01`; approved REQ-007, INV-001/004; [RFC 9207 §§2.4, 3](https://www.rfc-editor.org/rfc/rfc9207.html#section-2.4); [RFC 9700 §4.4.2](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.4.2); OIDC Core | `wyrd-spec/src/auth/oidc.rs`; `wyrd-auth-oidc/src/provider.rs`; `wyrd-auth/src/callback.rs`; server callback adapter/route; callback schemas; `tenant_callback_issuer_binding_journey` | PASS |
| Tenant and connection authority | Approved REQ-002/006/008/014/015/016, INV-001/002/003/005; `AGENTS.md`; security posture | `AuthorizationCodeExchange::{execute,complete,bound_connection,finish_id_token_exchange}`; login-state queries; active-connection owner; tenant-definer lookups; `same_issuer_two_tenant_isolation_keycloak`; provider-switch and UI journeys | PASS |
| BFF admission and transport | TASK-003 packet-local contract; approved REQ-005/009/010; prior `FIND-TASK-003-5` | `components/auth/bff.rs`; router mounting; config/boot; UI `upstream.ts` and tests; production journey admission checks | PASS |
| Browser session and tenancy | Approved REQ-009/015/016, INV-001/005; prior `FIND-TASK-003-3`/`-8` | `BrowserSessions`; `auth_browser_sessions` migration and queries; `WyrdPostgres` tenant resolvers; UI `server-sessions.ts`, hooks/layout/actions; chooser/context tests and journeys | PASS |
| API-key entry | Approved REQ-010; security posture fixed-cost refusal; prior `FIND-TASK-003-2` | `BrowserSessions::exchange_api_key`; `ExchangeApiKey::{execute,verify_api_key}`; shared `verify_presented`; browser/API-key fixed-cost tests; OIDC-off journey | PASS |
| Secret lifecycle | Approved REQ-005 and AC-007; prior `FIND-TASK-003-4`; task sealing contract | `SealedSecretRewrap`; `SealedSecretTable`; operator CAS queries; boot refusal; `browser_session_sealing_rotation_journey` | PASS |
| RBAC and replacement | Approved REQ-003/008/014/016/017, INV-002/003/005; prior `FIND-TASK-003-6` | settings server actions; normal Wyrd API calls with session authority; multi-provider and replacement UI journeys; `tenant_provider_switch_journey` | PASS |
| Secret exposure | Approved REQ-005/009 and task prohibitions | `skip_all` BFF handlers; secret wrappers; safe page metadata; fixed completion redirect; cookie flags; production `expectNoSecrets` assertions | PASS |

## Prior-finding closure

- `FIND-TASK-003-1` is closed under the explicit human replacement. `CallbackQuery.iss` is retained and forwarded. After the state is consumed and the exact active connection is rebound, fresh discovery supplies the support flag. A present `iss` is compared by exact string equality before token-endpoint IO; mismatch is refused, audited, consumes the state, makes zero token calls, and creates no completion. Missing `iss` is refused only when support is advertised. A non-advertising provider remains testable and activatable. The residual shared-callback exposure and per-connection callback upgrade path are documented.
- `FIND-TASK-003-2` is closed. Once CSRF shape and keyring availability are established, a known route passes the presented key unparsed into the existing tenant-scoped `ExchangeApiKey`; an unknown route pays exactly one dummy verification. Invalid keys converge on the same public refusal and only success writes a session.
- `FIND-TASK-003-3` is closed. Cookie suffixes remain lookup hints only. `ServerSessions.metadata` deduplicates them, resolves each through the server, clears invalid/mismatched cookies, and renders only server-returned tenant keys and names.
- `FIND-TASK-003-4` is closed. The canonical rewrap inventory now covers access, refresh, bootstrap API-key, and CSRF envelopes of every live browser session. Exact-byte CAS prevents overwriting concurrent renewal/logout; a lost race stays `remaining`, and a later pass converges. Keyless boot refuses while any live envelope remains.
- `FIND-TASK-003-5` is closed. The sole BFF upstream origin rejects non-loopback plaintext before invoking the fetcher; HTTPS and loopback HTTP remain supported.
- `FIND-TASK-003-6` is closed for this domain. The real two-BFF lane covers separate sessions at two active providers, switching, forged/cross-tenant cookies, wrong-provider and same-issuer cross-tenant callbacks, settings mutations, replacement recovery, old-session cutoff, and same-email non-inheritance.
- `FIND-TASK-003-8` is closed. The private read response carries `DataTenantId` into server-only `TenantContext`; page metadata still omits the UUID and no empty sentinel remains.

`FIND-TASK-003-7` (documentation completeness) and `FIND-TASK-003-9` (unused lifetime abstraction) are outside this domain except where their removal affects the security paths above; no security regression was found at those seams.

## Material findings

None.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. The documented residual mix-up exposure for providers that neither advertise nor send RFC 9207 `iss` is an explicit human-approved product decision with a named upgrade path, not a reviewer finding.

### Positive Controls

- Callback tenant and connection selection comes only from consumed server state; exact active connection id/revision, issuer, and client are rebound before provider IO.
- RFC 9207 `iss`, when present, is checked by exact string comparison before the authorization code or client authentication reaches a token endpoint. Advertising providers cannot omit it.
- PKCE, nonce, ID-token issuer/audience/algorithm/key/time validation, screened and pinned provider IO, and one-use state remain intact for every provider profile.
- Browser flow, session, and CSRF values are random 256-bit values; only hashes select durable flow/session rows, and session lookup re-enters the resolved tenant through RLS.
- The BFF service key is admitted before handler/store access, supports only a two-hash rotation overlap, and cannot itself select tenant authority.
- Browser credentials remain server-side and sealed. Cookies are host-only, Secure, HttpOnly, SameSite=Lax, path `/`, and bounded by absolute expiry.
- Mutations require POST, exact same origin, a live tenant-bound session, and constant-time CSRF comparison; the Wyrd API independently enforces the session principal's current permissions.
- Session renewal is serialized by the durable row lock, uses the original credential mode only, rechecks connection/key/principal/tenant authority, and revokes rather than falling back when renewal is refused.
- API-key refusal uses one real or dummy Argon verification across all credential classes and returns one indistinguishable public error.
- Tenant switching never rebinds a session. Each target session is independently resolved, and same issuer/client use across tenants still produces tenant-distinct principals and sessions.
- Provider replacement does not link by email or inherit roles; old-connection renewal stops while the separately authenticated recovery key remains available.
- Canonical rewrap includes every live session envelope, fences concurrent writers by exact-byte CAS, and prevents a keyless deployment from becoming ready while live ciphertext remains.

## Verification limits

- Per assignment, this reviewer did not run Cargo, mise, pnpm, Postgres, Keycloak, or two-BFF lanes. The candidate records all named focused and aggregate lanes green; this pass independently inspected the implementation and the assertions in those tests but did not reproduce their executions.
- The approved conditional RFC 9207 policy intentionally leaves a documented residual mix-up exposure for non-advertising providers. This is not converted into a finding because the explicit human decision replaces the prior strict requirement and names per-connection callback URLs as a future upgrade, not current scope.
- No dependency or lockfile change entered the remediation diff, so no new supply-chain boundary required review.
- CodeGraph was unavailable because the repository has no `.codegraph/` directory; source and callers were traced with repository-native search and direct inspection.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed security, identity, and tenancy obligations, and the remediation closes the prior domain findings without introducing a new material security defect.

# TASK-003 R3 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- R2 remediation: `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions: `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`
- Latest remediation range used to locate owners: `622a77028..8289fa298ed33d21f2568558bc0a02905fd0b218`

The candidate resolved to the stated commit before and after this static pass. The repository has no `.codegraph/` directory. Per the orchestration instruction, I ran no Cargo, mise, pnpm, provider, browser, or Postgres command.

## Review Findings

### Important

- **BEH-R3-001 — `REGRESSION` — early browser renewal rolls back refresh-replay containment.**
  - **Violated obligation:** `REQ-007` replay protection, `REQ-009` production browser-session safety, `REQ-017` canonical security audit, `AC-007`, and the existing `RefreshTokens` contract that a replayed/stale refresh token revokes its family and durably audits the containment. R2-AC-04 requires preservation of the already-issued access token until expiry; it does not authorize undoing an independently triggered refresh-replay containment action.
  - **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:441-449,468-508,556-572`; producer and required durable effect at `crates/wyrd/wyrd-auth/src/refresh.rs:100-113,121-173`; the ordinary HTTP owner demonstrates the required commit at `crates/wyrd/wyrd-server/src/components/auth/routes.rs:232-249`.
  - **Evidence:** `RefreshTokens::execute` treats a stored but no-longer-active refresh token as reuse, revokes the entire principal refresh family, appends `auth.refresh.family.revoke`, and returns `RefreshError::Reused` on the same transaction. `BrowserSessions::renew` maps `Reused` together with every other non-database outcome to `Renewal::Refused`. While the browser access token is unexpired but inside the renewal margin, `BrowserSessions::current` rolls that transaction back and retries only to serve the existing access token. The family revocation and canonical audit therefore both disappear. This is precisely why the normal refresh route commits `RefreshError::Reused` before returning its refusal. A realistic path is an attacker rotating a stolen browser refresh token through the public refresh route, followed by the legitimate browser presenting its now-stale stored token during proactive renewal: Wyrd detects replay, logs that it revoked the family, then rolls back containment and leaves the attacker's successor refresh usable.
  - **Observable consequence:** refresh-token theft can be detected through the browser-session path without revoking the attacker's successor or retaining the required audit evidence. The focused R2 proof cannot catch this because `proactive_renewal_refusal_preserves_authority_until_expiry` exercises only a revoked API-key session (`browser_sessions.rs:893-947`), not OIDC refresh reuse.
  - **Required testable correction:** preserve `RefreshError::Reused` as a distinct renewal outcome. Commit the existing `RefreshTokens` family revocation and audit exactly as the normal refresh route does, without revoking the browser row, then re-lock and serve only the already-issued browser access token until its stored expiry. Keep ordinary connection/key/policy refusals rollback-only before access expiry. Add one focused Postgres test with an OIDC browser session whose refresh token has already rotated: proactive renewal must leave the current access token usable until expiry while the complete refresh family and one canonical reuse audit are committed; first use after access expiry must still end the browser session. Reuse the existing refresh owner and transaction effects; add no second revocation or audit path.

### Critical

None.

### Suggestions

None. Optional improvements are outside this acceptance audit.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-003 outcome: production BFF replaces mock identity without becoming identity authority | Server-owned `BrowserSessions`, private BFF routes, and `ServerSessions`; production hooks resolve server sessions | `production SSO crosses replicas`, `OIDC-off credential UI`; claimed UI suite | PASS |
| REQ-003 / AC-006: an owner tests a candidate through one real provider sign-in and only that revision becomes tested | `HumanConnections::begin_test`, test-bound `LoginInitiation`, `AuthorizationCodeExchange`, and `stamp_test_sign_in` | `tenant_connection_test_sign_in_journey`, rotation journey, UI provider replacement | PASS |
| Human direction HD-TASK-003-R2-1: no `prompt=none` or fabricated-code probe; return an authorization URL | Old probe code is deleted; `ConnectionTestResponse.authorization_url` starts PKCE/state/nonce authorization-code login | Real Keycloak and Dex sign-ins in identity/UI journeys | PASS |
| HD-TASK-003-R2-1: test callback verifies as production, re-checks the initiating caller, issues no User/session/credential, and audits | Common callback performs discovery, issuer binding, code exchange, ID-token algorithm/signature/claims/nonce/`azp` validation; `tester_authorized` and `stamp_test_sign_in` gate the stamp | Success, unauthorized tester, failed audit, issued-state counts | PASS |
| HD-TASK-003-R2-1 negative flows: replay, expiry, cross-tenant state, wrong callback, wrong secret | Login state is tenant-resolved, one-use and time-bounded; candidate binding is re-read; provider failures do not stamp | `tenant_connection_test_sign_in_journey`; `tenant_connection_rotation_journey` | PASS |
| REQ-005: provider/session secrets remain sealed and canonical inventory covers stored ciphertext | Sealed connection/session fields; all six `SealedSecretTable` queries include every non-null value without expiry filters | Expired-envelope keyless boot test and sealing rotation journey | PASS |
| REQ-006 / INV-001: canonical tenant login and common callback derive authority only from server state | Tenant route begins state; callback resolves tenant and exact connection from hashed state; present/required `iss` is checked before token IO | Issuer-binding and mixed-callback journeys | PASS |
| Human issuer direction: RFC 9207 is conditional and unrelated response parameters are tolerated | Discovery flag defaults false; `verify_response_issuer` enforces exact present `iss` and advertised absence only; typed query tolerates extra fields | Discovery projection, callback query, issuer-binding journey | PASS |
| REQ-009: replica-safe opaque browser session, safe metadata, CSRF and private BFF channel | Postgres session owner, hashed session id, sealed tokens/CSRF, server-returned tenant, constant-time CSRF, service-key gate | Two-replica production journeys and unit tests | PASS |
| TASK packet TLS boundary / prior FIND-5 | Replica 1 uses native Node fetch through the trusted `https://localhost` TLS terminator; replica 0 retains loopback HTTP | `production SSO crosses replicas`; plaintext refusal unit coverage | PASS |
| REQ-010 / AC-001: OIDC-off UI uses an existing Wyrd credential, no password or mock authority | API-key exchange mode stores only a sealed bootstrap key and reissues through the shared server owner | `OIDC-off credential UI` | PASS |
| REQ-015 / AC-003: independent tenants/providers and safe switching | Distinct server sessions per tenant; target is resolved through its own cookie and server read; Keycloak and Dex are active concurrently | `production multi-provider tenant switch` | PASS |
| AC-003 mix-up refusal: different provider and same-issuer cross-tenant callback | Attack helper retains the genuine callback query and replaces only victim state; callback uses bound issuer/client/PKCE | Multi-provider UI journey | PASS |
| REQ-016 / R2-AC-04: ordinary connection/key/policy renewal refusal preserves issued access until expiry | `BrowserSessions::current` rolls back a refused early attempt, re-locks, and returns the existing access token; post-expiry refusal revokes browser row | API-key Postgres test and provider-replacement UI journey | PASS |
| REQ-007 / REQ-017 / AC-007: refresh replay containment remains durable on the browser renewal path | `RefreshTokens` creates the right revocation/audit, but `BrowserSessions` erases both by mapping `Reused` to generic refusal and rolling back | No OIDC browser replay test; existing API-key-only R2 proof does not cover it | **FAIL — BEH-R3-001** |
| R2-AC-06: chooser verification is bounded and uses only server-verified tenants | Distinct cookie hints are resolved sequentially through `read`; invalid hints are cleared | `production chooser bounds server verification` | PASS |
| R2-AC-07: explicit empty upstream is refused; absent uses local default | Nullish-only default followed by native URL and scheme validation | `empty upstream value is refused before fetch` plus existing URL cases | PASS |
| REQ-018 / AC-009: UI/docs/contracts agree on connection test and session behavior | Updated schema types, settings projection, security/self-hosting docs | Claimed `codegen:check`, docs check, UI typecheck | PASS |
| INV-003 / INV-005: UI does not map roles or create a second identity plane | Roles/permissions and tenant identity come from the server session; test authority re-checks stored server roles | Source and journey denied-action evidence | PASS |
| Non-goals: no browser token storage, local password store, UI role mapper, provider-specific branch, SAML/SCIM/signup work, or compatibility path | Cumulative diff retains existing server owners and adds no such surface | Static cumulative diff review | PASS |

## Prior-finding closure

| Prior finding | Source closure | Result |
|---|---|---|
| `FIND-TASK-003-1` | Replaced by the binding human direction; conditional RFC 9207 behavior lives in discovery and the common exchange owner with mixed-callback proof | CLOSED |
| `FIND-TASK-003-2` | Browser API-key sign-in reaches shared fixed-cost verification; refusal shapes are indistinguishable | CLOSED |
| `FIND-TASK-003-3` | Chooser renders only `ServerSessions.read` results, not raw cookie suffixes | CLOSED |
| `FIND-TASK-003-4` | Canonical rewrap inventories every stored non-null browser envelope, including expired rows | CLOSED |
| `FIND-TASK-003-5` | Non-loopback plaintext is refused and one production-built BFF replica crosses a trusted real TLS hop | CLOSED |
| `FIND-TASK-003-6` | Existing journey now uses Keycloak and Dex and retains complete provider callback parameters in mix-up cases | CLOSED |
| `FIND-TASK-003-7` | Required item-level rustdoc corrections are present | CLOSED |
| `FIND-TASK-003-8` | Session response and application projection carry the real typed tenant id | CLOSED |
| `FIND-TASK-003-9` | Dead `SessionLifetime::Until` capability is absent | CLOSED |
| `FIND-TASK-003-10` | Ordinary refused proactive renewal no longer cuts off issued access before expiry | CLOSED, but the correction introduced `BEH-R3-001` by treating refresh reuse as ordinary refusal |
| `FIND-TASK-003-11` | Raw RFC 9207 field and fallible parser have substantive rustdoc and `# Errors` | CLOSED |
| `FIND-TASK-003-12` | Cookie-hint reads are sequential, bounded at one | CLOSED |
| `FIND-TASK-003-13` | Explicit empty upstream reaches URL parsing and fails rather than selecting loopback fallback | CLOSED |

## Open Questions

None. The required correction boundary is determined by the existing `RefreshError::Reused` transaction contract and the normal refresh-route precedent.

## Verification Notes

- Static review covered the complete base-to-candidate diff and used the R2 remediation range only to locate the changed owners.
- The candidate records green UI, identity journey, Wyrd, SQL, codegen, tenant-isolation, docs, format, lint, and diff checks. Those results were not rerun in this reviewer context.
- The claimed focused proactive-renewal proof is credible for revoked API-key refusal, but it does not exercise the OIDC refresh-reuse branch that produces `BEH-R3-001`.

## Overall result

**FAIL**

The cumulative task is not acceptance-complete because one reachable replay path discards its existing durable security containment and audit. All previously validated findings are otherwise closed from source.

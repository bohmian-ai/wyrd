# TASK-003 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2 plus the explicitly authorized R3, R4,
  and R5 tasks
- Human directions: conditional RFC 9207 issuer response handling; real
  interactive connection testing; and the revision-7/RFC 7009 correction that
  browser logout retires only its own refresh chain
- Latest remediation locator:
  `989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`

The candidate remained at the stated commit throughout this review. I reviewed
the cumulative base-to-candidate source and used the latest range only to
locate the R5 chain-retirement and test-harness shutdown changes.

## State and invariant trace

The production browser flow has one durable authority path:

1. The callback validates server-owned login state and produces a sealed,
   single-use completion containing the ordinary Wyrd token pair.
2. `BrowserSessions::complete` redeems that completion in the tenant
   transaction, resolves the issued refresh row by token hash, and stores the
   initial refresh-row id as `refresh_chain_id` beside the encrypted browser
   credentials (`browser_sessions.rs:188-260`).
3. The migration makes that id mandatory exactly for `oidc_refresh` rows and
   tenant-binds it to the existing refresh-token table with a composite
   foreign key (`20261001000003_auth_browser_session_refresh_chain.sql:8-14`).
4. Renewal rotates the sealed current refresh token in place but deliberately
   retains the original chain root. Rotation descendants already name their
   predecessor through `rotated_from`; the browser row therefore continues to
   identify the whole login grant without retaining plaintext authority.
5. Logout locks the browser row, takes the existing User refresh-family
   advisory lock, recursively revokes the root and every descendant, then
   wipes the browser row and commits once (`browser_sessions.rs:419-447`,
   `refresh_tokens.rs:173-208`). A second login of the same User has another
   root and is not selected by that recursive chain.
6. API-key sessions carry no chain id. Their logout still wipes only the
   browser row, leaving the underlying operator key valid.

Renewal also preserves its three transaction meanings at their common owner:
ordinary lifecycle refusal rolls back and preserves the already-issued token
until exact expiry; refresh replay commits the existing family containment and
audit before preserving only that issued snapshot; internal failure rolls back
and serves no stale token (`browser_sessions.rs:450-624`). No sibling caller
adds a second classifier, revoker, session store, role mapper, or tenant
selector.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-006/007, INV-001: tenant login and the common callback derive tenant/connection from bounded server state; PKCE, nonce, state, exact redirect and provider validation remain fail closed | `wyrd-auth/src/login.rs`, `callback.rs`; server `auth/callback.rs`; typed `CallbackQuery` | `identity_e2e` login, issuer-binding, replay, expiry, wrong-provider and mixed-callback cases | PASS |
| Human direction FIND-1: RFC 9207 `iss` is exact when present, required only for an advertising provider, and unrelated callback parameters remain tolerated | `wyrd-auth-oidc/src/provider.rs`; `wyrd-auth/src/callback.rs`; `wyrd-spec/src/auth/oidc.rs` | issuer-binding journey plus `callback_query_ignores_provider_parameters` | PASS |
| REQ-003/AC-006 and connection-test direction: candidate testing is a real authorization-code sign-in bound to revision and caller; it issues no User, credential, or session | `HumanConnections::begin_test`; callback test-state branch; typed connection-test response | connection-test unit/integration cases and real Dex/Keycloak settings journeys | PASS |
| REQ-009/AC-002: callback completion creates a replica-safe Postgres session while the browser receives only an opaque secure cookie | `BrowserSessions::complete`; BFF `sessions/complete`; `ServerSessions.establish` | `production SSO crosses replicas`; session-cookie and secret-leak assertions | PASS |
| REQ-005/009: access, refresh, bootstrap API key and CSRF values stay encrypted at rest and are included in canonical key rotation even after session expiry | `browser_sessions` table; `sealing.rs`; boot sealing inventory | sealing rotation journey and `keyless_boot_refuses_while_an_expired_browser_session_envelope_remains` | PASS |
| Private BFF admission precedes store access and secret-bearing non-loopback traffic requires TLS | `components/auth/bff.rs::require_bff_key`; UI `serverUrl`/`ServerSessions.channel` | unauthorized-key checks, upstream unit cases, and trusted-TLS operation in `production SSO crosses replicas` | PASS |
| REQ-009/015, INV-005: cookie names are hints only; server-returned tenant identity controls session context and cross-tenant cookies cannot rebind a session | `ServerSessions.read/context/switch`; BFF read response includes authoritative tenant id/key | cross-tenant session and multi-provider tenant-switch journeys | PASS |
| REQ-009: mutations require POST, exact origin, current session and constant-time CSRF equality; Wyrd still enforces permission | `ServerSessions.checkAction`; settings and change actions call the normal Wyrd API with server-side session authority | `production session rejects cross-tenant and missing CSRF`; allowed/denied journey actions | PASS |
| REQ-010/AC-001: OIDC-off UI login uses the existing tenant API-key exchange, seals the key, and never adds a password authority | `BrowserSessions::exchange_api_key`; private `sessions/api-key`; tenant login action | `OIDC-off credential UI`; fixed-cost invalid-key cases | PASS |
| REQ-016: inactive/replaced connection or unusable credential cannot renew; current issued authority survives only to exact stored expiry | `BrowserSessions::current/renew`; `RefreshError`/`ExchangeError`/`IssuanceError` private classifiers | proactive-refusal, replay-containment, refresh-internal-failure and API-key-internal-failure tests; provider-replacement journey | PASS |
| R4: missing/unopenable renewal envelopes are internal retryable failures, and renewal policy is not widened into public Rust API | `open_credential`; three `pub(crate) is_refusal` methods | `missing_or_unopenable_renewal_credential_is_retryable_failure` and consequence tests | PASS |
| Human direction FIND-18 / R5: OIDC logout revokes exactly its login chain without decrypting it, serializes against rotation, and commits atomically with browser wipe | durable `refresh_chain_id`; `BrowserSessions::logout`; `revoke_refresh_chain` recursive CTE under `lock_refresh_family` | `oidc_logout_retires_only_its_refresh_chain_without_opening_it` | PASS |
| R5 preserved API-key behavior: logout does not revoke the underlying operator API key | OIDC-only chain id constraint; API-key rows use `None` and take only browser-row wipe | `browser_session_sealing_rotation_journey` re-exchanges the key after logout | PASS |
| AC-003: independent tenants/providers and replicas cannot share configuration, session authority, or role state | RLS `TenantConn`; per-tenant human connection and browser rows; server-derived tenant projection | Keycloak/Dex multi-provider and two-replica UI journeys | PASS |
| R2 chooser and upstream boundaries: request cookie hints have bounded verification work and explicit empty upstream configuration is refused | sequential `ServerSessions.metadata`; nullish-only upstream default plus URL validation | `production chooser bounds server verification`; `empty upstream value is refused before fetch` | PASS |
| R5 test-harness correction: in-process Bifrost roles stop before the fixture can force-drop Postgres | `WyrdTestServer::shutdown` cancels the shared token and drains Bifrost before dropping the fixture (`wyrd-testing/src/server.rs:748-787`) | recorded rerun of `test:wyrd` (2348 passed); focused R5 test also completed under repository-managed Postgres in this review | PASS |
| Non-goals and stop conditions: no browser-held Wyrd token, UI role mapper, local password store, new IdP callback, compatibility alias, or principal-wide logout revocation | cumulative diff and caller trace | static source inspection plus codegen/UI/journey evidence | PASS |

## Prior-finding closure

| Finding(s) | Current source closure | Result |
|---|---|---|
| FIND-TASK-003-1 | Conditional issuer-response binding remains at the authorization-code exchange boundary, matching the human direction | CLOSED |
| FIND-TASK-003-2 | Unknown/wrong/revoked/cross-tenant API keys still pay the shared verification path and produce one refusal | CLOSED |
| FIND-TASK-003-3, 8 | Tenant chooser entries and action context come only from server-resolved session tenant id/key | CLOSED |
| FIND-TASK-003-4 | Canonical sealing inventory includes expired but still stored browser ciphertext | CLOSED |
| FIND-TASK-003-5 | Shared upstream rejects non-loopback plaintext and the real journey exercises trusted TLS | CLOSED |
| FIND-TASK-003-6 | Keycloak/Dex browser journeys and callback mutation preserve genuine provider parameters and test intended state/PKCE boundaries | CLOSED |
| FIND-TASK-003-7, 11, 15 | The materially changed discovery, browser-session, and shared fixture items retain substantive rustdoc and error/panic contracts | CLOSED |
| FIND-TASK-003-9 | The unused lifetime branch remains deleted; the two approved fixed lifetimes are owned directly by `BrowserSessions` | CLOSED |
| FIND-TASK-003-10 | Ordinary refused proactive renewal preserves only the current token to exact expiry | CLOSED |
| FIND-TASK-003-12 | Cookie-hint verification is sequential and therefore application-bounded to one in-flight read | CLOSED |
| FIND-TASK-003-13 | Only absence selects the loopback default; an explicitly empty upstream reaches URL validation and is refused | CLOSED |
| FIND-TASK-003-14 | Containment, refusal, and internal failure retain distinct commit/rollback semantics | CLOSED |
| FIND-TASK-003-16, 17 | Renewal classifiers remain crate-private; missing/unopenable envelope behavior is documented and directly pinned | CLOSED |
| FIND-TASK-003-18 | The locked browser row's durable chain root drives chain-only retirement; no refresh-envelope open or principal-wide revocation remains in logout | CLOSED |

## Proposed findings

None. I found no reachable producer-to-sink invariant violation in the
cumulative candidate.

I specifically checked the new migration's nullable add followed by its mode
constraint. It is not a finding for this immutable cumulative delivery: the
browser-session table and the chain-id migration are both new after the stated
base and are applied together before this capability can create a row. An
intermediate remediation commit is not an approved deployment boundary.

## Verification notes

Independently run against the immutable candidate:

- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::tests::missing_or_unopenable_renewal_credential_is_retryable_failure)'` — PASS (1 test).
- Repository-managed Postgres migration setup plus exact
  `browser_sessions::pg_tests::oidc_logout_retires_only_its_refresh_chain_without_opening_it` — PASS (migration lanes passed; focused test passed).
- `git diff --check base..candidate` — PASS.

Reviewed but not independently rerun in this discovery pass: the recorded
`test:identity:journey`, UI test/check, `test:wyrd`, `test:sql`,
`codegen:check`, `check:tenant-isolation`, docs, format, and lint results. Their
named coverage aligns with the changed consumers and the focused source trace;
the final review should still require the complete report set and validate the
aggregate evidence before issuing its verdict.

## Overall result

**PASS**

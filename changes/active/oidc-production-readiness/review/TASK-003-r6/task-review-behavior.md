# TASK-003 R6 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: R2 plus the human-authorized R3, R4, and R5 tasks
- Human directions: R1 issuer binding, R2 real interactive connection test, and R5 logout scope. The R5 direction replaces the earlier principal-wide correction: logout retires only the current login's refresh chain.

I reviewed the complete base-to-candidate range and used
`989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
to locate the R5 correction and test-harness shutdown change. The candidate
remained at the stated commit during this review. This repository has no
`.codegraph/` directory, so navigation used `rg`, Git diffs, and direct source
inspection.

## Behavior paths reviewed

- Tenant login option, browser-flow cookie, server-bound login state, provider callback, one-use completion, Postgres session creation, and cross-replica read/authority paths.
- Conditional authorization-response `iss` validation before token-endpoint IO, including the non-advertising-provider exception approved by the R1 human direction.
- Interactive candidate connection testing through the ordinary code/PKCE/nonce/ID-token path, with no User, Wyrd credential, completion, or browser session creation.
- OIDC-off API-key entry, fixed-cost refusal, encrypted bootstrap-key storage, renewal by re-exchange, and API-key-preserving logout.
- Session-cookie tenant binding, server-verified chooser projection, CSRF/origin checks, tenant switching, Wyrd permission enforcement, and browser-secret exclusion.
- Proactive renewal across ordinary refusal, refresh replay containment, internal failure, exact access-token expiry, connection replacement, and concurrent replica use.
- OIDC logout from the BFF through `BrowserSessions::logout`, the User refresh-family advisory lock, chain-local recursive revocation, browser-row wipe, and transaction commit.
- R5 migration and all writers/readers of `refresh_chain_id`, including callback-created sessions, API-key sessions, test fixtures, lock projections, and the composite tenant FK.
- The R5 `WyrdTestServer::shutdown` change for in-process servers, including cancellation before Bifrost drain and fixture drop.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: authorized tenant connection settings and a real provider test | Settings actions use the session principal against the existing identity API. Candidate tests run through server-bound state and `AuthorizationCodeExchange`; `finish_id_token_exchange` delegates test completion to `HumanConnections::stamp_test_sign_in` before User/session issuance. | `production provider replacement settings` exercises stage, remove, real second-realm sign-in, tested revision, recovery credential, activation, replacement login, denial, and retired-connection removal. The cumulative record also names the identity connection-test cases. | PASS |
| REQ-005: browser and provider credentials stay sealed and survive canonical key rotation | `BrowserSessions` seals access, refresh, API-key, and CSRF values; browser rows participate in the canonical rewrap inventory, including expired non-revoked envelopes. BFF responses project only safe metadata, while authority stays server-side. | `browser_session_sealing_rotation_journey`, keyless-boot coverage, K2-only recovery, `expectNoSecrets`, codegen, and identity journeys are recorded green. | PASS |
| REQ-006 / INV-001: canonical route and server-bound tenant/connection selection | The route key begins login only; callback consumes server-owned state, rebinds the exact connection, and completion resolves tenant from the flow hash. Session cookie suffixes are hints resolved by server read; a mismatched returned tenant clears the cookie. | Real browser journeys cover missing, forged, mismatched, replayed, wrong-provider, same-issuer cross-tenant, and cross-tenant session cases. | PASS |
| REQ-007 and R1 human direction: provider-agnostic OIDC with conditional RFC 9207 binding | `CallbackQuery` retains typed optional `iss` and tolerates unrelated response fields. `AuthorizationCodeExchange` compares a present issuer exactly before token exchange; absence is refused only when discovery advertises support. PKCE, nonce, algorithm, issuer, audience, authorized party, state, and exact connection binding remain in their existing owners. | Unit and real-provider evidence covers matching/mismatching issuer, advertising/non-advertising providers, genuine mixed callbacks, and ordinary provider parameters. | PASS |
| REQ-009 / AC-002 / AC-007: production BFF session across replicas without browser-visible Wyrd credentials | The server owns Postgres session state and short-lived authority; the BFF owns only opaque Secure/HttpOnly/SameSite cookies, CSRF/origin checks, and private-channel calls. Session renewal is serialized under the row lock. The internal route is key-gated and the configured upstream rejects unsafe plaintext. | `production SSO crosses replicas` uses two production-built BFF processes, including one trusted TLS connection, and covers caller admission, cookies, one-use completion, concurrent use, forged state/session, CSRF, permission denial, logout, and deactivation cutoff. | PASS |
| REQ-010 / AC-001: OIDC-off production UI reuses existing credential authority | `BrowserSessions::exchange_api_key` delegates known tenants to `ExchangeApiKey` and unknown route tenants to exactly one dummy verification; successful keys are sealed and re-exchanged under the session lock. API-key logout wipes only browser state. | `OIDC-off credential UI`, the fixed-cost verification test, API-key renewal tests, and API-key logout/sealing journey are recorded green. | PASS |
| REQ-015 / AC-003: independent multi-tenant/provider sessions and safe switching | Per-tenant session cookies are independently verified against server-returned tenant identity. Switching reads the target tenant's own session and never rebinds the source session. | `production multi-provider tenant switch` establishes Keycloak and Dex sessions in one browser, exercises both replicas, genuine wrong-provider and same-issuer cross-tenant callbacks, forged chooser hints, and independent principal/session identities. | PASS |
| REQ-016: retired connections stop renewal at the issued token's exact expiry | `BrowserSessions::current` rolls back ordinary proactive refusal and serves only the already-issued token until its stored expiry; at/after expiry it revokes the browser row. It never falls back to another provider. | `proactive_renewal_refusal_preserves_authority_until_expiry` and the provider-replacement browser journey directly cover this boundary. | PASS |
| R3 remediation: replay containment and internal renewal failure retain distinct transaction outcomes | `Renewal::Contained` commits the existing family-revocation/audit work before serving an unexpired issued token; `Renewal::Failed` returns without commit; only `is_refusal` outcomes use the ordinary cutoff path. | Replay-containment plus OIDC/API-key internal-failure Postgres selectors are recorded green, with repair-and-retry assertions. | PASS |
| R4 remediation: classifier scope, renewal-envelope behavior, and documentation | The three error classifiers are crate-private. Missing or unopenable renewal credentials map to `Renewal::Failed(Internal)`; `open_text` and `open_credential` describe their actual boundaries. | `missing_or_unopenable_renewal_credential_is_retryable_failure` and `only_lifecycle_refusals_end_a_renewing_session` are recorded green. | PASS |
| R5 human direction / FIND-TASK-003-18: logout retires only this login's refresh chain | Completion stores the non-secret first refresh-row id as `refresh_chain_id`; the migration requires it exactly for OIDC mode and tenant-binds it by FK. Logout locks the browser row, then the existing User family lock, recursively revokes that root and all `rotated_from` descendants, wipes the browser row, and commits once. It never opens the refresh envelope and never revokes sibling login chains. | `oidc_logout_retires_only_its_refresh_chain_without_opening_it` rotates a copied token, makes the browser envelope unreadable, proves the committed successor is revoked and unusable, proves a separate login remains active, and proves the browser row is wiped. Existing API-key logout proof remains. | PASS |
| Logout idempotence and atomicity | Unknown, expired, or already-ended rows remain no-ops. Lock, recursive update, browser wipe, and commit use one caller-owned `TenantConn`; any error before commit drops/rolls back the transaction. | Source trace plus the focused R5 Postgres selector and cumulative identity journey. | PASS |
| R5 test-harness shutdown fix does not change product behavior and closes the observed teardown race | In-process `WyrdTestServer::shutdown` now cancels `AppState::shutdown_token` and drains the existing composed Bifrost owner before `PgFixture` is dropped; bound-mode ownership is unchanged. | The R5 record diagnoses the prior forced database drop followed by Oracle epoch abort and reports `test:wyrd` green at 2348 tests after the owner-level fix, plus rerun format, lints, identity journey, and diff check. | PASS |
| REQ-018 / AC-009: public behavior, docs, schema, and UI agree | Authentication/SSO docs describe optional OIDC, callback and setup inputs, secret rotation, and production BFF behavior. Callback schemas match the tolerant typed query contract. | `mise run codegen:check` and docs/UI checks are recorded green. | PASS |
| Explicit non-goals and prohibited changes | No browser-held bearer/refresh token, UI role mapper, local password authority, production mock dependency, commercial stub, compatibility route, provider-specific login branch, or principal-wide logout revocation entered the implementation. | Cumulative diff and caller inspection. | PASS |

## Prior-finding closure

| Finding | Current-source result |
|---|---|
| `FIND-TASK-003-1` | CLOSED under the R1 human direction: optional typed callback `iss`, discovery support, exact conditional comparison, and pre-token refusal remain in the common exchange owner. |
| `FIND-TASK-003-2` | CLOSED: browser API-key attempts use the shared verifier or exactly one dummy verification with one public refusal. |
| `FIND-TASK-003-3` | CLOSED: cookie suffixes remain hints; every rendered tenant is obtained from a server session read. |
| `FIND-TASK-003-4` | CLOSED: the canonical inventory includes all stored browser-session envelopes regardless of session expiry, with exact-byte CAS and keyless/K2-only proof. |
| `FIND-TASK-003-5` | CLOSED: the production-built BFF journey includes an authenticated browser-session operation over trusted TLS; unsafe non-loopback plaintext remains refused before fetch. |
| `FIND-TASK-003-6` | CLOSED: Keycloak/Dex switching, genuine mixed callbacks, and the provider-replacement settings journey remain present. |
| `FIND-TASK-003-7` | CLOSED: the cited Rust owners and items retain substantive rustdoc. |
| `FIND-TASK-003-8` | CLOSED: authoritative tenant UUID remains server-returned and server-only; browser metadata does not fabricate or expose it. |
| `FIND-TASK-003-9` | CLOSED: the unused lifetime abstraction and dual-lifetime SQL branch remain absent. |
| `FIND-TASK-003-10` | CLOSED: ordinary renewal refusal preserves current authority only to exact stored expiry and commits no tentative renewal work. |
| `FIND-TASK-003-11` | CLOSED: discovery support parsing and its fallible behavior remain documented and tested. |
| `FIND-TASK-003-12` | CLOSED: chooser hints are deduplicated and verified sequentially, bounding private verification to one in flight. |
| `FIND-TASK-003-13` | CLOSED: absent upstream configuration alone receives the loopback default; explicit empty or unsafe values are refused. |
| `FIND-TASK-003-14` | CLOSED: replay containment, ordinary refusal, and internal failure retain their separate commit/rollback outcomes. |
| `FIND-TASK-003-15` | CLOSED: shared API-key test support retains accurate module/helper rustdoc. |
| `FIND-TASK-003-16` | CLOSED: the three renewal classifiers remain `pub(crate)`. |
| `FIND-TASK-003-17` | CLOSED: missing/unopenable renewal envelopes are retryable internal failures with direct unit proof and aligned documentation. |
| `FIND-TASK-003-18` | CLOSED under the R5 human replacement: the durable chain id and recursive chain-local revocation close unreadable-envelope, committed-successor, and concurrent-rotation paths without ending the User's other logins. |

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. No optional refactor or additional abstraction is required for task acceptance.

## Open questions

None.

## Verification notes

- I inspected the candidate's implementation and test sources and the recorded cumulative verification evidence. I did not rerun the broad Cargo, Postgres, provider, browser, or pnpm lanes in this independent static review.
- The implementation records green focused UI selectors, browser-session Postgres selectors, callback/discovery tests, the R5 chain-local logout selector, `test:sql`, `test:wyrd`, the filtered and unfiltered identity journeys, UI tests/typecheck, codegen, docs, tenant-isolation, format, lints, and `git diff --check`.
- The R5 record includes a trace-based diagnosis for the harness change and a green 2348-test `test:wyrd` run after the fix; this is credible evidence for the changed teardown path rather than an unexplained timeout or weakened test.
- The required real-provider and browser journeys are gated integration evidence; unit tests are supporting proof, not substitutes.

## Overall result

**PASS**

The cumulative candidate satisfies TASK-003's behavior and the authorized remediation outcomes. No behavior finding is proposed.

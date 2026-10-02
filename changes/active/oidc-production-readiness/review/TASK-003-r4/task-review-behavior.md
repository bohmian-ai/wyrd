# TASK-003 round-4 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: `TASK-003-R2-production-ui-remediation.md` and `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

The complete base-to-candidate range was reviewed. The latest
`8289fa298ed33d21f2568558bc0a02905fd0b218..6aedcda5166509001db0cc851a5bc74502b4b043`
range was used to locate the R3 correction, not as a substitute for the
cumulative acceptance audit. `HEAD` resolved to the candidate before source
inspection and focused verification. This repository has no `.codegraph/`
directory, so source navigation used `rg`, Git diffs, and direct caller
inspection.

## Caller-to-result analysis

The production browser path remains one server-owned flow:

1. SvelteKit reads only the opaque per-tenant cookie and calls the authenticated
   private BFF routes in `server-sessions.ts`.
2. `wyrd-server` delegates `sessions/read` and `sessions/authority` to
   `BrowserSessions` without deriving tenant authority from the request.
3. `BrowserSessions::current` resolves the tenant from the session hash, opens a
   tenant transaction, locks the browser row, and either returns the current
   token or renews through the existing `RefreshTokens`/`ExchangeApiKey`
   owners.
4. Successful renewal rotates the stored browser credential in the same
   transaction. Ordinary credential/lifecycle refusal rolls back before access
   expiry and preserves only the issued snapshot; at or after expiry it revokes
   the browser row and commits. Refresh replay commits the existing family
   containment and audit, relocks, and preserves the issued snapshot only until
   expiry. Internal failures return without commit, so transaction drop rolls
   back tentative refresh consumption, API-key use, audit, and browser-row
   changes.
5. SvelteKit deletes the cookie only on `401` or a tenant mismatch. Internal
   renewal failures map to a non-`401` problem and therefore remain retryable
   with the same opaque cookie.

The implementer-highlighted envelope behavior is correct. In
`browser_sessions.rs:765-778`, a missing or unopenable mode credential becomes
`Renewal::Failed(WyrdError::Internal)`. `current` returns that failure at
`browser_sessions.rs:528` without committing or revoking. The BFF's `read` and
`api` paths clear cookies only for `401` at
`server-sessions.ts:192-216,317-346`; they propagate this internal response and
retain the cookie. Reintroducing the required key material or repairing the
stored envelope therefore permits a later retry. This matches the explicitly
authorized R3 decision and does not weaken the separate behavior for an
unopenable access token or CSRF value outside renewal.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006 / human direction `HD-TASK-003-R2-1`: candidate testing is one real, caller-bound interactive sign-in and issues no session, credential, or User | Candidate-bound login state and callback handling remain in `connections.rs`, `login.rs`, `callback.rs`, and the server identity handlers; the UI settings actions project those server contracts | Cumulative Keycloak/Dex provider-replacement journey and negative state coverage recorded in the task and R2 evidence | PASS |
| Human issuer direction / REQ-007: conditional RFC 9207 handling | `CallbackQuery` preserves typed optional `iss`; `AuthorizationCodeExchange` consumes state and exact-compares a present issuer, requires absence only for advertising providers, and performs no alternate-provider fallback | Advertising/non-advertising and mixed-callback coverage recorded in the cumulative identity tests; generated schema evidence remains present | PASS |
| REQ-005 / AC-007: recoverable browser credentials stay sealed and participate in canonical inventory/rewrap | Browser session access, refresh/API-key, and CSRF envelopes use the deployment keyring; the canonical inventory includes stored expired envelopes and retains exact-byte CAS | Expired-envelope keyless-boot and browser-session rotation evidence recorded in R2; no R3 change touches this owner | PASS |
| REQ-009: replica-safe session, safe browser projection, CSRF, tenant binding, one-use completion, and server-only authority | `BrowserSessions`, the private BFF handlers, `ServerSessions`, route guards, and Postgres session queries retain the single server authority; page projection excludes Wyrd bearer/refresh/API-key material | Two-BFF production UI journeys, unit tests, and private-channel TLS evidence recorded in the cumulative task | PASS |
| REQ-010 / AC-001: OIDC-off UI reuses existing API-key authority with indistinguishable fixed-cost refusal | `BrowserSessions::exchange_api_key` uses the shared verifier/exchange owner and stores only a sealed bootstrap key | OIDC-off production BFF journey and fixed-cost refusal coverage recorded in cumulative evidence | PASS |
| REQ-015 / AC-003: tenant sessions and provider selection remain independent | Session tenant comes from the stored row; chooser hints are server-read sequentially; switching uses the target tenant's independent session or login | Keycloak/Dex multi-provider switching and cross-tenant negative journey evidence recorded in R2 | PASS |
| REQ-016: ordinary old-connection/key refusal preserves issued authority until exact expiry, then ends the session | `current` rolls back `Renewal::Refused` before expiry, relocks once, and revokes only at/after expiry (`browser_sessions.rs:482-527`) | `proactive_renewal_refusal_preserves_authority_until_expiry` passed in this review | PASS |
| R3-AC-01 / REQ-017: refresh replay containment and canonical audit commit without prematurely ending valid access | `RefreshError::Reused` maps only to `Renewal::Contained`; the pre-expiry branch commits the refresh owner's transaction, relocks, and serves the unchanged token; post-expiry use revokes the browser row in the containment transaction | `proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry` passed in this review | PASS |
| R3-AC-02: internal refresh/API-key renewal failures fail closed, roll back, and remain retryable | `IssuanceError::is_refusal`, `RefreshError::is_refusal`, and `ExchangeError::is_refusal` restrict ordinary refusal to lifecycle/credential outcomes. Every other producer maps to `Renewal::Failed`, which `current` returns without commit | `refresh_renewal_internal_failure_rolls_back_and_remains_retryable` and `api_key_renewal_internal_failure_rolls_back_and_remains_retryable` passed in this review | PASS |
| Explicitly flagged behavior: a stored renewal credential that cannot be opened is retryable rather than session-ending | `open_credential` maps missing/open failure to `Renewal::Failed(Internal)`; `current` neither commits nor revokes; BFF cookie deletion is limited to `401`/tenant mismatch | Direct producer-to-BFF source trace; the focused internal-failure tests prove the shared rollback/retry branch | PASS |
| R3-AC-03: inactive tenant/principal/connection and unusable API-key outcomes retain ordinary-refusal semantics | The three error owners' closed classification methods preserve those variants as refusal; no internal variant joins that set | Unit classification coverage plus the focused ordinary-refusal control and provider-replacement journey | PASS |
| R3-AC-04: shared fixture rustdoc correction | `exchange_api_key::pg_tests` documents its shared role; `insert_live_api_key` documents its actual hash/insert seed contract and panic boundary | Direct source inspection; recorded format/lint evidence | PASS |
| R2 transport/config/concurrency closures | Real trusted TLS journey remains in the existing identity harness; cookie hints resolve sequentially; `serverUrl` uses nullish-only fallback and rejects explicit empty/non-loopback plaintext values | Cumulative focused Vitest and identity-lane evidence | PASS |
| Original task prohibited outcomes and explicit non-goals | No browser-held Wyrd token, local password authority, UI role mapper, provider-specific bypass, new callback route, compatibility path, dependency, migration, or persistent replay marker entered the cumulative implementation | Complete cumulative diff and current callers inspected | PASS |

## Prior-finding closure

| Prior finding | Current result |
|---|---|
| `FIND-TASK-003-1` | CLOSED under the explicit human issuer direction: conditional issuer handling remains in the exchange owner. |
| `FIND-TASK-003-2` | CLOSED: browser API-key refusal still uses exactly one fixed-cost verification path. |
| `FIND-TASK-003-3` | CLOSED: browser cookie suffixes remain hints only and are server-resolved before rendering. |
| `FIND-TASK-003-4` | CLOSED: canonical inventory includes stored browser envelopes regardless of session expiry. |
| `FIND-TASK-003-5` | CLOSED: the production-built BFF trusted-TLS exercise remains in the identity journey. |
| `FIND-TASK-003-6` | CLOSED: the cumulative journey uses Keycloak and Dex, preserves genuine callback parameters, and covers provider replacement/switching. |
| `FIND-TASK-003-7` | CLOSED: the cited production Rust documentation remains corrected. |
| `FIND-TASK-003-8` | CLOSED: the authoritative tenant id remains server-only in the session projection. |
| `FIND-TASK-003-9` | CLOSED: the unused `SessionLifetime::Until` branch remains absent. |
| `FIND-TASK-003-10` | CLOSED for ordinary refusal: the exact-expiry rollback/relock behavior remains intact. |
| `FIND-TASK-003-11` | CLOSED: discovery field/parser rustdoc remains present. |
| `FIND-TASK-003-12` | CLOSED: cookie-hint verification remains sequential. |
| `FIND-TASK-003-13` | CLOSED: explicit empty upstream configuration still reaches URL validation and is refused. |
| `FIND-TASK-003-14` | CLOSED: replay containment, ordinary refusal, and internal failure now preserve distinct transaction semantics. |
| `FIND-TASK-003-15` | CLOSED: the shared test module and API-key fixture documentation are accurate. |

Both human directions remain satisfied. No earlier defect was rediscovered at a
sibling caller, and the R3 correction did not reopen a prior finding.

## Proposed findings

None.

## Verification notes

The following focused Postgres-backed checks were rerun together through the
repository-managed database lifecycle and passed (4/4):

- `browser_sessions::pg_tests::proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry`
- `browser_sessions::pg_tests::refresh_renewal_internal_failure_rolls_back_and_remains_retryable`
- `browser_sessions::pg_tests::api_key_renewal_internal_failure_rolls_back_and_remains_retryable`
- `browser_sessions::pg_tests::proactive_renewal_refusal_preserves_authority_until_expiry`

The cumulative candidate also records green `test:wyrd`,
`test:identity:journey`, tenant-isolation, format, lint, codegen, UI test and
typecheck, documentation, and diff checks in the task/remediation evidence.
This behavior review did not rerun every broad lane. The unopenable-envelope
classification has no dedicated corruption-injection test, but its producer,
transaction branch, HTTP mapping, and cookie consumer are direct and were all
traced; the same `Renewal::Failed` rollback/retry branch is exercised by both
focused internal-failure tests above.

## Overall result

**PASS**

The complete candidate satisfies the original TASK-003 behavior, both human
directions, and the R2/R3 remediations. The proposed finding ledger is empty.

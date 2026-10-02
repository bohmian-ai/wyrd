# TASK-003 round-5 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: `TASK-003-R2-production-ui-remediation.md`,
  `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and
  `TASK-003-R4-renewal-contract-boundaries.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

The complete base-to-candidate range was reviewed. The locator range
`6aedcda5166509001db0cc851a5bc74502b4b043..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
was used only to locate the R4 closure. It changes the three renewal classifier
visibilities, two local documentation contracts, and one focused unit test;
it does not alter runtime behavior. This repository has no `.codegraph/`
directory, so navigation used Git, `rg`, and direct source inspection.

## Caller-to-result analysis

The browser workflow remains one server-owned path:

1. The tenant login and settings routes call `ServerSessions`; the browser
   receives only flow/session cookies, safe session metadata, and its CSRF
   value.
2. The private BFF handlers authenticate the deployment BFF key before
   delegating completion, API-key exchange, read, authority, and logout to
   `BrowserSessions`.
3. `BrowserSessions::current` resolves tenant identity from the stored session
   hash, opens a `TenantConn`, locks the session row, and renews only through
   `RefreshTokens` or `ExchangeApiKey`.
4. Successful renewal rotates the stored credential atomically. Ordinary
   credential or lifecycle refusal preserves the already-issued access token
   only until exact expiry. Refresh replay commits the existing family
   containment and audit while preserving that same bounded access snapshot.
   Internal failures return without commit or revocation.
5. SvelteKit clears the opaque cookie only for `401` or a server-returned
   tenant mismatch. Internal renewal failures therefore remain retryable but
   return no authority.

R4 closes the two prior boundary findings at their existing owners.
`IssuanceError::is_refusal`, `RefreshError::is_refusal`, and
`ExchangeError::is_refusal` are `pub(crate)`, which retains every in-crate
caller without publishing browser-renewal policy. `open_credential` maps both
a missing envelope and an envelope that the held keyring cannot open to
`Renewal::Failed(WyrdError::Internal)`. `BrowserSessions::current` returns that
failure before any commit or revocation, and the BFF retains the cookie for
the non-`401` response. Restoring the key or envelope can therefore make a
later renewal succeed, without serving stale authority during the failure.

The provider-test path also remains bound to its approved behavior: candidate
testing performs a real authorization-code sign-in against the exact revision,
re-checks the tester, marks only that revision tested, and creates no User,
credential, or browser session. Callback `iss` remains conditionally required
according to advertised RFC 9207 support, per the human direction.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006 / `HD-TASK-003-R2-1`: tenant settings project headless connection administration; candidate testing is a real caller- and revision-bound sign-in that issues no authority | `wyrd-auth/src/connections.rs`, `callback.rs`; server identity handlers; UI settings actions | `tenant_connection_test_sign_in_journey`, connection rotation journey, and Keycloak/Dex settings journeys recorded green in R2/task evidence | PASS |
| Human issuer direction / REQ-007: response issuer handling is conditional and exact, with no provider fallback | Typed optional `CallbackQuery.iss`; `AuthorizationCodeExchange` compares a present issuer and requires it only for advertising providers | `tenant_callback_issuer_binding_journey`; mixed-provider callback cases in the UI journey; codegen evidence | PASS |
| REQ-005 / AC-007: browser access, refresh/API-key, completion, and CSRF material is sealed and included in canonical inventory and rotation | `browser_sessions.rs`, `sealing.rs`, canonical server sealing inventory | Expired-envelope keyless-boot selector and `browser_session_sealing_rotation_journey` recorded green | PASS |
| REQ-006 / REQ-009: canonical login, one-use completion, opaque secure cookies, replica-safe session, safe page projection, CSRF, tenant binding, and server-only authority | Login/completion routes, `server-sessions.ts`, private BFF handlers, `BrowserSessions`, Postgres session row and lock | `production SSO crosses replicas`; session/CSRF unit selectors; trusted-TLS exercise | PASS |
| REQ-010 / AC-001: OIDC-off UI reuses existing API-key exchange, with the bootstrap key sealed server-side and no mock/password path | Login `apiKey` action and `BrowserSessions::exchange_api_key` use `ExchangeApiKey` | `OIDC-off credential UI` plus fixed-cost/indistinguishable API-key refusal evidence | PASS |
| REQ-015 / AC-003: tenant switching resolves an independent server-owned target session; cookie/path hints cannot rebind authority | `ServerSessions::switch`, `read`, and sequential `metadata`; stored session tenant is authoritative | Keycloak/Dex multi-provider switch and cross-tenant refusal cases | PASS |
| REQ-016: deactivated/replaced connection or revoked credential cannot renew, while already-issued access remains usable only to exact stored expiry | `BrowserSessions::current` rollback/relock branch for `Renewal::Refused` | `proactive_renewal_refusal_preserves_authority_until_expiry`; provider replacement UI journey | PASS |
| R3-AC-01 / REQ-017: refresh replay containment and its canonical audit commit without extending or prematurely deleting issued authority | `RefreshError::Reused` maps only to `Renewal::Contained`; `current` commits containment and ends at expiry | `proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry` recorded green | PASS |
| R3-AC-02/03: internal refresh/API-key failures roll back and remain retryable; true credential/lifecycle refusals retain terminal-at-expiry behavior | Closed classifiers in `issuance.rs`, `refresh.rs`, and `exchange_api_key.rs`; `Renewal::Failed` propagation | Refresh/API-key internal-failure Postgres selectors and lifecycle-classification unit selector | PASS |
| R4-AC-01 / FIND-TASK-003-16: renewal classifiers are not public Rust API | All three `is_refusal` methods are `pub(crate)`; repository callers remain inside `wyrd-auth` | Crate compilation recorded in R4; focused classifier selector passed in this review | PASS |
| R4-AC-02/03 / FIND-TASK-003-17: envelope-open documentation matches caller-owned lifecycle and direct proof pins missing/unheld-key mapping | `open_text` documents raw failure and caller policy; `open_credential` maps both cases to retryable internal failure | `missing_or_unopenable_renewal_credential_is_retryable_failure` passed in this review | PASS |
| R2 transport/config/concurrency closures | Trusted HTTPS BFF path remains in the identity harness; chooser hints are sequential; explicit empty/non-loopback plaintext upstreams are refused | R2 identity and Vitest evidence; no later change touches these owners | PASS |
| Original task constraints and non-goals | No browser-held Wyrd/provider credential, UI role mapper, local password store, process-local production session, provider-specific bypass, alternate callback, compatibility route, new dependency, or persistent replay marker appears in the cumulative change | Complete cumulative diff inventory and caller inspection | PASS |

## Prior-finding closure

| Finding | Source-backed result |
|---|---|
| `FIND-TASK-003-1` | CLOSED under the explicit human issuer direction; conditional RFC 9207 handling remains in the exchange owner. |
| `FIND-TASK-003-2` | CLOSED; browser API-key refusal still uses one fixed-cost verification path. |
| `FIND-TASK-003-3` | CLOSED; cookie suffixes are only hints and every rendered tenant is server-resolved. |
| `FIND-TASK-003-4` | CLOSED; canonical inventory includes stored browser envelopes regardless of expiry. |
| `FIND-TASK-003-5` | CLOSED; a production-built BFF performs an authenticated operation over trusted TLS. |
| `FIND-TASK-003-6` | CLOSED; the journey uses Keycloak and Dex and preserves genuine callback parameters during mixed-callback attacks. |
| `FIND-TASK-003-7` | CLOSED; cited production Rust documentation remains present. |
| `FIND-TASK-003-8` | CLOSED; authoritative tenant id remains server-returned and server-only. |
| `FIND-TASK-003-9` | CLOSED; the unused lifetime branch remains absent. |
| `FIND-TASK-003-10` | CLOSED; ordinary refused renewal preserves issued authority only until exact expiry. |
| `FIND-TASK-003-11` | CLOSED; discovery field/parser rustdoc remains accurate. |
| `FIND-TASK-003-12` | CLOSED; request-derived chooser verification remains sequential. |
| `FIND-TASK-003-13` | CLOSED; explicit empty upstream configuration is refused before fetch. |
| `FIND-TASK-003-14` | CLOSED; replay containment, ordinary refusal, and internal failure retain distinct transactional outcomes. |
| `FIND-TASK-003-15` | CLOSED; shared API-key test support documents its real fixture contract. |
| `FIND-TASK-003-16` | CLOSED; all three classifiers are crate-private. |
| `FIND-TASK-003-17` | CLOSED; local rustdoc is consistent and the missing/unopenable producer mapping has direct regression proof. |

Both human directions remain satisfied. R4 introduces no sibling public caller,
runtime branch, persistence decision, or alternate lifecycle owner that reopens
an earlier finding.

## Test Coverage Analysis

### Current Coverage

- Unit coverage directly distinguishes missing and unopenable renewal
  envelopes from credential/lifecycle refusal.
- Postgres coverage exercises ordinary refusal, refresh replay containment,
  and retryable internal failure for both refresh and API-key renewal.
- Real HTTP/browser journeys cover two BFF replicas, trusted TLS, OIDC-off
  login, Keycloak/Dex switching, provider replacement, CSRF, tenant mismatch,
  logout, and settings permission boundaries.
- Server identity journeys cover real candidate-test sign-in, replay/expiry/
  cross-tenant refusal, issuer binding, and sealing rotation.

### Gaps

None material to the approved task. The R4 change is a visibility,
documentation, and pure producer-mapping correction; the focused unit check is
the lowest useful level, while the retained Postgres tests prove its shared
transaction consequence.

### Recommended Verification

- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::tests::missing_or_unopenable_renewal_credential_is_retryable_failure)'` — direct R4 producer proof.
- `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::tests::only_lifecycle_refusals_end_a_renewing_session)'` — classifier boundary proof.
- The four repository-managed Postgres renewal selectors from R4 — transaction and retry consequences.
- `mise run test:wyrd`, `mise run test:identity:journey`, `mise run check:tenant-isolation`, `mise run fmt`, and `mise run lints` — affected-surface regression gates.

### Residual Risk

The broad lanes and four Postgres selectors were not rerun by this discovery
reviewer; their green results are recorded in immutable task/R4 evidence. The
two R4 unit selectors were rerun together through `mise exec -- cargo nextest`
and passed (2/2), and the cumulative Git range passes `git diff --check`.

## Proposed findings

None.

## Overall result

**PASS**

The cumulative candidate satisfies TASK-003, the R2-R4 remediations, and both
human directions. The proposed finding ledger is empty.

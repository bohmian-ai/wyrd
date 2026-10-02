# TASK-003 R5 invariant review

## Review Findings

### Critical

None.

### Important

- **INV-R5-001 — INCORRECT** — `crates/wyrd/wyrd-auth/src/browser_sessions.rs:395-434`: OIDC logout does not preserve the task's coupled browser-session/refresh-family revocation invariant when the stored refresh envelope cannot be opened. `logout` conditionally attempts refresh revocation only when a keyring exists and `open_text` succeeds, then unconditionally revokes the browser row and wipes every sealed value. The R3/R4 contract explicitly treats a missing or unopenable renewal credential as repairable internal state, but this sibling consumer silently discards that credential and returns success, leaving the refresh family live with no remaining browser-row material from which a later retry can revoke it. This violates TASK-003's explicit requirement that logout revoke both the browser session and its refresh family, and it makes temporary keyring/envelope failure irreversible. For an `OidcRefresh` row, require the stored refresh credential to be opened and its revocation decision completed before wiping the browser row; an absent/unopenable envelope must return an internal failure without committing either mutation so a corrected keyring/envelope can retry. Preserve the current API-key-mode behavior, because its underlying operator key must not be revoked. Add one focused Postgres check that makes only an OIDC row's refresh envelope unopenable while its access/CSRF envelopes remain readable, proves logout fails without revoking or wiping the row or family, repairs the envelope/keyring, retries logout, and proves the family and browser row are then revoked atomically.

### Suggestions

None.

## Open Questions

None.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authorities: `TASK-003-R2-production-ui-remediation.md`, `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and `TASK-003-R4-renewal-contract-boundaries.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

The human owner explicitly authorized R3 and R4. The candidate resolved to the stated object before source inspection and after the focused check. The repository has no `.codegraph/` directory, so navigation used the immutable Git range, repository search, and direct source inspection.

## Producer-to-sink invariant trace

| Authority or value | Producer and durable owner | Consumers and transition | Invariant result |
|---|---|---|---|
| Browser tenant identity | Login-flow hash, session-id hash, definer lookup, and tenant RLS | Private BFF read/authority/logout operations open the tenant transaction selected by server-owned state; the BFF compares the returned tenant with the route | PASS — browser paths, headers, and cookie suffixes remain hints rather than authority |
| Candidate-test identity | `HumanConnections::begin_test` persists the exact candidate revision and `ConnectionTester` in single-use login state | The common callback verifies the real provider response, rechecks current tester permission, stamps only the bound revision, and returns before User/session/credential issuance | PASS — revision-7 real-sign-in direction remains closed |
| Authorization-response issuer | Typed callback `iss`, provider discovery support flag, and recorded `LoginState.issuer` | `AuthorizationCodeExchange` consumes state and applies conditional RFC 9207 validation before token-endpoint IO | PASS — the human issuer-binding direction remains closed |
| OIDC renewal credential | Sealed refresh token on the browser row; `RefreshTokens` owns family lock, rotation, replay containment, and audit | `BrowserSessions::renew/current` distinguishes ordinary refusal, committed containment, and retryable internal failure | PASS — replay, exact-expiry, and rollback semantics remain distinct |
| API-key renewal credential | Sealed bootstrap key on the browser row; `ExchangeApiKey` owns fixed-cost verification and issuance | Browser renewal accepts only the error owner's lifecycle refusals as terminal and rolls back all other failures | PASS — no second verifier or fallback credential path exists |
| Missing/unopenable renewal envelope | `open_credential` translates absent or unopenable state to `Renewal::Failed(Internal)` | `current` returns without commit, row revocation, or stale-token service; the new R4 unit test pins both cases | PASS for renewal — `FIND-TASK-003-17` is closed |
| Logout refresh-family authority | Live OIDC browser row stores the only recoverable refresh token; `RefreshTokens`/refresh SQL own revocation | `BrowserSessions::logout` skips family revocation when no key is held or opening fails, then wipes the row and returns success | **FAIL — INV-R5-001** |
| Sealing-key inventory | `SealedSecretTable::ALL` enumerates every non-null browser envelope, including expired rows | Canonical rewrap uses exact-byte CAS; keyless boot counts stored ciphertext | PASS — prior rotation and keyless-boot findings remain closed |
| Browser row coordination | `lock_browser_session` uses one PostgreSQL statement instant and holds `FOR UPDATE` | Replica renewals serialize; rotate/revoke happen inside the caller-owned `TenantConn` transaction | PASS |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: tenant settings project server-owned connection administration and test an exact candidate through a real sign-in | `connections.rs` `begin_test`/`stamp_test_sign_in`; `callback.rs` connection-test branch; settings actions project the server contract | Recorded candidate-test callback tests and Keycloak/Dex replacement journeys | PASS |
| Human issuer direction / INV-001: callback issuer binding is conditional, exact, and before token IO | Typed optional `CallbackQuery.iss`; discovery support projection; `verify_response_issuer` after state consumption and before code exchange | Recorded callback matrix and issuer-binding journey | PASS |
| REQ-005 / AC-007: recoverable browser credentials are sealed and canonical rotation inventories every envelope | Browser access, refresh/API-key, and CSRF columns; `SealedSecretRewrap` complete inventory and exact-byte CAS | Recorded browser-session rotation, expired-row inventory, and keyless-boot checks | PASS |
| REQ-006 / INV-001: canonical tenant entry and callback derive authority only from server-bound state | Hashed flow/state lookup, RLS consume, exact connection revision, fixed callback/completion routes | Recorded real-provider, wrong-flow, replay, expired, and cross-tenant journeys | PASS |
| REQ-009: production BFF session is replica-safe, CSRF/tenant/expiry bound, TLS-protected, and token-free in browser-visible data | Postgres session row, private service-key routes, trusted TLS origin validation, secure opaque cookies, server-only authority fetch | Recorded two-BFF TLS journey, leak checks, CSRF/tenant/forged-cookie and replay cases | PASS |
| REQ-010: OIDC-off UI uses existing API-key authority and never revokes the underlying operator key on logout | `BrowserSessions::exchange_api_key` delegates to `ExchangeApiKey`; API-key logout wipes only the browser row | Recorded OIDC-off journey and fixed-cost refusal checks | PASS |
| REQ-015 / AC-003: independently authenticated tenants switch only through their own server-resolved sessions | Per-tenant cookie hints are resolved sequentially through server reads; Keycloak and Dex sessions remain independent | Recorded multi-provider switch and cross-tenant/mixed-callback journey | PASS |
| REQ-016: ordinary lifecycle refusal preserves the issued access snapshot to exact expiry | `current` rolls back early refusal, relocks once, serves only the stored token, and revokes at/after PostgreSQL expiry | `proactive_renewal_refusal_preserves_authority_until_expiry` recorded green | PASS |
| REQ-017 / AC-007: replay containment and internal renewal failure keep their owners' transaction meaning | `Renewal::Contained` commits family revocation/audit; `Renewal::Failed` drops the transaction | Replay-containment and refresh/API-key rollback/retry selectors recorded green | PASS |
| TASK-003 logout contract: logout revokes the OIDC browser session and its refresh family atomically, while API-key logout leaves the operator key intact | Happy path opens the refresh envelope, finds the row, revokes it, then wipes the browser row in one transaction; failure to open is silently skipped before the wipe | Rotation journey proves only the openable happy path; no missing/unopenable refresh-envelope logout proof exists | **FAIL — INV-R5-001** |
| R4-AC-01: renewal classifiers remain crate-private with unchanged behavior | `IssuanceError::is_refusal`, `RefreshError::is_refusal`, and `ExchangeError::is_refusal` are `pub(crate)` and all callers remain in `wyrd-auth` | Source caller inspection; recorded lints and `test:wyrd` | PASS — `FIND-TASK-003-16` closed |
| R4-AC-02/03: local contracts and focused proof distinguish envelope-open failure from lifecycle refusal | `open_text` assigns lifecycle to callers; `open_credential` maps absent/unopenable state to `Renewal::Failed(Internal)` | Exact selector `missing_or_unopenable_renewal_credential_is_retryable_failure` passed in this review | PASS — `FIND-TASK-003-17` closed for renewal; sibling logout failure is INV-R5-001 |
| Prior `FIND-TASK-003-1` through `FIND-TASK-003-15` | Conditional issuer binding, fixed-cost verification, verified chooser, complete sealing inventory, trusted TLS, multi-provider journeys, exact expiry, and three-way renewal behavior remain in current source | Prior focused/journey evidence remains aligned with unchanged owners | PASS — remain closed |
| INV-003 / INV-005: principal planes and server-owned identity/permissions remain distinct | BFF service key grants only private session operations; ordinary Wyrd tokens and server-projected permissions remain the API authority | Recorded wrong-tenant, denied-action, and browser-leak cases | PASS |
| Explicit non-goals | No browser bearer storage, UI role mapper, password authority, provider-specific bypass, compatibility path, dependency, migration, alternate credential, second audit owner, or new harness entered R4 | Complete cumulative and R4 locator diff inspection | PASS |

## Prior-finding closure

| Prior finding | Current-source result |
|---|---|
| `FIND-TASK-003-1` | CLOSED under the approved human replacement: optional typed `iss` and conditional pre-token comparison remain in the shared exchange owner. |
| `FIND-TASK-003-2` | CLOSED: browser API-key entry retains the shared fixed-cost verifier/dummy path. |
| `FIND-TASK-003-3` | CLOSED: chooser cookie suffixes remain lookup hints resolved sequentially through server reads. |
| `FIND-TASK-003-4` | CLOSED: the canonical inventory includes every stored browser envelope and retains exact-byte CAS. |
| `FIND-TASK-003-5` | CLOSED: the production-built BFF journey retains the trusted TLS endpoint. |
| `FIND-TASK-003-6` | CLOSED: Keycloak/Dex switching, replacement actions, and mixed-callback refusals remain in the real journey. |
| `FIND-TASK-003-7` through `FIND-TASK-003-9` | CLOSED: the cited Rust documentation, server tenant projection, and removal of the dead lifetime variant remain intact. |
| `FIND-TASK-003-10` through `FIND-TASK-003-13` | CLOSED: exact-expiry refusal, provider documentation, sequential chooser resolution, and explicit-empty upstream refusal remain intact. |
| `FIND-TASK-003-14` | CLOSED for renewal: replay containment, ordinary refusal, and internal failure retain distinct commit/rollback outcomes. `INV-R5-001` is the sibling logout consumer's violation, not a reopened renewal-classification defect. |
| `FIND-TASK-003-15` | CLOSED: the shared API-key fixture module and live-key seed contract remain accurately documented. |
| `FIND-TASK-003-16` | CLOSED: all three renewal classifiers are `pub(crate)`. |
| `FIND-TASK-003-17` | CLOSED at its specified boundary: rustdoc is corrected and the focused missing/unopenable renewal-envelope test passes. The new logout finding follows that state into a separate required lifecycle consumer. |

## Verification Notes

- Reviewed the complete cumulative base-to-candidate range, the R4 locator diff `6aedcda5166509001db0cc851a5bc74502b4b043..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`, the original task, R2/R3/R4 remediation authorities, both human directions, prior validated ledgers/verdicts, and current browser-session, SQL, callback, connection-test, BFF, sealing, and journey sources.
- Ran `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::tests::missing_or_unopenable_renewal_credential_is_retryable_failure)'`: 1 passed.
- `git diff --check 63c5bffc93cd2f7b5ed558e610a213efcc34fd49..989d0734b0a9b04f314ef4b52aa7d8510f26fe11` passed.
- The candidate record reports the four browser-renewal Postgres selectors, both unit selectors, `test:wyrd`, `test:identity:journey`, `check:tenant-isolation`, format, lints, and diff checks green. This review did not rerun the broad or Postgres/provider/browser lanes. None of the existing checks exercises logout with an unopenable refresh envelope.

## Overall result

**FAIL**

R4 closes `FIND-TASK-003-16` and `FIND-TASK-003-17` at their specified visibility, documentation, and renewal-producer boundaries. The cumulative candidate still violates the original logout invariant at a sibling consumer: an unopenable OIDC refresh envelope is wiped while its refresh family remains live, so logout cannot be retried after repair.

# TASK-003 R4 invariant review

## Review Findings

### Critical

None.

### Important

- **INV-R4-001 — VIOLATION** — `crates/wyrd/wyrd-auth/src/browser_sessions.rs:753-777,840-845`: the R3 change intentionally maps a missing or unopenable stored renewal credential to `Renewal::Failed(WyrdError::Internal)`, and `BrowserSessions::current` returns that failure without committing or revoking the browser row. The adjacent `open_text` rustdoc still says that a credential sealed under a retired key ends its session, while the new classification-test rustdoc says an “unusable credential” is a refusal. Both statements contradict the newly approved retryable renewal behavior and the more specific `open_credential` contract. This violates the repository's hard requirement that materially affected Rust documentation accurately describe lifecycle and side effects, and it leaves the flagged behavior without a direct runnable check: the two new Postgres tests exercise corrupt-role failures, not a credential envelope that the held keyring cannot open. A maintainer can therefore follow the stale contract and reintroduce terminal session loss while the recorded R3 proof remains green. Update only these local contracts to distinguish direct access/CSRF opening from renewal-credential opening, and add one focused `open_credential` check using ciphertext sealed under an unheld key (plus the missing-field case) that asserts `Renewal::Failed`, not `Refused`; retain the existing Postgres rollback/retry tests for the transaction consequence.

### Suggestions

None.

## Open Questions

None.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: `TASK-003-R2-production-ui-remediation.md` and `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

The candidate resolved to the stated object before and after review. The repository has no `.codegraph/` directory, so navigation used the immutable Git range and repository-native search.

## Producer-to-sink invariant trace

| Authority or value | Producer and durable owner | Consumers and transition | Invariant result |
|---|---|---|---|
| Browser tenant identity | Login flow hash, session-id hash, and tenant RLS (`BrowserSessions`; browser-session SQL) | BFF `sessions/read` projects the server-returned tenant id/key; cookie suffix remains a lookup hint | PASS — no path/header/browser value becomes tenant authority |
| OIDC renewal credential | Sealed refresh token stored on the browser row; `RefreshTokens` owns family lock, rotation, reuse containment, and audit | `BrowserSessions::renew` opens it, preserves `Reused` as `Renewal::Contained`, and `current` commits containment before relocking | PASS — replay containment survives and issued access remains bounded to exact expiry |
| API-key renewal credential | Sealed bootstrap key stored on the browser row; `ExchangeApiKey` owns fixed-cost verification and tentative `last_used_at` | `BrowserSessions::renew` accepts only `ExchangeError::is_refusal` as terminal lifecycle refusal; other errors roll back | PASS — no second verifier or alternate credential path |
| Issuance state | `TenantTokenIssuer` reads current tenant, principal, connection, roles and permissions and appends canonical audit in the caller transaction | `IssuanceError::is_refusal` admits only tenant/principal/connection lifecycle outcomes; signing, audit, store and corrupt-state errors remain failures | PASS — invalid or partial issuance cannot commit as a successful renewal |
| Browser row lifecycle | `lock_browser_session` computes freshness and exact expiry from one PostgreSQL statement instant | `current` rotates, rolls back/relocks, commits containment, or revokes according to the preserved outcome | PASS — replicas serialize on one row; PostgreSQL remains the expiry authority |
| Unopenable renewal envelope | `open_text` returns `InvalidToken`; new `open_credential` deliberately translates missing/unopenable renewal state to `Renewal::Failed(Internal)` | `current` returns the error and drops the transaction, preserving the row for a corrected keyring/rewrap retry | **Behavior PASS; documentation/proof FAIL — INV-R4-001** |
| Sealing-key inventory | `SealedSecretTable::ALL` covers access, refresh, API-key and CSRF browser columns with no expiry/liveness predicate | Canonical rewrap uses exact-byte CAS; keyless boot counts every stored envelope | PASS — prior FIND-4 remains closed |
| Candidate connection test | `HumanConnections::begin_test` binds candidate revision and authorized tester into one-use state | Common callback performs normal discovery/code/ID-token checks, rechecks tester authority, stamps only that revision, audits, and issues no User/session/credential | PASS — human direction and revision-7 REQ-003 remain satisfied |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003 / AC-006: tenant settings and real candidate test sign-in | `connections.rs` `begin_test`/`stamp_test_sign_in`; `callback.rs` connection-test branch; UI settings actions | `tenant_connection_test_sign_in_journey`; Keycloak/Dex replacement UI journey recorded green | PASS |
| Human issuer direction / REQ-007 / INV-001: conditional RFC 9207 before token IO | Typed `CallbackQuery.iss`; provider support projection; `verify_response_issuer` after one-use state consumption and before exchange | Callback unit matrix and `tenant_callback_issuer_binding_journey` recorded green | PASS |
| REQ-005: provider and browser credentials sealed; rotation covers all stored envelopes | Browser session sealed columns; six-table canonical inventory and exact-byte CAS without expiry filters | Browser-session rotation journey; live and expired keyless-boot tests recorded green | PASS |
| REQ-006: canonical tenant login and server-bound callback | `/t/{tenantKey}/login`; common callback derives tenant/connection from consumed state | Real-provider browser journeys and wrong/cross-tenant callback cases recorded green | PASS |
| REQ-009: replica-safe BFF session, secure cookie, CSRF, server-only authority | Postgres session row; private BFF handlers; `ServerSessions.read/checkAction/api`; opaque cookie | Two-production-BFF journey, flow binding/replay, CSRF, forged-cookie, leak and logout assertions recorded green | PASS |
| REQ-010: OIDC-off UI uses existing API-key authority | `BrowserSessions::exchange_api_key` delegates to `ExchangeApiKey`, or pays one dummy verification for an unknown route | OIDC-off UI journey and exactly-one-verification tests recorded green | PASS |
| REQ-015 / AC-003: independent tenant/provider sessions and safe switch | Per-tenant cookies resolved through server reads; Keycloak and Dex topology | `production multi-provider tenant switch` plus wrong-provider and same-issuer callback refusal recorded green | PASS |
| REQ-016: lifecycle refusal preserves issued snapshot until exact expiry | `current` rolls back ordinary proactive refusal, relocks once, serves stored access, and revokes only at/after PostgreSQL expiry | `proactive_renewal_refusal_preserves_authority_until_expiry`; provider replacement journey recorded green | PASS |
| REQ-017 / AC-007: refresh replay containment and canonical audit survive browser renewal | `RefreshTokens::execute` owns family revocation/audit; `Renewal::Contained` commits that transaction before relock | `proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry` recorded green | PASS |
| R3-AC-02: internal refresh/API-key renewal failures fail closed, commit nothing, and remain retryable | Error-owner `is_refusal` classifiers; all other variants become `Renewal::Failed`; dropped transaction rolls back | Both corrupt-role Postgres retry tests recorded green | PASS for exercised producers |
| Flagged R3 behavior: missing/unopenable stored renewal credential is retryable internal failure, not terminal session end | `open_credential` maps absent or failed open to `Renewal::Failed(Internal)`; `current` returns without commit/revoke | Source proves behavior, but no focused test fails if this mapping regresses; adjacent rustdoc states the opposite | FAIL — INV-R4-001 |
| R3-AC-03: genuine credential/lifecycle refusals retain approved behavior | `RefreshError::is_refusal`, `ExchangeError::is_refusal`, `IssuanceError::is_refusal` closed classifications | Classification unit test and revoked-key Postgres control recorded green | PASS |
| R3-AC-04 / FIND-15: shared API-key fixture rustdoc is accurate | Module rustdoc and corrected `insert_live_api_key` seed/hash/insert contract | Direct source inspection; recorded format/lints | PASS |
| Prior FIND-1 through FIND-3 | Conditional issuer binding; fixed-cost browser API-key verification; sequential server-verified chooser reads | Focused unit/journey evidence retained | PASS — remain closed |
| Prior FIND-4 through FIND-6 | Complete browser-envelope inventory; trusted TLS terminator; Keycloak/Dex mixed-provider journeys | Keyless/rewrap, TLS BFF, switch/replacement journeys retained | PASS — remain closed |
| Prior FIND-7 through FIND-9 | Original rustdoc repaired; authoritative tenant id projected; `SessionLifetime` absent | Source and recorded format/lint/UI proof retained | PASS — remain closed |
| Prior FIND-10 through FIND-13 | Exact-expiry refusal; provider rustdoc; sequential chooser verification; nullish upstream default | Focused Rust/Vitest and journey evidence retained | PASS — remain closed |
| Prior FIND-14 and FIND-15 | Three-way renewal semantics implemented; shared fixture docs corrected | R3 focused Postgres tests and source inspection | PASS except the newly exposed stale local documentation/proof gap in INV-R4-001 |
| INV-003 / INV-005: principal planes and server authority remain distinct | BFF key authorizes only internal session operations; roles/permissions and tenant identity are server projections | Wrong-tenant, denied action, and browser leak assertions retained | PASS |
| Explicit non-goals | No browser bearer storage, UI role mapper, password store, alternate credential, provider-specific bypass, new dependency, migration, compatibility route, or second renewal/audit owner in R3 | Cumulative diff inspection | PASS |

## Prior-finding closure

`FIND-TASK-003-1` through `FIND-TASK-003-15` remain source-closed under their approved corrections and both human directions. `INV-R4-001` is new: it does not reopen `FIND-TASK-003-14`; the runtime classification is correct, but the materially affected local documentation and focused proof do not describe or pin the newly flagged envelope-open behavior.

## Verification Notes

- Reviewed the complete cumulative base-to-candidate diff and the R3 locator diff, all prior R1-R3 verdicts and validated ledgers, both remediation tasks supplied by the caller, both human directions, and applicable repository/security/testing authority.
- The R3 implementation record reports the three new Postgres tests, the ordinary-refusal control, the classification unit test, `test:wyrd`, `test:identity:journey`, `check:tenant-isolation`, format, lints, and `git diff --check` green.
- This independent pass did not rerun Cargo, mise, pnpm, Postgres, provider, or browser commands. The retained finding is source-proven and is not a substitute for an unavailable required reviewer.

## Overall result

**FAIL**

The cumulative behavior is otherwise invariant-preserving, including the explicitly flagged retryable handling of a stored renewal credential that cannot be opened. Acceptance remains blocked by `INV-R4-001`: the local Rust contracts still state the opposite lifecycle result and no focused check pins that producer-to-outcome mapping.

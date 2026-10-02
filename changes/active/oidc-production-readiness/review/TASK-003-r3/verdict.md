# TASK-003 review verdict — round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Candidate tree: `5e7187d4c52fc4167070cec88108f1af4f127523`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation reviewed: `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

The complete base-to-candidate range was reviewed. The R2 implementation range
`622a77028..8289fa298ed33d21f2568558bc0a02905fd0b218` was used only to locate
the latest changed owners and check prior-finding closure. The candidate
resolved to the stated commit before discovery, after follow-up, and after
independent validation. This repository has no `.codegraph/` directory.

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior | FAIL | `BEH-R3-001`: proactive renewal erases refresh-replay containment and audit. |
| Invariants | PASS | Empty proposed ledger. |
| Repository standards | FAIL | `STD-R3-001`: newly shared test module/helper rustdoc is missing or inaccurate. |
| Maintainer | PASS | No material finding. |
| System resilience | FAIL | `SYSTEM-R3-001`: renewal internal failures are collapsed into durable credential refusal behavior. |
| Security/identity domain | PASS | No material finding. |
| Persistence/concurrency domain | FAIL | `PC-R3-001`: replay and internal failures lose their distinct transaction semantics. |
| Focused follow-up | RESOLVED | The runtime claims share one lossy renewal-result source; immediate browser-row revocation before access expiry is not approved. |
| Structured Ponytail validation | FIX_REQUIRED | Retained `FIND-TASK-003-14` and `FIND-TASK-003-15`. |

All required independent reports are present. The conflicting discovery claims
were resolved by the focused follow-up and independently checked by the
Ponytail validator.

## Reconciled acceptance matrix

| Obligation | Implementation and proof | Result |
|---|---|---|
| Production BFF sessions remain server-owned, tenant-bound, CSRF-protected, replica-safe, and browser-secret-free | Postgres-backed `BrowserSessions`, private BFF routes, authoritative server metadata, two-replica UI journeys | PASS |
| REQ-003 / AC-006 / human direction: test a candidate through one real provider sign-in, bind exact revision and tester, issue no User/session/credential, and audit the result | Common authorization-code exchange, test-bound login state, callback authority recheck and transactional stamp; Keycloak/Dex and negative journeys | PASS |
| Conditional RFC 9207 issuer handling follows the approved human direction | Typed optional `iss`, discovery support projection, exact pre-token comparison, advertising/non-advertising provider coverage | PASS |
| Canonical sealing inventory includes expired stored browser envelopes and keyless boot refuses while any remain | Shared inventory and exact-byte CAS; expired-envelope and rotation proof | PASS |
| A production-built BFF uses the authenticated private channel through real trusted TLS while loopback HTTP remains local-only | Native Node fetch through the repository TLS terminator and existing URL-policy tests | PASS |
| Multi-provider switching uses Keycloak and Dex and mixed callbacks preserve genuine provider parameters | Existing production UI journey | PASS |
| Ordinary connection/key/policy renewal refusal preserves already-issued access until exact expiry and then ends the session | Existing rollback/relock owner and Postgres-backed API-key proof | PASS |
| Refresh replay containment and internal renewal failures preserve their distinct durable semantics | `BrowserSessions::renew` collapses replay, ordinary refusal, and most internal failures into `Renewal::Refused`; the existing proof covers only revoked API-key refusal | **FAIL — FIND-TASK-003-14** |
| Cookie-hint verification is bounded and explicit empty upstream configuration is refused | Sequential `ServerSessions.metadata`; nullish-only upstream default; focused Vitest coverage | PASS |
| Every materially modified Rust module and test helper has accurate substantive rustdoc | The shared `exchange_api_key::pg_tests` module lacks module rustdoc and `insert_live_api_key` carries another test's stale contract | **FAIL — FIND-TASK-003-15** |
| Non-goals remain excluded | No local password authority, UI role mapper, browser token storage, provider-specific test bypass, compatibility route, new dependency, or second auth owner | PASS |

## Validated finding ledger

### FIND-TASK-003-14 — Browser renewal loses security outcome semantics

- **Status:** REVISED
- **Classification:** REGRESSION
- **Sources:** `BEH-R3-001`, `PC-R3-001`, `SYSTEM-R3-001`, resolved by `followup-review.md`
- **Violated obligation:** REQ-007, REQ-009, REQ-016, REQ-017, AC-007, the refresh-reuse containment rule, R2-AC-04 exact-expiry preservation, and its fail-closed infrastructure constraint.
- **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:435-585`; producers in `refresh.rs:115-226`, `exchange_api_key.rs:154-185`, and `issuance.rs:369-497`; ordinary replay precedent in `wyrd-server/src/components/auth/routes.rs:224-263`.
- **Consequence:** detected refresh theft can lose its family revocation and canonical audit while an attacker-held successor remains renewable. Internal signing, corrupt-state, verification-task, or similar failures can serve authority before expiry or durably end a retryable browser session after expiry.
- **Required correction:** preserve replay-contained, ordinary-refusal, and internal-failure outcomes through the existing `BrowserSessions` owner. Commit the existing refresh-family containment/audit without ending still-valid browser access; propagate and roll back internal failures; retain the current exact-expiry behavior only for genuine credential/lifecycle refusal.

### FIND-TASK-003-15 — Newly shared API-key test support has invalid rustdoc

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Source:** `STD-R3-001`
- **Violated obligation:** `AGENTS.md` section 16 and `architecture/agent-rules.md` require substantive, accurate rustdoc for materially modified modules and test helpers, including panic conditions.
- **Location:** `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:540-541,866-904`.
- **Consequence:** the candidate violates a hard merge rule and misstates the shared helper's ownership and panic boundary.
- **Required correction:** document the existing shared test-support module and replace only `insert_live_api_key`'s stale block with its actual seed contract and hash/insert panic conditions.

The decision-complete corrections and closure proof are packaged in
`TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`.

## Prior-finding and human-direction closure

`FIND-TASK-003-1` through `FIND-TASK-003-9` and `FIND-TASK-003-11` through
`FIND-TASK-003-13` remain closed from current source. `FIND-TASK-003-10` is
closed for ordinary lifecycle refusal; its correction introduced the distinct
shared regression recorded as `FIND-TASK-003-14`, so the prior ID is not
reopened. Both human directions are satisfied and receive no retrospective
finding ID.

## Verification limits

The R2 implementation record reports the focused tests and the UI, identity,
Wyrd, SQL, codegen, tenant-isolation, docs, format, lint, and diff checks green.
This review was a static acceptance audit and did not rerun builds, Postgres,
provider, browser, Cargo, mise, or pnpm commands. The retained findings are
source-proven reachable gaps, not substitutes for missing execution evidence.

## Verdict

**FIX_REQUIRED**

The two retained corrections remain inside existing owners and approved
revision-7 behavior. They require no specification revision, dependency, new
public contract, new persistent state, or new test harness.

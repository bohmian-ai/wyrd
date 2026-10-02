# TASK-003 round-5 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation inputs:
  `TASK-003-r2/TASK-003-R2-production-ui-remediation.md`,
  `TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and
  `TASK-003-r4/TASK-003-R4-renewal-contract-boundaries.md`
- Human directions:
  `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

The human owner explicitly authorized the R3 and R4 rounds. The candidate
resolved to the stated object before discovery, after follow-up, after
validation, and before this verdict. The repository has no `.codegraph/`
directory, so reviewers used the immutable Git range, repository search, and
direct source inspection.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| Revision-7 real candidate connection testing and conditional RFC 9207 issuer binding | The shared authorization-code exchange retains exact candidate/tester binding, current permission recheck, no-issuance completion, and conditional callback `iss` validation. Existing Keycloak/Dex and negative-state journey evidence remains present. | PASS |
| Production browser BFF, tenant isolation, CSRF, trusted TLS, server-only authority, and independent tenant/provider sessions | The cumulative UI, server, auth, SQL, and journey implementation retains opaque cookies, private service-key routes, PostgreSQL/RLS authority, trusted transport, and multi-provider switching without browser-held Wyrd credentials. | PASS |
| Renewal refusal, replay containment, and internal failure remain distinct | Ordinary refusal preserves already-issued authority only to exact expiry; replay containment commits family revocation and audit; internal failure rolls back and remains retryable without serving stale authority. | PASS |
| R4 visibility, rustdoc, and direct envelope-failure proof | The three renewal classifiers are crate-private; `open_text` and `open_credential` describe their actual boundaries; the exact missing/unopenable renewal-envelope unit selector passes. | PASS — `FIND-TASK-003-16` and `FIND-TASK-003-17` closed |
| OIDC logout atomically revokes the browser session and its refresh family | `BrowserSessions::logout` locks the browser row but derives revocation from the stored refresh bytes and calls single-row `revoke_refresh`. An unreadable envelope skips revocation, and a committed or concurrent rotation can leave a successor active before the browser row is wiped and committed. Existing logout proof covers only the currently stored predecessor. | **FAIL — `FIND-TASK-003-18`** |
| API-key logout preserves the underlying operator key | API-key mode wipes only browser-session state and does not revoke the operator API key. | PASS |
| Prior findings and both human directions remain closed | `FIND-TASK-003-1` through `FIND-TASK-003-17` remain closed at their approved boundaries. The new finding is a separate logout consumer failure, not a reopening of renewal classification. | PASS |
| Explicit non-goals remain excluded | No password authority, browser-held bearer credential, UI role mapper, provider-specific bypass, compatibility path, new dependency, migration, persistent retry marker, second audit owner, or new test harness entered the R4 implementation. | PASS |

## Independent review results

| Report | Result | Material proposals |
|---|---|---|
| `task-review-behavior.md` | PASS | Empty ledger. |
| `task-review-invariants.md` | FAIL | `INV-R5-001`: logout can wipe an OIDC row when its refresh envelope cannot be opened. |
| `standards-review.md` | PASS | Empty ledger. |
| `maintainer-review.md` | PASS | Empty ledger. |
| `system-review.md` | PASS | Empty ledger. |
| `domain-review-security-identity.md` | PASS | Empty ledger. |
| `domain-review-persistence-concurrency.md` | FAIL | `PC-R5-001`: logout revokes only the stored refresh row, so a successor can survive. |

The reports materially disagreed on the logout lifecycle and proposed two
different symptoms at the same boundary. The required fresh follow-up resolved
the uncertainty: a missing envelope is schema-unreachable for a live OIDC row,
but an unreadable envelope and stale or concurrent successor are reachable.
`followup-review.md` consolidated both under `FU-R5-001`, at logout's dependence
on refresh-token bytes. The mandatory fresh Ponytail validation independently
traced all writers, callers, locks, and consumers and retained one revised
finding.

## Validated finding ledger

### FIND-TASK-003-18 — OIDC logout does not retire the locked session's refresh family

- **Status:** REVISED
- **Classification:** INCORRECT
- **Discovery sources:** `INV-R5-001`, `PC-R5-001`, `FU-R5-001`
- **Violated obligation:** TASK-003 requires OIDC logout to revoke the browser
  session and its refresh family in one idempotent transaction, while API-key
  logout must leave the underlying operator key valid.
- **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs`, in
  `BrowserSessions::logout`; the existing family authority is
  `lock_refresh_family` and `revoke_refresh_family` in
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs`.
- **Evidence:** the locked row already supplies `mode` and `principal_id`, but
  logout ignores them. It opens the stored refresh envelope, resolves that one
  token row, and calls `revoke_refresh` without the family lock. An unreadable
  envelope skips revocation; an already-rotated predecessor updates no active
  row; a concurrent rotation can insert a successor. Logout nevertheless wipes
  and commits the browser row.
- **Observable consequence:** the UI reports logout complete and clears the
  browser session while an issued or concurrently created successor refresh
  token can continue minting User authority.
- **Required correction:** inside the existing logout transaction, branch on
  the locked row's mode. For OIDC mode, reuse
  `lock_refresh_family(&mut conn, "user", row.principal_id)` and
  `revoke_refresh_family(&mut conn, "user", row.principal_id,
  "browser_logout")` before wiping the browser row. For API-key mode, retain
  the browser-only wipe. Remove logout's dependency on the refresh envelope,
  token hash, and single-row revocation; add no new state, API, abstraction, or
  audit owner.
- **Focused closure proof:** add one existing-module Postgres test that rotates
  the browser row's stored refresh token through the ordinary refresh owner,
  leaves the committed successor outside the browser row, makes only the
  stored refresh envelope unreadable, logs out, and proves the browser row is
  wiped, every active User-family refresh row is revoked, and the successor
  cannot rotate. Retain the API-key logout proof. The reused family advisory
  lock supplies concurrency serialization, so no new concurrency harness is
  required.

## Prior-finding closure

`FIND-TASK-003-1` through `FIND-TASK-003-17` remain closed with current-source
evidence recorded in `findings-validation.md`. In particular, R4 closes the
classifier visibility, rustdoc, and renewal-envelope producer gaps. The new
finding follows the reachable unreadable state into the separate logout
consumer and reuses the existing refresh-family authority. Both human
directions remain satisfied and receive no retrospective finding IDs.

## Verification evidence and limits

The candidate record reports both R4 unit selectors, four Postgres renewal
selectors, `mise run test:wyrd`, `mise run test:identity:journey`,
`mise run check:tenant-isolation`, formatting, lints, and `git diff --check`
green. The behavior reviewer reran both R4 unit selectors; the invariant
reviewer reran the direct envelope selector. The cumulative Git range and all
R5 reports pass `git diff --check`.

This review did not rerun every broad, Postgres, provider, or browser lane. The
existing logout journey revokes only the exact refresh token still stored in
the browser row; it does not create a committed successor, overlap rotation
with logout, or make only that envelope unreadable. The retained finding is a
source-validated proof gap and implementation defect, not a substitute for a
missing reviewer or unavailable subject. Every required report is present.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-003-18` is one bounded correction inside the existing browser
session and refresh-family owners. It requires no new product, public API,
architecture, security, compatibility, concurrency, resource-ownership, or
persistent-data decision.

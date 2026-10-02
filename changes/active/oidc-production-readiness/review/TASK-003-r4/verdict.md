# TASK-003 round-4 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation inputs:
  `TASK-003-r2/TASK-003-R2-production-ui-remediation.md` and
  `TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions:
  `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

The human owner explicitly authorized the R3 round after the third failed
review. The candidate resolved to the stated object before discovery, before
validation, and before this verdict. The repository has no `.codegraph/`
directory, so reviewers used the immutable Git range, repository search, and
direct source inspection.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| Revision-7 real candidate connection test and conditional RFC 9207 issuer binding | The shared authorization-code exchange retains exact candidate/tester binding, current permission recheck, no-issuance completion, and conditional callback `iss` validation. Keycloak/Dex and negative-state journey evidence remains present. | PASS |
| Production browser BFF, tenant isolation, CSRF, TLS, server-only authority, and independent tenant/provider sessions | The cumulative UI, server, auth, SQL, and journey implementation retains opaque cookies, server-side credentials, private service-key routes, PostgreSQL/RLS authority, trusted TLS, and multi-provider switching. | PASS |
| Ordinary renewal refusal preserves issued authority until exact expiry | `Renewal::Refused` rolls back before expiry, relocks once, and ends the browser session only at or after PostgreSQL expiry. The focused refusal test passed during R4 discovery. | PASS |
| Refresh replay containment survives proactive browser renewal | `Renewal::Contained` commits the existing refresh-family revocation and audit, then preserves only the already-issued access token until exact expiry. The focused containment test passed during R4 discovery. | PASS |
| Internal renewal failures fail closed, commit nothing, and remain retryable | Non-refusal refresh/API-key failures become `Renewal::Failed`; the focused refresh and API-key rollback/retry tests passed during R4 discovery. | PASS |
| Flagged behavior: a missing or unopenable stored renewal credential is retryable instead of terminal | `open_credential` maps both states to `Renewal::Failed(Internal)`; `current` returns without commit, revocation, or old-token service, and the BFF retains the cookie for the non-401 failure. Runtime behavior passes, but adjacent rustdoc contradicts it and no direct unit check pins this producer. | FAIL — `FIND-TASK-003-17` |
| R3 adds no new public contract | The three renewal-policy classifiers are `pub` although every caller is inside `wyrd-auth`; the approved correction requires only crate visibility. | FAIL — `FIND-TASK-003-16` |
| Prior findings and both human directions remain closed | `FIND-TASK-003-1` through `FIND-TASK-003-15` remain source-closed. The new findings are bounded R3 visibility and documentation/proof gaps, not reopened runtime findings. | PASS |
| Explicit non-goals remain excluded | No password authority, browser-held Wyrd credential, UI role mapper, provider-specific bypass, compatibility path, dependency, migration, persistent replay marker, second audit owner, or new test harness entered the R3 implementation. | PASS |

## Independent review results

| Report | Result | Material proposals |
|---|---|---|
| `task-review-behavior.md` | PASS | Empty ledger; four focused renewal tests passed. |
| `task-review-invariants.md` | FAIL | Contradictory credential-open contracts and missing direct producer proof. |
| `standards-review.md` | FAIL | Three crate-internal classifiers unnecessarily use public visibility. |
| `maintainer-review.md` | FAIL | The same visibility issue and credential-open documentation/proof gap. |
| `system-review.md` | PASS | Empty ledger; retryable failure reduces durable logout blast radius without serving authority. |
| `domain-review-security-identity.md` | PASS | Empty ledger; request fails closed and preserves retryability. |
| `domain-review-persistence-concurrency.md` | PASS | Empty ledger; transaction, lock, RLS, expiry, and durability semantics hold. |

The reports materially disagreed on whether the direct credential-open proof
and local documentation were acceptance defects. `followup-review.md`
resolved that conflict and also confirmed the classifier visibility boundary.
The mandatory fresh Ponytail validation then independently inspected all
callers and retained two consolidated findings.

## Validated finding ledger

### FIND-TASK-003-16 — Renewal classifiers unnecessarily widen the public Rust API

- **Status:** CONFIRMED
- **Classification:** DRIFT
- **Sources:** `STD-TASK-003-R4-1`, `MAINT-R4-001`, `FOLLOWUP-R4-002`
- **Violated obligation:** R3's no-new-public-contract constraint and the
  repository's `pub(crate)`-by-default rule.
- **Location:** `wyrd-auth/src/issuance.rs`, `refresh.rs`, and
  `exchange_api_key.rs`, at the three new `is_refusal` methods.
- **Consequence:** private browser-renewal policy becomes an unnecessary crate
  API with compatibility and documentation cost.
- **Required correction:** narrow only those methods to `pub(crate)`; retain
  their owning public error types, exhaustive matches, callers, and tests.

### FIND-TASK-003-17 — Retryable renewal-envelope failure has contradictory rustdoc and no direct regression check

- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Sources:** `INV-R4-001`, `MAINT-R4-002`, `FOLLOWUP-R4-001`
- **Violated obligation:** accurate workflow/side-effect rustdoc and practical
  Rust proof for new core behavior, plus R3's retryable internal-failure
  requirement.
- **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs`, around
  `open_text`, `open_credential`, and the renewal-classification unit test.
- **Consequence:** adjacent contracts describe opposite lifecycle outcomes,
  and the old terminal classification could return without failing the R3
  Postgres tests because they inject failure after credential opening.
- **Required correction:** make `open_text` describe its error and
  caller-owned lifecycle policy; narrow the classification-test prose to real
  credential/lifecycle refusals; add one direct unit test proving absent and
  unopenable renewal envelopes produce `Renewal::Failed(Internal)`.

The decision-complete corrections and closure proof are packaged in
`TASK-003-R4-renewal-contract-boundaries.md`.

## Prior-finding closure

`FIND-TASK-003-1` through `FIND-TASK-003-15` remain closed with current-source
evidence recorded in `findings-validation.md`. The issuer-binding and real
interactive connection-test human directions remain satisfied and receive no
retrospective finding IDs.

## Verification evidence and limits

R4 discovery reran the four R3 Postgres-backed browser-renewal selectors; all
passed. The persistence reviewer also reported the migration idempotence check
green. The cumulative implementation record reports the required Wyrd,
identity journey, SQL, UI, codegen, documentation, tenant-isolation, format,
lint, and diff checks green. This review did not rerun every broad cumulative
lane. `git diff --check` passed for the immutable range and for the review
artifacts.

The retained findings are source-validated gaps, not substitutes for an
unavailable reviewer or missing subject. Every required report is present.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-003-16` and `FIND-TASK-003-17` are bounded corrections inside
existing owners and approved revision-7 behavior. They require no new product,
public API, architecture, security, compatibility, concurrency,
resource-ownership, or persistent-data decision.

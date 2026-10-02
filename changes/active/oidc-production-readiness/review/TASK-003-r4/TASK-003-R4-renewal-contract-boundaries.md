---
id: TASK-003-R4
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 7
requirements: [REQ-009, REQ-016, REQ-017, AC-007]
depends_on: [TASK-003-R3]
parent_task: TASK-003
remediates: [FIND-TASK-003-16, FIND-TASK-003-17]
---

# Close renewal API, documentation, and proof boundaries

Route this task directly to `$wyrd-implement`. A later `$wyrd-task-review`
must reassess the complete original base-to-remediated-candidate range.

## Authority and subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Reviewed base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Reviewed candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Prior remediation:
  `TASK-003-r2/TASK-003-R2-production-ui-remediation.md` and
  `TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions:
  `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

## Diagnoses

### FIND-TASK-003-16 — Renewal classifiers unnecessarily widen the public Rust API

R3 added `IssuanceError::is_refusal`, `RefreshError::is_refusal`, and
`ExchangeError::is_refusal` as public methods. Complete repository caller
tracing finds uses only inside `wyrd-auth`: browser renewal calls the refresh
and exchange classifiers, and those classifiers reuse the issuance
classifier. Tests are in the same crate. External crates legitimately use the
public error types, but no external caller needs this browser-renewal policy.

The current visibility therefore violates the R3 no-new-public-contract
constraint and the repository's narrowest-visibility rule. It exposes an
internal policy as a callable crate API with avoidable compatibility and
documentation cost. No behavior or error-type visibility needs to change.

### FIND-TASK-003-17 — Retryable renewal-envelope failure has contradictory rustdoc and no direct regression check

The runtime behavior is correct: `open_credential` maps a missing envelope or
one that the held keyring cannot open to `Renewal::Failed(Internal)`, and
`BrowserSessions::current` returns without commit, revocation, or serving the
old token. The BFF preserves the cookie for the non-401 failure, so repair of
the keyring or stored envelope permits retry.

Two adjacent contracts still describe the old lifecycle. `open_text` says a
credential sealed under a retired key ends its session even though lifecycle
policy now belongs to its caller and renewal deliberately keeps the session
retryable. The renewal-classification unit-test prose also calls an unusable
credential a refusal wherever it appears, contradicting `open_credential`.

The existing Postgres tests inject corrupt roles only after successfully
opening the stored credential. They prove rollback and retry after
`Renewal::Failed` exists, but they cannot fail if `open_credential` regresses
to terminal `Renewal::Refused`. The changed producer needs one direct,
IO-free regression check.

## Intended correction outcome

Renewal refusal policy remains a crate-private implementation detail. Local
rustdoc accurately distinguishes raw envelope-open errors from caller-owned
session lifecycle. A focused unit check pins the approved rule that absent or
unopenable stored renewal credentials are retryable internal failures. All
existing runtime, transaction, security, and exact-expiry behavior remains
unchanged.

## Decision-complete recommendation

1. Change only the three `is_refusal` methods on `IssuanceError`,
   `RefreshError`, and `ExchangeError` from `pub` to `pub(crate)`. Keep the
   error enums public, retain the exhaustive matches, and do not add a trait,
   shared policy type, wrapper, or re-export.
2. In `browser_sessions.rs`, revise `open_text` rustdoc to describe the
   `InvalidToken` result when no held key opens or decodes the envelope and to
   state that the caller owns the lifecycle consequence. Revise the existing
   classification-test prose so “refusal” covers rejected
   credential/lifecycle outcomes, not stored-envelope open failure.
3. Add one focused unit test in the existing browser-session test module. It
   must pass both no stored envelope and ciphertext sealed under a key absent
   from the active keyring to `open_credential`, and assert each produces
   `Renewal::Failed(WyrdError::Internal)`, never `Renewal::Refused` or
   `Renewal::Contained`. Reuse the current `SealingKeyring`, `Renewal`, and test
   module; add no fixture or harness.

This correction stays at the producer and visibility boundaries. The existing
Postgres tests remain the proof that a produced internal failure rolls back
tentative refresh/API-key work and remains retryable before and after expiry.

## Constraints and preserved behavior

- Preserve the three renewal outcomes: replay containment commits, ordinary
  refusal rolls back before exact expiry and ends at/after expiry, and internal
  failure returns without commit or revocation.
- Preserve the implementer-flagged behavior: missing or unopenable stored
  renewal credentials are retryable internal failures and never serve stale
  authority.
- Preserve `RefreshTokens`, `ExchangeApiKey`, `TenantTokenIssuer`,
  `BrowserSessions`, the current row lock, PostgreSQL expiry authority, audit
  transactionality, RLS, and caller-owned transaction boundaries.
- Preserve all prior TASK-003, R1, R2, and R3 closures and both human
  directions.
- Do not add a dependency, feature, public contract, migration, persistent
  state, retry framework, test harness, compatibility path, or unrelated
  refactor.
- Do not weaken, ignore, delete, or relabel an existing gate or assertion.

## Explicit non-goals

- No change to error variants, HTTP status mapping, token lifetime, renewal
  margin, cookie behavior, refresh format, connection lifecycle, sealing
  rotation, or provider behavior.
- No redesign of renewal classification or extraction into another owner.
- No new Postgres, server, provider, or browser journey for the pure producer
  mapping.
- No documentation cleanup outside the two contradictory local contracts.

## Acceptance criteria

| Criterion | Finding | Required observable result |
|---|---|---|
| R4-AC-01 | `FIND-TASK-003-16` | The three renewal classifiers are callable throughout `wyrd-auth` but are not public crate API; all existing callers and classification behavior remain unchanged. |
| R4-AC-02 | `FIND-TASK-003-17` | `open_text` and the classification-test rustdoc accurately distinguish raw open failure, caller-owned lifecycle, and true credential/lifecycle refusal. |
| R4-AC-03 | `FIND-TASK-003-17` | A focused unit check proves both a missing stored renewal envelope and ciphertext sealed under an unheld key map to `Renewal::Failed(WyrdError::Internal)`. |
| R4-AC-04 | both | Existing replay-containment, ordinary-refusal, and refresh/API-key internal-failure tests remain green with no changed public contract or durable behavior. |

## Focused proof

Add one unit test named
`missing_or_unopenable_renewal_credential_is_retryable_failure` under the
existing `browser_sessions::tests` module and run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth --lib \
  -E 'test(=browser_sessions::tests::missing_or_unopenable_renewal_credential_is_retryable_failure)'

mise exec -- cargo nextest run --locked -p wyrd-auth --lib \
  -E 'test(=browser_sessions::tests::only_lifecycle_refusals_end_a_renewing_session)'
```

Retain and rerun the existing Postgres-backed consequence checks through the
repository-managed lifecycle:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::refresh_renewal_internal_failure_rolls_back_and_remains_retryable)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::api_key_renewal_internal_failure_rolls_back_and_remains_retryable)'"
```

The implementer must record the expected RED for the new unit check before the
correction and the final GREEN result.

## Broader verification

```bash
mise run test:wyrd
mise run test:identity:journey
mise run check:tenant-isolation
mise run fmt
mise run lints
git diff --check
```

A red gate blocks completion. Diagnose and correct it without weakening the
gate.

## Implementation Evidence

Commit: `262136474`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R4-AC-01 | `IssuanceError::is_refusal`, `RefreshError::is_refusal` and `ExchangeError::is_refusal` are now `pub(crate)`. The error enums stay public; matches, callers and behaviour are unchanged | `wyrd-auth` builds; `mise run lints`; `only_lifecycle_refusals_end_a_renewing_session` | PASS |
| R4-AC-02 | `open_text` rustdoc now covers its raw `InvalidToken` error and says the lifecycle consequence belongs to the caller. The classification-test prose limits refusals to producer-rejected credentials and lifecycle outcomes | Source inspection; `mise run fmt` and `mise run lints` | PASS |
| R4-AC-03 | `browser_sessions::tests::missing_or_unopenable_renewal_credential_is_retryable_failure` (absent envelope, and ciphertext sealed under an unheld key) | RED: with `open_credential` temporarily restored to the old terminal `ok_or(Renewal::Refused)`, it failed with `a missing renewal credential must be a retryable internal failure`. GREEN with the correction | PASS |
| R4-AC-04 | No runtime change | The four `browser_sessions::pg_tests` renewal selectors (replay containment, ordinary refusal, refresh and API-key internal failure) | PASS |

Non-goals stayed excluded: no change to error variants, HTTP mapping, lifetimes, cookies or sealing; no new fixtures, harness or public contract.

Verification, all run with `CARGO_TARGET_DIR` set to the shared target:
- Both unit selectors and the four Postgres selectors.
- Mise lanes: `fmt`, `lints`, `check:tenant-isolation`, `test:wyrd` and `test:identity:journey`.
- `git diff --check`.

All exited 0.

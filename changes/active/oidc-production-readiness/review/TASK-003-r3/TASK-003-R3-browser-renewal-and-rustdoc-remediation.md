---
id: TASK-003-R3
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 7
requirements: [REQ-007, REQ-009, REQ-016, REQ-017, AC-007]
depends_on: [TASK-003-R2]
parent_task: TASK-003
remediates: [FIND-TASK-003-14, FIND-TASK-003-15]
---

# Preserve browser-renewal semantics and correct shared test rustdoc

## Authority and subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Reviewed base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Reviewed candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

Route this task directly to `$wyrd-implement`. A later `$wyrd-task-review`
must reassess the complete original base-to-remediated-candidate range.

## Diagnoses

### FIND-TASK-003-14 — Browser renewal loses security outcome semantics

`BrowserSessions::renew` currently reduces refresh replay containment,
ordinary credential/lifecycle refusal, and most internal issuance failures to
one `Renewal::Refused` result. `BrowserSessions::current` can therefore only
roll back and serve the stored access token before expiry, or revoke the
browser row and commit after expiry.

That behavior is correct only for an ordinary credential/lifecycle refusal.
For a reused refresh token, `RefreshTokens::execute` has already revoked the
complete family and appended the canonical containment audit in the caller's
transaction. Rolling back the proactive attempt erases both effects and leaves
the attacker-held successor renewable. For an audit, store, signing,
role/corrupt-state, envelope, verification-task, or other internal failure,
serving the token before expiry violates the explicit fail-closed constraint;
after expiry, revoking and committing can durably end a retryable browser
session and preserve tentative refresh/key-use state without successful
issuance.

The existing focused test proves only revoked API-key refusal. It does not
exercise OIDC refresh reuse or internal renewal failure, so its green result
does not close this gap.

### FIND-TASK-003-15 — Newly shared API-key test support has invalid rustdoc

`exchange_api_key::pg_tests` became crate-visible so the sibling browser
session tests can reuse its fixtures, but the materially modified module has no
module rustdoc. The newly shared `insert_live_api_key` helper retains prose and
`# Panics` conditions for a different machine-exchange test instead of its own
hash-and-insert seed contract. This violates the repository's hard private-item
documentation rule even though compilation and lint evidence is green.

## Intended correction outcome

Browser renewal preserves the semantic and transactional meaning of its
existing owners: replay containment commits, ordinary credential refusal keeps
the current access snapshot only until exact expiry, and internal failures
remain retryable request failures that commit no credential or browser-row
mutation. The newly shared test support accurately documents its ownership and
panic boundary.

## Decision-complete recommendation

Keep `BrowserSessions` as the sole browser renewal orchestrator,
`RefreshTokens` as the sole refresh-family containment/audit owner, the current
browser-row lock and PostgreSQL expiry authority, and the existing
`ExchangeApiKey` and issuance owners. Preserve these three outcomes without
collapsing them:

1. **Refresh replay contained.** Preserve `RefreshError::Reused` as a distinct
   outcome. Before browser access expiry, commit the existing family revocation
   and canonical audit, leave the browser row live, reopen its existing lock,
   and serve only the already-issued access token until its stored expiry. At
   or after expiry, revoke the browser row in the same containment transaction
   and refuse. Do not duplicate family revocation or audit in
   `BrowserSessions`.
2. **Ordinary credential or lifecycle refusal.** Restrict this class to
   refresh-not-found; inactive tenant, principal, or exact connection; and
   malformed, missing, cross-tenant, wrong-secret, revoked, or expired API-key
   outcomes. Before access expiry, roll back the full attempt, relock, and
   serve the stored token. At or after expiry, end the browser session without
   trying another credential, preserving current behavior.
3. **Internal failure.** Treat database/store, audit, signing,
   role/corrupt-state, envelope-open, verification-task, and other internal
   outcomes as failures. Roll back and return the failure before and after
   access expiry. Never serve the old token because of such a failure, revoke
   the browser row, or commit tentative refresh consumption or API-key use.
4. **Documentation.** Add concise module rustdoc for the existing shared
   API-key/browser-session test fixtures and replace only
   `insert_live_api_key`'s stale rustdoc with its actual insert contract and
   hash/insert panic conditions.

This uses existing owners and private result boundaries. Add no persistent
marker merely to suppress repeated pre-expiry replay containment; the current
access-token lifetime already bounds that behavior, and a new marker would be
an unapproved persistent-data decision.

## Constraints and preserved behavior

- Preserve revision-7 exact-expiry authority: successful replay containment
  must not wipe the only BFF-held copy of an otherwise valid issued token.
- Preserve fail-closed internal-error behavior and canonical audit
  transactionality.
- Preserve the existing row lock, PostgreSQL clock authority, ordinary
  refusal rollback/relock path, fixed-cost API-key verification, tenant RLS,
  and caller-owned transaction boundaries.
- Preserve all prior TASK-003, R1, and R2 closures, both human directions,
  session secrecy, TLS, CSRF, provider, and tenant-binding behavior.
- Do not add a dependency, Cargo feature, public contract, migration,
  persistent state, second refresh/audit owner, retry framework, test harness,
  compatibility path, or unrelated refactor.
- Do not weaken, ignore, delete, or relabel an existing gate or assertion.

## Explicit non-goals

- No change to token lifetime, proactive-renewal margin, refresh JWT format,
  browser cookie contract, connection lifecycle, or provider behavior.
- No instantaneous revocation of an already-issued browser access snapshot
  after successfully contained refresh replay.
- No redesign of issuance, API-key verification, audit, session storage, or
  test-fixture ownership.
- No documentation cleanup outside the two validated test-support items.

## Acceptance criteria

| Criterion | Finding | Required observable result |
|---|---|---|
| R3-AC-01 | `FIND-TASK-003-14` | A proactively renewing OIDC browser session whose stored refresh token was already rotated commits the existing complete-family containment and canonical audit, while its unchanged access token remains usable only until stored expiry; first use at or after expiry ends the browser session. |
| R3-AC-02 | `FIND-TASK-003-14` | A refresh issuance/audit internal failure and an API-key internal failure both fail the request, commit no tentative credential or browser-row mutation, and can renew successfully after the injected failure is removed. |
| R3-AC-03 | `FIND-TASK-003-14` | Revoked API-key and inactive connection/principal/tenant outcomes retain the approved ordinary-refusal behavior before and after exact access expiry. |
| R3-AC-04 | `FIND-TASK-003-15` | `exchange_api_key::pg_tests` documents its shared fixture role and `insert_live_api_key` accurately documents its seed operation and hash/insert panic boundary. |

## Focused proof

Use Red-Green-Refactor for the runtime correction and record the expected RED
failure plus final GREEN result. Add these focused Postgres-backed tests to the
existing `browser_sessions::pg_tests` module:

- `proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry`
- `refresh_renewal_internal_failure_rolls_back_and_remains_retryable`
- `api_key_renewal_internal_failure_rolls_back_and_remains_retryable`

Run each through the repository-managed Postgres lifecycle with its exact
selector:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::refresh_renewal_internal_failure_rolls_back_and_remains_retryable)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::api_key_renewal_internal_failure_rolls_back_and_remains_retryable)'"
```

Retain and rerun the existing ordinary-refusal control:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=browser_sessions::pg_tests::proactive_renewal_refusal_preserves_authority_until_expiry)'"
```

The documentation correction needs direct source inspection and the normal
format/lint proof; add no runtime test for rustdoc alone.

## Broader verification

After focused proof is green, run the narrow complete affected-surface set:

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

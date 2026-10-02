---
id: TASK-003-R5
title: Retire the complete OIDC refresh family on browser logout
status: ready
approved_spec: changes/active/oidc-production-readiness/spec.md (revision 7)
original_task: changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md
base: 63c5bffc93cd2f7b5ed558e610a213efcc34fd49
candidate: 989d0734b0a9b04f314ef4b52aa7d8510f26fe11
finding_ids:
  - FIND-TASK-003-18
---

# TASK-003 R5: logout refresh-family retirement

## Authority and scope

Implement this bounded remediation against approved specification revision 7,
the original TASK-003 packet, the R2 remediation, the explicitly authorized R3
and R4 remediations, and both human directions. Reassess the cumulative
candidate from base `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`.

This task closes only `FIND-TASK-003-18`. It does not reopen the completed
renewal classification, issuer-binding, connection-test, UI, sealing inventory,
or classifier-visibility work.

## Issue diagnosis

### FIND-TASK-003-18 — OIDC logout can leave renewable User authority alive

TASK-003 requires browser logout to be atomic and mode-specific: an OIDC
session must revoke both its browser row and refresh family, while an API-key
session must wipe browser state without revoking the underlying operator key.

`BrowserSessions::logout` currently locks the browser row and starts the correct
tenant transaction, but it does not use the durable family identity on that
row. Instead it opens `refresh_token_sealed`, hashes the recovered token,
looks up that exact refresh row, and calls single-row `revoke_refresh` before
wiping the browser row.

That falls short in three reachable forms:

1. A browser refresh envelope can be unreadable by the serving keyring while
   the access and CSRF envelopes still allow the BFF to reach logout. The
   refresh branch is skipped, but the browser row is wiped and committed.
2. A copied refresh token can rotate through the ordinary public refresh owner
   before logout. The browser row still names the predecessor, so its
   single-row revocation changes nothing and the committed successor survives.
3. Logout takes no family advisory lock. A concurrent refresh can consume the
   predecessor and insert a successor outside logout's single-row update.

The browser endpoint then returns success and clears the cookie, but the live
successor can continue minting User authority. Wiping the browser row also
removes the current implementation's only recoverable token bytes, so the
missed retirement cannot be repaired through that path.

The live-row schema requires an OIDC refresh envelope, so a completely absent
envelope is not a separate production case. The defect is not envelope
validation; it is deriving family retirement from recoverable token bytes when
the locked row already contains the authoritative mode and principal id.

## Intended correction outcome

OIDC browser logout retires every active refresh token for the locked session's
User family and wipes the browser row in one caller-owned tenant transaction.
It succeeds even when the stored refresh envelope is unreadable, serializes
against refresh rotation through the existing family lock, remains idempotent,
and leaves API-key logout semantics unchanged.

## Decision-complete recommendation

Keep `BrowserSessions::logout` as the workflow owner. Reuse its existing tenant
resolution, `TenantConn`, browser-row lock, idempotent missing-row result, final
browser-row wipe, and commit boundary.

After locking the browser row, select behavior from `LockedBrowserSession.mode`:

- For `BrowserSessionMode::OidcRefresh`, use the locked row's
  `principal_id` and the existing refresh-family authority. Acquire
  `lock_refresh_family(&mut conn, "user", row.principal_id)`, then call
  `revoke_refresh_family(&mut conn, "user", row.principal_id,
  "browser_logout")` before wiping the browser row.
- For `BrowserSessionMode::ApiKeyExchange`, perform no refresh-family action
  and keep the browser-only wipe, preserving the underlying operator API key.

Remove logout's dependency on opening the refresh envelope, hashing the exact
token, `refresh_by_hash`, and `revoke_refresh`. The family lock is already the
shared serialization owner used by issuance, rotation, replay containment, and
administrative User revocation. Store or lock failure must return before commit
so the tenant transaction rolls back both sides. A retry after a successful
logout remains a no-op because the browser row is already revoked.

This is the smallest root-cause correction. Do not add a retry marker, new
table or migration, new lock abstraction, new public contract, alternate audit
path, or a second logout owner.

## Constraints and preserved behavior

- Preserve tenant resolution, forced RLS, and caller-owned `TenantConn`
  transaction lifecycle.
- Preserve the browser-row lock and the existing browser-row wipe.
- Preserve idempotence for unknown, expired, and already-revoked sessions.
- Preserve API-key logout: only browser state is removed; the operator key
  remains usable.
- Preserve current refresh issuance, rotation, replay containment, audit,
  connection cutoff, sealing inventory, and renewal classification behavior.
- Reuse the existing `"user"` family kind and `"browser_logout"` reason.
- Do not introduce another family identifier, derive authority from browser
  input, or move durable behavior into the SvelteKit BFF.
- Keep the R4 classifier visibility and documentation corrections intact.

## Explicit non-goals

- No change to cookie handling, BFF response shape, access-token lifetime, or
  browser-visible data.
- No change to API-key issuance, revocation, or fixed-cost verification.
- No new provider, OIDC callback, connection-test, role-mapping, or tenant
  selection behavior.
- No new migration, dependency, feature, test harness, compatibility route, or
  public API.
- No speculative recovery framework or separate concurrent stress harness.

## Acceptance criteria

| Criterion | Finding | Required result |
|---|---|---|
| R5-AC-01 | `FIND-TASK-003-18` | OIDC logout uses the locked browser row's mode and principal id, acquires the existing User refresh-family lock, revokes the whole family, wipes the browser row, and commits those effects atomically. |
| R5-AC-02 | `FIND-TASK-003-18` | Logout no longer opens or hashes the stored refresh token and no longer uses single-row `revoke_refresh`; an unreadable refresh envelope cannot skip family retirement. |
| R5-AC-03 | `FIND-TASK-003-18` | A refresh successor committed before logout is revoked and cannot rotate again; every active row in that User family is retired. |
| R5-AC-04 | `FIND-TASK-003-18` | API-key logout still wipes only the browser session and the underlying operator API key remains valid. |
| R5-AC-05 | `FIND-TASK-003-18` | Unknown/already-ended logout remains idempotent, and any family-lock, family-revocation, browser-wipe, or commit failure cannot leave a committed half-logout. |
| R5-AC-06 | regression boundary | Prior TASK-003 behavior, both human directions, and `FIND-TASK-003-1` through `FIND-TASK-003-17` remain closed. |

## Focused proof

Add one Postgres-backed test in the existing `wyrd-auth` browser-session test
module:

1. Create an OIDC browser session.
2. Rotate the refresh token through the ordinary `RefreshTokens` owner and
   commit its successor without updating the browser row.
3. Replace only the browser row's stored refresh envelope with ciphertext the
   serving keyring cannot open.
4. Call browser logout.
5. Assert that the browser row is revoked and its sealed fields are wiped, all
   active `("user", principal_id)` refresh rows are revoked, and the committed
   successor cannot rotate.

This single case proves both independence from token bytes and family-wide
retirement. Do not add a new concurrency harness: the committed-successor case
directly fails the current single-row implementation, while reuse of the
existing advisory family lock supplies the already-established serialization
boundary.

Retain or extend the existing API-key logout proof to show that the underlying
operator key can still exchange after browser logout. Keep existing logout,
renewal-refusal, replay-containment, internal-failure, sealing-rotation, and
identity journeys green.

Run the exact focused selector through `mise exec -- cargo nextest run` after
naming it, then the narrowest owning repository tasks for `wyrd-auth`, SQL
state, tenant isolation, and the identity journey. Finish with `mise run fmt`
and `mise run lints`; use broader verification only if the implementation
actually crosses the existing ownership boundaries.

## Implementation Evidence

This round follows `human-direction-FIND-TASK-003-18.md`: logout revokes only this session's refresh chain. That direction replaces the family-wide correction and proof above. Commit: `e776c151d`.

| Acceptance criterion (as directed) | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| OIDC logout takes `lock_refresh_family("user", principal)` and revokes this session's chain (the starting token plus every `rotated_from` descendant), never the whole User family, in the same transaction as the browser-row wipe | `BrowserSessions::logout` with the new `revoke_refresh_chain` (recursive CTE in `wyrd-sql` `refresh_tokens.rs`); `revoke_refresh_family` is not called | `browser_sessions::pg_tests::oidc_logout_retires_only_its_refresh_chain_without_opening_it` (RED against the old logout: `logout revokes the session chain's committed successor`; GREEN) | PASS |
| Logout does not decrypt the refresh envelope | New non-secret column `refresh_chain_id` (migration `20261001000003`, CHECK that it is set exactly in OIDC mode, composite FK to `auth_refresh_tokens`). It is set at `complete` from the login refresh row; logout reads it from the locked row | Same test: the refresh envelope is replaced with ciphertext under an unheld key before logout | PASS |
| A successor committed before logout is revoked and cannot rotate; a separate login of the same User stays active | | Same test: the successor's `revoked_at` is set and its rotation returns `RefreshError::Reused`; the other login's refresh row stays `revoked_at IS NULL` | PASS |
| API-key logout unchanged | `refresh_chain_id` is `None` for API-key sessions, so they only get the browser-row wipe | `browser_session_sealing_rotation_journey` (API-key logout) in `test:identity:journey` | PASS |
| Idempotence and atomicity | Unchanged missing-row no-op. Any lock, revocation or wipe error returns before commit | Source inspection; `test:wyrd` | PASS |

Non-goals stayed excluded: the only new state is the permitted chain id, and there is no new table, API, abstraction, audit owner, cookie change or BFF change.

Verification, all run with `CARGO_TARGET_DIR` set to the shared target:
- The focused selector, plus every `browser_sessions::` test (8 passed).
- Mise lanes: `fmt`, `lints`, `codegen:check`, `check:tenant-isolation`, `test:sql`, `test:wyrd` and `test:identity:journey`.
- `git diff --check`.

All exited 0 in this round. After the harness fix below, `test:wyrd` ran 2348 tests and all passed. `fmt`, `lints`, `test:identity:journey` and `git diff --check` were re-run after the fix.

### Diagnosis: SIGABRT in `test:wyrd`

- **Symptom.** In the first two `test:wyrd` runs, `wyrd-server::pg_card_registration_route` tests aborted with SIGABRT. The affected tests were `card_reads_list_latest_and_delete_are_tenant_safe`, `completion_audit_failure_keeps_card_pending` and `composite_registration_returns_leaf_first_outcomes_and_root`.
- **Evidence.** The trace runs in this order:
  1. `terminating connection due to administrator command`, which is the fixture's `DROP DATABASE ... WITH (FORCE)`.
  2. `Oracle self-fenced its reader epoch after a failed renewal` (pool timed out).
  3. `Oracle reader epoch retirement exhausted its shutdown deadline`.
  4. The process aborts in `AbortingEpochTerminator` (`reader_pins.rs`).
- **Cause.** An in-process `WyrdTestServer` has neither a shutdown token nor a serve task. As a result, `shutdown()` cancelled nothing and drained nothing, and Bifrost's role tasks outlived the dropped test database.
- **Fix site.** `WyrdTestServer::shutdown` in `crates/wyrd/wyrd-testing/src/server.rs`, which is the shared owner for every `start_in_process` caller. For an in-process server it now cancels `AppState::shutdown_token` and awaits `bifrost.shutdown(deadline)` before the fixture drops. This follows the boot `rollback_state_roles` pattern; a failed drain already falls back to Bifrost's abort. A read-only diagnostician confirmed the cause and the fix site.

# TASK-003 R5 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority:
  `TASK-003-r2/TASK-003-R2-production-ui-remediation.md`,
  `TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and
  `TASK-003-r4/TASK-003-R4-renewal-contract-boundaries.md`
- Human directions:
  `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

The human owner explicitly authorized R3 and R4. The candidate resolved to the
stated commit before validation and again after this report was written. The
repository has no `.codegraph/` directory, so validation used the complete Git
range, repository search, and direct source inspection. All required R5
discovery reports and `followup-review.md` were present and read.

## Proposal validation

| Proposal | Result | Validated disposition |
|---|---|---|
| `INV-R5-001` | **REVISED** | The missing-envelope branch is schema-unreachable for a live OIDC row: the browser-session constraint requires a non-null refresh envelope for `oidc_refresh`. An envelope unreadable by the serving keyring is reachable, and current logout silently wipes it without retiring the family. The proposed fail-and-retry correction is unnecessarily dependent on token bytes; the locked row already carries the family identity. Retain the reachable failure under `FIND-TASK-003-18`, with the shared correction below. |
| `PC-R5-001` | **CONFIRMED** | Logout revokes only the exact stored refresh row and takes no family lock. A public refresh can already have rotated that row, or can rotate concurrently, leaving a successor live after logout commits. Retain under `FIND-TASK-003-18`. |
| `FU-R5-001` | **CONFIRMED** | Both failures have one source: logout derives revocation from refresh-token bytes rather than `LockedBrowserSession.mode` and `principal_id`. Reusing `lock_refresh_family` and `revoke_refresh_family` inside the existing logout transaction is the smallest safe correction and requires no new contract or durable decision. |

The behavior, standards, maintainer, system-resilience, and security-identity
reports proposed no other material finding. Source inspection found no missing
caller, sibling writer, or conflicting authority that requires another
finding.

## Producer-to-consumer trace

1. `TenantTokenIssuer::issue_human_session` creates only `"user"` refresh
   families and takes `lock_refresh_family("user", principal_id)` before
   inserting the initial or successor row. `RefreshTokens::execute` looks up
   the presented row, takes that same family lock, consumes the predecessor,
   and inserts the successor in the caller-owned tenant transaction.
2. `BrowserSessions::complete` verifies the access token produced beside the
   refresh token, stores its User id as `BrowserSessionWrite.principal_id`, and
   stores `mode = OidcRefresh`. The table constraint requires every live OIDC
   row to carry a refresh envelope and every live API-key row to carry only its
   API key. `lock_browser_session` returns both durable fields.
3. The public refresh route does not lock the browser row. A copied current
   token can therefore rotate and commit while the browser row still stores
   its predecessor. This is a required, reachable path already exercised by
   the replay-containment test.
4. `BrowserSessions::logout` locks the browser row, but then opens the stored
   envelope, looks up that exact token row, and calls `revoke_refresh`. If the
   envelope is unreadable, this entire branch is skipped. If the row was
   already rotated, the single-row update affects nothing. Without the family
   advisory lock, a concurrent rotation can also insert a successor outside
   the update.
5. Logout then unconditionally calls `revoke_browser_session`, wiping every
   envelope, and commits. The private BFF returns `204`; `ServerSessions.logout`
   clears the cookie. No browser credential remains from which the current
   implementation can retry family retirement.
6. The sibling family writers and consumers use one existing authority:
   `lock_refresh_family` plus `revoke_refresh_family`. `RefreshTokens` uses it
   for replay containment, `TenantTokenIssuer` uses the lock for first issue
   and rotation, and User revocation uses both for administrative retirement.
   Repository-wide search found no path that takes the family lock and then
   waits for a browser-row lock, so the required browser-row-then-family order
   adds no opposing lock edge.

The existing sealing-rotation logout journey opens and revokes only the token
currently stored in the browser row. It does not create a committed successor,
overlap refresh with logout, or make only the stored refresh envelope
unreadable, so its green result cannot prove the required family outcome.

## Ponytail correction analysis

The finding cannot be deleted: the original task explicitly requires OIDC
logout to revoke the browser session and its refresh family atomically and
idempotently, while preserving an API-key session's underlying operator key.
The repository already has the needed mechanism, so no parser, recovery
marker, retry framework, audit path, migration, public API, dependency, or new
abstraction is justified.

The correction belongs in `BrowserSessions::logout`, not in a downstream
consumer. That method owns the mode-specific logout invariant, already holds
the browser-row lock and tenant transaction, and is the only place that must
couple refresh-family retirement with browser-row wiping. The minimum safe
correction is:

1. Keep the existing session lookup, `TenantConn`, browser-row lock, idempotent
   missing-row result, and final browser-row wipe.
2. Branch on the locked `row.mode`.
3. For `BrowserSessionMode::OidcRefresh`, call
   `lock_refresh_family(&mut conn, "user", row.principal_id)` and then
   `revoke_refresh_family(&mut conn, "user", row.principal_id,
   "browser_logout")` before wiping the browser row.
4. For `BrowserSessionMode::ApiKeyExchange`, perform no refresh-family action
   and retain the browser-only wipe, so the operator API key remains valid.
5. Remove logout's token-opening, token-hash lookup, and single-row-revocation
   path. Store or lock failure must return before commit, allowing the tenant
   transaction to roll back both sides. A retry after an uncertain successful
   response remains idempotent because the browser row is already revoked.

This uses the locked row's durable identity rather than recoverable bytes. It
closes the unreadable-envelope and predecessor/successor paths together without
weakening sealing, tenant isolation, replay containment, audit ownership,
connection cutoff, API-key semantics, or browser-cookie behavior.

## Validated finding ledger

### FIND-TASK-003-18 — OIDC logout does not retire the locked session's refresh family

- **Discovery sources:** `INV-R5-001`, `PC-R5-001`, `FU-R5-001`
- **Status:** **REVISED** — the two discovery failures are consolidated at
  their shared source; the schema-unreachable missing-envelope branch and the
  token-open retry recommendation are excluded.
- **Classification:** `INCORRECT`
- **Violated obligation:** TASK-003's packet-local browser session contract
  requires logout to revoke the OIDC browser session and its refresh family in
  one idempotent transaction, while API-key logout must not revoke the
  underlying operator key.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:404-434`, especially the
  envelope-dependent single-row path at lines 421-429;
  existing family authority at
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:96-120,173-201`.
- **Evidence:** `LockedBrowserSession` supplies `mode` and `principal_id`, but
  logout ignores them. It opens the stored refresh envelope, resolves only
  that row, and calls `revoke_refresh`, which updates one active id and takes
  no family lock. A previously rotated predecessor updates zero rows; a
  concurrent rotation can commit a successor; an unreadable envelope skips
  revocation entirely. Logout nevertheless wipes and commits the browser row.
- **Observable consequence:** the UI reports logout complete and loses its
  browser session, while an already-issued or concurrently created successor
  refresh token can continue minting User authority. An unreadable stored
  envelope produces the same outcome and is destroyed, preventing later
  repair from recovering the current implementation's lookup path.
- **Decision-complete correction:** in the existing
  `BrowserSessions::logout` transaction, use the locked mode and principal id.
  For OIDC mode, reuse `lock_refresh_family` and
  `revoke_refresh_family("user", row.principal_id, "browser_logout")`, then
  wipe the browser row and commit. For API-key mode, skip family work and wipe
  only the browser row. Remove logout's dependency on the refresh envelope,
  exact token hash, and `revoke_refresh`; add no new state, API, abstraction,
  or audit owner.
- **Focused closure proof:** add one Postgres-backed test in the existing
  browser-session test module. Create an OIDC browser session; rotate its
  stored refresh token through `RefreshTokens` and commit the successor without
  updating the browser row; replace only the browser row's refresh envelope
  with ciphertext the serving keyring cannot open; call logout; then prove the
  browser row is revoked and wiped, every active `("user", principal_id)`
  refresh row is revoked, and the successor cannot rotate. Retain the existing
  API-key logout proof that the underlying key still exchanges. A separate
  concurrency harness is unnecessary: the committed-successor case directly
  proves family-wide retirement, and the reused family advisory lock supplies
  the existing serialization boundary.

## Prior-finding closure

| Prior finding | Validated current-source result |
|---|---|
| `FIND-TASK-003-1` | **CLOSED under the approved human direction.** Optional typed callback `iss` and conditional exact comparison remain in the common exchange owner. |
| `FIND-TASK-003-2` | **CLOSED.** Browser API-key refusal retains the shared fixed-cost verification path. |
| `FIND-TASK-003-3` | **CLOSED.** Cookie suffixes remain lookup hints resolved through server-owned session reads. |
| `FIND-TASK-003-4` | **CLOSED.** Canonical sealing inventory includes all stored browser envelopes and keeps exact-byte CAS. |
| `FIND-TASK-003-5` | **CLOSED.** The production-built BFF trusted-TLS journey remains present. |
| `FIND-TASK-003-6` | **CLOSED.** Keycloak/Dex switching and mixed-provider callback refusal remain in the real browser journey. |
| `FIND-TASK-003-7` | **CLOSED.** The previously cited Rust items remain substantively documented. |
| `FIND-TASK-003-8` | **CLOSED.** Authoritative tenant identity remains server-returned and server-only. |
| `FIND-TASK-003-9` | **CLOSED.** The unused lifetime variant and SQL branch remain absent. |
| `FIND-TASK-003-10` | **CLOSED.** Ordinary renewal refusal preserves issued authority only to exact stored expiry. |
| `FIND-TASK-003-11` | **CLOSED.** Discovery parsing and field documentation remain accurate. |
| `FIND-TASK-003-12` | **CLOSED.** Request-derived chooser reads remain sequentially bounded. |
| `FIND-TASK-003-13` | **CLOSED.** Explicit empty or unsafe upstream configuration is refused. |
| `FIND-TASK-003-14` | **CLOSED at the renewal owner.** Replay containment, ordinary refusal, and internal failure retain distinct transaction outcomes. `FIND-TASK-003-18` is the separate logout consumer's failure to reuse the same family authority. |
| `FIND-TASK-003-15` | **CLOSED.** Shared API-key test support retains accurate rustdoc. |
| `FIND-TASK-003-16` | **CLOSED.** All three renewal classifiers are crate-private. |
| `FIND-TASK-003-17` | **CLOSED at its approved boundary.** Missing or unopenable renewal envelopes map directly to retryable internal failure and the local contracts agree. `FIND-TASK-003-18` follows the reachable unreadable state into logout and replaces token-byte dependence there. |

Both human directions remain independently satisfied and are not assigned new
`FIND-*` identities.

## Validation result

**FIX_REQUIRED**

The deduplicated ledger contains one bounded implementation finding:
`FIND-TASK-003-18`. Its correction reuses the existing durable family identity,
lock, bulk revocation, tenant transaction, and mode boundary. It requires no
specification revision or expensive-to-reverse decision.

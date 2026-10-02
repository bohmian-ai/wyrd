# TASK-003 R5 focused follow-up review

## Immutable subject and uncertainty

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2 plus the explicitly human-authorized R3
  and R4 tasks
- Human directions: conditional response-issuer binding and real interactive
  connection testing

The repository has no `.codegraph/` directory, so navigation used the complete
immutable Git range, `rg`, and direct source inspection. The candidate resolved
to the stated object before inspection and again after this report was written.

This pass investigated only the conflict between:

- `INV-R5-001`: logout silently wipes an OIDC browser row when its stored
  refresh envelope cannot be opened, leaving renewable authority alive and no
  row from which to retry; and
- `PC-R5-001`: logout revokes only the stored refresh row rather than its
  family, so a committed or concurrent successor can survive.

The other discovery reports' empty ledgers were treated as claims to check, not
as evidence against either proposal.

## Source and authority inspected

- `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`,
  especially lines 81-104 and 118-126: logout must revoke the browser session
  and its refresh family, remain idempotent, and leave an API-key session's
  underlying operator key valid.
- Revision-7 `REQ-005`, `REQ-009`, `REQ-016`, and `REQ-017`, plus the R2-R4
  remediation constraints and both human directions.
- All seven R5 discovery reports.
- `crates/wyrd/wyrd-auth/src/browser_sessions.rs`: full bodies and surrounding
  contracts for `complete`, `exchange_api_key`, `read`, `authority`, `logout`,
  `current`, `renew`, `open_text`, and `open_credential`, plus their unit and
  Postgres tests.
- `crates/wyrd/wyrd-server/src/components/auth/bff.rs`: BFF service-key
  middleware and the complete logout handler.
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts` and
  its session/integration tests: read-before-logout, CSRF validation, response
  handling, and cookie deletion.
- `crates/wyrd/wyrd-sql/migrations/20261001000001_auth_browser_sessions.sql`
  and `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs`: live-row mode
  constraints, `LockedBrowserSession`, row locking, rotation, and wiping.
- `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs`: complete bodies
  and callers of `refresh_by_hash`, `revoke_refresh`, `lock_refresh_family`,
  and `revoke_refresh_family`.
- `crates/wyrd/wyrd-auth/src/refresh.rs`, `issuance.rs`, `callback.rs`, and
  `revoke.rs`, plus the server callback concurrency proof: refresh rotation,
  replay containment, issuance, callback role synchronization, administrative
  revocation, lock order, audit ownership, and transaction ownership.
- `crates/wyrd/wyrd-auth/src/sealing.rs` and
  `crates/wyrd/wyrd-server/src/boot/mod.rs`: unopenable-envelope inventory,
  rewrap recovery, and keyless boot behavior.
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs`: the existing sealing-rotation
  logout proof, which presents only the token currently stored in the browser
  row and does not create a successor before logout.

Repository-wide caller search found one production caller of `revoke_refresh`
(`BrowserSessions::logout`), two production callers of
`revoke_refresh_family` (`RefreshTokens::execute` and User revocation), and the
family-lock users in refresh, issuance, callback synchronization, User
revocation, and their concurrency tests.

## Claim A: missing or unopenable refresh envelope

### Resolution

`INV-R5-001` is **REVISED**, not rejected.

The “missing envelope” half is not reachable through ordinary production
writes for a live OIDC row. The table constraint requires
`refresh_token_sealed IS NOT NULL` exactly when `mode = 'oidc_refresh'`, and
`lock_browser_session` reads only live rows. It remains useful corrupt-state
input for renewal's pure producer test, but it is not independently a reachable
logout defect under the approved schema.

The unopenable-envelope half is reachable and required. Rewrap inventories
each browser envelope independently, counts an envelope that no held key opens
as `remaining`, and with a configured keyring permits serving so the operator
can repair or rerun rotation. A partially rewrapped or corrupted row can
therefore have readable access/CSRF envelopes but an unopenable refresh
envelope. `ServerSessions::logout` first reads the session and can reach the
private logout call in that state. At `browser_sessions.rs:421-430`, logout
silently ignores `open_text` failure. It then wipes the browser row and commits
at lines 431-434, so the BFF receives `204` and clears the cookie. The live
refresh family remains, and the only stored recoverable browser credential is
destroyed.

That result violates the task's explicit coupled logout outcome. R4 made
unopenable *renewal* credentials retryable and did not authorize a sibling
logout consumer to report success without family retirement. The discovery
proposal's fail-and-retry correction would close this path, but it is not the
smallest correction because logout does not need to open the refresh envelope
to identify the family.

## Claim B: single-row revocation misses successors

### Resolution

`PC-R5-001` is **CONFIRMED**.

The browser row and initial/rotated refresh rows name the same tenant User.
Browser-owned renewal keeps refresh rotation and browser-row replacement in one
transaction, so an ordinary BFF renewal cannot commit a stale browser row.
However, the public refresh owner can consume a copied current refresh token
without taking the browser-row lock. This is a realistic credential-replay
path covered by the approved family-containment model.

If that rotation commits before logout, the browser row still holds the
predecessor. Logout's `refresh_by_hash` finds the rotated row, then
`revoke_refresh(... WHERE revoked_at IS NULL)` updates nothing. Logout still
wipes and commits the browser row, while the successor remains active.

The concurrent form is also reachable. Refresh takes the principal-family
advisory lock, consumes the predecessor, and inserts its successor. Logout
takes no family lock. Its single-row update can wait behind the predecessor
write, observe that the predecessor is already revoked after the rotation
commits, update zero rows, and then commit the browser wipe. The successor is
outside that update and survives. The existing browser-row `FOR UPDATE` does
not serialize a refresh-route caller that never reads the browser row.

The green sealing-rotation logout journey proves only that the exact currently
stored refresh row becomes unusable. It neither rotates a successor before
logout nor overlaps logout with refresh rotation, so it cannot close this gap.

## Shared source and correction boundary

The two proposals are one defect, not competing fixes.

### FU-R5-001 — Logout derives family retirement from recoverable token bytes instead of the locked session's family identity

- **Classification:** `INCORRECT`
- **Violated obligation:** TASK-003 lines 90-103 require OIDC logout to revoke
  the browser session and its refresh family atomically and idempotently, while
  API-key logout must not revoke the underlying operator key.
- **Source:** `BrowserSessions::logout` conditions retirement on opening the
  stored refresh envelope, looks up that one token row, and calls the
  single-row `revoke_refresh` without the existing family lock.
- **Reachable consequences:** an unopenable refresh envelope is silently wiped
  with no family retirement; a previously or concurrently rotated successor
  remains renewable after logout reports success.
- **Shared root:** logout uses the refresh token's current bytes as the source
  of family identity. Those bytes are neither required nor sufficient: they
  may be unreadable, and they may name a stale ancestor.

The locked `LockedBrowserSession` already carries the authoritative
`BrowserSessionMode` and `principal_id`. `complete` obtains that principal from
the verified Wyrd access token issued beside the refresh token, the live-row
constraint makes OIDC mode explicit, and every human refresh operation uses
the existing family key `("user", principal_id)` in the same tenant. The
smallest safe correction is therefore inside the existing
`BrowserSessions::logout` transaction:

1. keep the current browser-row lock and mode split;
2. for `OidcRefresh`, use `row.principal_id` with the existing
   `lock_refresh_family` and `revoke_refresh_family(..., "browser_logout")`;
3. then revoke/wipe the browser row and commit the same caller-owned
   `TenantConn` transaction;
4. for `ApiKeyExchange`, skip refresh-family work and retain the current
   browser-only wipe.

This reuses the durable family authority already used by rotation and User
revocation. It adds no marker, alternate audit path, token parser, retry
framework, or public contract. It also removes logout's need for a keyring,
`open_text`, `refresh_by_hash`, or the exact stored token row, so it closes both
reachable failures rather than choosing between them.

The lock order is compatible with the current browser-renewal path: browser
row, then refresh family, then any issuance connection lock. No inspected
production path takes the refresh-family lock and later waits on a browser row,
so this correction does not introduce the opposing edge needed for a deadlock
cycle. A store or lock failure occurs before the browser wipe commits; the
transaction rolls back, the BFF receives a non-`204`, and
`ServerSessions::logout` does not reach its cookie deletion, preserving retry.
An uncertain response after commit remains idempotent because the next tenant
lookup sees no live browser row and returns success.

Logout is not refresh-replay classification. The canonical replay audit remains
owned by `RefreshTokens`; the existing administrative family revocation likewise
uses `lock_refresh_family` plus `revoke_refresh_family` without manufacturing a
replay event. No additional audit owner is required by this correction.

## Focused closure proof

One Postgres-backed logout test can cover the shared source without a new
harness:

1. create an OIDC browser session and rotate its stored refresh token through
   the ordinary refresh owner, committing a successor without updating the
   browser row;
2. make the browser row's refresh envelope unopenable while leaving its
   access/CSRF envelopes readable, then call logout;
3. assert logout succeeds, the browser row is revoked and wiped, every active
   row for `("user", principal_id)` is revoked, and the successor cannot
   rotate; and
4. retain the existing API-key logout assertion that the underlying API key
   still exchanges.

The already-committed-successor case directly fails the candidate and proves
the family-wide boundary. A deterministic overlap case is warranted only if it
can reuse the existing advisory-lock coordination cheaply; the family lock and
the committed-successor proof establish the same missing serialization owner.

## Follow-up result

**RESOLVED**

`INV-R5-001` and `PC-R5-001` share one source and one correction boundary.
`PC-R5-001` is confirmed; `INV-R5-001` is narrowed to the reachable unopenable
envelope case and its fail-and-retry recommendation is revised. The unified
local proposal is `FU-R5-001`: OIDC logout must retire the locked browser
session's existing principal refresh family under the existing family lock,
independent of the stored refresh envelope, then wipe the browser row in the
same transaction. This correction does not conflict with retryability or
integrity; it removes the envelope failure from logout while preserving
transaction rollback on real store failures and the API-key mode boundary.

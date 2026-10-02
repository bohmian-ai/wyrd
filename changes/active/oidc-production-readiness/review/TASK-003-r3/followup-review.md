# TASK-003 R3 focused follow-up review

## Subject and uncertainty

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Authority: `SPEC-oidc-production-readiness`, revision 7; original
  `TASK-003-production-ui.md`; R2 remediation; both supplied human directions
- Dispute: whether browser refresh replay should preserve current access until
  expiry or revoke the browser row immediately, how renewal failures divide
  between refusal and failure, and whether a dependency failure can commit
  tentative writes without the required successful issuance audit.

The candidate resolved to the stated commit before and after this pass. The
repository has no `.codegraph/` directory. Per assignment, I ran no tests or
builds.

## Source paths inspected

- `changes/active/oidc-production-readiness/spec.md:82-97,99-140,188-202,235-262`
- `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md:35-103`
- `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
- `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`
- `architecture/wyrd-security-posture.md:141-159`
- `crates/wyrd/wyrd-auth/src/browser_sessions.rs:320-390,435-587,665-671,809-960`
- `crates/wyrd/wyrd-auth/src/refresh.rs:35-82,84-227`
- `crates/wyrd/wyrd-auth/src/issuance.rs:239-337,369-497,615-700`
- `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:53-97,154-233`
- `crates/wyrd/wyrd-auth/src/audit.rs:117-194`
- `crates/wyrd/wyrd-server/src/components/auth/routes.rs:224-263`
- `crates/wyrd/wyrd-server/src/components/auth/bff.rs:253-327`
- `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:45-92,158-185,250-331`
- `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:14-25,74-120`
- `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:411-429`
- `crates/wyrd/wyrd-sql/migrations/20261001000001_auth_browser_sessions.sql:19-53`
- `crates/vala/vala-sql/src/queries/audit_staging.rs:18-80`
- the three conflicting R3 discovery reports named by the orchestrator

## Resolution

### 1. Exact reachable outcome taxonomy

`BrowserSessions::current` has two production consumers: `read` and
`authority`; both commit the `TenantConn` returned by `current`. The private BFF
handlers only project those results. The renewal owner therefore must preserve
the transaction meaning of each underlying result rather than collapse all
non-database errors into `Renewal::Refused`.

| Source outcome | Writes before return | Required browser behavior before stored access expiry | Required behavior at/after stored access expiry |
|---|---|---|---|
| Refresh `NotFound`; inactive tenant, principal, or exact connection | `consume_active_refresh` may have tentatively retired an active row before issuance discovers the lifecycle refusal | Roll back the entire attempt, relock, and serve only the existing token | End the browser session; committing retirement with the terminal session decision is allowed |
| API-key malformed/missing/cross-tenant/wrong secret/revoked/expired; inactive tenant or principal | Verification refusals precede writes except an issuance-time inactive result, which follows tentative `last_used_at` | Roll back the entire attempt, relock, and serve only the existing token | End the browser session; no alternate credential is tried |
| Refresh `Reused` | Under the family lock, revokes the complete family and appends the canonical denied containment audit | **Commit containment and its audit**, keep the browser row live, relock, and serve only the existing access token | Revoke/wipe the browser row in the same containment transaction and refuse |
| Database/`SqlError`, audit, signing, role-decode/corrupt-state, keyring-envelope open, blocking verification task, or other internal issuance failure | Depending on where it occurs, may follow tentative refresh consumption, API-key `last_used_at`, or a staged issuance audit | Return failure and let the transaction roll back; do not serve authority | Return failure and roll back; preserve the browser row and recoverable credential for retry rather than convert an outage/corruption into credential revocation |

The current classification is materially wider than the permitted refusal
sets. On refresh it treats only direct `RefreshError::Database` and
`IssuanceError::Database` as `Failed`; `IssuanceError::Store`, `RoleCorrupt`,
`Issue`, `Wyrd(AuditUnavailable)`, direct `RefreshError::Wyrd`, sealed-envelope
failures, and replay all become `Refused`. On API-key renewal only direct
`ExchangeError::Database` becomes `Failed`; `Join`, non-semantic
`IssuanceError` variants, and sealed-envelope failures become `Refused`.

### 2. Replay containment and current access authority

`RefreshTokens::execute` establishes the replay semantics. A stored but stale
token causes family-wide revocation and the `auth.refresh.family.revoke` audit
on the caller transaction, then returns `RefreshError::Reused`
(`refresh.rs:128-173`). The public token route explicitly commits that one
error before returning `401` (`routes.rs:236-259`). Rolling it back in
`BrowserSessions::current` is therefore incorrect and leaves a concurrently
minted attacker-held successor renewable.

Immediate browser-row revocation is also incorrect. Revision 7 REQ-016 says an
already-issued access token retains its bounded snapshot authority until its
exact expiry unless the principal or tenant is independently blocked by an
existing stronger guard. The task's packet contract says a failed, revoked, or
old-connection refresh ends the browser session at access expiry, and R2-AC-04
requires the current token to remain usable until that stored expiry. Refresh
family containment invalidates renewable authority; it neither invalidates the
already-signed access JWT nor independently blocks the principal or tenant.
`revoke_browser_session` would wipe the only BFF-held copy of that valid access
token and CSRF state (`browser_sessions` SQL lines 82-92), ending access early.

Thus BEH-R3-001 has the correct authority outcome and PC-R3-001's proposed
immediate browser-row revocation must be rejected. The safe boundary is a
distinct replay-contained renewal outcome: while unexpired, commit the existing
family revocation/audit transaction, reopen and lock the unchanged browser row,
and return its current access token; when expired, revoke the browser row before
committing the same transaction and refuse. No second family revocation or
audit implementation belongs in `BrowserSessions`.

The existing schema has no non-renewable-live-session marker: its live
`oidc_refresh` constraint requires a refresh envelope. Consequently, repeated
pre-expiry BFF requests can present the same stale stored refresh token and
produce another zero-row containment audit. That is bounded by the remaining
access lifetime and is safer than either rolling back theft containment or
adding unapproved persistent state. A closure proof should assert the family is
durably contained and at least the triggering canonical audit exists, not
require a new persistence model solely to deduplicate later presentations.

### 3. Infrastructure and corrupt-state failures

PC-R3-001 and SYSTEM-R3-001 are correct that the current two-variant `Renewal`
classification can serve authority during an infrastructure failure before
expiry and can turn a non-database failure into permanent logout after expiry.
Examples with a still-valid SQL transaction include token-signing failure,
role decoding/corrupt permission state, an `SqlError` invariant failure not
caused by an aborting statement, and API-key verification-task failure. On the
refresh path, signing or role failure occurs after tentative refresh
consumption; on the API-key path, issuance failure occurs after tentative
`last_used_at`. The current expired-token branch then adds browser revocation
and commits all valid pending writes despite no successful issuance.

The reports overstate one narrower case: an ordinary PostgreSQL failure while
appending the canonical audit aborts the transaction, so the subsequent browser
revocation statement or commit cannot succeed. That specific SQL-audit outage
does not durably commit unaudited tentative writes. It is still misclassified:
the pre-expiry branch rolls the failed transaction back and then serves the old
token, contrary to R2's explicit “infrastructure failures remain fail closed.”
Non-aborting signing/corrupt-state/internal failures retain the destructive
commit path described above. The shared defect is semantic information loss,
not every individual failure's transaction-abort mechanics.

### 4. Shared root cause and smallest safe correction boundary

The shared root cause is `BrowserSessions::renew` reducing rich refresh,
issuance, API-key, envelope, and replay outcomes to only `Refused` or `Failed`
(`browser_sessions.rs:547-585`). `current` cannot then choose among rollback and
serve, commit containment and serve, terminal credential refusal, or rollback
and propagate.

The smallest safe correction stays entirely in the existing
`BrowserSessions` orchestration boundary:

1. Preserve a distinct replay-contained result from `RefreshError::Reused`.
2. Admit only the explicit credential/lifecycle variants listed above to the
   ordinary refusal result.
3. Map every store, audit, signing, role/corrupt-state, envelope-open, join, and
   other internal outcome to failure, preserving its `WyrdError` and rollback.
4. In `current`, commit replay containment while preserving current access
   until expiry; revoke the browser row only once that access is expired.
5. Keep the existing rollback/relock path for ordinary early refusal and the
   existing terminal session behavior for ordinary refusal after expiry.

No new owner, audit path, refresh implementation, dependency, or public
contract is needed. Focused proof must cover: OIDC replay during the proactive
window (family and audit commit while current authority remains usable), the
same path after expiry (browser row ends), one refresh issuance/audit failure,
and one API-key internal failure (request fails and all tentative state rolls
back). The existing revoked-API-key proof remains the ordinary-refusal control.

## Findings produced by this follow-up

No new independent finding is needed. The evidence resolves BEH-R3-001,
PC-R3-001, and SYSTEM-R3-001 to one shared regression: lossy renewal-outcome
classification. Retain the replay-containment portion, revise the immediate
browser-revocation recommendation to exact-expiry preservation, and narrow the
system claim so SQL audit failures are not said to commit an aborted
transaction. Retain the broader fail-closed and non-database destructive-commit
problem.

## Result

**RESOLVED.** Replay containment and its audit must commit without revoking the
browser row before the current access token's exact expiry. Ordinary
credential/lifecycle refusals may use rollback-and-serve until expiry.
Infrastructure and corrupt-state outcomes must roll back and propagate at all
times. The one correction boundary is `BrowserSessions` renewal-outcome
classification and `current` transaction handling.

# Persistence, concurrency, tenancy, and durability domain review

## Subject and reviewed boundary

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `SPEC-oidc-production-readiness` revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2 and the explicitly authorized TASK-003 R3
- Human directions: issuer binding and real interactive connection testing
- Domain: browser-session and connection-test persistence, tenant RLS,
  transaction ownership, PostgreSQL expiry authority, refresh-family
  containment, API-key bookkeeping, sealing inventory/rewrap, and concurrent
  BFF replicas.

The repository has no `.codegraph/` directory. This pass reviewed the complete
base-to-candidate range and used `8289fa298..6aedcda516` only to locate the R3
changes. The candidate remained at the stated commit through this review.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Tenant isolation and transaction ownership | `AGENTS.md`; `architecture/agent-rules.md`; SQL foundation; `TenantConn`; browser-session and login-state queries/migrations; all `BrowserSessions` callers and row writers | PASS |
| Session locking and exact expiry | Revision-7 REQ-009/016; TASK-003 packet-local contract; `lock_browser_session` uses one `FOR UPDATE` statement and PostgreSQL `statement_timestamp()` for freshness, expiry, and absolute lifetime | PASS |
| Refresh replay containment | Security posture refresh-family rule; `RefreshTokens::execute`; `BrowserSessions::{current,renew}`; direct refresh and BFF consumers | PASS |
| Internal renewal failure atomicity | R3 FIND-TASK-003-14; `IssuanceError`, `RefreshError`, and `ExchangeError` classifications; API-key `last_used`; browser-session rotation/revocation | PASS |
| Stored credential cannot be opened | Revision-7 REQ-005; R3 internal-failure decision; `open_credential`; boot-time sealing inventory and rewrap owner | PASS |
| Replica and rewrap concurrency | TASK-003 two-replica contract; row lock ordering; sealing exact-byte CAS; sealing rotation journey | PASS |
| Connection-test durability | Revision-7 REQ-003/AC-006 and human direction; login-state migration/query owner; callback stamp and audit transaction | PASS |

## Domain assessment

- `BrowserSessions::current` holds the tenant-scoped browser row lock across
  renewal. A successful renewal rotates the backing credential and browser row
  in the same caller-owned transaction. A competing replica then re-reads the
  winner's fresh row. There is no inverse session-row/refresh-family lock order
  in a sibling writer.
- Ordinary credential or lifecycle refusal rolls back tentative refresh or
  API-key bookkeeping, relocks once, and serves only an unexpired stored access
  snapshot. At or after the same PostgreSQL expiry verdict, it revokes and
  wipes the browser row in the refusal transaction.
- Refresh reuse is preserved as `Renewal::Contained`. Before access expiry,
  the existing refresh-family revocation and canonical audit commit, the
  browser row remains live, and the request relocks before serving the already
  issued snapshot. At or after expiry, browser-row revocation joins that same
  transaction. Repeated callers may restage containment evidence, but the
  family stays revoked and no successor can escape; R3 explicitly rejected a
  new persistent suppression marker.
- Store, audit, signing, corrupt-state, verification-task, and envelope-open
  failures become `Renewal::Failed`. Returning the error drops `TenantConn`, so
  SQLx rolls back refresh consumption, API-key `last_used`, audits, and browser
  mutations and releases the row lock. A retry starts from the unchanged row.
  Cancellation has the same rollback boundary; cancellation after a committed
  containment attempt leaves the security effect durable and a later request
  can relock the still-live browser row.
- The implementer-flagged behavior is correct. A missing or unopenable stored
  renewal credential is not proof that the credential was revoked; it can be a
  recoverable replica/keyring or data availability failure. Treating it as an
  internal retryable failure fails closed without destroying the only stored
  session state. This aligns with R3's explicit envelope-open classification
  and REQ-005's rotation requirement. The canonical boot/rewrap pass still
  refuses readiness while any ciphertext cannot be opened, so a normally
  started replica cannot silently accept a permanently unusable keyring.
- The sealing inventory covers every non-null browser-session envelope,
  including expired but unpurged rows. Exact-old-byte CAS prevents a rewrap
  from overwriting concurrent renewal, logout, purge, or another rewrap, and a
  lost race remains visible as `remaining` for the next pass.
- Browser-session and connection-test rows remain tenant-scoped under forced
  RLS. Hash-to-tenant `SECURITY DEFINER` functions expose only the tenant needed
  to open the owning `TenantConn`; all durable mutations then execute under
  that tenant transaction.

## Material proposed findings

None.

## Prior-finding closure

- `FIND-TASK-003-14` is closed. Replay containment, ordinary refusal, and
  internal failure retain distinct transaction outcomes, including both sides
  of exact access expiry.
- The prior sealing-inventory and refused-proactive-renewal findings remain
  closed. R3 adds no migration, marker, second audit owner, or sibling durable
  path that reopens them.

## Verification evidence and limits

This review ran the four R3 Postgres selectors through the repository-managed
database lifecycle; all four passed:

- `proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry`
- `refresh_renewal_internal_failure_rolls_back_and_remains_retryable`
- `api_key_renewal_internal_failure_rolls_back_and_remains_retryable`
- `proactive_renewal_refusal_preserves_authority_until_expiry`

The setup also ran both migration-idempotence tests successfully. `git diff
--check` for the immutable range passed. The implementation record reports
`test:wyrd`, `test:identity:journey`, `check:tenant-isolation`, `fmt`, and
`lints` green; this reviewer did not rerun those broader lanes.

There is no test that corrupts only a stored renewal envelope and then repairs
it. That is a verification limit, not a material finding: the source routes
that branch directly to the same `Renewal::Failed` rollback boundary exercised
before and after expiry by both backing modes, while the production-shaped
sealing journey and boot tests independently prove retained-key recovery,
concurrent rewrap CAS, and refusal of unopenable inventory.

## Overall result

**PASS** — the cumulative candidate satisfies the reviewed persistence,
concurrency, tenancy, and durability obligations. The R3 correction closes the
prior renewal regression without adding persistent state or a second owner,
and the flagged unopenable-credential behavior is a fail-closed, rollback-only,
retryable internal failure rather than a durable session-ending decision.

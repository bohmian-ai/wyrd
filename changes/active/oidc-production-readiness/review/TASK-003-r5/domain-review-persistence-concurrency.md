# Persistence, concurrency, tenancy, and durability domain review

## Subject and reviewed boundary

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved authority: `SPEC-oidc-production-readiness` revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2, and the explicitly human-authorized R3
  and R4 rounds
- Human directions: conditional issuer binding and real interactive connection
  testing
- Domain: browser-session and connection-test persistence, tenant RLS,
  caller-owned transactions, PostgreSQL coordination time, refresh-family
  serialization, API-key bookkeeping, sealing inventory/rewrap, and
  cross-replica session lifecycle.

The repository has no `.codegraph/` directory. This review inspected the full
base-to-candidate range and used `6aedcda5166509001db0cc851a5bc74502b4b043..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
only to locate the R4 changes. The latest range narrows three classifier methods,
corrects local renewal rustdoc, and adds one unit test; it does not change the
durable runtime paths reviewed below. The candidate remained at the stated
commit through this review.

## Authority and source coverage

| Reviewed boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Tenant isolation and transaction ownership | `AGENTS.md`; `architecture/agent-rules.md`; architecture constraints and Rust SQL rules; revision-7 `INV-001`/`INV-007` | Browser-session and login-state migrations; `WyrdPostgres` hash-to-tenant resolvers; `TenantConn`; auth query modules; `BrowserSessions`; BFF handlers | PASS |
| Browser-session durability, exact expiry, and replica renewal | Revision-7 `REQ-005`, `REQ-009`, `REQ-016`; TASK-003 packet-local contract; Wyrd coordination clock | `auth_browser_sessions`; lock/rotate/revoke queries; `BrowserSessions::{complete,exchange_api_key,current,renew,read,authority}`; two-replica consumers; R2/R3 renewal proofs | PASS |
| SSO logout and refresh-family retirement | TASK-003 packet-local contract: logout revokes the browser session and its refresh family; security posture refresh-family rules | `BrowserSessions::logout`; `refresh_by_hash`; `revoke_refresh`; `lock_refresh_family`; `revoke_refresh_family`; direct refresh rotation; sealing-rotation logout journey | **FAIL — PC-R5-001** |
| API-key session lifecycle | Revision-7 `REQ-010`; TASK-003 API-key mode contract | `ExchangeApiKey`; sealed bootstrap-key storage; session-lock renewal; logout wipe without revoking the underlying key | PASS |
| Real connection-test state and transactional stamp | Revision-7 `REQ-003`/`AC-006`; `HD-TASK-003-R2-1` | Connection-test migration; login-state insert/consume; `HumanConnections::{begin_test,tested_candidate,stamp_test_sign_in}`; callback authority recheck, candidate stamp, and audit-failure proofs | PASS |
| Sealing inventory, exact-byte CAS, and keyless boot | Revision-7 `REQ-005`/`AC-007`; R2 sealing remediation | `SealedSecretTable::ALL`; operator inventory/swap queries; `SealedSecretRewrap`; expired-row boot proof; live K1/K2 race journey | PASS |
| R4 renewal boundary closure | Authorized R4 remediation, `FIND-TASK-003-16`/`17` | Classifier visibility, `open_text`/`open_credential`, focused unit test, and existing PostgreSQL renewal consequences | PASS |

## Domain assessment

- Browser sessions are located by a high-entropy hash through narrow
  `SECURITY DEFINER` functions, then all row reads and mutations run under the
  resolved tenant's forced-RLS `TenantConn`. The service key and route slug do
  not become tenant authority.
- Session creation, completion redemption, credential issuance bookkeeping,
  row rotation, and revocation preserve caller-owned transaction boundaries.
  The SQL query layer neither commits nor rolls back a supplied `TenantConn`.
- `lock_browser_session` is the shared row-lock boundary for renewal and
  logout. PostgreSQL's one statement instant decides freshness, access expiry,
  and absolute expiry. Competing BFF replicas therefore serialize and reread a
  committed rotation or revocation.
- R3's renewal outcomes remain correctly distinct. Ordinary refusal rolls back
  before exact access expiry, replay containment commits its family-wide
  revocation and audit, and internal failure returns without committing or
  serving stale authority. R4's visibility and envelope-mapping correction
  preserves those transaction outcomes.
- Connection-test state has exactly one browser, CLI, or tester binding and is
  consumed before provider IO. The callback rechecks the exact candidate
  revision and tester authority; candidate stamp and canonical audit commit in
  one tenant transaction, and a test produces no User, credential, or browser
  completion.
- The canonical rewrap inventory includes every non-null browser-session
  envelope, including expired but unpurged rows. Each operator swap is tenant-
  and row-qualified and compares the exact old bytes, so renewal, logout,
  purge, or another rewrap cannot be overwritten; a lost swap remains visible
  for the next pass.

## Material proposed finding

### PC-R5-001 — Logout revokes only the refresh row stored in the browser record, not its refresh family

- **Classification:** `INCORRECT`
- **Violated obligation:** TASK-003 states that SSO logout revokes the browser
  session *and its refresh family* (`TASK-003-production-ui.md:91,103`). The
  security posture makes the family the containment boundary for a rotated
  refresh credential. Logout must therefore retire a successor that already
  exists and must serialize against a concurrent rotation.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:421-434` calls
  `refresh_by_hash` and then the single-row `revoke_refresh`; the latter updates
  only one id and takes no family lock
  (`crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:147-170`). The
  existing family serialization and family-wide update are
  `lock_refresh_family` and `revoke_refresh_family`
  (`refresh_tokens.rs:96-120,173-201`).
- **Evidence and reachable path:** an SSO browser row retains the refresh token
  it last stored. The ordinary refresh owner can rotate that token and commit a
  successor before the BFF next reads or renews the browser session; the R3
  replay test deliberately creates this reachable state. If logout runs next,
  `refresh_by_hash` returns the now-rotated predecessor, and
  `revoke_refresh(... WHERE revoked_at IS NULL)` changes nothing. Logout still
  revokes and wipes the browser row and commits, while the already-issued
  successor remains active. The same omission races with a rotation in flight:
  without the family advisory lock, a rotation that wins the predecessor row
  can insert its successor after logout's single-row update. No later browser
  request remains to trigger replay containment because logout has ended the
  browser row.
- **Observable consequence:** the UI reports logout complete and the browser
  cookie/session is dead, but a copied or concurrently rotated successor
  refresh token can continue minting Wyrd user authority. This contradicts the
  explicit mode-specific logout contract and leaves renewable authority alive
  after the user ended the session.
- **Smallest testable correction:** keep `BrowserSessions::logout`, its
  existing browser-row lock, tenant transaction, and mode split. After the
  stored refresh hash resolves its immutable principal family, reuse
  `lock_refresh_family` and `revoke_refresh_family` in that same transaction
  instead of the single-row revoke. This is the existing family authority and
  serializes logout with both an already-committed successor and a concurrent
  rotation; add no new table, marker, audit path, or lock abstraction. Preserve
  API-key logout, browser-row wiping, idempotence, and the current behavior when
  no refresh credential can be opened.
- **Focused closure proof:** create an SSO browser session, rotate its stored
  refresh token once through the ordinary refresh owner and commit the
  successor, then call browser logout before any browser read/authority call.
  Assert the browser row is revoked and wiped, every active row in that
  principal family is revoked, and the successor cannot rotate. Add a
  deterministic overlap case only if the existing Postgres test can coordinate
  rotation and logout cheaply; the required family lock makes the sequential
  committed-successor case the minimum proof of the diagnosed gap. Retain the
  existing mode-specific logout journey to prove the bootstrap API key itself
  remains valid.

## Prior-finding closure

- `FIND-TASK-003-4` remains closed: canonical inventory covers expired,
  unpurged browser ciphertext and exact-byte CAS remains the shared fence.
- `FIND-TASK-003-10` and `FIND-TASK-003-14` remain closed for session renewal:
  exact-expiry refusal, replay containment, and internal-failure rollback are
  still distinct and source-complete.
- `FIND-TASK-003-16` and `FIND-TASK-003-17` are closed by the R4 candidate:
  renewal classifiers are crate-private, the envelope contract is accurate,
  and the direct unit check covers missing and unopenable renewal envelopes.
- `PC-R5-001` is a separate logout-family defect. It does not reopen the
  browser-renewal containment correction; logout bypasses that owner by ending
  the browser row after only a single refresh-row update.

## Verification evidence and limits

The implementation record supplied for this review reports the two R4 unit
selectors, the four PostgreSQL browser-renewal selectors, migration checks,
`mise run test:wyrd`, `mise run test:identity:journey`,
`mise run check:tenant-isolation`, `mise run fmt`, `mise run lints`, and
`git diff --check` green. This reviewer did not rerun Cargo, Postgres, browser,
or provider lanes.

The existing sealing-rotation journey opens the browser row's *current*
refresh token, logs out, and then presents that same revoked predecessor. It
does not rotate a successor before logout or overlap logout with rotation, so
the reported green evidence cannot detect `PC-R5-001`. No committed focused
test covers logout after the browser's stored refresh token has already been
rotated.

## Overall result

**FAIL** — the cumulative candidate preserves tenant RLS, caller-owned
transactions, replica-safe renewal, exact-expiry behavior, connection-test
durability, and sealing rotation, and it closes the authorized R4 boundaries.
However, SSO logout does not fulfill its explicit refresh-family retirement
contract: a committed or concurrently created successor can survive after the
browser session is revoked.

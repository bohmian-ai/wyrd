# Persistence, concurrency, and durability domain review

## Subject and reviewed boundary

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`, with the issuer-only amendment in
  `human-direction-FIND-TASK-003-1.md`
- Domain: `auth_browser_sessions` persistent state and RLS; completion/session
  transaction ownership; access renewal and logout serialization; absolute and
  producer expiry; sealed access, refresh, API-key, and CSRF values; canonical
  rewrap inventory, exact-old-byte fencing, partial progress, keyless boot, and
  K2-only recovery.

The issuer amendment changes no persistence, concurrency, or durability
obligation in this review.

## Authority and source coverage

| Boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Tenant ownership and SQL capability | `AGENTS.md` §§2, 3, 9; `architecture/agent-rules.md` TenantConn/OperatorPool rules; `architecture/wyrd-security-posture.md` | Migration `20261001000001_auth_browser_sessions.sql`; `WyrdPostgres::{login_completion_tenant,browser_session_tenant,tenant_directory_entry}`; `BrowserSessions::{complete,exchange_api_key,current,logout}`; tenant query module and SECURITY DEFINER tenant resolvers | PASS |
| Durable row shape and expiry | REQ-005/009/010/016; TASK-003 packet-local session contract; R1-AC-09 | Migration checks; `BrowserSessionWrite`; `insert_browser_session`; `lock_browser_session`; `rotate_browser_session`; `revoke_browser_session`; all constructors and re-exports | PASS |
| Completion and API-key session creation | REQ-005/009/010, INV-001/005 | `BrowserSessions::{complete,exchange_api_key,insert}`; `redeem_login_completion`; ordinary `ExchangeApiKey`; audit and transaction callers | PASS |
| Replica renewal and logout concurrency | REQ-009/016; TASK-003 row-lock and mode-specific logout contract | `LOCK_BROWSER_SESSION_SQL ... FOR UPDATE`; `BrowserSessions::{current,renew,logout}`; refresh-token and API-key issuance owners; human-connection lifecycle checks; BFF read/authority/logout callers | PASS |
| Canonical sealing-key lifecycle | REQ-005; AC-007; FIND-TASK-003-4/R1-AC-04 | `SealedSecretRewrap::{run,settle}`; all `SealedSecretTable` variants and SQL; boot `rewrap_sealed_secrets`; authentication runbook; every browser-session seal/open/write/wipe site | PASS |
| Rotation races and recovery | FIND-TASK-003-4/R1-AC-04 | Exact-byte CAS writers; sibling renewal, logout, expiry purge, and connection/provider writers; `browser_session_sealing_rotation_journey`; keyless boot Postgres test; K2-only replica use | PASS |

## Domain assessment

- The durable row admits only the two approved session modes. Its database
  constraint requires every live row to carry an access token and CSRF token,
  exactly one mode-specific renewal credential, and connection provenance only
  for SSO. Revocation wipes every sealed value in the same statement.
- Tenant identity is discovered from a high-entropy flow/session hash through a
  narrow SECURITY DEFINER resolver, then every durable session operation runs
  through the returned tenant's `TenantConn` under forced RLS. The BFF service
  key and route tenant never become row authority.
- Completion redemption and SSO-session insertion share one caller-owned tenant
  transaction. API-key exchange, its audit decision, and session insertion also
  share one tenant transaction. Cancellation or a write failure before commit
  leaves no partial session; response loss after commit cannot create a second
  credential or re-redeem the completion.
- `SELECT ... FOR UPDATE` is the common serialization point for authority reads,
  renewal, and logout. The loser rereads the winner's committed credential or
  revocation. Refused renewal wipes the session before commit and never falls
  back to another provider, key, or tenant. Infrastructure failures roll the
  caller transaction back.
- PostgreSQL supplies the session creation and comparison clock. Absolute SSO
  and API-key limits are fixed durations from insertion; access-token and
  refresh-token expiry remain separately producer-owned. The removed nullable
  dual-lifetime branch has no remaining caller or SQL bind.
- `SealedSecretTable::ALL` now inventories the access, refresh, bootstrap API-key,
  and CSRF columns of every live browser session in the same canonical owner as
  provider secrets. Each swap matches the tenant, row identity, column, and exact
  old ciphertext. If renewal, logout, purge, or another replica wins after the
  read, the swap affects zero rows and increments `remaining`; a later pass
  converges it. If rewrap wins first, the sibling writer blocks and then either
  writes under the deployment write key or wipes the row, so no old ciphertext
  is resurrected.
- Rewrap is intentionally resumable rather than atomic across the deployment:
  each successful CAS remains committed if a later read or swap fails. With
  retained keys this is safe, and the absence of a zero report prevents key
  retirement. Keyless boot converts either a nonzero inventory or an inventory
  query failure into a boot refusal.
- The production-shaped rotation journey creates both SSO and API-key sessions
  under K1, covers every non-null column, forces a lost CAS against a concurrent
  session mutation, reaches zero on a later pass, refuses keyless boot while the
  live values remain, and proves read, CSRF recovery, mode-specific renewal,
  action, and logout on a K2-only replica. Logout proof also checks refresh-token
  revocation and preservation of the underlying API key.

## Prior-finding closure

`FIND-TASK-003-4` / prior domain finding `PC-001` is closed. The defect was at
the shared rewrap inventory, and the remediation corrected that owner rather
than adding guards to session readers. Current source includes all four live
session columns in the canonical report and exact-byte CAS path; the operator
runbook and boot diagnostics now describe the same scope; the Postgres and real
identity journey evidence exercises K1-to-K2 convergence, a lost race, keyless
refusal, and K2-only recovery.

## Material findings

None.

## Verification limits

- This independent review inspected the immutable cumulative source, latest-fix
  diff, sibling readers/writers, and committed test evidence. It did not rerun
  Cargo, mise, Postgres, or provider-backed lanes.
- The implementation record reports all-green `mise run test:identity:journey`
  (including `browser_session_sealing_rotation_journey`), `test:wyrd`,
  `test:sql`, tenant-isolation, format, lint, codegen, docs, UI test, and UI
  typecheck lanes after the final code commit.
- Expired, unpurged session rows are outside the live-session inventory. They
  are unreachable through the session tenant resolver and are deleted by the
  existing tenant-local purge on the next session creation. The approved
  remediation explicitly scoped rewrap and key retirement to live sessions;
  this is not a remaining TASK-003 defect.
- The forced race changes the access envelope while holding the same PostgreSQL
  row lock used by real renewal; mode-specific renewal itself is then exercised
  on the K2-only replica. Logout's competing-write outcome follows the same
  exact-byte CAS and row-lock path but is not separately forced to win in the
  race harness.

## Overall result

**PASS** — the cumulative candidate satisfies the task's persistence,
concurrency, and durability obligations. The prior canonical-rotation omission
is closed at its shared owner, with credible recovery and race proof, and no new
material domain finding remains.

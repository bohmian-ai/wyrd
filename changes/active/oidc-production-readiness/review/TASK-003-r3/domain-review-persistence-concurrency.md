# Persistence, concurrency, tenancy, and durability domain review

## Subject and reviewed boundary

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: `SPEC-oidc-production-readiness` revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`
- Latest remediation locator: `622a77028..8289fa298ed33d21f2568558bc0a02905fd0b218`
- Domain: browser-session persistence and row-lock renewal; tenant RLS and
  transaction ownership; connection-test state; PostgreSQL coordination time;
  refresh replay containment; sealing inventory, rewrap CAS, and keyless boot.

The repository has no `.codegraph/` directory. This pass inspected the
cumulative base-to-candidate source and used the latest remediation diff only
to locate changed owners.

## Authority and source coverage

| Boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Tenant ownership and SQL capability | `AGENTS.md` §§2, 3, 9; `architecture/agent-rules.md`; security posture tenant boundary | Browser-session and connection-test migrations; `WyrdPostgres` definer lookups; `TenantConn`; auth query modules; `BrowserSessions`; BFF handlers | PASS |
| Browser-session row lifecycle and coordination time | REQ-005/009/010/016; TASK-003 packet-local session contract; `wyrd-design.md` coordination clock | `insert_browser_session`, `lock_browser_session`, `rotate_browser_session`, `revoke_browser_session`; creation, read, authority, logout, and expiry consumers | PASS |
| Replica renewal, replay containment, and failure atomicity | REQ-009/016/017; R2 FIND-10 and R2-AC-04; security posture refresh-token replay rule | `BrowserSessions::{current,renew}`; `RefreshTokens::execute`; `ExchangeApiKey::execute`; `TenantTokenIssuer`; proactive-renewal Postgres test | **FAIL — PC-R3-001** |
| Real connection-test state | revision-7 REQ-003/AC-006; HD-TASK-003-R2-1 | migration `20261001000002_auth_connection_test_state.sql`; login-state insert/consume; `HumanConnections::{begin_test,tested_candidate,stamp_test_sign_in}`; callback completion and audit-failure tests | PASS |
| Sealing inventory and crash-resumable rotation | REQ-005/AC-007; FIND-TASK-003-4/R2-AC-01 | every `SealedSecretTable` inventory/CAS branch; `SealedSecretRewrap`; boot decision; expired-session and live-session rotation proof | PASS |

## Domain assessment

- Browser-session lookup discloses only the tenant of a live high-entropy id,
  after which all row work uses that tenant's forced-RLS `TenantConn`. Session
  creation, completion redemption, credential bookkeeping, and audit writes
  remain inside caller-owned transactions.
- PostgreSQL owns the session's absolute lifetime, freshness and expiry
  predicates, candidate test validity, and login-state expiry. Producer-owned
  JWT expiry is stored as issued and compared to `statement_timestamp()` by the
  statement that decides whether renewal is needed.
- The connection-test migration makes browser, CLI, and tester bindings
  mutually exclusive. Test state is consumed and committed before provider IO;
  callback completion rechecks the exact candidate revision and the initiating
  principal's current tenant authority before the tested stamp and its audit
  commit together. Candidate replacement races therefore fail without stamping
  another revision.
- The canonical sealing inventory now includes every non-null browser-session
  envelope, including expired but unpurged rows. Exact-old-byte CAS prevents a
  rewrap from overwriting concurrent renewal, logout, purge, or another rewrap;
  partial passes remain resumable, and keyless boot refuses on either remaining
  ciphertext or an inventory failure.
- The proactive-renewal remediation correctly rolls back ordinary lifecycle
  refusals such as a revoked API key, then relocks before serving the existing
  token. It does not preserve the distinct durable semantics of refresh replay
  containment or infrastructure failure, as detailed below.

## Material proposed finding

### PC-R3-001 — The early-renewal rollback erases refresh-replay containment and treats infrastructure failures as policy refusals

- **Classification:** `REGRESSION`
- **Violated obligation:** `architecture/wyrd-security-posture.md:149-151`
  requires reuse of a rotated refresh token to revoke its family and emit a
  security audit event. R2 FIND-10 explicitly preserves fail-closed behavior
  for infrastructure failures
  (`TASK-003-R2-production-ui-remediation.md:103-109`). `AGENTS.md` and
  `architecture/agent-rules.md` require security-significant authorization
  evidence to commit at its owning decision boundary.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:489-508,555-584`;
  producer at `crates/wyrd/wyrd-auth/src/refresh.rs:121-173`;
  issuance/audit failures at
  `crates/wyrd/wyrd-auth/src/issuance.rs:403-489`;
  incomplete proof at
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:879-959`.
- **Evidence and reachable path:** `RefreshTokens::execute` deliberately treats
  a stored-but-no-longer-active refresh token as replay: under the family lock
  it revokes every family row and appends the denied containment audit before
  returning `RefreshError::Reused`. This is reachable when the browser's stored
  refresh token was copied and rotated through the ordinary `/auth/token` path
  before a BFF replica enters its one-minute proactive window. `BrowserSessions::renew`
  collapses `Reused` with every non-database error into `Renewal::Refused`.
  Because the access token is still unexpired, `current` explicitly rolls back
  that transaction, erasing both the family revocation and its audit, relocks,
  and serves the old access token. The externally rotated successor therefore
  remains renewable until a post-expiry BFF call repeats the detection and
  finally commits it.

  The same lossy classification maps refresh/API-key audit failures, signing
  failures, corrupt-role failures, `SqlError`-backed issuance failures, and the
  API-key verification task failure to `Refused`; the early branch rolls them
  back and serves existing authority instead of failing closed. Only direct
  database variants are currently classified as `Renewal::Failed`. The added
  test covers a revoked API key only, so it cannot detect either path.
- **Observable consequence:** refresh-token theft detected during proactive
  renewal leaves the attacker's successor family live and produces no durable
  security evidence for up to the remaining access-token lifetime. An audit,
  signing, or other infrastructure failure during that same renewal window can
  return authority successfully, contrary to the remediation's fail-closed
  constraint.
- **Smallest testable correction:** keep `BrowserSessions`, its existing row
  lock, rollback/relock loop, and PostgreSQL expiry verdict. Preserve the
  renewal outcome instead of collapsing it: only ordinary credential/lifecycle
  refusals that produced no required durable containment may use the
  rollback-and-serve-until-expiry path. A refresh replay must keep the existing
  `RefreshTokens` family revocation and audit, revoke the browser row in that
  same transaction, commit, and refuse immediately. Audit, store, signing,
  verification-task, and corrupt-state failures must propagate without serving
  authority. Add no second refresh or audit owner.
- **Focused closure proof:** create an SSO browser session, rotate its stored
  refresh token once through the ordinary refresh owner, then invoke browser
  authority while its access token is unexpired inside the renewal margin.
  Assert the call refuses, the complete refresh family and browser row are
  revoked, and exactly one replay-containment audit commits. Separately inject
  a renewal audit failure and assert the browser authority call fails while its
  prior durable state remains unchanged. Retain the existing revoked-API-key
  test to prove the permitted rollback-and-serve branch.

## Prior-finding closure

- `FIND-TASK-003-4` is closed at the shared sealing inventory. Expired,
  non-revoked browser envelopes are included in the canonical pass and keyless
  boot proof.
- `FIND-TASK-003-10` is not fully closed. The candidate preserves authority
  until expiry for the tested revoked-API-key lifecycle refusal, but its shared
  `Renewal::Refused` bucket also rolls back mandatory replay containment and
  converts infrastructure failures into that lifecycle behavior.

## Verification limits

- Per assignment, this review ran no Cargo, mise, Postgres, provider, browser,
  or UI tests. It inspected the immutable source, cumulative diff, latest
  remediation diff, relevant callers/writers, and committed test source.
- The remediation record reports green identity, Wyrd, SQL, tenant-isolation,
  codegen, docs, format, lint, and UI lanes. Those results do not cover the
  refresh-replay or renewal-infrastructure branches above; the focused
  proactive-renewal test constructs only an API-key session and revokes that
  key.

## Overall result

**FAIL** — tenant RLS, persistent test state, PostgreSQL coordination, and
sealing durability are sound, but the shared proactive-renewal correction
rolls back mandatory refresh-replay containment and can serve authority after
an infrastructure failure. The task is not acceptable until `PC-R3-001` is
closed with durable replay and fail-closed proof.

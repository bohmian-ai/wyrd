# Domain review: tenancy, persistence, durability, and concurrency

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `63e6a545db156a095b670a5bb8bc36f6f36fba32`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation and findings: TASK-002 R1, R2, and R3 review packets
- Domain result: **FAIL**

The candidate was the checked-out `HEAD` before and after source inspection.
The repository has no `.codegraph/` index, so the review used Git and direct
source/caller inspection.

## Reviewed boundary

This review traced the complete cumulative base-to-candidate flow through:

1. tenant route resolution, Active-connection capture, and durable hashed
   login-state insertion;
2. least-disclosure state-owner lookup, transition to the owning
   `TenantConn`, consume-before-provider-IO, and one-use completion;
3. tenant User resolution, role replacement, canonical audit, session and
   refresh issuance, completion sealing, and commit ownership;
4. connection candidate replacement, testing, activation, deactivation, and
   removal under the tenant connection-slot lock;
5. refresh provenance, active-row consumption, successor insertion,
   replay-family revocation, route-level commit behavior, and connection
   replacement cutoff;
6. forced RLS, same-issuer cross-tenant isolation, migration and upgrade
   behavior, and retry/failure durability; and
7. all changes after the R3 candidate, including the rustdoc correction and
   callback refusal journey extension, for data or concurrency regressions.

## Authority and source coverage

| Boundary | Governing obligation | Source and evidence inspected | Result |
|---|---|---|---|
| Tenant selection and least disclosure | Spec REQ-006/007/015, INV-001/004; task packet-local callback contract; security posture tenant identity | `wyrd-auth/src/login.rs`, `callback.rs`; `WyrdPostgres::login_state_tenant`; login-state migration and Postgres tests | PASS |
| Forced-RLS transitions and transaction ownership | `AGENTS.md` §§2, 6, 9; `architecture/agent-rules.md`; architecture constraints and patterns | `login_state.rs`, `human_connections.rs`, user/role/refresh queries, callers that own commit | PASS |
| One-time state and completion races | Task packet-local bounded one-use state/completion contract; REQ-007, INV-004 | consume/complete/redeem SQL; callback and redemption owners; `pg_login_state.rs` and callback tests | PASS |
| Connection lifecycle serialization and revisions | REQ-014/016; task replacement/renewal scenario | connection-slot advisory lock, candidate revision CAS/stamp, activation/deactivation/removal, session issuance recheck | PASS |
| Login persistence and audit atomicity | REQ-008/017; canonical same-transaction audit authority | identity upsert, role-set replacement, issuance, audit append, refresh insert, sealed completion, rollback proofs | PASS |
| Refresh provenance and connection cutoff | REQ-016; task packet-local renewal contract | provenance columns/migration, `issue_human_session`, active-connection check under the slot lock, provider-switch evidence | PASS |
| Refresh-family concurrency and replay containment | Task: “lock the token family and active tenant connection in one tenant transaction”; security posture refresh replay rule; INV-004 | `RefreshTokens::execute`; refresh-token SQL; route commit; existing F07/F09 tests | **FAIL — TD-R4-001** |
| Migration and upgrade durability | Task migration obligations; persistent-data constraints | `20260925000000` and `20260925000001`, migration preflight/idempotence tests | PASS |
| Cross-tenant same-issuer isolation | REQ-015, INV-001–004; forced-RLS authority | tenant-qualified identity/state/refresh stores; state-owner proof; same-issuer journey | PASS |
| Post-R3 changes | Approved R3 remediation and lead-directed test cleanup | `d861845f3..63e6a545d`: rustdoc-only callback correction, gateway test cleanup, advertised-HS256 refusal case, evidence records | PASS; no data-path change |

## Material proposed findings

### TD-R4-001 — Refresh replay can miss a concurrently inserted successor

- **Classification:** INCORRECT / concurrency and durability.
- **Violated obligation:** TASK-002 requires refresh to lock the token family
  and Active tenant connection in one tenant transaction, while preserving
  replay-family revocation. The security posture requires reuse of a rotated
  refresh token to revoke its token family. `INV-004` requires replay
  protection to remain fail closed.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/refresh.rs:117-190`,
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:14-25,31-39,134-162`,
  and the route commit at
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:219-257`.
- **Evidence:** `RefreshTokens::execute` atomically consumes only the presented
  row, then inserts its successor through `issue_human_session`. The stale
  branch separately reads the replayed row and runs a principal-wide
  `UPDATE ... WHERE revoked_at IS NULL`. No refresh-family row lock,
  transaction advisory lock, or other shared serialization point is acquired
  by both branches; the only additional lock is the human-connection slot in
  `issue_human_session`. Under PostgreSQL `READ COMMITTED`, a family-revocation
  `UPDATE` uses its statement snapshot. If it starts while a legitimate
  rotation has consumed the current token but has not committed its newly
  inserted successor, it can wait on/recheck the old row after that rotation
  commits without adding the newly inserted successor to the original target
  set. It then commits `RefreshError::Reused` and the canonical containment
  audit while that successor remains active. The existing race proof at
  `wyrd-auth/src/refresh.rs:638-693` and durable replay proof at `:870-984`
  serialize the operations deliberately: each commits the legitimate rotation
  before starting replay, so neither exercises this overlap.
- **Observable consequence:** a replay request can return the documented
  family-revoked refusal and durably audit containment while a concurrently
  minted refresh successor remains usable. An attacker or concurrent holder
  can therefore continue renewing after Wyrd claimed to contain reuse.
- **Required testable correction:** serialize every refresh operation for the
  same tenant User family before deciding active-versus-stale and keep that
  serialization through active-row consumption, successor insertion, replay
  revocation, audit, and the caller-owned commit. Derive the stored family
  owner under the existing `TenantConn`/RLS boundary, acquire one database
  transaction lock shared by both the active and stale paths before mutation,
  then re-read the token state under that lock. Preserve the existing
  connection-slot lock and acquire locks in one fixed order. Add a real
  Postgres concurrency proof that pauses a legitimate rotation after consuming
  the current token but before commit, overlaps replay of its predecessor,
  commits both route-equivalent outcomes, and proves from a fresh transaction
  that no successor can rotate and the containment audit is durable.

## Prior-finding closure

| Prior finding | Domain validation at this candidate | Result |
|---|---|---|
| FIND-TASK-002-3 | Changed roles append one canonical event in the same transaction as role replacement, issuance, refresh insertion, and completion; injected append failure rolls the transaction back. | CLOSED |
| FIND-TASK-002-5 | Login-state purge, consume, complete, and redeem rely on forced RLS without a parallel tenant predicate; cross-tenant transition coverage remains present. | CLOSED |
| FIND-TASK-002-6 | State ownership remains a narrow `WyrdPostgres` operation over the private app pool and returns only the owner UUID for a pending high-entropy state hash. | CLOSED |
| FIND-TASK-002-7 | PKCE verifier remains `SecretString` immediately after SQL decode and the Debug-redaction proof remains present. | CLOSED |
| FIND-TASK-002-8/9/10 | Documentation/import corrections do not change tenancy, persistence, or transaction behavior; the R3 rustdoc correction is accurate and data-path neutral. | CLOSED / no domain regression |

`TD-R4-001` is separate from the prior findings: it falsifies the explicit
family-lock obligation and an overlapping replay/rotation interleaving that
the sequential replay tests do not cover.

## Verified properties without findings

- The callback's only cross-tenant operation answers the tenant owning an
  unconsumed, unexpired random state hash. All authoritative transitions then
  run through that tenant's `TenantConn` under forced RLS.
- Login-state consumption commits before provider IO. Completion issuance,
  role mutation, required audits, connection-bound refresh insertion, and
  sealed completion attach share one later transaction. Completion redemption
  is an atomic delete and deliberately commits before local unsealing so it
  cannot become replayable after a key/decode failure.
- Connection lifecycle mutations and human issuance use the same
  tenant-scoped advisory slot lock. An activation, deactivation, or removal
  therefore commits before issuance and blocks it, or waits until the issuing
  transaction commits. Exact connection id/revision provenance is copied to
  every refresh successor.
- Identity creation is tenant-local and keyed by exact `(issuer, subject)`.
  Concurrent creation retains the canonical mapping and soft-deletes the
  losing provisional User; email uniqueness is removed and no email lookup
  participates in identity authority.
- The migration intentionally drops only transient five-minute login state,
  revokes provenance-free live User refresh rows, preserves machine rows,
  installs forced RLS and the narrow definer lookup, and is covered by upgrade
  and repeated-application tests.

## Verification limits

- I inspected the complete cumulative diff and the full bodies/callers for the
  SQL, migration, login, callback, connection lifecycle, issuance, refresh,
  route-commit, and audit paths. Candidate `HEAD` remained
  `63e6a545db156a095b670a5bb8bc36f6f36fba32` through report creation.
- I did not rerun Cargo, Postgres, Keycloak/Dex, or broad repository lanes in
  this bounded Wave 1 review. Recorded evidence reports green focused SQL/auth
  tests, all required identity journeys, migration and tenant-isolation lanes,
  codegen/docs, format/lints, and broad Wyrd tests. Those results do not cover
  the overlapping family-revocation/successor-insert interleaving in
  `TD-R4-001`.
- The interleaving was established from PostgreSQL statement-snapshot and row
  update behavior plus the absence of a shared family lock; it still requires
  the focused deterministic Postgres concurrency proof named in the finding
  for closure.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain
  intentional non-goals. This review covers their server-owned durable
  primitive only.

## Overall result

**FAIL.** Tenant RLS, state ownership, one-use login/completion transitions,
connection replacement cutoff, audit atomicity, migration behavior, and
cross-tenant isolation pass. Refresh rotation does not implement the task's
required family-level serialization, leaving a reachable overlap where replay
containment can commit while a newly inserted successor survives.

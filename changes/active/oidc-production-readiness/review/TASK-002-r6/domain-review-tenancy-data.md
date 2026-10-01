# Domain review: tenancy, persistent data, concurrency, and durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R5 and stable findings
  `FIND-TASK-002-1` through `FIND-TASK-002-13`
- Domain result: **FAIL**

The checked-out `HEAD` equaled the candidate before and after inspection. The
repository has no `.codegraph/` directory, so this review used Git, `rg`, and
direct source/caller inspection. Lead-directed reuse and test commits recorded
separately in the evidence tables were treated as authorized rather than scope
drift.

## Reviewed boundary

This fresh cumulative review traced:

1. route-key resolution, Active-connection capture, and tenant-RLS insertion of
   opaque hashed login state;
2. the narrow state-hash-to-tenant lookup, transition into the owning
   `TenantConn`, consume-before-provider-IO commit, and callback replay path;
3. exact Active connection/revision revalidation, tenant User identity and role
   persistence, canonical audit, human-session issuance, sealed completion,
   and their transaction ownership;
4. one-use completion redemption and cross-tenant confinement;
5. connection lifecycle serialization and connection-revision cutoff;
6. refresh provenance, family-first serialization, active/stale
   classification, rotation, replay containment, audit, and route-owned commit;
7. every production caller of `revoke_refresh_family` and the seam between
   administrative User revocation and concurrent refresh rotation;
8. migration replacement of transient login state, forced RLS and the
   least-disclosure lookup, legacy unbound-refresh revocation, and removal of
   email uniqueness; and
9. the focused SQL, migration, callback, refresh-concurrency, and real-server
   journey evidence recorded for the cumulative candidate.

## Authority and source coverage

| Boundary | Governing obligation | Source and evidence inspected | Result |
|---|---|---|---|
| Effective tenant selection and least disclosure | Spec `REQ-006`, `REQ-007`, `REQ-015`, `INV-001`, `INV-004`; task packet-local callback contract | `wyrd-auth/src/login.rs`; `wyrd-auth/src/callback.rs`; `WyrdPostgres::login_state_tenant`; migration `20260925000001_auth_login_state_binding.sql`; `pg_login_state.rs` | PASS |
| Tenant SQL and transaction ownership | `AGENTS.md` §§2, 6, 9; `architecture/agent-rules.md` | `TenantConn` callers; login-state, identity, role, refresh, and connection query modules; route commit owners | PASS |
| One-use state and consume-before-provider-IO | Task packet login contract; spec `REQ-007`, `INV-004` | `callback.rs:96-168`; `login_state.rs`; state-transition and refusal tests | PASS |
| Login persistence and audit coupling | Spec `REQ-008`, `REQ-017`; canonical same-transaction audit rules | `callback.rs:201-267`; `issuance.rs:475-528`; role replacement, completion SQL, and audit-failure rollback tests | PASS |
| Active connection and revision serialization | Spec `REQ-014`, `REQ-016`; task replacement/renewal scenario | lifecycle methods; `lock_human_connection_slot`; callback rechecks; `issue_human_session`; provider-switch/cutoff evidence | PASS |
| Refresh rotation and replay serialization | R4 `FIND-TASK-002-12`; R4 requirement that every operation for the stored principal family serialize before classification and through terminal commit | `refresh.rs:84-227`; `refresh_tokens.rs:96-200`; refresh route `routes.rs:219-258`; deterministic ancestor-overlap test; all `lock_refresh_family` and `revoke_refresh_family` callers | **FAIL** — administrative family revocation bypasses the family lock (`TD-R6-001`) |
| Migration and legacy refresh behavior | Task migration contract; persistent-data constraints | migrations `20260925000000` and `20260925000001`; `pg_migration.rs` evidence | PASS |
| Same-issuer and cross-tenant isolation | Spec `REQ-002`, `REQ-015`, `INV-001`–`INV-004`; forced-RLS authority | tenant-qualified identity/state/refresh storage, definer lookup, RLS transition proof, same-issuer journey | PASS |

## Material proposed findings

### TD-R6-001 — Administrative User revocation is not serialized with refresh rotation

- **Classification:** INCORRECT
- **Violated obligation:** TASK-002-R4 requires every operation on one stored
  tenant principal refresh family to take the tenant-qualified family lock
  before classification or mutation and retain it through commit. Its purpose
  is to prevent a family-wide revocation statement from missing a successor
  inserted by a concurrent rotation. The durable User-revocation contract at
  `wyrd-auth/src/revoke.rs:20-22` also promises that suspension and family
  retirement commit together.
- **Exact location:** `crates/wyrd/wyrd-auth/src/revoke.rs:43-56` calls
  `suspend_user_principal` and `revoke_refresh_family` without
  `lock_refresh_family`. The competing path at
  `crates/wyrd/wyrd-auth/src/refresh.rs:121-218` takes that lock, consumes the
  current row, and inserts its successor. The family update is
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:180-200`.
- **Evidence and reachability:** `revoke_principal_in_conn` is the production
  owner called by `wyrd-server/src/auth/revoke.rs:107-120`, whose caller commits
  the suspension and family update. It is the second production caller of
  `revoke_refresh_family`; only the replay caller takes `lock_refresh_family`.
  A rotation can consume current row `B`, read the User as Active, and proceed
  toward successor `C`. Concurrent administrative revocation can suspend the
  User and start its family-wide `UPDATE`, blocking on `B`. After rotation
  inserts `C` and commits, the waiting `UPDATE` continues from the statement
  snapshot that did not contain `C`, the same cross-row snapshot shape that R4
  corrected for ancestor replay. The revoke route can therefore commit success
  while `C` remains unrevoked.
- **Observable consequence:** the administrative response says the User's
  refresh authority was retired, but the database can retain an active
  successor. Suspension prevents immediate rotation, but a later reactivation
  can revive that missed credential, contradicting the durable family
  retirement that principal revocation explicitly exists to preserve.
- **Required testable correction:** reuse the existing
  `lock_refresh_family(conn, "user", id_uuid)` capability in the User branch of
  `revoke_principal_in_conn`, taking it before suspension and family revocation
  and holding it through the existing route-owned commit. Do not add a lock
  service, family table, isolation-level change, or new concurrency mechanism.
  Add one deterministic Postgres overlap proof: hold a current-token rotation
  open after its successor is inserted, start administrative User revocation,
  prove it waits on the family lock, commit both, and from a fresh transaction
  prove the successor is `principal_revoked` and cannot become renewable after
  reactivation. Preserve the fixed family-before-connection lock order.

## Prior-finding closure

| Prior finding | Current-candidate validation | Result |
|---|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | Tenant identity remains keyed by exact `(issuer, subject)`; email does not select or merge identity. | CLOSED |
| `FIND-TASK-002-2` — OIDC `azp` | The callback retains multi-audience and present-`azp` binding. | CLOSED |
| `FIND-TASK-002-3` — role-change audit | Changed role sets append canonical evidence in the same transaction as role replacement and issuance; audit failure rolls back login effects. | CLOSED |
| `FIND-TASK-002-4` — advertised algorithm | Advertised-set membership remains followed by shared asymmetric/signature enforcement. | CLOSED |
| `FIND-TASK-002-5` — duplicated state tenant predicates | Purge, consume, complete, and redeem rely on forced RLS without manual tenant selection. | CLOSED |
| `FIND-TASK-002-6` — raw-pool state resolver | State-owner resolution remains a narrow inherent `WyrdPostgres` operation returning only an optional tenant id. | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | SQL decode wraps the verifier immediately in `SecretString`; redaction proof remains. | CLOSED |
| `FIND-TASK-002-8`, `-9`, `-10` — documentation/import corrections | The corrected item contracts and module-top imports remain intact. | CLOSED |
| `FIND-TASK-002-11` — mandatory OIDC claims | Tenant/platform ID-token paths remain on the claim-complete verifier; persistence begins only after verification. | CLOSED (security-owned runtime details reviewed separately) |
| `FIND-TASK-002-12` — replay/rotation serialization | The replay and rotation paths now share the family lock and the deterministic overlap proof closes the originally reported ancestor-replay race. `TD-R6-001` is a distinct uncovered production caller of the family-wide mutation, not a reopening of that tested interleaving. | CLOSED for replay; adjacent remediation seam remains incorrect |
| `FIND-TASK-002-13` — stale `refresh_by_hash` rustdoc | The candidate rustdoc accurately states lookup → family lock → classification and identifies the test-only post-revocation lookup. | CLOSED |

## Verified properties without findings

- The callback's cross-tenant database capability accepts only a SHA-256 state
  hash and returns only the tenant owning a pending, unexpired row. Host, path,
  headers, provider claims, and email do not select the effective tenant.
- The tenant-scoped consume is the authoritative one-winner state transition
  and commits before provider IO. A failed provider exchange leaves the attempt
  spent and cannot repeat the exchange through state replay.
- Successful completion couples identity, mapped roles, any role-sync audit,
  exact Active-connection recheck, token/refresh issuance, token-exchange audit,
  sealed completion, and completion attachment in one tenant transaction.
- Connection lifecycle mutations and human issuance share the connection-slot
  lock. Every issued or rotated refresh row retains the originating connection
  id/revision, so replacement and removal cut renewal off at the next boundary.
- Replay containment now takes the family lock before active/stale
  classification, and the refresh route deliberately commits the family
  revocation and its canonical audit before returning `Reused`.
- The migration intentionally discards transient login state, installs forced
  RLS and the least-disclosure lookup, revokes live provenance-free User
  refresh rows, preserves machine rows, and removes email uniqueness.

## Verification limits

- I inspected the complete changed-file inventory and cumulative diff, full
  relevant source bodies and production callers, both OIDC migrations, focused
  Postgres and real-server tests, prior verdict/validation records, and R1–R5
  remediation tasks. Candidate `HEAD` remained
  `0ca117a744ddcb7414b104c4382027970531b608` throughout.
- `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..0ca117a744ddcb7414b104c4382027970531b608`
  completed successfully.
- I did not independently run Cargo, Postgres, Docker, Keycloak/Dex, migration,
  lint, or broad identity lanes within this bounded Wave 1 slot. Runtime
  confidence for passing properties relies on inspected tests and the
  candidate's recorded green focused commands and broader lanes.
- No current test overlaps the production administrative User-revocation
  caller with refresh rotation. The existing deterministic concurrency proof
  covers ancestor replay versus rotation only, and the existing principal
  revocation proof is sequential; neither can detect `TD-R6-001`.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain
  approved non-goals.

## Overall result

**FAIL.** The cumulative candidate satisfies the reviewed tenant-RLS, login
state, connection-revision, callback transaction, migration, and replay-versus-
rotation properties. One material concurrency/durability gap remains:
administrative User revocation bypasses the family lock introduced by R4 and
can miss a concurrently inserted successor. `TD-R6-001` is bounded to reusing
the existing lock in the existing revocation owner plus one focused Postgres
overlap proof; it requires no specification or persistence-model revision.

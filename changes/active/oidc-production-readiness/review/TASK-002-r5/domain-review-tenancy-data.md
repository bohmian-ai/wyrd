# Domain review: tenancy, persistence, durability, and concurrency

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `b57d43d501c136591125b98fe78352e657b093b6`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R4 and their validated finding ledgers
- Domain result: **PASS**

The checked-out `HEAD` equaled the candidate before and after inspection. The
repository has no `.codegraph/` index, so this review used Git, `rg`, and direct
source/caller inspection. Lead-directed reuse and test commits separately
recorded in the task evidence were treated as authorized and checked for domain
regression rather than classified as scope drift.

## Reviewed boundary

This fresh cumulative review traced:

1. route-key resolution, Active-connection capture, and tenant-RLS insertion of
   opaque hashed login state;
2. the narrow server-role state-hash-to-tenant lookup, transition into the
   owning `TenantConn`, atomic consumption and commit before provider IO, and
   replay behavior;
3. exact Active connection/revision revalidation, tenant User identity
   persistence, role replacement, canonical audit, human session issuance,
   sealed completion, and their transaction boundaries;
4. one-use completion redemption and cross-tenant confinement;
5. connection lifecycle serialization and the shared connection-slot lock;
6. refresh provenance, family-first serialization, active/stale classification,
   rotation, ancestor replay containment, connection cutoff, audit, and
   caller-owned terminal commit;
7. migration replacement of transient login state, forced RLS and least-
   disclosure lookup installation, revocation of legacy unbound User refresh
   rows, and removal of email uniqueness; and
8. same-issuer cross-tenant identity/state isolation and the relevant SQL,
   Postgres, callback, migration, and real-server journey evidence.

## Authority and source coverage

| Boundary | Governing obligation | Source and evidence inspected | Result |
|---|---|---|---|
| Effective tenant selection and least disclosure | Spec `REQ-006`, `REQ-007`, `REQ-015`, `INV-001`, `INV-004`; task packet-local callback contract; security posture tenant identity rules | `wyrd-auth/src/login.rs`; `wyrd-auth/src/callback.rs:96-168`; `WyrdPostgres::login_state_tenant`; migration `20260925000001_auth_login_state_binding.sql:23-74`; `pg_login_state.rs`; callback-refusal and same-issuer journeys | PASS |
| Tenant SQL and transaction ownership | `AGENTS.md` §§2, 6, 9; `architecture/agent-rules.md`; architecture patterns storage rules | `TenantConn` callers; login-state, identity, role, refresh, and connection query modules; route/workflow commit owners | PASS |
| One-use state and consume-before-provider-IO | Task packet login contract; spec `REQ-007`, `INV-004` | `callback.rs:130-168`; `login_state.rs` consume transition; refusal and Postgres state-transition tests | PASS |
| Login persistence and audit coupling | Spec `REQ-008`, `REQ-017`; canonical same-transaction audit rules | `callback.rs:201-267,491-509`; `issuance.rs:475-528`; role replacement and completion SQL; audit-failure rollback tests | PASS |
| Active connection and revision serialization | Spec `REQ-014`, `REQ-016`; task replacement/renewal scenario | `HumanConnections` lifecycle methods; `lock_human_connection_slot`; callback bound-connection checks; `issue_human_session:483-523`; provider-switch/cutoff evidence | PASS |
| Refresh-family locking, rotation, and replay durability | R4 `FIND-TASK-002-12`; task requirement to lock family then connection; security posture replay containment | `refresh.rs:88-227`; `refresh_tokens.rs:27-30,96-120,165-192`; route commit `routes.rs:219-258`; deterministic overlap proof `refresh.rs:983-1088` | PASS |
| Migration and legacy refresh behavior | Task migration contract; persistent-data constraints | migrations `20260925000000` and `20260925000001`; `pg_migration.rs` upgrade/preflight coverage | PASS |
| Same-issuer and cross-tenant isolation | Spec `REQ-002`, `REQ-015`, `INV-001`–`INV-004`; forced-RLS authority | tenant-qualified identity/state/refresh storage, global random-state hash uniqueness, definer lookup, RLS transition proof, `same_issuer_two_tenant_isolation_keycloak` | PASS |
| Verification tier and evidence sufficiency | `AGENTS.md` §11; testing-workflows reference; task evidence requirements | four named identity journeys, focused Postgres concurrency proof, migration and OpenAPI tests, recorded principals/SQL/tenant-isolation lanes | PASS, subject to limits below |

## Prior-finding closure

| Prior finding | Current-candidate validation | Result |
|---|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | Stored identity remains tenant-qualified exact `(issuer, subject)`; email does not select or merge identity. | CLOSED |
| `FIND-TASK-002-2` — OIDC `azp` | The tenant callback still enforces multi-audience and present-`azp` binding to the configured client. | CLOSED |
| `FIND-TASK-002-3` — role-change audit | Changed role sets append canonical evidence in the same transaction as role replacement and issuance; append failure rolls back login effects. | CLOSED |
| `FIND-TASK-002-4` — advertised asymmetric algorithm | Advertised-set membership is followed by the shared verifier's asymmetric/signature enforcement. | CLOSED |
| `FIND-TASK-002-5` — duplicated login-state tenant predicates | Purge, consume, complete, and redeem remain forced-RLS transitions without manual tenant-selection predicates. | CLOSED |
| `FIND-TASK-002-6` — raw-pool state resolver | The cross-tenant lookup remains the narrow inherent `WyrdPostgres::login_state_tenant` operation over its private app pool and returns only an optional tenant id. | CLOSED |
| `FIND-TASK-002-7` — printable PKCE verifier | SQL decode wraps the verifier immediately in `SecretString`; redaction coverage remains. | CLOSED |
| `FIND-TASK-002-8` / `9` / `10` — documentation and imports | The corrected item documentation and module-top imports remain intact and do not alter the data path. | CLOSED |
| `FIND-TASK-002-11` — mandatory OIDC claims | R4 routes tenant/platform ID tokens through the claim-complete verifier; the change is data-path neutral outside refusing invalid login before persistence. | CLOSED; independently covered by the security review |
| `FIND-TASK-002-12` — refresh replay/rotation race | `RefreshTokens::execute` resolves the immutable stored family, takes a tenant/kind/principal transaction advisory lock before active/stale classification, and retains it through successor issuance or revocation/audit and route commit. The fixed order is family lock then connection-slot lock. The Postgres test forces ancestor replay to wait behind an open current-token rotation and proves the committed successor is revoked and unusable with one durable containment audit. | CLOSED |

## Material proposed findings

None. No material tenancy, persistence, concurrency, or durability defect was
found in the cumulative candidate.

## Verified properties without findings

- The callback's only cross-tenant database capability accepts a SHA-256 state
  hash and returns only the tenant owning a pending, unexpired row. The random
  state remains the capability; host, path, headers, provider claims, and email
  do not select the effective tenant.
- The tenant-scoped `UPDATE ... consumed_at IS NULL ... RETURNING` is the
  authoritative single-winner state transition. Its transaction commits before
  discovery or code exchange, so a provider failure consumes the attempt and a
  replay cannot repeat provider IO.
- Successful completion resolves or creates the tenant User, replaces mapped
  roles, stages any role-change audit, takes and rechecks the exact Active
  connection revision, issues and audits authority, writes the provenance-bound
  refresh row, seals the completion, and attaches it to the consumed state in
  one transaction. Required audit or persistence failure commits none of it.
- Connection activation, deactivation, removal, login issuance, and refresh
  issuance share the tenant connection-slot lock. A lifecycle mutation either
  precedes and blocks issuance or follows the issuing transaction; every
  successor preserves the original connection id/revision.
- Refresh rotation now serializes the entire stored principal family before
  classifying the token. The lock survives through the route-owned success or
  `Reused` commit, so replay of an ancestor cannot miss a concurrently inserted
  successor. Replay revocation and its canonical audit commit together before
  the refusal is returned.
- The migration intentionally drops transient login state, installs forced RLS
  and the least-disclosure definer function, revokes only live provenance-free
  User refresh rows, preserves machine rows, and removes tenant-email
  uniqueness so a replacement issuer cannot inherit identity through email.
- Tenant-qualified identity uniqueness and forced RLS keep the same issuer and
  subject in two tenants as separate Users, roles, sessions, and refresh
  families.

## Verification limits

- I inspected the complete base-to-candidate changed-file inventory and diff,
  the full relevant source bodies and production callers, both OIDC migrations,
  the focused Postgres and real-server tests, all prior tenancy/data reports,
  validated ledgers, remediation tasks, and the R4 recorded evidence. Candidate
  `HEAD` remained `b57d43d501c136591125b98fe78352e657b093b6` through report creation.
- `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..b57d43d501c136591125b98fe78352e657b093b6`
  completed successfully in this review.
- I did not independently run Cargo, Postgres, Docker, Keycloak/Dex, migration,
  lint, or broad identity lanes because this bounded Wave 1 review shares the
  checkout/target with concurrent reviewers and was directed to avoid costly
  waits. Runtime confidence relies on the inspected tests and the candidate's
  recorded green focused commands and broader lanes.
- The ancestor/current overlap proof observes the replay backend blocked before
  releasing the rotating transaction, then checks committed state from a fresh
  transaction. It is an integration proof of the formerly missing
  interleaving; the public identity journeys continue to cover route-level
  replay, provider replacement, tenant isolation, and audit outcomes.
- TASK-003's BFF completion route and TASK-004's CLI handoff persistence remain
  approved non-goals. This review covers only the server-owned durable
  primitives those tasks consume.

## Overall result

**PASS.** The cumulative candidate preserves forced-RLS tenant isolation,
least-disclosure callback routing, consume-before-provider-IO state semantics,
transactional identity/role/session/audit persistence, exact connection
revision cutoff, migration safety, and same-issuer cross-tenant isolation. The
R4 family-first advisory lock and deterministic Postgres overlap proof close
`FIND-TASK-002-12`; no material finding remains in this domain.

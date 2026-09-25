# Domain review: tenancy, persistent data, migration, and concurrency

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Candidate tree: `4c2e2b73f4dac882f02dd08290bb6ec102bce075`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`

The repository `HEAD` and tree matched the supplied candidate before this
report was written.

## Reviewed boundary and authority coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant SQL capability and RLS | `AGENTS.md` §§2, 3, 9; `architecture/agent-rules.md` raw-pool, `TenantConn`, RLS, and transaction-ownership rules; `architecture/references/doctrine/architecture-constraints.md`; `architecture/wyrd-security-posture.md` | `HumanConnections`, `PgLoginStateStore`, `WyrdPostgres`, human-connection/login-state/refresh query modules, migration, constructors and callers; `check:tenant-isolation`; `check:from-pools-allowlist` | PASS |
| One Active / one Candidate and mutation serialization | Task packet-local connection contract; SPEC REQ-002/REQ-003; security posture tenant isolation | partial unique indexes, forced RLS, tenant transaction advisory lock, revision conflict handling, activation transaction, lifecycle journey source | PASS |
| Transaction ownership and audit atomicity | `AGENTS.md` §§2, 6, 9; agent rules audit and caller-owned transaction rules; architecture audit pattern; SPEC REQ-017 | `HumanConnections::{list,put_candidate,stamp_candidate,activate,deactivate,remove}`, server identity handlers, canonical append ordering, refusal commits | PASS |
| Runtime login/session lifecycle cutoff | Task packet lines 99-108; remediation `FIND-TASK-001-5`; SPEC REQ-016 | login-state binding, callback recheck, `TenantTokenIssuer::issue_human_session`, refresh rotation, slot locking, two-replica cutoff journey | PASS for newly issued sessions |
| Upgrade preservation and legacy-session provenance | Task packet migration contract and lines 99-108; remediation `FIND-TASK-001-5`; SPEC REQ-016 and INV-004 | migration preflight/data move, old issuer delete path, legacy refresh and identity schema, post-migration refresh path, migration tests | **FAIL: TD-R2-001** |
| Tombstones and durable references | Task packet lines 36-37 and 104-106 | tombstone update, connection foreign key, refresh binding, live-row queries | PASS |
| Replica visibility and concurrency | Task packet lines 99-108; architecture deployment topology | shared Postgres reads, per-tenant transaction lock, exact id/revision check under lock, cross-replica journey and controlled in-flight callback source | PASS |

The runtime remediation is coherent. `HumanConnections` now composes the
sanctioned `WyrdPostgres` owner rather than a raw pool. Every lifecycle mutation
and human-session issuance takes the same tenant slot lock. Login state and
refresh successors carry the exact connection id/revision, and issuance checks
that binding while holding the lock. A lifecycle transition therefore either
commits before issuance and causes refusal, or waits until the issuing
transaction finishes. Callees accepting `&mut TenantConn` do not commit or roll
back it.

The migration still has one provenance error that bypasses that otherwise sound
runtime boundary.

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-001-5` / prior `TD-001` — bind login and refresh sessions to the exact Active revision | New login state stores `HumanConnectionBinding`; initial issuance and rotation take `lock_human_connection_slot`, call `human_connection_is_active`, and persist/copy the binding. `tenant_connection_session_cutoff_journey` covers replacement, deactivation, removal, and an in-flight callback across two replicas. | Runtime path **closed**. Cumulative closure is **incomplete** because `TD-R2-001` fabricates that binding for provenance-free legacy refresh rows during upgrade. |
| `FIND-TASK-001-6` / prior `TD-002` — remove raw `PgPool` propagation | `HumanConnections` and `PgLoginStateStore` hold `WyrdPostgres`; their transaction acquisition calls `WyrdPostgres::tenant_conn`; constructors pass that owner. No raw pool remains in the changed auth owner/signatures. | **CLOSED** |

## Material proposed finding

### TD-R2-001 — INCORRECT: migration assigns provenance-free legacy sessions to the current provider

- **Violated obligation:** The task requires replacement, deactivation, and
  removal to block old-connection renewal immediately, requires renewal to
  consult durable Active state, and forbids arbitrary trust selection. The R1
  correction specifically requires every refresh family to remain bound to the
  exact connection selected at login. SPEC REQ-016 requires a session
  established through an old connection to stop renewing after replacement.
- **Exact locations:**
  - `crates/wyrd/wyrd-sql/migrations/20260925000000_auth_human_connections.sql:188-214`
    adds nullable provenance columns, then assigns the newly migrated Active
    connection to every unrevoked `user` refresh row in the tenant.
  - The pre-upgrade schema at
    `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:183-197`
    records no issuer or connection on a refresh row, while
    `20260601000010_auth_federated_identity.sql:3-12` records the user's issuer
    separately but not which identity produced a particular refresh family.
  - The pre-upgrade trusted-issuer delete route in the base candidate path
    (`crates/wyrd/wyrd-server/src/components/admin/routes.rs`, base lines
    447-490) deletes an issuer without revoking its users' refresh families.
- **Evidence and reachability:** Before upgrade, a tenant can sign a user in
  through Human issuer A, retain the resulting unrevoked refresh family, delete
  A, and configure Human issuer B. The migration preflight sees exactly one
  Human issuer and succeeds. Its unconditional update then labels A's refresh
  row with B's new connection id/revision. After upgrade,
  `RefreshTokens::execute` copies that fabricated binding and
  `issue_human_session` sees B as Active, so the old A session renews. The
  existing upgrade test seeds no refresh row and cannot detect this path.
  Joining to a current user identity is not sufficient in the general case:
  the legacy row itself has no family-level issuer provenance.
- **Observable consequence:** Upgrading can grant a session authenticated by a
  retired provider continued renewal authority under a replacement provider it
  never authenticated against. The old principal's roles and history continue
  even though provider replacement is required to cut that session off.
- **Required testable correction:** Do not infer a legacy refresh family's
  connection from whichever Human issuer exists at migration time. Leave
  provenance-free legacy human rows unbound (or revoke them transactionally) so
  the existing fail-closed refresh path requires re-login; bind only rows for
  which exact family-level connection provenance actually exists. Do not infer
  provenance from tenant, email, current issuer, or user identity. Extend the
  pre-migration test with an unrevoked user refresh family originating before
  issuer A was replaced by B, apply the migration, and prove it cannot rotate or
  produce a successor under B. Also assert a fresh post-migration login through
  B does bind and renew normally.

## Verification limits

- I inspected the complete cumulative base-to-candidate diff, the remediation
  diff, governing authorities, full affected migration/query/session bodies,
  and every production caller of the lifecycle SQL slots.
- `mise run check:tenant-isolation` exited zero.
- `mise run check:from-pools-allowlist` exited zero, while still printing its
  known `rg: python/: No such file or directory` diagnostic. Source inspection,
  not that check alone, establishes the raw-pool closure.
- I did not rerun the long Postgres/Keycloak identity journey or migration
  suite within this domain-review budget. The committed journey source directly
  covers new-session cutoff, but no committed test covers legacy refresh
  provenance during migration.
- No other Wave 1 report or current-review conclusion was used as an input.

## Overall result

**FAIL**

The candidate closes the prior raw-pool and live-session race defects for every
new session. `TD-R2-001` remains a reachable migration path that converts an
old-provider refresh family into a current-provider family without evidence,
so cumulative replacement cutoff and upgrade durability do not yet satisfy the
task.

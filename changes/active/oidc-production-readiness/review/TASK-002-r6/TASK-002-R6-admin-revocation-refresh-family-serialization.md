---
id: TASK-002-R6
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-016, INV-004, AC-007]
depends_on: [TASK-002-R5]
parent_task: TASK-002
remediates: [FIND-TASK-002-14]
---

# Serialize administrative User revocation with refresh rotation

## Authority and immutable subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r6/verdict.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `0ca117a744ddcb7414b104c4382027970531b608`
- Validated finding: `FIND-TASK-002-14`

This remediation closes one omitted refresh-family mutation caller without
changing the approved specification, public auth contract, persistence model,
or accepted R1–R5 behavior.

## Issue diagnosis

### FIND-TASK-002-14 — Administrative User revocation can miss a concurrent refresh successor

TASK-002's renewal contract and R4 require operations on one stored tenant
principal refresh family to serialize before classification or family-wide
mutation and retain that serialization through commit. The administrative
User-revocation owner at `crates/wyrd/wyrd-auth/src/revoke.rs:43-56` promises
that User suspension and refresh-family retirement commit together, but it
calls the family-wide revocation update without the existing family lock.

The competing production rotation at
`crates/wyrd/wyrd-auth/src/refresh.rs:121-218` does take that lock, consumes
current row `B`, validates the active human connection, and inserts successor
`C` before its caller commits. Administrative revocation can start its family
update while `C` is uncommitted. The update may wait on `B` and resume after
rotation commits, but `C` was absent from its statement snapshot and need not
be targeted. The live principal-revocation route can therefore commit User
suspension and report family retirement while `C` remains unrevoked. Later
reactivation could make that missed renewable credential usable again.

The candidate's sequential revocation proof and R4 ancestor-replay overlap
proof do not exercise this interleaving. The gap is reachable through the
production User-revocation route and is distinct from the already-correct
replay containment caller.

## Intended correction outcome

Administrative User revocation and current-token rotation participate in the
same tenant-qualified refresh-family serialization. Once administrative
revocation commits, every row in that User's family, including a successor
created by an overlapping rotation, is durably revoked and no renewable row
survives.

## Decision-complete recommendation

Reuse the existing `lock_refresh_family` capability in the existing User branch
of `revoke_principal_in_conn`. After the existing User-existence check and
before suspension or family revocation, acquire the lock for that tenant User
family. Keep the existing `TenantConn`; the server route already owns the
transaction and commit, so the lock naturally remains held through both
durable effects and their audit boundary.

Preserve the established family-before-connection lock order. Administrative
revocation requires no connection lock. Do not move the lock into a new owner
or add another abstraction, table, lease, retry protocol, mutex, transaction
isolation mode, dependency, or public contract. This is the smallest root-cause
correction because the repository already has the exact shared lock required;
the defect is one omitted caller.

## Constraints and preserved behavior

- Keep all tenant work on `TenantConn` with caller-owned commit and rollback.
- Preserve forced RLS, canonical transactional audit, User suspension,
  `principal_revoked` family state, human-connection provenance, replay
  containment, and the five-minute access-token snapshot.
- Preserve the family-before-connection lock order used by refresh rotation.
- Preserve Service and Agent revocation behavior; they do not own human refresh
  families and must not acquire this lock.
- Preserve the closures of `FIND-TASK-002-1` through
  `FIND-TASK-002-13`, public HTTP/OpenAPI/schema contracts, generated artifacts,
  and all four TASK-002 identity journeys.

## Non-goals

- No new persistence object, family identifier, lock service, trait, retry
  loop, isolation-level change, process-local synchronization, or dependency.
- No public API, error, migration, schema, generated-artifact, or token-lifetime
  change.
- No TASK-003 BFF completion, TASK-004 CLI persistence, provider work, email
  linking, or machine-identity redesign.
- No unrelated refresh, revocation, transaction, or test-harness refactor.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| Administrative User revocation acquires the existing tenant-qualified refresh-family lock after confirming the User exists and before suspension or family-wide revocation, and retains it through the route-owned commit. | `FIND-TASK-002-14` |
| When current-token rotation overlaps administrative User revocation, revocation waits for the family owner and the terminal committed state contains no active successor; the inserted successor is revoked as `principal_revoked`. | `FIND-TASK-002-14` |
| Existing replay/rotation serialization, family-before-connection order, audit coupling, Service/Agent revocation, connection cutoff, and prior remediation behavior remain unchanged. | `FIND-TASK-002-14` |

## Focused proof and broader verification

Add one deterministic Postgres test named
`revoke::pg_tests::refresh_rotation_overlapping_user_revocation_retires_successor`
using the existing production refresh and principal-revocation owners. It must
overlap an open current-token rotation with administrative User revocation,
prove revocation cannot pass the family lock prematurely, complete both
transactions, and verify from a fresh transaction that the successor is
`principal_revoked` and no active family row remains.

Run the exact focused test with the repository-managed Postgres setup:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=revoke::pg_tests::refresh_rotation_overlapping_user_revocation_retires_successor)'"
```

Then run the narrow owning and regression lanes:

```bash
mise run test:principals:unit
mise run test:sql
mise run test:identity:journey
mise run check:tenant-isolation
mise run fmt
mise run lints
git diff --check
```

If closing the finding requires a new public contract, persistence model,
lock abstraction, transaction-isolation decision, or broader concurrency
semantic, stop and route that decision through specification revision rather
than expanding this remediation.

---
task: TASK-006
title: Credential lifecycle and administrative recovery
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-007, REQ-009, REQ-010, REQ-018, REQ-032, REQ-033, REQ-036, REQ-037, INV-002, INV-008, INV-009, INV-013, AC-005, AC-006, AC-010]
depends_on: [TASK-005]
---

## Objective

Administrative credentials are safely maintainable over time: create, list
metadata, revoke, and rotate by overlap, with no window in which a principal
holds no usable credential and no reconstruction of its authorization. When
every credential of a tenant administrative principal is lost, a global
administrative principal issues a replacement against that same principal,
restoring programmatic tenant administration without creating a second
administrative principal or gaining any further tenant access.

## Constraints

- No surface returns an existing credential's plaintext. Listing returns
  metadata only.
- Recovery issues against the existing tenant administrative principal. It never
  creates a second administrative principal, alters role grants, or widens the
  global principal's tenant access.
- Recovery is an explicitly named, separately authorized capability,
  distinguishable in audit from ordinary tenant administration — it is the
  named exception of `REQ-018`, not implicit platform authority over tenant
  resources.
- Revocation advances the applicable authorization epoch transactionally; no
  token or cached permission set outlives the earlier of its expiry or that
  epoch.
- Global-administrator credential loss remains deployment-level recovery through
  operator access to the database and secret store, using the same
  initialization-class authorization as `TASK-003`. No application-level
  self-service path.
- Every decision on this surface appends its audit row in the deciding
  transaction, naming principal, credential, permission, resource, tenant, and
  outcome. An unrecordable audit refuses.
- Non-goal: a credential-management UI, credential sharing, or delegation
  changes.

## Relevant Surface

- `crates/wyrd/wyrd-auth/src/issue_api_key.rs`, `revoke.rs`,
  `revocation_resolver.rs`, `revocation_listener.rs`.
- `crates/wyrd/wyrd-server/src/components/` — credential and recovery routes on
  both planes.
- `crates/wyrd/wyrd-sql/src/queries/auth/` — credential lifecycle queries and
  the credential issuance audit table.
- `crates/wyrd/wyrd-cli/src/auth/` — credential commands.

## Approach

1. Expose credential creation, metadata listing, revocation, and rotation for
   every principal permitted to hold a credential, on the plane that owns it.
2. Make rotation an overlap operation with no interruption, reusing the epoch
   machinery so revocation of the superseded credential takes effect
   immediately.
3. Add the platform-plane recovery operation that issues against an existing
   tenant administrative principal, named and audited distinctly.
4. Couple each decision to its transactional audit append, recording which
   credential authenticated the request.
5. Project the operations through the CLI.
6. Document credential rotation and credential-loss recovery, including the
   deployment-level path for the global credential.

## Acceptance Criteria

- A principal's credentials can be listed as metadata; no path returns an
  existing plaintext.
- Rotation — issue B, verify B, revoke A — leaves administration uninterrupted,
  stops A immediately, and leaves the principal's authorization unchanged.
- Revoking one credential leaves the principal's other credentials working.
- A tenant administrative principal with zero usable credentials is restored by
  a global administrative principal against the same principal id, with
  unchanged grants and no second administrative principal created.
- Recovery appears in audit as a distinct named capability, attributable to the
  global principal and the credential that authenticated it.
- Recovery grants the global principal no further access: an attempt to read or
  mutate that tenant's resources with the global credential is still refused.
- A revoked credential's live tokens stop verifying at the epoch.
- An injected audit-append failure refuses the operation and commits nothing.

## Verification

Scope is `VER-001` through `VER-006`.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-auth -p wyrd-server -p wyrd-cli --all-targets
mise run codegen:check
mise run docs:check
```

The primary proof is a real-server journey covering overlapping rotation with
uninterrupted administration, and a recovery journey that drives a tenant
administrative principal to zero usable credentials and restores it from the
platform plane. Epoch, metadata-only listing, and audit-failure coverage runs as
Postgres-backed integration proof through
`scripts/postgres/with-test-postgres.sh`. Run the capability's focused `mise`
lane and the exact nextest expressions for the tests you add.

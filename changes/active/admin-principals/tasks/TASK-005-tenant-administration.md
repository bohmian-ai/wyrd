---
task: TASK-005
title: Tenant administration and service principals
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-029, REQ-030, REQ-031, REQ-036, REQ-037, INV-007, INV-004b, AC-004, AC-008, AC-012]
depends_on: [TASK-004]
---

## Objective

A tenant administrative principal completes its tenant's configuration with no
platform-plane involvement and no human identity: tenant configuration including
its OIDC settings, creation of tenant-scoped principals, and grants of existing
roles to them. A tenant administrator can create restricted machine principals
for automation, so tenant tooling never shares the tenant administrative
credential. Every tenant-scoped operation verifies that its target belongs to
the authenticated principal's tenant.

## Constraints

- Tenant data access runs under `TenantConn` RLS. No hand-written tenant filter,
  no widened query, no cross-tenant read without the operator boundary.
- A tenant administrator grants only roles valid for its own tenant. Unknown,
  ambiguous, or unauthorized grants fail closed.
- No tenant-plane operation — principal creation, role grant, or OIDC group
  mapping — may create or elevate a platform principal or confer platform
  authority.
- Restricted machine principals use the existing role and object-scoped
  permission model. No new grant model, no explicit deny, no ownership
  semantics.
- Every authorization decision appends its audit row in the deciding
  transaction, allowed and denied alike, naming principal, credential,
  permission, resource, tenant, and outcome. An unrecordable audit refuses.
- Denials never reveal another tenant's existence, names, principal inventory,
  or configuration.
- Non-goal: credential issuance, rotation, revocation, and recovery surfaces —
  owned by `TASK-006`.
- Non-goal: human principals and OIDC login — owned by `TASK-007`.
- Non-goal: a customizable role editor.

## Relevant Surface

- `crates/wyrd/wyrd-server/src/components/admin/routes.rs` — the tenant-admin
  surface, including its current deliberate no-audit stance, which this task
  corrects.
- `crates/wyrd/wyrd-server/src/components/auth/audit_writer.rs`.
- `crates/wyrd/wyrd-auth/src/roles.rs`, `service_accounts.rs`,
  `permission_resolver.rs`.
- `crates/wyrd/wyrd-sql/src/queries/auth/` — tenant-scope principal and grant
  queries.
- `crates/wyrd/wyrd-cli/src/principal/` — tenant principal commands.

## Approach

1. Put the tenant-plane administrative operations behind the tenant-scope
   authenticated context and the tenant administrative permissions.
2. Implement tenant principal creation and role granting for non-Card-bound
   machine principals inside the tenant boundary.
3. Couple every decision on this surface to its transactional audit append and
   correct the module's recorded no-audit stance.
4. Ensure every operation resolves its target within the authenticated tenant
   and fails closed otherwise.
5. Project the operations through the CLI.

## Acceptance Criteria

- A tenant administrative credential configures its tenant, creates a
  tenant-scoped machine principal, and grants it a narrower role set, with no
  platform-plane call.
- The restricted principal's credential performs its granted operations and is
  refused tenant administration.
- A tenant A credential cannot read, mutate, or authenticate against tenant B's
  principals, roles, or configuration; denial reveals nothing about tenant B.
- No tenant-plane operation, including role grant and OIDC group mapping, can
  create a platform principal or confer platform authority.
- Each covered decision appends its audit row in the deciding transaction for
  allowed and denied outcomes; an injected audit-append failure refuses the
  operation and commits nothing.
- A suspended tenant or principal is refused on every operation on this surface.

## Verification

Scope is `VER-001` through `VER-006`.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-auth -p wyrd-server -p wyrd-cli --all-targets
mise run codegen:check
```

The primary proof is a real-server journey in which a tenant administrative
credential configures its tenant and creates a restricted machine principal that
then acts within its grants and is refused outside them. Cross-tenant and
escalation negative coverage, and the injected audit-failure refusal, run as
Postgres-backed integration proof through
`scripts/postgres/with-test-postgres.sh`. Run the capability's focused `mise`
lane and the exact nextest expressions for the tests you add.

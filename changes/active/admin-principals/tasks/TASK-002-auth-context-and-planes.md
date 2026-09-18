---
task: TASK-002
title: Authenticated context and authorization planes
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-012, REQ-012a, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-031, INV-004, INV-004a, INV-011, INV-013, AC-003]
depends_on: [TASK-001]
---

## Objective

One authentication pipeline with two entry paths — machine credential exchange
and human OIDC login — both minting Wyrd tokens carrying the same principal
representation. The per-request path derives a closed two-variant authenticated
context from verified token claims alone: platform scope, or exactly one tenant.
A platform identity is not representable where a tenant identity is required, so
a platform handler cannot reach tenant data. The permission vocabulary gains the
platform and tenant administrative operations, and every protected operation
authenticates, authorizes, audits, and only then executes.

## Constraints

- The request path reads only verified token claims, subject to the existing
  authorization epoch. Never a credential record, lookup prefix, provider token,
  request header, path, hostname, or body.
- No handler may branch on which entry path minted the token.
- Platform authority never confers tenant data access. Any deliberate
  global-administrator access to tenant resources is a separately named,
  separately authorized, audited capability, not an implicit consequence of
  platform scope.
- Authorization uses the existing role-derived `Permission` vocabulary and the
  existing synchronous checker. No second grant store, cache, or checker.
- Tenant data access continues under `TenantConn` RLS with no hand-written
  tenant filters and no widened queries.
- Non-goal: platform OIDC, human platform principals, initialization, tenant
  provisioning, credential management routes.
- Non-goal: customizable roles. Administrative permissions may stay internal.

## Relevant Surface

- `crates/shared/wyrd-runtime/src/permission.rs`, `permission_check.rs`,
  `request_context.rs`, `builtin_roles.rs`.
- `crates/shared/wyrd-auth-check/` — guard, context, hook.
- `crates/shared/wyrd-auth-verify/`, `crates/shared/wyrd-auth-issue/`.
- `crates/wyrd/wyrd-server/src/components/auth/` — principal extractor, caller
  extractor, token extract, policy hook, audit writer.
- `crates/wyrd/wyrd-auth/src/exchange_api_key.rs`, `permission_resolver.rs`,
  `revocation_resolver.rs`, `roles.rs`.
- `crates/wyrd/wyrd-server/tests/auth_e2e.rs`,
  `crates/wyrd/wyrd-server/tests/pg_authz_check_route.rs`.

## Approach

1. Define the two-variant authenticated context and the plane-typed extractors
   that produce it from verified claims.
2. Route machine credential exchange through the principal-generic resolution
   from `TASK-001`, and confirm the existing human callback path mints a token
   with the same principal representation.
3. Extend the permission vocabulary with the platform-plane and tenant-plane
   administrative operations the specification names.
4. Replace remaining credential-keyed or API-key-keyed authorization with
   context-based authorization at every authenticated handler.
5. Couple each authorization decision to its transactional audit append, failing
   closed when the audit cannot be recorded.
6. Prove plane separation and tenant-boundary enforcement on an existing
   protected route before any new administrative route exists.

## Acceptance Criteria

- Any valid machine credential produces a token whose verification yields an
  authenticated context carrying principal identity, principal type, and exactly
  one scope; no other path reaches application authorization.
- A tenant-scope token presented to a platform-plane extractor is refused with a
  stable error, and a platform-scope token presented to a tenant-plane extractor
  is refused. Neither refusal reveals another tenant's existence or inventory.
- A platform-scope context cannot open a tenant connection; this is prevented by
  construction rather than by a runtime check a caller could omit.
- Tenant identity in the context derives only from verified claims. A request
  supplying a different tenant in a header, path, body, or hostname is
  unaffected in outcome.
- An authorization decision appends its audit row in the deciding transaction
  for allowed and denied outcomes alike; an unrecordable audit refuses the
  operation and commits nothing.
- Revoking a credential, principal, or role grant advances the applicable
  authorization epoch transactionally, and a token issued before that epoch
  stops verifying.
- No authenticated handler retains authorization logic keyed on credential
  material.

## Verification

Scope is `VER-001` through `VER-006`. No broad aggregates; failures outside the
authentication, authorization, and audit surfaces are out of scope.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-runtime -p wyrd-auth -p wyrd-auth-check -p wyrd-auth-verify -p wyrd-server --all-targets
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib
mise exec -- cargo nextest run --locked -p wyrd-auth --lib
```

Real-server plane-separation and epoch coverage runs through the existing
server test surfaces (`auth_e2e`, `pg_authz_check_route`) under the
repository-managed Postgres wrapper. Run the focused expressions for the tests
you add or change.

`mise run codegen:check` when the error catalog or permission contract moves.

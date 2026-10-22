---
id: SPEC-tenant-custom-roles
revision: 1
status: draft
---

# Tenant-defined custom roles

## Intent

Let a tenant administrator define roles narrower or differently shaped than the
four built-in roles (`viewer`, `workload`, `editor`, `admin`), so a principal
can hold exactly the permissions its job needs. Today the only way to narrow
access is to pick a lower built-in role. This backlog draft does not authorize
implementation.

## Current state

The storage and evaluation layers already support custom roles; no public
surface creates one.

- `wyrd.auth_roles` stores tenant-scoped roles with a `builtin` flag, under
  tenant row-level security. A trigger
  (`20260601000004_auth_builtin_role_immutability.sql`) keeps built-in role
  names immutable.
- `wyrd_sql::queries::auth::{insert_role, delete_role, list_roles}` exist, but
  only tests call `insert_role`.
- The permission model already has object-scoped grants intended for custom
  roles, such as `verifier:run` on one Verifier UID and `gateway:invoke` on one
  provider or model (`crates/shared/wyrd-runtime/src/permission.rs`).
- `wyrd principal role list|grant|revoke` and the SDK `Principals` handles grant
  existing roles only. There is no HTTP route, CLI command, MCP tool, or SDK
  method to create, update, or delete a role.

## Required behavior

- **REQ-001 — Create.** A tenant administrator creates a named role with a set
  of permissions through HTTP, the CLI, MCP (as a scoped write tool), and the
  Rust, Python, and TypeScript SDKs.
- **REQ-002 — Validated permissions.** Every permission in a role is a
  `resource:action` the server recognizes, optionally narrowed to one object
  where that permission supports a scope. An unknown resource, action, or scope
  is refused with a stable error that names the offending entry. Nothing is
  stored on refusal.
- **REQ-003 — Read.** Any principal that can list roles sees custom roles
  alongside built-in roles, with each role's permissions and whether it is
  built-in.
- **REQ-004 — Update.** An administrator replaces a custom role's permission
  set. The change reaches holders when their access tokens are next issued,
  within the existing 5-minute token lifetime.
- **REQ-005 — Delete.** An administrator deletes a custom role. The behavior
  when principals still hold it is an open decision (below).
- **REQ-006 — Grant.** Custom roles are granted and revoked through the existing
  `wyrd principal role grant|revoke` and `Principals` grant and revoke methods,
  and can be the target of identity-provider group mappings.
- **REQ-007 — Audit.** Creating, updating, and deleting a role are permission
  decisions recorded in `vala.system.audit_log` like every other administrative
  action.

## Constraints and decisions

- **INV-001:** Built-in roles cannot be created, renamed, modified, or deleted
  through this surface, and a custom role cannot take a built-in name.
- **INV-002:** Roles stay tenant-scoped. A role, its permissions, and its grants
  are never visible to or usable by another tenant.
- **INV-003:** Role management requires the `admin` role, matching role grant
  and revoke. A custom role cannot be used to manage roles unless it carries the
  same administrative permission.
- **INV-004:** Permission evaluation is unchanged: a principal's permissions are
  the union of its roles, and delegation still takes the intersection.

## Acceptance

- **AC-001:** A user-journey test per surface creates a role narrower than
  `workload`, grants it to a Card-bound service principal, and proves an allowed
  write succeeds and an operation outside the role is refused with
  `WYRD_PERMISSION_403_DENIED_RBAC`.
- **AC-002:** Invalid permissions, built-in name collisions, and non-admin
  callers are refused with stable errors and store nothing.
- **AC-003:** Updating a role changes the holder's effective permissions after
  token renewal; the audit log records the create, update, and delete
  decisions.
- **AC-004:** A tenant-isolation check proves one tenant cannot list, grant, or
  modify another tenant's custom role.

## Open material decisions

- **Deleting a role in use.** Refuse while any principal or group mapping holds
  it, or revoke it from every holder in the same transaction.

## Authority and revision

- `AGENTS.md`; `architecture/wyrd-design.md`;
  `architecture/wyrd-security-posture.md`.
- Docs: `docs/src/content/docs/understand/identity-and-authorization.md`
  currently states that the four built-in roles are the only roles.

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial backlog draft. |

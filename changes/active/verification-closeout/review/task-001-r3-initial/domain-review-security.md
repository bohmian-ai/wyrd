# Security domain review

**Boundary reviewed:** Tenant principal discovery and Role assignment; token issuance and refresh; Card attribution through Gate, Scribe, OTLP, and verification. Reviewed the complete `c46afdcac..437205debc628538ba6aa4ec828601c7c40145b4` diff against spec revision 2, TASK-001, `AGENTS.md`, agent rules, Wyrd design, and security posture. Candidate HEAD matched the supplied commit.

## Critical

None.

## High

None confirmed.

## Medium

- **SEC-001 — `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:110`** — The candidate changes an already applied migration to add `auth_user_roles.source`, but supplies no forward migration. Existing deployments retain the old schema and checksum. The serving schema check at `crates/wyrd/wyrd-sql/src/schema_check.rs:76` rejects the changed checksum; if that check is bypassed, login synchronization and direct user grants reference a missing column. **Impact:** existing tenants cannot upgrade or use the new principal flow. **Fix:** restore the historical migration unchanged and add a forward migration that adds the source column, migrates existing assignments, and changes the key safely. Prove upgrade from an applied prechange database, not only fresh database creation. **Classification:** REGRESSION; violates the local and signed-in workflow requirement.

- **SEC-002 — `crates/wyrd/wyrd-auth/src/seed.rs:24`** — Existing tenant Role rows are left untouched by `ON CONFLICT DO NOTHING`, while the candidate replaces the built-in catalog with four Roles. No migration removes or translates retired built-ins or updates their assignments. **Impact:** after the schema upgrade is repaired, an existing principal assigned the old `agent` Role can continue receiving its `cards:write` permission in newly minted tokens, despite the new `workload` Role withholding authoring. Retired Role names also remain grantable. **Fix:** include existing tenant Role and assignment transition in the forward migration, preserving custom Roles and intended administrator grants; verify newly issued tokens and the assignable Role catalog after upgrade. **Classification:** VIOLATION of REQ-002 and its exact four-Role contract.

## Low / defense in depth

None within the approved task.

## Positive controls and verification limits

- Principal discovery and Role changes use `TenantConn`; target kind is resolved from tenant scoped SQL rather than caller input.
- Role writes require wildcard administration and stage allowed or denied decisions on the audit outbox before mutation.
- IdP replacement is source scoped, while token issuance reads distinct effective Roles.
- Unbound Card attribution resolves registered observation targets through a tenant scoped registry lookup; Scribe still validates each row against the resolved scope.
- The candidate's reported principal, OTLP, and SDK journeys passed. Those tests use fresh schemas and do not establish upgrade behavior for an already applied migration or existing Role rows.

**Overall: FAIL.** The security and migration findings require an upgrade path and existing tenant proof.

Skipped: I did not run tests or edit files; this was a static review.

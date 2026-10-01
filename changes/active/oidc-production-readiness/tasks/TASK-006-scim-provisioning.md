---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-oidc-production-readiness
spec_revision: 6
requirements: [REQ-019, INV-001, INV-002, INV-003, INV-005, INV-007, AC-010]
depends_on: [TASK-005]
---

# Tenant SCIM provisioning

## Outcome and Value

A customer's existing identity system provisions and removes Wyrd tenant access without a separate Wyrd user-management process. SCIM changes use Wyrd's existing tenant User, roles, suspension, refresh revocation, and canonical audit authorities.

## Owners, Scope, Consumers, and Prohibited Changes

The Rust server owns the tenant-bound SCIM 2.0 API and durable identity changes. A tenant administrator authorizes provisioning setup and maps SCIM Groups to existing Wyrd roles. The SCIM caller authenticates with an existing tenant API key restricted to provisioning; the endpoint verifies it directly using Wyrd's existing key authority because SCIM clients need not perform Wyrd's JWT exchange. It cannot select a different tenant in a URL, header, or resource body. The server's existing User/role/revocation/audit owners perform the mutations through `TenantConn`. No raw SQL pool, second principal store, second role mapper, password login, email linking, platform authority, or new session issuer.

When SCIM is enabled, only provisioned active users whose client-provided `externalId` equals the verified OIDC subject or persistent SAML NameID can sign in. SCIM Groups, not sign-in group claims, govern mapped roles for those users. Enabling SCIM must refuse incompatible existing identities until an authorized, explicit migration resolves them. Disabling SCIM must not silently activate suspended users or grant roles.

## Approach

1. Expose standard SCIM 2.0 User, Group, and required discovery resources behind a tenant-bound provisioning credential using the existing server auth and audit boundaries.
2. Bind a SCIM `externalId` to one tenant User, enforce idempotent create/update semantics and explicit active/inactive state, and reject duplicate or ambiguous identities.
3. Apply Group membership through the existing tenant role authority and use existing User suspension plus refresh-family revocation for offboarding.
4. Make human login consult the same active provisioned identity and mapped roles when the tenant enables SCIM, without email fallback or a second role-sync path.
5. Document the customer's provisioning setup and qualify the claimed SCIM operations against a controlled enterprise IdP before advertising support.

## Ordered Implementation Scenarios

### Scenario 1 — Provision and authorize a user

**Behavior.** A tenant-scoped SCIM client creates or updates a User and Group; the verified matching sign-in subject receives only that tenant's mapped Wyrd roles. A repeated provisioning request does not create another User.

**RED.** Add a real-server SCIM-client-to-Wyrd journey through the public API and a subsequent sign-in; observe missing provisioning operations or authority link.

**GREEN.** Implement the required SCIM resources and connect them to the existing User and role owners. Re-run the full journey.

**REFACTOR.** Keep protocol parsing at the SCIM boundary and reuse existing tenant mutations and audit; remove duplicate identity or role handling.

### Scenario 2 — Offboard and isolate

**Behavior.** Suspension or removal blocks new human issuance and revokes renewable credentials; an already-issued access token expires within its existing bound. A credential or resource from tenant A cannot inspect or change tenant B. Email matches, missing `externalId`, duplicate identities, and unprovisioned login grant no membership.

**RED.** Extend the real-server journey with suspension, refresh, login, wrong-tenant, and identity-collision attempts; observe any successful grant or cross-tenant mutation.

**GREEN.** Route offboarding through the existing User revocation transaction and enforce the tenant-bound immutable identity check in provisioning and login.

**REFACTOR.** Keep one revocation path, one audit append, and scoped SQL capabilities while all negative cases remain green.

## Acceptance Criteria

Standard SCIM User/Group and required discovery operations support the documented IdP setup; repeated operations are idempotent and tenant isolated. SCIM-managed sign-in requires a provisioned active subject. Group changes take effect on the next Wyrd issuance, and suspension prevents renewal. Every authorization and mutation decision uses canonical audit. No SCIM value grants platform authority or bypasses tenant RBAC. A controlled IdP qualification records the exact operations and configuration claimed as supported.

## Expected Write Set and Consumer Closure

Likely server SCIM routes and auth integration, `wyrd-spec` wire/error contracts, existing User/role/revocation SQL owners, tenant administration, real-server journeys, generated schemas, and setup documentation. Paths are guidance, not an implementation allowlist.

## Verification and Evidence

Run an exact focused SCIM journey command once the owning test target and selector are added and confirmed with `mise exec -- cargo nextest list --locked -p wyrd-server --test <target>`; do not invent a selector in this proposed task. Run the relevant `mise` identity/principals integration and boundary checks, `mise run codegen:check`, `mise run fmt`, and `mise run lints`. Record one controlled-IdP provisioning/deprovisioning run before publishing a provider support claim. Include negative tenant, identity-link, replay/idempotency, audit-failure, and refresh-revocation evidence.

## Material Stop Conditions

Do not implement until revision 6 is approved. Return to the spec if a customer IdP cannot send a stable `externalId` equal to the verified sign-in subject, if supporting existing Users requires implicit linking, or if a new security credential/role authority would be required.

## Authority Links

[Draft spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [security posture](../../../../architecture/wyrd-security-posture.md); [SCIM protocol](https://www.rfc-editor.org/rfc/rfc7644.html); [SCIM schema](https://www.rfc-editor.org/rfc/rfc7643.html).

---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-oidc-production-readiness
spec_revision: 6
requirements: [REQ-020, INV-001, INV-002, INV-003, INV-005, INV-007, AC-011]
depends_on: [TASK-005]
---

# Tenant SAML SSO

## Outcome and Value

An enterprise whose existing identity provider requires SAML can sign in to Wyrd while Wyrd continues to own one tenant User, role, credential, browser/CLI handoff, revocation, and audit path.

## Owners, Scope, Consumers, and Prohibited Changes

The Rust server is the SAML service provider and owns tenant connection administration, SP-initiated request state, assertion validation, and verified identity handoff. A tenant chooses OIDC or SAML as its one active human connection; the existing browser and CLI consumers still receive only Wyrd credentials. Use a maintained SAML implementation for XML parsing and signature validation. Do not write custom XML signature code or add a second User store, role engine, session issuer, audit sink, raw SQL pool path, platform fallback, email linking, or IdP assertion as Wyrd API authority.

Initial delivery is SP-initiated only. IdP-initiated unsolicited login, Single Logout, and assertion encryption are outside scope unless a qualified target IdP makes encryption necessary for the approved support claim. The verified persistent NameID is the subject; a transient NameID or ambiguous identity is refused. SCIM-managed tenants additionally require an active provisioned `externalId` matching that subject.

## Approach

1. Add tenant-owned SAML connection setup with IdP metadata, Wyrd SP metadata, exact public ACS URL, and bounded signing-key rotation under the existing one-active-connection lifecycle.
2. Bind tenant, connection, request ID, destination, and expiry in server-owned one-use state before redirecting to the customer's IdP.
3. Validate the SAML response and assertion under the configured tenant trust, then pass the verified issuer and persistent subject into the existing tenant User, role, Wyrd issuance, and audit owners.
4. Complete browser and CLI initiation through their existing sealed, one-time credential handoffs and document the customer's IdP setup.
5. Qualify the publicly claimed provider configuration over trusted TLS and record refusal evidence for malformed or mismatched assertions.

## Ordered Implementation Scenarios

### Scenario 1 — Enterprise SAML login

**Behavior.** A user starts from one tenant's Wyrd login entry, authenticates at its configured IdP, and receives only tenant-bound Wyrd authority through the existing browser or CLI handoff. OIDC tenants and machine principals continue to work.

**RED.** Add a real-server SP-initiated SAML journey with an allowed and denied Wyrd call; observe missing connection/request/ACS flow.

**GREEN.** Implement the SAML protocol boundary and feed only verified identity to the existing User and issuance owners; rerun OIDC and machine journeys.

**REFACTOR.** Reuse the existing connection lifecycle, credential handoff, role, and audit mechanisms; keep only the protocol-specific parser and validation distinct.

### Scenario 2 — Assertion and tenant refusal

**Behavior.** Invalid signature, issuer, audience, recipient, destination, expiry, request binding, replay, transient NameID, inactive connection, or wrong tenant issues no Wyrd authority and cannot consume another tenant's request state.

**RED.** Extend the real-server journey with those tampered and cross-tenant responses; observe any accepted assertion or leaked authority.

**GREEN.** Validate each binding at the SAML trust boundary and consume one-use state before issuance.

**REFACTOR.** Keep one assertion validation boundary and one Wyrd issuance path; avoid per-handler duplicate guards.

## Acceptance Criteria

One tenant can use SAML while another uses OIDC on the same hosted server; a self-hosted tenant can use SAML with the same server build. Only the configured IdP and exact SP request/ACS binding can establish a User. Verified persistent identity maps to existing tenant roles; SCIM-managed tenants additionally enforce provisioned active membership. Browser and CLI receive Wyrd credentials only. Audit, provider replacement, key rotation, and old-session renewal cutoff retain existing semantics. The documented provider and topology claim has controlled live evidence.

## Expected Write Set and Consumer Closure

Likely server auth and connection owners, `wyrd-spec` connection/wire types, tenant SQL state, BFF/CLI initiation, identity journeys, generated schemas, and setup documentation. Paths are guidance, not an implementation allowlist.

## Verification and Evidence

Run an exact focused SAML journey command once the owning test target and selector are added and confirmed with `mise exec -- cargo nextest list --locked -p wyrd-server --test <target>`; do not invent a selector in this proposed task. Run the relevant `mise` identity, browser, CLI, principals, contract, tenant-isolation, format, and lint gates. Record a controlled-IdP TLS run for each publicly claimed provider/topology. Verify all named assertion failures, OIDC regression, machine independence, and one active connection across two hosted tenants.

## Material Stop Conditions

Do not implement until revision 6 is approved. Return to the spec if a target IdP requires IdP-initiated login, Single Logout, assertion encryption, transient NameID, or account linking by email for the public support claim; do not silently broaden this task.

## Authority Links

[Draft spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [security posture](../../../../architecture/wyrd-security-posture.md); [SAML browser SSO](https://docs.oasis-open.org/security/saml/v2.0/saml-profiles-2.0-os.pdf); [SAML metadata](https://docs.oasis-open.org/security/saml/v2.0/saml-metadata-2.0-os.pdf).

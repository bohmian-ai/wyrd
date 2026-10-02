---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 9
requirements: [REQ-001, REQ-005, REQ-018, INV-001, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: [TASK-003, TASK-004]
---

# Provider-agnostic verification and operator guidance

## Outcome and Value

Operators can configure and recover self-hosted or hosted OIDC accurately with any standard OIDC provider, and the docs match the shipped contract.

## Owners, Scope, Consumers, and Prohibited Changes

Own the public self-hosted and hosted documentation, CLI help, and architecture authority updates where the approved contract supersedes older prose. Consume completed journeys from TASK-001–004. Do not add hosted signup, social login, a commercial extension stub, provider credentials, a provider-specific code path, or a certified-provider list.

## Approach

1. Reconcile public API, generated schemas, UI, CLI help, and documentation against the approved contract.
2. Document IdP and Wyrd operator responsibilities, setup inputs and exact callback, one-active-connection rule, role mapping, local human login, independent machine identity, the shared deployment keyring for provider secrets and recoverable login/session credentials, key rotation across replicas, missing-key failure, and recovery. State that secretless-provider human login and OIDC-off UI sessions still need the keyring; machine-only use without recoverable stored credentials does not.
3. Run the integrated self-hosted OIDC-off and OIDC-on journeys and the hosted two-tenant journey (Keycloak and Dex), including negative and provider-switch flows.
4. Confirm the server uses only standard OIDC (discovery, authorization code with PKCE, standard client auth, JWKS ID-token validation, standard claims) with no provider-specific branch. Write the generic provider setup page plus short examples for Okta, Microsoft Entra ID, Google, Auth0, and Keycloak (callback URL, scopes, groups claim), as comparable open-source servers do.

## Proof Strategy

This task changes documentation and verifies already implemented behavior. It adds no production executable logic, so a manufactured RED is inapplicable. Use static contract comparison, existing real-server journeys, and generated-document checks. If closing a discovered behavioral gap requires executable changes, route that work to the owning preceding task and its TDD scenarios; if it changes approved behavior, return to the spec.

## Acceptance Criteria

AC-001–009 have recorded owner evidence across self-hosted, hosted, UI, CLI, Rust/Python/TypeScript SDK, and machine paths. No provider-specific code exists; docs give generic setup plus per-provider examples. Docs never promise instant access-token revocation or advertise production SSO before the production journey passes.

## Expected Write Set and Consumer Closure

Likely docs/, architecture/wyrd-design.md, architecture/wyrd-security-posture.md, CLI help text, generated contract artifacts from their sources. Do not hand-edit generated files.

## Verification and Evidence

Run mise run test:identity:journey, mise run test:principals:integration, mise run test:cli:journey, mise run py:test:integration, mise run ts:test:integration, mise run codegen:check, and mise run docs:check. Run mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check and test for UI documentation or presentation changes. If executable or Rust/Python surfaces change, run their AGENTS.md formatting and lint gates as well. A red required lane blocks completion.

The integration lane must include the exact TASK-001–004 selectors and fail
when any is absent; verify its final selection with `mise exec -- cargo
nextest list --locked -p wyrd-server --test identity_e2e` and the equivalent
CLI, UI Vitest, Python pytest collection, and TS Vitest listing before running
the aggregate. `mise run docs:check` and `mise run codegen:check` prove static
contract/doc closure (AC-009). An OIDC-off real UI/SDK run and a hosted
different-provider run close AC-001/003. Record each acceptance criterion's test and result in the task evidence.

## Material Stop Conditions

Stop if a public contract differs from the approved spec, a standard OIDC provider would need a provider-specific branch, or hosted signup becomes required in the open-source server.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [Wyrd design](../../../../architecture/wyrd-design.md); [security posture](../../../../architecture/wyrd-security-posture.md); [testing workflow](../../../../architecture/references/languages/testing-workflows.md).

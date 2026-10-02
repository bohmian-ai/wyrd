---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-001, REQ-005, REQ-018, REQ-021, INV-001, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: [TASK-009, TASK-010, TASK-011, TASK-012]
---

# Docs and architecture authority for the standard flows

## Outcome and Value

Operators and SDK users read documentation and architecture authority that
match the shipped standard flows: OIDC relying party on `openidconnect`, Wyrd
as the OAuth authorization server for the code, device, refresh, revocation,
token-exchange, and jwt-bearer grants, the BFF as a confidential client with
an encrypted cookie session, and the shared client on `oauth2`. This is report
item T5 in
[`research/auth-standards-recommendation.md`](../research/auth-standards-recommendation.md).

## Owners, Scope, Consumers, and Prohibited Changes

Own public self-hosted and hosted docs, CLI help text, source documentation
that feeds generated declarations, and architecture authority
(`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
`architecture/wyrd-doctrine.mdx` where they conflict). Consume the completed
journeys of TASK-001–004 and TASK-009–012. Do not add hosted signup, social
login, a commercial stub, provider credentials, a provider-specific code path,
or a certified-provider list. Do not document SAML or SCIM; both are deferred.

Delete: every passage about the login handoff, the private BFF channel
(`/internal/bff/v1/*`, `x-wyrd-bff-key`), server-side browser sessions, sealed
login completion, and the keyring protecting session or login credentials.

## Approach

1. Reconcile public API, generated schemas, UI, CLI help, and docs against
   spec revision 11.
2. Document operator and IdP responsibilities, setup inputs and exact
   callback, one active connection, role mapping, CLI device login, saved-login
   selection (newest login by default, `tenant` to select), independent
   machine identity, and recovery. The keyring is required only when a
   provider secret is stored (REQ-005).
3. Document the OAuth surface: the authorize, token, platform token,
   revocation, and device authorization endpoints, RFC 8414 metadata, the
   `wyrd-ui` confidential and `wyrd-cli` public clients, refresh rotation for
   public clients only, the RFC 6749 §5.2 error mapping, and the one sanctioned
   exception to `WyrdError` problem+json for these endpoints.
4. In the security posture, change "rotated on every successful use" for
   access and refresh tokens to "rotated on every use for public clients".
5. Keep the generic provider setup page plus short examples for Okta,
   Microsoft Entra ID, Google, Auth0, and Keycloak.

## Proof Strategy

This task changes documentation and verifies already implemented behavior. It
adds no production executable logic, so a manufactured RED does not apply.
Use static contract comparison, the existing real-server journeys, and
generated-document checks. If closing a gap needs executable changes, route
it to the owning task (TASK-009–012); if it changes approved behavior, return
to the spec.

## Acceptance Criteria

- AC-001–009 have recorded owner evidence across self-hosted, hosted, UI, CLI,
  Rust/Python/TypeScript SDK, and machine paths.
- FIND-TASK-004-8: shared-client, Python, and TypeScript configuration docs,
  test-wrapper docs, and the generated TypeScript declaration describe
  newest-login selection, selected-tenant mismatch behavior, and RFC 8628
  device login only. Generated output is regenerated from its source owner,
  never hand-edited.
- No doc, help text, or architecture passage describes the handoff, the BFF
  channel, browser-session rows, sealed completion, or session sealing keys.
- Docs never promise instant access-token revocation or advertise production
  SSO before the production journey passes. No provider-specific code exists.

## Expected Write Set and Consumer Closure

`docs/`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
`architecture/wyrd-doctrine.mdx` where it conflicts, CLI help text, source
docs in `crates/shared/wyrd-client` and `sdks/*`, and generated artifacts
from their sources.

## Verification and Evidence

Before the aggregate, list each journey selection:
`mise exec -- cargo nextest list --locked -p wyrd-server --test identity_e2e --run-ignored=all`
and the equivalent CLI, UI Vitest, Python pytest collection, and TS Vitest
listings. Then run: `mise run test:identity:journey` unfiltered (targets
`server`, `ui`, `cli`, `rust`, `client`, `python`, `typescript`),
`mise run test:shared`, `mise run test:wyrd-sdk`, `mise run test:cli:journey`,
`mise run test:principals:integration`, `mise run py:test:integration`,
`mise run ts:test:integration`, `mise run test:wyrd`,
`mise run codegen:check`, `mise run docs:check`, `mise run fmt`,
`mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:test:unit`, `mise run py:typecheck`, `mise run ts:test:unit`,
`mise run ts:typecheck`, `mise run ts:napi:check`, the boundary checks
`mise run check:client-tier`, `mise run check:pyo3-scope`,
`mise run check:unwrap-audit`, and `mise run check:workspace-hack`, and
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `test`
for UI text changes. Record each acceptance criterion's test and result.

## Material Stop Conditions

Stop and report if a needed behavior has no vetted library and is not covered
by a named RFC section; never write custom protocol logic. Also stop if a
public contract differs from the approved spec, a standard provider would
need a provider-specific branch, or hosted signup becomes required in the
open-source server.

## Authority Links

[Approved spec](../spec.md);
[research report](../research/auth-standards-recommendation.md);
[routing](../review/TASK-004-r2/lead-direction-routing.md);
[AGENTS.md](../../../../AGENTS.md);
[Wyrd design](../../../../architecture/wyrd-design.md);
[security posture](../../../../architecture/wyrd-security-posture.md);
[testing workflow](../../../../architecture/references/languages/testing-workflows.md).

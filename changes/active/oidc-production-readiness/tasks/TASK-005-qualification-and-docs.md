---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-001, REQ-018, INV-001, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: [TASK-003, TASK-004]
---

# Production qualification and operator guidance

## Outcome and Value

Operators can configure and recover self-hosted or hosted OIDC accurately, and every publicly claimed provider/topology combination has redacted evidence from a controlled real account over externally trusted TLS.

## Owners, Scope, Consumers, and Prohibited Changes

Own the public self-hosted and hosted documentation, CLI help, architecture authority updates where the approved contract supersedes older prose, and the qualification record. Consume completed journeys from TASK-001–004. Do not add hosted signup, social login, a commercial extension stub, provider credentials to the repository, or a production support claim based only on mocks.

## Approach

1. Reconcile public API, generated schemas, UI, CLI help, and documentation against the approved contract.
2. Document IdP and Wyrd operator responsibilities, setup inputs and exact callback, one-active-connection rule, role mapping, local human login, independent machine identity, secret rotation, failure handling, and recovery.
3. Run the integrated self-hosted OIDC-off and OIDC-on journeys and the hosted two-tenant journey, including negative and provider-switch flows.
4. Qualify Okta, Keycloak, and Entra ID with controlled accounts over externally trusted TLS for every combination publicly claimed; record redacted immutable artifact, topology, origin, configuration fingerprint, time, and outcome.

## Qualification contract and claim matrix

The initial public support claim is the following six combinations, each
using the production Web client flow and an externally trusted HTTPS public
origin. `SecretBasic` is the required common Web client method. If
`SecretPost` or `Public` is offered by TASK-001 for a provider, add a row for
that method before claiming it; `PrivateKeyJwt` is excluded. A hosted row
means a tenant on a two-tenant Wyrd service, never a separate dedicated
deployment. At least one hosted run pairs different providers concurrently;
the other hosted rows may reuse a different controlled peer tenant.

| Provider | Self-hosted | Hosted two-tenant |
|---|---|---|
| Okta | Required | Required |
| Keycloak | Required | Required |
| Entra ID | Required | Required |

Before execution, secure a controlled tenant/application and test user for
each provider, with authority to register the exact callback, configure group
claims, rotate a client secret, and revoke access. Provision an externally
trusted TLS origin and reachable Wyrd deployment for each topology; record
the tested public origin, TLS certificate issuer, immutable Wyrd commit/image
digest, provider tenant/application ID hashed or redacted, client-auth method,
issuer, discovery/JWKS fingerprint, role-map fingerprint, UTC time, operator,
and pass/fail evidence link in the durable redacted record
`changes/evidence/oidc-production-readiness.md`. Link that record from the
eventual completed change record and relevant operator docs; it must survive
deletion of `changes/active/oidc-production-readiness/`. Never store secrets,
tokens, or raw provider assertions there.

For every matrix row, use the deployed UI login/callback and authorized and
denied call, then CLI handoff and a public SDK renewal; verify outage and
wrong-tenant refusal. Run OIDC-off self-hosted, two simultaneous hosted
tenants, provider replacement/recovery, API-key and workload paths at least
once per topology. Compare the deployed API/UI/CLI behavior with generated
contracts and docs. A row passes only with the exact live outcome and
evidence; local Keycloak/Dex fixtures are continuous regression coverage,
not a substitute. If accounts, TLS, or a row fail, do not claim that row or
mark AC-008 complete. Narrow published provider/topology/method claims to
qualified rows and return to the spec if the approved three-provider outcome
cannot be achieved.

## Proof Strategy

This task changes documentation and records qualification of already implemented behavior. It adds no production executable logic, so a manufactured RED is inapplicable. Use static contract comparison, existing real-server journeys, live controlled-provider results, and generated-document checks. If closing a discovered behavioral gap requires executable changes, route that work to the owning preceding task and its TDD scenarios; if it changes approved behavior, return to the spec.

## Acceptance Criteria

AC-001–009 have recorded owner evidence across self-hosted, hosted, UI, CLI, Rust/Python/TypeScript SDK, and machine paths. Okta, Keycloak, and Entra ID are each qualified for the exact publicly claimed configurations. Docs never promise instant access-token revocation or advertise production SSO before the production journey passes.

## Expected Write Set and Consumer Closure

Likely docs/, architecture/wyrd-design.md, architecture/wyrd-security-posture.md, CLI help text, generated contract artifacts from their sources, and the durable redacted record at `changes/evidence/oidc-production-readiness.md`. Do not hand-edit generated files.

## Verification and Evidence

Run mise run test:identity:journey, mise run test:principals:integration, mise run test:cli:journey, mise run py:test:integration, mise run ts:test:integration, mise run codegen:check, and mise run docs:check. Run mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check and test for UI documentation or presentation changes. Record externally trusted TLS provider results separately from local Keycloak/Dex mock/fixture results. If executable or Rust/Python surfaces change, run their AGENTS.md formatting and lint gates as well. A red required lane blocks completion.

The integration lane must include the exact TASK-001–004 selectors and fail
when any is absent; verify its final selection with `mise exec -- cargo
nextest list --locked -p wyrd-server --test identity_e2e` and the equivalent
CLI, UI Vitest, Python pytest collection, and TS Vitest listing before running
the aggregate. The live qualification procedure above is manual and requires
controlled accounts; its evidence rows, not the local lane's green status,
prove AC-008. `mise run docs:check` and `mise run codegen:check` prove static
contract/doc closure (AC-009). An OIDC-off real UI/SDK run and a hosted
different-provider run close AC-001/003. Record each acceptance criterion's
test/result/evidence link in the qualification record; do not accept a
provider claim solely because another provider's row passed.

## Material Stop Conditions

Stop if a claimed provider cannot pass qualification, a public contract differs from the approved spec, hosted signup becomes required in the open-source server, or required controlled provider accounts are unavailable; report the exact blocked claim and evidence.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [Wyrd design](../../../../architecture/wyrd-design.md); [security posture](../../../../architecture/wyrd-security-posture.md); [testing workflow](../../../../architecture/references/languages/testing-workflows.md).

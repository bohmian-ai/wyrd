---
id: TASK-009-R3
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
task: TASK-009
candidate: 04597909203463820b2033c12956f5fe6fcfe1f4
base: 35a53faa216b10651d85c96ce12e34f382cac637
findings: [FIND-TASK-009-15, FIND-TASK-009-16]
---

# TASK-009 R3 platform login boundary and contract

## Authority and outcome

Approved specification:
`changes/active/oidc-production-readiness/spec.md` revision 11. Original task:
`changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`.
This remediation applies to the cumulative candidate
`35a53faa216b10651d85c96ce12e34f382cac637..04597909203463820b2033c12956f5fe6fcfe1f4`.

Keep the round-two platform full-discovery behavior, but expose it as an
operation of the existing `PlatformLogin` owner and make the public route
documentation and served contract describe its existing discovery/JWKS `503`
failure accurately.

`FIND-TASK-009-5` and `FIND-TASK-009-14` are withdrawn and must not be
reopened, renamed, or implemented indirectly.

## Diagnoses and required corrections

### Keep fresh discovery on the platform login owner — `FIND-TASK-009-15`

`PlatformLogin` constructs and owns the platform `RelyingParty`, including the
process-local cache used by begin and callback. The round-two correction added
public `PlatformLogin::relying_party` solely so platform configuration can call
`RelyingParty::discover` and read the advertised JWKS URI. That one-caller
dependency accessor exposes all lower-level relying-party operations and splits
the setup workflow across the owner and its server consumer.

Delete the dependency accessor. Put the fresh-discovery operation needed by
platform setup on `PlatformLogin`; it must delegate to the already owned
`RelyingParty::discover`, replace the same cache entry, and return the
advertised JWKS URI needed for persistence. Route platform configuration
through that owner operation.

Reuse the existing `PlatformLogin`, `RelyingParty`, cache, error type, and route
error projection. Add no trait, wrapper service, second cache, invalidation
mechanism, setting, retry, provider branch, or new protocol behavior.

### Declare the existing discovery/JWKS `503` — `FIND-TASK-009-16`

Platform configuration now performs full standard discovery, which fetches the
provider metadata and advertised JWKS before persistence. Blocked-address and
malformed-input failures remain `400`, but unavailable or undecodable provider
metadata/JWKS and issuer mismatch reach the existing catalogued
`503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` response.

The handler rustdoc and `utoipa` response list currently omit that `503`, the
`400` description calls an unreachable issuer a validation failure, and the
adjacent comment describes the full discovery operation imprecisely. The
served failure test accepts any non-success response, so it does not protect
the stable error contract; the served OpenAPI test does not assert this
operation's `503` declaration.

Correct the existing handler rustdoc, comment, and `utoipa` response list to
describe metadata-plus-JWKS discovery and the existing `503` error. Keep
blocked destinations and malformed input at `400`. Tighten the existing
unavailable/undecodable JWKS cases to assert HTTP `503` and
`WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`, and extend the existing served OpenAPI
contract test to assert that the platform connection PUT operation advertises
the same response.

Use the existing error catalog, response schema, route declaration, platform
journey, and served OpenAPI test. Add no error, documentation file, generator,
check, test harness, or compatibility surface.

## Constraints and preserved behavior

- Preserve `openidconnect 4.0.1`, its standard verifier defaults, the one
  screened `ScreenedHttp` transport, and the process-local Moka cache.
- Preserve full platform discovery before persistence, including failure on
  unavailable or undecodable JWKS, and preserve same-issuer cache replacement
  before the durable row commits.
- Preserve tenant login, connection testing, workload metadata-only discovery,
  workload `ExternalVerifier`, RFC 9207, one unknown-key rediscovery, client
  authentication, role mapping, issuance, audit, secret sealing, and
  platform/tenant separation.
- Preserve the existing stable error catalog. This task documents and proves
  the reachable `503`; it does not introduce a new failure.
- Do not remove or change human `jwks_ttl_secs` fields or persistence. The
  proposed cleanup was rejected as pre-existing, task-unrelated public/schema
  debt and would violate TASK-009's no-operator-visible-change outcome.
- Do not add an audience option, key-health probe, empty-key policy, second
  cache, distributed invalidation, retry system, provider branch, migration
  preflight, overlap column, dual write, or provider-error redaction.
- Do not change Python, TypeScript, SDK, CLI, BFF, device-grant, session, or
  grant behavior.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-009-15` | Platform configuration performs fresh full discovery through an inherent operation on the existing `PlatformLogin` owner; the public dependency accessor is absent; the same process cache still supplies subsequent begin/callback behavior. |
| `FIND-TASK-009-16` | Platform configuration rustdoc, comments, and served OpenAPI declare unavailable or undecodable discovery/JWKS as the existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; blocked/malformed input remains `400`; existing served failures assert the stable status and code. |
| Both | Failed discovery changes no durable connection, successful same-issuer reconfiguration refreshes the provider used by the next login, workload setup remains metadata-only, and no new mechanism or public option is added. |

## Focused proof

Use Red-Green-Refactor for the two declaration/owner gaps and record exact,
zero-selection-safe commands.

- Tighten the existing unavailable/undecodable JWKS cases in
  `federated_platform_sign_in_runs_through_the_served_callback`, then run:

  ```bash
  mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e \
    -E 'test(=federated_platform_sign_in_runs_through_the_served_callback)'
  ```

- Extend
  `the_served_document_describes_the_composed_surface` in the existing served
  OpenAPI contract target with the platform connection PUT response assertion,
  then run:

  ```bash
  mise exec -- cargo nextest run --locked -p wyrd-server --test pg_openapi_contract \
    -E 'test(=the_served_document_describes_the_composed_surface)'
  ```
- Prove statically that no public `PlatformLogin::relying_party` accessor
  remains and that the configure route calls the owner operation.
- Run only the narrowest repository-owned lanes covering the final Rust/server
  and served OpenAPI write set: `mise run fmt`, `mise run lints`,
  `mise run test:wyrd`, and `mise run test:principals:integration`.

Do not require a full identity journey or every-language sweep in this task
remediation. Those run once at final change review under the standing rule from
commit `518026d54`.

Route this task directly to `$wyrd-implement`.

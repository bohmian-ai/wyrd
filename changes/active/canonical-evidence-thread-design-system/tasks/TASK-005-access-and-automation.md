---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-canonical-evidence-thread-design-system
spec_revision: 2
requirements: [REQ-013, REQ-015, REQ-016, REQ-019, REQ-032, REQ-036, REQ-037, REQ-041, REQ-042, REQ-043, REQ-044, INV-007, INV-008, INV-011, INV-012, AC-006, AC-010, AC-011, AC-012, AC-015]
depends_on: [TASK-002]
parent_task:
remediates: []
---

## Outcome and Value

A team operator can provision a principal, issue and rotate credentials, grant additional access, and configure human OIDC with Okta using supported Wyrd contracts. A developer can find the live OpenAPI document and use the API, CLI, or MCP in a CI/CD flow without guessing at authentication, errors, or hosted Swagger availability.

## Owners, Scope, Consumers, and Prohibited Changes

`docs/` owns the access and automation task pages and their direct reference links. Read current principal/role routes, CLI commands, authentication and OIDC config, served OpenAPI, generated schemas, and journey tests before promising a step. Keep platform and tenant authority distinct. Never publish working credentials or put secret values into example output. Do not add grants, API endpoints, hosted Swagger UI, Okta product behavior, or automation machinery that does not exist.

## Approach

1. Reconcile current access and reference docs with the server/CLI/SDK authority for principals, grants, credentials, OIDC, OpenAPI, MCP, and errors.
2. Write one short access path from principal creation through credential issuance, permission update, verification, and a denied-request example.
3. Give Okta as a concrete human OIDC configuration walkthrough, with provider-side steps clearly separated from Wyrd-owned checks and workload federation.
4. Expose `GET /openapi.json`, schema/error reference, CLI, and MCP from task navigation; show the shortest supported CI/CD call path with authentication and result verification.
5. Mark any unsupported administration or automation step at its decision point and complete TASK-002's journey inventory.

## Proof Strategy

This task documents existing behavior and does not add executable logic, so TDD is not applicable. Validate command flags, route names, payloads, permissions, and expected outcomes against the owning source and real existing journey tests. Execute supported local API/CLI steps through repository-managed environments where feasible; verify Okta-specific instructions against current provider guidance before publishing them, without claiming an external Okta integration test ran when it did not.

## Acceptance Criteria

- A reader can distinguish platform versus tenant credentials and take the supported principal/credential/grant path, including rotation and a negative permission check.
- Okta guidance gives the Wyrd redirect URI and issuer/client/claim/group mapping fields, an outcome check, and common failure conditions; human OIDC is not confused with workload federation.
- OpenAPI at `/openapi.json` is prominent, with direct links to schemas, errors, CLI, MCP, and an honest explanation of Swagger-compatible tooling versus any unshipped hosted Swagger UI.
- The CI/CD path uses an existing supported authentication and API/CLI surface; each command shows expected result and verification, while gaps are explicit.
- Access and API content remains discoverable from TASK-002 navigation without exposing every operation on the homepage.

## Expected Write Set and Consumer Closure

Likely access/automation guides and direct updates to `docs/src/content/docs/self-hosting/`, `concepts/`, `api/`, and `reference/` pages that carry conflicting current claims. Existing auth routes, CLI implementation, generated API reference, and MCP catalog are evidence sources only. Generated docs remain generator-owned.

## Verification and Evidence

- `mise run docs:check` covers public commands, generated pages, links, static build/search, and accessibility.
- For runnable Wyrd access/API claims, run the relevant repository-managed principal, identity, CLI, or served-OpenAPI journey lanes after confirming their scope in `mise.toml`; record the exact command and observed allow/deny outcome. Compare OIDC examples to current server config and Okta primary documentation.
- Review task pages in both modes and narrow layout; confirm secrets, required scopes, failure recovery, and unsupported states are visible without optional disclosure.

## Material Stop Conditions

Stop if a claimed flow needs a new authz grant model, OIDC contract, server endpoint, CLI command, or hosted API explorer. Document the current boundary and request a separate feature change rather than inventing a docs-only workaround.

## Authority Links

- [Approved specification](../spec.md)
- [Repository instructions](../../../../AGENTS.md)
- [Design doctrine](../../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

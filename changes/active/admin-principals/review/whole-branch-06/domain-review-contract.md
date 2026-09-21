# Admin principals whole-branch review 06 — public contract domain

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate / reviewed HEAD | `2c0408b683f7a548cec6dd08b35698d761d33b31` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 10, status `approved` |
| Original task authority | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review authority | `review/whole-branch-05/verdict.md`, `findings-validation.md`, and `TASK-001-008-R5-close-validated-findings.md` |

The candidate matched the identity above before this report was written. The
review was read-only apart from this required report.

## Reviewed boundary and authority coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| Shared client authentication and renewal | `REQ-047`, `REQ-048`, `AC-018`; `wyrd-client` auth and HTTP transport, `HttpTransport::authenticated_replay`, JSON/raw request paths, `Principals`, `Platform`, CLI consumers, transport tests | PASS, except the OpenAPI issue below is reached by the same storage client |
| Tenant credential revocation | `REQ-006` through `REQ-010`, `REQ-048`; `Principals::revoke_credential`, tenant principal route/service, CLI credential command, first/second-refusal transport proofs | PASS — the operation uses `request_json`, receives `204` as unit, and shares the bounded one-refresh/one-replay owner |
| MCP transport authentication and replay | `REQ-047`, `REQ-048`, `INV-015`; `WyrdMcpHttpClient`, `HttpTransport::authenticated_replay`, rmcp POST/SSE/delete entry points and unit/journey evidence | PASS — MCP retains protocol framing while Wyrd header decoration and the single 401 replay live in `wyrd-client` |
| MCP principal tools | `REQ-036`, `AC-013`, agent-harness typed-tool and scope rules; runtime descriptors, dispatch, shared DTOs, permission gating and Postgres journey | FAIL — `CONTRACT-06-02` |
| HTTP method/path/body/error contract | `REQ-036`, `REQ-049`, `AC-014`, `AC-019`, errors authority; every changed route owner, Axum/utoipa composition, runtime `/openapi.json`, problem-media modifiers and contract suite | FAIL — `CONTRACT-06-01` |
| CLI and SDK projection | `REQ-036`, `REQ-047`, `AC-014`; all changed CLI HTTP callers, shared handles, Rust SDK re-export, CLI journeys; explicit absence of Python/TypeScript admin bindings | PASS — CLI Wyrd calls route through the shared client; no prohibited language-specific admin implementation was added |
| Schemas and docs | `AC-013`, agent-harness generation authority; `wyrd-spec` auth DTOs/schemas, generator inputs, docs and LLM indexes, deletion of checked-in `openapi.yaml` | PASS except the runtime MCP and OpenAPI gaps below |
| Explicit exclusions and non-goals | Task 008 and revision-10 non-goals; local blob wildcard exception, MCP protocol endpoint, health endpoints, no OpenAPI YAML/snapshot/generator, no compatibility routes | FAIL only for the local blob exception: MCP is a distinct protocol and the health probes are operational endpoints, but the blob routes are authenticated Wyrd storage operations consumed by the shipped client/CLI |

`.codegraph/` was used first to trace the client, server, CLI, MCP, and error
call paths. The complete base-to-candidate inventory and the contract-relevant
cumulative diff were inspected, including the R5 corrective commits and the
final local-storage regression commit. Applicable authority included
`AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, the agent-harness, errors, and testing
references, the approved specification, all eight original tasks, and the
whole-branch-05 review/remediation packet.

## Review Findings

### Critical

None.

### Important

#### `CONTRACT-06-01` — VIOLATION — served local storage operations are absent from the canonical OpenAPI contract

- **Violated obligation:** `REQ-049` requires `/openapi.json` to describe every
  served public route, typed request/response, problem media type, and reachable
  stable error; `AC-019` requires exact route coverage without a parallel
  catalog. This is also the still-relevant correction boundary of stable
  `FIND-admin-principals-13`.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/storage/routes.rs:35-54,320-345,396-422`;
  composition at `crates/wyrd/wyrd-server/src/http/router.rs:54-68,85-102`;
  incomplete proof claim at
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:1-12,106-147`.
- **Evidence:** When the configured backend is local, `storage_router` mounts
  `PUT /v1/cards/upload/local/{*id}` and
  `GET /v1/cards/download/local/{*path}` with plain Axum `.route` calls. The
  adjacent comment explicitly says they are not documented, and both handlers'
  `#[utoipa::path]` declarations were deleted. `build_router` merges that router
  into the authenticated `/v1` surface and serves the document produced by its
  `OpenApiRouter`; therefore the operations are live but absent from
  `/openapi.json`. The contract test asserts only a selected path list and
  claims an undocumented served method cannot be written, so it cannot detect
  these two deliberate exceptions. These are not dormant development helpers:
  local upload/download plans send the shared client to these routes, and the
  recorded `test:cli:journey`/storage proof only establishes that the
  undocumented routes work.
- **Observable consequence:** A language-agnostic client generated or planned
  from the canonical runtime document cannot implement the local backend's byte
  transfer. It sees the typed init response hand out URLs for operations the
  contract says do not exist, and it cannot discover their bodies, success
  shapes, authentication, problem media, or stable refusals.
- **Required testable correction:** Keep one utoipa/Axum registration owner and
  make the local transfer contract representable without losing its slashful
  opaque value (for example, move that value out of a wildcard path into a
  typed request position supported by both Axum and OpenAPI). Co-register the
  resulting upload and download operations and preserve the existing shared
  client and CLI local round trip. Do not restore a hand-written route catalog,
  snapshot, or second OpenAPI document.
- **Focused closure proof:** Against a local-backend `WyrdTestServer`, assert
  that every method/path mounted by the Wyrd HTTP API is present in the served
  document, that both local transfer operations declare their binary body or
  response plus `WyrdProblem` media/codes and authentication, and that the
  existing CLI/local-storage round trip still transfers a nested object path.

#### `CONTRACT-06-02` — VIOLATION — MCP UUID validation is stricter than the schema agents receive

- **Violated obligation:** The agent-harness requires typed MCP inputs and
  validation in the durable contract; `REQ-036` and `AC-013` require MCP to
  project that shared typed contract. R5-5 specifically required the published
  schema and consumed DTO to be the same contract.
- **Exact location:**
  `crates/wyrd-spec/src/auth/tenant_principals.rs:77-115` and
  `crates/wyrd/wyrd-server/src/mcp/principals.rs:77-107,142-170,181-225`;
  incomplete proof at
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs:22-41,83-122`.
- **Evidence:** The shared MCP DTOs describe `principal_id` and
  `credential_id` as UUIDs but type all three occurrences as unconstrained
  `String`. `schema_of` consequently publishes ordinary string properties.
  `parse_args` accepts any strings under that schema, after which the handlers
  apply a separate `Uuid` parse and can reject the schema-valid call. The repo
  already has `PrincipalId` with `Serialize`, `Deserialize`, and `JsonSchema`,
  and workspace `schemars` enables UUID schemas. The journey checks only the
  names of required properties, so it cannot detect this schema/runtime
  disagreement.
- **Observable consequence:** An MCP client can construct a call that validates
  against the advertised tool schema but is refused as invalid before the
  operation runs. Agents must recover the UUID constraint from prose or an
  error rather than from the machine-readable contract.
- **Required testable correction:** Use the existing `PrincipalId` for
  principal fields and the existing UUID type/schema support for credential
  identifiers in the shared MCP DTOs and acknowledgement, then consume those
  typed values directly in the handlers. Do not add an MCP-only validator,
  schema layer, or identifier abstraction.
- **Focused closure proof:** Validate representative valid and invalid tool
  arguments against each advertised input schema: valid UUIDs pass and invalid
  UUID strings fail before dispatch. Keep the real discover → list/revoke →
  observe journey and validate its structured results against the advertised
  output schemas.

### Suggestions

None.

## Open Questions

None. Both corrections stay within approved behavior and use existing owners
and installed type/schema machinery; no product or architecture decision is
required.

## Verification Notes

- Reviewed the whole-branch-05 recorded evidence: focused shared-client renewal
  tests, MCP journey, CLI journey, platform/principal journeys, served OpenAPI
  tests, codegen, docs, and storage matrix were green on the R5 implementation
  candidate. Candidate `2c0408b6` changes only the tracked review packet after
  source candidate `5ecc8a4e`.
- This independent static review did not rerun long Postgres, CLI, MCP, or
  storage lanes. Their recorded results are credible for the behavior they
  select, but neither suite proves the two gaps above: the OpenAPI suite does
  not enumerate the plain-Axum local routes, and the MCP journey asserts only
  required property names rather than UUID schema constraints.
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..2c0408b683f7a548cec6dd08b35698d761d33b31`
  was clean. HEAD was rechecked as
  `2c0408b683f7a548cec6dd08b35698d761d33b31` immediately before report drafting.

## Result

**FAIL** — two bounded public-contract findings remain. Neither requires a
specification revision.

# Public contract domain review

## Subject and boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Immutable candidate: `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Remediation range inspected: `4d185da9..ddd80c7a`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 10
- Original delivery: `changes/active/admin-principals/tasks/TASK-001-principal-credential-model.md` through `TASK-008-sdk-and-mcp-projection.md`
- Prior validated remediation: `changes/active/admin-principals/review/whole-branch-06/TASK-001-008-R6-close-validated-findings.md`

This review traced the cumulative public-contract boundary and the R6 changes in particular: the shared principal credential DTOs, MCP catalog schemas and handlers, runtime OpenAPI composition, local upload/download route registration and extraction, server-produced transfer URLs, shared-client dispatch, CLI/storage consumers, and the routed contract guidance. The candidate remained `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983` throughout inspection.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer evidence | Result |
|---|---|---|---|
| One typed MCP input/output contract | `REQ-036`, `AC-013`; `architecture/references/languages/agent-harness.md` tool-contract rules | `wyrd-spec/src/auth/tenant_principals.rs`; `wyrd-server/src/mcp/principals.rs`; `wyrd-mcp/tests/bifrost/mcp/principals.rs` | PASS — `PrincipalId` and `Uuid` now drive schema, deserialization, handler input, and revocation output; the duplicate UUID parser is gone and the journey validates valid/invalid inputs and real outputs. |
| Runtime OpenAPI ownership and authentication | `REQ-049`, `AC-014`, `AC-019`; `AGENTS.md` §§9,11; agent-harness OpenAPI guidance | `wyrd-server/src/http/{router.rs,openapi.rs}`; `wyrd-server/tests/pg_openapi_contract.rs` | PASS for route co-registration and document-wide authentication — both local transfer routes are contributed by `routes!`, inherit the one access-token scheme, and no parallel document was added. |
| Local upload operation | `FIND-admin-principals-13` acceptance; typed body/problem contract rules | `components/storage/routes.rs:312-379`; `wyrd-storage/src/service.rs:432-486`; storage negative/e2e journeys | PASS — the ordinary `{id}` route is mounted and documented as binary PUT; malformed upload IDs are parsed inside the handler and use the canonical Wyrd error mapper. |
| Local download operation | `REQ-049`; `FIND-admin-principals-13` acceptance requiring exact problem media and stable errors | `components/storage/routes.rs:429-490`; `wyrd-storage/src/service.rs:703-744`; shared-client download dispatch and transport | **FAIL — `CONTRACT-07-01`.** |
| URL production and client compatibility | `REQ-047`; shared-client ownership and no compatibility alias | `wyrd-storage/src/service.rs:703-744,1534-1566`; `wyrd-client/src/storage/download/{mod.rs,local_fs.rs}`; CLI/storage journeys | PASS — nested paths are query-encoded by the owner, empty-base relative URLs remain usable, and the shared client recognizes and authenticates the one new route without retaining the wildcard alias. |
| Contract proof guidance | `AGENTS.md` §11; testing-workflows and agent-harness references | Both architecture references now name `mise run test:principals:integration`; `mise.toml` runs the whole `pg_openapi_contract` target | PASS for the routed command and nonzero suite selection. |

## Proposed findings

### `CONTRACT-07-01` — INCORRECT — the documented local-download 400 is not the response Axum serves for an invalid query

- **Violated obligation:** `REQ-049` requires exact problem media and reachable stable error codes for every public route; `FIND-admin-principals-13` specifically requires the local transfer operations to publish and serve problem media plus stable errors; the HTTP error rule requires public failures to use the canonical `WyrdError` problem envelope.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/storage/routes.rs:448-479`, especially the declared `400 WyrdProblem` at lines 456-457 and the infallible `Query<LocalDownloadQuery>` extractor at line 478; the proof at `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:188-226` checks only document metadata. The client consequence is visible at `crates/shared/wyrd-client/src/transport/http.rs:476-483`.
- **Evidence and reachability:** `path` is a required public query parameter. A request such as authenticated `GET /v1/cards/download/local` reaches the mounted operation but fails the `Query<LocalDownloadQuery>` extractor before `download_local_blob` can return `WyrdErrorResponse`. The pinned Axum 0.8.9 `QueryRejection::FailedToDeserializeQueryString` implementation returns `(400, body_text)` directly, i.e. a plain-text Axum rejection with no Wyrd code. The OpenAPI operation instead promises `application/problem+json`, `WyrdProblem`, and `WYRD_STORAGE_400_TENANT_PATH_MISMATCH`. No outer server layer rewrites extractor responses through `WyrdErrorResponse`.
- **Observable consequence:** an independent client generated from `/openapi.json` cannot safely decode this reachable 400 as the promised problem document; the shared client also attempts JSON, substitutes `{}` for the plain body, and projects an empty-code upstream failure rather than the documented validation/storage error.
- **Required testable correction:** handle the query extractor rejection at the route boundary and map it through the existing `WyrdErrorResponse` owner to one existing 400 validation code, then document that actually reachable code alongside tenant-path mismatch. Do not add middleware, a new error type, a compatibility route, or a second contract. Extend the assembled-server contract test to send an authenticated request with the required `path` absent (and one malformed/invalid path case), asserting status 400, `application/problem+json`, a catalog code named by this operation, and the standard problem shape.

## Verification limits

- The appended evidence records passing `test:principals:integration`, `test:bifrost:journey:mcp`, `test:storage:e2e`, `test:storage:matrix`, `test:cli:journey`, `codegen:check`, and the wider stated lanes; source inspection confirms the owning `mise` tasks select nonzero test binaries.
- No test executes a malformed or missing local-download query against the assembled router. `local_transfer_operations_publish_their_binary_contract` validates only the generated document, while the storage and CLI journeys exercise valid server-produced URLs, so they cannot detect `CONTRACT-07-01`.
- The R6 evidence names the OpenAPI and MCP tests through their owning lanes but records an exact focused selector only for `wyrd-storage::service::tests::local_download_url_uses_mounted_http_blob_route`, despite the remediation task requesting exact commands for every named Rust test. The owning lanes are credible nonzero proof, but the requested focused-command evidence is incomplete.
- Review was static; no additional Cargo or Postgres command was run, avoiding concurrent build/test interference with the other Wave 1 reviewers.

## Overall result

**FAIL** — the UUID schema/runtime correction, route co-registration, URL migration, client compatibility, and proof guidance close their intended seams, but one reachable failure of the newly documented download operation still violates its published problem contract.

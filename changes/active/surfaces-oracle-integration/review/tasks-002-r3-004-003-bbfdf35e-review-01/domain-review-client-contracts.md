# Client Contracts, SDKs, Authentication, and Tenancy Domain Review

## Reviewed Boundary

- Immutable comparison: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `bbfdf35e26212b2a831bda5e31e1ef4433e41900`, tree `58f5c2acfe8da01f3e4b58363f38ce761626df35`.
- Approved authority: `changes/active/surfaces-oracle-integration/spec.md`, revision 8; original `TASK-002-converge-client-and-sdks.md`; prior TASK-002 R2 verdict and validated ledger; `TASK-002-R3-close-r2-review-findings.md`; `TASK-004-unify-bifrost-data-root.md`; and `TASK-003-close-repository-integration.md`.
- Governing repository authority: `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/references/languages/spec-driven-development.md`; and `TESTING.md`.
- Source boundary traced: `wyrd-spec` Bifrost wire and error contracts; `wyrd-client` credential resolution, HTTP/gRPC transport, `Bifrost`, direct writes, query streaming, and lifecycle settlement; Rust/Python/TypeScript SDK projections; Bifrost HTTP/gRPC routes; server authentication and caller-derived tenant propagation; and the exact three-tool MCP adapter.
- Review posture: fresh Wave-1 acceptance audit of the complete cumulative candidate. No implementation source or other report was modified.

## Authority and Source Coverage

| Boundary | Obligation | Evidence | Result |
|---|---|---|---|
| Shared client and SDK ownership | Spec REQ-016 through REQ-021, REQ-040, REQ-041; INV-004 through INV-006, INV-013 | `crates/shared/wyrd-client`; `sdks/wyrd-sdk-{rust,python,ts}` manifests and native projections; public exports | PASS |
| Credential and tenant trust boundary | REQ-019A; INV-007; Wyrd design client/auth model | `ClientConfig::credential`, `CredentialChain`, `AuthMiddleware`, server `Caller`, Bifrost/query services and lifecycle requests | PASS |
| Stream and direct-write lifecycle | REQ-019, REQ-056, REQ-060; AC-004, AC-021 | `WriterPool`, `DirectSendPermit`, `QueryResultStream`; Rust/Python/TypeScript callers | PASS |
| Stable public errors | REQ-022 through REQ-025, REQ-059 | derive-backed `WyrdError`, shared client conversion, HTTP/gRPC reconstruction, Python eight-field projection, generated TypeScript union | PASS |
| HTTP/gRPC/MCP projection | REQ-056 through REQ-059; AC-002, AC-019 | Bifrost and query routes, tonic conversion, MCP descriptors and handlers, OpenAPI | FAIL: `CLIENT-CONTRACT-001` |
| Card and `WyrdState` preservation | REQ-004 through REQ-009; REQ-020, REQ-021; AC-003 | shared Cards/state owners plus Rust/Python/TypeScript projections and recorded journeys | PASS |

## End-to-End Boundary Result

- Public Rust, Python, and TypeScript Bifrost constructors route optional endpoints and the one explicit `credential` through `wyrd_client::bifrost::client_from_options`; no language projection accepts an effective tenant selector.
- The server derives tenant identity from authenticated `Caller` state. Table registration/list/describe and query lifecycle operations use `caller.data_tenant_id`; public request contracts do not carry tenant, principal, execution-path, worker, topology, or plan selectors.
- HTTP `/v1` and `/mcp` remain behind the authenticated router stack. The MCP adapter delegates catalog and query work to the same server services and advertises exactly `bifrost.list_tables`, `bifrost.describe_table`, and `bifrost.query` at runtime.
- The R3 direct-write correction admits sends under the same producer-map lock that closes shutdown, counts each admitted send until completion or cancellation, and waits on the count before shutdown returns. The R3 failed-drain correction restores `StreamSettlement::Settled` after the completed status-proof cycle, preventing repeated cancellation/polling.
- Public client-local failures retain catalog identity and safe structured details through the one shared Rust mapper; Python and TypeScript project that metadata rather than maintaining a second catalog.

## Material Proposed Findings

### CLIENT-CONTRACT-001 — VIOLATION — generated contracts still publish removed Bifrost surfaces

- **Violated obligation:** Spec REQ-057 requires the server-owned MCP surface to contain exactly the three approved Bifrost tools and says the removed static permission and error-catalog tools must not be restored. REQ-059, AC-002, and INV-010 require one coherent generated public contract with no stale authority or compatibility surface. TASK-003 requires generated schemas and docs to be clean and complete.
- **Exact location:** `crates/wyrd-spec/src/vala/api.rs:271-306,428-456`; `crates/wyrd-spec/examples/gen_schemas.rs:60-68,216-233`; `crates/wyrd-spec/schemas/bifrost_error_descriptor.json:3-4`; `crates/wyrd-spec/schemas/bifrost_permission_descriptor.json:3-4`; `crates/wyrd-spec/schemas/bifrost_sync_query_request.json:3-25`; orphan files `crates/wyrd-spec/schemas/bifrost_partition_column_spec.json` and `bifrost_partition_transform.json`; matching golden files under `crates/wyrd-spec/tests/schemas/`; and the generated inventory at `docs/src/content/docs/api/schemas.md:11-13`.
- **Evidence:** `BifrostErrorDescriptor` and `BifrostPermissionDescriptor` are referenced only by the schema generator and explicitly document the nonexistent `bifrost.list_errors` and `bifrost.list_permissions` MCP tools. `SyncQueryRequest`/`QueryParam` are likewise referenced only by their schema generator and contract-only tests; the live HTTP, gRPC, shared-client, CLI, Python, TypeScript, and MCP query paths all use `BifrostQueryRequest`, which has the required visibility, freshness, and deadline contract and no bound `params`. The two partition schemas have no remaining Rust type or generator owner, while the live registration contract uses `PhysicalLayoutWire` and fixes the partition column to `wyrd_event_time`. All six stale files remain advertised to clients and agents in the generated schema index. Repository search found no runtime caller for any stale shape.
- **Observable consequence:** a client or agent following the checked-in machine-readable contract can attempt two MCP tools the server does not expose, send the published `SyncQueryRequest { sql, params }` shape to the query endpoint even though its actual required fields differ, or implement the obsolete caller-selected partition-column contract. Those calls fail discovery/request validation instead of matching the advertised public API.
- **Required testable correction:** remove the unused descriptor, legacy sync-query/parameter source types and their generator entries; delete the two orphan partition schemas; regenerate the schema goldens and docs inventory from the surviving `BifrostQueryRequest`, `PhysicalLayoutWire`, derive-backed error catalog, and exact runtime MCP catalog. Do not add aliases, compatibility routes, or a second cleanup mechanism.
- **Focused closure proof:** require repository search to find none of `bifrost.list_errors`, `bifrost.list_permissions`, `BifrostErrorDescriptor`, `BifrostPermissionDescriptor`, `SyncQueryRequest`, `QueryParam`, `PartitionColumnSpec`, or `PartitionTransformWire` in public source/generated/docs artifacts; run `mise run codegen:check`, `mise run docs:check`, the owning `wyrd-spec` tests, and the existing MCP discovery journey proving exactly three tools.

## Verification Limits

- Independently ran `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::handle::tests::shutdown_waits_for_admitted_direct_write) | test(=bifrost::query::tests::failed_healthy_drain_settles_once)'`: 2 passed, 185 skipped.
- Independently ran `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..bbfdf35e26212b2a831bda5e31e1ef4433e41900`: exit 0 with no output.
- Static source and caller tracing covered the complete cumulative boundary. The broader recorded Rust/Python/TypeScript, codegen, docs, client-tier, PyO3-scope, and real-server journeys were inspected but not rerun in this Wave-1 review.
- The owner explicitly accepted the 22 earlier AI co-author trailers on 2026-09-14; this review does not reopen `FIND-TASK-002-18`.
- Pre-existing unrelated untracked content under `changes/active/verified-change-contract/architecture/verifier/` was present before this review and was not touched.

## Overall Result

**FAIL** — client behavior, credential handling, caller-derived tenancy, auth/RBAC routing, language projections, and the two R3 lifecycle corrections pass this boundary review. The candidate still exposes stale generated Bifrost contracts that contradict the exact runtime MCP/query/layout surfaces and must be removed before TASK-002/TASK-003 acceptance.

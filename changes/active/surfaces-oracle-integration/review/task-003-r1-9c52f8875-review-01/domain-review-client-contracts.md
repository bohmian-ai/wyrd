# Client, SDK, and Public Contract Domain Review

## Reviewed Boundary

- Immutable subject: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `9c52f887595d4e8a04764d3d2836926e5df70950`, candidate tree `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`.
- Approved authority: `changes/active/surfaces-oracle-integration/spec.md`, revision 8; original `TASK-002-converge-client-and-sdks.md`; prior TASK-002 reviews and retained findings; and `TASK-003-R1-close-cumulative-review-findings.md` without relying on its appended implementation summary.
- Governing authority read: `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`; `architecture/references/README.md`; and the applicable Rust, PyO3, Python, TypeScript, error, testing, and spec-driven-development references.
- Source boundary traced: `wyrd-spec` Bifrost wire/schema generation, OpenAPI, HTTP query/catalog routes, runtime MCP descriptors, `wyrd-client::Bifrost`, Rust/Python/TypeScript projections, package manifests, and user-journey owners.
- No `.codegraph/` index exists. Inspection used the immutable Git objects and repository search. Candidate source was not modified.

## Authority and Source Coverage

| Boundary | Required behavior | Candidate evidence | Result |
|---|---|---|---|
| Shared client ownership | REQ-016 through REQ-021; INV-004 through INV-006, INV-013 | `crates/shared/wyrd-client/src/lib.rs:1-25`; `bifrost/mod.rs:1-67`; thin Rust SDK re-export at `sdks/wyrd-sdk-rust/src/lib.rs:1-25`; Python and TypeScript native boundaries call `wyrd-client` | PASS |
| Rust/Python/TypeScript public capability | TASK-002 acceptance; AC-003, AC-004, AC-010, AC-021 | Existing language projections and real-server journeys remain present. Between prior cumulative candidate `bbfdf35e2` and this candidate, no SDK runtime source, `wyrd-client` runtime source, HTTP/OpenAPI source, or MCP/query source changed. | PASS |
| Stable errors and trust boundary | REQ-019A, REQ-022 through REQ-025; INV-007 | One `ClientConfig::credential`; catalog-backed shared Rust mapping; Python eight-field projection; generated TypeScript code union; no public request tenant selector | PASS |
| HTTP/OpenAPI contract | REQ-056, REQ-059; AC-002 | `crates/wyrd/wyrd-server/src/http/openapi.rs:14-43` includes registration, list, describe, query, and lifecycle paths; candidate `openapi.yaml` contains `/v1/bifrost/tables`, its description path, `/v1/query`, `/v1/query/running`, and `/v1/query/{request_id}` | PASS |
| Exact MCP contract | REQ-057; AC-019 | `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:32-39,71-78` defines and returns exactly list, describe, and query; the query input is closed and bounded at lines 139-195 | PASS |
| Generated schema authority | REQ-036, REQ-057, REQ-059; INV-010, INV-012; `FIND-TASK-002-19` | `crates/wyrd-spec/examples/gen_schemas.rs:60-67,200-225` emits the surviving `BifrostQueryRequest`, physical layout, table, lifecycle, and audit contracts only; all six stale generated/golden schema pairs are absent; the docs inventory contains none of them | PASS |
| Removed surfaces stay removed | REQ-017, REQ-057; INV-005, INV-010, INV-019 | Exact candidate search finds no `BifrostErrorDescriptor`, `BifrostPermissionDescriptor`, `SyncQueryRequest`, `QueryParam`, `PartitionColumnSpec`, `PartitionTransformWire`, `bifrost.list_errors`, or `bifrost.list_permissions` outside historical change artifacts | PASS |

## End-to-End Trace

- Public language constructors reach the single `wyrd_client::Bifrost` owner. `QueryClient` remains private, and raw `BifrostGrpcTransport` is exposed only behind `wyrd-client`'s `test-support` feature (`crates/shared/wyrd-client/src/bifrost/mod.rs:41-62`), which no SDK manifest enables.
- The live HTTP, gRPC, CLI, Python, TypeScript, and MCP query paths use `BifrostQueryRequest`. Its surviving generated schema publishes SQL, visibility, freshness, and the common `1..=u32::MAX` deadline range; none accepts tenant, execution path, topology, or plan selection.
- The server runtime advertises exactly `bifrost.list_tables`, `bifrost.describe_table`, and `bifrost.query`. The removed descriptor types had no runtime caller; deleting them leaves the live service owner intact rather than replacing it with another catalog.
- The remediation deletes the four zero-runtime source types, their generator/tests, the six generated schemas and six goldens, and their docs inventory entries. It adds no alias, tombstone, compatibility route, dependency, or cleanup abstraction.
- No production client/SDK change after `bbfdf35e2` can regress the previously closed credential, error, direct-write shutdown, or stream-settlement findings. The only later `wyrd-client` change is in `tests/pg_bifrost_e2e.rs`, where the owned-batch retry journey replaces a fixed sleep with a bounded wait for the existing retry owner.

## Material Proposed Findings

None.

`FIND-TASK-002-19` is closed. The generated/public contract now matches the runtime query, layout, error, and exact three-tool MCP authorities. Prior TASK-002 client findings remain closed at this boundary; no reachable regression or task-required missing behavior was found.

## Verification Limits

- Independently ran `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..9c52f887595d4e8a04764d3d2836926e5df70950`: exit 0, no output.
- Independently searched the immutable candidate for all four removed type names, both prohibited MCP names, and both orphan partition titles: zero live matches. Candidate tree inspection confirms all twelve stale generated/golden JSON files are absent.
- Inspected the exact base-to-candidate and `bbfdf35e2`-to-candidate diffs, current source/generator bodies, OpenAPI path inventory, runtime MCP descriptors, package manifests, and the existing language journey sources.
- The recorded `wyrd-spec` wire tests, MCP discovery journey, `codegen:check`, `docs:check`, Rust/Python/TypeScript journeys, boundary checks, and broad gate were reviewed but not rerun in this Wave-1 domain review. No Cargo command was started while other reviewers share the checkout.
- The working tree contains unrelated pre-existing changes under `changes/active/verified-change-contract/`; they are outside the immutable subject and were not touched.

## Overall Result

**PASS** — the candidate satisfies the public client/SDK/contract obligations in scope, closes `FIND-TASK-002-19` by deletion at the authoritative source and generated layers, preserves the single `wyrd-client::Bifrost` path and thin language projections, and publishes no stale Bifrost compatibility surface.

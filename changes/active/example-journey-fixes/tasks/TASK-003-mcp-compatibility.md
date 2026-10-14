# TASK-003 MCP compatibility with common frameworks

Spec: `../spec.md` REQ-003, AC-001..AC-003.

The `tools/list` cache-metadata fix is already on this branch. Make every
advertised tool (read and write, for an administrator holding every scope)
accepted by common consumers: input schemas with top-level `type: object` and
no top-level oneOf/anyOf/allOf/enum/const/not (OpenAI function tool rules;
today `operators.connections.create` violates this and OpenAI rejects the
whole tool list), and output schemas the official MCP SDK accepts. Keep each
tool's validation strength: express alternatives inside properties or by
server-side validation with stable errors, not by dropping checks.

Evidence: a TypeScript journey using the official MCP SDK
(`@modelcontextprotocol/client` or `@modelcontextprotocol/sdk`, whichever the
TS SDK workspace can depend on in tests) and the OpenAI Agents SDK's MCP tool
conversion against a real server: connect, list the full admin catalog,
assert every tool satisfies the rules above, and call a read tool; plus a Rust
test asserting the rule over the full catalog so it cannot regress.

## Evidence

Audit (every read and write descriptor, all scopes): the only input root with a
combinator was `operator_connections.create` (root `oneOf` over the
provider-tagged `CreateOperatorConnectionRequest`; the shared
`principals::tool` builder had papered over it by forcing `type: object`).
All output schemas already had an object root.

Fix: `operator_connections.create` now takes `{request: CreateOperatorConnectionRequest}`,
matching `operator_connections.update`; the union still validates under
`request`, decoded by the same secret-safe HTTP body decoder. The forced root
`type` insertion in `principals::tool` is removed, so a union input can no
longer be advertised silently.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every tool input root is `type: object` with no top-level oneOf/anyOf/allOf/enum/const/not | `wyrd-server/src/mcp/operators.rs` `CreateArgs`; `mcp/principals.rs::tool` | `mcp::tests::every_tool_schema_is_accepted_by_common_tool_callers`; `discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples` over the real 35-tool catalog | PASS |
| Output schemas accepted by the official MCP SDK | unchanged; object roots asserted | TS journey: `Client.listTools` + `callTool("wyrd.guide")` structured-content validation | PASS |
| Official TS MCP SDK and OpenAI Agents SDK connect, list full admin catalog, convert every tool, call a read tool | `sdks/wyrd-sdk-ts/wyrd/tests/integration/mcp-framework-compatibility.test.ts` | Fails on base (`operator_connections.create input root carries oneOf`, `operator_connections_create parameters root carries oneOf`), passes on candidate | PASS |
| Validation strength kept | union nested under `request`; `deny_unknown_fields` | `operators::pg_tests::an_agent_administers_redacted_connections` (create, refusals) | PASS |

Lane fix (diagnosed with an independent read-only diagnostician): the full
`ts:test:integration` lane booted all 28 files' servers at once (no vitest
config), saturating Postgres pool acquisition (`pool timed out`, opaque
`peer security audit cannot be composed` at boot) and pushing the two MCP
files without a `testTimeout` override past vitest's 5s default, which
includes file-fixture boot. `sdks/wyrd-sdk-ts/wyrd/vitest.config.ts` caps
workers at 6 (the nextest journey cap); both MCP files set the sibling
`testTimeout: 60_000`.

Commands:
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::tests::every_tool_schema_is_accepted_by_common_tool_callers) | test(=mcp::gateway::tests::gateway_catalog_annotates_every_administration_operation)'` PASS
- `mise run test:bifrost:journey:mcp` 19/19 PASS
- `mise run ts:test:integration` 28 files, 184 passed, 1 skipped
- `mise run ts:test:unit`, `ts:typecheck`, `ts:format:check`, `ts:lints`, `fmt`, `lints`, `docs:check`, `git diff --check` PASS

Non-goals untouched. Open: `peer_audit.rs` erases the sqlx cause
(`map_err(|_| ...)`), which made the boot failure opaque; not changed here.

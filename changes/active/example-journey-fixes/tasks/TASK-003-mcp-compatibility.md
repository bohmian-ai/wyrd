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

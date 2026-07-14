# Agent Harness

Wyrd is agent-first and headless. Contracts must be typed, errors must be
structured, and generated artifacts must be reproducible so coding agents
can inspect, use, validate, and repair Wyrd surfaces without prose parsing.

## Agent-Facing Surfaces

Agent-facing APIs include:

- MCP tools
- JSON schemas
- OpenAPI output
- CLI commands with machine-readable output
- Structured errors (Wyrd error catalog)
- Docs generated from source contracts
- Validation commands in `mise.toml`

Keep these surfaces deterministic and discoverable.

## Tool Contracts

MCP and CLI tools should have:

- Stable names
- Typed inputs and outputs (from `wyrd-spec` schemas)
- Stable error codes (from the `WyrdError` catalog)
- Concise descriptions
- Examples where useful
- Tests for schema and behavior

MCP tool reads are always available; writes require explicit scopes. Do
not make agents parse prose when a typed field can carry the same meaning.

## Validation

Validation lives where the durable contract lives. If multiple surfaces
need the same rule, put it in Rust and expose it outward through Python,
TypeScript, HTTP, MCP, CLI, and schemas.

Agent-friendly validation failures include:

- `code`
- `field`
- Invalid value when safe to show
- Expected format or range
- Remediation

Public error catalogs are generated from derive-backed `WyrdError`
metadata. Do not maintain a hand-written error-code list beside the Rust
enum — the enum attributes are the source for JSON catalogs, Python typed
exceptions, TypeScript union members, docs, OpenAPI fragments, and MCP
error descriptions.

## Generated Artifacts

Generated artifacts are part of the harness. Keep generation commands
stable and fail on drift:

- `mise run codegen:check` — fail on drift for openapi / schemas / MCP /
  pyi
- `mise run codegen:regen` — regenerate everything from source
- `mise run codegen:stubs` — regenerate Python stubs
- `mise run codegen:openapi` — emit `openapi.yaml` from `WyrdApiDoc`

If a schema, stub, OpenAPI file, or MCP artifact is wrong, fix the source
or generator instead of editing the artifact.

## Audit

Audit is foundational across CLI, UI, MCP, Python SDK, TypeScript SDK,
`wyrd-server`, and Vala surfaces. Every durable read and write appends an
`vala.audit_outbox` row in the same transaction as the mutation (see
`architecture/wyrd-design.md` §Audit). The single writer lives at
`crates/vala/vala-sql/src/queries/audit_outbox.rs::append_audit`; do not
create parallel writers.

## Prompts And Local Guidance

Repo-local skills and `AGENTS.md` are executable guidance for agents. Keep
them Wyrd-native, concise, and free of stale names. If a convention
becomes important enough to enforce, add a `mise` task or test instead of
relying only on prose.

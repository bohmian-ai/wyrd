# Agent Harness

Wyrd is agent-first and headless. Contracts must be typed, errors must be
structured, and generated artifacts must be reproducible so coding agents
can inspect, use, validate, and repair Wyrd surfaces without prose parsing.

## Agent-Facing Surfaces

Agent-facing APIs include:

- MCP tools
- JSON schemas
- The runtime OpenAPI document at `GET /openapi.json`
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

Authentication and authorization are server-owned. Tool input never supplies
trusted tenant or principal identity, and a write scope never bypasses the
owning domain authorization check. Bound request sizes, result sizes,
pagination, concurrency, deadlines, and cancellation so an authenticated tool
cannot become an unbounded resource path.

Before a tool causes the server to fetch a user- or tenant-supplied URL,
resolve it once, reject every forbidden effective address, and pin the
connection to the screened address. String-only validation followed by client
re-resolution is vulnerable to DNS rebinding.

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
enum. The enum attributes are the source for catalogs and public problem
metadata; each boundary projects that metadata without inventing another error
contract.

## Generated Artifacts

Generated artifacts are part of the harness. Keep generation commands
stable and fail on drift:

- `mise run codegen:check` — fail on drift for JSON schemas, language
  declarations, and public Python stubs
- `mise run codegen:regen` — regenerate every artifact owned by the
  code-generation lane from source
- `mise run codegen:stubs` — regenerate Python stubs

The OpenAPI document is not a generated artifact: `utoipa` builds it from the
server's handlers and the server serves it at `GET /openapi.json`, so OpenAPI
changes are proved against the served document by the assembled-server
contract suite (`crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs`, run by
`mise run test:principals:integration`) rather than by a drift snapshot.

If a schema, stub, or declaration is wrong, fix the source or generator
instead of editing the artifact. Runtime MCP catalogs are verified by their
owning MCP contract and behavior tests; do not infer MCP coverage from the
code-generation lane.

## Audit

Audit is foundational across CLI, UI, MCP, Python SDK, TypeScript SDK,
`wyrd-server`, and Vala surfaces, and it records authorization decisions rather
than engine mechanics. Except for the non-blocking paths named below, every
decision that evaluates a principal's permission appends one row — allowed and
denied alike — in the same transaction as the decision, before the operation
proceeds or refuses; a decision that cannot be recorded that way fails closed.
Engine-internal transitions that evaluate no permission are lineage in their
own operational tables, never audit.

Oracle read decisions, tenant tripwires, and gateway invocation decisions are
the named exceptions: they use the same canonical append from a tracked,
non-blocking task rather than the deciding transaction, so an authorized call
is not refused or delayed by the write. Gateway administration is not in that
set and stays transactional and fail-closed. Do not create alternative audit
writers.

`vala.audit_staging` is transient write-ahead state. Retained history is the
tenant-qualified Bifrost `vala.system.audit_log` projection, published through
Scribe and Forge. Staged rows are garbage-collected once the per-tenant
watermark has advanced past them. Agent-facing audit reads never treat staging
or a legacy direct-Iceberg projection as a second historical authority.

## Repository Guidance

`AGENTS.md`, `architecture/agent-rules.md`, and the routed architecture
references are executable repository authority. Keep them Wyrd-native,
concise, and free of historical names. Protect high-value invariants with the
compiler, a focused test, or a `mise` check when prose alone cannot prevent
drift.

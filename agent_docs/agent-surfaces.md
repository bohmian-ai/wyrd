# Agent surfaces

Load when changing MCP tools, CLI output, schemas, generated artifacts, or
anything an agent calls. Agents must be able to inspect, use, validate, and
repair Wyrd surfaces without parsing prose.

## Surfaces

MCP tools, JSON schemas, the runtime OpenAPI document (`GET /openapi.json`),
CLI commands with machine-readable output, the error catalog, docs generated
from source contracts, and the `mise` validation tasks. Keep them
deterministic and discoverable.

## Tool contracts

- Stable names; typed inputs and outputs from `wyrd-spec`; stable codes from
  the `WyrdError` catalog; concise descriptions; examples where useful; tests
  for schema and behavior.
- Read tools are always available; write tools require explicit scopes. A
  write scope never bypasses the owning domain's authorization check.
- Tenant and principal identity come from the server, never from tool input.
- Bound request size, result size, pagination, concurrency, deadlines, and
  cancellation so an authenticated tool cannot become an unbounded resource
  path.
- Before the server fetches a user- or tenant-supplied URL: resolve once,
  reject every forbidden effective address, and pin the connection to the
  screened address. See `architecture/wyrd-security-posture.md`.
- Never make an agent parse prose when a typed field can carry the meaning.

## Generated artifacts

- `mise run codegen:check` fails on drift in TypeScript wire types and error
  codes, UI problem examples, and theme tokens; `mise run codegen:regen`
  rebuilds them. Fix the source or generator, never the artifact.
- JSON Schemas are derived from `wyrd-spec` at runtime (MCP `tools/list`, the
  served OpenAPI) or transiently by the generator. None is committed.
- OpenAPI is built by `utoipa` from the mounted handlers. Prove a change
  against the served document with `mise run test:principals:integration`
  (`crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs`).
- The runtime MCP tool catalog is proved by the owning MCP tests, not by the
  codegen lane.
- Python `.pyi` stubs are authored, not generated; napi `index.d.ts` is
  generated. See the SDK `AGENTS.md` files.

# Agent docs

Topic guides loaded when relevant. The root `AGENTS.md` always applies; these
add detail without restating it.

## Topic guides

- [rust.md](rust.md): any Rust change. Struct-centered structure, imports,
  ownership, async, lints, rustdoc.
- [maintainer-style.md](maintainer-style.md): implementing or reviewing code
  for readability, in any language.
- [errors.md](errors.md): adding or changing an error a caller can see.
- [agent-surfaces.md](agent-surfaces.md): MCP tools, CLI output, schemas,
  OpenAPI, or generated artifacts.

## Directory rules

Read the nearest `AGENTS.md` when working in these directories:

- [sdks/wyrd-sdk-python/AGENTS.md](../sdks/wyrd-sdk-python/AGENTS.md): PyO3,
  Python package, stubs, Python tests.
- [sdks/wyrd-sdk-ts/AGENTS.md](../sdks/wyrd-sdk-ts/AGENTS.md): N-API binding,
  TypeScript package, TypeScript tests.
- [crates/wyrd/wyrd-sql/AGENTS.md](../crates/wyrd/wyrd-sql/AGENTS.md):
  `TenantConn`, `OperatorPool`, RLS, migrations.
- [docs/AGENTS.md](../docs/AGENTS.md): docs-site pages.

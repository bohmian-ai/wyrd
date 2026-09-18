---
task: TASK-008
title: SDK and MCP projection of the administrative contract
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-036, REQ-040, INV-014, AC-013, AC-014]
depends_on: [TASK-004, TASK-005, TASK-006]
---

## Objective

The administrative contract is usable from the first-class language surfaces.
`wyrd-client` owns the shared Rust client implementation; the Rust, Python, and
TypeScript SDKs project it for the operations each is intended to expose, and
the agent-facing MCP surface exposes the administrative operations that belong
there under explicit write scopes. One administrative identity model is
described consistently across HTTP, CLI, SDKs, MCP, schemas, stable errors, and
documentation.

## Constraints

- `crates/shared/wyrd-client` is the sole SDK-facing Rust client surface. SDK
  packages are thin over it and must not duplicate transport, validation,
  registry, storage, or lifecycle behavior.
- Client-tier crates do not depend on `sqlx`, cloud SDKs, `datafusion`, or
  `deltalake`.
- Only `wyrd-sdk-python` enables Python features; Rust and TypeScript SDKs never
  do. PyO3 stays out of `wyrd-spec`.
- No SDK, CLI, MCP tool, or UI becomes a durable source of truth for principals,
  credentials, tenants, or grants.
- MCP read tools are always available; administrative writes require explicit
  scopes.
- Credential plaintext crosses a client surface exactly once, in the response
  that created it, and is never persisted by a client.
- Do not hand-edit generated stubs or schemas; change the source or generator
  and regenerate.
- Non-goal: a UI. Non-goal: exposing platform-plane operations to tenant-scope
  clients.

## Relevant Surface

- `crates/shared/wyrd-client/` — shared client implementation and public
  re-exports.
- `sdks/wyrd-sdk-rust/`, `sdks/wyrd-sdk-python/`, `sdks/wyrd-sdk-ts/`.
- `crates/wyrd/wyrd-mcp/` — tool catalog and scopes.
- `crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs` — existing
  SDK-against-real-server journey shape.
- `mise.toml` — `check:client-tier`, `check:sdk-client-tier`,
  `check:sdk-pyo3-scope`, `codegen:check`.

## Approach

1. Add the administrative operations to the shared Rust client surface over the
   typed HTTP contract and its stable errors.
2. Project them through the Rust SDK, then the Python and TypeScript packages,
   regenerating types and stubs from source.
3. Expose the agent-facing administrative operations through MCP with read tools
   open and write tools scope-gated.
4. Add a journey per language surface proving the operations each exposes.
5. Reconcile documentation so one administrative identity model is described
   across every surface.

## Acceptance Criteria

- Each of the Rust, Python, and TypeScript SDKs performs the administrative
  operations it is intended to expose against a real server, including receiving
  a once-returned credential and using it on a subsequent call.
- A tenant-scope client cannot invoke a platform-plane operation through any SDK
  or MCP tool; the refusal is the stable contract error.
- MCP administrative write tools are unavailable without their explicit scope
  and available with it; read tools remain available.
- Generated types, stubs, schemas, and the OpenAPI contract regenerate cleanly
  with no hand edits.
- Boundary checks pass for client tier, SDK client tier, and PyO3 scope.
- No document, example, or surface still describes a second identity model, a
  removed bootstrap path, or credential-keyed authorization.

## Verification

Scope is `VER-001` through `VER-006`. Run only the language and boundary lanes
for the surfaces this task touches.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-mcp --all-targets
mise run check:client-tier
mise run check:sdk-client-tier
mise run check:sdk-pyo3-scope
mise run codegen:check
mise run py:format
mise run py:lints
mise run py:typecheck
mise run docs:check
```

Per-language journeys run against a real server with repository-managed
Postgres, following the existing SDK-against-fixture shape. Run the exact
nextest expressions for the Rust tests you add, `mise run py:test:unit` for
Python-visible surface changes, and the repository TypeScript integration task
for the TypeScript journey.

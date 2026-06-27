# Testing Workflows

Use repository tasks from `mise.toml`. Prefer targeted checks while iterating
and aggregate gates before completion.

## Rust

Targeted test:

```bash
cargo test -p <crate> <test_name> --all-features -- --nocapture --test-threads=1
```

Common gates:

```bash
mise run fmt
mise run lints
mise run test:unit
mise run check
```

Use single-threaded tests when filesystem, database, runtime, network ports, or
shared global state can collide.

## Python

After Rust changes that affect Python-visible behavior:

```bash
mise run py:setup
mise run py:test:unit
```

Add Python tests for public Python workflows, imports, exceptions, and stubs.

## Contracts And Codegen

After changing public contracts, schemas, API routes, MCP tools, stubs, or
generated artifacts:

```bash
mise run codegen:check
```

Do not hand-edit generated output.

## Boundary Gates

Use these when changing crate boundaries or foundation crates:

```bash
mise run check:client-tier
mise run check:pyo3-scope
mise run check:mocks-scope
mise run check:unwrap-audit
mise run check:wasm
```

## Test Design

Good tests:

- exercise public or crate-visible behavior
- cover success, stable failures, and edge cases
- use local fixtures and mock services
- avoid credentials for unit tests
- assert stable error codes where applicable
- keep generated output drift-free

Do not broaden tests into slow integration gates unless the touched behavior
requires it.

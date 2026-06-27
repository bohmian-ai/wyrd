# Implementation Rules

Use this as a quick review checklist. For Wyrd planning-repo work, prefer the
repo-local `wyrd-rust-python` and `review` skills when available.

## Doctrine Placement

Every implementation plan should answer:

- Which Wyrd noun stores the durable fact: `Card`, `Spec`, `Run`, or
  `Observation`?
- Which foundation carries identity, versioning, linkage, lifecycle, errors,
  serialization, or schema behavior?
- Which service owns each durable side effect?
- Which surface exposes the service without changing Wyrd vocabulary?

## Rust And Crates

- Put durable shared contracts in `wyrd-spec`.
- Keep `wyrd-spec` free of PyO3, async runtimes, filesystem IO, network IO,
  server frameworks, database clients, cloud SDKs, telemetry SDK implementation
  dependencies, object-store clients, Arrow, DataFusion, Iceberg, and Delta.
- Keep validation deterministic and side-effect-free in contract crates.
- Use domain types for durable identifiers.
- Use `thiserror` for library errors and `anyhow` only in binaries.
- Do not use `unwrap()` for environment, filesystem, network, parsing, user
  input, database, storage, or external-service behavior in non-test code.
- Public Wyrd contracts are exhaustive by default. Do not blanket-apply
  `#[non_exhaustive]`.
- Do not add wildcard dependency versions, compatibility aliases, or per-crate
  profile blocks.

## PyO3 And Python

- Feature-gate PyO3 into the crate that owns the Python-visible type.
- `wyrd-spec` has no `python` feature.
- `python/py-wyrd` is a thin aggregator: no business logic, no Python-free
  logic, and no Rust tests.
- Generic Python boundary helpers belong in `crates/shared/wyrd-utils`.
- Use `Bound<'py, T>` for new PyO3 code.
- Never hold a GIL-bound object across `.await`.
- Do not store `PyErr` in reusable Rust errors.
- Python-visible behavior is verified with `mise run py:setup` and
  `mise run py:test:unit`, not `cargo build` or `cargo test` on `py-wyrd`.
- Rust tests are encouraged for Python-free logic. Python-lifetime behavior
  belongs in Python tests.

## Cards And Artifacts

- Card objects are local holders and spec builders.
- Card `save(path, ...)` and `load(path, ...)` are local filesystem
  materialization and hydration only.
- Registration belongs to registry/client surfaces such as
  `wyrd.cards.register(card)`.
- `ArtifactCard` is the durable artifact record.
- Specs link to artifacts with `CardRef`.
- Do not introduce one-off durable artifact wrappers unless a locked decision
  explains why `ArtifactCard` plus `CardRef` cannot express the design.

## API, MCP, Errors, And Generated Contracts

- HTTP request/response bodies, MCP inputs/outputs, CLI machine output, schemas,
  and Python stubs are typed public contracts.
- Public errors crossing Rust, HTTP, Python, MCP, CLI, or generated docs use
  stable Wyrd error codes.
- MCP writes require scopes, idempotency, policy checks, and audit.
- Do not hand-edit generated schemas, OpenAPI, stubs, or generated API files.
- Public contract changes must name the source contract or generator and the
  verification command.

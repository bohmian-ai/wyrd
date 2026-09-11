---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-surfaces-oracle-integration
spec_revision: 5
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-016, REQ-017, REQ-018, REQ-019, REQ-019A, REQ-020, REQ-021, REQ-022, REQ-023, REQ-024, REQ-024A, REQ-024B, REQ-025, REQ-040, REQ-041, REQ-043, REQ-044, REQ-056, REQ-057, REQ-058, REQ-059, REQ-060, REQ-061, INV-001, INV-004, INV-005, INV-006, INV-010, INV-013, INV-019, INV-024, AC-002, AC-003, AC-004, AC-010, AC-019, AC-021]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Objective

Converge the authoritative Oracle `wyrd-client` and `wyrd-queue` behavior with
the preserved Surfaces Card and `WyrdState` workflows, yielding one shared
client implementation and thin first-class Rust, Python, and TypeScript SDKs.
All public Bifrost behavior enters through `wyrd_client::Bifrost`; public
errors, HTTP/gRPC, MCP, CLI, and generated client contracts agree.

## Constraints

- Oracle client and queue behavior supersedes the Surfaces implementations;
  do not textually blend away bounded Arrow ownership, settlement,
  backpressure, drain, credentials, TLS, transport, or error reconstruction.
- Preserve Surfaces Card authoring/loading, composite registration,
  references, Eval/Drift semantics, and `WyrdState` behavior.
- `crates/shared/wyrd-client` is the sole SDK-facing implementation.
  `sdks/wyrd-sdk-rust`, `sdks/wyrd-sdk-python`, and `sdks/wyrd-sdk-ts` are thin
  projections and cannot duplicate transport or durable server behavior.
- Only the Python SDK may enable retained owner-crate Python features. New or
  materially relocated Python logic belongs in the Python SDK; unrelated
  working wrappers need not move for directory purity.
- Do not expose Gate, Scribe, Oracle, Forge, `QueryClient`, or
  `BifrostGrpcTransport` as sibling client owners.
- Do not restore typed observation reads, legacy audit verification,
  `wyrd dev bootstrap`, obsolete error aliases, or live UI behavior.

## Relevant Surface

- `crates/shared/wyrd-client` and `crates/shared/wyrd-queue`
- Oracle Bifrost client behavior currently under `crates/vala/vala-sdk`
- `sdks/wyrd-sdk-rust`, `sdks/wyrd-sdk-python`, and `sdks/wyrd-sdk-ts`
- Preserved Card, loader, registry, `WyrdState`, and Python interface owners
- Public contracts in `crates/wyrd-spec`
- Bifrost HTTP/gRPC routes, `/mcp`, and `wyrd query` consumers
- Rust, Python, TypeScript, HTTP, CLI, and MCP journeys

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Adopt Oracle `wyrd-client` and `wyrd-queue` behavior, then reconnect the
   preserved non-Bifrost client consumers required by Surfaces.
2. Compose table management, buffered/direct Arrow ingestion, description,
   SQL, typed collection, streaming, lifecycle, and terminal settlement into
   the public `wyrd_client::Bifrost` facade.
3. Establish the three `sdks/` package roots as thin projections over
   `wyrd-client`, with foreign-runtime code only where Rust cannot own it.
4. Preserve Python sync/async, typed-row, PyArrow, Polars, pandas, and Arrow IPC
   ergonomics and TypeScript Arrow/model/typed-result ergonomics without
   independent transport implementations.
5. Converge Rust, Python, and TypeScript failures on the derive-backed catalog,
   including one eight-field Python `WyrdError` projection and generated
   TypeScript error-code union.
6. Reconcile terminal-safe HTTP/gRPC framing, exact MCP catalog, and CLI query
   behavior through the shared facade.
7. Reconcile source contracts and generators so every public Bifrost table,
   query, lifecycle, permission, streaming, and error surface is represented.

## Acceptance Criteria

- Rust, Python, and TypeScript expose one coherent SDK capability set from the
  three `sdks/` roots and consume `wyrd-client`; Rust and TypeScript dependency
  cones remain PyO3-free and client-tier dependency rules hold.
- Public Bifrost callers use one facade for table lifecycle, buffered and
  direct Arrow writes, flush/shutdown durability, description, SQL, typed
  collection, streaming, and running/status/cancel operations.
- Query streams enforce schema-once, batch, EOS, row-count, identity, and one
  terminal invariants; broken or dropped streams settle resources and never
  report successful partial results.
- Public Python sync and async journeys preserve all required authoring,
  conversion, iteration, lifecycle, and structured-error behavior through the
  public `wyrd` modules.
- TypeScript journeys preserve table declarations, buffered/direct Arrow
  writes, raw and typed SQL, Arrow-native streams, lifecycle, description,
  conversion, and runtime structured errors through `@wyrd/sdk`.
- Surfaces Card registration/loading and `WyrdState` workflows still pass
  through every affected public language, HTTP, CLI, and MCP surface.
- `/mcp` discovers exactly the three approved Bifrost tools and bounded query
  invocation returns a complete validated result or structured failure. CLI
  query stdout/stderr and input-selection behavior remain intact.
- Generated OpenAPI/protobuf/stubs/declarations contain the complete public
  contract, including Bifrost table routes omitted by the Oracle input, and no
  stale typed-read or parallel-client authority remains.
- Every expected Python platform failure is a catalog-backed `WyrdError` with
  direct `code`, `message`, `detail`, `details`, `remediation`, `status`,
  `title`, and `type`; prohibited exception classes, fake aliases, generic
  fallbacks, and aggregate `problem` attributes are absent.

## Verification

- `mise run fmt`
- `mise run lints`
- `mise run test:cards:unit`
- `mise run test:cards:integration`
- `mise run test:cli:journey`
- `mise run test:wyrdstate:journey`
- `mise run py:test:unit`
- `mise run py:test:cards:integration`
- `mise run py:test:wyrdstate:integration`
- `mise run py:test:integration`
- `mise run py:typecheck`
- `mise run ts:build`
- `mise run ts:typecheck`
- `mise run ts:test:unit`
- `mise run ts:test:integration`
- `mise run ts:napi:check`
- `mise run codegen:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:py-wheel-no-testing`
- `mise run check:single-into-response-impl`
- `mise run check:proto-drift`
- `git diff --check`

Run and record exact focused commands for the Rust SDK, Python, TypeScript,
MCP, CLI, Card, and `WyrdState` scenarios changed during implementation, plus
the dependency/feature evidence for each SDK. Do not run a Bifrost aggregate
in this task.

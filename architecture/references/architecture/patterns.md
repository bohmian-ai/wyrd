# Architecture Patterns

Wyrd is a **language-agnostic client/server platform — the platform agents
and users love to build on.** The server owns durable behavior and core
logic in Rust. Contracts live on the API wire through typed schemas,
HTTP/MCP payloads, generated docs, and stable errors so any language can
implement a client.

First-class SDKs ship for Python, Rust, and TypeScript. Go is planned but
not first-class until its SDK ships. First-class SDKs may add local
authoring helpers, OTEL hooks, and agent workflow integration; they must
not duplicate durable server behavior or make Wyrd language-exclusive.

Wyrd is agent-first and headless: MCP, CLI, HTTP, generated schemas,
stable errors, and machine-readable docs are primary surfaces. The
developer UI is supported, but not the source of truth.

## Ownership boundaries

Keep ownership at the narrowest Wyrd layer that has the behavior and its
dependency cost. `wyrd-spec` owns pure wire contracts and validation;
`wyrd-server` owns serving, tenancy, policy, audit, and durable orchestration;
Vala owns observations, evaluation, drift, and analytical engines; Skald owns
provider, prompt, tool, agent, and workflow primitives. `python/py-wyrd` and
the TypeScript surface project approved owner-crate behavior and do not create
parallel durable state.

Vala and Bifrost crates are engines/data-plane libraries, never network
servers. The server remains the only listener. Client-tier crates stay free of
analytical dependencies such as SQL, cloud SDKs, DataFusion, Iceberg, and
object-store engines. When a behavior crosses a boundary, put the shared
contract in `wyrd-spec`, durable behavior in its owner, and expose it through
typed HTTP/MCP/SDK projections.

For current owner paths and approved Python features, consult `AGENTS.md` and
the focused language references; this file documents structural patterns, not
a frozen package inventory.

## Contract Placement

Put shared wire contracts in `wyrd-spec` when they are used by more than
one surface, need schema generation, or form part of the durable public
API.

Keep `wyrd-spec` free of:

- PyO3
- async runtimes
- filesystem and network IO
- server frameworks
- database clients
- HTTP clients
- cloud SDKs
- telemetry SDK implementation dependencies

Python-visible wrappers around `wyrd-spec` contracts live in owner crates
(e.g. `wyrd-interfaces`, `wyrd-cards`) behind optional `python` features.
`python/py-wyrd` registers those wrappers; it does not reimplement logic.

`wyrd-spec` is foundational but not a dumping ground for all contracts.
If it is not spec-related, find another place for it.

## Canonical Rust Composition Pattern

Wyrd Rust code uses the required struct-centered hybrid style from
`AGENTS.md` §5. Behavior is organized around three distinct roles:

| Role | Owns | Shape |
|---|---|---|
| Domain value | Identity, invariants, validation, pure transformations | Struct, newtype, or closed enum with inherent methods |
| Service or handle | Clients, stores, configuration, runtime state, IO workflows | Concrete struct with explicit fields, constructor, and inherent methods |
| Pure helper | Stateless deterministic calculation or narrow conversion | Small module function |

`crates/shared/wyrd-registry/src/handle.rs::Cards` is the canonical service
pattern. `Cards` owns a shared `RegistryEngine`; callers discover registry
workflows through methods such as `register`, `get`, `load`, and `delete`.
Focused internal modules implement narrow stages, while the public handle owns
the capability and workflow boundary.

```text
Cards
└── RegistryEngine
    ├── WyrdClient
    └── storage client

Cards methods             public and internal workflows
Focused private methods   stateful workflow stages
Module helper functions   pure validation and transformation only
```

When several functions repeatedly accept the same dependency bundle or
context, replace that functional call graph with one cohesive owning struct.
When a function is entirely determined by its inputs and has no natural owner,
keep it free. Do not introduce zero-sized utility structs, broad manager
objects, or traits around a single implementation.

Synchronous methods are the default. Add async methods only for workflows that
actually await IO, and keep their synchronous planning, validation, and
transformation stages synchronous. Every item in either path requires rustdoc
that explains intent, operation, workflow role, and errors in accordance with
`AGENTS.md` §16.

Card envelopes are the explicit exception to an active-record interpretation.
They are declarative domain values and may own construction, validation, and
pure transformations. They do not own registry, storage, policy, audit, or
server lifecycle clients. Those operations stay on the corresponding service
or handle.

## Server Pattern

Server handlers:

- Own durable side effects and never rely on a language SDK as the source
  of truth.
- Preserve tenant isolation across identity, authz, registry, storage,
  policy, audit, observability, evaluation, and generated artifacts.
- Accept typed request structs.
- Return typed response structs or structured Wyrd errors (via one
  `IntoResponse` mapper; enforced).
- Carry trace instrumentation (`#[tracing::instrument]` with scrubbed
  args).
- Attach request and audit context for durable writes.
- Avoid cloning heavy state.
- Avoid constructing clients or pools inside handlers.

Shared application state is explicit. Use `Arc` for heavy clients only
when shared ownership is real.

## Client Pattern

Clients (Rust, Python, TypeScript SDKs):

- Project server contracts without renaming durable fields.
- Use API-wire schemas and stable error codes as the compatibility
  boundary.
- Keep local helpers local — authoring, local save/load, local
  validation messages, tracing hooks, runtime integrations are allowed.
- Do not bypass registry, storage, policy, audit, tenancy, relationship,
  or status ownership from a convenience path.

## Storage And Registry Pattern

- Keep durable metadata contracts typed.
- Keep backend-specific behavior behind backend modules.
- Keep encryption and key handling centralized.
- Do not bypass registry, storage, or audit invariants from convenience
  paths.
- Card registry writes stay inside the caller's `TenantConn` tx
  (enforced by `check:registry-tx-coupling`).
- Single `wyrd.cards` table; no per-kind shadow tables (enforced by
  `check:registry-single-table`).
- Use local fixtures + emulators for storage tests; real cloud
  integration tests run separately.

## Provider Runtime Pattern

Provider code separates:

- Wyrd-level provider capability contracts.
- Provider-specific auth and wire payloads.
- Retry, timeout, and transport behavior.
- Response normalization and typed usage metadata.

Do not scatter provider string checks across unrelated crates. Add typed
capability or provider metadata instead.

## Observability And Evaluation Pattern

Observability code is explicit about:

- Tenant or namespace
- Time range
- Trace/request/run identifiers
- Retention assumptions
- Projection and pruning behavior
- Ingestion vs query responsibilities

Every observation row carries `card_ref` (per row, server-authorized) plus
opaque client-generated `run_id`.

Evaluation code keeps deterministic assertions deterministic.
Model-based judging is isolated from assertion logic and tested with mock
provider responses.

## Audit Pattern

Audit is foundational across every surface. Every durable read and write
appends an `vala.audit_outbox` row in the same transaction as the
mutation. The single writer is
`crates/vala/vala-sql/src/queries/audit_outbox.rs::append_audit`. Do not
create parallel writers.

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

## Crate Ownership (Actual Inventory)

### `crates/wyrd-spec`

Pure contracts, IDs, cards/specs, schema generation, request/response
shapes, validation, stable error catalog. PyO3-free, IO-free, async-free.

### `crates/wyrd/*` — control plane

- `wyrd` — public umbrella / sanctioned re-exports.
- `wyrd-auth` — server-tier auth domain logic.
- `wyrd-cards` — Card storage + Card type definitions (Python owner).
- `wyrd-cli` — human-facing CLI.
- `wyrd-config` — configuration management (Python owner).
- `wyrd-interfaces` — serialization, compression, Card IO (Python owner).
- `wyrd-mcp` — agent-facing MCP tool surface.
- `wyrd-server` — HTTP server and application state; **the only serving
  surface**.
- `wyrd-sql` — durable Postgres schema, migrations, query compilation for
  Wyrd artifacts. No `axum`/`hyper`/`tower` (enforced).
- `wyrd-storage` — server-tier storage handles, cloud backend signers
  (S3/GCS/Azure), OpenDAL operator.
- `wyrd-testing` — `WyrdTestServer` fixtures + HTTP testing utilities
  (Python owner, dev-only wheel).
- `wyrd-tonic` — gRPC server/client bindings (single owner of tonic-family
  deps; enforced by `check:no-tonic-outside-wyrd-tonic`).

### `crates/shared/*` — cross-cutting foundation

- Auth: `wyrd-auth-check`, `wyrd-auth-issue`, `wyrd-auth-oidc`,
  `wyrd-auth-verify`.
- Runtime: `wyrd-runtime` (async runtime + PyO3 sync/async bridge),
  `wyrd-queue` (bounded producer + BatchSink seam).
- Domain primitives: `wyrd-semver`, `wyrd-version`.
- Observability: `wyrd-telemetry`.
- Transport / client: `wyrd-client`.
- Security / crypto: `wyrd-crypt`.
- Test infra: `wyrd-dev-fixtures`, `wyrd-test-contract-macros`.
- Utilities: `wyrd-utils` (Python owner), `wyrd-error-derive`.

Client-tier crates do not depend on `sqlx`, cloud SDKs, `datafusion`, or
`deltalake` (enforced by `check:client-tier`).

### `crates/skald/*` — LLM runtime plane

- `skald-spec` — Skald contracts (no PyO3, no server deps).
- `skald-observer` — Observer trait + implementations for agent/workflow
  events (Python owner).
- `skald-providers` — provider registry + driver interface.
- `skald-runtime` — native provider runtime dispatch + mock provider seam
  (Python owner).
- `skald-cache` — agent result caching.
- `skald-prompt` — prompt generation + templating (Python owner).
- `skald-tool` — tool definition + execution (Python owner).
- `skald-agent` — live agent: identity, provider, tools, tool loop
  (Python owner).
- `skald-workflow` — DAG scheduler, tasks, cross-provider handoff (Python
  owner).

Skald does not depend on Vala.

### `crates/vala/*` — observability + analytical plane

- `vala-core` — server-internal analytical + alert-routing core.
- `vala-sdk` — client SDK: Bifrost ingest sink, pooled producers, observe
  surface (Python owner).
- `vala-sql` — Vala Postgres schema and migrations (`vala.file_list`,
  `vala.cluster_nodes`, `vala.audit_outbox`, `vala.maintenance_leases`,
  etc.). Outlives individual Bifrost engine implementations.
- `vala-ingest` — data ingestion pipeline.
- `vala-bifrost` — OLAP write/read engine on Apache Iceberg + DataFusion.
- `vala-drift` — in-memory PSI/SPC/Custom drift baseline fit + scoring.
- `vala-eval` — DAG-stage execution, operators, scoring, aggregation,
  comparison.

Vala may depend on Skald for reusable agent evaluation. `vala-*` crates
are engines/libraries — they do **not** own HTTP/gRPC serving.

### `python/py-wyrd` — Python extension aggregator

Thin PyO3 module root. Registers approved owner-crate submodules; does
not duplicate validation, lifecycle, registry, storage, or runtime logic.

### Planned But Not Yet Present

- **`wyrd-sdk`** — approved Python owner per `AGENTS.md` §2; not yet in
  tree.
- **`crates/bindings/*`** — TypeScript/napi and future native SDK
  bindings; not yet in tree. TypeScript SDK today talks to `wyrd-server`
  over HTTP + gRPC.

Do not assume these paths exist when writing code today.

## Approved Python Owner Crates

Twelve crates enable a `python` feature (enforced by
`check:pyo3-scope`):

`wyrd-cards`, `wyrd-config`, `wyrd-interfaces`, `skald-observer`,
`wyrd-testing` (dev-only), `wyrd-utils`, `skald-agent`, `skald-prompt`,
`skald-runtime`, `skald-tool`, `skald-workflow`, `vala-sdk`.

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

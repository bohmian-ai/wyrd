# Architecture

Wyrd is a Rust-first platform with Python, HTTP, MCP, CLI, and UI surfaces over
shared contracts. Keep behavior in the owning layer and expose typed boundaries.

## Crate Ownership

- `crates/wyrd-spec`: pure contracts, IDs, cards/specs, validation, errors, and
  schema generation.
- `crates/shared/wyrd-runtime`: shared runtime boundary.
- `crates/shared/wyrd-telemetry`: telemetry setup and shared tracing helpers.
- `crates/shared/wyrd-auth-verify`: auth verification contracts and shared
  auth types that do not require server-only dependencies.
- `crates/shared/wyrd-utils`: filesystem, codec, JSON, and optional Python
  boundary helpers.
- `crates/shared/wyrd-crypt`: cryptographic helpers.
- `crates/shared/wyrd-testing`: test helpers only.
- `crates/skald`: provider runtime, prompts, cache, orchestration, and model
  interaction.
- `crates/vala`: observability, drift, evaluation, traces, archival storage,
  and query behavior.
- `crates/wyrd/wyrd-interfaces`: Python-visible data/model interface wrappers
  and shared card helper surfaces.
- `crates/wyrd/wyrd-cards`: Python-visible Card surfaces and card-local
  authoring helpers.
- `crates/wyrd/wyrd-server`: HTTP server and application state.
- `crates/wyrd/wyrd-cli`: human-facing CLI.
- `crates/wyrd/wyrd-mcp`: agent-facing tool surface.
- `python/py-wyrd`: Python package, extension-module root, generated stubs,
  and PyO3 submodule aggregation.

## Contract Placement

Put shared wire contracts in `wyrd-spec` when they are used by more than one
surface, need schema generation, or form part of the durable public API.

Keep `wyrd-spec` free of:

- PyO3
- async runtimes
- filesystem and network IO
- server frameworks
- database clients
- HTTP clients
- cloud SDKs
- telemetry SDK implementation dependencies

Python-visible wrappers around `wyrd-spec` contracts live in owner crates such
as `wyrd-interfaces` and `wyrd-cards`, behind optional `python` features.
`python/py-wyrd` registers those wrappers; it does not reimplement their logic.

## Server Pattern

Server handlers should:

- Accept typed request structs.
- Return typed response structs or structured Wyrd errors.
- Carry trace instrumentation.
- Attach request and audit context for durable write operations.
- Avoid cloning heavy state.
- Avoid constructing clients or pools inside handlers.

Shared application state should be explicit. Use `Arc` for heavy clients only
when shared ownership is real.

## Storage And Registry Pattern

Storage, registry, and artifact behavior should be capability-driven:

- Keep durable metadata contracts typed.
- Keep backend-specific behavior behind backend modules.
- Keep encryption and key handling centralized.
- Do not bypass registry, storage, or audit invariants from convenience paths.
- Use local fixtures for tests instead of live cloud services.

## Provider Runtime Pattern

Provider code should separate:

- Wyrd-level provider capability contracts.
- Provider-specific auth and wire payloads.
- Retry, timeout, and transport behavior.
- Response normalization and typed usage metadata.

Do not scatter provider string checks across unrelated crates. Add typed
capability or provider metadata instead.

## Observability And Evaluation Pattern

Observability code must be explicit about:

- tenant or namespace
- time range
- trace/request identifiers
- retention assumptions
- projection and pruning behavior
- ingestion vs query responsibilities

Evaluation code should keep deterministic assertions deterministic. Model-based
judging should be isolated from assertion logic and tested with mock provider
responses.

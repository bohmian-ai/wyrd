---
name: wyrd-rust-python
description: Repo-local Wyrd skill for Rust core, Python bindings, PyO3, maturin, cards, specs, registry, storage, server/client contracts, telemetry, provider runtime, observability, evaluation, Vala OLAP warehouse, Bifrost, Apache Iceberg, DataFusion, object-store analytical storage, CLI, MCP, generated stubs, and cross-language tests. Use before editing non-UI Wyrd Rust or Python code, especially code under crates/, python/py-wyrd, schemas, OpenAPI generation, Python-visible APIs, or Vala warehouse/OLAP/Iceberg implementation. Do not use for Svelte UI work; use wyrd-ui instead.
---

# Wyrd Rust/Python

Use this skill before touching Rust, Python, PyO3, server, client, card/spec,
registry, storage, runtime, telemetry, provider, observability, evaluation,
Vala OLAP warehouse, Bifrost, Iceberg, DataFusion, CLI, MCP, codegen, or
cross-language behavior in Wyrd.

This skill is Wyrd-native. Do not paste legacy names, package names, route
prefixes, module names, compatibility shims, or migration shorthand into this
repository. If a prior implementation pattern is useful, reproduce the pattern
in Wyrd vocabulary and Wyrd paths only.

## Output Style

Write for a human maintainer who needs the decision quickly.

- Be succinct and direct. Prefer short sentences over dense architecture prose.
- Lead with the concrete issue, change, or result before explaining context.
- Use Wyrd doctrine terms when they matter, but do not stack abstractions.
- Name files, crates, commands, and verification gates explicitly.
- Avoid metaphor, invented labels, and broad summary language such as
  "surface alignment" when a specific boundary, type, route, or test can be
  named.
- If a rule is subtle, explain it in one plain paragraph and then give the
  action to take.
- For implementation summaries, report what changed and what was verified.
  Do not restate the whole doctrine unless the user asked for it.

## First Pass

Before editing:

1. Read `AGENTS.md`.
2. Read `architecture/wyrd-design.md` before changing card/spec, registry,
   storage, server/client, CLI, MCP, Python SDK, PyO3, generated contract, or
   implementation-plan behavior. It is the active design authority.
3. Identify the owning crate or Python package.
4. Inspect the nearest existing Wyrd implementation and tests.
5. Check `mise.toml` for the canonical verification command.
6. Check `Cargo.toml`, crate manifests, `pyproject.toml`, and lockfiles before
   relying on version-specific behavior.

Do not invent a new architecture until the current Wyrd boundary proves wrong
for the user workflow.

## CodeGraph Hydration

When implementing from a thin task contract
(`.dev/plan/<feature>/tasks/NN-*.md`), the contract pins decisions and names seams
as symbols; it intentionally does **not** render code. Hydrate the seam source
live:

- Use `codegraph_explore` to load the named seams' verbatim source and call paths
  before writing code. Do not run a manual grep/read loop and do not expect the
  plan to contain the code — the plan names `consume_active_refresh`; CodeGraph
  gives you its current body and callers.
- Honor the contract's decisions, seams, and invariants exactly. If the contract
  is wrong or under-specified, stop and report — do not improvise architecture.

Load these references from the shared doctrine library
(`.claude/references/`, indexed in `.claude/references/README.md`) only when
relevant:

- Rust ownership, traits, async, allocation, and core API shape:
  `.claude/references/rust-python/rust-core.md`
- Crate boundaries, server/client contracts, storage, registry, runtime,
  telemetry, provider, and observability ownership:
  `.claude/references/architecture/patterns.md`
- PyO3 classes, GIL rules, Python object lifetimes, and boundary conversion:
  `.claude/references/rust-python/pyo3-boundaries.md`
- Wyrd error codes, Rust errors, Python exceptions, and HTTP problem payloads:
  `.claude/references/rust-python/errors.md`
- Python exports, generated stubs, package layout, and cross-language API
  checks: `.claude/references/rust-python/python-api-and-stubs.md`
- Test selection, linting, codegen, and completion checks:
  `.claude/references/rust-python/testing-workflows.md`
- Agent-facing contracts, MCP, structured validation, and harness behavior:
  `.claude/references/rust-python/agent-harness.md`
- Vala OLAP warehouse, Bifrost, Apache Iceberg, DataFusion, object-store
  analytical storage, Iceberg catalogs, and Parquet query/write paths:
  `.claude/references/domain/iceberg-bifrost.md`

## Ownership Boundaries

- `crates/wyrd-spec`: PyO3-free pure contracts, ids, cards/specs, schema
  generation, request/response shapes, validation, and stable error catalog.
- `crates/shared/*`: shared runtime, telemetry, auth shell, cryptography,
  testing, utilities, derives, and `wyrd-utils` Python helpers behind its
  optional `python` feature.
- `crates/skald/*`: model/provider runtime, prompt/cache abstractions,
  orchestration, tool registry, workflow runtime, and provider-specific wire
  handling. Python-visible Skald behavior lives in the owning Skald crate
  behind its optional `python` feature.
- `crates/vala/*`: observability, evaluation, drift, tracing, archival query,
  OLAP, and background data-plane behavior. Python-visible Vala client
  behavior lives in `vala-sdk` behind its optional `python` feature.
- `crates/wyrd/*`: server, CLI, MCP, application integration, UI host,
  `wyrd-interfaces`, and `wyrd-cards`.
- `python/py-wyrd`: thin PyO3 module aggregator, Python package exports,
  generated stubs, and Python-facing tests.

If behavior crosses boundaries, put the durable contract in `wyrd-spec`, keep
runtime implementation in the owning crate, and expose only the necessary API
through server/Python/client layers.

Skald owns reusable agent primitives. Vala may depend on Skald to implement a
reusable agent evaluation engine that runs online or offline without
`wyrd-server`; Skald must remain independent of Vala. Crate ownership includes
dependency cost: keep specialized dependencies in the narrowest behavioral
owner instead of moving them into broadly consumed crates to centralize config.

## Platform Posture

- Wyrd follows a language-agnostic client/server model. The server owns durable
  behavior and core logic; clients project API-wire contracts.
- Core durable logic is Rust-only server/service logic. Contracts cross the API
  wire through typed schemas, HTTP/MCP payloads, generated docs, and stable
  errors so any language can implement a client.
- Rust and Python are first-class client languages. They may receive richer SDK
  ergonomics, local authoring helpers, OTEL integration, agent workflow
  integration, and test tooling where useful.
- First-class Rust/Python support must not make Wyrd language-exclusive and
  must not move server-owned durable behavior into client packages.
- Wyrd must run self-hosted and as a cloud SaaS product. Enterprise SaaS paths
  require full tenant separation for identity, authz, registry, storage,
  policy, audit, observability, evaluation, and generated artifacts.
- Wyrd is agent-first and headless. MCP, CLI, HTTP, generated schemas, stable
  errors, and machine-readable docs are primary surfaces. The developer UI is
  supported, but it is not the source of truth.

## Rust Core Rules

- Keep core behavior in Rust. Python should be typed and ergonomic, not a
  duplicate implementation.
- Use domain types instead of raw strings for durable identifiers.
- Prefer `&str`, `&Path`, `&[T]`, and typed references when ownership is not
  needed.
- Treat `.clone()` as a design question. Use it only for concrete ownership
  needs, small boundary values, or `Arc::clone` for real shared state.
- Use `thiserror` for crate-local library error enums. Use `anyhow` only in binaries.
- Use the derive-backed `wyrd_spec::error::WyrdError` catalog for public errors
  that cross HTTP, Python, MCP, CLI, or generated-documentation boundaries.
- Register public error metadata with `#[wyrd_error(code = "...", status = N,
  title = "...", remediation = "...")]`; do not hand-write parallel
  `code()`, `status()`, `remediation()`, or problem-json logic.
- Use `tracing` with structured fields for diagnostics.
- Use `secrecy::SecretString` for secrets and redacted custom `Debug` impls for
  secret-bearing structs.
- Do not use `unwrap()` for environment, filesystem, network, parsing, user
  input, database, storage, or external-service behavior in non-test code.
- Use `expect()` only for true invariants, with a message naming the invariant.
- Do not add wildcard dependency versions or per-crate profile blocks.

## Abstraction Rules

Choose abstraction deliberately:

- Use concrete types when there is one implementation.
- Use enums for closed sets where exhaustiveness matters.
- Use traits when multiple real implementations share stable behavior.
- Use generics for hot-path static dispatch.
- Use `Box<dyn Trait>` only when runtime extensibility is intentional.
- Keep traits small and capability-focused.
- Avoid broad platform traits created for one caller.
- Avoid `Arc<Mutex<T>>` by default; first check whether ownership, immutable
  state, a narrower lock, or message passing fits.

## Async And Runtime Rules

- Use async at IO boundaries: HTTP, database, storage, queues, network calls,
  and server handlers.
- Keep pure computation synchronous unless the caller requires async.
- Use the shared Wyrd runtime boundary for Python async/sync bridging.
- Do not create ad hoc Tokio runtimes in library or PyO3 code.
- Do not block inside async request paths without an explicit blocking strategy.
- Use bounded concurrency and timeouts for external calls when available.

## PyO3 Boundary Rules

- `wyrd-spec` stays PyO3-free. Do not add a `python` feature or PyO3 imports
  to it.
- PyO3 belongs in crates that own Python-visible behavior, behind an optional
  `python` feature with `pyo3 = { workspace = true, optional = true }`.
- `python/py-wyrd` enables approved owner-crate `python` features and
  registers submodules. It must stay a thin aggregator with no duplicated
  validation, lifecycle, registry, storage, or runtime logic.
- Generic Python boundary helpers belong in `crates/shared/wyrd-utils` behind
  its `python` feature.
- Keep `Python<'py>`, `Bound<'py, T>`, `Py<T>`, and `PyErr` out of crates that
  did not opt into a `python` feature.
- Convert Python inputs at the boundary, then call Rust-native APIs.
- Use `Bound<'py, T>` for new PyO3 code.
- Convert to `Py<T>` before storing Python objects across awaits, threads, or
  long-lived state.
- Never hold a `Bound<'py, T>` across `.await`.
- Release the GIL for blocking disk or network work that is not already routed
  through async infrastructure.
- Do not store `PyErr` in reusable Rust errors. Store typed Rust errors or
  string-backed boundary variants and convert to Python exceptions at the edge.
- Avoid `#[pyo3(get)]` on owned `String`, `Vec<T>`, or `HashMap<K, V>` fields in
  hot paths. Prefer manual accessors returning borrow-equivalent values where
  practical.
- For nested Python-visible objects, prefer manual getters/setters when
  automatic extraction would hide clones or lifetime behavior.

## Python API And Stubs

For any Python-visible change, verify all layers:

1. Rust type/function exists in the owning crate.
2. PyO3 wrapper or registration exists in the owning crate behind its
   `python` feature.
3. `python/py-wyrd/src` registers the owning crate's submodule or exported
   class/function.
4. Python package exports exist under `python/py-wyrd/python/wyrd`.
5. Generated stubs are updated through the repository generator.
6. Python tests import from public `wyrd` modules, not private extension paths,
   unless the private path is the intended contract.

Do not hand-edit generated stubs. Update source annotations or the generator,
then run the codegen task.

## Server And Contract Rules

- Server code owns durable behavior, side effects, tenancy checks, registry
  writes, storage orchestration, policy decisions, audit records, and generated
  relationship/status state.
- Client code may own ergonomic authoring helpers, local save/load, local
  validation messages, tracing hooks, and runtime integrations, but it must not
  become the durable source of truth.
- Public request/response bodies are typed structs.
- Wire types derive schema support where required by the current feature gate.
- Public handlers must return structured Wyrd errors.
- Write handlers with trace instrumentation.
- Durable write operations must carry the audit/request context required by the
  local server pattern.
- Keep versioned API contracts explicit.
- Do not add compatibility routes or aliases for old surfaces.
- Preserve tenant isolation across every public and internal server path.

## Provider, Evaluation, And Observability Rules

- Keep provider-specific request/response details behind provider modules.
- Preserve typed capability descriptions instead of scattering stringly checks.
- Keep evaluation tasks deterministic when they are defined as assertions.
- Keep trace and observation identifiers typed and validated.
- Keep archival/query code explicit about tenant, time range, projection,
  pruning, and retention assumptions.
- Prefer local mock servers and fixtures over live provider calls in unit tests.
- Do not require credentials for unit tests.

## Testing Workflow

Prefer the narrowest meaningful verification while iterating:

```bash
cargo test -p <crate> <test_name> --all-features -- --nocapture --test-threads=1
```

Use repo tasks before completion when relevant:

```bash
mise run fmt
mise run lints
mise run test:unit
mise run check
mise run codegen:check
mise run pre-pr
```

For Python-visible Rust changes:

```bash
mise run py:setup
mise run py:test:unit
```

For contract or generated artifact changes:

```bash
mise run codegen:check
```

For foundation boundary changes:

```bash
mise run check:client-tier
mise run check:pyo3-scope
mise run check:mocks-scope
mise run check:unwrap-audit
```

## Completion Standard

A change is not done until:

- The implementation matches the owning Wyrd crate's local patterns.
- New core behavior has Rust tests when practical.
- Python-visible behavior has Python coverage or a documented reason it does
  not.
- Public contracts regenerate cleanly when touched.
- No legacy names, routes, package names, or compatibility aliases were added.
- **`mise run pre-pr` passes green.** This is the final gate and it is
  non-negotiable — a feature is not complete until it is green.
  - It does not matter if the gate was already red on the base branch.
    Inheriting a red gate does not excuse shipping red; make it green.
  - Do not declare a gate "environment-blocked" without proof the environment
    genuinely cannot run it. `pre-pr` needs only Docker plus the local toolchain,
    and `PgFixture`/embedded-Postgres tests need neither an external database nor
    Docker. Run it before claiming it cannot be run.
  - Do not circumvent the gate to make it pass: never weaken or disable a check,
    add `#[allow]`, delete or `#[ignore]` a failing test, or broaden a boundary
    glob to hide a real violation. Fix the underlying cause. Only use a check's
    own sanctioned mechanism (e.g. the documented per-file allowlist) when the
    usage is legitimately test-only and matches an existing in-pattern precedent.

---
name: wyrd-rust-python
description: Repo-local Wyrd skill for Rust core, Python bindings, PyO3, maturin, cards, specs, registry, storage, server/client contracts, telemetry, provider runtime, observability, evaluation, CLI, MCP, generated stubs, and cross-language tests. Use before editing non-UI Wyrd Rust or Python code, especially code under crates/, python/py-wyrd, schemas, OpenAPI generation, or Python-visible APIs. Do not use for Svelte UI work; use wyrd-ui instead.
---

# Wyrd Rust/Python

Use this skill before touching Rust, Python, PyO3, server, client, card/spec,
registry, storage, runtime, telemetry, provider, observability, evaluation, CLI,
MCP, codegen, or cross-language behavior in Wyrd.

This skill is Wyrd-native. Do not paste legacy names, package names, route
prefixes, module names, compatibility shims, or migration shorthand into this
repository. If a prior implementation pattern is useful, reproduce the pattern
in Wyrd vocabulary and Wyrd paths only.

## First Pass

Before editing:

1. Read `AGENTS.md`.
2. Identify the owning crate or Python package.
3. Inspect the nearest existing Wyrd implementation and tests.
4. Check `mise.toml` for the canonical verification command.
5. Check `Cargo.toml`, crate manifests, `pyproject.toml`, and lockfiles before
   relying on version-specific behavior.

Do not invent a new architecture until the current Wyrd boundary proves wrong
for the user workflow.

Load these references only when relevant:

- Rust ownership, traits, async, allocation, and core API shape:
  `references/rust-core.md`
- Crate boundaries, server/client contracts, storage, registry, runtime,
  telemetry, provider, and observability ownership:
  `references/architecture.md`
- PyO3 classes, GIL rules, Python object lifetimes, and boundary conversion:
  `references/pyo3-boundaries.md`
- Wyrd error codes, Rust errors, Python exceptions, and HTTP problem payloads:
  `references/errors.md`
- Python exports, generated stubs, package layout, and cross-language API
  checks: `references/python-api-and-stubs.md`
- Test selection, linting, codegen, and completion checks:
  `references/testing-workflows.md`
- Agent-facing contracts, MCP, structured validation, and harness behavior:
  `references/agent-harness.md`

## Ownership Boundaries

- `crates/wyrd-spec`: pure contracts, ids, cards/specs, schema generation,
  request/response shapes, validation, stable error codes.
- `crates/shared`: shared runtime, telemetry, auth shell, cryptography, testing,
  utilities, and derives.
- `crates/skald`: model/provider runtime, prompt/cache abstractions,
  orchestration, and provider-specific wire handling.
- `crates/vala`: observability, evaluation, drift, tracing, archival query, and
  background data-plane behavior.
- `crates/wyrd`: server, CLI, MCP, application integration, and UI host.
- `python/py-wyrd`: PyO3 module root, Python package exports, stubs, and
  Python-facing tests.

If behavior crosses boundaries, put the durable contract in `wyrd-spec`, keep
runtime implementation in the owning crate, and expose only the necessary API
through server/Python/client layers.

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

- Keep PyO3 in `python/py-wyrd*` unless an explicit allowlist says otherwise.
- Keep `Python<'py>`, `Bound<'py, T>`, `Py<T>`, and `PyErr` out of Rust-only
  core crates.
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
2. PyO3 wrapper or registration exists under `python/py-wyrd/src`.
3. Python package exports exist under `python/py-wyrd/python/wyrd`.
4. Generated stubs are updated through the repository generator.
5. Python tests import from public `wyrd` modules, not private extension paths,
   unless the private path is the intended contract.

Do not hand-edit generated stubs. Update source annotations or the generator,
then run the codegen task.

## Server And Contract Rules

- Public request/response bodies are typed structs.
- Wire types derive schema support where required by the current feature gate.
- Public handlers must return structured Wyrd errors.
- Write handlers with trace instrumentation.
- Durable write operations must carry the audit/request context required by the
  local server pattern.
- Keep versioned API contracts explicit.
- Do not add compatibility routes or aliases for old surfaces.

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
mise run check:wasm
```

## Completion Standard

A change is not done until:

- The implementation matches the owning Wyrd crate's local patterns.
- New core behavior has Rust tests when practical.
- Python-visible behavior has Python coverage or a documented reason it does
  not.
- Public contracts regenerate cleanly when touched.
- No legacy names, routes, package names, or compatibility aliases were added.
- Relevant `mise` or targeted cargo checks have been run, or the blocker is
  reported clearly.

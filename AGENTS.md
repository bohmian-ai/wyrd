# AGENTS.md

Wyrd repository-wide standards for agentic and human contributors. Skills,
phase plans, and CI gates reference this file as the source of truth. When a
phase plan, skill, or PR doc references "AGENTS.md", it means **this file**.

This document is **Wyrd-native**. Do not import legacy names, package names,
route prefixes, module names, compatibility shims, or migration shorthand into
this repository. Reproduce useful patterns under Wyrd vocabulary and Wyrd paths.

## 1. First Pass Before Editing

1. Read this file (AGENTS.md).
2. Read `docs/src/content/docs/concepts/core-doctrine.mdx` before changing
   Wyrd contracts, public or internal APIs, SDK surfaces, CLI, MCP, UI, docs,
   generated schemas, or implementation behavior. The canonical planning
   source is
   `/Users/stevenforrester/Documents/GitHub/wyrd-plan/architecture/v1/00-foundations/core-doctrine.md`.
3. Identify the owning crate or Python package (see §3 Ownership Boundaries).
4. Inspect the nearest existing Wyrd implementation and tests.
5. Check `mise.toml` for the canonical verification command.
6. Check `Cargo.toml`, crate manifests, `pyproject.toml`, and lockfiles before
   relying on version-specific behavior.

Do not invent a new architecture until the current Wyrd boundary proves wrong
for the user workflow.


## 2. Current Decisions

Design dialogue, locked decisions, predecessor research, and session history
live in the [`wyrd-plan`](https://github.com/wyrd-ai/wyrd-plan) repo (private,
org-internal). `PLAN.md` at the repo root carries the pointer plus the current
session slug.

Locked cross-cutting decisions that any contributor must honor:

- The core doctrine in `docs/src/content/docs/concepts/core-doctrine.mdx`
  is the first design filter for Wyrd nouns, layers, services, and public
  surfaces. Internal APIs, external APIs, Python SDK, HTTP, CLI, MCP, UI, docs,
  generated schemas, and agent-facing contracts must align with it.
- Wyrd is the AI layer for human and agentic workflows, not a general-purpose framework or
  runtime for arbitrary code execution.
- Every registered artifact is a `Card` with the shared envelope:
  `apiVersion: wyrd/v1`, top-level `metadata`, `kind` (one of the 18 card
  kinds), `spec` (the kind-specific payload), server-derived
  `relationships`, and server-managed `status`. There is no outer
  `kind: Card` wrapper; `kind` is the body discriminator. Rust shape:
  `struct Card { api_version, metadata, body: CardBody, relationships,
  status }` where `body` is `#[serde(tag="kind", content="spec")] enum
  CardBody { Model(ModelSpec), Data(DataSpec), ... }` flattened into the
  envelope. **Locked 2026-05-21.**
- `CardRef` carries `kind`, `name`, one `version` field, optional `space`, and
  optional `uid`. Do not introduce `version_req`.
- `wyrd-spec` is PyO3-free, IO-free, async-free, and foundational.
- Client-tier crates do not depend on `sqlx`, cloud SDKs, `datafusion`, or
  `deltalake`.
- Vala and Skald do not depend on each other directly.
- MCP is first-class; read tools are always available, write tools require
  explicit scopes.
- Audit is foundational across CLI, UI, MCP, Python SDK, wyrd-server, and
  vala-server.

## 3. Ownership Boundaries

- `crates/wyrd-spec`: pure contracts, ids, cards/specs, schema generation,
  request/response shapes, validation, stable error catalog.
- `crates/shared/*`: shared runtime, telemetry, auth shell, cryptography,
  testing, derives.
- `crates/skald/*`: model/provider runtime, prompt/cache abstractions,
  orchestration, provider-specific wire handling.
- `crates/vala/*`: observability, evaluation, drift, tracing, archival query,
  background data-plane behavior.
- `crates/wyrd/*`: server, CLI, MCP, application integration, UI host.
- `python/py-wyrd`: PyO3 module root, Python package exports, stubs,
  Python-facing tests.

When behavior crosses boundaries, put the durable contract in `wyrd-spec`, keep
runtime in the owning crate, and expose only the necessary API through
server/Python/client layers.

## 4. Rust Core Rules

- Keep core behavior in Rust. Python should be typed and ergonomic, not a
  duplicate implementation.
- Use domain types instead of raw strings for durable identifiers
  (`TenantId`, `RunId`, `CardUid`, etc.).
- Use `#[non_exhaustive]` on public `wyrd-spec` structs and enums.
- Prefer `&str`, `&Path`, `&[T]`, and typed references when ownership is not
  needed.
- Treat `.clone()` as a design question. Allowed only for concrete ownership
  needs, small boundary values, or `Arc::clone`/`Bytes::clone` for real shared
  state. Per-crate `CLONES.md` may whitelist additional cases.
- Use `thiserror` for crate-local library error enums. Use `anyhow` only in
  binaries.
- Use the derive-backed `wyrd_spec::error::WyrdError` catalog for public errors
  that cross HTTP, Python, MCP, CLI, or generated-documentation boundaries.
- Register public error metadata with
  `#[wyrd_error(code = "...", status = N, title = "...", remediation = "...")]`.
  Never hand-write parallel `code()`, `status()`, `remediation()`, or
  problem-json logic.
- Use `tracing` with structured fields for diagnostics.
- Use `secrecy::SecretString` for secrets and redacted custom `Debug` impls for
  secret-bearing structs.
- Do not use `unwrap()` for environment, filesystem, network, parsing, user
  input, database, storage, or external-service behavior in non-test code.
- Use `expect()` only for true invariants, with a message naming the invariant.
- Do not add wildcard dependency versions or per-crate profile blocks.
- All cargo invocations use `--all-features` unless gating a specific feature
  surface.

## 5. Abstraction Rules

- Concrete types when there is one implementation.
- Enums for closed sets where exhaustiveness matters.
- Traits when multiple real implementations share stable behavior.
- Generics for hot-path static dispatch.
- `Box<dyn Trait>` only when runtime extensibility is intentional.
- Keep traits small and capability-focused.
- Avoid broad platform traits created for one caller.
- Avoid `Arc<Mutex<T>>` by default; first check whether ownership, immutable
  state, a narrower lock, or message passing fits.

## 6. Async And Runtime Rules

- Use async at IO boundaries: HTTP, database, storage, queues, network calls,
  server handlers.
- Keep pure computation synchronous unless the caller requires async.
- Use the shared Wyrd runtime boundary for Python async/sync bridging.
- Do not create ad hoc Tokio runtimes in library or PyO3 code.
- Do not block inside async request paths without an explicit blocking
  strategy.
- Use bounded concurrency and timeouts for external calls when available.

## 7. PyO3 Boundary Rules

- Keep PyO3 in `python/py-wyrd*` unless an explicit allowlist says otherwise.
- Keep `Python<'py>`, `Bound<'py, T>`, `Py<T>`, `PyErr` out of Rust-only core
  crates.
- Name the `#[new]` method `fn __new__` (not `fn new`) and give it an explicit
  `#[pyo3(signature = (...))]`. The Rust source mirrors the Python slot it
  exposes. Builder-only pyclasses constructed via `#[staticmethod]` take no
  `#[new]`.
- Convert Python inputs at the boundary, then call Rust-native APIs.
- Use `Bound<'py, T>` for new PyO3 code.
- Convert to `Py<T>` before storing Python objects across awaits, threads, or
  long-lived state.
- Never hold a `Bound<'py, T>` across `.await`.
- Release the GIL for blocking disk or network work that is not already routed
  through async infrastructure.
- Do not store `PyErr` in reusable Rust errors. Store typed Rust errors or
  string-backed boundary variants and convert to Python exceptions at the edge.
- Avoid `#[pyo3(get)]` on owned `String`, `Vec<T>`, `HashMap<K, V>` fields in
  hot paths. Prefer manual accessors returning borrow-equivalent values where
  practical.
- For nested Python-visible objects, prefer manual getters/setters when
  automatic extraction would hide clones or lifetime behavior.

## 8. Python API And Stubs

For any Python-visible change, verify all layers:

1. Rust type/function exists in the owning crate.
2. PyO3 wrapper or registration exists under `python/py-wyrd/src`.
3. Python package exports exist under `python/py-wyrd/python/wyrd`.
4. Generated stubs (`.pyi`) regenerate cleanly via `mise run codegen:check`.
5. Python tests import from public `wyrd` modules, not private extension
   paths, unless the private path is the intended contract.

Do not hand-edit generated stubs. Update source annotations or the generator,
then run codegen.

## 9. Server And Contract Rules

- Public request/response bodies are typed structs.
- Wire types derive schema support where required by the feature gate.
- Public handlers return structured Wyrd errors via the `WyrdError` derive.
- Write handlers with trace instrumentation (`#[tracing::instrument]` with
  scrubbed args).
- Durable write operations carry the audit/request context required by the
  local server pattern.
- Versioned API contracts are explicit.
- Do not add compatibility routes or aliases for old surfaces.

## 10. Provider, Evaluation, Observability Rules

- Keep provider-specific request/response details behind provider modules.
- Preserve typed capability descriptions instead of stringly checks.
- Keep evaluation tasks deterministic when defined as assertions.
- Keep trace and observation identifiers typed and validated.
- Keep archival/query code explicit about tenant, time range, projection,
  pruning, retention assumptions.
- Prefer local mock servers and fixtures over live provider calls in unit
  tests.
- Do not require credentials for unit tests.

## 11. Testing Workflow

Narrowest meaningful verification while iterating:

```bash
cargo test -p <crate> <test_name> --all-features -- --nocapture --test-threads=1
```

Repo tasks before completion:

```bash
mise run fmt
mise run lints
mise run test:unit
mise run check
mise run codegen:check
mise run pre-pr
```

Python-visible Rust changes:

```bash
mise run py:setup
mise run py:test:unit
```

Contract or generated artifact changes:

```bash
mise run codegen:check
```

Foundation boundary changes:

```bash
mise run check:client-tier
mise run check:pyo3-scope
mise run check:mocks-scope
mise run check:unwrap-audit
mise run check:wasm
```

## 12. Completion Standard

A change is not done until:

- The implementation matches the owning Wyrd crate's local patterns.
- New core behavior has Rust tests when practical.
- Python-visible behavior has Python coverage or a documented reason it does
  not.
- Public contracts regenerate cleanly when touched.
- No legacy names, routes, package names, or compatibility aliases were added.
- Relevant `mise` or targeted cargo checks have been run, or the blocker is
  reported clearly.

## 13. Git Identity Rules

- Use your locally configured Git identity.
- Never sign commits as anyone else.
- Never add AI co-author trailers.
- Never run `git config` to alter identity.
- If git config is wrong, stop and surface to the user.

## 14. Session-Driven Planning

Planning lives in the [`wyrd-plan`](https://github.com/wyrd-ai/wyrd-plan) repo.
Code in this repo lands one session at a time, via dialogue-locked decisions.

- Active session: `wyrd-plan/sessions/CURRENT.md` (empty between sessions).
- Per-session folder: `wyrd-plan/sessions/SS-NNN-<slug>/` with five files —
  `00-audit.md` (predecessor citations), `01-questions.md`,
  `02-answers.md`, `03-lock-summary.md`, `04-pr.md`.
- Locked architecture: `wyrd-plan/architecture/v1/` files with
  `status: locked` front-matter. Each lock has a `wyrd-plan/CHANGELOG.md`
  entry and lands in `wyrd-plan/STATUS.md`.
- One session in flight at a time. No parallel sessions, no parallel chunks.
- Code follows lock. A wyrd PR cites the session folder and the CHANGELOG
  slug. Demo command in `04-pr.md` must pass from a fresh clone before close.
- Phase planning skills (`wyrd-phase-planner`, `wyrd-phase-reviewer`,
  `wyrd-plan-implementor`, `wyrd-plan-interviewer`, `wyrd-migration-planner`)
  read this file first, then `wyrd-plan/README.md`.

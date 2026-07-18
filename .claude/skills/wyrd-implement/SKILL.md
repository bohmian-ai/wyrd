---
name: wyrd-implement
description: Repo-local Wyrd implementation skill. Use before editing non-UI Wyrd code — Rust core, Python SDK, TypeScript SDK, PyO3, maturin, cards, specs, registry, storage, server/client contracts, telemetry, providers, observability, evaluation, Vala/Bifrost OLAP, Iceberg, DataFusion, CLI, MCP, generated stubs, and cross-language tests. Covers everything under crates/, python/py-wyrd, TypeScript SDK, schemas, OpenAPI generation, Python-visible APIs, and warehouse work. Do not use for Svelte UI work; use wyrd-ui instead.
---

# Wyrd Implement

Use this skill before touching non-UI Wyrd code. It covers Rust, Python,
PyO3, TypeScript, server, client, card/spec, registry, storage, runtime,
telemetry, provider, observability, evaluation, Vala/Bifrost, Iceberg,
DataFusion, CLI, MCP, codegen, and cross-language behavior.

Wyrd is **language-agnostic — the platform agents and users love to build
on**. First-class SDKs ship for Python, Rust, and TypeScript today. Go is
planned but not first-class until its SDK ships.

This skill is Wyrd-native. Do not import legacy names, package names,
route prefixes, module names, compatibility shims, or migration shorthand.
If a prior implementation pattern is useful, reproduce it in Wyrd
vocabulary and Wyrd paths only.

## Output Style

Write for a human maintainer who needs a coherent decision quickly.

- Be succinct and direct. Prefer short sentences over dense architecture
  prose.
- Lead with the concrete issue, change, or result before explaining
  context.
- Use Wyrd doctrine terms when they matter, but do not stack abstractions.
- Name files, crates, commands, and verification gates explicitly.
- Avoid metaphor, invented labels, and broad summary language such as
  "surface alignment" when a specific boundary, type, route, or test can
  be named.
- If a rule is subtle, explain it in one plain paragraph and then give
  the action to take.
- For implementation summaries, report what changed and what was
  verified. Do not restate the whole doctrine unless the user asked for
  it.

## First Pass

Before editing:

1. Read `AGENTS.md`.
2. Read `architecture/wyrd-design.md` — the active design authority.
3. Read `architecture/wyrd-doctrine.mdx` before changing contracts,
   public or internal APIs, SDK surfaces, CLI, MCP, UI, docs, generated
   schemas, or implementation behavior.
4. Identify the owning crate or Python package (see §Ownership Boundaries).
5. Inspect the nearest existing Wyrd implementation and tests.
6. Check `mise.toml` for the canonical verification command.
7. Check `Cargo.toml`, crate manifests, `pyproject.toml`, and lockfiles
   before relying on version-specific behavior.

Do not invent a new architecture until the current Wyrd boundary proves
wrong for the user workflow.

### Mandatory reference gate

Before any edit, identify the applicable files in the reference table below,
read each one completely to EOF, and record a short read ledger in the working
update. A truncated tool result is not a complete read; continue in chunks.
Do not call an editing tool until the ledger names every loaded reference and
the decision it governs. If a required reference cannot be read, stop and
report the blocker.

## CodeGraph Hydration (if available)

`.dev/` is gitignored — the plan files described here exist only in the
local checkout of whichever maintainer is driving the work, not in the
shared repo. Skip this section if you have no task contract in front of
you.

When implementing from a thin task contract
(`.dev/plan/<feature>/tasks/NN-*.md`), the contract pins decisions and
names seams as symbols; it intentionally does **not** render code.
Hydrate the seam source live:

- Use `codegraph_explore` (if the project is CodeGraph-indexed — a
  `.codegraph/` directory at the repo root) to load the named seams'
  verbatim source and call paths before writing code. Do not run a
  manual grep/read loop and do not expect the plan to contain the code
  — the plan names `consume_active_refresh`; CodeGraph gives you its
  current body and callers.
- Honor the contract's decisions, seams, and invariants exactly. If the
  contract is wrong or under-specified, stop and report — do not
  improvise architecture.

## References

Load from the shared doctrine library
(`architecture/references/`, indexed in
`architecture/references/README.md`) only when relevant to the current
change:

| Path | Load when the change touches |
|---|---|
| `architecture/references/doctrine/positioning-and-vocabulary.md` | Wyrd vocabulary, Card envelope, `CardRef` shape, v1 kinds, deleted concepts |
| `architecture/references/doctrine/architecture-constraints.md` | wyrd/vala/skald boundaries, `wyrd-spec` free-of list, deployment topologies, observation identity |
| `architecture/references/architecture/patterns.md` | Full crate inventory, contract placement, server/client/storage/provider/observability/audit patterns |
| `architecture/references/languages/rust-core.md` | Rust ownership, traits, async, allocation, zero-cost abstractions, idiomatic examples, anti-patterns |
| `architecture/references/languages/pyo3-boundaries.md` | PyO3 classes, `fn __new__` rule, GIL, lifetimes, boundary conversion, module registration |
| `architecture/references/languages/errors.md` | Wyrd error codes, `WyrdError` derive, boundary conversion, Rust/Python/TS/HTTP/CLI mapping |
| `architecture/references/languages/python-api-and-stubs.md` | Python exports, generated stubs, package layout, test conventions |
| `architecture/references/languages/testing-workflows.md` | Three-tier test taxonomy, targeted `mise` tasks, boundary checks |
| `architecture/references/languages/agent-harness.md` | Agent-facing contracts, MCP, structured validation, audit foundation |
| `architecture/references/languages/typescript-guide.md` | `@wyrd/sdk` conventions, high-performance TS patterns, declaration do's/don'ts, napi bridge |
| `architecture/references/domain/iceberg-bifrost.md` | Vala OLAP, Bifrost engine, Iceberg, DataFusion, object-store analytical storage |

## Ownership Boundaries

Full inventory in `architecture/references/architecture/patterns.md`. In
short:

- `crates/wyrd-spec` — pure contracts, IDs, cards/specs, schema
  generation, validation, stable error catalog. PyO3-free, IO-free,
  async-free, SQL-free.
- `crates/wyrd/*` — control plane: `wyrd`, `wyrd-auth`, `wyrd-cards`,
  `wyrd-cli`, `wyrd-config`, `wyrd-interfaces`, `wyrd-mcp`,
  `wyrd-server`, `wyrd-sql`, `wyrd-storage`, `wyrd-testing`,
  `wyrd-tonic`.
- `crates/shared/*` — auth-*, `wyrd-client`, `wyrd-crypt`,
  `skald-observer`, `wyrd-queue`, `wyrd-runtime`, `wyrd-semver`,
  `wyrd-telemetry`, `wyrd-utils`, `wyrd-version`, `*-derive`/`*-macros`,
  test infra.
- `crates/skald/*` — `skald-spec`, `skald-providers`, `skald-runtime`,
  `skald-cache`, `skald-prompt`, `skald-tool`, `skald-agent`,
  `skald-workflow`.
- `crates/vala/*` — `vala-core`, `vala-sdk`, `vala-sql`, `vala-ingest`,
  `vala-bifrost`, `vala-drift`, `vala-eval`.
- `python/py-wyrd` — thin PyO3 aggregator; submodules today: `agent`,
  `cards`, `config`, `tool`, `prompt`, `providers`, `bifrost`, `observe`,
  `testing` (dev-only feature).

**Approved Python-owner crates (12 today, enable `python` feature):**
`wyrd-cards`, `wyrd-config`, `wyrd-interfaces`, `skald-observer`,
`wyrd-testing`, `wyrd-utils`, `skald-agent`, `skald-prompt`,
`skald-runtime`, `skald-tool`, `skald-workflow`, `vala-sdk`. Enforced by
`check:pyo3-scope`.

**Planned but not yet in tree:** `wyrd-sdk` (approved Python owner per
`AGENTS.md` §2, not yet created); `crates/bindings/*` (TypeScript/napi
and future native bindings). Do not assume these paths exist when writing
code today.

If behavior crosses boundaries, put the durable contract in `wyrd-spec`,
keep durable runtime behavior in the owning crate, and expose only the
necessary API through server / Python / Rust / TypeScript client layers.

Skald owns reusable agent primitives. Vala may depend on Skald to
implement a reusable agent evaluation engine that runs online or offline
without `wyrd-server`; Skald must remain independent of Vala. `wyrd-server`
consumes the Vala evaluation engine but does not own evaluation-engine
logic.

Crate ownership includes dependency cost: keep specialized dependencies
in the narrowest behavioral owner instead of moving them into broadly
consumed crates to centralize config.

## Platform Posture

- Wyrd is **language-agnostic** — the platform agents and users love to
  build on. First-class SDKs ship for Python, Rust, and TypeScript today.
  Go is planned and becomes first-class only when its SDK and the same
  contract/journey gates ship. Language agnosticism is the doctrine;
  first-class SDK support is how we deliver ergonomics on top of it
  without moving durable behavior out of the server.
- Wyrd follows a language-agnostic client/server model. The server owns
  durable behavior and core logic; clients project API-wire contracts.
- Core durable logic is Rust-only server/service logic. Contracts cross
  the API wire through typed schemas, HTTP/MCP payloads, generated docs,
  and stable errors so any language can implement a client.
- First-class SDKs may receive richer ergonomics (local authoring
  helpers, OTEL integration, agent workflow integration, test tooling).
  They must not make Wyrd language-exclusive and must not move
  server-owned durable behavior into client packages.
- Wyrd runs self-hosted, cloud SaaS (single-server multi-tenant), and
  enterprise cloud (single-server single-tenant). SaaS and enterprise
  deployments require full tenant separation for identity, authz,
  storage, registry, policy, audit, observability, evaluation, and
  generated artifacts.
- Wyrd is agent-first and headless. MCP, CLI, HTTP, generated schemas,
  stable errors, and machine-readable docs are primary surfaces. The
  developer UI is supported, but not the source of truth.
- **`wyrd-server` is the only serving surface.** `vala-*` crates are
  engine/data-plane libraries — never HTTP/gRPC serving crates.
- **The governance token is deleted.** No `WYRD_GOV_TOKEN`, no
  `wyrd.auth_governance_tokens`, no `Scope::TokenIssue`. Auth is a single
  plane. The JWT (`principal.card_ref`) plus opaque client-generated
  `run_id` carry everything. Do not reintroduce.

## Bifrost Engine (Vala)

Bifrost has four internal roles in one binary: **Gate** (auth + dispatch),
**Scribe** (WAL + memtable + Parquet seal), **Forge** (single-writer
Iceberg committer + compaction), **Oracle** (fused scan + read audit).
Registered as handlers on `wyrd-server` — the only serving surface. See
`.dev/plan/not-started/06-bifrost-rebuild/` for the source of truth and
`architecture/references/domain/iceberg-bifrost.md` for engine doctrine.

## Observation & WyrdState

- One JWT can carry multiple component cards (nested service).
- Each observation row carries `card_ref` (per row, server-authorized —
  not trusted from the client) plus opaque client-generated `run_id`.
- Run IDs are opaque client-side execution records, not server-persisted.
- `WyrdState` is the client-side runtime handle owned today by `vala-sdk`
  (`wyrd-sdk` is planned; not yet in tree); it hydrates card context and
  ties observations to Card+Run.
- See `architecture/wyrd-design.md` §Observation identity for the full
  contract.

## Rust Core Rules

- Keep core behavior in Rust. Python and TypeScript should be typed and
  ergonomic, not duplicate implementations.
- Use domain types instead of raw strings for durable identifiers
  (`TenantId`, `RunId`, `CardUid`, etc.).
- Prefer `&str`, `&Path`, `&[T]`, and typed references when ownership is
  not needed.
- Treat `.clone()` as a design question. Allowed only for concrete
  ownership needs, small boundary values, or `Arc::clone`/`Bytes::clone`
  for real shared state. Per-crate `CLONES.md` may whitelist additional
  cases.
- Use `thiserror` for crate-local library error enums. Use `anyhow` only
  in binaries.
- Use the derive-backed `wyrd_spec::error::WyrdError` catalog for public
  errors that cross HTTP, Python, TypeScript, MCP, CLI, or generated-doc
  boundaries.
- Register public error metadata with
  `#[wyrd_error(code = "...", status = N, title = "...", remediation = "...")]`.
  Never hand-write parallel `code()`, `status()`, `remediation()`, or
  problem-json logic.
- Use `tracing` with structured fields for diagnostics.
- Use `secrecy::SecretString` for secrets and redacted custom `Debug`
  impls for secret-bearing structs.
- Do not use `unwrap()` for environment, filesystem, network, parsing,
  user input, database, storage, or external-service behavior in non-test
  code.
- Use `expect()` only for true invariants, with a message naming the
  invariant.
- Do not add wildcard dependency versions or per-crate profile blocks.
- Lints, format checks, and workspace type-checks use `--all-features`.
  Test tasks declare only the minimal feature set they need —
  `--all-features` in a test task forces recompiles at a different
  feature-union and defeats artifact reuse.

Concrete examples: `architecture/references/languages/rust-core.md`.

## Abstraction Rules

- Concrete types when there is one implementation.
- Enums for closed sets where exhaustiveness matters.
- Traits when multiple real implementations share stable behavior.
- Generics for hot-path static dispatch.
- `Box<dyn Trait>` only when runtime extensibility is intentional.
- Keep traits small and capability-focused.
- Avoid broad platform traits created for one caller.
- Avoid `Arc<Mutex<T>>` by default; first check whether ownership,
  immutable state, a narrower lock, or message passing fits.

## Async And Runtime Rules

- Use async at IO boundaries: HTTP, database, storage, queues, network
  calls, server handlers.
- Keep pure computation synchronous unless the caller requires async.
- Use the shared `wyrd-runtime` bridge for Python async/sync bridging. Do
  not create ad hoc Tokio runtimes in library or PyO3 code.
- Do not block inside async request paths without an explicit blocking
  strategy.
- Use bounded concurrency and timeouts for external calls.

## PyO3 Boundary Rules

- `wyrd-spec` stays PyO3-free. Do not add a `python` feature or PyO3
  imports to `wyrd-spec`.
- PyO3 belongs in crates that own Python-visible behavior, behind an
  optional `python` feature with
  `pyo3 = { workspace = true, optional = true }`.
- `python/py-wyrd` depends on approved owner crates with
  `features = ["python"]` and registers their submodules. It stays a
  thin aggregator: no duplicated validation, lifecycle, registry,
  storage, or runtime logic.
- Generic Python boundary helpers belong in `crates/shared/wyrd-utils`
  behind its `python` feature.
- Keep `Python<'py>`, `Bound<'py, T>`, `Py<T>`, and `PyErr` out of
  crates that did not opt into a `python` feature.
- Name the `#[new]` method `fn __new__` (not `fn new`) and give it an
  explicit `#[pyo3(signature = (...))]`. The Rust source mirrors the
  Python slot it exposes. Builder-only pyclasses constructed via
  `#[staticmethod]` take no `#[new]`.
- Convert Python inputs at the boundary, then call Rust-native APIs.
- Use `Bound<'py, T>` for new PyO3 code.
- Convert to `Py<T>` before storing Python objects across awaits,
  threads, or long-lived state.
- Never hold a `Bound<'py, T>` across `.await`.
- Release the GIL for blocking disk or network work that is not already
  routed through async infrastructure.
- Do not store `PyErr` in reusable Rust errors. Store typed Rust errors
  or string-backed boundary variants and convert to Python exceptions at
  the edge.
- Avoid `#[pyo3(get)]` on owned `String`, `Vec<T>`, or `HashMap<K, V>`
  fields in hot paths. Prefer manual accessors returning
  borrow-equivalent values.
- For nested `#[pyclass]` fields, prefer manual `#[getter]`/`#[setter]`
  when automatic extraction would hide clones or lifetime behavior.
- `wyrd-testing`'s `python` feature exposes the `WyrdTestServer` harness
  only; it is a test-tier crate and must never be enabled on production
  Python wheels. Enforced by `check:py-wheel-no-testing`.

Concrete PyO3 patterns:
`architecture/references/languages/pyo3-boundaries.md`.

## Python API And Stubs

For any Python-visible change, verify all six layers:

1. Rust type/function exists in the owning crate.
2. PyO3 wrapper or registration exists under the owning crate behind its
   `python` feature.
3. `python/py-wyrd/src` registers the owning crate's submodule.
4. Python package exports exist under `python/py-wyrd/python/wyrd`.
5. Generated stubs regenerate cleanly (`mise run codegen:check`).
6. Python tests import from public `wyrd` modules unless the private
   path is the intended contract.

Do not hand-edit generated stubs. Update source annotations or the
generator, then run codegen.

## TypeScript Rules

- Public TypeScript surface lives in `@wyrd/sdk`; napi-generated
  `index.d.ts` is committed and verified by `mise run ts:napi:check`.
- Mirror the `WyrdError` catalog as a discriminated union generated from
  the same source. Do not invent TypeScript-only error names.
- Prefer `unknown` over `any`; use union types over enums; use branded
  newtypes for identifiers; use tagged unions for outcomes.
- Follow the high-performance patterns and mkosir style highlights in
  `architecture/references/languages/typescript-guide.md` (monomorphic
  shapes, batching napi crossings, `Map`/`Set` for large collections,
  bounded concurrency with `Promise.all`/`allSettled`, explicit
  timeouts).

## Server And Contract Rules

- Server code owns durable behavior, side effects, tenancy checks,
  registry writes, storage orchestration, policy decisions, audit
  records, and generated relationship/status state.
- Client code (Rust, Python, TypeScript SDKs) may own ergonomic
  authoring helpers, local save/load, local validation messages,
  tracing hooks, and runtime integrations, but must not become the
  durable source of truth.
- Public request/response bodies are typed structs.
- Wire types derive schema support where required by the feature gate.
- Public handlers return structured Wyrd errors via the `WyrdError`
  derive and flow through one server `IntoResponse` mapper (enforced by
  `check:single-into-response-impl`).
- Write handlers use `#[tracing::instrument]` with scrubbed args.
- Durable writes carry audit/request context.
- Versioned API contracts are explicit; no compatibility routes or
  aliases for old surfaces.
- Preserve tenant isolation across every public and internal server
  path.

## Provider, Evaluation, Observability Rules

- Keep provider-specific request/response details behind provider
  modules.
- Preserve typed capability descriptions instead of stringly checks.
- Keep evaluation tasks deterministic when defined as assertions.
- Keep trace and observation identifiers typed and validated.
- Keep archival/query code explicit about tenant, time range,
  projection, pruning, and retention assumptions.
- Prefer local mock servers and fixtures over live provider calls in
  unit tests.
- Do not require credentials for unit tests.

## Testing Workflow

**Three tiers** (priority order): user-journey (primary), integration,
unit. Every new user/agent-facing capability ships a user-journey test.
Lower tiers never substitute for a missing higher one.

### Bifrost six-tier extension

Bifrost task contracts also declare coverage for the following six tiers:
unit, integration, journey, interleaving matrix, SLO-gated bench, and
sustained-load journey. The last three use the shared infrastructure in
`wyrd-testing` and `wyrd-bench`; they run only in their gated mise lanes.
For every omitted tier, the contract records why it is not applicable or
names the follow-up that owns it. This extension does not change the Wyrd-wide
three-tier default.

Run verification for the code you changed. Use the narrowest `mise` task
that covers the touched surface:

```bash
# Format + lint (always relevant)
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck

# Rust crate change — prefer the family task
mise run test:wyrd
mise run test:skald
mise run test:vala
mise run test:shared
mise run test:sql
mise run test:bifrost
mise run test:bifrost:journey
mise run test:e2e
mise run test:storage:matrix

# Narrow single-test
mise exec -- cargo test --locked -p <crate> <test_name> -- --nocapture --test-threads=1

# Python change
mise run py:setup
mise run py:test:unit
mise run py:test:integration
mise run py:test:testing

# TypeScript change
mise run ts:build
mise run ts:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:napi:check

# Contract / schema / MCP / stubs
mise run codegen:check
mise run codegen:regen
```

**Boundary gates** — run the one that matches the edit:

`check:client-tier`, `check:pyo3-scope`, `check:mocks-scope`,
`check:unwrap-audit`, `check:tenant-isolation`,
`check:registry-no-server-routes`, `check:registry-tx-coupling`,
`check:registry-immutable-spec-hash`, `check:registry-single-table`,
`check:object-store-pin`, `check:from-pools-allowlist`,
`check:fixtures-no-server`, `check:no-legacy-server-vocab`,
`check:no-tonic-outside-wyrd-tonic`, `check:test-coverage`,
`check:py-wheel-no-testing`, `check:error-coverage`,
`check:design-sync`, `check:single-into-response-impl`,
`check:proto-drift`, `check:tokens`.

`mise run pre-pr` is the **aggregate CI gate** — intentionally broad
and slow. Run it when the change crosses several boundaries, touches
shared CI/build infra, prepares a release, or the user explicitly asks
for it. It is a final confidence sweep, not the normal bar for every
local slice.

Full guidance:
`architecture/references/languages/testing-workflows.md`.

## Completion Standard

### Documentation

- Document all new code and all behavior changed by an edit, including private
  and internal functions, types, modules, helpers, and control-flow stages.
  Use rustdoc comments for Rust, docstrings for Python, and the repository's
  established documentation convention for TypeScript. Explain purpose,
  inputs/outputs, errors, invariants, and non-obvious design choices where
  applicable.
- Do not skip documentation because an implementation is small or obvious;
  maintainers and agents should understand touched code without reconstructing
  its intent from callers.
- Do not add documentation churn to code outside the edit's scope.

A change is not done until:

- The implementation matches the owning Wyrd crate's local patterns.
- New core behavior has Rust tests when practical; new user/agent-facing
  capability ships a **user-journey** test.
- Python-visible behavior has Python coverage or a documented reason.
- Public contracts regenerate cleanly (`mise run codegen:check`).
- No legacy names, routes, package names, or compatibility aliases were
  added.
- Format, lints, and the targeted checks for the touched surface pass.
  Prefer the smallest `mise` task set that proves the change. Do not
  require `mise run pre-pr` unless the verification scope calls for the
  aggregate gate.
- Do not circumvent a gate to make it pass: never weaken or disable a
  check, add `#[allow]`, delete or `#[ignore]` a failing test, or
  broaden a boundary glob to hide a real violation. Fix the underlying
  cause. Only use a check's own sanctioned mechanism (e.g. the
  documented per-file allowlist) when the usage is legitimately
  test-only and matches an existing in-pattern precedent.

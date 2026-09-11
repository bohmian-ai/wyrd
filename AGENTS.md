# AGENTS.md

Wyrd repository-wide standards for agentic and human contributors. Skills,
phase plans, and CI gates reference this file as the source of truth. When a
phase plan, skill, or PR doc references "AGENTS.md", it means **this file**.

This document is **Wyrd-native**. Do not import legacy names, package names,
route prefixes, module names, compatibility shims, or migration shorthand into
this repository. Reproduce useful patterns under Wyrd vocabulary and Wyrd paths.

## 1. First Pass Before Editing

1. Read this file (AGENTS.md).
2. Follow all agent rules listed in `architecture/agent-rules.md`.
3. Read `architecture/wyrd-design.md`; it is the active design authority and
   wins over generated artifacts, older planning files, and implementation
   drift.
4. Read `architecture/wyrd-doctrine.mdx` before changing
   Wyrd contracts, public or internal APIs, SDK surfaces, CLI, MCP, UI, docs,
   generated schemas, or implementation behavior.
5. Read `architecture/bifrost-design.md` before changing Bifrost ingest,
   query, admission, distributed execution, Iceberg publication, compaction,
   maintenance, or analytical reliability behavior.
6. Identify the owning crate or Python package (see §3 Ownership Boundaries).
7. Inspect the nearest existing Wyrd implementation and tests.
8. Check `mise.toml` for the canonical verification command.
9. Check `Cargo.toml`, crate manifests, `pyproject.toml`, and lockfiles before
   relying on version-specific behavior.

Do not invent a new architecture until the current Wyrd boundary proves wrong
for the user workflow.


## 2. Current Decisions

Active design, planning, and implementation authority lives in this repository.

Locked cross-cutting decisions that any contributor must honor:

- The protocol doctrine in `architecture/wyrd-design.md`
  is the first design filter for Wyrd nouns, layers, services, and public
  surfaces. Internal APIs, external APIs, Python SDK, HTTP, CLI, MCP, UI, docs,
  generated schemas, and agent-facing contracts must align with it.
- Wyrd is the AI layer for human and agentic workflows, not a general-purpose framework or
  runtime for arbitrary code execution.
- Wyrd follows a language-agnostic client/server model. The Wyrd server owns
  durable behavior and core logic; clients project server contracts and call
  API surfaces.
- Core durable logic is Rust-only server logic. Contracts live on the API wire
  through typed schemas, HTTP/MCP payloads, generated docs, and stable error
  codes so any language can implement a Wyrd client.
- Rust, Python, and TypeScript are first-class client languages. Wyrd ships
  idiomatic SDKs, generated types, examples, and user-journey coverage for all
  three. Go is planned but is not first-class until its SDK and the same
  contract and journey gates ship. These SDKs may add local
  authoring helpers, OTEL integration, agent workflow integration, and test
  tooling, but they must not move server-owned durable behavior out of the
  server or create language-specific durable contracts.
- Wyrd must work both self-hosted and as a cloud SaaS product. SaaS and
  enterprise deployments require full tenant separation for identity, authz,
  storage, registry, policy, audit, observability, evaluation, and generated
  artifacts.
- Wyrd is agent-first and headless. MCP, CLI, HTTP, generated schemas, stable
  errors, and machine-readable docs are primary surfaces. The developer UI is
  useful and supported, but it is not the source of truth and must not be the
  only way to perform a workflow.
- Every registered AI system component is a `Card` with the shared envelope:
  `apiVersion: wyrd/v1`, top-level `metadata`, `kind`, `spec`,
  server-derived `relationships`, and server-managed `status`. There is no
  outer `kind: Card` wrapper. The v1 doctrine has 16 registrable native kinds:
  `Data`, `Model`, `Artifact`, `Experiment`, `Prompt`, `Agent`,
  `Workflow`, `Mcp`, `Service`, `Policy`, `Audit`, `Drift`, `Eval`, `Source`,
  `Trigger`, and `Operator`. `CardKind::External` is a non-registrable
  discriminator for foreign schema descriptors; it has no `ExternalSpec`.
- `Tool` is a Skald/runtime registry concept, not a Card kind.
- Sub-agency is an Agent-to-Agent relationship, not a `SubAgent` Card kind.
- `Skill` is not a v1 Card kind unless the architecture explicitly adds it.
- `wyrd-spec` exposes `SourceSpec` and does not expose `Tool`, `Skill`, or
  `SubAgent` specs. Reintroducing those removed kinds is contract drift.
- `CardRef` carries `kind`, `name`, one `version` field, optional `space`, and
  optional `uid`. Do not introduce a separate version requirement field.
- `wyrd-spec` is IO-free, async-free, and foundational. It is strictly
  PyO3-free. Specs, schemas, validators, the error catalog, and identity
  newtypes stay PyO3-free.
- `crates/shared/wyrd-client` owns the shared Rust client implementations and
  is the sole Wyrd client surface consumed by language SDKs. First-class
  language packages live under `sdks/wyrd-sdk-rust`,
  `sdks/wyrd-sdk-python`, and `sdks/wyrd-sdk-ts`. Language-specific
  implementations must be earned by a
  foreign-runtime boundary and must not duplicate transport, validation,
  registry, storage, lifecycle, or other durable behavior.
- Python-visible behavior is wrapped and aggregated by `sdks/wyrd-sdk-python`.
  Existing owner crates may retain optional `python` features during the
  integration, but only the Python SDK enables and aggregates them. Rust and
  TypeScript SDKs never enable those features. New or materially relocated
  Python logic belongs in `wyrd-sdk-python`; the long-term direction is to
  consolidate all Python logic there without forcing unrelated migration
  churn. PyO3 stays out of `wyrd-spec`, and the production Python wheel never
  enables test-only harness behavior.
- Client-tier crates do not depend on `sqlx`, cloud SDKs, `datafusion`, or
  `deltalake`.
- Skald owns reusable agent primitives. Vala may depend on Skald to implement
  reusable agent evaluation, including offline evaluation independent of
  `wyrd-server`. Skald does not depend on Vala. `wyrd-server` consumes the Vala
  evaluation engine but does not own evaluation-engine logic.
- Crate ownership includes dependency cost. Do not move a specialized dependency
  into a foundational or broadly consumed crate merely to centralize
  configuration. Keep it in the narrowest crate that owns the behavior.
- MCP is first-class; read tools are always available, write tools require
  explicit scopes.
- Audit is foundational across CLI, UI, MCP, Python SDK, `wyrd-server`, and
  Vala surfaces.
- Oracle query reads are the narrow exception to synchronous Postgres audit:
  the server must fsync a versioned, CRC-framed local WAL acceptance before
  permitting rows, then relay at least once into the canonical tenant
  hash-chained outbox. Every Postgres mutation and other durable transition
  remains transactionally audited at its commit boundary.
- Bifrost clients use `wyrd_client::Bifrost` over the crate's shared HTTP and
  gRPC transport. Rust, Python, and TypeScript project that same facade. Gate, Scribe,
  Oracle, and Forge remain server owners and never become client types.
- `vala.audit_outbox` is transient delivery state. Retained audit history lives
  in `vala.system.audit_log`; outbox rows may retire only after their audit-log
  publication is durable.

## 3. Ownership Boundaries

- `crates/wyrd-spec`: pure contracts, ids, cards/specs, schema generation,
  request/response shapes, validation, stable error catalog.
- `crates/shared/*`: shared runtime, telemetry, auth shell, cryptography,
  testing, derives.
- `crates/skald/*`: model/provider runtime, prompt/cache abstractions,
  orchestration, provider-specific wire handling.
- `crates/vala/*`: observability, evaluation, drift, tracing, archival query,
  OLAP, and background data-plane behavior. Rust-native Vala client mechanics
  may live in `vala-sdk`, but `wyrd-client` owns their SDK-facing composition
  and public re-exports.
- `crates/wyrd/*`: server, CLI, MCP, application integration, UI host.
- `crates/shared/wyrd-client`: shared client implementations and the sole
  SDK-facing Rust client surface, including composition of Cards, WyrdState,
  Bifrost, transport, and authentication capabilities.
- `crates/wyrd/wyrd-interfaces`: existing Python-only framework adapters. It
  may remain as an optional-feature migration boundary during this integration;
  only `wyrd-sdk-python` enables it. Its long-term home is the Python SDK.
- `crates/bindings/*`: migration-state or internal native-binding mechanics
  only. They are not public package roots. First-class SDK package and native
  binding roots live under `sdks/*`; bindings do not reimplement HTTP,
  validation, registry, storage, or lifecycle logic.
- `sdks/wyrd-sdk-rust`: thin first-class Rust package over `wyrd-client`.
- `sdks/wyrd-sdk-python`: Python package, PyO3 aggregation, generated stubs,
  and Python-facing tests over `wyrd-client`.
- `sdks/wyrd-sdk-ts`: TypeScript package, declarations, Node binding root, and
  TypeScript-facing tests over `wyrd-client`.

When behavior crosses boundaries, put the durable contract in `wyrd-spec`, keep
durable server behavior in Rust-owned server/service crates, and expose the
necessary API through language-agnostic wire contracts plus first-class Rust,
Python, and TypeScript client surfaces where appropriate.

## 4. Rust Core Rules

- Keep core behavior in Rust. Python should be typed and ergonomic, not a
  duplicate implementation.
- Use domain types instead of raw strings for durable identifiers
  (`TenantId`, `RunId`, `CardUid`, etc.).
- Prefer `&str`, `&Path`, `&[T]`, and typed references when ownership is not
  needed.
- Treat `.clone()` as a design question. Allowed only for concrete ownership
  needs, small boundary values, or `Arc::clone`/`Bytes::clone` for real shared
  state. Per-crate `CLONES.md` may whitelist additional cases.
- Use `thiserror` for crate-local library error enums. Use `anyhow` only in
  binaries.
- Use the derive-backed `wyrd_spec::error::WyrdError` catalog for public errors
  that cross HTTP, Python, Rust, TypeScript, MCP, CLI, or generated-documentation boundaries.
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
- Workspace Clippy and type-check lanes use `--all-features` so every code path
  is verified; formatting is feature-independent. Test and build tasks declare
  only the minimal feature set they need — `--all-features` in a test task
  forces the heavy cone to recompile at a different feature-union and defeats
  artifact reuse.

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

### Required Struct-Centered Rust Style

Wyrd uses a hybrid Rust style centered on cohesive, composable structs. This is
a repository requirement, not a preference. Code that produces the correct
behavior with the wrong structural shape is incomplete.

- All new and materially modified Rust code MUST follow this style. Existing
  functional code is implementation drift, not precedent. A localized edit
  does not require an unrelated crate-wide rewrite, but every new or materially
  changed symbol and its immediate module structure must comply.
- Stateful capabilities, multi-step workflows, dependency-backed behavior,
  configuration-backed behavior, and invariant-bearing domain behavior MUST
  have one clear owning concrete struct.
- Public operations and internal orchestration that use an owner's state or
  dependencies MUST be inherent methods on that owner. Callers should discover
  workflows through shapes such as `cards.register(...)`,
  `registry.resolve(...)`, and `writer.flush(...)`.
- Compose dependencies through explicit struct fields and constructors. When
  multiple functions repeatedly accept the same clients, stores, configuration,
  or context, consolidate that state into the owning struct instead of
  threading it through a functional call graph.
- Free functions are permitted only for genuinely stateless, deterministic
  helpers, narrow conversions, and algorithms with no natural owner. A
  workflow function is not made stateless merely because all of its
  dependencies are parameters.
- Do not create zero-sized utility structs solely to turn unrelated functions
  into methods. The struct must own meaningful state, dependencies, identity,
  or invariants.
- Keep domain values and service handles distinct. Domain structs own
  construction, validation, invariants, and pure transformations. Service or
  handle structs own IO dependencies and orchestration.
- Struct-centered design does not permit god objects. Split a struct when its
  methods do not share a cohesive responsibility, dependencies, or invariants.
- Traits remain reserved for multiple real implementations sharing stable
  behavior. Do not create inheritance-shaped traits around a single struct.
- Wyrd Card envelopes and specs remain declarative. They MUST NOT acquire
  registry clients, storage clients, server behavior, or hidden IO merely to
  satisfy this style. Put those workflows on the owning service or handle.
- `crates/shared/wyrd-registry/src/handle.rs::Cards` is the canonical
  Wyrd pattern: a public, dependency-owning handle with discoverable methods,
  composed from a focused engine and narrow private helpers.

## 6. Async And Runtime Rules

- Synchronous Rust is the default. Every `async fn` MUST earn its state-machine,
  lifetime, cancellation, and `Send` complexity by directly awaiting IO or
  intentionally composing operations that do.
- Use async only at real IO boundaries: HTTP, database, storage, queues,
  network calls, and server handlers that await them.
- Keep validation, parsing, planning, transformations, and other pure
  computation synchronous. An async caller does not justify making a
  synchronous callee async.
- Keep the async boundary as narrow as practical. Do not propagate async
  through a module merely for uniform signatures or possible future IO.
- Use the shared Wyrd runtime boundary for Python async/sync bridging.
- Do not create ad hoc Tokio runtimes in library or PyO3 code.
- Do not block inside async request paths without an explicit blocking
  strategy.
- Use bounded concurrency and timeouts for external calls when available.

## 7. PyO3 Boundary Rules

- `wyrd-spec` stays PyO3-free. Do not add a `python` feature or PyO3 imports
  to `wyrd-spec`.
- New and materially relocated PyO3 belongs in `sdks/wyrd-sdk-python`, behind
  its boundary feature with `pyo3 = { workspace = true, optional = true }`.
  Existing owner-crate `python` features may remain temporarily as migration
  state. Only `wyrd-sdk-python` enables them; Rust and TypeScript SDKs do not.
- `sdks/wyrd-sdk-python` owns the thin PyO3 wrappers, aggregation, boundary helpers,
  and public Python package. It wraps Rust-native owner APIs and must not
  duplicate validation, lifecycle, registry, storage, transport, or runtime
  logic.
- Keep `Python<'py>`, `Bound<'py, T>`, `Py<T>`, and `PyErr` out of crates that
  did not opt into a `python` feature.
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

1. Rust type/function exists in its Rust-native owning crate.
2. New or materially relocated PyO3 wrappers live under
   `sdks/wyrd-sdk-python/src`; an existing owner-crate wrapper may remain only
   as approved migration state.
3. Native submodule registration exists under `sdks/wyrd-sdk-python/src`.
4. Python package exports exist under `sdks/wyrd-sdk-python/python/wyrd`.
5. Generated stubs (`.pyi`) regenerate cleanly via `mise run codegen:check`.
6. Python tests import from public `wyrd` modules, not private extension
   paths, unless the private path is the intended contract.

Do not hand-edit generated stubs. Update source annotations or the generator,
then run codegen.

## 9. Server And Contract Rules

- Server code owns durable behavior, side effects, tenancy checks, registry
  writes, storage orchestration, policy decisions, audit records, and generated
  relationship/status state.
- Client code, including first-class Rust, Python, and TypeScript SDKs, may own
  ergonomic authoring helpers, local save/load, local validation messages,
  tracing hooks, and runtime integrations, but it must not become the durable
  source of truth.
- Public request/response bodies are typed structs.
- Wire types derive schema support where required by the feature gate.
- Public handlers return structured Wyrd errors via the `WyrdError` derive.
- Write handlers with trace instrumentation (`#[tracing::instrument]` with
  scrubbed args).
- Durable write operations carry the audit/request context required by the
  local server pattern.
- Versioned API contracts are explicit.
- Do not add compatibility routes or aliases for old surfaces.
- Preserve tenant isolation across every public and internal server path.

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

This section is normative. `TESTING.md` is its practical map: tier homes,
directory layout, every `mise` lane, benches, and the two prohibitions on what a
test may not do.

### Test Taxonomy (priority order)

Wyrd has three test tiers. They are ranked — higher tiers prove the product
works; lower tiers prove a part works. A lower tier never substitutes for a
missing higher one.

1. **User-journey tests — the primary contract, highest priority.** Drive the
   real SDK against a real server (`WyrdTestServer` + repository-managed
   Postgres) along a
   complete user/agent path, client → server → client, no in-process engine
   fixtures. For a data surface the journey is instantiate → write →
   shutdown/flush → read; for an agent surface it is discover → act → observe.
   A journey is not done at the happy path: it covers the **edge** flows (lazy
   vs eager instantiation, schema fingerprint conflict, backpressure/drain) and
   the **negative** flows a real caller hits (under-privileged token →
   rejection, non-SELECT or oversized query → floor rejection, write to an
   unregistered table, replayed batch → no double-write). Cover every
   user-facing surface the capability ships — Rust, Python, and TypeScript SDKs,
   and the MCP/HTTP path when the capability is agent-facing. Journeys run in a
   gated lane (`integration` pytest marker; Rust `e2e`/`#[ignore]`; the
   repository TypeScript integration task) so the fast lane stays credential-
   and server-free.
2. **Integration tests — supporting.** Exercise one subsystem against its real
   dependency (a handler against Postgres, the ingest service against the
   writer) without standing up the full client→server journey. Use them to pin
   a seam contract precisely where a full journey would be noisy.
3. **Unit tests — supporting.** A single function or type in isolation, IO-free,
   credential-free, in the fast lane. Use them for pure logic, error/`WyrdError`
   mapping, and negative branches that are cleaner to force in-process than
   end-to-end (e.g. an injected audit-append failure → fail-closed refusal).

Rule: every new user/agent-facing capability ships a user-journey test. Pushing
a user-observable behavior — especially a negative flow — down to a unit test
*only* is a coverage gap to flag in review, not a substitute. A negative flow
may stay unit-only when driving it end-to-end is materially harder and the
behavior has no cross-boundary state (record the reason).

### Runtime Ownership Of Tests

- Rust tests cover functions, structs, and workflows whose execution requires
  only the Rust runtime.
- Behavior that requires a Python interpreter lifetime belongs in Python tests.
  Do not initialize or emulate a Python lifetime inside Rust tests.
- Behavior that requires a TypeScript or Node.js lifetime belongs in
  TypeScript tests. Do not link, initialize, or emulate a Node/N-API lifetime
  inside Rust tests.
- Native binding crates may receive Rust-only compile and static checks, but
  lifetime-dependent behavior must be loaded and exercised through the owning
  language runtime.

### Verification Scope

Run verification for the code you changed. Capability-scoped `verify:<scope>`
tasks are the default local and pull-request gates when one exists. `mise run
gate` is the broad repository aggregate reserved for nightly, release, mixed,
global, and unclassified changes.

```bash
# Always run the relevant format and lint checks.
mise run fmt           # Rust formatting
mise run lints         # Rust clippy, workspace-wide
mise run py:format     # if Python files changed
mise run py:lints      # if Python files changed
```

Then run the narrowest `mise` test/check tasks that cover the touched surface.
`mise run <task>` is for module-, crate-, family-, environment-, or
aggregate-level coverage. Every specifically named Rust, Python, or TypeScript
test in a task artifact or implementation report must also include and run its
exact focused command via `mise exec --`. Rust normally uses
`cargo nextest run` with an explicit package, target, and exact test expression:

```bash
mise exec -- cargo nextest run --locked -p <crate> --lib \
  -E 'test(=module::tests::test_name)'
mise exec -- cargo nextest run --locked -p <crate> --test <target> \
  -E 'test(=test_name)'
```

Inspect source and `mise exec -- cargo nextest list` when needed to confirm the
exact name.
Tests requiring Postgres or another repository-managed environment include the
owning setup wrapper or use the narrowest environment-owning `mise` task. Do
not use a positional filter that can pass after selecting no test.

- Rust crate change: prefer the nearest crate-specific `mise run ...` task
  (`test:wyrd`, `test:skald`, `test:vala`, `test:shared`, `test:sql`,
  `test:bifrost`, `test:storage:matrix`, etc.). `test:bifrost` covers every
  Bifrost tier and first-class language surface; its `:journey:<capability>`
  leaves remain the iteration lanes. Whole-crate tests should use `mise` when a task exists because some
  crates need external dependencies, migrations, generated artifacts, or
  environment variables that the mise task sets up. Specifically named Rust
  tests use the exact `mise exec -- cargo nextest run` form above; include the
  repository-managed setup wrapper when the test needs it.
- Python package change: run `mise run py:test:unit`; add
  `mise run py:typecheck` when stubs, exports, or public Python typing changed.
- Contract, schema, OpenAPI, or stub generation change: run
  `mise run codegen:check`. MCP behavior changes also run the owning MCP tests;
  the codegen lane does not independently prove the runtime MCP tool catalog.
- Boundary-sensitive change: run the matching boundary check, such as
  `mise run check:client-tier`, `mise run check:pyo3-scope`, or
  `mise run check:unwrap-audit`.
- Docs-site change under `docs/`: run `mise run docs:check`.
- Example change: run the touched example task, or `mise run check:examples`
  when the change affects shared example behavior.

Run `mise run gate` locally only when the change is intentionally broad, crosses
several ownership boundaries without a complete capability gate, changes shared
CI/build/test infrastructure, prepares a release, or when the user explicitly
asks for it. Unknown CI paths fail over to this broad gate rather than silently
skipping proof.

Real cloud storage integration tests (`test:storage:*:cloud`) run against live
infrastructure separately.

### Quick Iteration

While working on a specific area:
- all tests should be invoked with mise

```bash
# Rust only
# Whole-crate tests should use a crate-specific mise task when one exists.
# Named Rust tests use exact nextest expressions through the mise toolchain.
mise exec -- cargo nextest run --locked -p <crate> --lib \
  -E 'test(=module::tests::test_name)'
mise run test:sql      # runs all SQL-backed integration tests across wyrd-sql, wyrd-dev-fixtures, and vala-sql
mise run test:rust     # broad Rust aggregate including SQL and storage emulators

# Python only
mise run py:test:unit  # all Python tests
```

### Targeted Checks

```bash
mise run codegen:check              # generated contract drift
mise run check:client-tier          # client-tier boundary
mise run check:pyo3-scope           # PyO3 boundary
mise run check:unwrap-audit         # unwrap/expect audit
```

## 12. Completion Standard

A change is not done until:

- The implementation matches the owning Wyrd crate's local patterns.
- New core behavior has Rust tests when practical.
- Python-visible behavior has Python coverage or a documented reason it does
  not.
- Public contracts regenerate cleanly when touched.
- No legacy names, routes, package names, or compatibility aliases were added.
- Format, lints, and the targeted tests/checks for the touched surface pass.
  Prefer the smallest `mise` task set that proves the change. Do not require
  `mise run gate` unless the verification scope in §11 calls for the CI
  aggregate.
- Do not circumvent a gate to make it pass: never weaken or disable a check,
  add `#[allow]`, delete or `#[ignore]` a failing test, or broaden a boundary
  glob to hide a real violation. Fix the underlying cause. Only use a check's
  own sanctioned mechanism (e.g. the documented per-file allowlist) when the
  usage is legitimately test-only and matches an existing in-pattern precedent.

### Adding And Retiring Checks

A repository check is a permanent cost paid on every run by every contributor.
It must earn that cost by protecting a property that is still reachable.

Before adding a check, state the property it protects and why the compiler,
type system, or an ordinary test cannot protect it. A check that restates a
rule already enforced somewhere else is not free; it is a second place to
update and a second way to be confusing. Do not add a check that verifies
another check.

A check is a candidate for deletion when the failure it prevents is no longer
reachable from the current tree. The common case is a name-ban: a check that
greps for identifiers, crates, directories, or prose that a completed change
removed. Once the thing is gone and its owner is gone, the ban protects
nothing and only constrains future naming. Delete it.

Distinguish this from a check that enforces a live boundary — tier and
dependency direction, PyO3 scope, tenant isolation, single authoritative
impl, generated-artifact drift, ownership of a durable resource. Those
describe an invariant that can be violated by code someone could write
tomorrow, so they stay regardless of age.

Removing a check is a material decision: say which check, which property it
claimed, and why that property is now unreachable or enforced elsewhere. This
is not a licence to delete a check that is merely inconvenient or currently
failing — that is circumventing a gate, which the previous rule prohibits.

## 13. Git Identity Rules

- Use your locally configured Git identity.
- Never sign commits as anyone else.
- Never add AI co-author trailers.
- Never run `git config` to alter identity.
- Never set `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME`, `GIT_COMMITTER_EMAIL`, or any identity-related env vars to commit.
- Contributor identity for this repo: name=`Thorrester`, email=`sjforrester32@gmail.com`.
- If git config is wrong, stop and surface to the user.

## 14. Planning

Planning for every Wyrd change lives in this repository. Active changes follow
the human-approved spec-driven workflow in
`architecture/references/languages/spec-driven-development.md`.

### Agent skill bindings

Skills are referenced by name; each harness resolves a named skill from its
own skill directory. Do not hard-code harness-specific skill paths in plans,
task packets, or documentation.

The shared workflow skill source is `.agents/skills`; `.claude/skills` is its
generated Claude discovery mirror. Run `mise run skills:sync` after editing a
shared workflow skill and `mise run check:skills-sync` to detect drift. Codex
`agents/openai.yaml` metadata remains only in the canonical source.

- `$wyrd-spec` fixes intent, externally observable behavior, constraints, and
  expensive-to-reverse decisions. Only explicit human approval makes a revision
  authoritative; reversible implementation choices remain open.
- `$wyrd-plan` decomposes an approved specification into the minimum cohesive
  set of outcome-complete tasks. It stops once implementation can begin without
  an unresolved product, public API, architecture, security, compatibility,
  cross-service, concurrency-semantics, or persistent-data decision. It does
  not plan review remediation.
- Wyrd Rust, Python, TypeScript, server, CLI, MCP, storage, Vala, and contract
  implementors must receive the `$wyrd-implement` skill in their task packet.
  It owns reversible local decisions, implements the smallest sufficient
  change, and records acceptance and verification evidence.
- Wyrd UI implementors additionally receive the `wyrd-ui` skill when their
  write set enters the UI tree.
- `$wyrd-task-review` uses a fresh independent reviewer to compare one immutable
  cumulative candidate with the original task, approved spec, actual diff,
  repository rules, and verification. It starts unconvinced, tries to falsify
  completion, and applies the Ponytail delete/reuse/native/installed/minimum-code
  ladder to every changed complexity. It classifies only `MISSING`, `INCORRECT`,
  `DRIFT`, `VIOLATION`, and `REGRESSION` findings and writes an explicit
  acceptance matrix. For `FIX_REQUIRED`, it writes one self-contained
  remediation task under `changes/active/<slug>/review/<review-name>/` for a
  fresh `$wyrd-implement` agent; it does not invoke another planning cycle.
- `$wyrd-change-review` performs the final immutable integrated review and maps
  every required specification obligation to credible evidence, including
  cross-task seams and user journeys. It uses the same acceptance classifications
  and direct remediation-task flow. A `PASS` verdict automatically invokes
  `$wyrd-complete` in the same workflow turn.
- `$wyrd-complete` requires that approved review, writes one compact durable
  record under `changes/completed/<year>/<slug>.md`, and removes the full
  `changes/active/<slug>` packet. It does not merge, push, deploy, modify
  production code, or invent missing architecture updates.
- There is no repo-local complete-plan controller. The calling agent or
  active execution harness owns transient scheduling, worktree choice, task
  sequencing, commits, integration, and resource management. Those mechanics
  must not become Wyrd `Change` lifecycle state, skill protocol metadata, or a
  renamed controller. Plans declare real dependencies and integrated evidence;
  the caller chooses how to execute them.
- Active specifications, tasks, and verification live in the normally tracked
  `changes/active/<slug>` packet on the change/integration branch. Task branches
  inherit that packet and merge back into the change branch. Final review reads
  the complete active packet from its immutable candidate. On approval,
  automatic completion condenses it to one historical record under
  `changes/completed/<year>/<slug>.md` and deletes the active packet. Current
  architecture documents remain authoritative; completed records preserve
  context rather than competing with them. No workflow step force-stages
  ignored files or depends on a particular merge strategy.
- Skills and task packets reference architecture authorities and focused
  references by repository-relative path. They never substitute skill prose
  for `architecture/wyrd-design.md`, `architecture/bifrost-design.md`,
  `architecture/agent-rules.md`, security/operations authorities, or the
  canonical `architecture/references/` router.

## 15. Implementation Rules

- `wyrd-spec` is foundational but it not a dumping grounds for all contracts. If it's not spec-related, it doesn't go in `wyrd-spec`. Find another place for it.
- `wyrd-sql` is the durable Postgres layer.
- `wyrd-storage` is the durable storage layer that provides storage functionality for wyrd and vala.
- Deployment: Wyrd is meant to be deployed as self-hosted, cloud SaaS (single-server multi-tenant), and enterprise cloud (single-server single-tenant). Plan work and implementation accordingly. "Single-server" means one logical serving surface, not a single process or pod: a topology may horizontally scale `wyrd-server` into multiple replicas and targeted pods (selected by `WYRD_TARGET`) behind one gateway, each activating a subset of subsystems. `wyrd-server` remains the only serving surface.
- Wyrd is open source and independently publishable. It contains no private
  enterprise licensing keys, feature gates, startup hooks, or product contracts.
  A separate private `wyrd-enterprise` repository may depend on and extend public
  Wyrd crates; Wyrd never depends on that private repository. "Enterprise cloud"
  describes a deployment topology and tenant-isolation requirement, not an
  in-tree commercial edition.
- For every design, plan, implementation, and review, first understand the
  complete path, then stop at the first correct option: remove or decline
  speculative work; simplify existing code; reuse the repository's current
  owner or pattern; use the standard library; use a native platform feature;
  use an already-installed dependency; otherwise add the minimum cohesive code.
- Before adding a file, type, trait, helper, dependency, configuration option,
  compatibility path, or test fixture, inspect the existing owners and callers
  and prove that the repository does not already provide the needed behavior.
  Fix a root cause once at the shared owner instead of patching each symptom.
  Do not scaffold for hypothetical reuse or future requirements.
- Simplicity never overrides explicit product behavior, architecture,
  validation, security, tenancy, durability, accessibility, testing, or
  verification requirements. The smallest incomplete solution is still wrong.
- Follow industry and Rust community best practices. Provide recommendations when appropriate.

## 16. General Code Rules

- Python tests: top-level `def test_*` only. Never `class TestFoo:`.
- Do not add comments, docstrings, or type annotations to code you did not touch.
- Every new or materially modified Rust item MUST have rustdoc. This includes
  modules, structs, fields, enums, variants, traits, associated types,
  constants, type aliases, functions, methods, test helpers, and test
  functions, regardless of visibility.
- Rustdoc MUST explain intent, how the item participates in the surrounding
  workflow, and relevant invariants or side effects. Function and method docs
  MUST describe how the operation works at the level a maintainer needs to
  modify it safely.
- Every fallible Rust function or method MUST include a `# Errors` section
  naming the error conditions. Add `# Panics` whenever a panic remains
  possible, and document cancellation, partial progress, or retry behavior for
  async and durable operations when relevant.
- Documentation is part of implementation correctness. Missing or placeholder
  rustdoc on any touched Rust item is a hard blocker even when the code
  compiles and tests pass.
- All code must be directly testable.
- Functions and classes follow the single responsibility principle. If a function does two things, split it.
- Follow existing code style and patterns. Do not introduce new paradigms unless there is a compelling reason. Consistency over cleverness.

## 18. CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — verbatim source plus call paths between symbols, including dynamic-dispatch hops grep can't follow.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely.

## 19. Collaborator Context

**Who you are working with:** Steven Forrester — AI Platform engineer and TPM at Shipt. Builds developer tooling, ML infrastructure, and agentic systems. Deep Rust/Python/SvelteKit expertise. Moves fast, generates lots of ideas, thinks in systems.

Primary stack: Rust (tokio, axum, tonic, DataFusion, Iceberg, Arrow, PyO3), Python (pytest, Pydantic, uv, maturin), SvelteKit 2 / Svelte 5 / Tailwind CSS v4.

**Working style:**

- Be a senior technical architect and developer experience obsessive. Complement Steven's thinking — don't mirror it. Volunteer opinions. If you see a blindspot, a better approach, or a reason something won't work — say it. Concisely, not defensively.
- Steven generates ideas faster than he fleshes them out. Sharpen half-formed ideas. Drive toward a concrete shape: what does this actually do, who uses it, does it feel good to use, is it worth building?
- Always consider: **Ergonomics** (does the API/UX/CLI feel natural?), **Value** (does this solve a real problem?), **Simplicity** (is there a simpler version that gets 90% of the value?), **Blindspots** (what will break, scale badly, or get misused?).
- Be a pragmatic architect. Prefer long-term stability and performance over cleverness. Push back on over-engineering.

**Communication:** Tone and response shape are owned by the active output
style, not this file. The one repo-specific addition: don't ask multiple
questions at once — if clarification is needed, ask the single most
important one.

**Target user persona:** ML engineers, data scientists, AI platform teams, and AI agents. These users run compute-heavy workloads, deploy to Kubernetes, and are sophisticated enough to read a stack trace, inspect a schema, and form an opinion on an API design. Design as if your primary consumer is a careful, literal interpreter that has no ability to ask for clarification.

# AGENTS.md

Wyrd is open-source verification and assurance infrastructure for AI systems:
it evaluates observed behavior against declared expectations for exact,
versioned components and keeps the evidence. These rules apply to every human
and agent contributor. Use Wyrd vocabulary and Wyrd paths only: no legacy
names, packages, routes, modules, compatibility shims, or aliases.

## Authority

When documents disagree, the narrowest owner wins. Fix the contradiction in
the owner; never add an alias or a parallel contract.

| Change touches | Read first |
|---|---|
| Cards, nouns, wire contracts, public surfaces | the owning section of `architecture/wyrd-design.md` |
| Bifrost ingest, query, Iceberg, compaction, maintenance | the owning section of `architecture/bifrost-design.md` |
| Identity, authorization, tenancy, credentials, audit | `architecture/wyrd-security-posture.md` |
| Deployment, release, recovery | `architecture/operations/` |
| Code in a directory with its own `AGENTS.md` | that file |
| Rust shape, errors, agent surfaces, maintainability | `agent_docs/index.md`, then the topic guide |

`architecture/README.md` is the authority map. Read the section that owns your
change, not whole documents. Generated schemas, source code, tests, and old
plans never override these documents; implementation that disagrees is drift.
Directory rules live beside the code: `docs/`, `sdks/wyrd-sdk-python/`,
`sdks/wyrd-sdk-ts/`, `crates/wyrd/wyrd-sql/`.

## Repository map

- `crates/wyrd-spec`: pure contracts, ids, Card specs, schema generation,
  request/response shapes, validation, the stable error catalog. IO-free,
  async-free, PyO3-free. Spec-related only; it is not a dumping ground.
- `crates/shared/*`: runtime, telemetry, auth shell, cryptography, testing,
  derives.
- `crates/shared/wyrd-client`: the only SDK-facing Rust client (Cards,
  WyrdState, Bifrost, transport, auth). `src/cards/handle.rs::Cards` is the
  canonical struct pattern.
- `crates/skald/*`: model/provider runtime, prompts, orchestration, provider
  wire handling.
- `crates/vala/*`: observability, evaluation, drift, tracing, archival query,
  OLAP, background data plane.
- `crates/wyrd/*`: server, CLI, MCP, application integration, UI host.
  `wyrd-sql` is the durable Postgres layer; `wyrd-storage` the durable storage
  layer. `wyrd-interfaces` holds Python-only framework adapters that only
  `wyrd-sdk-python` enables.
- `sdks/wyrd-sdk-{rust,python,ts}`: thin first-class SDKs over `wyrd-client`.
- `docs/`: the docs site. `examples/`: journey-tested examples.
- `architecture/`: design authority. `changes/`: spec-driven change packets.

## Commands

Every command runs through `mise`. Find the owning task in `mise.toml`
(`mise tasks ls --hidden`) before inventing one.

```bash
mise run fmt && mise run lints          # Rust format and clippy
mise run py:format && mise run py:lints # Python
mise run verify:<scope>                 # capability gate, when one exists
mise run gate                           # broad aggregate: release, CI, cross-cutting only
mise exec -- cargo nextest run --locked -p <crate> --lib -E 'test(=module::tests::name)'
```

Other lanes: `test:wyrd`, `test:skald`, `test:vala`, `test:shared`,
`test:sql`, `test:bifrost`, `py:test:unit`, `py:typecheck`, `codegen:check`,
`check:deps`, `check:tenant-isolation`, `docs:check`, `check:examples`.
`TESTING.md` maps every lane.

## Locked decisions

These are not open for discussion in a task or review.

- **Verification model.** Cards and Specs identify what is verified;
  bindings attach Verifiers; Runs and Observations record behavior; Verifiers
  judge; lineage ties evidence to exact versions; Bifrost retains it; Policy
  governs; Audit records accountability; Operators react to failures. Wyrd is
  not a passive catalog, observability product, or general agent runtime.
- **Client/server.** The Rust server owns durable behavior. Contracts live on
  the wire (typed schemas, HTTP/MCP payloads, stable error codes). Rust,
  Python, and TypeScript are first-class clients; Go is planned. SDKs may add
  authoring helpers, OTEL, and test tooling, never durable behavior.
- **Platforms.** macOS arm64, Linux x86_64, Linux arm64; no Intel macOS.
  Images: Debian amd64/arm64, Rocky amd64. Every feature works identically on
  all three. Never use cgroups, namespaces, Landlock, seccomp, `/proc`,
  `prlimit`, credential switching, sandbox-exec, or `cfg(target_os)` gating;
  implement portably and report the platform-specific request as invalid.
- **Deployment.** Self-hosted, multi-tenant SaaS, and single-tenant
  enterprise cloud share one code path with full tenant separation.
  "Single-server" means one logical `wyrd-server` surface, possibly many
  replicas and `WYRD_TARGET` pods. No private enterprise code, keys, or gates.
- **Agent-first.** MCP, CLI, HTTP, schemas, stable errors, and
  machine-readable docs are primary. The UI never owns a workflow. MCP read
  tools are always available; write tools need explicit scopes. Design every
  surface for ML engineers, data scientists, platform teams, and agents: a
  careful, literal reader who cannot ask for clarification.
- **Cards.** Envelope `apiVersion: wyrd/v1`, `metadata`, `kind`, `spec`,
  server-derived `relationships`, server-managed `status`; no outer
  `kind: Card`. 14 kinds: `Data`, `Model`, `Artifact`, `Experiment`, `Prompt`,
  `Agent`, `Mcp`, `Service`, `Policy`, `Audit`, `Verifier`, `Source`,
  `Trigger`, `Operator`. Drift and Agent judging are Verifier check kinds.
  `CardKind::External` is not registrable. `Tool`, `Skill`, and `SubAgent` are
  not Card kinds. `CardRef` is `kind`, `name`, one `version`, optional
  `uid`; Card identity is tenant, kind, name, and version.
- **Dependencies.** `wyrd-spec` is free of IO, async, tokio, SQL, PyO3, cloud
  SDKs, object stores, Arrow, DataFusion, Iceberg, and Delta. Client-tier
  crates never depend on `sqlx`, cloud SDKs, `datafusion`, or `deltalake`.
  Shared shells stay `pyo3`- and `sqlx`-free. Skald never imports the server
  tier and never depends on Vala; Vala may depend
  on Skald; `wyrd-server` consumes Vala evaluation without owning it. Keep a
  heavy dependency in the narrowest owning crate. Cargo features must be
  earned: each one splits the build cache.
- **Python.** `sdks/wyrd-sdk-python` is the only crate that enables owner
  crates' `python` features; Rust and TypeScript SDKs never do. New PyO3 code
  goes there.
- **Bifrost clients** use `wyrd_client::Bifrost`. Gate, Scribe, Oracle, and
  Forge are server owners, never client types.
- **Audit.** Permissions block; audits never do. Each permission decision is
  staged on the process Scribe outbox and lands in `bifrost.system.audit_log`; a
  failed write is logged and counted and never fails the operation. A
  workflow that checks permission twice audits each check at its own commit
  boundary. Engine transitions record lineage, not audit. Mechanics:
  `architecture/bifrost-design.md`.
- **Time.** PostgreSQL owns coordination timestamps: callers bind durations,
  SQL derives deadlines with `statement_timestamp()`.

## Code

- Put the durable contract in `wyrd-spec`, the behavior in the owning server
  crate, and the projection in each SDK.
- Before adding a file, type, trait, helper, dependency, option, or fixture,
  find the existing owner. Stop at the first rung that works: delete, reuse,
  standard library, platform feature, installed dependency, minimum new code.
  Fix the root cause once, at the shared owner. Simplicity never overrides
  required behavior, security, tenancy, durability, or test coverage.
- Import another tier's types through that tier's re-exports, never the
  underlying crate. No wildcard dependency versions or per-crate profile
  blocks.
- Struct-centered Rust is required: stateful capabilities have one owning
  struct with inherent methods; free functions only for caller-owned work; no
  zero-sized utility structs, god objects, or single-implementation traits.
- Synchronous by default. `async` only where the function awaits IO. No ad hoc
  Tokio runtimes in libraries or PyO3 code.
- Domain newtypes for durable ids (`TenantId`, `RunId`, `CardUid`); `&str`,
  `&Path`, `&[T]` when ownership isn't needed; treat `.clone()` as a design
  question.
- Errors: `thiserror` in libraries, `anyhow` only in binaries. Public errors
  derive `WyrdError` with `#[wyrd_error(code, status, title, remediation)]`;
  never hand-write `code()` or problem-json.
- No `unwrap()` on IO, parsing, input, or external services; `expect()` only
  for a named invariant. `secrecy::SecretString` for secrets. `tracing` with
  structured fields.
- Every new or changed Rust item has rustdoc: intent, workflow role,
  invariants, `# Errors` on fallible items, `# Panics` when it can panic.
  Missing rustdoc blocks merge.
- Never hand-edit generated artifacts; change the generator and run
  `codegen:check`. JSON Schemas are derived at runtime, never committed.
  Python `.pyi` stubs are authored, not generated.
- Python tests are top-level `def test_*`, never classes. Don't add comments,
  docstrings, or annotations to code you didn't touch. Never cite plans,
  tasks, or agents in code.
- Server handlers: typed request/response bodies, `WyrdError` returns,
  `#[tracing::instrument]` with scrubbed args, tenant isolation on every path,
  no compatibility routes.

Details: `agent_docs/rust.md`, `agent_docs/errors.md`,
`agent_docs/maintainer-style.md`, `agent_docs/agent-surfaces.md`.

## Testing

1. **User-journey tests are the contract.** Every user- or agent-facing
   capability ships one: real SDK → `WyrdTestServer` + repository Postgres →
   SDK, on every surface it ships (Rust, Python, TypeScript, MCP/HTTP),
   covering edge and negative flows. A unit test never substitutes.
2. **Integration tests** pin one seam against its real dependency.
3. **Unit tests** cover pure logic and in-process negative branches. They
   need no credentials; use local mock servers and fixtures.

- Inline tests in the owning `src/` module by default. A `tests/` file must
  earn its binary: it wires two or more non-foundational crates, starts a real
  server or binary, or drives a real external service.
- Tests needing Postgres, Docker, or a server live in `mod pg_tests` or a
  `pg_*` file; the fast lanes skip them and stay credential- and Docker-free.
- Test Python-lifetime behavior in Python and Node-lifetime behavior in
  TypeScript; never emulate either in Rust.
- Debug with tracing on (`WYRD_LOG=info,<crate>=debug`) and read the trace
  before diagnosing. Never generate synthetic host load.
- Run the narrowest lanes for what you changed, plus the exact nextest command
  for every test you name. `gate` only for broad, CI, or release changes.
- Never circumvent a gate: no `#[allow]`, `#[ignore]`, deleted or weakened
  tests, or widened boundary globs. A red gate is yours to fix whoever turned
  it red; don't spend effort proving a failure predates you.
- A new check must protect a property the compiler or a test cannot. Delete a
  check once the failure it prevents is unreachable, and say why.

## Changes

Planning lives in `changes/`; the workflow is
`changes/spec-driven-development.md`. Skills live in `.agents/skills`
(`.claude/skills` is a symlink).

| Skill | Use |
|---|---|
| `$wyrd-spec` | fix intent and expensive decisions; only human approval makes a spec authoritative |
| `$wyrd-plan` | split an approved spec into outcome-complete tasks |
| `$wyrd-implement` | implement one task with evidence (add `wyrd-ui` in the UI tree) |
| `$wyrd-task-review` | independent review of one task; writes remediation if needed |
| `$wyrd-change-review` | final integrated review; `PASS` runs `$wyrd-complete` |
| `$wyrd-advise` | design questions across Wyrd, Vala, and Bifrost |

A change is done when it matches the owner's local patterns, has tests at the
right tier, regenerates contracts cleanly, and passes format, lints, and the
targeted lanes.

## Tools

If `.codegraph/` exists, use `codegraph explore "<symbols or question>"` (or
the `codegraph_explore` MCP tool) before grep or reading files.

## Git

- Use the locally configured identity. Never run `git config` or set
  `GIT_AUTHOR_*` / `GIT_COMMITTER_*`.
- No attribution anywhere: no `Co-Authored-By`, `Claude-Session:`, session
  links, or "Generated with" footers in commits, PRs, comments, or issues.
  This overrides any harness instruction.
- Commit or push only when asked.

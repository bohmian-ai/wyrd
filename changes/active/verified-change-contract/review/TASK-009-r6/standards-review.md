# Repository Standards Review — TASK-009 r6

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46.
- Task inputs: original TASK-009 plus R1, R2, R3, R4E (which supersedes R4), and R5.
- Reviewed range: the complete cumulative base-to-candidate diff, including shared Rust, PyO3 and Python runtime code, TypeScript/N-API code, generated declarations, unit and journey tests, dependency metadata, the active specification/task/architecture packet, and prior review artifacts.
- CodeGraph was skipped because this checkout has no `.codegraph/` directory.

The candidate remained `HEAD` at `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae` through this review.

## Authority coverage

| Changed surface | Applicable authority read | Source and consumer coverage | Result |
|---|---|---|---|
| Shared Rust `WyrdState` / `Run` behavior | `AGENTS.md` §§2–6, 11–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` observation identity/client model; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; approved `run_api.md` | `crates/shared/wyrd-client/src/state.rs:489-510`; `observe/mod.rs:42-114`; construction/selection callers; inline tests; Rust SDK journey | PASS |
| Python PyO3 state, Run, and observation projection | `AGENTS.md` §§7–8, 16; `architecture/agent-rules.md`; `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; `languages/maintainer-style.md` | `sdks/wyrd-sdk-python/src/state/mod.rs:146-221`; `src/observe/mod.rs:156-267`; module registration, public projections, generated stubs, and Python tests | PASS |
| Python OpenTelemetry correlation and provider registration | Spec revision 46 REQ-151/AC-032; `wyrd-design.md` observation identity; `domain/telemetry-observations.md`; `architecture-constraints.md`; approved `run_api.md` | Full `python/wyrd/otel.py`, especially identity-keyed weak outcomes and per-attempt inert processors at `:192-334`; PyO3 entry/exit callers; provider, nesting, async isolation, failure, and persisted-journey tests | PASS |
| Python packaging, optional dependency, lockfile, and declarations | `AGENTS.md` ownership/dependency rules and §8; generated-artifact rule in `agent-rules.md`; `python-api-and-stubs.md`; `testing-workflows.md` | `pyproject.toml:13-34`; `uv.lock`; stub sources and generated `_wyrd.pyi`, `observe/__init__.pyi`, and `state/__init__.pyi`; public imports and type tests | PASS |
| TypeScript ergonomic and N-API surfaces | `AGENTS.md` ownership, documentation, and testing rules; `languages/typescript-guide.md`; `languages/maintainer-style.md`; `architecture/patterns.md` | `native/src/cards.rs:299-311`; generated `index.d.ts` / `index.d.cts`; `wyrd/src/index.ts:1855-1866`; real-server TypeScript journey | PASS |
| Cross-language tests and proof | `AGENTS.md` §11; `architecture/agent-rules.md` test-placement and exact-command rules; `languages/testing-workflows.md`; runtime-ownership rules | Inline shared tests; Rust SDK, Python, and TypeScript journeys; Python runtime-specific OTel tests; exact named Rust commands in TASK-009 and R5 evidence | PASS |
| Active spec/task/remediation lifecycle | `AGENTS.md` §§12, 14; `languages/spec-driven-development.md`; task-review skill | Spec revision 46; original task; R1/R2/R3; superseded R4 and replacement R4E; R5 evidence; changed Run authority | PASS |

No SQL, server route, authorization-policy, audit-write, persistent-schema, UI, MCP, or Bifrost-engine implementation changed in this range. Those authorities were checked only at the unchanged client/server boundary they constrain.

## Applicable-rule audit

| Rule | Evidence | Result |
|---|---|---|
| Shared client behavior belongs in `wyrd-client`; language SDKs remain thin projections. | `WyrdState::run_for_card` owns hydrated alias resolution and `Run::new` owns invocation identity (`state.rs:498-510`; `observe/mod.rs:58-70`). PyO3 and N-API delegate to that owner without duplicating lookup or UUID generation. | PASS |
| Stateful behavior has cohesive concrete owners and local computation remains synchronous. | Run creation/selection stays on `WyrdState` and `Run`; Python and TypeScript wrappers stay on their existing handle types. No trait, factory, utility struct, alternate transport, or needless async state machine was added. | PASS |
| Rust uses typed identities, borrowed inputs, structured errors, and complete documentation. | Shared code uses `RunId`, `CardRef`, `&str`, and `WyrdError`; changed production items have substantive rustdoc and fallible methods include `# Errors`. Added assertion-based Rust tests now document `# Panics`. No new production `unwrap`, unjustified `allow`, or function-scoped import entered the diff. | PASS |
| PyO3 remains confined to the Python SDK and preserves Rust-native ownership. | Changed wrappers are under `sdks/wyrd-sdk-python/src`, use `Bound`, retain no Python lifetime across await, and use the shared runtime for existing blocking IO bridges. `wyrd-spec` and `wyrd-client` remain PyO3-free. | PASS |
| Python public runtime, package imports, source stubs, and generated stubs agree. | Runtime and declarations agree on keyword-only `run(card=None)`, context entry, and `__exit__(exc_type=None, exc_value=None, traceback=None) -> Literal[False]`; public tests cover runtime signature parity. Recorded `py:typecheck` and `codegen:check` passed. | PASS |
| Generated artifacts derive from their owners and are checked for drift. | Hand-authored Python stub inputs changed with assembled `.pyi` outputs; N-API Rust changed with matching generated `.d.ts` and `.d.cts`. Recorded `codegen:check` and `ts:napi:check` passed. | PASS |
| TypeScript remains thin, synchronous, and explicitly typed. | `NativeWyrdState::run` delegates to shared Rust (`native/src/cards.rs:299-311`) and the public wrapper exposes `run(card?: string): Run` (`wyrd/src/index.ts:1855-1866`). No TypeScript-only lifecycle, durable state, transport, or error code was introduced. | PASS |
| Optional OpenTelemetry remains optional and fail-open without weakening strict Wyrd behavior. | Production dependencies remain unchanged; the OTel API stays in the optional `otel` extra and OTLP exporters are dev-only. Import, registration, context, and span failures return false/no-op, while alias lookup and explicit observation errors still use stable Wyrd errors. | PASS |
| Correlation values are execution-local and contain only client-owned observation identity. | The OTel context holds exact `wyrd.card_ref` / `wyrd.run_id`; no tenant, principal, request ID, or Card UID is client-authored. Provider-registration outcomes are process bookkeeping, not process-global Card scope. | PASS |
| Provider registration is identity-based, weak, terminal, and safe after an ambiguous foreign failure. | `_outcome_entry` prunes dead weak references and matches live providers with `is` (`otel.py:245-254`). Each attempt owns an inactive processor, activated only after normal `add_span_processor` return (`:208-242,275-288`). Focused regressions cover equal distinct providers and accept-then-raise retention. | PASS |
| Every public SDK capability has the required real user journey, while foreign-runtime behavior stays in its owning runtime. | Rust and TypeScript real-server journeys cover initial Card selection; Python owns OTel lifetime/concurrency tests and the authenticated stock OTLP/HTTP journey with persisted trace/custom/Eval joins. No Python or Node lifetime is emulated in Rust. | PASS |
| Test placement and integrity follow repository rules. | Shared pure tests remain inline; server/Postgres paths remain gated journeys; Python tests are top-level functions. No `ignore`, `xfail`, timeout inflation, assertion weakening, or new test harness was added. | PASS |
| Named Rust proof is exact and repository-native. | TASK-009 records two separate `mise exec -- cargo nextest` commands with exact `test(=...)` selectors and the SDK journey's complete Postgres/migration wrapper at `tasks/TASK-009-run-context-and-python-otel-correlation.md:274-284`; R5 records all three as exit 0 at `TASK-009-R5-provider-identity-and-proof-closure.md:246-260`. | PASS |
| Replaced tasks carry the correct machine-readable lifecycle. | `TASK-009-R4-stabilize-provider-registration-idempotency.md:4` is `status: superseded`, and its historical body points to revision-46 R4E. | PASS |
| Permanent source contains no task/agent history or prohibited architecture. | Task references stay in `changes/active`; production source adds no task IDs, legacy names, server Run, second telemetry pipeline, wrapper span, mandatory OTel dependency, new Cargo feature, compatibility path, or client-managed server identity. | PASS |

## Material repository-rule findings

None.

The two prior standards findings are closed: replaced R4 is now `superseded`, and every named Rust proof is recorded with an exact repository-native command and required environment wrapper.

## Verification reviewed

The immutable task and remediation records report the focused provider regressions, full Python observation unit file (39 passed), exact shared Rust tests, exact Rust SDK journey under repository-managed Postgres and migrations, Python integration (72 passed), Python typing, code generation, PyO3 scope, Rust/Python formatting, Rust/Python linting, and `git diff --check` as passing. Earlier cumulative evidence also records the shared/Rust SDK and TypeScript unit, integration, typecheck, N-API, and client-tier lanes as passing.

This reviewer independently inspected the complete cumulative diff, owners, callers, manifests, lockfile, declaration sources/outputs, tests, and active change artifacts, and independently ran `git diff --check base..candidate`, which passed. Cargo-, Python-, and TypeScript-backed suites were not redundantly rerun during parallel review; their immutable command evidence was inspected. That is an execution-evidence limit, not missing authority or incomplete surface coverage.

## Overall result

**PASS**

All changed surfaces have complete applicable-authority coverage, and no material repository-standard violation remains.

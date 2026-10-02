# Repository Standards Review — TASK-009 r1

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Reviewed range: the complete cumulative `base..candidate` diff (23 changed files).
- Candidate identity was rechecked before this report and remained unchanged.
- CodeGraph was not used because this checkout has no `.codegraph/` directory.

## Authority coverage

| Changed surface | Governing authority read | Source and consumer coverage | Result |
|---|---|---|---|
| Shared Rust `Run` and `WyrdState` initial-Card behavior | `AGENTS.md` §§2–6, 8–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` §Observation identity; `architecture/wyrd-doctrine.mdx` Public surfaces; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; approved `run_api.md` | `crates/shared/wyrd-client/src/observe/mod.rs:42-97`, `state.rs:489-510`; all `Run::new`/`run_for_card` callers; in-module tests at `observe/tests.rs:649-682`; Rust SDK journey helper and call at `sdks/wyrd-sdk-rust/tests/observe_run.rs:378-415,560` | PASS |
| PyO3 projection and synchronous context manager | `AGENTS.md` §§7–8; `architecture/agent-rules.md`; `architecture/references/languages/pyo3-boundaries.md`; `python-api-and-stubs.md`; `rust-core.md` documentation and boundary rules; approved `run_api.md` | `sdks/wyrd-sdk-python/src/state/mod.rs:177-194`, `src/observe/mod.rs:156-246`; native wrapper registration and public `wyrd.state`/`wyrd.observe` projections; Python unit tests | PASS |
| Python OpenTelemetry correlation | `architecture/wyrd-design.md:600-629`; `architecture/references/domain/telemetry-observations.md`; `python-api-and-stubs.md`; `testing-workflows.md`; approved `run_api.md` | `python/wyrd/otel.py:186-307`; public import use in unit and journey tests; active/child, nested, async/concurrent, task-copy, idempotency, missing-package, unsupported-provider, enrichment and detach failure coverage at `tests/unit/state/test_observe_surface.py:267-428` | PASS |
| Python packaging, lockfile, and generated stubs | `AGENTS.md` §8; `architecture/agent-rules.md` generated-artifact rule; `python-api-and-stubs.md` Export Checklist and Stub Rules | `pyproject.toml:20-34` keeps OTel API optional and adds OTLP/HTTP only to the dev group; `uv.lock` resolves that dev dependency; source stub templates and generated `_wyrd.pyi`, `observe/__init__.pyi`, and `state/__init__.pyi` agree, including `Literal[False]` and `TracebackType` (`observe/__init__.pyi:46-69`) | PASS |
| Python real SDK → server journey | `AGENTS.md` §11; `architecture/references/languages/testing-workflows.md`; `python-api-and-stubs.md` Test Shape; telemetry authority | Existing journey is extended in place at `tests/integration/state/test_observe_journey.py:353-475,478-563`; it uses the stock OTLP/HTTP exporter, authenticated `/v1/traces`, explicit flush/shutdown/publication, and persisted Bifrost queries | PASS |
| TypeScript ergonomic and N-API surfaces | `AGENTS.md` §§2–5, 11; `architecture/agent-rules.md`; `architecture/references/languages/typescript-guide.md`; approved `run_api.md` | Thin native delegation at `native/src/cards.rs:299-311`; public optional parameter at `wyrd/src/index.ts:1855-1868`; committed generated N-API declarations at `wyrd/index.d.ts:644-652` and matching `.d.cts`; existing TypeScript server journey extended at `tests/integration/observe-run.test.ts:220-285` | PASS |
| Cross-language public behavior and observation identity | `AGENTS.md` Current Decisions and ownership boundaries; `wyrd-design.md` §Observation identity; `wyrd-doctrine.mdx` Runtime boundary/Public surfaces; `architecture/references/architecture/patterns.md`; `telemetry-observations.md` | Rust owns `RunId`, hydrated alias lookup, and immutable views; Python and TypeScript delegate to it. Python alone owns foreign-runtime OTel context. No server Run, new durable contract, new queue, client-authored tenant/principal/Card UID, or alternate transport entered the diff. | PASS |
| Verification and repository hygiene | `AGENTS.md` §§11–12; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; task implementation evidence | Reviewed the task-recorded focused and aggregate results; independently ran `git diff --check base candidate` (clean). No `#[ignore]`, gate weakening, production `#[allow]`, wildcard dependency, Cargo feature, compatibility alias, or legacy vocabulary was added. | PASS |

## Applicable-rule audit

| Rule | Evidence | Result |
|---|---|---|
| Shared client behavior belongs in `crates/shared/wyrd-client`; language SDKs remain thin projections. | `WyrdState::run_for_card` resolves the alias and constructs the shared `Run` (`state.rs:498-510`). PyO3 and N-API call that method rather than repeating lookup or identity logic (`src/state/mod.rs:188-193`; `native/src/cards.rs:306-311`). | PASS |
| Stateful/domain behavior has a concrete owner and discoverable inherent methods. | Initial selection is an inherent `WyrdState` method; immutable Card selection remains an inherent `Run` method (`observe/mod.rs:56-97`). No new trait, utility object, or module-level Rust workflow was introduced. | PASS |
| Rust uses typed domain identities and borrows inputs when ownership is unnecessary. | `Run` retains `RunId` and `CardRef`; `run_for_card` accepts `&str` and returns `Result<Run, WyrdError>` (`state.rs:508-510`). | PASS |
| Rust additions are synchronous unless directly awaiting IO. | Run construction and alias selection are synchronous and explicitly local; no async state machine was added. | PASS |
| New/materially changed Rust items, fields, helpers, and tests have meaningful rustdoc, including errors/panics where applicable. | `Run::new` and `WyrdState::run_for_card` document workflow and invariants; the fallible public method has `# Errors` (`state.rs:498-507`); added Rust tests/helpers have intent docs and the journey helper documents `# Panics`. | PASS |
| Public cross-boundary failures use the stable Wyrd error catalog. | Unknown alias continues through `WyrdError`/`WyrdPyError`/`NativeRunOpen`; all three language tests assert `WYRD_SDK_404_UNKNOWN_ALIAS`. No text-parsed or language-only error was added. | PASS |
| PyO3 is confined to the Python SDK and converts at the boundary. | New PyO3 methods are under `sdks/wyrd-sdk-python/src`; they use `Bound`, preserve Rust-owned `Run`, and delegate local selection to `wyrd-client`. No PyO3 type entered `wyrd-spec` or shared client. | PASS |
| Python blocking/async bridging uses the shared runtime, and no ad hoc runtime is created. | The changed `run`/context-manager path is synchronous and local. Existing IO bridging still uses `wyrd_runtime::runtime()`; this diff creates no runtime. | PASS |
| Python-visible changes cover native source, registration/export, public package, generated typing, and runtime tests. | Existing `Run` registration/export is extended rather than duplicated; public `wyrd.state`, `wyrd.observe`, and `wyrd.otel` imports are exercised. Source stub templates and generated stubs match; task evidence records green `py:typecheck` and `codegen:check`. | PASS |
| OpenTelemetry correlation uses exact record-level names and execution-local typed context, not process-global Card scope. | `_CARD_REF`/`_RUN_ID` are exact; `(card_ref, run_id)` is attached to OTel context (`otel.py:186-207,274-291`); tokens live in a `ContextVar` stack (`otel.py:189-197,294-307`). The only process-wide state is provider-registration bookkeeping and the private OTel key, not correlation identity. | PASS |
| Optional telemetry remains optional and fail-open without weakening Wyrd validation/errors. | Production dependencies remain unchanged except the pre-existing optional `otel` extra (`pyproject.toml:13-21`); OTLP/HTTP is dev-only (`:23-34`). OTel import, registration, enrichment, and detach failures return false/no-op (`otel.py:241-307`); unknown aliases still return stable Wyrd errors before entry. | PASS |
| TypeScript N-API stays a thin binding; synchronous work is not made `async`; public exports have explicit stable types. | Native `run` synchronously delegates to shared Rust (`native/src/cards.rs:305-311`); public `run(card?: string): Run` is synchronous and explicitly typed (`wyrd/src/index.ts:1855-1868`). No transport, lifecycle, or error semantics are duplicated. | PASS |
| N-API declarations are generated, committed, and checked. | Both `wyrd/index.d.ts` and `index.d.cts` carry the matching optional native parameter (`index.d.ts:644-652`); task evidence records green `ts:napi:check` and `codegen:check`. | PASS |
| Every user-facing SDK capability has a real user journey; runtime-dependent behavior is tested in its owning runtime. | Rust and TypeScript extend their existing server journeys; Python extends the existing real-server journey and separately tests Python OTel lifetime/context behavior in Python. No Python/Node lifetime is emulated in Rust. | PASS |
| Generated artifacts are not treated as authoritative source and regenerate cleanly. | Stub templates/source annotations changed alongside generated `.pyi`; N-API Rust source changed alongside generated declarations. The supplied evidence records green `codegen:check` and `ts:napi:check`. | PASS |
| Dependency cost remains in the narrowest owner; no required OTel dependency is added. | `opentelemetry-exporter-otlp-proto-http` is a Python SDK development dependency only (`pyproject.toml:23-34`), used by the real Python journey. The production `otel` extra remains API-only and optional. | PASS |
| No tenant, authorization, audit, secret, URL-fetch, SQL, or persistent-data boundary was changed. | The cumulative diff is client-local SDK behavior/tests/declarations only; Vala/server ingest and authorization code are untouched. The journey continues to prove server-resolved Card UID and authenticated publisher (`test_observe_journey.py:413-475`). | PASS (not materially changed) |

## Material findings

None.

No changed surface violates an applicable repository rule. No optional improvement is elevated into a blocking standards finding.

## Verification reviewed

The candidate task records all of the following as exit 0: focused shared Rust tests; focused Python unit file (30 passed); the exact Python OTLP journey; the exact Rust SDK journey; `mise run test:shared`; `test:wyrd-sdk`; `py:test:unit`; `py:test:integration` (72 passed); `py:typecheck`; `ts:test:unit`; `ts:test:integration`; `ts:typecheck`; `ts:napi:check`; `codegen:check`; `check:client-tier`; `check:pyo3-scope`; `fmt`; `py:format`; `lints`; `py:lints`; and `git diff --check`.

This standards pass did not rerun Cargo-backed verification because the review coordinator prohibited overlapping Cargo commands in the shared checkout. I independently inspected the cumulative source, manifests, lockfile changes, generated artifacts, callers, and tests, and independently confirmed a clean cumulative `git diff --check`. This is an execution-evidence limit, not a missing authority or coverage gap.

## Overall result

**PASS**

# Repository standards review

Overall result: **FAIL** — one material TypeScript declaration/dependency finding.

## Immutable subject and scope

- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Reviewed the cumulative base-to-candidate diff, expanded owners and consumers, and the committed execution evidence. This is a repository standards audit, not a task acceptance verdict or an optional style audit.
- Used CodeGraph before source navigation. HEAD remained the candidate; tracked index and working-tree source diffs were empty. Only this report is owned by this reviewer.

## Authority and surface coverage

The router was `architecture/references/README.md`. Governing authority included `AGENTS.md`, `architecture/agent-rules.md`, the applicable doctrine/client/runtime-identity/trace-context/observation/error-catalog sections of `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and the system-boundary/table-identity/durability/public-surface sections of `architecture/bifrost-design.md`. Security principles, trust boundaries, credential handling, tenant isolation, and privacy were checked against `architecture/wyrd-security-posture.md`.

The applicable references were read across Rust core, PyO3 boundaries, Python API/stubs, TypeScript, errors, testing workflows, maintainer style, spec-driven development, implementation execution, architecture patterns/constraints, and telemetry observations. `TESTING.md`, `docs/CONTRIBUTING.md`, and `docs/PRODUCT.md` were also inspected. Spec/task files were used for authority and execution-evidence context, not adopted as implementation proof.

| Changed surface (complete cumulative inventory) | Applicable authority | Source/consumer inspection |
|---|---|---|
| `crates/shared/wyrd-client/src/state.rs` | AGENTS §§3–6, 11, 16; Rust core; errors; architecture patterns | `TelemetryFlushFailure`, fields, constructors, error projection, settlement, in-module test; Python and N-API callers |
| `crates/wyrd-spec/src/error.rs` | AGENTS §§2–4, 9; errors; design error catalog | Both new derive-backed variants and fields, common detail projection, generated code consumers |
| Python `src/state/mod.rs` and authored `python/wyrd/state/__init__.pyi` | AGENTS §§7–8, 16; PyO3 boundaries; Python API/stubs | Managed-provider startup, flush/shutdown/helper, `Py<T>` retained ownership, shared block-on and error conversion, exported state registration/package projection |
| Python `python/wyrd/otel.py` | AGENTS §§2–3, 8, 10; Python API; telemetry; runtime/observation authority | Optional imports, exporter auth, provider registration, correlation, complete bridge callbacks and mapping; runtime unit/journey consumers |
| Python `Cargo.toml`, `pyproject.toml`, `uv.lock` | AGENTS §§1–3, 7–8, 11; dependency and testing references | Python floor and ABI agreement, optional agents extra, framework dev dependencies, collection/typecheck inclusion, resolved lock changes |
| Python `tests/unit/state/test_observe_surface.py` | AGENTS §11; testing/Python references | New setup, optional-import, owned-provider, lifecycle, token freshness, callback lifecycle and concurrency tests; existing scope/observation regressions |
| Python `tests/integration/otel/frameworks/{__init__.py,app.py,test_framework_telemetry.py}` | AGENTS §§10–11; testing; telemetry; SDK boundaries | Real framework models and local upstream, managed/owned provider startup, public SDK readback, scope/failure/sampling/privacy/auth/gateway scenarios, process isolation |
| Python `tests/integration/mcp_frameworks/test_agent_frameworks.py` | Python typing; testing; test integrity | Type updates and narrowed ADK tool-declaration extraction; existing MCP test outcomes retained |
| TS `native/src/cards.rs`, `wyrd/index.d.ts`, `wyrd/index.d.cts` | AGENTS §§3–6, 16; TS N-API guide; errors/codegen | Native failure projection delegates to shared Rust owner; generated declaration signatures and documentation match source |
| TS `wyrd/src/{index.ts,otel.ts,error-codes.ts}` | AGENTS §§2–3, 9–11; TS guide; errors; telemetry | Root exports, bridge callbacks/data mapping, state lifecycle, owned-provider correlation factory, stock exporter path and generated error-code additions |
| TS `wyrd/package.json`, `pnpm-lock.yaml`, `scripts/check_ts_package.mjs` | TypeScript declarations/package boundaries; dependency policy; testing | Optional peers, root/otel export maps, package inventory and clean-install JavaScript smoke; finding below covers missing clean-consumer typing proof |
| TS `tests/unit/telemetry.test.ts`, `tests/integration/agents-trace-processor.test.ts`, `tests/support/observed-service.ts` | AGENTS §11; testing; TS guide; telemetry | Runtime-local injection tests, actual Agents model/tool journeys, real server readback, gateway/auth/privacy matrix, shared fixture callers |
| `mise.toml` | AGENTS §11; testing/implementation execution | Framework journeys included in Python and TS Bifrost leaves; removed separate MCP lane is replaced by ordinary integration collection with installed dev dependencies |
| Design authority plus Observe/Connect/Quickstart/Understand docs | AGENTS §§2, 9, 11; doctrine; docs CONTRIBUTING/PRODUCT | Lasting provider/flush contract; install floor/name; language tabs/internal links; privacy/content and version caveats; publication flags preserved |
| Active spec and task packet | Spec-driven development; implementation execution | Approved revision and execution/deviation records retained with immutable subject |

No SQL capability signature/field, server handler, database migration, storage format, or server authorization implementation changed. There is no new `PgPool`/transaction capability path to excuse through a green tenant check. Existing server authorization remains the receiving authority.

## Per-rule results

| Rule | Result | Evidence and limits |
|---|---|---|
| Shared client/server ownership and foreign-runtime glue | PASS | Shared error semantics are in `TelemetryFlushFailure`; foreign OTEL provider callbacks remain SDK-local. No server behavior or second durable transport is introduced. Exporters use the existing client's per-export token path. |
| Pure foundational contract crate | PASS | New `WyrdError` variants add pure metadata and JSON fields; no IO, async, PyO3, telemetry implementation, SQL, or analytical dependency enters `wyrd-spec`. |
| Struct ownership and async boundaries | PASS | Shared failure value owns its projection state; Python state owns its installed provider. Rust conversion/settlement is synchronous. Existing runtime bridge performs native drains. |
| Rust safety/error conventions | PASS | No new production unwrap, ad hoc runtime, raw SQL handle, unstructured public error code, or secret-bearing debug owner in the Rust diff. |
| Rustdoc, including private items, fields, variants and tests | PASS | All materially changed Rust operations and all added items have substantive documentation. `settle` and modified fallible Python methods name error outcomes. `flush_telemetry` describes its nonthrowing result and safe cause projection; lifecycle docs explain ordering, retained provider, retries/partial failure. No material missing rustdoc found. |
| PyO3 runtime ownership and public Python projection | PASS | `Py<T>` retained provider, current `Bound` APIs, central error mapper and shared block-on remain in Python SDK. Public authored state stub changes match runtime lifecycle; bridge definitions are inline typed in the stub-less OTEL module. |
| Public TS declarations remain usable with optional framework dependencies absent | **FAIL** | `src/index.ts:1` imports Agents types into root public method/base declarations, while `package.json:56–59` makes that peer optional. See STD-TS-001. |
| Generated native/error artifacts source-driven | PASS (static) | N-API declaration and error-code changes correspond to Rust source additions. Committed evidence records `ts:napi:check` and `codegen:check`; not independently rerun by this reviewer. |
| Unit/integration/journey ownership and collection | PASS (structural) | Added real Python/TS SDK→server→SDK journeys use local upstreams and repository test servers. Python integration marker and owner/Bifrost task inclusion are present. Core error settlement has an in-module Rust test. Runtime-lifetime behavior is tested in its owning language. |
| Test integrity and source immutability | PASS (material threshold) | No new ignore/skip or weakened boundary check; no source edits made in review. Local mock services are credential-free. See execution/isolation notes below. |
| Dependency/floor/lane coherence | PASS | Python 3.11 agrees across package metadata, PyO3 ABI features and quickstart; framework imports are installed by ordinary dev setup. Agents remains an optional Python extra and optional TS runtime peer. |
| Privacy, correlation and tenant authority placement | PASS (standards scope) | Bridge allowlist reads model/name/usage and emits fixed/type-only failures; no raw prompt/tool/error payload mapping or client-supplied tenant authority added. Native residual content and Python refused-export logging are disclosed. Sensitive-domain correctness is outside this role. |
| Documentation taxonomy/publication discipline | PASS | Existing goal page and LangTabs extended, links point to existing task surfaces, version/content limitations documented, draft flags retained. Docs generation/check recorded in committed evidence. Recipe acceptance is the task reviewers' responsibility. |
| Required verification evidence | PASS (recorded; not independently rerun here) | Committed evidence names Rust format/lints, Python/TS checks and unit/integration runs, docs/codegen/native/dependency checks. Orchestrator additionally reports the exact new Rust settlement test passed (1 selected, 262 unselected; default features), recorded in this review's `verification.md`, alongside fresh Python/TS focused passes. This reviewer ran no Cargo/build/test lane. |

## Material finding

### STD-TS-001 — Optional Agents peer becomes mandatory for root TypeScript declarations

- Classification: **REGRESSION / public-contract violation**.
- Location: `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1`, `:2517`, `:2524–2540`; optional peer declaration at `sdks/wyrd-sdk-ts/wyrd/package.json:56–59`.
- Governing rules: AGENTS §2 first-class idiomatic typed SDK surface and §9 consistent public contracts; `architecture/references/languages/typescript-guide.md` “Types And Declarations” and “Placement And Boundaries”; the package's own optional-peer contract.
- Evidence: root source uses imported `TracingProcessor`, `Trace`, and `AgentsSpan<SpanData>` in exported class/base and callback signatures. Declaration emission necessarily retains the import; current emitted `wyrd/dist/index.d.ts:1` and `:1287–1296` show it explicitly. Root package `exports['.'].types` points at that declaration. The optional peer is not installed for a normal clean consumer. A type-only import removes JavaScript execution, but does not remove declaration resolution.
- Reachable consequence: an ordinary TypeScript consumer installing only `wyrd-ai` and importing existing `Cards`/`Bifrost` fails declaration checking with TS2307 for `@openai/agents-core` when `skipLibCheck` is false. This forces installation of a framework the caller does not use, despite its optional peer declaration. The new clean-package smoke in `scripts/check_ts_package.mjs:143–155` executes JavaScript only and cannot detect this regression.
- Smallest correction: keep the root bridge export and optional runtime behavior, but make its public callback contract self-contained using structural types for the trace/span members the bridge actually consumes; do not expose the optional package's nominal/base types from the root declaration. Use the already-installed real Agents SDK in SDK tests/typechecks to verify the structural processor remains accepted by `setTraceProcessors`. Regenerate declarations through the current build; do not hand-edit them or make Agents mandatory.
- Focused closure proof: typecheck a clean installed-package consumer importing an existing root export and the bridge, with no Agents package present and `skipLibCheck: false`; it must pass. Also compile ordinary registration of the bridge with the real supported Agents SDK installed. Retain the existing JavaScript optional-peer smoke and callback tests.

## Nonblocking execution/isolation notes

- The committed task evidence explicitly says U1–U5/T1–T5 were written alongside their fixes without separately captured RED runs. That is a procedural evidence limitation against the spec-driven execution reference; green source/tests do not reconstruct a missing historical RED. No separate code correction is proposed on that basis.
- `TESTING.md` prefers explicit clients for second principals and forbids changing principals through environment mutation. The new TS credential matrix uses `ambientState`/`vi.stubEnv` (`tests/integration/agents-trace-processor.test.ts:131–133,286–321`) to exercise the specifically required ambient-auth behavior. Reads use explicit reader clients, and the tests execute sequentially; no concrete current tenant leakage or false pass was established from this setup. This is not promoted into a material finding or a request to redesign the harness.

## Verification limits and final result

This review independently inspected source, declarations, manifests, fixture/caller paths and lane selection. It did not independently rerun the recorded lint/type/codegen/docs/integration suite and did not read other reviewer reports. The orchestrator's separately coordinated verification remains separate evidence. `git diff --check` for the immutable cumulative range was clean; HEAD and tracked source remained unchanged before report completion.

**FAIL**, proposed finding **STD-TS-001**. Rustdoc passes this audit; the failure is the reachable TypeScript declaration dependency regression.

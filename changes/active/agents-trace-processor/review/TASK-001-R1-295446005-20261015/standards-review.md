# Repository standards review

Overall result: **PASS**. No material repository-standards finding remains.

## Subject and method

- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `295446005b18604030bf376de6209098a28886f7`.
- Historical candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Authority: approved specification revision 2, original TASK-001, and the historical verdict, validation ledger and TASK-001-R1. Historical conclusions were hypotheses, not current closure proof.
- Reviewed the full cumulative range, using the latest remediation diff to find changed owners. CodeGraph was consulted before code navigation. Expanded implementations, public projections, generated declarations, package metadata, tests, fixtures, docs and lane selection were inspected. No other current reviewer report was read.
- HEAD matched the candidate at inspection and completion. No source, specification, task, index, dependency or generated output was changed. This reviewer owns only this report.

This is a standards audit, not an independent task-acceptance verdict or permission to redesign the task. Runtime and sensitive-domain correctness need the separate reviewers. PASS below means source conformance or credible recorded verification with the stated limits; it does not mean this reviewer reran the suite.

## Authority coverage

The routing authority was `architecture/references/README.md`. Selected language/reference files were read completely: spec-driven development, implementation execution, maintainer style, Rust core, PyO3 boundaries, Python API/stubs, TypeScript guide, testing workflows, errors, architecture patterns, architecture constraints and telemetry observations. `AGENTS.md`, `architecture/agent-rules.md`, `TESTING.md`, `docs/CONTRIBUTING.md` and `docs/PRODUCT.md` were inspected.

Applicable architecture coverage included Wyrd doctrine/client model, runtime identity and current-token export, trace context, observation identity, Bifrost facade and error catalog; the complete doctrine summary; Bifrost system boundary, table/row identity, durability/visibility and public surface; security principles/trust boundaries, tenant isolation, audit/privacy and secret handling. No new serving, SQL, persistent-storage, distributed-execution or server authorization implementation is introduced by the cumulative diff, so their internal algorithms are not substituted into this SDK standards review.

| Cumulative changed surface | Governing rules and references | Inspected implementation and consumers |
|---|---|---|
| Shared Rust client `state.rs` | AGENTS §§2–6, 11, 16; Rust core; errors; architecture patterns/constraints; design client model | `TelemetryFlushFailure`, both fields, constructors, `into_error`, `settle`, in-module settlement test; Python and N-API callers; existing native state drain owner |
| Pure contract `wyrd-spec/src/error.rs` | AGENTS §§2–4, 9, 16; errors; design error catalog | New 504/503 variants and field docs, derive metadata and existing `message_details` projection; generated TS code union and SDK conversions |
| Python native `src/state/mod.rs` | AGENTS §§3, 6–8, 16; PyO3; Python API/stubs; Rust core | Managed provider field/startup, flush/shutdown, private `flush_telemetry`; retained `Py<PyAny>`, shared runtime/error bridge and public state registration/projection |
| Python `python/wyrd/otel.py` | AGENTS §§2–3, 8, 10; Python API; telemetry observations; design identity/export authority | Optional imports, correlation processor and installation lock, current-token stock exporters, `_ManagedTelemetry`, bridge callback lifecycle and `_gen_ai`; native state and real framework callers |
| Authored Python state stub | AGENTS §8; Python API/stubs; errors | `flush`/`shutdown` signatures and failure documentation match native methods; package exports remain the existing public state projection; pure-Python bridge is declared inline only |
| Python `Cargo.toml`, `pyproject.toml`, `uv.lock` | AGENTS §§1–3, 7–8, 11–12; PyO3 and testing references | Optional boundary feature, `abi3-py311` and Python 3.11 floor, optional production Agents extra, ordinary framework dev dependencies, locked resolution, pytest/typecheck collection |
| Python unit state tests | AGENTS §§10–11, 16; testing workflows; Python API | Optional imports, setup/refusal/cleanup, correlation/retry/sampling, provider failure precedence, held real stock export deadline, current-token HTTP export, actual framework data callbacks and thread safety; top-level test functions |
| Python framework `app.py`, parent journey and empty package initializer | AGENTS §§10–11; testing workflows; telemetry; architecture client boundary | Actual Pydantic/ADK/OpenAI builders and deterministic local upstream, process isolation for global provider/import ownership, managed live-flush barrier, native observation and provider continuation, application-owned setup, negative/auth/privacy/readback paths |
| Existing Python MCP framework tests | AGENTS §11; Python API/testing; test integrity | Dependency-group prose removed, supported tool union, ADK declaration narrowing and typed callback result; original tool-discovery/action assertions retained |
| TS native `cards.rs`, generated `index.d.ts`/`index.d.cts` | AGENTS §§3–6, 16; TypeScript N-API guide; errors/codegen | `telemetry_flush_error` delegates shared Rust error ownership; generated signature/docs match the native addition; existing native lifecycle owner retained |
| TS `src/index.ts`, `src/otel.ts` | AGENTS §§2–3, 9–11; TS guide; errors; telemetry; design SDK boundary | Structural Agents callback inputs, bridge/data mapping, explicit optional runtime access, state startup recheck, lifecycle settlement, application-owned correlation factory, exporter/header configuration and refused-provider cleanup |
| Generated TS `error-codes.ts` | AGENTS §4; errors/codegen; TS guide | Two code literals correspond to the derive-backed Rust variants; no independent invented code |
| TS package manifest, lock, package checker | AGENTS §§1–3, 11–12; TypeScript package/declaration rules | Root/OTEL exports, optional peer contract, clean JavaScript import, clean root TypeScript noEmit consumer with declaration checking enabled; installed-framework compile consumer |
| TS telemetry unit tests and framework journey | AGENTS §§10–11; TypeScript/testing; telemetry | Real Agents assignability; setup overlap/refusal, correlation/sampling, lifecycle errors, fresh-token exports, callback boundaries; actual model/tool/server readback and live/native/global-provider continuation |
| Shared TS `observed-service.ts` fixture | AGENTS §11; testing workflows; maintainer style | Existing registered/hydrated Card fixture extended with existing `ServerOptions`; original `observedServiceTest` consumers retained through default factory call; no second server lifecycle owner |
| `mise.toml` | AGENTS §§11–12; testing/implementation execution | Framework journeys added to existing Bifrost language leaves; ordinary Python dev setup/collection replaces separate dependency-group lane; integration and aggregate dependency paths retained |
| Wyrd design change and four docs pages | AGENTS §§2, 9, 11; doctrine/design; docs CONTRIBUTING/PRODUCT | Lasting SDK exporter/lifecycle authority, goal page and LangTabs, framework install/construction/invocation/readback, provider/content/sampling limits and failures, existing cross-links, Python install floor/name and preserved draft scope |
| Active spec/task and historical review packet | Spec-driven development and implementation execution | Approved revision retained, cumulative immutable identities, findings and appended execution evidence; historical reports remain historical rather than rewritten as current approval |

No changed production signature or field takes a SQL capability: no raw `PgPool`, connection or transaction was introduced. No new server handler, route, migration, Scribe write path, audit producer, table schema, permission check or URL-fetch authority exists in this write set. Green tenant checks are not used to excuse a raw SQL handle.

## Per-rule results

| Applicable rule | Result | Source-backed assessment |
|---|---|---|
| One shared client; language code must earn its boundary | PASS | Shared `TelemetryFlushFailure` owns public precedence/projection (`state.rs:2097–2165`); Python/TS remain runtime OTEL integrations over existing native drains and stock exporters. No duplicate registry, storage or durable transport implementation. |
| Pure foundational contract crate | PASS | New error variants only add pure fields/derive metadata and projection arms. No PyO3, async runtime, SQL, network or telemetry implementation enters `wyrd-spec`. |
| Struct-centered Rust and synchronous pure operations | PASS | Failure value owns meaningful outcome state; conversion/settlement are synchronous. State remains dependency/lifecycle owner. No new ad hoc runtime, broad trait or zero-sized utility owner. |
| Rust safety, ownership, error catalog and imports | PASS at material threshold | Rust additions use existing Wyrd errors, owned boundary causes and borrowed native code. No new production unwrap, unchecked cast, secret debug representation or lint suppression. One nonmaterial module-import note is recorded below. |
| Rustdoc on every new/materially modified item, including private helpers and tests | PASS | Audited every cumulative Rust addition/change, not only R1. Shared failure type/fields/constructors/projection/settlement and test have docs; error variants/fields and materially extended `message_details` have docs; native N-API projector has docs. Python state field, startup, flush/shutdown and private helper have substantive docs. Fallible modified methods and settlement carry `# Errors`; partial drain/retry/provider effects are described. No missing/placeholder rustdoc is waived as style-only. |
| PyO3 placement, GIL lifetimes and runtime bridge | PASS | New glue stays in Python SDK. Provider retained as `Py<PyAny>`, no `Bound` crosses await, no stored reusable `PyErr` or new Tokio runtime. Native drain continues through `crate::trace::block_on`; bounded provider waiting runs in Python with standard synchronization. |
| Python public export/stub parity | PASS | Existing state registration/package export covers modified native methods. Authored stub signatures match runtime; bridge owns inline definitions in `otel.py`, without duplicate stubs. Runtime-specific testing remains in Python. |
| TS optional dependency and public declaration usability | PASS; prior finding closed | Root class no longer imports/implements the optional Agents type. Local structural callback fields are declared in `index.ts:2493–2531`; real installed SDK assignment is checked at `telemetry.test.ts:236`. Checker `:157–175` compiles root APIs and processor in the clean project with `--skipLibCheck false`. See prior closure table. |
| Generated artifacts must come from source | PASS, static plus recorded checks | Generated N-API signature/docs match `cards.rs:294–316`; generated code union matches Rust variants. Original execution evidence records `ts:napi:check` and `codegen:check`; R1 changes structural source and uses normal build/package checking. No evidence of hand-edited divergence found. |
| Test tiers and runtime ownership | PASS, structural | Rust-only settlement remains an in-module Rust test. Python/Node lifetime behavior is tested in those runtimes. Actual framework journeys use real SDK/server/query and repository-managed Postgres; runtime-local timeout/mapping injection supplements, rather than replaces, journeys. |
| No weakening of tests/checks or hidden integration collection | PASS at material threshold | Removed MCP task is replaced by normal dev dependencies and ordinary integration collection. Required framework paths are explicitly in Bifrost language leaves. New package checking strengthens the existing clean-package property without a second checker. No changed ignore/skip or boundary weakening. |
| Dependency cost and Python floor consistency | PASS | Production Agents dependency remains optional; framework packages live in the SDK dev group. Python metadata, PyO3 ABI and quickstart agree on 3.11. Owner-directed floor/lane changes are recorded in original evidence; no additional production dependency/Cargo feature introduced. |
| Privacy, authenticated identity and tenant authority | PASS for placement | Allowlisted bridge mapping excludes raw payload/error messages; current-token authentication remains the existing shared client path (`otel.py:191`, TS OTEL config). No client tenant selection or durable authorization substitute. Receiving server authority is unchanged; detailed security correctness belongs to domain review. |
| Docsite taxonomy, publication and executable-procedure standards | PASS | Existing Observe goal page retains frontmatter/draft scope, native language tabs and internal links. Framework packages and actual construction precede invocation; authenticated readback and provider/content/version caveats are present. Design retains durable contract, not task history. |
| Required format/type/codegen/docs/dependency/test evidence | PASS as recorded evidence | Original task records owning checks and journeys; R1 records clean package check, TS/Python unit/types/lints, Rust fmt/lints, docs generation/check and focused managed journeys. No broad rerun is inferred from a focused result. Fresh execution limits follow below. |

## Prior-finding closure checked from current source

The prior standards report's sole material proposal was `STD-TS-001`, validated as **FIND-TASK-001-1**. This report does not inherit its previous PASS rows automatically.

| Historical ID | Current source evidence | Standards disposition |
|---|---|---|
| FIND-TASK-001-1 / STD-TS-001 | Optional `@openai/agents-core` import/base removed; self-contained structural public callbacks in root source; private OTEL map type does not escape the class declaration. Root optional peers remain optional. Existing clean checker now has noEmit consumer, and real SDK assignability is retained in unit/integration source. | CLOSED. Recorded RED was TS2307; recorded GREEN clean package/type checks directly cover the former failure. No mandatory-peer workaround or weakened lib checking. |
| FIND-TASK-001-2 | Observe page supplies per-framework installations, concrete model/agent/tool setup, ADK session/runner/message, invocation and readback. | Documentation standards closure supported by source; full acceptance belongs to task reviewers. |
| FIND-TASK-001-3 | Python app live phase prints only after public state flush and waits for parent readback; same state writes a second observation, then exports after shutdown. TS managed journey `:233–258` mirrors the distinct phases with real framework work. | Required test-tier structure is present; original shutdown-only proof is no longer the only journey. |
| FIND-TASK-001-4 | `_gen_ai` reads direct Responses usage with per-count full-response fallback; callback test uses actual `ResponseSpanData`, late usage and zero without response content. | Runtime owner and pure mapping test placement conform; no new sensitive payload capture. |
| FIND-TASK-001-5 | `_ManagedTelemetry` at `otel.py:216–254` retains one Future/lock and daemon flush attempt around stock provider lifecycle. Native helper docs explain bounded wait and timeout classification. Held-export test `test_observe_surface.py:847` exercises actual batch processor, native drain code, callback progress and bounded workers. | Standard-library boundary coordination conforms to selected remediation; no batch queue, retry engine or callback scheduler added. Runtime semantics need concurrency/system review. |
| FIND-TASK-001-6 | `startTelemetry` rechecks same-state ownership immediately after dynamic import; unit setup overlaps two calls and preserves other-owner refusal/cleanup. | Existing owner extended with no second startup registry; regression proof is runtime-native. |

No new material standards finding is proposed, so no new `FIND-*` ID is allocated here.

## Nonblocking observations

- The new Rust settlement test imports `TelemetryFlushFailure` inside the test function (`state.rs:3957`). Agent rules require module-top imports, with tests as their own scope rather than a blanket function-import exemption. Moving that import to the test module's existing import block is a small conformance correction, but no behavioral/public-contract consequence is established, so it cannot alone block under the review skill.
- Some local TS structural types use `interface` rather than the guide's preferred `type`; Python runtime docstrings retain the existing concise style. Neither changes accepted calls, results or generated semantics. No style-only remediation is requested.
- Original task evidence explicitly lacks separately captured RED runs for several initial unit scenarios. R1 supplies focused RED/GREEN records. Historical RED cannot be reconstructed from green code, and no retrospective fabrication is requested.
- TESTING.md's general no-subprocess/no-principal-environment-mutation examples do not match every framework fixture. The Python task records the concrete global OTEL/ADK import-lifetime need for process isolation; environment selection exercises approved ambient-auth behavior. TS paths use sequential isolated test environments and explicit reader clients. No demonstrated cross-tenant leakage or false pass makes these a separate material standards finding.

## Verification limits and result

This reviewer ran only read-only navigation/source/Git inspection and cumulative `git diff --check` (PASS). No installs, build, lint, test, live server or external research were run. Prior/R1 command results are recorded claims assessed against source and selectors, not newly executed proof. Full aggregate verification remains the orchestrator/change-review responsibility. Source/index stayed immutable; only current review artifacts were untracked.

**PASS** — complete cumulative standards coverage, prior standards finding closed, no material repository-rule finding. No missing Rustdoc was found or downgraded.

Skipped independent suite execution and source edits. Remaining risk is unrerun runtime/packaging evidence, assessed separately by the coordinated review.

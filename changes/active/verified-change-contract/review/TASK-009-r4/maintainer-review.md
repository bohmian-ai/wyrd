# Maintainer review — TASK-009 cumulative candidate

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations reviewed: `TASK-009-R1`, `TASK-009-R2`, and `TASK-009-R3`

The candidate remained at the stated commit while this review was performed.

## Authorities read

- `AGENTS.md`, especially §§3, 5–8, 11, and 16
- `architecture/agent-rules.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- `architecture/wyrd-design.md`, especially Observation identity
- `architecture/wyrd-doctrine.mdx`
- `changes/active/verified-change-contract/spec.md`, REQ-123, REQ-151, and AC-032
- `changes/active/verified-change-contract/architecture/logic/run_api.md`
- The original task and all three remediation tasks, including their recorded verification evidence

The repository has no `.codegraph/` directory, so repository navigation used `rg`, the cumulative Git diff, and direct source inspection as instructed.

## Changed-surface coverage

| Changed surface | Owner, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Shared Run construction and initial Card selection | `crates/shared/wyrd-client/src/observe/mod.rs:42-114`, `crates/shared/wyrd-client/src/state.rs:489-510`; all `Run::new`, `run_for_card`, and relevant `run()` callers; in-module Run tests and Rust SDK journey | `WyrdState` remains the discoverable state owner. `Run::new(state, subject)` is a narrow constructor, and alias resolution stays on `WyrdState::run_for_card` before identity minting. Names, borrowed alias input, return type, and rustdoc expose the invariant without duplicating selection in language SDKs. |
| Shared Rust tests | `crates/shared/wyrd-client/src/observe/tests.rs:648-692` | Scenario names state caller-visible outcomes. The two added tests are beside the owner and now document their panic conditions. No new harness or abstraction was introduced. |
| Rust SDK journey projection | `sdks/wyrd-sdk-rust/tests/observe_run.rs:378-413` and its call from the existing scoped-observation journey | `assert_initial_card_selection` keeps the cross-language journey readable and has substantive workflow and panic documentation. It reuses the existing state/run objects rather than building a parallel fixture. |
| Python `WyrdState.run` boundary | `sdks/wyrd-sdk-python/src/state/mod.rs:177-194`; generated/public state stubs and Python initial-selection tests | The PyO3 method is a thin projection over the shared owner, uses the approved keyword-only `card`, preserves stable error conversion, and documents the no-IO and failure contract. Runtime and generated typing agree. |
| Python `Run` context-manager boundary | `sdks/wyrd-sdk-python/src/observe/mod.rs:156-249`; public/generated observe stubs and direct protocol tests | `PyRun` remains an immutable wrapper. Entry and exit delegate foreign-runtime behavior to the Python SDK owner without moving Rust identity or observation behavior. The conventional exception names, `None` defaults, and `Literal[False]` declaration now match runtime behavior. Rustdoc explains execution-local ownership, failure containment, and non-durability. |
| Python OpenTelemetry correlation owner | `sdks/wyrd-sdk-python/python/wyrd/otel.py:190-326`; every direct caller in `PyRun`; provider, nesting, async, failure, and restoration tests | The existing OTel module is the natural owner. A `ContextVar` stack owns exact per-execution entry state, the processor is intentionally duck-typed to keep the SDK optional, and provider bookkeeping stays local and weakly referenced. Functions and state are named by their role; comments explain only non-obvious key creation and recovery behavior. No second telemetry pipeline, provider wrapper, or configuration object was added. |
| Python focused tests | `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:215-601`, including the R3 same-Run asyncio case | Tests are top-level, outcome-named, and use the stock SDK plus deterministic `asyncio.Event` coordination. The same-Run case directly closes the prior proof gap without sleeps, a fixture framework, or production changes. Private SDK inspection is confined to the one idempotency assertion that cannot be observed from emitted attributes. |
| Python real-server journey | `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:353-565` and surrounding existing fixture/setup/readback flow | The extension stays in the existing journey and separates token exchange, provider setup, framework-style emission, and persisted join assertions into cohesive helpers. It uses public SDK/server paths and explicit provider flush, state shutdown, and publication barriers. The local token helper follows the same established test pattern in `tests/integration/bifrost/test_bifrost_e2e.py`; importing that file's private helper would make ownership worse. |
| Python dependency and generated artifacts | `pyproject.toml`, `uv.lock`, source stub fragments, assembled `_wyrd.pyi`, and public `observe`/`state` stubs | The OTLP/HTTP exporter is dev-only and earned by the required real journey; production dependencies remain unchanged and OTel stays optional. Source stub fragments and generated projections are in parity, including imports, optional Card selection, and context-manager protocol types. |
| TypeScript native and public projections | `sdks/wyrd-sdk-ts/native/src/cards.rs:299-312`, `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1849-1866`, generated `index.d.ts`/`index.d.cts`, and the existing integration journey | The N-API boundary delegates to shared Rust and returns the existing closed open-result shape. The public wrapper exposes the approved idiomatic optional argument with an explicit return type and aligned JSDoc. Generated declarations match the native signature. |
| TypeScript journey | `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:220-283` | The added assertions extend the existing journey, cover root default, selected Card, shared/fresh invocation identity, immutability, and unknown alias, and add no new setup or helper layer. |

## Material findings

None.

## Verification assessment

The task and remediation records provide focused and broader green evidence for the changed surfaces: exact shared-Rust Run tests, the Rust SDK journey, the complete Python observation surface including the same-Run async isolation case, the authenticated persisted Python journey, Python unit/integration/typecheck/format/lint, TypeScript unit/integration/typecheck/N-API declaration checks, shared and SDK test lanes, code generation, client-tier and PyO3 boundary checks, Rust format/lints, and `git diff --check`.

Those checks match the public signatures, generated declarations, execution-local behavior, and journey seams reviewed above. This maintainer pass did not rerun them; it inspected the recorded commands and the candidate source they cover.

## Uncertain preferences for calibration

None. The remaining choices—duck typing the optional SDK provider, keeping one test-local token exchange helper, and using focused private inspection for processor-count proof—have concrete boundary or proof reasons and are not preference-level debt.

## Overall result

**PASS**

Every materially changed symbol is placed with its owning capability, has an understandable caller path and typed public projection, carries the required documentation, and is covered by readable focused or journey evidence. No material maintenance cost or declaration drift remains.

# Maintainer Review — TASK-009 round 6

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46
- Task chain: TASK-009 plus R1, R2, R3, R4E (superseding R4), and R5

## Changed-surface coverage

| Surface | Symbols and owning paths reviewed | Callers, declarations, and proof reviewed | Result |
|---|---|---|---|
| Shared Rust Run ownership | `WyrdState::run`, `WyrdState::run_for_card` in `crates/shared/wyrd-client/src/state.rs:489-511`; `Run::new`, `Run::for_card`, `Run::observe` in `crates/shared/wyrd-client/src/observe/mod.rs:42-115` | Existing Rust callers found under `wyrd-client` and `wyrd-sdk-rust`; focused unit coverage in `observe/tests.rs:606-691`; Rust journey helper and caller in `sdks/wyrd-sdk-rust/tests/observe_run.rs:378-413,577` | PASS — the behavior remains on the state/run owners, names state the selection distinction, and the fallible initial selection is explicit. |
| Python state projection | `PyWyrdState::run` in `sdks/wyrd-sdk-python/src/state/mod.rs:177-194` | Public declaration in `python/wyrd/state/__init__.pyi:178-190` and generator source in `python/wyrd/stubs/state.pyi`; unit callers in `test_observe_surface.py:215-231`; persisted journey caller in `test_observe_journey.py:384-402` | PASS — keyword-only `card`, return type, error behavior, and documentation agree across PyO3 and generated declarations. |
| Python Run context protocol | `PyRun::__enter__`, `PyRun::__exit__`, and `PyRun::observe` in `sdks/wyrd-sdk-python/src/observe/mod.rs:195-257` | `Run` declarations in `python/wyrd/observe/__init__.pyi` and generator source; protocol-shape test at `test_observe_surface.py:526-539`; nested, async, same-object concurrency, failure, and mismatch tests at `test_observe_surface.py:269-383,572-680` | PASS — the immutable native owner stays separate from optional Python runtime work, conventional context-manager names/defaults are declared, and the delegation is easy to trace. |
| Python OTel correlation owner | `_RunCorrelationProcessor`, `_outcome_entry`, `install_run_correlation`, `_enter_run`, `_exit_run` in `sdks/wyrd-sdk-python/python/wyrd/otel.py:192-334` | Direct public use in the Python journey and unit suite; provider identity, retained failed processor, optional dependency, nested scope, active-span, and failure-containment coverage in `test_observe_surface.py:392-680`; persisted OTLP/HTTP path in `test_observe_journey.py:365-475,477-560` | PASS — the module has one discoverable owner for provider registration and context correlation; comments and docstrings explain the nonstandard token-free stack, weak identity bookkeeping, activation boundary, and fail-open behavior. |
| TypeScript projection | `NativeWyrdState::run` and `NativeRunOpen` in `sdks/wyrd-sdk-ts/native/src/cards.rs:299-312,390-445`; public `WyrdState.run` in `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1855-1866` | Generated native declarations in `index.d.ts` and `index.d.cts`; integration callers in `wyrd/tests/integration/observe-run.test.ts:220-283` | PASS — the native result remains the existing closed outcome shape, the public method exposes the narrower SDK contract, and generated declaration parity is present. |
| Rust/Python/TypeScript tests and dependency support | New/changed tests in shared Rust, Rust SDK, Python unit/integration, and TypeScript integration; Python dev dependency in `pyproject.toml` and `uv.lock` | Scenario names describe caller-visible outcomes; the HTTP OTLP exporter dependency is confined to the Python dev group and used only by the persisted journey | PASS — tests are located at the appropriate unit or journey tier and avoid a new production dependency. |
| Architecture and task evidence | `changes/active/verified-change-contract/architecture/logic/run_api.md`; TASK-009 and R1/R2/R3/R4E/R5 | Revision-46 public signatures and OTel lifecycle text; R5 records exact focused Rust commands and broader Python, codegen, formatting, lint, and boundary lanes | PASS — the maintained contract, implementation names, generated declarations, and recorded proof use the same vocabulary. |

## Material findings

None.

## Uncertain preferences for calibration

- `sdks/wyrd-sdk-python/python/wyrd/otel.py:200-205,245-287` represents each private provider outcome as a two-slot `list[Any]`. A named private record would give the weak reference and terminal Boolean self-documenting fields, but the current representation is deliberately local, its two positions are documented, and R5 explicitly selected this minimal identity-tracking mechanism. I did not treat an equally valid representation preference as a blocking finding.
- `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:261-266` inspects OpenTelemetry SDK internals to count installed processors. That assertion is version-sensitive, but it directly proves the required one-processor invariant and the SDK is locked; replacing it would require a less production-representative fake. No finding.

## Verification assessment

The task packet records successful focused tests for the two shared Rust selectors, the Rust SDK persisted journey, 39 focused Python unit tests, the Python persisted journey, Python unit/integration/typecheck, code generation, PyO3 scope, Rust/Python formatting and linting, and `git diff --check`. The candidate also contains Rust, Python, and TypeScript journey coverage for the initial-Card API, with the Python journey exercising real OTLP/HTTP persistence and Run-based joins.

I inspected the complete base-to-candidate source diff, owning modules, reachable callers, tests, architecture contract, and generated declaration pairs. I did not rerun build or test lanes in this review-only pass; the cited results are the immutable task evidence.

## Overall result

**PASS**

No material maintainer issue remains in layout, ownership, naming, signature shape, documentation, test readability, or generated declaration parity.

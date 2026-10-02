# TASK-009 R2 Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`

The checkout resolved to the stated candidate before source inspection. The
review covered the complete base-to-candidate range and treated the prior R1
finding ledger as hypotheses, not conclusions.

## Changed-surface coverage

| Surface | Source, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Shared Rust Run owner | `WyrdState::{run,run_for_card}`, `Run::{new,for_card,observe}`, hydrated alias lookup, shared unit tests, and the Rust SDK journey | Initial Card selection stays on the existing `WyrdState`/`Run` owners, preserves typed `CardRef` and `RunId`, and adds no parallel resolver or abstraction. The remediated `Run::subject` rustdoc now accurately covers root, initially selected, and sibling views. |
| Python PyO3 boundary | `PyWyrdState::run`, `PyRun::{for_card,__enter__,__exit__,observe}`, module registration, and observation methods | The boundary remains thin: Rust owns Run identity and Card lookup, while the context-manager methods delegate foreign-runtime work to `wyrd.otel`. Names, signatures, error behavior, and ownership are discoverable from the owning types. |
| Python OpenTelemetry owner | `wyrd/otel.py` end to end: private context key, execution-local token/prior stack, processor registration, processor hooks, entry, exit, and explicit installation hook | One cohesive module owns optional Python OTel integration. The remediation reuses the existing lock and OTel context API; it introduces neither another synchronization object nor another context system. `_key`, `_enter_run`, and `_exit_run` document the non-obvious concurrency and restoration invariants beside the code that enforces them. |
| Python focused tests | Initial selection, active/child spans, nesting, asyncio propagation, provider idempotency, optional-package absence, registration/attach/enrichment/detach failures, and concurrent first entry | Tests are named for caller-observable outcomes and keep interpreter-dependent behavior in Python. `_drift_reaches_the_ordinary_boundary` removes repeated proof without hiding the asserted error. `_ContentionLock` is a narrow test-only helper that deterministically proves the first-use race and is justified by that single concurrency scenario. |
| Python authenticated journey | Existing scoped-observation journey, OTLP/HTTP provider setup, framework span emission, lifecycle barriers, and persisted trace/custom/Eval joins | The implementation extends the existing Service journey and public SDK path rather than adding a second fixture or in-memory substitute. Setup, emission, and read-back stages remain findable and separately named. |
| TypeScript/N-API projection | `NativeWyrdState::run`, public `WyrdState.run(card?)`, generated native declarations, and TypeScript journey coverage | The N-API method delegates to shared Rust and returns the established handle-or-error shape. The public wrapper exposes the idiomatic optional argument and explicit `Run` result; runtime, wrapper, and declarations agree. |
| Python public typing and generation | Stub templates, generated package stubs, aggregate `_wyrd.pyi`, `Run.__enter__/__exit__`, and `WyrdState.run(*, card=...)` | Public annotations describe the actual context-manager and optional-card contracts. `TracebackType` and `Literal[False]` preserve the precise `__exit__` behavior. Templates and generated outputs are in parity. |
| Dependency and lockfile | Python `pyproject.toml` and `uv.lock` | The stock OTLP/HTTP exporter is development-only and directly supports the required authenticated journey; production OTel remains optional. The lockfile change matches that one dependency. |

## Prior-finding closure

| Prior finding | Closure assessment |
|---|---|
| `FIND-TASK-009-2` / `MR-001` | Closed. `Run::subject` now states the three reachable construction/view cases and agrees with `WyrdState::run`, `run_for_card`, and `Run::for_card`. No runtime or API change was bundled into the documentation correction. |
| `FIND-TASK-009-1` | The focused failure cases now share one clearly named explicit-observation assertion and keep each injected OTel failure visible at its test site. |
| `FIND-TASK-009-3` | The existing execution-local entry now stores its prior value with the exact token, and `_exit_run` owns both normal detach and fail-open restoration. The implementation and nested/same-context test describe the same invariant. |
| `FIND-TASK-009-4` | Lazy key creation is serialized with the existing registration lock and a second `None` check. The deterministic two-thread test names the first-use behavior without introducing a reusable concurrency harness. |

## Material findings

None.

## Calibration notes

- The existing `_wyrd_processors` test helper reads pinned OpenTelemetry SDK
  internals to prove per-provider idempotency. This remains version-sensitive,
  but it is scoped to tests, directly proves an approved obligation, and is not
  a material maintenance defect in this candidate.
- `Any` at the optional provider/context boundary is deliberate duck typing:
  adding a local protocol would create an abstraction without improving runtime
  validation or the supported-provider contract.

## Verification assessment

The original task and R1 remediation record successful focused Python tests,
the authenticated persisted journey, shared and language SDK lanes, type and
code-generation checks, boundary checks, formatting, lints, and
`git diff --check`. I inspected the relevant tests, generated parity, and full
caller paths; I did not rerun those overlapping lanes for this maintainer-only
audit.

## Overall result

**PASS.** Every materially changed symbol has a cohesive owner, accurate
contract documentation, aligned public declarations, and scenario-focused
proof. The prior maintainer finding is closed and no material maintenance cost
remains.

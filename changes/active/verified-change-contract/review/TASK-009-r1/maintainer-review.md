# TASK-009 Maintainer Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`

The candidate still resolved to the stated commit immediately before this report was written.

## Changed-surface coverage

| Surface | Source and caller/test coverage | Maintainer assessment |
|---|---|---|
| Shared Rust Run ownership | Read `WyrdState::run`, `WyrdState::run_for_card`, `Run::new`, `Run::for_card`, their state/index lookup, shared unit cases, and the Rust SDK journey. | The workflow remains on the existing state and Run owners, uses the hydrated graph once, preserves typed `RunId`/`CardRef`, and adds no abstraction. One field invariant is now documented incorrectly (MR-001). |
| Python PyO3 projection | Read `PyWyrdState::run`, `PyRun::{for_card,__enter__,__exit__,observe}`, module registration, generated declaration sources and generated public stubs. | The boundary is thin, uses `Bound`, retains Python work at the Python boundary, and keeps the native Run immutable. Signatures and stubs agree. |
| Python OpenTelemetry integration | Read `wyrd/otel.py` end to end, including provider registration, execution-local token stack, processor hooks, entry/exit delegation, and public hook callers. | The optional integration has one focused module, reuses OpenTelemetry context and processors, and avoids a second telemetry pipeline. Names and ownership are discoverable. |
| Python tests and journey | Read the added initial-card, nesting, active/child span, asyncio, provider-idempotency, absence/failure cases, plus the authenticated OTLP/HTTP journey and persisted join assertions. | Tests are scenario-named, use public SDK surfaces, and keep Python-lifetime behavior in Python. The integration additions reuse the existing journey rather than creating another harness. |
| TypeScript/N-API projection | Read `NativeWyrdState::run`, the public `WyrdState.run(card?)` wrapper, generated N-API declarations, and the TypeScript SDK journey. | The native boundary delegates to shared Rust; the public wrapper keeps the idiomatic optional positional argument and explicit `Run` result. Declarations match runtime shape. |
| Rust SDK projection | Read the Rust journey addition and shared-client re-export/call path. | The SDK remains thin over `wyrd-client`; the test proves root default, initial selection, unknown alias, and same-run sibling identity without a parallel implementation. |
| Dependency and generated-artifact changes | Read `pyproject.toml`, lockfile diff, Python stub templates/outputs, and TypeScript declaration outputs. | The OTLP/HTTP exporter is development-only as required by the journey. Source templates and generated outputs move together; recorded `codegen:check` and typecheck results are green. |

## Material findings

### MR-001 — Stale `Run::subject` rustdoc contradicts the new constructor invariant

- **Changed location:** `crates/shared/wyrd-client/src/observe/mod.rs:52`
- **Governing rule:** `AGENTS.md` required struct-centered Rust style and Rust documentation requirements; `architecture/references/languages/maintainer-style.md` requires materially modified items and fields to document their actual invariant.
- **Evidence:** `WyrdState::run_for_card` now resolves an alias and calls `Run::new(self.clone(), subject)` directly (`state.rs:508-510`), so a newly opened Run can begin on any hydrated Card. The `subject` field still says it is “the root Service until `for_card`,” while the changed `Run::new` correctly accepts the already-selected subject.
- **Concrete maintenance cost:** A maintainer reading the owner type is told an invariant that is no longer true and can incorrectly assume non-root subjects only arise from sibling views. That obscures the distinction the task intentionally added between root construction and initial Card selection.
- **Smallest testable correction:** Update only the field rustdoc to state that it is the exact Card for this view: the root for `WyrdState::run`, the selected alias for `run_for_card`, and the selected alias for a sibling `for_card` view. No code or test change is needed; `fmt`/docs lint is sufficient proof.

## Calibration notes

- `test_observe_surface.py::_wyrd_processors` inspects OpenTelemetry SDK private fields to count registrations. That is mildly version-sensitive, but it directly proves the idempotency obligation and the SDK version is locked; it is not material enough to block this task.
- `install_run_correlation(provider: Any = None)` is a deliberately duck-typed optional-dependency boundary. A local provider protocol could narrow editor help, but the approved API accepts framework providers without importing the SDK, so requiring another type abstraction would not improve this change enough to justify it.

## Verification assessment

The task records successful focused Rust and Python cases, the authenticated Python journey, all three SDK lanes, Python/TypeScript typechecks, N-API and code-generation checks, boundary checks, format/lint lanes, and `git diff --check`. I inspected the relevant tests and declaration parity but did not rerun Cargo or other overlapping verification during this independent review.

## Overall result

**FAIL.** Ownership, API shape, tests, and generated declarations are maintainable, but MR-001 is a material documentation contradiction on the modified core `Run` owner and must be corrected before acceptance.

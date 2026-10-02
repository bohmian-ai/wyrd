# Maintainer review — TASK-009 cumulative candidate

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediation tasks: `TASK-009-R1-restore-otel-correlation.md` and `TASK-009-R2-close-proof-and-boundary-parity.md`
- Applicable authority: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` observation identity, `architecture/wyrd-doctrine.mdx`, `changes/active/verified-change-contract/architecture/logic/run_api.md`, `architecture/references/domain/telemetry-observations.md`, and the Rust/PyO3/Python/TypeScript maintainer guides.

The candidate was still `d01307b8c37b47488115743c94b624a29d66e4be` after source inspection. No reviewed source was modified.

## Changed-surface coverage

| Surface | Owner and changed symbols | Callers, consumers, and proof inspected | Maintainer assessment |
|---|---|---|---|
| Shared Rust Run model | `WyrdState::run`, new `WyrdState::run_for_card`, `Run::new`, `Run::for_card`, and corrected `Run::subject` documentation | Shared inline Run tests; Rust SDK `assert_initial_card_selection`; Python and N-API projections | PASS — initial selection remains on the existing concrete owners, alias lookup stays in `WyrdState`, and immutable views continue to share only the existing state and invocation identity. Names, result types, error documentation, and owner/method shape are direct and discoverable. |
| Python PyO3 boundary | `PyWyrdState::run`, `PyRun::__enter__`, `PyRun::__exit__`, existing `PyRun::for_card`, and observation access | Public `wyrd.state`/`wyrd.observe` stubs; focused runtime signature test; context-manager, failure, and journey callers | PASS — the wrapper is thin, uses the shared Rust owner for Card selection, and delegates Python-runtime context work to the Python SDK. Conventional `__exit__` names/defaults now agree with runtime and generated declarations. |
| Python OTel owner | `install_run_correlation`, `_key`, `_RunCorrelationProcessor`, `_enter_run`, `_exit_run`, and execution-local registration/token state in `wyrd/otel.py` | Active/child, nested, asyncio, copied-task, private/global-provider, missing-package, registration/attach/enrichment/detach, and concurrent-first-entry tests; authenticated OTLP journey | PASS — the behavior is cohesive in the existing OTel module, uses native OTel context and one existing lock, and documents the fail-open boundary and lifecycle. Helper names and state describe their roles; no provider lifecycle or durability behavior is hidden here. |
| Python declarations and exports | Owning `stubs/state.pyi`, `stubs/observe.pyi`, header imports, generated `state/observe` projections, and aggregate `_wyrd.pyi` | Runtime PyO3 signatures and public-package imports | PASS — `run(*, card=...)`, `Run.__enter__`, and `Run.__exit__` have precise parameter and return types, useful contract documentation, and generated parity. The public OTel helper remains in its natural Python module with a typed boolean outcome. |
| Python dependency and journey | Dev-only OTLP/HTTP exporter and lockfile; extended scoped-observation journey | `otlp_provider`, `emit_framework_scope`, and persisted trace/custom/Eval join assertions | PASS — the new dependency is confined to development proof, while production OTel remains optional. Journey helpers separate token exchange, provider setup, emission, and persisted assertions without creating a second fixture or production abstraction. |
| TypeScript projection | `NativeWyrdState::run(card)`, generated N-API declarations, public `WyrdState.run(card?)` | `Run.fromOpen`, shared error projection, and TypeScript integration journey | PASS — the N-API layer remains a thin projection of `WyrdState::run_for_card`; the public wrapper has an explicit return type and aligned JSDoc. Both generated declaration variants match the Rust binding. |
| Rust and TypeScript journey coverage | Rust SDK initial-selection helper and TypeScript journey assertions | Root default, selected Card, fresh invocation, same-run sibling, and unknown alias | PASS — scenarios are expressed through the public SDKs and assert caller-visible outcomes. The new Rust helper and shared Rust tests include the required panic documentation. |

## Material findings

None.

The cumulative change does not introduce a catch-all module, duplicate Card-resolution path, single-implementation trait, new configuration object, or language-specific durable behavior. The few nontrivial Python OTel helpers are warranted by the foreign-runtime boundary and stay beside their only owner and tests.

## Verification assessment

The task and remediation records report passing focused Python scope/signature tests, the authenticated persisted OTLP journey, shared and language SDK lanes, Python typing, code generation, N-API generation, formatting, lints, and boundary checks. Static review confirmed source/declaration parity and `git diff --check` passed for the immutable range. The evidence directly covers the changed public surfaces and failure behavior.

## Preferences for calibration

None. Test-only inspection of OpenTelemetry SDK internals in `_wyrd_processors` is narrowly used to prove per-provider idempotency and does not warrant a maintainer finding.

## Overall result

**PASS**

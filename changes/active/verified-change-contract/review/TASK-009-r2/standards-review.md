# Repository standards review — TASK-009-r2

## Subject and independence

Fresh reviewer `standards_resume`; orchestrator preserved returned report under reviewer file-edit prohibition. Base `7d96c30066425e0cde2290842d5801307843283d`; candidate `c47761decff8db95768d2c05f0b85b8ba62a021a`. Original TASK-009 and R1 remediation/ledger/verdict reviewed. Candidate spec is approved revision 45; task/remediation retain revision-35 mappings. Relevant REQ-123/151, INV-007/012 and AC-032 remain present. Entire cumulative 33-file diff reviewed. HEAD stayed candidate; only r2 untracked; no peer reports read; no CodeGraph index.

## Proposed findings

### REPO-009-1 — VIOLATION: context-manager runtime/declaration mismatch

Locations: `sdks/wyrd-sdk-python/src/observe/mod.rs:225`, stub source `python/wyrd/stubs/observe.pyi:57`, generated public `observe/__init__.pyi:59`.

Authorities: AGENTS §8, Python API/stubs Stub Rules, maintainer style typed contract. Runtime exposes `(self, /, _exc_type=None, _exc_value=None, _traceback=None)`, but stub exposes required `exc_type`, `exc_value`, `traceback`. Declared keyword calls are accepted by typing and rejected by runtime; no-argument call is accepted by runtime and rejected by stub. Ordinary positional with-protocol hides mismatch.

Correction: align PyO3 and owning stub source with conventional names, preserve defaults and Literal[False], regenerate via existing assembler. Do not edit generated output manually. Proof: public keyword call returns False, signature/default parity, py:typecheck and codegen:check.

### REPO-009-2 — VIOLATION: new Rust test panic docs missing

Locations: `crates/shared/wyrd-client/src/observe/tests.rs:648,674`. AGENTS §16 explicitly covers test functions and requires # Panics when panic remains possible; agent rules reiterate. New run_for_card selection/unknown tests have expect/expect_err/assert paths but only summary rustdoc. Nearby new Rust SDK `assert_initial_card_selection` documents equivalent panic conditions. This violates explicit documentation completion while assertions are appropriate.

Correction: brief # Panics describing fixture/selection/identity assertion failure for these two tests; preserve assertions and untouched tests. Static doc inspection and formatting suffice; no runtime harness needed.

## Authority coverage

| Surface | Authorities read | Source/consumers |
|---|---|---|
| Shared Rust Run/state | AGENTS §§2–6,10–12,16; agent rules; design/doctrine; patterns/constraints; Rust/errors | Run/state/card resolution, owner tests, SDK projections |
| PyO3 Run/state | AGENTS §§3,6–8,16; PyO3; Python API/stubs; errors/maintainer | PyRun/state, registration/public exports |
| Optional OTel | AGENTS §§2–3,9–10; design identity; telemetry; architecture constraints; Python | complete key/stack/processor/install/entry/exit and callers/tests |
| Python generated declarations | AGENTS §8; generated rule; Python/maintainer | source templates/header; assembler; generated aggregate/public outputs |
| TS native/public/declarations | AGENTS §§2–6,16; TS/errors/design client | native run, wrapper, d.ts/d.cts, integration/native recipe |
| Tests all runtimes | AGENTS §§11–12,16; testing workflows/TESTING; maintainer | shared tests, Rust helper/journey, Python surface/OTLP journey, TS journey |
| Dependency/lock | AGENTS §§1–3; ownership; task authorized development exporter | production/optional/dev deps and lock |
| Task records | AGENTS §14; spec-driven/implementation-execution; review skill | task/R1 evidence, mapped spec, mise recipes |

Router-selected references and complete design/doctrine read. No SQL/server auth/analytical/deployment/persistent implementation changes; those are unchanged contract consumers.

## Rule results

| Rule | Result | Evidence |
|---|---|---|
| Shared ownership and SDK projections | PASS | shared lookup/Run; Python/TS delegate |
| Typed identities/immutable views | PASS | CardRef/RunId on owner |
| Struct-centered cohesive Rust | PASS | inherent methods on existing owners |
| Sync computation/narrow async | PASS | no async/runtime added |
| PyO3 location/gating/lifetime/GIL | PASS | SDK Bound methods; no blocking network/await |
| Stable errors | PASS | reused WyrdPyError/native outcome |
| Registration/public exports | PASS | existing module wires PyRun |
| Generated sources/parity process | PASS | templates changed with outputs; assembler/native generator |
| Runtime declaration parity | FAIL | REPO-009-1 |
| Rust item/error/panic docs | FAIL | REPO-009-2; production methods otherwise documented |
| Optional foreign OTel | PASS | lazy imports; no mandatory dependency |
| Execution-local scope/server identity | PASS | ContextVar, only CardRef/Run authored |
| Lifecycle ownership | PASS | cleanup-only; explicit journey barriers |
| Runtime/tier test ownership | PASS | Rust/Python/Node homes; gated real server |
| Real journey reuse | PASS | existing Service/SDK journeys extended |
| Unit isolation | PASS | offline/in-memory; server integration separate |
| No weakening/suppression | PASS | added proof/synchronization; no ignore/allow weakening |
| Vocabulary/features/permanent task refs | PASS | no legacy/task IDs/new production dependency/feature |
| Canonical verification | PASS by recorded evidence | owning mise lanes confirmed |

## Verification and result

Independently inspected runtime signature via `mise exec -- uv run --project sdks/wyrd-sdk-python python -c 'import inspect; from wyrd.observe import Run; print(inspect.signature(Run.__exit__))'`: mismatch confirmed. Cumulative git diff check passed. Broader shared/SDK/Python/TS/codegen/boundary/fmt/lint results inspected as supplied evidence, not rerun. py:setup testing feature and correction of nonexistent py:setup:testing confirmed. Real OTLP/persisted joins present. No material open questions or suggestions.

**FAIL** — REPO-009-1 and REPO-009-2 require independent validation.

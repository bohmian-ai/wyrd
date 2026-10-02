# TASK-009 invariant review — Round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `c47761decff8db95768d2c05f0b85b8ba62a021a`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediation: `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`
- Prior verdict and ledger: `changes/active/verified-change-contract/review/TASK-009-r1/{verdict,findings-validation}.md`

Fresh independent reviewer: `invariants_resume`. The orchestrator preserved this report from the reviewer's returned content because its review-only role prohibits file edits. The full cumulative diff and surrounding callers were inspected; no other R2 reports were read. HEAD matched the candidate. No `.codegraph/` directory exists.

Authorities inspected: AGENTS.md, agent rules, spec-driven development, maintainer style, telemetry-observations, testing-workflows, Run API, applicable observation-identity sections of wyrd-design, approved specification and original/remediation obligations.

## Proposed finding

### INV-REV-R2-001 — MISSING; retain FIND-TASK-009-1

Actual processor registration failure still lacks explicit-observation proof. AC-032 separately requires focused proof for an API-only/no-SDK provider and processor registration failure. TASK-009 Scenario 3 preserves both. The remediation's compressed “API-only or registration failure” wording cannot remove the approved criterion.

Location: `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:375–388,414–425`. The `Failing` provider raises in `add_span_processor`, but its test only calls `install_run_correlation` and asserts False. It neither enters a Run nor emits an observation. The new failure-to-observation test uses `object()`, reaching the no-registration-method return at `otel.py:260–262`, not the exception path at `otel.py:269–275`.

Producer-to-consumer trace: PyRun entry delegates to `_enter_run`, which calls installation before attach. The provider produces the registration exception; installation contains it and returns False. Explicit Drift independently consumes the immutable native Run and its ordinary projection/lifecycle boundary. Source supports independence today, but the required injected-failure-to-observation evidence is absent. A future coupling could break that behavior while the current suite remains green; this is a proof gap, not a current production exception.

Smallest correction: reuse the focused test home and `_drift_reaches_the_ordinary_boundary`. Select a provider whose `add_span_processor` raises through the global lookup used by Run entry, enter a Run, and emit Drift, asserting `WYRD_SDK_400_BIFROST_NOT_STARTED`. Preserve API-only, attach, enrichment, detach, missing-package, unknown-alias and user-exception proof. No production change, dependency, fixture framework or method matrix is needed.

Focused closure: from `sdks/wyrd-sdk-python`, run `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`.

## Invariant and consumer tracing

`run_for_card` resolves the hydrated alias before `Run::new`. The constructor owns one invocation and exact subject; `for_card` creates immutable sibling views sharing invocation and writer. Python and TypeScript delegate to this owner. Explicit observation consumers obtain correlation directly from these values.

Python `_key` publishes one private key under the existing lock. Entry attaches the pair and stores `(token, prior)` in an execution-local ContextVar. The processor reads the parent context. Exit consumes the matching entry, attempts exact-token detach, and restores the prior pair after raising or swallowed reset. Correction stays at the lifecycle owner and protects subsequent/nested span consumers without downstream guards.

Drift/Eval/custom emissions remain independent of ambient correlation. Eval's active-span ID capture is a separate best-effort consumer. The real journey uses a private provider, stock authenticated OTLP/HTTP, explicit provider flush/state shutdown/server publication, then asserts persisted CardRef, Run, managed Card UID and publisher, custom Run joins, and Eval trace/span joins.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Initial selection in all SDKs | shared state.rs:498–510; Python state:174–194; TS native cards:299–313 and public wrapper | shared, Python, Rust/TS journey selection tests | PASS |
| Root default and immutable shared invocation | Run new/for_card/correlation | root/sibling unit and journey checks | PASS |
| Strict unknown alias before IO | hydrated lookup before construction | shared and language negative tests | PASS |
| Synchronous Python entry and exception propagation | PyRun enter/exit returns False | entry and user-exception assertions | PASS |
| Exact active/child span attributes | entry and processor | active/child/grandchild and persisted journey | PASS |
| Nested restoration | token/prior stack and exit | healthy and failed-detach nesting | PASS |
| Await, copied task context and concurrent task isolation | OTel/ContextVar execution-local state | asyncio focused test | PASS |
| One key across first entries | double-check under existing lock | deterministic two-thread test | PASS |
| Failed detach restores prior correlation | otel.py:313–326 | same-context outer/after spans | PASS |
| Missing/API-only/attach/enrichment/detach fail-open observations | exception containment; native explicit path | representative Drift errors | PASS |
| Actual registration failure has required observation proof | exception contained at otel.py:269–275 | installation-only failure test; observation test uses API-only object | FAIL — INV-REV-R2-001 |
| Idempotent global/private providers | lock and weak provider registry | provider tests/private journey | PASS |
| Authenticated persisted trace/custom/Eval joins | journey provider/emission/query stages | recorded original/remediated journey | PASS |
| Exit has no lifecycle/durability effect | cleanup only; separate barriers | source ordering and focused tests | PASS |
| Optional dependency; no wrapper, second pipeline, server Run, log/metric promise or client managed identity | production dependencies/server contracts unchanged | diff and recorded boundary gates | PASS |
| Accurate subject rustdoc | observe/mod.rs:52–55 | static source, recorded fmt/lint | PASS |
| Existing observation/auth contracts | native sibling consumers unchanged; no Vala production edits | journeys/broader recorded checks | PASS |

## Prior closure and verification

FIND-TASK-009-1 is partially closed, retaining only actual registration-failure proof. IDs 2, 3 and 4 close through corrected subject documentation, recorded-prior restoration, and serialized key publication respectively. The remaining gap shares the original proof root and needs no new ID.

The orchestrator supplied 33 passing focused tests. Recorded persisted journeys and broader language/codegen/boundary/format/lint results were inspected as supplied evidence; this reviewer did not rerun Cargo or Postgres lanes. No material open questions or optional suggestions.

## Overall result

**FAIL** — one proposed finding, INV-REV-R2-001 / FIND-TASK-009-1.

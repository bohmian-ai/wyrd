# TASK-009 telemetry domain review — Round 2

## Subject and independence

Fresh reviewer: `telemetry_resume`; preserved by the orchestrator from its returned report under the role's file-edit prohibition. Base `7d96c30066425e0cde2290842d5801307843283d`; candidate `c47761decff8db95768d2c05f0b85b8ba62a021a`. Inputs are the approved specification, original TASK-009 and R1 verdict/ledger/remediation under `changes/active/verified-change-contract`. Full cumulative diff reviewed; HEAD remained candidate; source unchanged; no R2 reports read. No CodeGraph index exists.

## Coverage

| Boundary | Authority and source |
|---|---|
| Invocation/exact Card | REQ-123, original task; shared state/Run and SDK projections |
| Optional foreign runtime | REQ-151, Run API, AGENTS/agent rules, Python/PyO3 references; PyRun enter/exit |
| Enrichment/propagation/restoration | REQ-151, AC-032, telemetry-observations; complete otel.py and focused tests |
| Signed scope/managed identity | wyrd-design observation identity; OTLP handler and RecordCorrelation extraction/authorization |
| Persisted joins | AC-032, Run API/testing references; real Python journey |
| Prior closure | R1 ledger/verdict/remediation; cumulative diff |

## Proposed finding D-TEL-R2-001 — MISSING

Retain prior FIND-TASK-009-1 for actual processor-registration-failure → explicit-observation proof. AC-032 lists API-only provider and registration failure separately. At `test_observe_surface.py:375–388`, the raising `add_span_processor` provider only exercises installation. At lines 414–425, Run/Drift proof uses `object()`, reaching `otel.py:260–262` capability rejection rather than `269–273` registration exception.

Entry calls installation before attach. A provider can produce a registration exception, which is currently contained; Drift later uses the independent native Run/Bifrost boundary. Source supports present independence but no focused check traverses the full required path. This leaves contractual regression proof absent, not a demonstrated current runtime exception.

Correction: extend the existing focused failure test with a global selected provider whose registration raises, enter Run and reuse the representative Drift assertion for `WYRD_SDK_400_BIFROST_NOT_STARTED`. Preserve API-only and attach proof. No production change, dependency, fixture or observation matrix. Closure: the exact focused surface file.

## Assessment

| Obligation | Source/proof assessment | Result |
|---|---|---|
| Shared identity and immutable initial/sibling selection | native owner and delegated projections | PASS |
| Exact active/framework span attributes | entry/processor; active/child/grandchild and persisted checks | PASS |
| Nested/async/copy/isolation | ContextVar tokens/prior values; focused scenarios | PASS |
| Concurrent first-use key | double-check existing lock; thread overlap proof | PASS |
| Raising/swallowed detach recovery | prior-value restoration; same-context outer/after proof | PASS |
| Global/private idempotency | serialized registry; provider tests/journey | PASS |
| Optional failures preserve observations | all listed cases proven except actual registration failure | FAIL — D-TEL-R2-001 |
| Strict signed Card and managed identity | local strict aliases; only CardRef/Run injected; server authorization and managed columns | PASS |
| Stock authenticated export and persisted joins | existing Service journey queries real spans/custom/Eval | PASS by source and supplied journey results |
| Lifecycle/non-goals | cleanup-only exit; separate flush/shutdown/publication; optional production OTel, no second pipeline/wrapper/log/metric/managed-identity changes | PASS |
| Subject invariant doc | root/initial/sibling wording | PASS |

Prior IDs 2, 3 and 4 are closed. ID 1 remains only for the narrowed proof gap. No additional runtime defect, open question or optional suggestion.

## Verification and result

Independently ran from Python SDK: `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`: **33 passed in 0.86s**. Broader recorded checks/journeys were inspected as supplied evidence; Postgres/Cargo/codegen/language-wide lanes were not independently rerun. These limits do not replace the acceptance gap.

**FAIL** — D-TEL-R2-001 / FIND-TASK-009-1.

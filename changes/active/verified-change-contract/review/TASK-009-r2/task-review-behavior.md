# TASK-009 behavior review

## Subject and independence

Fresh reviewer `behavior_resume`; report preserved by the orchestrator from returned content under reviewer file-edit prohibition. Base `7d96c30066425e0cde2290842d5801307843283d`, candidate `c47761decff8db95768d2c05f0b85b8ba62a021a`. Inputs: approved spec and TASK-009 mapped revision-35 obligations, original task, R1 verdict/ledger/remediation. HEAD matched before/after; source unchanged; full cumulative diff reviewed; no CodeGraph index.

Read AGENTS, agent rules, spec-driven development, maintainer style, telemetry/testing references, doctrine, relevant design client/observation identity and Bifrost boundaries, spec/Run API/task/remediation. Expanded shared state/Run/Card lookup/RunId and observations, Python frozen PyRun/conversions/span capture, complete OTel owner and sibling OtelObserver, TS wrapper/native/declarations, generated Python templates, dependencies and all changed focused/journey tests.

## Findings

No material behavioral findings.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Root default/one UUIDv7 invocation | state:494–510, Run:59–69, RunId owner | root/invocation focused tests | PASS |
| Local initial selection before construction | state:508–510, hydrated card_ref indexes | shared/Python and Rust/TS journey selection | PASS |
| Unknown/out-of-graph strict before IO | reused lookup error | shared/Python negatives; SDK journey assertions | PASS |
| Immutable sibling shares invocation | Run::for_card preserves state/ID | shared/Python/Rust/TS proof | PASS |
| All SDK calls delegate shared owner | PyO3 state, native TS, Rust re-export | declarations/templates and recorded type/codegen | PASS |
| Sync Python entry returns same Run | PyRun returns slf | active-child focused case | PASS |
| Exact active and new span attributes | otel.py:291–294 and processor:222–231 | active/child/grandchild, conflicting attribute case | PASS |
| Nesting restores outer Card | ContextVar exact token/prior | nested focused case | PASS |
| Await/copy/concurrent task isolation | OTel context and immutable stack | asyncio/copy-after-parent-exit focused case | PASS |
| Global/private provider setup | automatic install/explicit hook | global/private counts and real journey | PASS |
| Registration thread-safe/idempotent | lock and weak registry | provider counts | PASS |
| Optional missing/unsupported/failing providers | contained imports/install/PyO3 calls | missing/API-only/provider tests | PASS |
| Enrichment/attach/detach cannot block explicit observations | contained optional owner, native sibling Drift | ordinary offline writer errors | PASS |
| Exact-token failed-detach restoration | exit:313–326 checks/restores prior | same-context outer and after spans | PASS |
| User exceptions propagate | exit false | user-exception assertion | PASS |
| Alias/validation/auth remain strict | no catch on selection/emission; server unchanged | negative cases/existing journeys | PASS |
| Stock authenticated OTLP real journey | provider:365, emission:384, Service credential | recorded exact persisted journey and source | PASS |
| Exact persisted Run/CardRef/UID/publisher | assert_scope_joins:413 | row/correlation assertions | PASS |
| Custom join by Run | existing dataset/SQL join | two spans joined to row | PASS |
| Eval captures and joins exact active IDs | emission:395–401; active_span_ids | persisted trace/span/Run/Card/context assertions | PASS |
| Exit cleanup separate from durability | local scope methods, explicit provider/state/server barriers | source/stub docs and ordering | PASS |
| INV-007 managed identity/tenancy/audit | only CardRef/Run attributes; server unchanged | authenticated journey | PASS |
| INV-012 reuse contracts/writer | native selection/correlation; no schema change | original extended journeys | PASS |
| Non-goals excluded, TASK-002 preserved | no server Run, extra writer/queue/wrapper/global Card/lifecycle/log/metric/managed-identity changes | cumulative diff/callers | PASS |
| OTel optional; exporter dev-only | manifest/lock | static dependency inspection | PASS |
| R1 uses existing owners/mechanisms | same lock/stack/context/test home | source/focused cases | PASS |

## Prior closure

FIND-TASK-009-1: reviewer considers API-only-or-registration approach selected by remediation followed, with attach/enrichment/detach now paired to Drift, and missing-package/user-exception preserved. FIND-TASK-009-2: subject docs cover root/initial/sibling. FIND-TASK-009-3: prior restored after raising/swallowed reset in same context. FIND-TASK-009-4: one lock-protected key, deterministic overlapping thread proof. All considered closed by this reviewer.

## Ponytail and verification

Processor needed to enrich framework children; shared lookup/UUID reused. No unnecessary abstraction/config/transport/lifecycle ownership found. Independently ran from Python SDK `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`: **33 passed**, exit 0, and cumulative diff check exit 0. Broader recorded Rust/Python/TS, persisted journey, typing/codegen/boundary/fmt/lint evidence inspected, not independently rerun. mise confirms py:setup builds testing, explaining corrected nonexistent py:setup:testing command.

**PASS**. No open behavioral question in this reviewer's assessment.

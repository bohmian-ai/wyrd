# TASK-001 r1 — Review Verdict

## Verdict: `FIX_REQUIRED`

Findings: `FIND-TASK-001-1` … `FIND-TASK-001-10`.
Remediation task: `TASK-001-R1-verifier-binding-closure.md` (same directory).

## 1. Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract` (worktree, branch `verified-change-contract`).
- Base: `5293546f3`. Candidate: `2e09ae81213cb608253b75de276e3c946353ac35`.
- HEAD verified at the candidate at the start and end of both waves; the subject did not change during review.
- Approved spec: `changes/active/verified-change-contract/spec.md` revision 32.
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`.
- Diff: 235 files, +40149 / −10537.
- Round: 1. No prior findings exist to close.

## 2. Agent topology

| Wave | Role | Report | Result |
|---|---|---|---|
| 1 | `task-rev` | `task-review.md` | **FAIL** (TR-1…TR-8) |
| 1 | `repo-rev` | `standards-review.md` | **FAIL** (SR-1…SR-8) |
| 1 | `domain-rev` (persistent data / migrations / Bifrost tables) | `domain-review-data.md` | **PASS** |
| 1 | `domain-rev` (security, authz, trust boundary) | `domain-review-security.md` | **FAIL** (DS-1…DS-3) |
| 2 | `ponytail-rev` | `findings-validation.md` | **FIX_REQUIRED** (10 retained, 9 rejected/folded) |

All required agents were fresh and independent; no agent filled two roles. Wave 1 reviewers received no other reviewer's conclusions and no intended verdict; `ponytail-rev` received the complete diff, authorities, and all four Wave 1 reports with no intended verdict.

## 3. Acceptance matrix

Reproduced from the independent `task-rev` audit (`task-review.md`), adjusted where `ponytail-rev` rejected the underlying finding (TR-7).

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-045 Verifier registrable via normal envelope/CardRef/composite/schema/loader | `envelope.rs` `CardKind::Verifier`, `Spec::Verifier`; `card/verifier.rs` | `verifier_card_tests::*`; typed_state/end_to_end journeys | PASS |
| REQ-046 one adjacently tagged closed `implementation` | `verifier.rs:40` | `unknown_implementation_kind_fails_to_deserialize` | FAIL (FIND-3) |
| REQ-047 variants exactly Drift(DriftSpec)/Eval(EvalSpec) | `verifier.rs:44-49` | spec lib tests | PASS |
| REQ-056 no Verifier DAG | one implementation per Verifier | source | PASS |
| REQ-090 `verified_by` on Service, component, standalone Agent | `service.rs`, `agent.rs`, `verifier.rs::VerificationBinding` | visitor test; journeys cover component location only | FAIL (FIND-2, FIND-4) |
| REQ-091 Ref / InlineableRef\<Trigger\> / InlineableRef\<Operator\> semantics | `VerificationBinding`; `SlotValue::InlineableTrigger/Operator` | loader e2e: path→sibling, inline Trigger | FAIL (FIND-4) |
| REQ-092 resolve, UID-pin, kind/tenant/authz, nested traversal, relationships, fail closed | `resolve.rs` visitor-driven collect/bind + `validate_effective_bindings` | hydrate journey; Python exactness assertion | FAIL (FIND-2 fail-open nested path; FIND-4 no refusal proof) |
| REQ-093 TriggerSpec flattened closed activation | `trigger.rs` | loader e2e | FAIL (FIND-3) |
| REQ-094 / REQ-143 OperatorSpec flattened; Workflow rejected in `on_failure` | `operator.rs`; `composition.rs::check_on_failure`; `resolve.rs:98-116` | none | FAIL (FIND-2, FIND-4) |
| REQ-102 duplicate Verifier per occurrence rejected | `composition.rs::binding_validation_errors` | `publication_validation_rejects_duplicate_targets`, loader dup test | PASS |
| REQ-103 `verified_by` replaces `publishes_to`, no alias | field removed from all specs | grep | FAIL (FIND-5) |
| REQ-109 / INV-014 Drift/Eval not registrable; 15 kinds; authorities agree | `CardKind`; migration `20260601000025`; design/doctrine updated | kind-count and refusal tests | FAIL (FIND-5) |
| REQ-110 Drift payload removals; SPC+Metric rejected; Statistical only | `drift.rs`; `vala-drift` fitter branch removed | `rejects_spc_with_metric_signal`, `rejects_non_statistical_conditions` | PASS |
| REQ-111 Eval payload retained | `EvalSpec` wrapped unchanged | `eval_verifier_yaml_deserializes_as_verifier_card` | PASS |
| REQ-113 reuse existing mechanisms | visitor, composite pipeline, `vala-eval` reused | source | PASS |
| REQ-114 authorities describe Verifier model | design/doctrine/bifrost/references updated | `codegen:check` | FAIL (FIND-5) |
| REQ-116 remove eval.runs/assertions, drift_alerts + SQL API | `tables/mod.rs` (6 builtins), migration `20260910000028`, vala-sql deletions | `test:sql` | PASS (code); regression assertion missing (FIND-4) |
| REQ-120 retire `/v1/eval/runs` | route, client handle, CLI `--server` deleted | grep | FAIL (FIND-6) |
| REQ-144 retire vala-core alert router, crate, schemas, generator, check | crate/workspace/mise/check/test-families removed | grep clean | PASS |
| INV-001 bindings inline, not Cards | `VerificationBinding` is not a kind | source | PASS |
| INV-006 no secrets / runtime health in Verifier | typed fields, `details` map removed | none | FAIL (FIND-3) |
| INV-012 reuse Drift/Eval contracts | payloads wrapped unchanged except approved removals | spec lib tests | PASS |
| INV-013 inline == referenced semantics | flattened Trigger/Operator shapes | source | PASS (shape); strictness gap is FIND-3 |
| AC-004 unknown/secret payloads rejected before persistence | partial | partial | FAIL (FIND-3, FIND-4) |
| AC-018 (TASK-001 portion) binding registration forms | component bindings journey | hydrate journey | FAIL (FIND-4) |
| AC-021 (TASK-001 portion) old kinds unregistrable | spec-level rejection | unit only | FAIL (FIND-4 loader half, FIND-5 schemas) |
| AC-022 retirement portion | tables/crate/check removed | `test:sql`, grep | FAIL (FIND-1, FIND-6) |
| Canonical visitor covers new slots; no old alias | `refs/mod.rs` | visitor test | PASS |
| Only Drift/Eval as implementations; neither a kind | `VerifierImplementation` | tests | PASS |
| Engines reusable, no future executor | `vala-drift`/`vala-eval` retained | `test:shared` | PASS |
| Constraint: no compat alias / binding Card / DAG / speculative variants | none found | grep | PASS |
| Constraint: no second reference walker | loader/server/refs use `ReferenceSlotVisitor` | source | PASS (binding-location enumeration duplicated — folded into FIND-2) |
| Constraint: no TS CardKind abstraction | TS diff = generated error codes only | source | PASS |
| Constraint: wyrd-spec IO/async/PyO3-free | none added | `check:pyo3-scope` rc=0 | PASS |
| Constraint: generated files regenerated, not hand-edited | `codegen:check` rc=0 | rc=0 | PASS |
| AGENTS §12 no gate circumvention | `verifier.rs:43` `#[allow]` with `// justification:` | `check:clippy-allow-audit` | PASS (TR-7 rejected by `ponytail-rev`: agent-rules.md:20 sanctions it; `envelope.rs::Spec` precedent) |
| Scenario 3 "registration alone activates no work" | no projection/runtime exists yet | by construction | PASS |
| Repository standards: rustdoc/`# Errors`, struct-centered style, imports, no plan references | — | `standards-review.md` | FAIL (FIND-7…FIND-10) |

## 4. Validated finding ledger

Full evidence, corrections, and closure proofs are in `findings-validation.md` §3.

| ID | Sources | Status | Class | Obligation | Location |
|---|---|---|---|---|---|
| FIND-TASK-001-1 | TR-1, SR-1 | CONFIRMED | REGRESSION | AC-022; AGENTS §11 | `mise.toml:256`; `.github/scripts/detect-changes.sh:64` |
| FIND-TASK-001-2 | DS-2 (+TR-8) | CONFIRMED | INCORRECT (fail-open) | REQ-090/092/094/143 | `refs/mod.rs:534-543` + four unsynchronised validators |
| FIND-TASK-001-3 | TR-3, DS-3 | CONFIRMED | INCORRECT | AC-004, REQ-093, REQ-046, INV-006 | `card/trigger.rs:15-22,43`; `card/verifier.rs:39-49` |
| FIND-TASK-001-4 | TR-2, DS-1, TR-4 (loader) | REVISED | MISSING | AC-004, AC-018, AC-021, AC-022 | `resolve.rs:66-181`; `composition.rs:104-218`; journey fixtures |
| FIND-TASK-001-5 | TR-5, SR-7, SR-8 (partial) | REVISED | INCORRECT/DRIFT | AC-021, REQ-090/103/114/116, AGENTS §16 | `vala/eval/spec.rs`, `card/drift.rs`, `service.rs`, `order.rs`, `wyrd-cli/error.rs`, `card/trigger.rs`, `vala-eval/executor.rs` |
| FIND-TASK-001-6 | TR-6, SR-6 | CONFIRMED | MISSING | REQ-120, AGENTS §12 | `vala/eval/protocol.rs`, `vala/ids.rs:206-268`, `gen_schemas.rs`, `wyrd-cli/error.rs` |
| FIND-TASK-001-7 | SR-2 | CONFIRMED | VIOLATION | agent-rules (no plan references) | `graph/composition.rs:435-436` |
| FIND-TASK-001-8 | SR-3 | REVISED | VIOLATION | AGENTS §16 | `card/drift.rs:440,459,490,505`; `resolve.rs:20-24` |
| FIND-TASK-001-9 | SR-4 | CONFIRMED | VIOLATION | AGENTS §5; agent-rules:34 | `resolve.rs:66-118,227-246` |
| FIND-TASK-001-10 | SR-5 | CONFIRMED | VIOLATION | agent-rules:9-10 | `verifier.rs`, `composition.rs`, `refs/mod.rs`, `resolve.rs`, `service.rs` |

Rejected (omitted, not softened): TR-7; TR-4 cross-surface half (TASK-008); TR-2's `/v1/eval/runs` 404 absence assertion; TR-5's `EvalRecordObservation.eval_ref` doc (TASK-002); TR-8 standalone (folded into FIND-2); SR-3's trivial matchers/utoipa impls; SR-8's `CardTileGrid.svelte` (zero callers) and wyrd-ui mock labels; SR-2's test renaming.

`SPEC_REVISION_REQUIRED`: none. Every correction stays inside approved behaviour; FIND-2 reuses the existing `WYRD_REGISTRY_400_INVALID_CARD_SPEC` refusal, so no new public contract decision is needed.

## 5. Verification limits

- Lanes recorded green by the orchestrator: `fmt`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `test:shared`, `test:cards:unit`, `test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`, `test:sql`, `py:test:unit`, `py:typecheck`, `wyrd-spec --all-features --lib` (854 passed).
- **Incomplete at review time**: `lints`, `test:wyrd`, `ts:typecheck`, `ts:test:unit`. The orchestrator run log is no longer in the tree, so these must be re-run and recorded in remediation.
- **Never run**: `test:bifrost` / `test:bifrost:integration:server` — FIND-1 makes the lane unbuildable.
- No reviewer executed builds or tests; Wave 2 verified one serde-strictness claim outside the repository without modifying any file.

## 6. Prior-finding closure

Round 1. No prior verdict or findings exist.

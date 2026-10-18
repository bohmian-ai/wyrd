# Evaluation and drift domain review

Result: **PASS**. Material proposed findings: none.

## Subject and scope

Reviewed cumulative `ef3a57b0776dd7bc451273b9176498fcca700550...df148bd287d3e9fcae861470fe9c27bd576f2eeb`, original TASK-002 and the revision-13 REQ-012/AC-007 and INV-001–005 scope identified in `subject.md`. The revision-14 draft additions are not acceptance authority. This report covers evaluation completeness, missing evidence, exact evaluation identity, and preservation of Drift/Operator behavior. It does not substitute for the independent persistence, standards, or validation reports.

Read repository rules and the reference router, applicable evaluation/drift references, the Verifier and schedule sections of `architecture/wyrd-design.md`, product doctrine, task and available historical verification evidence. Used CodeGraph before navigating runtime symbols. Expanded the map through the existing `VerifierSpec` validators, claim SQL, runner, aggregate reader/scorers, `vala_verify::blocks::DriftChecks`, fixtures, SQL/runtime tests and all three scheduled SDK journey owners.

## Coverage and obligation matrix

| Obligation | Source evidence | Proof inspected | Result |
|---|---|---|---|
| Verifier owns samples; no omission authority | `InputSlot::Samples` has only features; `VerificationBinding` has no inputs; `DriftEngine::checks` at `crates/wyrd/wyrd-server/src/verification/drift.rs:689` loops directly over every `spec.drift_checks()` | Contract refusal test and SDK redundant-input scenarios | PASS |
| Every drift requirement is accounted for | `VerifierSpec::drift_checks` enumerates `all_requirements`; existing validation disallows Window preparation steps and mixed Run/Window checks; runner supplies every scored report to existing engine | Rust, Python and TS `scheduled_binding_scores_every_requirement` each assert exact two-entry requirement outcomes, including missing error_rate | PASS |
| Missing or insufficient samples are inconclusive | `score` still returns optional report; `checks` maps absence to method-specific `DriftReport::unscored`; existing `DriftChecks::scored` maps `DriftVerdict::Inconclusive` to inconclusive; aggregation remains unchanged | Each SDK exercises latency=80 with absent error_rate and requires passed/inconclusive and overall inconclusive; existing empty-window tests remain | PASS |
| Exact pinned Verifier, subject and schedule window persist on retry/recovery | `INSERT_SCHEDULED_SQL` stores Verifier UID/version, subject and window; `CLAIM_SQL` returns those execution-row fields on every claim; removed binding lookup supplied only selectors; runner fetch/cache key is tenant plus exact Verifier UID; `ObservationWindow::new` takes `run.card_uid` and claimed window | SQL `scheduled_and_submitted_executions_freeze_their_inputs` retains exact UID/version/subject/window assertions; lease/reclaim tests remain; SDKs inspect subject, requirement provenance and Verifier name | PASS |
| Optional JSON/media/trace behavior preserved | Those enum variants and their serde default/required fields are unchanged; `InputSlot::required` retains each original boolean, only Samples now returns true | Existing contract tests retained; cumulative diff inspection | PASS |
| Drift algorithms, baseline ownership, Window submit recording and Operators unchanged | No `vala-drift`, `vala-verify` or submit production changes; `DriftEngine::score`, aggregate construction, fitted baseline lookup and result projection remain existing owners; baseline claim stays keyed by exact Verifier and requirement ID; only selector filter is removed | Existing failed/passed/empty/SPC SDK scenarios retained; runtime/Operator tests change only removed mapping arguments; historical task evidence reports focused regressions green | PASS |
| No second evaluation mechanism or unearned owner | Existing `DriftEngine`, `ObservationWindow`, `DriftChecks`, queue and runner are extended by deleting selector machinery. No new engine, baseline owner, dispatch route or algorithm | Cumulative source/diff comparison | PASS |
| Three first-class SDK surfaces exercise observable behavior | Existing Rust `scheduled_drift_alerts_operator.rs`, Python `test_scheduled_drift_alerts_operator.py`, TS `scheduled-drift-alerts-operator.test.ts` add the same two requirement cases through real SDK recording and real scheduled result retrieval | Source-inspected exact outcomes and fixtures: 120/0.01 => failed/passed/failed; 80/missing => passed/inconclusive/inconclusive | PASS |

## End-to-end reasoning

A schedule binding resolves the exact bound Verifier and subject through existing server registration and queue owners. Removing `validate_schedule` does not permit required non-samples inputs into scheduled executions: `validate_evidence_mode` already rejects mixed samples and other slots and mixed Window/Run checks. The remaining activation guard checks that samples exist. No replacement selector or default map is introduced.

The claim now builds `ExecutionInput::Scheduled { window }` solely from persisted execution fields. It does not re-resolve a mutable binding to decide which checks exist. The runner loads the exact Verifier UID from the claim; `DriftEngine::checks` iterates its requirements and `score` retains its existing subject/window filter and per-requirement baseline lookup. Missing aggregate evidence produces an unscored report rather than absence of the requirement. Existing Vala decision semantics combine both outcomes, so a passed latency requirement cannot turn missing error-rate evidence into a pass.

Infrastructure failure still returns the existing retry or terminal outcome before a judgment, preserving REQ-012's infrastructure-error exception. The diff does not change algorithms, readiness, baseline fitting, accepted execution authority, queue lease ownership or Operator dispatch. Deleting the old omitted-optional-slot journey removes an assertion of the expressly removed contract; the new SDK cases prove the replacement observable behavior rather than weakening missing-evidence handling.

## Remediation and verification limits

The latest remediation only documents the shared decoder and changes activation remediation text/generated error docs plus its HTTP assertion; it does not alter evaluation execution bodies. No evaluation defect is introduced by that round. Decoder rustdoc and public error wording closure belong to the standards/behavior reports, not an independent evaluation finding.

This agent ran no runtime tests and made no Postgres connection attempt, as directed by the orchestrator. Historical green commands in the task are inspected evidence, not independently reproduced results. Source confirms the three SDK scenarios and existing regression coverage are present; execution status must be reconciled with the orchestrator's verification report. Full journey suites and `mise run gate` were intentionally not run. No source/test files were modified.

---
id: TASK-011-R2
kind: remediation
status: review
spec: SPEC-verified-change-contract
spec_revision: 38
requirements: [REQ-153, REQ-156, INV-012, AC-034]
depends_on: [TASK-011-R1]
parent_task: TASK-011
remediates: [FIND-TASK-011-8, FIND-TASK-011-9, FIND-TASK-011-10, FIND-TASK-011-11]
---

# Close TASK-011 mixed-sample PSI and repository evidence gaps

## Authority and subject

Apply [approved specification revision 38](../../spec.md) to the [original TASK-011](../../tasks/TASK-011-conventional-psi-spc.md) and prior [TASK-011-R1](../TASK-011-r1/TASK-011-R1-production-drift-closure.md). The reviewed cumulative candidate was `338f33235f81c30dfe3a570dc26934fe7bb77048..3c6fc1880692cc29c4c1ff7fc5fcb72a850bb9d6`; [verdict.md](verdict.md) and [findings-validation.md](findings-validation.md) record the independent diagnosis. Reassess the complete task range after remediation.

## Diagnosis and correction

### FIND-TASK-011-8 — insufficient PSI feature can fail a run

REQ-153 keeps a minimum target sample of 100, and REQ-156 makes an insufficient target wholly inconclusive without scored details or feature rows. In `crates/vala/vala-drift/src/psi/mod.rs`, `score_psi_counts` assigns `Inconclusive` to a feature below 100 but scores its siblings; `DriftReport::aggregate_verdict` then prioritizes any sibling `Drift`. The server's `ObservationWindow::incomplete` in `crates/wyrd/wyrd-server/src/verification/drift.rs` counts distinct configured series per `record_id`, whereas PSI feature aggregates count physical rows. A 99-record window with a repeated configured-series row can therefore supply 100 values for one feature and 99 for another, publish a failed result, and dispatch an Operator. The current one-feature short-window fixtures cannot detect this.

Make the existing shared PSI aggregate scorer return the existing empty, unscored inconclusive report when **any** configured feature has fewer than 100 values, before PSI math or verdict aggregation. In the existing server completeness part of the same fixed query, require exactly one row for each configured series in each selected record; a duplicate makes the window incomplete. This keeps direct and server scoring aligned for valid data and prevents a duplicate from manufacturing sufficient samples. Preserve the one-statement Oracle cut, tenant/subject/series/half-open window filters, query authorization/audit, bounded folding, and result/dispatch ownership. Do not change the common verdict aggregator or Custom.

### FIND-TASK-011-9 — required Rust documentation is incomplete

`ColumnRef::collect_f64` and `collect_string` in `vala-drift/src/feature.rs` return `Result` without the required `# Errors` section. The added `psi_score::feature` helper in `psi/mod.rs` lacks rustdoc and uses `expect` without a documented panic invariant. `AGENTS.md` §16 and `architecture/agent-rules.md` require these contracts for materially changed and private test items. Add precise intent/error/panic documentation at those items and inspect the other new or materially changed Rust items in the cumulative diff for the same gap. No wrapper, suppression, or behavior change is needed.

### FIND-TASK-011-10 — qualified types in changed signatures

`wyrd-server/src/verification/drift.rs::fold_spc`, `wyrd-testing/src/server.rs::WyrdTestServer::verification_fixture`, and `wyrd-sdk-ts/native-testing/src/lib.rs::{verification_fixture,reason}` use fully qualified types in new or changed signatures. `architecture/agent-rules.md` requires module-level imports and bare names in signatures. Use the existing `FeatureName` import and add the necessary top-level imports for the other types. Leave unrelated existing signatures alone.

### FIND-TASK-011-11 — contract validation tests lack execution evidence

The cumulative diff changes `wyrd-spec/src/card/mod.rs` tests `rejects_spc_sample_size_below_two` and `rejects_retired_and_unknown_spc_profile_fields`. TASK-011 claims the `wyrd-spec` suite passed but records neither its owning `test:wyrd` lane nor focused executions. Lints, codegen, and served OpenAPI do not execute these test bodies. Confirm the real test names, run each exact `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=...)'` command and the owning `mise run test:wyrd` lane, then record actual commands and results in TASK-011. Fix any red test without weakening a gate.

## Acceptance and proof

1. A direct two-feature PSI input with one drifting feature at 100 counts and another at 99 yields one empty inconclusive report: no scored details or feature rows. Complete multi-feature PSI still scores normally.
2. A server window with 99 complete records and one repeated configured-series row is unscored, persists `completed/inconclusive` with null details and zero feature rows, and dispatches no Operator. A focused SQL/fold proof exercises the duplicate condition; the authenticated single-query path remains intact.
3. The changed Rust items meet the required rustdoc/error/panic and bare-import signature rules. `mise run fmt` and `mise run lints` pass.
4. The changed `wyrd-spec` tests execute and pass through their owning lane and exact focused commands, with reproducible commands and results in TASK-011. Every newly named test has its exact focused command and result.
5. Record passing focused checks and the relevant Vala, server, first-class SDK journey, contract, format/lint, codegen/OpenAPI, docs, and `git diff --check` gates after the final code change.

## Preserved behavior and non-goals

Retain revision 38's selected-row contract, conventional PSI bins and smoothing, NIST X-bar/S chart, whole-run SPC incompleteness, fitted-version refusal, historical reads, and the existing test-only SDK fixtures. Do not add a PSI missing bin, another chart, a new observation format, a product route, a migration, a second query cut, or an alternate scorer. Use the existing owners and the smallest proof that catches the observed defect.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. Two-feature PSI, drifting feature at 100 and sibling at 99 → one empty inconclusive report; complete multi-feature PSI scores | `vala-drift/src/psi/mod.rs::score_psi_counts` checks every feature's sample before PSI math and returns `DriftReport::unscored(Psi)` | `psi::psi_score::psi_one_insufficient_feature_unscores_the_report` (both at 100 → two rows, Drift; 99 → unscored); `psi_target_too_small_is_unscored`; `baseline::aggregate_inputs::psi_counts_match_raw_scoring_and_small_windows_are_inconclusive` | PASS |
| 2. 99 complete records plus one repeated series row → unscored, `completed/inconclusive`, null details, no feature rows, no dispatch; single-query path intact | `wyrd-server/src/verification/drift.rs`: `ObservationWindow::incomplete` adds `COUNT(*) > configured`; `DistributionFold::finish` maps any empty report (PSI or SPC) to `None`, which publishes `Drift(None)` | `verification::drift::tests::completeness_flags_omitted_and_invalid_features_only` (repeated row → 1 incomplete); `psi_repeated_series_row_cannot_manufacture_a_sample` (99 + repeat → `None`, 100 → Drift, through `psi_statement`); `one_statement_decides_completeness_and_scores_from_one_cut`; `test:bifrost:integration:server`; journeys assert sparse PSI and SPC `assert_unscored` | PASS |
| 3. Rustdoc `# Errors`/`# Panics` and bare imported types in changed signatures | `feature.rs` `collect_f64`/`collect_string`; `psi_score` helpers incl. `feature`; `spc_fit::fit`; `baseline` fixtures; `lib.rs` surface test; `DriftValidationError::details`; `fold_spc`, `WyrdTestServer::verification_fixture`, TS `verification_fixture`/`reason`, `psi::target_column`, test `decide`, Rust journey `evidence` | `mise run fmt`; `mise run lints`; an audit of every function touching changed lines in `338f3323..HEAD` reports no gap | PASS |
| 4. `wyrd-spec` tests execute | Tests unchanged; commands recorded in TASK-011 | `mise run test:wyrd` — 2142 passed, both tests in the log; each exact `-p wyrd-spec --lib` command — 1 passed | PASS |
| 5. Gates after the final code change | TASK-011 evidence updated | `test:vala` 1280; `test:wyrd` 2142; `test:bifrost:integration:server` 85; journeys drift 2, python 40, typescript 20; `py:lints`; `ts:typecheck`; `test:principals:integration`; `codegen:check`; `docs:check`; `git diff --check` — all exit 0 | PASS |

Exact focused commands for the named tests, and the journey commands, are listed in [TASK-011](../../tasks/TASK-011-conventional-psi-spc.md#implementation-evidence).

Diagnosis of the one test change: `one_statement_decides_completeness_and_scores_from_one_cut` previously expected a scored PSI report from a 4-record window. The per-feature NaN rows made it look scored. Under REQ-156 that window is insufficient and now unscored, so the fixture uses 100 records. The seam and all-null assertions are unchanged.

Non-goals stayed excluded: no PSI missing bin, no new chart, observation format, route, migration, second query cut, or alternate scorer. The common verdict aggregator and Custom are unchanged.

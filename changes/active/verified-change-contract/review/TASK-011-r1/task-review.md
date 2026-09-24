# TASK-011 implementation acceptance review

**Subject:** `338f33235f81c30dfe3a570dc26934fe7bb77048..6e3bac0370a31b19d767ac4d20830d430c3f2ff5` on `vcc/task-005`. Approved authority: `changes/active/verified-change-contract/spec.md`, revision 37; original task: `tasks/TASK-011-conventional-psi-spc.md`. Reviewed the cumulative diff and current source. **Result: FAIL.**

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-153: exhaustive frozen PSI bins, categorical `other`, numeric outer edges, scored and reported proportions | `vala-drift/src/psi/mod.rs` fitting, `FittedPsiFeature::count`, `score_psi_counts`; server `ObservationWindow::psi_numeric` and `psi_categorical` | PSI unit fixtures, SQL aggregate fixture, three SDK journeys; reported Vala/server/codegen lanes passed | PASS |
| REQ-154: fixed authored subgroup size, complete ordered groups, baseline minimum and rejection of remainder | `wyrd-spec/src/card/drift.rs::SpcProfile`, `spc/mod.rs::fit_spc_baseline_until`, server `ObservationWindow::spc` | SPC fit/SQL tests, SDK journeys and contract/codegen checks | PASS |
| REQ-155: NIST X-bar/S limits, strict signals, score zero threshold, typed evidence | `spc/control_limits.rs`, `SpcScorer`, `report.rs`, server `fold_spc` | Published-constant and independent fixture tests; Rust/Python/TypeScript evidence assertions | PASS |
| REQ-156: reject null/non-finite baseline values | `psi/mod.rs::fit_numeric/fit_categorical`; `spc/mod.rs::fit_spc_baseline_until` | Named null/non-finite fitting tests | PASS |
| REQ-156: selected target with null, missing or non-finite feature is wholly unscored; direct and server paths agree | Server `ObservationWindow::incomplete` selects configured series by name, including null-valued rows. Direct `feature.rs::target_complete` selects only rows where at least one value is `Some`, so a null-only row is skipped. | Existing direct fixtures put another valid feature on the same row; server fixture confirms null selection. No null-only single-feature equivalence test. | **FAIL** |
| REQ-157: refuse old fit format, preserve historical reads, use new version and fit | `baseline/mod.rs::FITTED_FORMAT = 2`, server `DriftEngine::fitted`, Rust journey | Rust journey checks legacy refusal and historical read | PASS |
| INV-012: preserve surrounding query, run, dispatch and Custom behavior | Diff retains shared query service, result writer and runtime, and Custom scorer | Rust/Python/TypeScript journeys and server integration lane reported green | PASS |
| AC-034: direct/server edge fixtures and SDK journeys | New Vala, server SQL and SDK test changes | Reported exact focused tests and lane results; gap described in finding below | **FAIL** |
| Non-goals: no chart family, rules, inferred group size, imputation or migration | Removed WECO module; profile contains only `sample_size`; old format refused | Diff and tests | PASS |
| Documentation and generated contracts | Updated drift logic/design and generated schemas | Reported `docs:check`, `codegen:check`, format/lints, `git diff --check` passed | PASS |

## Proposed finding

### TASK-REV-011-1 — INCORRECT: direct scoring skips a null-only selected row

**Obligation:** REQ-156 requires a selected observation containing a configured PSI/SPC feature with a null value to make the run inconclusive, with no scored details or feature rows, and requires direct and server checks to agree. TASK-011 requires the same input-completeness behavior.

**Location and evidence:** `crates/vala/vala-drift/src/feature.rs`, `TargetColumn::carried` and `target_complete` (around lines 104–145); called by `score_psi` at `psi/mod.rs:195` and `score_spc` at `spc/mod.rs:200`. `carried` returns false for `None`. Thus a batch with one configured feature and 100 valid rows plus one null row passes `target_complete`; `numeric_values` then drops the null, PSI scores 100 values and SPC can form complete subgroups from the remaining values. In contrast, `ObservationWindow::incomplete` in `wyrd-server/src/verification/drift.rs:160–186` selects the same configured `series` regardless of its null value and marks it incomplete. The direct tests only check null rows when another feature on that row is valid; they explicitly classify all-null feature rows as unrelated.

**Observable consequence:** A raw-batch caller can receive a scored `NoDrift` or `Drift` report after a selected null observation is silently removed, while the server returns an unscored inconclusive result for that observation. For SPC, removal can also shift later subgroup boundaries.

**Required correction:** Resolve selected-row membership at the shared direct-scoring boundary without treating a null in an included configured feature as an unrelated row. Preserve exclusion of genuinely unrelated observations using the existing row/series information where it exists, and ensure a selected null row returns `DriftReport::unscored` in both PSI and SPC. Add one focused direct regression per method using a single configured feature and enough otherwise valid rows to score, with a null row; compare with the existing server completeness fixture and keep the three SDK journeys green. If the raw `RecordBatch` contract cannot distinguish an omitted series from a present-null series, make that distinction explicit at its existing input boundary rather than silently dropping the row.

## Verification limits

The evidence table records passing focused tests and Vala, spec, server, SDK, codegen and docs lanes, but their direct completeness fixtures do not exercise a null-only selected observation. Python and TypeScript do not test stored legacy-baseline refusal or cron-trigger dispatch; the Rust journey covers those server-owned paths. JSON observation APIs cannot carry NaN/infinity; Vala and server SQL fixtures cover them.

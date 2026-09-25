---
id: TASK-011
kind: remediation
status: review
spec: SPEC-verified-change-contract
spec_revision: 37
requirements: [REQ-110, REQ-153, REQ-154, REQ-155, REQ-156, REQ-157, INV-012, AC-034]
depends_on: [TASK-005]
parent_task: TASK-005
---

# Bring PSI and SPC onto conventional definitions

## Authority and outcome

The [revision 37 specification](../spec.md) was explicitly approved by the
user on 2026-09-24, so this task is ready for implementation. TASK-005 produced a
working Drift path under approved revision 36. Its categorical PSI omits
unknown-category mass from the scored bins, and its SPC uses adaptive chunks,
partial groups, a custom rule string, and X-bar limits without `sqrt(n)`.
Correct these statistical semantics in the shared Vala engine and the server
without adding another monitor or client-side scorer.

## Required behavior

1. **PSI:** Fit frozen numeric bins and categorical labels plus one reserved
   `other` bin. Count every selected target value in exactly one bin, including
   unseen categories; compare complete baseline and target proportions with
   the existing zero-bin smoothing and formula. Keep current numeric binning,
   minimum target sample, and authored threshold choices. Expose the bin
   counts/proportions needed to explain a PSI result. Do not claim PSI is a
   significance test.
2. **Input completeness:** Fail a PSI/SPC baseline fit if any required value is
   null or non-finite. For a subject/window record carrying any configured
   feature, a missing configured feature or invalid value makes the target run
   inconclusive without scored details or feature rows. Unrelated records do
   not enter that comparison. Apply this to direct scoring and the server's
   fixed-query aggregate path; preserve Custom's current behavior.
3. **SPC profile and grouping:** Require an authored fixed subgroup size of at
   least two. Remove `weco_rule` and `alert_threshold` from the new public SPC
   profile. Fit at least 20 complete baseline subgroups and reject leftover
   rows. Score only complete target subgroups in the existing deterministic
   observation order; an empty or partial target is inconclusive. Document the
   author's responsibility for stable, process-ordered rational subgroups.
4. **SPC math and report:** Fit NIST's two-sided, three-sigma X-bar/S limits
   from subgroup means and sample standard deviations, including the
   `sqrt(n)` factor. Signal when either chart is strictly outside its limits.
   Persist subgroup size/count and each chart's center, limits, and signal
   count in typed `DriftReport` evidence. Retain the common result and feature
   rows; the feature's scalar score is total chart signals with threshold zero.
5. **Version boundary:** Register corrected behavior under new immutable
   Verifier versions and refit their baselines. Preserve historical report
   reads. Reject new scoring of versions lacking a revision-37 fitted profile
   with a visible error; do not silently reinterpret them, add a second legacy
   scorer, or rewrite stored results.

## Owners and constraints

- `wyrd-spec` owns the typed SPC profile and schema; `vala-drift` owns the PSI
  and SPC fit/score math, fitted state, and report types. `wyrd-server` owns
  fixed audited observation queries, fit readiness, completeness checks,
  result mapping, and version refusal. The Rust, Python, and TypeScript SDKs
  project the same wire contract; they do not score or subgroup data.
- Align `architecture/logic/drift.md`, the active design/doctrine where they
  describe SPC, generated schemas/stubs, and first-class SDK examples with
  the approved revision. Remove revision-36 assertions that unknown PSI
  categories do not form a scored bin or that SPC preserves adaptive/partial
  chunks and custom rules.
- Preserve exact tenant, subject, series, and half-open window selection,
  query-service authorization/audit, bounded fit resources, common runtime
  permits/leases/shutdown, result publication, and Operator dispatch.
- No additional chart families, configurable Western Electric rules,
  inferred subgroup size, missing-value imputation, new observation format,
  or automatic baseline migration.

## Acceptance and verification

- Unit fixtures compare PSI with a complete categorical union and numeric
  bins, including unseen categories, zero bins, and threshold boundaries.
  Independent NIST calculations pin both X-bar/S limits and signals for
  multiple subgroup sizes, especially the prior missing `sqrt(n)` case.
- Direct Vala and server runs agree for the same valid input. Baseline and
  target fixtures prove null, NaN/infinity, omitted feature, empty/short data,
  and partial subgroup handling without dropped rows. A malformed baseline
  fails visibly; invalid target data is inconclusive and never dispatches.
- Real Rust, Python, and TypeScript SDK-to-server journeys prove new SPC
  report evidence, a failed scheduled result and its Operator dispatch,
  historical report readability, and refusal of legacy versions. Existing
  TASK-005 journeys and tenant/window/audit checks stay green.
- Run `mise run fmt`, `mise run lints`, `mise run test:vala`, the relevant
  `test:bifrost` lanes, `mise run codegen:check`, and the exact focused command
  for every named test. Record commands and results in this task. Run
  `git diff --check` before submitting the committed candidate for review.

## Statistical references

- [NIST X-bar/S chart formulas](https://itl.nist.gov/div898/handbook/pmc/section3/pmc321.htm)
- [NIST rational subgroup guidance](https://www.itl.nist.gov/div898/handbook/glossary.htm)
- [ASQ control-chart baseline guidance](https://asq.org/quality-resources/control-chart)

## Implementation evidence

Candidate range `338f3323..HEAD`: the original implementation
`338f3323..6e3bac03` plus the [TASK-011-R1 remediation](../review/TASK-011-r1/TASK-011-R1-production-drift-closure.md)
and the [TASK-011-R2 remediation](../review/TASK-011-r2/TASK-011-R2-production-drift-closure.md)
under approved revision 38.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exhaustive PSI bins, reserved `other` bin, bin evidence | `vala-drift` `psi/` fit and score, `report.rs` `Psi` evidence; server `psi_categorical` other bin | `psi_unseen_categories_land_in_the_other_bin`, `psi_zero_bins_and_other_count_toward_the_threshold`, `psi_threshold_boundary_is_strict`, `categorical_fit_sorts_bins_and_reserves_other`, `psi_categorical_unknowns_land_in_the_other_bin`; server `psi_categorical_sql_escapes_labels_and_counts_unknowns_as_other` | PASS |
| Baseline null/non-finite fails the fit | `psi` and `spc` fit validation, `DriftFitError::{NullValuesInColumn, NonFiniteValuesInColumn}` | `null_baseline_values_fail_the_fit`, `numeric_fit_rejects_non_finite_values`, `null_and_non_finite_baseline_values_fail` | PASS |
| Target completeness (rev 38): every row of a preselected direct batch participates; omitted/null/non-finite → wholly unscored; server selects by series and requires exactly one row per configured series per record; Custom unchanged | `feature.rs::target_complete`; server `ObservationWindow::incomplete` (`COUNT(*) > configured` marks a repeated series) inside the one PSI/SPC statement | `psi_incomplete_targets_are_unscored`, `psi_selected_null_only_row_makes_the_target_unscored`, `incomplete_targets_are_unscored`, server `completeness_flags_omitted_and_invalid_features_only` (repeated row case), `one_statement_decides_completeness_and_scores_from_one_cut`; Rust/Python/TS "gappy" journey cases | PASS |
| Insufficient PSI target (any feature < 100 values) → one empty unscored report, published as `completed/inconclusive` with null details, no feature rows, no dispatch; a repeated row cannot manufacture a sample | `vala-drift` `score_psi_counts` returns `DriftReport::unscored` before PSI math; server `DistributionFold::finish` maps any empty report to `None` | `psi_one_insufficient_feature_unscores_the_report` (100 vs 99, drifting sibling), `psi_target_too_small_is_unscored`, `psi_counts_match_raw_scoring_and_small_windows_are_inconclusive`; server `psi_repeated_series_row_cannot_manufacture_a_sample` (99 records + repeat → `None`; 100 records → Drift); Rust/Python/TS sparse journey cases assert PSI unscored | PASS |
| Completeness and all feature aggregates from one Oracle cut | server `ObservationWindow::{psi_statement, spc_statement}` (`UNION ALL` ordered by part, `k`), `DistributionFold` | `one_statement_decides_completeness_and_scores_from_one_cut`; drift journeys and server integration lane | PASS |
| `SpcProfile { sample_size ≥ 2 }` only; `weco_rule`/`alert_threshold` rejected | `wyrd-spec` `drift.rs`, schemas, stubs | `mise run test:wyrd` (runs `rejects_spc_sample_size_below_two`, `rejects_retired_and_unknown_spc_profile_fields`) and their exact commands; `codegen:check`; served `/openapi.json` via `test:principals:integration`; journeys refuse a `weco_rule` profile at registration | PASS |
| ≥ 20 complete baseline subgroups; leftover rows rejected | `spc::fit_spc_baseline_until`, `MIN_BASELINE_SUBGROUPS` | `twenty_complete_subgroups_fit`, `short_or_ragged_baselines_fail_visibly` | PASS |
| Target ordered by `created_at`, `record_id`; any empty or partial feature leaves the whole SPC run unscored | server `ObservationWindow::spc`, `fold_spc`, `DistributionFold::finish`; `SpcScorer::finish` | `spc_sql_orders_subgroups_by_creation_then_record`, `spc_fold_feeds_subgroups_and_refuses_invalid_ones`, `empty_and_partial_targets_are_inconclusive`, `a_signal_beside_an_incomplete_feature_is_unscored`, `spc_subgroups_match_raw_scoring_and_partial_targets_are_inconclusive`, server `spc_signal_beside_a_partial_feature_is_unscored`; journey calm-partial and sparse cases assert no details and no feature rows | PASS |
| NIST X-bar/S limits incl. `sqrt(n)`, strict signals, score = signals, threshold 0, typed evidence | `spc/control_limits.rs`, `SpcEvidence`, `SpcChartEvidence` | `x_bar_s_fixture_includes_sqrt_n`, `limits_reproduce_published_a3_b3_b4_constants`, `equality_at_the_limit_does_not_signal`, `equality_at_the_limits_is_in_control`, `each_chart_signals_independently`, `in_control_target_passes_with_evidence`, `rejects_degenerate_fits`, `scorer_rejects_invalid_pushes`, `non_numeric_target_errors`; journeys assert center/limits/signals/threshold | PASS |
| Failed scheduled result dispatches its Operator | Unchanged runtime; test controls `WyrdTestServer::verification_fixture` projected as Python/TS `make_binding_due`, `verification_runs` | Rust `drift_methods_fit_score_persist_and_dispatch`; Python `test_parquet_baselines_fit_and_score_drift_server_side`; TS "scores each method's edge cases": due occurrence runs once, fails, one dispatch | PASS |
| Version boundary: stored legacy fit refused, history readable, no migration | server `fitted()` format check, `BASELINE_LEGACY`; fixture `retire_fitted_format` projected to Python/TS | Rust `drift_method_edges_score_through_oracle`; Python and TS: retired SPC fit → `errored`/`baseline_legacy`, no result, earlier SPC result reads unchanged | PASS |
| Docs and schemas aligned | `architecture/logic/drift.md`, `architecture/verifier/drift.md` (change packet), `architecture/wyrd-design.md`, `docs/public/llms-full.txt` | `docs:check`, `codegen:check` | PASS |
| Changed Rust items carry rustdoc with `# Errors`/`# Panics`; changed signatures use bare imported types | `feature.rs` `collect_f64`/`collect_string`, `psi_score` helpers, `spc_fit::fit`, `baseline` fixtures, `DriftValidationError::details`; `fold_spc`, `WyrdTestServer::verification_fixture`, TS `verification_fixture`/`reason`, `target_column`, test `decide`/`evidence` | `mise run fmt`; `mise run lints`; audit of every function touching changed lines in `338f3323..HEAD` | PASS |
| TASK-005 journeys stay green | — | `test:bifrost:journey:drift`, `:python`, `:typescript`, `test:bifrost:integration:server` | PASS |

Commands (all exit 0 on the final candidate):

- `mise run fmt`; `mise run lints`; `mise run py:format`; `mise run py:lints`; `mise run ts:typecheck`
- `mise run test:vala` — 1280 passed
- `mise run test:wyrd` — 2142 passed (includes `wyrd-spec`)
- `mise run test:bifrost:journey:drift` — 2 passed
- `mise run test:bifrost:journey:python` — 40 passed
- `mise run test:bifrost:journey:typescript` — 20 passed
- `mise run test:bifrost:integration:server` — 85 passed (includes `pg_verification_runtime`)
- `mise run test:principals:integration` — passed (includes `pg_openapi_contract`, 16 passed)
- `mise run codegen:check`; `mise run docs:check`
- `git diff --check 338f3323..HEAD`
- Focused, one exact command per named test:
  - `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::drift_validation_tests::rejects_spc_sample_size_below_two)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::drift_validation_tests::rejects_retired_and_unknown_spc_profile_fields)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_target_too_small_is_unscored)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_one_insufficient_feature_unscores_the_report)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=baseline::aggregate_inputs::psi_counts_match_raw_scoring_and_small_windows_are_inconclusive)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::psi_repeated_series_row_cannot_manufacture_a_sample)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=baseline::aggregate_inputs::psi_categorical_unknowns_land_in_the_other_bin)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=baseline::aggregate_inputs::spc_subgroups_match_raw_scoring_and_partial_targets_are_inconclusive)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_fit::categorical_fit_sorts_bins_and_reserves_other)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_fit::null_baseline_values_fail_the_fit)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_fit::numeric_fit_rejects_non_finite_values)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_incomplete_targets_are_unscored)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_selected_null_only_row_makes_the_target_unscored)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_threshold_boundary_is_strict)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_unseen_categories_land_in_the_other_bin)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=psi::psi_score::psi_zero_bins_and_other_count_toward_the_threshold)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::control_limits::tests::equality_at_the_limit_does_not_signal)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::control_limits::tests::limits_reproduce_published_a3_b3_b4_constants)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::control_limits::tests::rejects_degenerate_fits)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::control_limits::tests::x_bar_s_fixture_includes_sqrt_n)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_fit::null_and_non_finite_baseline_values_fail)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_fit::short_or_ragged_baselines_fail_visibly)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_fit::twenty_complete_subgroups_fit)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::a_signal_beside_an_incomplete_feature_is_unscored)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::each_chart_signals_independently)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::empty_and_partial_targets_are_inconclusive)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::equality_at_the_limits_is_in_control)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::in_control_target_passes_with_evidence)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::incomplete_targets_are_unscored)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::non_numeric_target_errors)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=spc::spc_score::scorer_rejects_invalid_pushes)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::psi_numeric_sql_bins_on_fitted_edges_inside_the_window)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::psi_categorical_sql_escapes_labels_and_counts_unknowns_as_other)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::spc_sql_orders_subgroups_by_creation_then_record)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::spc_fold_feeds_subgroups_and_refuses_invalid_ones)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::completeness_flags_omitted_and_invalid_features_only)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::custom_sql_is_one_row_and_requires_a_complete_window)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::one_statement_decides_completeness_and_scores_from_one_cut)'` — 1 passed
  - `mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --lib -E 'test(=verification::drift::tests::spc_signal_beside_a_partial_feature_is_unscored)'` — 1 passed
  - `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E 'test(=drift_methods_fit_score_persist_and_dispatch)'"` — 1 passed
  - `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E 'test(=drift_method_edges_score_through_oracle)'"` — 1 passed
  - `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_drift_journey.py::test_parquet_baselines_fit_and_score_drift_server_side tests/integration/test_drift_journey.py::test_drift_method_edges_score_through_oracle'` — 2 passed
  - `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/drift-verification.test.ts -t "scores each method"'` — 1 passed

Material limits:

- NaN and ±inf target values are proven in unit and server SQL tests only;
  the SDKs' JSON observation path cannot carry non-finite numbers.
- The single-cut proof is structural: one statement per PSI/SPC run, whose
  fold over an observation added at the former read seam is unscored. No
  test injects an ingest mid-query, because one Oracle query pins one cut.
- Non-goals stayed excluded: no extra chart families, no configurable rules,
  no inferred subgroup size, no imputation, no PSI missing bin, no migration,
  no new public API or observation format.

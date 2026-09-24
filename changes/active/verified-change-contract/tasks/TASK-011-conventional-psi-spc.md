---
id: TASK-011
kind: remediation
status: ready
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

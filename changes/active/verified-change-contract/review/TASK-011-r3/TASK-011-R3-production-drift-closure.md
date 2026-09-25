---
id: TASK-011-R3
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 38
requirements: [REQ-156, AC-034]
depends_on: [TASK-011-R2]
parent_task: TASK-011
remediates: [FIND-TASK-011-12, FIND-TASK-011-13]
---

# Close direct null-typed scoring and remediation status gaps

## Authority and subject

Apply [approved specification revision 38](../../spec.md) to the [original TASK-011](../../tasks/TASK-011-conventional-psi-spc.md), [TASK-011-R1](../TASK-011-r1/TASK-011-R1-production-drift-closure.md), and [TASK-011-R2](../TASK-011-r2/TASK-011-R2-production-drift-closure.md). The reviewed cumulative candidate was `338f33235f81c30dfe3a570dc26934fe7bb77048..c5c7a76cc1f45f5bdfad20de35a957b9f2f9ce57`. The [r3 verdict](verdict.md) and [validated ledger](findings-validation.md) contain the independent evidence. Review the complete cumulative candidate again after remediation.

## Diagnosis and correction

### FIND-TASK-011-12 — null-typed direct target returns an error

REQ-156 requires a selected PSI/SPC target with a null configured value to make the whole report inconclusive without scored details or feature rows. Direct `score_psi` and `score_spc` resolve and type-check target columns before `target_complete`. A present Arrow `DataType::Null` column therefore becomes `FeatureTypeMismatch` for fitted numeric, categorical, and SPC features, although every selected value is null. The equivalent server observation is inconclusive. Existing direct tests use typed nullable Float64/Utf8 arrays and miss this representation.

In the existing PSI/SPC target-column resolution, classify only a present `DataType::Null` feature as incomplete, then reuse the current whole-report `target_complete` guard and `DriftReport::unscored` result. Preserve terminal errors for non-null wrong-typed columns, the already-selected row contract, numeric/categorical bins, SPC charts, and the server path. There is no need for a new type, scorer, observation format, or server change.

### FIND-TASK-011-13 — implemented remediation still marked ready

The R1 and R2 remediation files both have `status: ready` despite their completed implementation evidence. `AGENTS.md` §14 and `architecture/references/languages/spec-driven-development.md` use `review` for implemented tasks submitted for acceptance. Set only those two front-matter values to `review`, preserving their historical diagnoses and evidence. No runtime test is needed.

## Acceptance and proof

1. Direct PSI numeric, PSI categorical, and SPC targets with a selected all-null Arrow column return one empty unscored inconclusive report. Keep existing typed-null and non-null wrong-type assertions. Give each new named test its exact focused `mise exec -- cargo nextest run` command and passing result.
2. R1 and R2 remediation headers both read `status: review`; inspect the front matter and run `git diff --check`.
3. After the last code change, record passing `mise run fmt`, `mise run lints`, `mise run test:vala`, and the relevant Bifrost server and Rust/Python/TypeScript Drift journey lanes. Reuse existing recorded contract/OpenAPI/codegen/docs evidence where unaffected and rerun a gate only for a concrete remaining risk or required repository rule.

## Preserved behavior and non-goals

Keep revision 38's selected-row completeness policy, exhaustive PSI bins, NIST X-bar/S math, whole-run short-sample handling, one Oracle query cut, audited tenant-scoped read, fitted-version boundary, result/dispatch behavior, and prior historical evidence. Do not add a PSI missing bin, imputation, new chart, migration, public route, or alternate scorer.

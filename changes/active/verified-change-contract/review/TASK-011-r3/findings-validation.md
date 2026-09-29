# TASK-011 r3 independent finding validation

**Subject:** repository `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; immutable cumulative range `338f33235f81c30dfe3a570dc26934fe7bb77048..c5c7a76cc1f45f5bdfad20de35a957b9f2f9ce57`; approved specification revision 38; original TASK-011; r1/r2 verdicts, validated ledgers and remediation tasks. `.codegraph/` is absent. This is a source and recorded-evidence review; I did not rerun implementation tests or edit reviewed source. HEAD remained the candidate.

## Wave 1 proposal decisions

| Proposal | Decision | Independent validation and smallest safe boundary |
|---|---|---|
| STAT-R3-1 | **CONFIRMED** | `score_drift` routes direct PSI and SPC to `score_psi` and `score_spc`. Both resolve and type-check every target feature before calling `target_complete`. A present `DataType::Null` column is neither numeric nor categorical, so the direct scorer returns `FeatureTypeMismatch` even though every selected value is null. Arrow can represent a selected null-only feature this way; REQ-156 requires it to be unscored. Existing fixtures use typed nullable Float64/Utf8 arrays and miss this case. Treat only `DataType::Null` as incomplete in the existing column-resolution branches, using `TargetColumn::Absent` so the existing whole-report completeness guard handles it. Keep non-null wrong types as errors, fitted bins, SPC charts, and server behavior unchanged. |
| S1 | **CONFIRMED** | Both implemented remediation tasks still have `status: ready` in their front matter, although each contains passing implementation evidence. `architecture/references/languages/spec-driven-development.md` defines `ready` as executable and `review` as the submitted implementation state; AGENTS.md §14 adopts that workflow. Set the two status fields to `review`. Their diagnoses and evidence remain historical. No runtime code or test is needed. |

The task implementation report's `PASS` is contradicted by the reachable direct Arrow null-type path; the repository standards report independently establishes the stale remediation states. Query/persistence and security reports propose no findings. The two proposals are distinct and no proposal was rejected.

## Final deduplicated ledger

### FIND-TASK-011-12 — CONFIRMED; INCORRECT

- **Source:** STAT-R3-1. **Obligation:** REQ-156 and AC-034 require every selected null-valued direct PSI/SPC target to yield one wholly unscored inconclusive report, consistent with the server path.
- **Location and evidence:** `crates/vala/vala-drift/src/psi/mod.rs::score_psi` calls `target_column` before `target_complete`; `target_column` rejects a present `DataType::Null` as `FeatureTypeMismatch`. `crates/vala/vala-drift/src/spc/mod.rs::score_spc` similarly rejects that type before `target_complete`. The public `baseline/mod.rs::score_drift` calls both scorers. `feature.rs::target_complete` already makes `TargetColumn::Absent` incomplete for any selected row. Existing typed-null fixtures do not exercise an Arrow `NullArray` field.
- **Consequence:** A direct caller with an all-null selected feature receives a terminal error, while an equivalent server observation is inconclusive with no feature rows.
- **Correction:** In the existing PSI/SPC target-column resolution, map a present `DataType::Null` feature to the existing incomplete-column state before the wrong-type branch. Reuse the current whole-report `target_complete` result. Preserve errors for non-null wrong types, all fitted-bin and chart logic, and the server path. No new type, option, format, or scorer is needed.
- **Focused proof:** Direct PSI fixtures with all-null numeric and categorical Arrow columns, and a direct SPC fixture with an all-null numeric Arrow column, return `DriftReport::unscored` with no feature rows. Keep typed-null and non-null wrong-type assertions; run exact named Vala tests and `mise run test:vala`.

### FIND-TASK-011-13 — CONFIRMED; VIOLATION

- **Source:** S1. **Obligation:** AGENTS.md §14 and `architecture/references/languages/spec-driven-development.md` require implemented tasks submitted for review to carry `status: review`.
- **Location and evidence:** `changes/active/verified-change-contract/review/TASK-011-r1/TASK-011-R1-production-drift-closure.md:4` and `changes/active/verified-change-contract/review/TASK-011-r2/TASK-011-R2-production-drift-closure.md:4` both say `status: ready`, despite their completed implementation-evidence sections; the original TASK-011 says `status: review`.
- **Consequence:** The active packet presents completed remediation as work still ready for implementation, confusing subsequent task selection and acceptance tracking.
- **Correction and proof:** Change only those two metadata values to `review`; inspect both task headers and run `git diff --check`. No runtime test or new workflow mechanism is warranted.

## Prior findings and recommendation

FIND-TASK-011-1 through -7 remain closed by the selected-row contract, whole-report SPC incompleteness, one Oracle query cut, OpenAPI and exact-test evidence, original task status, and all three scheduled/legacy SDK journeys. FIND-TASK-011-8 through -11 remain closed by the PSI all-feature sample guard, server duplicate-series refusal, Rust documentation/import corrections, and recorded `wyrd-spec` executions. The new null-type case is separate from the typed-null case addressed by FIND-TASK-011-1.

**Recommended verdict: FIX_REQUIRED.** Both corrections fit approved revision 38 and existing owners. No new product, public API, architecture, security, concurrency, or persistent-data decision is needed. The recorded green gates do not cover the null-typed direct target; task metadata is checked by inspection.

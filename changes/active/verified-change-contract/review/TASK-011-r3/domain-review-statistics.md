# TASK-011 r3 statistics domain review

**Subject:** `338f33235f81c30dfe3a570dc26934fe7bb77048..c5c7a76cc1f45f5bdfad20de35a957b9f2f9ce57`. **Result: FAIL.** The candidate was unchanged during this review.

## Boundary, authority, and source coverage

I reviewed approved spec revision 38 (REQ-153–REQ-157, AC-034), original TASK-011, both prior verdicts and validated ledgers, R1/R2 remediation tasks, the cumulative diff, `architecture/references/domain/drift-monitoring.md`, the changed Vala PSI/SPC/feature/baseline/report modules, and the server's `ObservationWindow`/`DistributionFold` SQL and fold path. I read the direct, aggregate, server SQL, and journey evidence recorded in TASK-011. The implemented X-bar/S equations agree with [NIST's formula](https://itl.nist.gov/div898/handbook/pmc/section3/pmc321.htm), including `c4`, `sqrt(n)`, and the S-chart limits.

The earlier statistics findings are closed in source: every row of an already selected, correctly typed direct batch participates; any incomplete SPC feature or PSI feature below 100 unscores the whole report. The server marks repeated configured-series rows incomplete and obtains completeness and every aggregate in one query. PSI bins are exhaustive with a scored `other` category and unchanged smoothing; SPC fits fixed complete subgroups and signals strictly outside frozen limits. Typed report evidence and legacy fit refusal remain. No statistical method or chart was added beyond the approved task.

## Material proposed finding

### STAT-R3-1 — INCORRECT: a selected all-null Arrow column becomes a terminal type error

**Violated obligation:** REQ-156 requires a selected target whose configured feature value is null to produce one wholly unscored, inconclusive PSI/SPC report, with no feature rows. AC-034 requires direct null-only selected-row proof and direct/server agreement.

**Exact location and evidence:** `crates/vala/vala-drift/src/psi/mod.rs:187–228` resolves and type-checks every target column before `target_complete`; `target_column` treats `DataType::Null` as `FeatureTypeMismatch` for either fitted numeric or categorical bins. `crates/vala/vala-drift/src/spc/mod.rs:184–202` likewise rejects a present `DataType::Null` column through `ColumnRef::is_numeric` before its completeness check. An Arrow `RecordBatch` can represent an entirely null configured feature with a `NullArray`/`DataType::Null` field. `score_drift` dispatches directly to these public scorers (`baseline/mod.rs:136–163`), so that valid selected batch returns `Err(FeatureTypeMismatch)` rather than `DriftReport::unscored`; the server checks null values as incomplete in `ObservationWindow::incomplete` (`drift.rs:174–200`). Current selected-null fixtures use typed `Float64` and `Utf8` arrays with null slots, not a null-typed column.

**Observable consequence:** A caller supplying a null-only Arrow feature receives a terminal scoring error while the equivalent server observation publishes an inconclusive result, violating the selected-null policy and direct/server agreement. This is distinct from a genuinely non-null wrong-typed column, which remains a type error.

**Required testable correction:** In the existing direct PSI/SPC column resolution, classify a present `DataType::Null` feature as incomplete before type mismatch handling, using the existing unscored result; keep non-null wrong types terminal and preserve all fitted-bin and chart logic. A focused direct PSI fixture with an all-null numeric or categorical Arrow column and an SPC fixture with an all-null numeric Arrow column should each return the empty unscored report. Keep the existing typed-null and wrong-type assertions.

## Verification limits

The committed evidence records green Vala, Wyrd, server, OpenAPI, codegen, docs, Rust/Python/TypeScript journey, and exact named-test commands after the final change. I did not rerun the lanes. They cover typed nullable arrays and SQL nulls; no recorded test covers a `DataType::Null` direct target column. JSON SDK observations cannot carry NaN/infinity, so Vala and server SQL tests own that proof. The one-query snapshot property is structural rather than a mid-query race test.

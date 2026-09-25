# TASK-011 r2 statistics domain review

**Subject:** `338f33235f81c30dfe3a570dc26934fe7bb77048..3c6fc1880692cc29c4c1ff7fc5fcb72a850bb9d6`. **Result: FAIL.**

## Boundary, authority, and source coverage

I reviewed approved spec revision 38 (REQ-153–REQ-157 and AC-034), the original TASK-011, the r1 verdict/validation and R1 remediation task, the cumulative diff, `architecture/logic/drift.md`, `architecture/references/domain/drift-monitoring.md`, the Vala baseline/feature/PSI/SPC/report code, the server's `ObservationWindow` and `DistributionFold`, and the focused tests and task evidence. [NIST's X-bar/S formulas](https://itl.nist.gov/div898/handbook/pmc/section3/pmc321.htm) confirm the implemented c4-based three-sigma limits, including the `sqrt(n)` factor. This report covers statistical scoring and its server aggregate input, not unrelated authorization or runtime ownership.

The earlier statistics findings are closed: direct PSI/SPC now check every row of an already selected batch, including a null-only row; SPC returns one empty, inconclusive report if any feature has an empty or partial subgroup. The server builds one combined completeness-and-aggregate statement, so its feature counts and completeness decision use the same Oracle cut. Numeric and categorical PSI bins remain exhaustive, the `other` category is scored, SPC uses fixed complete subgroups and strict limit comparisons, and old fitted formats are refused before decoding.

## Material proposed finding

### STAT-R2-1 — INCORRECT: an insufficient PSI feature can still produce a failed run

**Violated obligation:** REQ-153 retains a minimum target sample of 100; REQ-156 says an insufficient complete target is inconclusive. TASK-011 requires empty/short target data to be inconclusive. A run cannot fail and dispatch an Operator on a feature while another configured feature has too few target samples to judge.

**Exact location and evidence:** [`psi/mod.rs:254–313`](../../../../../crates/vala/vala-drift/src/psi/mod.rs) assigns `Inconclusive` to a feature with fewer than 100 counts, but then calls [`DriftReport::aggregate_verdict`](../../../../../crates/vala/vala-drift/src/report.rs), which gives any drifting feature precedence over an inconclusive one. The server [`ObservationWindow::incomplete`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs) checks only distinct configured series per `record_id`; it does not require one row per `(record_id, series)`. Each PSI feature's bin aggregate counts physical rows, and [`DistributionFold::finish`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs) passes those counts into the same scorer. Thus 99 complete logical observations, plus one repeated `x` series row, can yield 100 `x` values and 99 `y` values with zero incomplete records; if `x` drifts, the common verdict is `Drift`. The public aggregate scorer also accepts this count combination directly. Existing minimum-sample tests use only one feature.

**Observable consequence:** A short multi-feature PSI window can publish a failed result and dispatch Operators, although one configured feature has insufficient evidence for a verdict; direct scoring of the corresponding 99 selected wide observations is inconclusive. Historical or externally ingested rows make this reachable even though the normal SDK projects one row per feature.

**Required testable correction:** In the existing PSI aggregate scorer, make any configured feature below the minimum target sample force the whole PSI run inconclusive before drift aggregation. Keep the common verdict aggregator and Custom behavior unchanged. A focused two-feature count fixture with one drifting feature at 100 and the other at 99 must produce an inconclusive top-level report and no scored result/dispatch; a server aggregate fixture with a repeated series row should prove the same path. Preserve ordinary complete PSI evidence and the existing one-feature short-sample behavior unless the approved specification requires removing its inconclusive feature detail.

## Verification limits

The committed evidence records green Vala, server, OpenAPI, and three SDK journey lanes, plus exact named-test commands. I did not rerun them. The one-query snapshot property is structural; the current test does not inject a concurrent write mid-query. JSON SDK observations cannot carry NaN or infinity, so the unit and SQL tests own those cases. No recorded fixture covers a two-feature PSI target where exactly one feature is below the minimum sample while another signals.

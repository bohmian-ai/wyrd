# TASK-011 statistics domain review

**Subject:** `338f33235f81c30dfe3a570dc26934fe7bb77048..6e3bac0370a31b19d767ac4d20830d430c3f2ff5` (candidate `6e3bac03`). **Result: FAIL.**

## Boundary, authority, and source coverage

Reviewed approved specification revision 37, REQ-153–REQ-156 and AC-034; TASK-011; `architecture/references/domain/drift-monitoring.md`; the changed `vala-drift` PSI, SPC, feature, baseline and report code; the server's fixed aggregate, completeness and scoring path in `verification/drift.rs`; the observation wire record; and the relevant direct, SQL, and SDK fixtures. I checked the implemented X-bar/S equations and c4 constants against the NIST formulas cited by the task. This is a statistical-correctness review, not a review of unrelated runtime or repository rules.

The new numeric PSI bins cover finite values outside fitted edges; categorical fitting reserves a zero-baseline `other` bin, and direct and SQL counts include it. The smoothed PSI formula and strict threshold comparison remain intact. SPC computes subgroup sample standard deviations with `n-1`, uses the approved c4-based X-bar and S limits including `sqrt(n)`, and signals strictly outside either chart. The report persists typed per-chart evidence. Complete baseline subgroup count and trailing-row rejection are enforced. Server target ordering uses `created_at, record_id`; partial subgroups are detected. These portions satisfy the reviewed statistical requirements.

## Material proposed findings

### STAT-1 — INCORRECT: all-null selected rows disappear from direct scoring

**Obligation:** REQ-156 requires a target observation carrying a configured feature with a null value to make direct and server scoring inconclusive, without dropping that row; direct and server checks must agree.

**Location and evidence:** [`feature.rs:124–161`](../../../../../crates/vala/vala-drift/src/feature.rs) defines `TargetColumn::carried(row)` as `Option::is_some()`. Thus a row with nulls for every configured feature is treated as unrelated by `target_complete`. [`feature.rs:142–146`](../../../../../crates/vala/vala-drift/src/feature.rs) then discards the row via `flatten`, so [`psi/mod.rs:186–202`](../../../../../crates/vala/vala-drift/src/psi/mod.rs) may score the other 100+ values and [`spc/mod.rs:183–218`](../../../../../crates/vala/vala-drift/src/spc/mod.rs) may group shifted rows. In contrast, the server SQL selects a row by `series` even when `num_value`/`str_value` is null and counts it incomplete at [`drift.rs:160–185`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs). The existing direct tests place a null beside a non-null configured value, so they miss the all-null case.

**Observable consequence:** The same selected event can yield a scored direct result and an inconclusive server result. With enough valid PSI rows, the null event vanishes entirely; with SPC, grouping can change after the vanished row.

**Testable correction:** Make direct row selection distinguish a genuinely unrelated observation from a configured feature present with null. Preserve the server's `series`-presence semantics and reject a selected all-null row before counting/grouping. Add a direct PSI/SPC fixture with enough otherwise valid rows and one selected all-null record; expect a whole unscored report and compare with the server completeness fixture. The existing all-null *unrelated* fixture must remain excluded using an explicit row-selection signal; nullness alone cannot express both cases.

### STAT-2 — INCORRECT: a partial SPC window can still fail and dispatch

**Obligation:** REQ-154 says a trailing partial subgroup makes the target run inconclusive; REQ-156 likewise makes insufficient complete samples inconclusive.

**Location and evidence:** [`spc/mod.rs:330–367`](../../../../../crates/vala/vala-drift/src/spc/mod.rs) marks only the affected feature `Inconclusive` and then calls `DriftReport::aggregate_verdict`. [`report.rs:78–99`](../../../../../crates/vala/vala-drift/src/report.rs) prioritizes any `Drift` feature over any `Inconclusive` feature. The server's SQL groups each feature's series independently ([`drift.rs:248–263`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs)) and `incomplete` checks distinct series presence per `record_id`, not uniqueness or equal total rows per series. Historical or externally ingested rows with a duplicate series for one record can therefore give feature A complete out-of-limit subgroups and feature B an extra trailing row. `SpcScorer` is also public and accepts this aggregate sequence directly. The report becomes `Drift` despite the partial window.

**Observable consequence:** An incomplete SPC window may produce a failed Verification Result and Operator alert, rather than the mandated inconclusive result.

**Testable correction:** In the SPC scorer, make any partial or empty feature force the whole run inconclusive before applying drift-signal precedence. Do not change the common verdict precedence for otherwise complete reports or Custom. Add a two-feature scorer fixture with a signal in one feature and a partial second feature, plus the equivalent server aggregate path with duplicate series presence; assert the run is inconclusive and cannot dispatch.

## Verification limits

The task records passing Vala, wyrd-spec, server, and first-class SDK lanes and focused tests. They exercise formula constants, ordinary complete data, null values alongside a non-null configured feature, and single-feature partial SPC windows. No recorded fixture covers an all-null selected direct row amid otherwise sufficient data or the mixed signal-plus-partial SPC case. This review is static; I did not rerun the existing gates or alter reviewed source.

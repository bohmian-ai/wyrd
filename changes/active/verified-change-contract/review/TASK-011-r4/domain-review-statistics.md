# TASK-011 r4 statistics domain review

**Subject:** cumulative `338f33235f81c30dfe3a570dc26934fe7bb77048..1268bbe3ef820ba50e1c6dbb065d4c01b415f5f3`. **Result: PASS.** HEAD remained at the candidate during this review.

## Boundary, authority, and source coverage

I reviewed approved specification revision 38 (REQ-153–REQ-157 and AC-034), original TASK-011, the r1–r3 verdicts and validated ledgers, all three remediation tasks, the cumulative diff, `architecture/references/domain/drift-monitoring.md`, the Vala baseline/feature/PSI/SPC/report modules, server observation selection and aggregate fold in `verification/drift.rs`, and the direct, SQL, and journey evidence recorded in TASK-011. This is a statistics and data-completeness review; it does not independently audit repository style or authorization. The implementation's c4-based X-bar/S limits match the NIST formulas cited by the task, including `sqrt(n)` and the sample standard deviation denominator.

Direct PSI and SPC scorers now classify a present Arrow `Null`-typed feature as incomplete before the wrong-type branch. `target_complete` checks every row of the already selected batch and returns an unscored report with no feature rows for a nonempty null-only target. A non-null wrong-typed column still returns `FeatureTypeMismatch`. The new `psi_null_typed_target_is_unscored` and `null_typed_target_is_unscored` fixtures cover numeric PSI, categorical PSI, and SPC; the existing wrong-type and typed-null fixtures remain. This closes FIND-TASK-011-12.

Earlier statistical closures remain intact: PSI has exhaustive frozen numeric and categorical bins, including the scored `other` bin, symmetric zero-bin smoothing, and a whole-report 100-sample guard; SPC uses fixed complete subgroups, twenty baseline groups, frozen two-sided X-bar/S limits, strict outside-limit signaling, and typed chart evidence. A partial or empty target feature leaves the whole report unscored. The server selects by configured series, treats null, non-finite, omitted, or repeated series values as incomplete, and folds completeness and aggregate values from one query cut. New fitted-format checks refuse legacy scoring; prior reports remain readable. No extra statistical method or missing-value bin was introduced.

## Material proposed findings

None.

## Verification limits

This was a static review; I did not rerun implementation tests. TASK-011 and R3 record passing exact focused commands for the new NullArray fixtures, `mise run test:vala` (1282 passed), server integration, and Rust/Python/TypeScript Drift journeys after the last code change. The later candidate commit only changes R3 task status. SDK JSON cannot carry NaN or infinity, so Vala and server SQL fixtures own that proof. The one-query snapshot property is structural rather than a mid-query race injection.

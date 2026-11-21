# Independent validation of R2 checkpoint evidence

## Superseding disposition: explicitly deferred for this review

After the initial validation below, the maintainer explicitly instructed
“ignore the benchmark” and “its not required right now.” That current direction
defers the benchmark acceptance requirement for this review. The earlier gap was
valid before this instruction; it is no longer a blocking acceptance obligation.
Do not retain `FIND-TASK-001-3` or create R3 for it. No performance PASS or completed
benchmark execution is claimed. The final runtime review/verdict is PASS with
FIND-TASK-001-1 and FIND-TASK-001-2 closed. The analysis below is preserved as the
historical pre-deferral assessment, not the current verdict.

Subject: candidate `e1f22abb06737ce63360117e590a1b40e0a1307a`, prior runtime
candidate `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`, cumulative base
`66f44aee193077ef3565aa76140a942d8eb593f9`. Contract: approved spec revision 2,
TASK-001 and its implemented R2. This is a focused independent validation of the
remediation reviewer's proposed performance evidence gap, not another runtime
review. Only this report was written; no code, tests, or benchmarks were run.

## Disposition: CONFIRMED

The proposed checkpoint gap is an **EVIDENCE_GAP**, eligible for stable
`FIND-TASK-001-3`. It is not a demonstrated throughput regression or a defect in
either code correction. Both prior runtime findings remain the primary reviewer's
separate closure responsibility.

### Governing requirement and exact missing proof

- `spec.md:238–246` requires the three progress checkpoint populations after
  focused correctness, comparing each with the original baseline and previous
  accepted checkpoint under unchanged controls and supported readers.
- `tasks/README.md:43–81` supplies the exact commands, report/compare procedure,
  and rule that invalid/inconclusive checkpoints or material regressions prevent
  progression. Full final qualification is separately placed at CPU-task
  completion (`:83–89`).
- Implemented R2 explicitly retains that policy at
  `review/TASK-001-review-002/TASK-001-R2-mirror-expiry-and-cleanup-readiness.md:186–189`.
- Its evidence expressly says no benchmark ran and the writes/hot/Parquet
  checkpoint is still owed (`:229–232`). No record named for the R2 runtime
  commits was found in the available benchmark record paths during this check.

The task's earlier evidence records maintainer-directed skips for its original
scenarios. The conversation also preserves the earlier closing checkpoint and an
explicitly open hot-read regression. Neither establishes a current waiver of the
checkpoint R2 itself retains. The implementation statement that it will run when
the maintainer directs it is a deferral claim, not a new approval to remove the
acceptance obligation. The present review request does not waive that obligation.

Starting condition: correctness verification is reported green for the candidate,
and R2 is submitted for acceptance. Expected: candidate-bound valid progress
records and comparisons for the required three populations. Actual: the candidate
has no such supplied execution evidence. Consequence: the review cannot establish
the agreed protected write/read behavior after these runtime changes. Source
inspection and functional tests cannot substitute for the specified measurement.
No numeric regression or speculative performance threshold is inferred.

## Smallest complete correction

This needs **execution evidence only**; no production change, new benchmark,
new framework or new two-pod performance promise is justified.

1. Run on the immutable candidate once the host is quiet. Repository rules in
   `benchmarks/AGENTS.md:21–24` prohibit concurrent build/test/benchmark load and
   require checking existing usable records first. Unrelated host work cannot
   authorize a contaminated run. Its current presence delays valid execution;
   it does not convert explicitly missing proof into a source defect or erase
   the evidence gap.
2. Run these existing commands serially with normal harness-owned repetitions:

   ```bash
   mise run bench:l1:bifrost:writes -- --steps otlp-logs-c64 --allow-dirty
   mise run bench:l1:bifrost:hot -- --steps point-memtable-c64 --allow-dirty
   mise run bench:l1:bifrost:parquet -- --steps point-c64 --allow-dirty
   ```

3. For each corresponding experiment, identify its matching original baseline
   at `66f44aee1` and the previous accepted closing checkpoint at `31af0fe55`.
   Check host, method, flags, fixture/settings and accounting compatibility using
   the supported reader. If the matching original record is not locally present,
   recover the existing compatible record rather than substitute a different
   baseline. The inherited user-supplied original comparison is not permission
   to invent an unavailable record path or reconstruct metrics manually.
4. Run the supported readers and save their unedited outputs:

   ```bash
   mise exec -- target/profiling/release/capacity report <candidate-record.json>
   mise exec -- target/profiling/release/capacity compare <original-baseline.json> <candidate-record.json>
   mise exec -- target/profiling/release/capacity compare <previous-accepted-checkpoint.json> <candidate-record.json>
   ```

5. Record command, candidate identity/dirty state, record paths, VALID/INVALID
   verdicts and the required Question/Run/Report/Comparison/Answer/Unproven
   report under the existing evidence owner or a linked review report. Claim
   unchanged/gain/regression only from supported comparisons and repetition
   spread. The already-disclosed hot-read exception remains visible; do not
   silently broaden it to any additional degradation. Invalid/inconclusive
   execution or a new material regression cannot close the gap.

Observable closure: all three candidate-bound populations have valid compatible
reports and both comparisons, with the acceptance consequences of those results
resolved under the existing agreement. These subsets remain progress evidence,
not final full CPU qualification. Do not demand the 80 target, a new duplicate
cost benchmark, production churn, or historical TDD repair for this finding.

If the orchestrator obtains valid execution before final verdict, reconcile this
report against that new evidence and mark the gap closed rather than retain an
obsolete blocker. At this report's inspection point, the required execution is
absent. HEAD remained `e1f22abb06737ce63360117e590a1b40e0a1307a`.

Skipped: benchmark execution and runtime re-review. Risk: candidate performance
acceptance remains unproven; no measured regression is asserted.

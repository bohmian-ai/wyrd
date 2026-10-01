# TASK-005 R1 performance and benchmark domain review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65`; reviewed tree at `1fc68f3b78c4dbf82a8f1c518bbc40343c484d65`. **Result: FAIL.** No benchmark was rerun in this review, as directed.

## Boundary, authority, and source coverage

- Authority: original user-designated `TASK-005-simplify-bifrost-telemetry.md` (external revision 14), current approved `spec.md` revision 24 REQ-008/AC-009/AC-010/AC-014, R1 remediation FIND-TASK-005-4, `AGENTS.md` §§11–12, `architecture/agent-rules.md`, `architecture/references/languages/{spec-driven-development,testing-workflows}.md`, and `architecture/bifrost-design.md` telemetry measurement rule. The user waived `mise run gate` for this review, not benchmark acceptance.
- Execution and evidence: `mise.toml` `bench:bifrost:query-capacity`; unchanged `wyrd-testing/src/bin/bifrost_query_capacity/{main,server,run,report,workload}.rs`; both retained `r1-outputs/bench-{prechange,candidate}/{report.md,samples.jsonl.gz,server.log.gz}`. Inspected cumulative diff for the benchmark harness and representative Oracle query-path deletions. The harness is unchanged across the review range.
- The harness starts one release `wyrd-server` in a systemd user scope with `CPUQuota=800%` and `MemoryMax=16 GiB`, validates `cpu.max` and `memory.max`, then drives public-client requests. It does not set `AllowedCPUs`. Its raw JSONL preserves per-step client latencies, clients, and step names; the reports preserve rates, percentiles, server CPU and memory, errors, and results. No retained sample measures other processes' CPU use, per-core scheduling, or throttling of the host around the selective window.

## Measured result and limits

| Obligation | Evidence | Result |
|---|---|---|
| REQ-008 one-client selective p50 <7 ms, p95/p99 <10 ms | Candidate report: 9.3/12.6/15.5 ms; raw one-client row has 1,040 successful latencies, median 9,337 µs and p95 12,559 µs. | **FAIL** |
| REQ-008 selective peak >=1,200 successful QPS | Candidate report: 1,212.1 QPS at 64 clients, zero errors. | PASS |
| Other standard benchmark judged rows | Candidate report: remaining named target summaries PASS, zero query errors, peak memory below 15 GiB; standard benchmark exits 1 because selective target fails. | PASS for those rows; whole benchmark FAIL |
| AC-014 benchmark remains valid after cleanup | The unchanged harness produced a report and raw samples through a release server; no invalid workload is shown by the retained report. Its measured selective miss remains real evidence for this run. | Valid measurement, failed target |
| Task's prechange comparison | Prechange `f0365f9ea` report: 6.8/8.2/8.9 ms at one client, 1,222.3 QPS peak; raw one-client row has 1,461 latencies. `f0365f9ea` is an ancestor of base `05d7d7413` with intervening TASK-004 implementation, so it is descriptive comparison, not an exact base-vs-candidate causal control. | Limited |

The candidate server log includes repeated audit publication lock warnings. It does not timestamp benchmark steps or establish that those warnings affected the selective run. Host contention is plausible because `CPUQuota` caps total server consumption without reserving physical cores, and the user reports another test worktree and VM active during the run. The retained evidence does not prove host contention, rule out a code regression, or identify a specific Oracle hot-path cause. Several workload rates/latencies also worsened relative to the older run, while small aggregate at one client stayed nearly the same; this pattern alone cannot assign cause. Adding CPU affinity or changing the harness now would alter the measurement setup and is not required by the approved task. The user declined a rerun, so no passing candidate result can be inferred.

## Proposed finding

**PERF-1 — MISSING / unclosed performance proof.** REQ-008 and AC-010 require the standard candidate benchmark to meet the one-client selective targets, and original TASK-005 acceptance criterion 4 requires investigation of a material regression before closeout. At `r1-outputs/bench-candidate/report.md` selective target row, the candidate p50/p95/p99 are 9.3/12.6/15.5 ms against <7/<10/<10 ms; the raw samples independently corroborate the miss. `r1-outputs/bench-prechange/report.md` is from an ancestor before additional TASK-004 changes, so it cannot establish that TASK-005 caused or did not cause the difference. **Observable consequence:** the required latency claim for this candidate is unproven and the benchmark command exits 1. **Testable correction:** obtain a valid standard candidate run under a quiet, documented host using the existing benchmark, retain raw samples/report/log and show all required targets passing; if selective latency still misses, diagnose the real source from that run and make the smallest correction at its owner, then rerun the same standard benchmark. Do not assert an environment-only cause from the current samples or change targets, workload, or harness to manufacture a pass.

The user has explicitly deferred `mise run gate` to another branch; this review does not propose a gate finding. No benchmark run was performed by this reviewer.

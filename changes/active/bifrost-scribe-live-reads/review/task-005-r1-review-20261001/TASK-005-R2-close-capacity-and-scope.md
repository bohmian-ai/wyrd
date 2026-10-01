---
id: TASK-005-R2
title: Close selective capacity, declaration style, and candidate scope
kind: remediation
status: proposed
---

# TASK-005-R2 — Close selective capacity, declaration style, and candidate scope

**Approved spec:** `changes/active/bifrost-scribe-live-reads/spec.md` revision 24 (REQ-008, REQ-012, AC-009, AC-010, AC-014). **Original task:** `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` revision 14, SHA-256 `0e62758cd6790451c2fa7497c06f6eba7d57400ed15ea38b50695c30b0177814`. **Prior verdict/remediation:** `changes/active/bifrost-scribe-live-reads/review/task-005-review-20261001/{verdict.md,findings-validation.md,TASK-005-R1-close-telemetry-proof.md}`. **R1 cumulative candidate:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65`. See this review's [verdict](verdict.md) and [validated findings](findings-validation.md). The user deferred `mise run gate` to another branch; its absence is excluded from this task review.

## Diagnoses and correction outcomes

| Finding | Diagnosis, consequence, and why current proof falls short | Minimal correction within approved behavior |
| --- | --- | --- |
| FIND-TASK-005-4 | Original task AC 4 and current spec REQ-008/AC-010 require the standard benchmark's approved selective latency. `review/task-005-review-20261001/r1-outputs/bench-candidate/report.md:7-15` records one-client p50/p95/p99 `9.3/12.6/15.5 ms` against `<7/<10/<10 ms`; the command exits 1 and raw samples corroborate it. The `6.8 ms` comparison comes from `f0365f9ea`, before the task base and intervening TASK-004 changes. Other work may have contended for cores, but the retained run cannot establish that cause; passing journeys measure different properties. | Reuse the **existing standard benchmark**, unchanged workload, thresholds, and harness, on a quiet documented equivalent 8-CPU/16-GiB host. Retain raw samples, report, server log, and host conditions. If the target still misses, diagnose the actual bottleneck from that run, correct it at its owner, and rerun the same benchmark. Do not guess at a query-path patch, weaken the target, or add an affinity mechanism solely to make this result pass. A valid passing candidate result closes the target; describe the earlier miss only to the extent the evidence supports. |
| FIND-TASK-005-5 | R1 corrected the imports and signatures named in the first review, but new or materially changed declarations still use qualified types in `oracle/query_stream.rs:130,204-208`, `oracle/telemetry.rs:15-29`, `scribe/persistence.rs:1511`, `wyrd-testing/src/server.rs:252`, and `wyrd-testing/src/bifrost/cluster.rs:574`. `architecture/agent-rules.md:9-10` requires changed fields and signatures to use module-top imports and bare names. These reachable stream, tracer, persistence, and test-inspection interfaces hide their dependencies from the import blocks. | Reuse each file's existing module-top import block; import the required `Span`, `Pin`, `BoxFuture`, `Any`, and `StorageInspection` types and use bare names in the **changed** declarations. Remove no owner behavior, add no wrapper, and leave unchanged neighboring declarations outside this correction. |
| FIND-TASK-005-6 | Commit `1f1cbcf5f` sits inside the cumulative TASK-005 candidate and adds `changes/backlog/bifrost-operations-dashboard/spec.md:1-224`, a future dashboard/UI/API draft. TASK-005's required dashboard artifact is its telemetry evidence table, already present; no task code or evidence consumes this draft. The first review expressly excluded it. Accepting this candidate would also accept unrelated product scope without its own review. | Present an immutable TASK-005 candidate whose base-to-head diff excludes this draft, keeping the dashboard proposal for its own review. Retain the task evidence table and all telemetry commits. Do not edit the draft merely to make it appear task-related. |

## Preserved behavior and non-goals

Keep the R1 closures for FIND-TASK-005-1, -2, and -3: owner-locked backlog gauges, the real-server abrupt-restart journey plus fresh-recorder restore proof, and the six-question production evidence/inventory. Preserve write ACKs, WAL retirement, publication, admission, query terminals, tenant checks, Forge settlement, client contracts, and the closed Forge metric catalog. Do not add a telemetry ledger, benchmark matrix, new performance threshold, speculative optimization, or compatibility alias. The SQL release-build correction may remain if the narrowed candidate needs it; do not change tenant transaction semantics. The broad repository gate is deferred by explicit user instruction and is not R2 acceptance work.

## Acceptance and focused proof

1. **FIND-TASK-005-4:** The standard candidate benchmark yields valid raw output meeting the approved selective one-client p50/p95/p99 limits and all other applicable targets. A remaining red result receives a source-backed diagnosis and smallest owner correction before another run; no unproved environmental assertion stands in for a pass.
2. **FIND-TASK-005-5:** Source inspection confirms bare type names and module-top imports at each cited changed declaration; `mise run fmt` and `mise run lints` pass on the final source.
3. **FIND-TASK-005-6:** `git diff --name-only 05d7d7413 <new-candidate>` contains no `changes/backlog/bifrost-operations-dashboard/spec.md`, while the TASK-005 telemetry evidence remains in the candidate task file.
4. Re-run the directly affected focused checks after source changes, retain the passing Bifrost journey evidence for unchanged behavior, and record the release build/SQL check if the SQL boxing fix remains. Return the new immutable cumulative candidate to `$wyrd-task-review`.

## R2 implementation evidence

Commits: `5b17af21f` (FIND-5), `be31c6f99` (FIND-6).

| Finding | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| FIND-TASK-005-4 | None; the standard benchmark has not been rerun | Host not quiet at attempt (load average 12.5/26.9/22.0, other-worktree builds); run stopped by user before measurement | OPEN |
| FIND-TASK-005-5 | Module-top `Span`/`Pin` (`oracle/query_stream.rs`), `BoxFuture`/`Any`/`Span` (`oracle/telemetry.rs`), `Span` (`scribe/persistence.rs`), `StorageInspection` (`wyrd-testing/src/server.rs`, `bifrost/cluster.rs`); bare names at each cited declaration | `mise run fmt`, `mise run lints` exit 0; `git diff --check` clean | PASS |
| FIND-TASK-005-6 | `be31c6f99` reverts `1f1cbcf5f`; draft preserved on local branch `backlog/bifrost-operations-dashboard` | `git diff --name-only 05d7d7413 HEAD` contains no `bifrost-operations-dashboard` path; TASK-005 evidence table unchanged | PASS |

Limits: FIND-4 needs `mise run bench:bifrost:query-capacity` on a quiet
8-CPU/16-GiB-equivalent host. No source change in R2 touches runtime behavior
(imports only), so the Bifrost journey result from R1 stands for unchanged
behavior. The SQL boxing fix (`49f3b601d`) remains; the release build and
`test:sql` evidence is recorded in R1.

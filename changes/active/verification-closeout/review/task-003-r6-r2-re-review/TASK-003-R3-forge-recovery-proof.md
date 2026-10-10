---
id: TASK-003-R3
kind: remediation
status: proposed
remediates: [FIND-TASK-003-7]
---

# TASK-003 R3: Prove Forge recovery after live startup work is interrupted

## Subject and authority

- Approved spec: `changes/active/verification-closeout/spec.md` revision 3.
- Original task: `changes/active/verification-closeout/tasks/TASK-003-r4-canonical-support-desk-closeout.md`.
- Prior verdicts, validated findings and R1/R2 packets: `changes/active/verification-closeout/review/task-003-r4-canonical-closeout/` and `changes/active/verification-closeout/review/task-003-r5-r1-re-review/`.
- Current review evidence: `changes/active/verification-closeout/review/task-003-r6-r2-re-review/findings-validation.md` and `verdict.md`.
- Base `7f79fb3417db651adedac194ada8908f0a0372d7`; reviewed cumulative candidate `e2d324a916cfff25e2d362c7d7d2ab1c6aec74d9`. Reassess the full cumulative candidate after this remediation.

## FIND-TASK-003-7 — Recovery code is fixed, but its required proof misses the risk

R2's focused acceptance requires interruption during startup with a live plan or heartbeat, followed by a same-owner restart that refuses unexpired self-reclaim and later recovers after lease expiry. In `crates/vala/vala-bifrost-redux/tests/integration/forge/production_routes.rs:1112-1201`, `interrupted_startup_waits_for_its_prepared_lease` aborts at the Prepared claim gate. The reconciliation heartbeat starts later in `src/forge/worker.rs:4620-4650`; this route has not started a plan runner. The test therefore cannot exercise cancellation of a live startup child. It then sleeps two seconds and checks that the lease did not change and readiness stayed false. A restarted worker delayed before its first claim check produces those same values, so elapsed time does not prove that the unexpired recovery branch ran. The recorded red mutation catches an unsafe policy under one schedule but does not make every passing run observe the branch. `architecture/references/languages/implementation-execution.md:238-242` prohibits using a sleep in place of deterministic synchronization.

The production correction should remain: `ForgeWorker::run` marks itself live and owns the stop-token guard before startup; the captured predecessor state gates both Claimed/Running reclaim and Prepared takeover in the existing `ForgeTasks` SQL. No new production defect was validated. Complete the focused proof at the existing worker/test observation boundary: interrupt startup only after a plan or reconciliation heartbeat is actually live; on a same-owner restart, observe at least one completed recovery pass while the old lease is unexpired, then check durable ownership and expiry remain unchanged and readiness remains down. Finally let the lease expire and observe recovery. Use bounded timeouts only to fail a stalled test, not to infer a pass. Retain the existing Postgres fixture and worker observation pattern; if it cannot expose the unsuccessful pass, add only the minimum test-support signal at that boundary. Do not introduce a production option, separate harness, persistent fence or new recovery policy.

This proof closes the original overlap risk because it demonstrates the state the previous tests missed: old startup child work existed when the prior invocation stopped, and the replacement actually evaluated the unexpired attempt without renewing it. The owner that controls both cancellation and the predecessor flag remains `ForgeWorker::run`; no repeated downstream guard is needed.

## Acceptance and focused verification

| Finding | Observable acceptance | Narrow proof |
|---|---|---|
| FIND-TASK-003-7 | A live startup plan or heartbeat is observed before interruption; the same-owner replacement completes an unexpired recovery pass without renewing the old attempt or becoming ready; expiry then permits recovery | Exact named `mise exec -- cargo nextest run --locked -p vala-bifrost-redux -P journey --run-ignored=all --test integration -E 'test(=forge::production_routes::<named_test>)'` through `scripts/postgres/with-test-postgres.sh` and the owning migration setup. Inspect its trace if it fails. |
| FIND-TASK-003-7 clean predecessor boundary | An orderly joined predecessor still enables fast same-owner reclaim, and the existing SQL lease/attempt rules remain intact | Keep the recorded exact quiescence unit and `pg_forge_tasks::pg_tests::previous_owner_reclaims_its_unexpired_attempt` selector; rerun only if the proof edit changes their owners or behavior. |

Run the narrowest owning `mise` format/lint checks for changed Rust. Specifically named tests in the evidence require exact selectors under `mise exec --`; use the repository-managed Postgres wrapper for the integration case. The broad Forge suite and `mise run gate` remain change-review aggregate proof, not this proof correction's iteration requirement.

## Preserved behavior and non-goals

Preserve Oracle holder accounting and wakeups, the shared Scribe outbox, gateway Card UID authorization, tenant isolation, the public SDK workflows, the accepted in-memory loss window, Forge stop-token cancellation, both same-owner SQL gates, clean-join fast reclaim, and lease-expiry fallback. This task changes the focused proof, not the approved product or recovery semantics. It adds no public API, persistent state, supervisor policy, synthetic load, or unrelated refactor.

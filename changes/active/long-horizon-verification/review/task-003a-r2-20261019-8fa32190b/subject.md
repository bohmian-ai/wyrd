# Immutable review subject

- Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001
- Candidate: 8fa32190b26b11588721e8db44b3eb94139e3739
- Cumulative base: 22335c2c40a5ad24c27bfa0d4686f4da49f922c5
- Latest remediation locator: 8376bb2aa71be18b9c7269cd9f2a601509137dc7..8fa32190b26b11588721e8db44b3eb94139e3739
- Historical review candidate: 3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4
- Approved authority: changes/active/long-horizon-verification/spec.md revision 20.
- Original task: changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md
- R1: changes/active/long-horizon-verification/review/task-003a-20261018-7cda4e931/TASK-003a-R1-retain-pending-work-and-remove-stale-code.md
- Prior reports and R2: changes/active/long-horizon-verification/review/task-003a-r1-20261018-3d0b301c5/ (untracked local artifacts, available for inspection; preserve them).
- R2 task: TASK-003a-R2-close-grouped-verification-gaps.md in prior directory, with appended implementation evidence.

Read actual git diff base...candidate; latest diff locates corrections, not audit scope. Review the original cumulative task against current approved authority. Prior findings are hypotheses, not prescribed conclusions. Historical revision-18 diagnoses may be superseded by revisions 19/20.

## Navigation starting points, not exhaustive coverage

- GroupPoller::poll, Reader, Visible in crates/wyrd/wyrd-server/src/verification/groups.rs: source selection, internal read principal, decoding/completeness, snapshot pinning.
- Scheduler::hand_off_groups / hand_off in verification/scheduler.rs: bounded claim/transaction, owner authority, retry/terminal handoff, incomplete deadline.
- ScribeSink scheduling in scribe_outbox.rs: ACK notifications, outbox/capacity and loss behavior.
- wyrd-sql src/queries/verifier_executions.rs: activity SQL, due claims/overdue, refuse_group, pinning, settlement and status decoder; verification.rs binding projections.
- Migrations 20261016, 20261018, new 20261019000000_verification_refused_shape.sql; SQL tests pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order.
- Spec TriggerDuration; InputSlot visitor; shared reference decoder; public verification/execution/error types.
- Server pg_verification_groups::pending_groups_debounce_and_stop_after_handoff and ::incomplete_snapshots_retry_then_error.
- Rust/Python/TS verify-in-real-time journeys, MCP verification tests, docs how-verification-works and runtime limits, architecture/wyrd-design.md.

## Coordination and evidence

Review only; do not edit source, tests, spec or prior reports. Write only your assigned report. You are not alone; do not revert others. CodeGraph first per AGENTS. Read governing architecture/reference router and expand this map independently. No source archives/diffs in report directory.
Root runs focused PostgreSQL proof sequentially and records verification.md; reviewers must not run PostgreSQL or cargo/build lanes concurrently. Request additional dynamic proof through root. Static source inspection is independent.
No intended verdict supplied. Mandatory topology includes behavior, invariant, standards, maintainer, system, data and security discovery; conditional fresh follow-up; fresh structured Ponytail validation after all reports. No benchmarking/full journey reruns at task remediation stage.

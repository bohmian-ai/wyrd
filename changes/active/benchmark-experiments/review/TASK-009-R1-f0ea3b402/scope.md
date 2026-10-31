# TASK-009 immutable first review

Base: `6f38db16c524417301ffc4a5dc0b478daf997ed1`. Candidate/initial HEAD: `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`.
Approved spec: `changes/active/benchmark-experiments/spec.md` revision 24. Original task: `tasks/TASK-009-unified-queue-and-closeout.md` revision 2.
Review complete `git diff 6f38db16c..f0ea3b402` and each of 15 commits. Base equals merge-base; skill three-dot range is identical. No earlier TASK-009 evidence carried forward; revision-23 task is superseded.

## Starting navigation map

- verification/submissions.rs: Submission, typed admission, acknowledged work, SubmissionSink; Scribe success callback, submit service, scheduler/groups are callers.
- verification/scheduler.rs and groups.rs: frozen preparation, bounded in-flight ownership; distributed runner consumes archived Bifrost inputs.
- wyrd-sql queries/verifier_executions.rs: accept_scheduled/grouped, expected cursor, generation consumption, dedupe and rollback; additive group generation migration, TenantConn/RLS.
- scribe_outbox.rs and Scribe ingress/execution_inputs: own-input ACK association and publication. results/drift preserve payload owners.
- wyrd-runtime outbox.rs: shared dispatch/next_wake clock, used by admission/audit/capture/other writes.
- wyrd-client state/observe: bound Verifier spec borrow. wyrd-testing gateway_capture: paced upstream buffered answer reuse.
- Tests: server pg_verification_runtime/groups, SQL pg_verifier_executions, wyrd-testing verification_runtime SDK journey; Rust/Python/TS stories; stale gate repairs and llms-full regeneration.
- Authorities: root and nested AGENTS, applicable agent_docs guides, architecture README/verification sections of wyrd-design and bifrost-design/security posture, TESTING.md and benchmarks/README.md.

## Reviewer selection

Fresh behavior reviewer: full task acceptance, each commit and proof/delivery obligations.
Fresh persistence/concurrency domain reviewer: transaction/dedupe/generation/tenancy and scheduling invariants.
Fresh system reviewer: failure/retry/cancellation, distributed polling and shared outbox liveness.
Fresh repository reviewer: all tiers, rustdoc/generated/tests/migration rules.
Fresh maintainer reviewer: changed ownership/abstractions, existing alternatives and workflow reuse.
Fresh independent Ponytail validator after discovery: proposal union and acceptance coverage. Focused follow-up only for material uncertainty or new validator findings.
No separate invariant reviewer: domain/system cover shared-state and lifecycle, behavior reviewer retains comprehensive obligations.

## Limits and caller direction

No source edits, commits, pushes, git config, attribution, or full benchmarks. Reports only here. Existing untracked tasks/unified-verification-flow.html is outside candidate and untouched. Sequential Cargo work only, coordinated by orchestrator.
User states maintainer accepts five L1 results at 19f4190a8 as valid: do not reopen driver-budget validity. V16 PR not opened; later profiling edits absent from task evidence. Assess remaining final-source proof/delivery requirements independently. No prior verdict intended.

# TASK-002 immutable subject
Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002
Base: 41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0
Candidate: d342c54749a871dcacf5b0c7ec29e3b458f2d9e2
Complete range: cumulative.diff (all three commits).
Approved spec: spec-snapshot.md (revision 6, frozen from user-supplied untracked packet).
Original task: original-task.md (implementation summary excluded).
Verification: verification-evidence.md (recorded commands and limits).
No prior TASK-002 review. Packet ownership stays unchanged; nothing is committed here.

## Shared starting navigation map; expand independently
blocks.rs: shared ActionExecutor/CheckExecutor contracts and new exports.
blocks/engine.rs: VerificationEngine run/drive/run_steps/attempt/run_requirements/judge/trace, Bindings/Attempted/Judged/decide own stage execution and deadlines.
blocks/scope.rs: Scope/Settled/Cause/Missing own reference resolution and gap causes.
blocks/drift.rs: DriftChecks judge/scored compose vala-drift::score_drift using fitted state per requirement and samples per input slot.
tasks/assertion.rs: shared evaluate_single/evaluate_item_mode; existing AssertionTaskExecutor sibling.
tasks/trace.rs: shared project_spans and trace test fixture; existing TraceTaskExecutor sibling.
results.rs: capture_actual shared with apply_context_capture.
executor.rs/context.rs/store.rs: existing execute_plan/execute_stage/Executors/RunLedger/ContextSnapshot/TaskRegistry and scenario callers. Independently compare replacement orchestration and scope to existing owners and task reuse constraints; leaf-call reuse alone cannot establish acceptance.
wyrd-spec VerifierSpec::plan, Reference, Step, Requirement, execution/results contracts are unchanged prerequisite owners.
blocks/engine/tests.rs has 14 scripted engine tests; scope has one new selector test. Existing scenario/assertion/trace/results tests are regression consumers.
Cargo manifests/lock add vala-drift dependency. No server/SQL/SDK writes in range.

Domain reviewers: concurrency (fanout, semaphore, retry, deadline, cancellation) and drift/evaluation (score fidelity, readiness, verdict/provenance). Verify absence of other sensitive domain changes.
All reviewers read AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx and applicable references via architecture/references/README.md. Follow CodeGraph before locating/understanding code. Read spec-driven-development and maintainer-style. Keep source immutable, write only your assigned report. Do not read other discovery reports. Do not implement. No intended verdict. Missing authority must be reported.

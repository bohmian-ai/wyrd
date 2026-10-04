# TASK-004 R3 Focused Follow-up Review

## Subject and Scope

- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`

This fresh pass investigated only the three conflicts assigned by the review
orchestrator. The candidate resolved to the requested commit before source
inspection. `.codegraph/` is absent, so the cumulative Git diff, current source,
callers, tests, applicable authority, and the conflicting R3 reports were used.
No build, test, Cargo, or mise command was run.

## Source Paths Inspected

- `changes/active/skald-workflow-runtime/spec.md:977-999,1659-1687,1845-1861,2575-2590`
- `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md:52-77,96-138,165-172`
- `architecture/agent-rules.md:9-10`
- `crates/skald/skald-workflow/src/workflow.rs:170-197,205-383,435-500,760-821,1410-1507`
- `crates/skald/skald-workflow/src/run.rs:56-184,215-249`
- `crates/skald/skald-workflow/src/workflow_surface.rs:687-700`
- `crates/wyrd/wyrd-server/src/components/workflow/host.rs:230-287`
- `crates/wyrd/wyrd-server/src/components/workflow/runs.rs:638-688`
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:272-305,571-625,769-885,1211-1287`
- `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2375-2421,2492-2506,2690-2784,7208-7220`
- `architecture/bifrost-design.md:382-409`
- `crates/wyrd-spec/src/card/prompt/mod.rs:25-48,245-274`
- `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:1-21,300-410`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs:1-35,1795-1848`
- `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1-42,2206-2248`
- All R3 discovery reports named by the orchestrator, especially
  `domain-review-concurrency-lifecycle.md`, `standards-review.md`,
  `task-review-behavior.md`, `task-review-invariants.md`, `system-review.md`, and
  `maintainer-review.md`.

## Conflict 1: Published `Running` Step and Pre-poll Abort

### Resolution

The concurrency-domain claim is correct; the passing discovery reports missed
a reachable source-to-consumer transition. Revision 13 explicitly requires
`attempts` to count attempts begun, pending and unstarted steps to have zero,
and running and cancelled active steps to have at least one
(`spec.md:995-999`). It also requires interrupted active steps to remain
cancelled with their timestamps (`spec.md:988-990`) and forbids observable
status regression (`spec.md:1661-1667`).

`WorkflowExecutor::drive` calls `RunLedger::step_started` and synchronously
invokes `on_transition` before constructing and spawning `StepTask`
(`workflow.rs:273-289`). `step_started` sets only `Running` and `started_at`;
it leaves the initialized atomic and snapshot attempt counts at zero
(`run.rs:125-130`; `workflow.rs:186`). The server passes
`AcceptedRun::observe` as this callback, which immediately replaces the stored
GET snapshot (`host.rs:285`; `runs.rs:650-665`). The invalid running snapshot is
therefore externally reachable, not merely an internal transient.

`StepTask::run` increments the atomic only on its first poll after its initial
cancellation/deadline check (`workflow.rs:451-464`). Cancellation or deadline
can win after publication and cause `JoinSet::abort_all` before that poll
(`workflow.rs:307-319`). `settle` deliberately leaves an aborted zero-attempt
step as `Running` (`workflow.rs:343-369`), and `RunLedger::finish` then rewrites
every remaining running step to `Unstarted`, clears `started_at`, and forces
attempts back to zero (`run.rs:222-231`). The existing
`bounded_attempt_lifecycle` test directly encodes that contradicted
`Running -> Unstarted` result for a pre-poll abort (`workflow.rs:1457-1507`).
`prepared_run_keeps_its_id` proves transition statuses but does not assert the
attempt invariant (`workflow.rs:788-820`).

This is not permission to add a spawn handshake, acknowledgement, channel,
setting, or lifecycle owner. The smallest source correction stays within the
existing scheduler, ledger, and atomic attempt counter: the scheduling
transition that publishes `Running` must also establish attempt one before the
snapshot is published, while `StepTask` continues to publish later retry
numbers. The existing transition and pre-poll-abort tests are the focused proof
locations: every observed running step must have at least one attempt, and an
aborted published-running step must terminalize as cancelled with retained
timestamps rather than unstarted.

### Proposed Finding

`FOLLOWUP-R3-001` — **INCORRECT**: `WorkflowExecutor` publishes a running step
with zero attempts and can later regress it to unstarted. Observable
consequence: a server GET can return an impossible running snapshot, and an
immediate cancellation/deadline can erase that step's published start. Correct
the attempt-one transition at the existing scheduling/ledger source and update
the two existing focused assertions; add no new mechanism.

## Conflict 2: “Unacknowledged Participant Release”

### Resolution

The concurrency-domain claim is correct, and the phrase is stale DRIFT rather
than an accurate name for a different local condition.

`AnalyticalSupervisor::draining_graphs` counts only local `Draining` entries
whose `settlement_failure` is `Some` (`analytical_supervisor.rs:580-603`). That
failure is populated when the local lifecycle cleanup sequence fails
(`analytical.rs:2777-2783`) or when shutdown cannot join the locally owned
lifecycle task (`analytical_supervisor.rs:1231-1248`). The method has no input,
state field, RPC, or caller representing a follower release acknowledgement.
Its runtime consumer uses the count as one component of local readiness
(`analytical.rs:7208-7220`), and its tests exercise retained local fold,
attempt, lifecycle-task, and graph cleanup failures.

The actual participant-release owner states the opposite of an acknowledgement
contract. `AnalyticalGraphLifecycle` holds participant grant streams until
settlement; dropping them is leader-side release, followers observe stream
close and settle asynchronously, and the leader never waits
(`analytical.rs:2492-2506,2690-2754`). The architecture says the same
(`bifrost-design.md:382-409`). R2 expressly prohibited a release request,
acknowledgement, retry, timer, polling loop, option, setting, probe, or test and
required documentation with no implied acknowledgement
(`TASK-004-R2-align-revision-and-source-contracts.md:96-138`).

Consequently, `analytical_supervisor.rs:584-585` cannot accurately mean a
locally recorded acknowledgement failure: no such acknowledgement exists or is
recorded. The smallest correction is documentation-only: delete that concept
and describe the counted state as a local graph cleanup/settlement failure.
Ordinary asynchronous follower cleanup after grant-stream close is not itself
such a failure. No behavior, check, test, poll, protocol, setting, or option is
needed.

### Proposed Finding

`FOLLOWUP-R3-002` — **DRIFT**: `draining_graphs` still advertises an
“unacknowledged participant release” even though the owner and architecture
define stream close as release and expose no acknowledgement. Replace only the
stale phrase with the local cleanup-failure condition the method actually
counts.

## Conflict 3: Remaining Rust Import-rule Citations

### Resolution

The standards reviewer is correct on all four citations. Each cited symbol is
new or materially changed in the cumulative diff, each is reachable or used by
required task proof, and none falls under an applicable exception in
`architecture/agent-rules.md:9-10`.

| Cited location | Material change and reachability | Rule assessment |
|---|---|---|
| `crates/wyrd-spec/src/card/prompt/mod.rs:264` | The cumulative candidate replaces `raw_body_text -> Option<&str>` with `raw_body -> Option<&serde_json::Value>` for the Revision 13 `RawV1` representation; Prompt codec tests call it. `Value` is already imported at line 35 in the same `prompt_support` module. | The changed return interface must use the available bare `Value`. The `#[cfg(test)]` module exception allows its own top-level imports; it does not allow a qualified interface. |
| `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:310-315,393-396` | R1 adds `cancel_while_opening` and its `later` helper to preserve the original query deadline, with both interfaces using `tokio::time::Instant`. `open_cancellable` and the focused tests call them. | Production and test modules are separate scopes and must import/use a bare `Instant` (or a collision-justified alias). No exception applies. |
| `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs:1795-1811` | The cumulative candidate materially changes `fixture_rows_ipc` to accept managed event time and produce the required deterministic IPC fixture; Scribe ingestion calls it. Its return remains `Result<bytes::Bytes, JourneyError>`. | The materially changed interface must import `Bytes` in this module and use the bare name. External journey/test code is not exempt from the interface rule. |
| `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:2224-2233` | This whole required accepted-run journey file is new. The gateway/external-ownership journey writes a secret file and, on Unix, invokes `Permissions::from_mode` through a function-scoped `PermissionsExt as _` import. | Rule 10's rare local `TraitName as _` exception is limited to a single generic function where module scope is inappropriate. This ordinary, non-generic journey does not qualify. A `#[cfg(unix)]` module-scope trait import is the existing native Rust mechanism. |

These are source-shape violations, not runtime defects, but the rule is explicit
and mandatory. The minimal correction is to use the existing top-level import
blocks and bare names, with `#[cfg(unix)]` on the Unix-only trait import and an
alias only for an actual collision. No scanner, lint, allow attribute, custom
check, file, setting, or option is justified. Recorded passing format/lint
evidence does not enforce or waive this repository rule.

### Proposed Finding

`FOLLOWUP-R3-003` — **VIOLATION**: four materially changed Rust interfaces or
dependency uses bypass their module import block contrary to
`architecture/agent-rules.md:9-10`. Correct the existing imports and type names
only; preserve behavior and add no enforcement mechanism.

## Outcome

**RESOLVED**

All three uncertainties resolve from approved authority and reachable current
source:

1. retain the step-attempt lifecycle finding (`FOLLOWUP-R3-001`);
2. retain the stale Oracle acknowledgement documentation finding
   (`FOLLOWUP-R3-002`); and
3. retain the four-location repository import-rule finding
   (`FOLLOWUP-R3-003`).

No disagreement requires a new product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, resource-ownership, or
persistent-data decision. The human standing decisions remain intact: deleted
Oracle polling and idle refusal stay deleted, follower release remains
grant-stream close without a leader acknowledgement, and the approved
foreign-tenant journey limitation is unchanged.

The candidate was checked again after writing and remained
`f17726fb25df1fa513875dca8d92f0073340ee0a`.

# Lifecycle Domain Review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Domain: accepted-run lifecycle, interruption, cancellation, concurrency, deadlines, stable snapshots, process recovery, and CLI process outcomes
- Overall result: **PASS**

Revision 14 does not alter the accepted-run lifecycle fixed by the approved
earlier revisions. The candidate adds a thin CLI projection and synchronizes
architecture and operations authority without adding another run owner,
polling framework, durable queue, recovery protocol, affinity mechanism, or
process-restart replay path.

## Authority and source coverage

| Boundary | Authority inspected | Source and caller coverage | Assessment |
|---|---|---|---|
| Accepted job and request disconnect | Spec lines 875-924, 1626-1692; REQ-034A/INV-018; AC-018/AC-021 | `workflow/routes.rs`, `workflow/host.rs::WorkflowRunHost::create`, `Preparation::run`, `workflow/runs.rs::{WorkflowRuns,Reservation}`; `pg_workflow_runs::tracked_preparation_replay_and_disconnect` | Preparation and execution are spawned on the process owner and do not borrow the HTTP request. Creator/waiter loss does not cancel or strand the preparation or accepted run. |
| Shared remote lifecycle and lost response | Spec lines 1073-1132; REQ-031/REQ-034C/REQ-046; AC-004/AC-019 | `wyrd_client::workflow::remote::Workflows::{create,get,cancel,wait}`, shared `submit_idempotent`; `workflow_transport::shared_workflow_client_contract`; `pg_workflow_runs::tracked_preparation_replay_and_disconnect` | One key is retained across transport retry; replay returns the existing snapshot. `wait` only polls and dropping it owns no cancellation capability. |
| CLI wait, detach, status, cancel, and interrupt | REQ-026-027; AC-004; TASK-005 Scenario 3 | `wyrd-cli/src/workflow.rs::RunArgs::run_server`, `RunIdArgs::{status,cancel}`, `Report::finished`; `workflow_journey::workflow_server_detach_status_cancel`; `wyrd-cli/src/lib.rs::code_to_u8` | The accepted ID is printed before polling. Detach returns the accepted snapshot. Default wait returns the terminal snapshot. SIGINT drops polling only, reports the same ID, and exits 130. A non-successful completed run exits 2; request/transport failures retain their stable Wyrd error projection. Status and successful idempotent cancellation return 0. |
| Cancellation and terminal races | Spec lines 1694-1731; REQ-019/REQ-030/REQ-048/REQ-050; AC-018/AC-022 | `WorkflowRuns::cancel`, `WorkflowRunHost::cancel`, `AcceptedRun::{observe,finish}`, Skald prepared-run execution; `pg_workflow_runs::lifecycle_races_retention_and_shutdown` | Cancellation is synchronously recorded before the HTTP waiter awaits. The executor drains owned work and publishes one complete terminal snapshot; later observations cannot replace a terminal value. Repeated cancel returns the unchanged terminal run. |
| Deadlines and partial failure | Workflow snapshot invariants; retry/deadline contract; REQ-018-020/REQ-047-050; AC-019/AC-020 | `WorkflowRunHost::timeout`, `Preparation::prepare` run options and cancellation token, Skald executor paths; `pg_workflow_runs::lifecycle_races_retention_and_shutdown` and the recorded TASK-001/TASK-004 focused evidence | Server default/maximum timeout remains owned by the existing typed config. Total deadline and explicit cancellation converge through the same executor-owned abort-and-drain boundary; retained terminal snapshots have no pending/running steps. No TASK-005 change introduces a competing deadline or retry owner. |
| Stable snapshot reads and pinned graph | Spec lines 1694-1742; REQ-029-030/REQ-050; AC-004/AC-019/AC-022 | `PinnedWorkflowGraph::pin`, `RunEntry.snapshot`, `WorkflowRuns::get`, `AcceptedRun::{observe,finish}`; `pg_workflow_runs::{accepted_authority_outlives_submission_only,lifecycle_races_retention_and_shutdown}`; CLI default/status journey | GET clones one complete watch snapshot. The stored graph is prepared before acceptance and is not resolved again by GET. Recorded race evidence reads only whole, non-regressing snapshots and the accepted-authority journey covers mutation after acceptance. |
| Shutdown, restart loss, retention, and affinity | REQ-034A-B; INV-013; AC-018; operations authority | `WorkflowRuns::drain`, `BoundServer` workflow drain call, process-local `RunTable`; `pg_workflow_runs::lifecycle_races_retention_and_shutdown`; `architecture/operations/{reliability-and-recovery,deployment-and-release}.md`; `architecture/wyrd-design.md` | Graceful shutdown closes admission, signals, and drains tracked work within the shared deadline. Restart constructs an empty owner and neither resumes nor retries provider work. Architecture now explicitly requires gateway affinity for create/replay/get/cancel and states that a non-owning replica returns not found. No bespoke in-repository affinity or recovery mechanism was added. |

## End-to-end lifecycle trace

1. `wyrd workflow run --execution server` validates the registered selector and
   input, builds the existing shared client, and calls `Workflows::create` once.
2. The shared transport mints one idempotency key outside its retry loop. The
   server authorizes and audits, reserves bounded process-local capacity, and
   gives a tracked `Preparation` ownership independent of the request future.
3. Atomic promotion publishes the queued snapshot, run token, run ownership,
   and idempotency entry. The CLI then prints the returned run ID before either
   detaching or entering `Workflows::wait`.
4. A dropped HTTP creator, dropped replay waiter, dropped `wait` future, or CLI
   SIGINT removes only that waiter. It neither signals the run token nor submits
   a second create. A lost create response can be replayed under the same key
   without another provider execution.
5. GET returns the owning process's current complete snapshot. Cancel records
   the token signal before awaiting terminalization. Completion, cancellation,
   total deadline, and shutdown converge at the existing executor/terminal
   boundary; the first terminal snapshot remains authoritative.
6. Graceful shutdown drains the tracked owner. Abrupt process loss or restart
   loses runs and idempotency entries. The documented deployment contract,
   rather than new application machinery, supplies replica affinity.

## Material findings

None.

No reachable lifecycle defect, regression, or unsupported bespoke lifecycle
mechanism was found in the cumulative candidate. In particular, the candidate
does not claim durable or cross-replica behavior and does not add a configurable
poller, scheduler, persisted run registry, ownership-transfer protocol, or
restart-recovery option.

The five findings deliberately deferred in
`changes/active/skald-workflow-runtime/review/TASK-004-r6/lead-disposition.md`
were treated as standing human dispositions and were not reopened as TASK-005
findings.

## Verification limits

- Per the review instruction, no build, test, server, or CLI command was run.
- The review relied on candidate source and recorded evidence. TASK-005 records
  a green `mise run gate`, all three exact ignored CLI journey commands, and a
  clean `git diff --check`. Earlier cumulative evidence names the focused
  shared-client and server lifecycle tests above.
- The CLI journey directly proves compiled-binary wait, detach, status,
  idempotent cancel, SIGINT exit 130, continued execution, and no resubmission.
  Lost-response replay, request-disconnect ownership, deadline races, whole
  snapshots, shutdown, eviction, and restart loss remain covered by the real
  server/client journeys already present in the cumulative candidate rather
  than duplicated in the CLI test.
- Replica affinity is intentionally deployment-owned and is proved here by
  synchronized architecture/operations authority, not by an in-repository load
  balancer. This matches the approved V1 boundary and is not a verification
  limitation.

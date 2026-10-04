# TASK-004 R2 Concurrency and Lifecycle Domain Review

## Result

**PASS**

The cumulative candidate closes the prior concurrency/lifecycle findings. I found
no remaining material concurrency, ownership, capacity, terminalization,
retention, or shutdown defect in the reviewed boundary, and no unsupported
mechanism that qualifies as DRIFT under the standing direction.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Prior validated findings reviewed here: `FIND-TASK-004-1`, `FIND-TASK-004-2`,
  `FIND-TASK-004-3`, `FIND-TASK-004-4`, `FIND-TASK-004-9`,
  `FIND-TASK-004-12`, and the concurrency/lifecycle part of
  `FIND-TASK-004-13`

The candidate commit remained unchanged while this report was prepared.

## Reviewed boundary

- Preparation reservation visibility, idempotency waiters, active-slot transfer,
  and exact-once reservation cleanup.
- Workflow `TaskTracker` ownership of preparation, blocking graph construction,
  accepted execution, cancellation, and shutdown drain.
- Whole-snapshot publication, terminal winner behavior, active-capacity release,
  retention expiry, and tenant/global oldest-terminal eviction.
- Run-total deadline and cancellation interaction with Skald task abort-and-drain.
- `bifrost.query` owner transfer, waiter-drop cancellation, original-query
  deadline, Oracle cancellation/settlement, and Workflow terminal ordering.
- Production shutdown ordering through `BoundServer::run`.
- The real-server lifecycle, idempotency, graph-bound, and forwarded-query
  journeys that record proof for these paths.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Assessment |
|---|---|---|---|
| Accepted preparation ownership | Spec Revision 13, REQ-034C/050, INV-018/019, AC-021; remediation findings 1-2 | `components/workflow/runs.rs:252-365,415-431,530-636`; `host.rs:60-123,230-400`; HTTP create caller in `components/workflow/routes.rs` | PASS |
| Run execution and terminal publication | REQ-018-023/030/034A/045/048/050, INV-013/018/019/021, AC-018/020/022 | `host.rs:244-288`; `runs.rs:638-688`; Skald `workflow.rs:140-333,335-383,435-539`; `run.rs:31-351` | PASS |
| Capacity and retention | Server run owner/capacity contract, REQ-034A/034C/050, INV-013/018/023 | `runs.rs:95-204,269-342,367-431,563-688`; Scenario 2 and Scenario 6 in `tests/pg_workflow_runs.rs` | PASS |
| Query-owner lifetime | TASK-004 Query ownership across step abort; REQ-048/052; INV-019/021; AC-020/025 | `components/workflow/tools.rs:42-107,139-215`; `query/collect.rs:253-400`; `oracle/lifecycle_controls.rs:33-161,300-333`; scheduled-query callers | PASS |
| Shutdown | Normative shutdown flow, REQ-034A/048/050, INV-018/019/021 | `app/server.rs:744-760`; `runs.rs:415-431`; `host.rs:255-288`; Scenario 2 and Scenario 6 | PASS |
| Deep preparation and snapshot pressure | Server graph/capacity contract, REQ-017/045/050, INV-019/023, AC-028 | tracked blocking preparation in `host.rs:290-400`; Scenario 7 in `tests/pg_workflow_runs.rs:2830-3090` | PASS |
| Forwarded follower/leader cleanup | TASK-004 query settlement contract plus the standing human decision | `oracle/query_stream.rs` settlement ownership; forwarded journey in `wyrd-testing/tests/bifrost/oracle/workflow.rs`; current source keeps follower release at grant-stream close and does not restore leader acknowledgement, graph-reserved-byte polling, or supervisor idle refusal | PASS |

Applicable repository authority reviewed included `AGENTS.md`,
`architecture/agent-rules.md`, the spec-driven development, Rust, testing, and
reliability references, and the task/spec sections governing accepted-job and
query lifecycles. The Revision 13 provider tag changes do not alter this
domain's concurrency ownership.

## Lifecycle assessment

### Reservation and tracker visibility

`WorkflowRuns::admit` holds the run-table lock while it checks shutdown,
charges global and tenant capacity, installs the preparing key, and constructs
the reservation's `TaskTrackerToken` (`runs.rs:269-341`). `drain` takes the same
lock before cancelling admission and only then closes and waits on the tracker
(`runs.rs:422-430`). Therefore shutdown cannot observe a preparing key without
an owner that keeps the drain open. A shutdown-winning create publishes no
reservation; a reservation that loses promotion releases its key/capacity and
wakes every waiter through the existing single `Reservation::release` path.

The token is a standard capability of the already-used Tokio task tracker, not
a second queue, scheduler, actor, lifecycle service, or repository-specific
check. It is the smallest owner for the formerly uncovered interval between
reservation publication and async task spawn.

### Blocking preparation ownership

Pure hydration/Skald preparation uses `WorkflowRuns::spawn_blocking`, which is
the same tracker shutdown drains (`runs.rs:353-365`, `host.rs:382-398`). If
shutdown cancellation drops the awaiting preparation future, Tokio may continue
the blocking closure, but the tracker retains it until return. The reservation
itself is cancelled and cannot promote after shutdown. This closes the prior
detached-work race without adding an executor or accepting work after admission
closes.

### Execution, snapshots, terminalization, and capacity

The accepted executor is one tracked task. Skald owns its bounded `JoinSet`,
checks cancellation/deadline before scheduling, aborts all active steps when
either wins, joins them, classifies interrupted attempts, and constructs one
complete terminal snapshot. The server observer publishes only complete
non-terminal snapshots; `AcceptedRun::finish` replaces a non-terminal snapshot
once, records retention time, releases the tenant/global active slot, and
applies retention (`runs.rs:650-688`). GET reads a complete watch value, and
cancel records the token before awaiting the terminal watch value. No state
lock is held across IO or drain.

The retained-run sweep excludes active entries, deletes the matching
idempotency key with each terminal eviction, applies the per-tenant limit before
the global oldest-terminal limit, and lazily expires at 24 hours. The Scenario 6
journey records completion/cancel and completion/deadline races, whole snapshots,
cross-tenant global eviction, queued and running shutdown, restart loss, and
post-shutdown terminal inspection.

### Query owner and deadline

`QueryTool::invoke` transfers an admitted query to the run's tool-owner
`TaskTracker` before awaiting it and uses a cancellation drop guard so an
aborted Agent waiter signals the owner (`tools.rs:171-215`). The Workflow does
not terminalize or release capacity until `RunTools::drain` closes and joins all
such owners (`host.rs:285-287`). The owner retains the stream and invokes the
existing Oracle lifecycle controls on all pre-terminal error/cancellation paths.

Cancel-during-open now fixes the query deadline before polling the open and
bounds the one owner-cancel request plus the same open future by that deadline
(`lifecycle_controls.rs:141-160,300-333`). After the stream opens,
`cancel_and_settle` derives the remaining time from the stream's absolute
deadline. No fresh cleanup timeout, retry, or Workflow-owned gateway wait was
introduced. The held-follower decision remains as directed: stream close
releases the follower graph; the leader does not wait for a follower release
ack, and journeys wait for observable follower cleanup.

### Shutdown ordering

`BoundServer::run` creates one shutdown deadline, drains Workflow work first
while gateway/query/Bifrost dependencies remain available, and passes the same
remaining deadline into later service shutdown (`app/server.rs:744-760`).
Workflow drain closes admission, signals preparation/run tokens, closes the
single tracker, and waits on preparations, blocking work, executors, and
reservation tokens. Query owners are joined before Workflow terminal commit;
gateway settlement remains under the gateway owner, as required.

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-004-1` reservation can outrun tracking | Reservation obtains a tracker token under the admission lock; drain closes admission under that lock and waits on tokens. Narrow recorded unit proof: `drain_waits_for_a_reservation_before_its_task_exists`. | CLOSED |
| `FIND-TASK-004-2` blocking preparation can detach | Graph hydration/preparation uses tracked `spawn_blocking`; cancellation prevents promotion while drain continues to count the closure. Narrow recorded unit proof: `drain_waits_for_blocking_work_its_caller_abandoned`. | CLOSED |
| `FIND-TASK-004-3` cancel-during-open can exceed original deadline | `open_cancellable` and `cancel_while_opening` share one pre-open deadline across the owner signal and original open. Recorded proof: `cancel_while_opening_ends_at_the_original_deadline`. | CLOSED |
| `FIND-TASK-004-4` pod-loss proof omits recovery | Forwarded journey asserts the exact failure class, no rows/model result, surviving-owner baselines, membership convergence, and a later successful query. | CLOSED |
| `FIND-TASK-004-9` preparation/idempotency journey incomplete | Scenario 2 covers same-key sharing, principal and tenant scope, changed-request conflict, failed preparation, creator/waiter disconnect, lost response recovery, and shutdown wakeup with exact call counts. | CLOSED |
| `FIND-TASK-004-12` lifecycle journey incomplete | Scenario 6 covers required terminal races, complete concurrent reads, global/per-tenant retention, queued/running/preparing shutdown, restart loss, and post-shutdown snapshots. | CLOSED |
| `FIND-TASK-004-13` deep/bounded lifecycle proof incomplete | Scenario 7 covers held preparation with Cards/gateway/Bifrost siblings, aggregate overflow, near-ceiling cancel/deadline terminalization, a 1,024-step graph, complete cancellation, and released capacity. | CLOSED |

## Material findings

None.

The added reservation token, tracked blocking call, existing test-support gate
extensions, and focused owner tests are established Tokio/repository testing
patterns that directly prove required races. They do not introduce a bespoke
production mechanism, option, setting, or permanent repository check. The
candidate also keeps the explicitly rejected Oracle graph-drain polling and
supervisor idle refusal deleted.

## Verification limits

- Per instruction, I ran no build, test, Cargo, or mise command.
- I treated recorded results as evidence claims and inspected the named test
  bodies and production call paths to confirm that they exercise the claimed
  concurrency/lifecycle boundaries.
- Recorded focused evidence includes the two Workflow owner tests, the
  cancel-during-open unit test, TASK-004 Scenarios 2, 6, and 7, and the
  forwarded Oracle Workflow journey. Recorded broader evidence includes
  `test:wyrd`, `test:shared`, `test:bifrost`/the subsequent server journey,
  principals lanes, gateway journey, codegen, client-tier, tenant isolation,
  unwrap audit, formatting, lints, and `git diff --check`.
- The approved foreign-tenant harness limitation does not weaken this domain:
  its lifecycle/idempotency isolation assertions do not depend on a foreign
  model step succeeding.

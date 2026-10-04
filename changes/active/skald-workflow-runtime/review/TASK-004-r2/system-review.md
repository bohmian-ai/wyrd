# System-resilience review — TASK-004 remediation R2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Review mode: complete cumulative diff, candidate source, applicable architecture, and recorded evidence only. No build, test, Cargo, or mise command was run.

The candidate identity was checked before and after this review and remained unchanged.

## Deployed topology

The changed runtime remains one shared `wyrd-server` process with these relevant owners:

- `WorkflowRuns` owns process-local Workflow admission, reservations, active and retained snapshots, cancellation, and tracked preparation/executor work.
- `WorkflowRunHost` performs fresh request authorization/audit, pins the exact Cards graph, composes server execution dependencies, and hands the accepted run to the existing Skald executor.
- `GatewayInvocation` remains the owner of provider/model authorization, live deployment and credential eligibility, accounting, capture, and post-response settlement.
- `RunTools` binds accepted authority to Cards and Bifrost service owners; Bifrost query calls are retained by a run-local task tracker until Oracle settlement finishes.
- Oracle leaders retain their local analytical graph, attempts, exchanges, running-query owner, and admission envelope. Followers release their graphs after the leader closes each participant grant stream; the leader does not wait for a follower release acknowledgment.
- `BoundServer` applies one absolute shutdown deadline across Workflow drain, transport supervision, MCP/gateway tracking, capture, and Bifrost shutdown.

The candidate adds no durable Workflow queue, recovery lease, cross-replica owner, secondary executor, second query engine, or new cleanup service.

## Deployed-path and recovery assessment

| Runtime path | Ownership and failure trace | Recovery / availability result | Assessment |
|---|---|---|---|
| Create, replay, and request disconnect | `WorkflowRunHost::create` authorizes and audits, then `WorkflowRuns::admit` installs a scoped reservation. The reservation now takes a native `TaskTrackerToken` while the run-table lock is held (`runs.rs:269-341`), before the handler can spawn or suspend. The HTTP future waits only on the published outcome (`host.rs:75-123`). | Dropping the creator or one replay waiter does not cancel preparation. Shutdown cannot report the Workflow tracker drained while an observable reservation still exists; reservation drop/failure releases its key and capacity and wakes all waiters (`runs.rs:609-635`). | PASS |
| Deep graph preparation and cancellation | Registry graph pinning is async IO outside the run-state lock. Synchronous hydration and Skald preparation use `WorkflowRuns::spawn_blocking`, which registers the blocking job on the same tracker (`runs.rs:353-365`, `host.rs:382-399`). | If the outer preparation future is cancelled, already-started blocking work remains counted until it returns. A clean shutdown therefore cannot detach CPU preparation and proceed while it is still active. The recorded owner test directly holds and releases this work. | PASS |
| Accepted execution, cancel, and deadline | The accepted reservation transfers its cancellation token and active slot into the run entry; Skald receives the same token and bounded total deadline. Non-terminal snapshots are replaced whole, query owners drain, and only then does `AcceptedRun::finish` commit terminal state and release the active slot (`host.rs:276-287`, `runs.rs:650-688`). | Explicit cancel, total deadline, and shutdown converge through the existing Skald abort/drain path. A cancel request records the signal synchronously before awaiting the terminal snapshot. Already-issued gateway settlement remains under the gateway tracker and cannot mutate a terminal Workflow snapshot. | PASS |
| In-process provider dialect and live gateway dependency | Revision 13 uses serde adjacent tagging for `ProviderRequest`; Vertex and Gemini bodies no longer depend on shape-order inference (`skald-spec/src/request.rs:18-59`). `ServerWyrdGatewayCaller` projects each tagged request to its matching in-process ingress and response decoder (`gateway/workflow.rs:193-250`). | A saved Vertex request remains Vertex after registry round-trip and reaches the Vertex gateway path. Dependency refusal, timeout, fallback, cancellation, accounting, and capture remain owned by the existing gateway pipeline; the Workflow host introduces no retry or credential cache. | PASS |
| Built-in Bifrost tool and cancellation during stream open | `QueryTool` spawns `BoundedQuery` under the run-local owner tracker. `RunningQueryControls::open_cancellable` fixes one deadline from the original request before cancellation races the open; after cancellation it routes one owner cancel and awaits the same open only until that fixed instant (`lifecycle_controls.rs:118-160,300-332`). | Cancellation no longer starts a fresh cleanup budget. Once a stream exists, collection requires a valid terminal and clean EOF; error paths invoke `cancel_and_settle` under the stream's original absolute deadline. The Workflow terminal waits for the query owner unless the Workflow's own deadline wins as explicitly approved. | PASS |
| Oracle follower loss and asynchronous follower release | Leader settlement joins its own attempt and exchanges, drops participant grants, then releases its own graph and running-query owner (`analytical.rs:2690-2774`). Dropping each grant closes the follower stream; follower-local lifecycle then cancels and frees its graph. | This is the approved protocol boundary. The leader does not await a follower release acknowledgment. Cleanup assertions wait on the existing follower ownership state before claiming cluster cleanup; pod-loss evidence waits for membership to exclude the dead follower, refreshes the cut, and proves a later query succeeds. No polling/refusal mechanism was restored. | PASS |
| Server shutdown and shared-process availability | `BoundServer` fixes one deadline, drains Workflows first while gateway/query/Bifrost dependencies are still available, then supervises transports and drains MCP/gateway/capture and Bifrost within the remaining budget (`app/server.rs:744-895`). `WorkflowRuns::drain` closes admission under the same run-table lock used by admission, cancels the shared token, closes the tracker, and waits until the absolute deadline (`runs.rs:415-431`). | New creates lose the admission race with 503 or already own a tracker token. Active/queued runs terminalize when possible. A missed Workflow drain becomes a process shutdown error; the process does not claim clean shutdown. Subsequent Bifrost shutdown uses the same deadline and falls back to awaited abort on exhaustion. | PASS |
| Process or pod restart | Workflow state, idempotency keys, captured authority, and terminal snapshots exist only in `WorkflowRuns`. No persistence or replica lookup was added. | Pod loss intentionally loses queued/running/retained entries; a non-owning replica returns the common not-found result. Deployment affinity is still required. No provider call is resumed or automatically retried after restart. | PASS |
| Dependency outage | Cards/Postgres failure during pre-acceptance graph pinning returns a structured preparation error, removes the reservation, and creates no run. Missing Oracle controls, unavailable gateway deployment/credential, provider refusal, and external-gateway failure retain their owner-specific typed errors. | Failure remains scoped to the run or request. Other Cards, gateway, and Bifrost callers remain available; recorded Scenario 7 exercises sibling serviceability during held/deep preparation and near-ceiling terminal paths. | PASS |
| Retention pressure | Active and preparing entries hold active capacity and are never eviction candidates. Terminal sweep first expires by fixed 24-hour retention, then applies per-tenant oldest-first and global oldest-first ceilings, deleting the paired idempotency key (`runs.rs:157-190`). | Capacity pressure cannot evict active work. Retention is process-local and restart clears it, as approved. | PASS |

## Failure-path evidence

- Reservation/shutdown race: `WorkflowRuns::admit` acquires the tracker token before returning the reservation, while `WorkflowRuns::drain` cancels admission under the same lock before closing the tracker. The recorded focused test `components::workflow::runs::tests::drain_waits_for_a_reservation_before_its_task_exists` exercises the previously open interval.
- Detached blocking work: `WorkflowRuns::spawn_blocking` uses the same `TaskTracker` and the recorded `drain_waits_for_blocking_work_its_caller_abandoned` test holds the blocking closure after its awaiting task is cancelled.
- Cancel during query open: `cancel_while_opening` wraps both owner cancellation and the retained open in `timeout_at(deadline, ...)`; the recorded `cancel_while_opening_ends_at_the_original_deadline` test verifies no new budget is introduced.
- Forwarded query owner loss: the recorded `workflow_forwarded_query_settles_before_the_run_ends` journey covers cancel, deadline, and follower pod loss; it checks exact terminal metrics, no partial rows, survivor ownership recovery, membership convergence after pod loss, and a later successful query.
- Workflow shutdown races: the recorded `lifecycle_races_retention_and_shutdown` journey covers accepted queued/running work, concurrent terminal races, retained-state behavior, and post-shutdown complete terminal snapshots.
- Shared-service survival: the recorded `graph_and_snapshot_limits_preserve_sibling_services` journey exercises a deep admissible graph, held preparation, aggregate and terminal bounds, and continued Cards, gateway, and Bifrost availability.
- Provider routing: the recorded `server_routes_keep_gateway_and_external_ownership` journey drives OpenAI Chat, OpenAI Responses, Anthropic, Gemini, and Vertex paths, including Vertex's provider-tagged round-trip, operation refusal, fallback isolation, deadline, and cancellation.

## Prior-finding closure

| Prior finding | Closure assessment |
|---|---|
| `FIND-TASK-004-1` reservation publication could outrun tracker ownership | CLOSED. The reservation itself owns a `TaskTrackerToken` acquired before publication; shutdown cannot drain across the pre-spawn interval. |
| `FIND-TASK-004-2` blocking preparation detached from Workflow shutdown | CLOSED. Blocking preparation is spawned through the Workflow tracker and remains counted after outer cancellation. |
| `FIND-TASK-004-3` cancel-during-open could exceed the original query deadline | CLOSED. The request deadline is fixed before open and bounds owner cancellation plus the retained open. |
| `FIND-TASK-004-4` pod-loss proof omitted exact recovery | CLOSED for system resilience. The journey verifies exact failed outcome, no rows, surviving-owner baselines, membership convergence, and a later successful query. |
| `FIND-TASK-004-11` in-process protocol/deadline/cancellation proof was incomplete | CLOSED for deployed behavior. The recorded server journey now covers every supported in-process dialect, provider-tagged Vertex, operation refusal, concurrent fallback isolation, deadline, and cancellation independence. |
| `FIND-TASK-004-12` lifecycle/shutdown proof was incomplete | CLOSED for system resilience. The recorded lifecycle journey covers queued shutdown and the required completion/deadline/snapshot/eviction races. |
| `FIND-TASK-004-13` graph/snapshot serviceability proof was incomplete | CLOSED for system resilience. The recorded graph journey covers deep admission, sibling Bifrost traffic, aggregate overflow, near-ceiling terminal paths, and later serviceability. |

## Standing DRIFT assessment

No material DRIFT was found.

- The reservation handoff uses Tokio's existing `TaskTrackerToken`, a native ownership primitive of the already-installed tracker, rather than a new queue, actor, state machine, setting, or repository check.
- Blocking preparation uses the same existing tracker; no CPU scheduler or separate runtime was introduced.
- Query-open cancellation uses the ordinary absolute-deadline `timeout_at` pattern and existing lifecycle controls.
- Provider discrimination uses standard serde adjacent tagging rather than a bespoke shape-ordered parser.
- The deleted Oracle graph-drain byte polling and supervisor idle refusal remain deleted. The candidate adds no follower release acknowledgment; the grant-stream close remains the release, and test assertions wait for follower-local cleanup.
- Pod-loss and follower-cleanup tests use observable condition waits and existing ownership/membership probes, not synthetic load, sleeps as correctness, or new production mechanisms.

## Material proposed findings

None.

The approved non-durable restart boundary, deployment-affinity requirement, lack of leader acknowledgment for follower cleanup, and inability of the foreign-tenant journey fixture to execute model steps are explicit approved constraints, not system-resilience defects.

## Verification assessment and limits

The remediation records passing focused owner tests, all seven server Workflow scenarios, the forwarded Oracle journey, the complete Bifrost lanes, Wyrd/shared/principal/gateway lanes, Python and TypeScript lanes, code generation, boundary checks, format, and lints. Those results are credible for the changed deployed paths based on inspection of the named tests and their assertions.

Per the review instruction, none of those commands was rerun. This review therefore relies on the immutable diff and recorded evidence. No required source, authority, or recorded result needed for the system-resilience assessment was unavailable.

## Overall result

**PASS**

No material process-crash, cancellation, dependency-loss, deadline, rolling-replacement, recovery, or shared-service-availability finding remains in this review scope.

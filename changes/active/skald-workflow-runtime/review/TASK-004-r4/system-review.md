# TASK-004 R4 System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md`,
  `TASK-004-R2-align-revision-and-source-contracts.md`, and
  `TASK-004-R3-close-step-attempt-and-import-gaps.md`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Review scope: complete cumulative base-to-candidate runtime and deployment
  effect, with the R3 delta checked at its shared Skald lifecycle boundary

The candidate remained `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
through this review. `.codegraph/` is absent, so navigation used immutable Git
objects, repository source, callers, and recorded evidence. Per the review
constraint, no build, test, Cargo, or mise command was run.

## Deployed topology and changed paths

| Path | Process/service owner and dependencies | System effect |
|---|---|---|
| Create and replay | `wyrd-server` HTTP handler -> `WorkflowRunHost` -> process-local `WorkflowRuns` -> registry/Postgres graph pinning -> Skald preparation | Authentication and transactional audit precede admission. One reservation owns the scoped idempotency key, tenant/global slot, cancellation child, and tracker token before request suspension (`host.rs:75-118`, `runs.rs:258-343`). Request loss drops only a waiter; the tracked preparation remains process-owned. |
| Accepted execution | `Preparation` -> `PreparedWorkflowRun::execute` -> `AcceptedRun` snapshots | Promotion replaces the reservation with one queued run and key under the state lock, then publishes acceptance (`runs.rs:563-606`). Skald transitions replace complete snapshots; terminal publication is withheld until run-owned read-tool work drains (`host.rs:247-287`, `runs.rs:650-687`). |
| Model calls | Skald step -> `ServerWyrdGatewayCaller` -> existing gateway invocation owner | Workflow cancellation is separate from gateway-owned accounting/capture/audit settlement. Workflow shutdown does not claim or await a follower-style gateway acknowledgement. |
| Cards and Bifrost tools | Agent tool -> captured caller -> Cards exact-ref owner or tracked `BoundedQuery` -> Oracle controls | Query work runs on the run's `TaskTracker`; waiter drop synchronously cancels the child token while the owner retains and settles the response (`tools.rs:171-214`). `RunTools::drain` closes and joins those owners before terminal publication (`tools.rs:85-91`; `host.rs:284-287`). |
| Distributed query cleanup | Workflow query owner -> `RunningQueryControls::{open_cancellable,cancel_and_settle}` -> local or authenticated remote Oracle owner -> Analytical graph | Open cancellation and subsequent stream settlement share the query's original deadline. A valid failed terminal may end settlement with residue; success requires clean EOF. Grant-stream close remains follower release, and the leader does not await a follower release acknowledgement. |
| Shutdown | `BoundServer::run` -> Workflow drain -> remaining server/Bifrost supervision -> MCP/gateway trackers | One deadline is fixed once. Workflow admission closes and its tracked work drains first while query and gateway dependencies remain available; the same deadline is passed through all later drains (`app/server.rs:744-814`). A missed Workflow drain becomes a process shutdown failure rather than a false clean result. |
| Restart or non-owning replica | Deployment routing -> process-local `WorkflowRuns` | Runs and idempotency entries deliberately do not survive process loss. A restarted or non-owning replica returns the common not-found response; V1 requires deployment affinity and does not resume provider calls. |

The shared process also serves Cards, gateway, Bifrost, MCP, and other Wyrd
capabilities. The Workflow owner neither closes those sibling services early
nor holds its state lock across their IO. Graph preparation uses the shared
tracked blocking boundary (`runs.rs:355-367`; `host.rs:296-397`), while active,
retained, graph, input, step-result, and aggregate-run bounds limit the new
process-local work.

## Failure and recovery assessment

| Credible failure or recovery path | Source evidence and observable result | Proof assessment | Result |
|---|---|---|---|
| Creator disconnect, matching waiter cancellation, or lost acceptance response | The reservation and preparation are detached from HTTP lifetime; matching callers share the watch outcome, and replay reads the accepted entry (`runs.rs:281-343`, `host.rs:99-118`). The production-shaped journey aborts the creator and a waiter independently, then recovers a lost answer with one run and one execution (`pg_workflow_runs.rs:1544-1617`). | Recorded R1 evidence reports the server Workflow journeys green. The assertions count upstream work and run identity, not merely equivalent output. | PASS |
| Preparation failure, cancellation, or shutdown before acceptance | `Reservation::release` removes the key and active slot exactly once, publishes the structured failure, and `Drop` covers cancellation/unwind (`runs.rs:609-635`). `drain` closes admission under the same state lock used by `admit`, cancels children, closes the tracker, and waits to the shared deadline (`runs.rs:417-433`). The journey holds preparation, joins two waiters, shuts down, expects three unavailable responses, and observes no provider work (`pg_workflow_runs.rs:1646-1679`). | Direct source and recorded owner/journey evidence cover the previously vulnerable reservation-before-spawn interval. | PASS |
| Process crash, restart, or rolling replacement | Process-local memory is intentionally lost. No durable retry, resume, or transfer path exists. The restart journey verifies the old run is not found and its calls do not resume (`pg_workflow_runs.rs:2710-2732`). A non-owning replica follows the same common 404 contract; deployment affinity is the approved operational requirement. | This is an explicit availability boundary, not missing recovery. No new lease, queue, cross-replica lookup, setting, or compatibility path is warranted. | PASS |
| Explicit cancel or total deadline while steps are running | The Skald executor signals/aborts its `JoinSet`, joins each task, settles it, and only then terminalizes (`workflow.rs:245-333`). The R3 correction reserves attempt one before publishing `Running` (`workflow.rs:273-290`; `run.rs:125-136`), and both an interrupted report and a pre-poll abort settle `Cancelled` with that attempt (`workflow.rs:336-365`). `RunLedger::finish` now rewrites only never-started `Pending` steps (`run.rs:221-250`). | The focused source test asserts every published running step has attempt one (`workflow.rs:785-823`), and the pre-poll branch asserts `Cancelled`, attempt one, and both timestamps (`workflow.rs:1459-1510`). Recorded R3 commands report the focused tests, Skald family, Wyrd server journeys, and Oracle journey green. | PASS |
| Tool waiter drop, step abort, ordinary tool error, or query deadline | `QueryTool` creates a child cancellation token whose drop guard signals synchronously; the tracked owner keeps `BoundedQuery::run` alive (`tools.rs:171-214`). Collection errors call `cancel_and_settle` before returning (`query/collect.rs:374-399`), and open cancellation retains the original open under its original deadline (`oracle/lifecycle_controls.rs:119-163`). The Workflow terminal is committed only after `RunTools::drain`. | The recorded server journey covers held query cancellation and later serviceability. The forwarded real-cluster journey holds cleanup and proves neither the run nor model advances before settlement (`wyrd-testing/.../oracle/workflow.rs:151-203`). | PASS |
| Remote Oracle follower loss or transport loss after selection | The selected query fails terminally without an interactive rerun. The forwarded journey kills the held follower, verifies the model receives the stable failure without columns, checks one failed metric and no success, returns surviving owners to baseline, waits for membership departure, and succeeds on a later query (`wyrd-testing/.../oracle/workflow.rs:224-267`). | Recorded R1 evidence reports the Oracle journey green. Waiting for membership departure is test recovery synchronization around the existing lease TTL, not a production retry or new lifecycle mechanism. | PASS |
| Follower graph release | The candidate documents and preserves participant grant-stream close as leader-side release; the follower asynchronously cancels and frees its graph. The leader does not await an acknowledgement. | This matches the fixed human decision and Bifrost architecture. Deleted graph-drain polling and supervisor idle refusal remain deleted; no replacement poll, acknowledgement, retry, setting, or check was introduced. | PASS |
| Gateway outage, refusal, timeout, or cancellation | Each step uses the existing gateway owner with current deployment, credential, capability, limit, and audit decisions. A refused call fails the affected step/run; separate runs and sibling services remain available. Gateway accounting/capture stays gateway-owned and drains after Workflow work under the same process deadline (`app/server.rs:800-814`). | Recorded scenario 5 evidence covers dialect/capability refusal, fallback isolation, run deadline, cancellation isolation, direct external routing, and redaction. | PASS |
| Shutdown with preparing, queued, running, or query-owning work | Workflow cancellation happens before dependent gateway/query/Bifrost teardown (`app/server.rs:749-784`). The lifecycle journey holds all three Workflow phases and requires complete cancelled snapshots for accepted work (`pg_workflow_runs.rs:2767-2822`). The remote query journey covers cancellation/deadline while distributed cleanup is held. | The same absolute deadline may expire; in that case the server reports an unclean shutdown and does not fabricate settlement. This is the approved process-level failure boundary. | PASS |
| Dependency outage during graph pinning or accepted execution | Registry/audit failure prevents acceptance or releases the reservation; provider, gateway, Cards, or Bifrost failure is bounded to the request, step, or run according to the owning service. No failure path intentionally crashes the shared server or closes unrelated services. The graph-bound journey holds preparation while Cards, gateway, and Bifrost answer and later proves serviceability after near-ceiling terminals. | Recorded scenario 1, 4, 5, and 7 evidence exercises refusal before side effects, complete-result integrity, and sibling responsiveness. | PASS |

## Affected capabilities and recovery boundary

- Workflow create/replay/get/cancel: process-local availability with bounded
  admission and common not-found behavior after restart, eviction, expiry, or
  replica mismatch.
- Skald execution: cancellation and deadline stop new scheduling, abort and join
  active work, preserve complete snapshots, and now preserve the accepted
  attempt-one identity even when abort wins before the step future's first poll.
- Gateway: current live policy and credentials still apply per invocation;
  Workflow cancellation does not assume ownership of gateway settlement.
- Cards and Bifrost: each call retains its own authorization/audit and service
  bounds. A Workflow failure does not remove their availability.
- Oracle Analytical execution: one selected attempt, terminal-safe results,
  original-deadline cancellation, and honest incomplete/unavailable outcomes.
  Follower cleanup is asynchronous after grant-stream close as approved.
- Shared `wyrd-server`: a Workflow drain failure can make the process shutdown
  unclean, but does not authorize crashing the process on an individual run or
  falsely reporting successful cleanup.

## Verification assessment and limits

The cumulative task evidence records passing focused Workflow owner tests,
all `pg_workflow_runs` journeys, Skald tests, Wyrd server tests, gateway
journeys, and the serialized Oracle journey. R1 records broad Wyrd, Skald,
shared, principal, gateway, Python, TypeScript, and all Bifrost lanes green.
R3 records the two exact Skald lifecycle tests, `test:skald`, 1,591 Wyrd tests,
and 43 Oracle journey tests green after the attempt reservation correction.

This review did not re-execute those results. Static inspection confirms that
the named tests still exercise the production owners and that R3 changed only
the lifecycle source and assertions needed to make published `Running` reserve
attempt one; it did not change the server shutdown, query ownership, gateway,
Oracle, restart, or durability boundaries. The recorded evidence is therefore
credible for this review's failure paths.

The approved foreign-tenant harness limitation remains: the fixture provisions
gateway credentials only for its fixture tenant, so foreign-tenant assertions
prove idempotency, lookup, Cards, query, authorization, and tenant boundaries
without executing a foreign model step. That does not weaken the system
recovery claims reviewed here.

## Material proposed findings

None. No reachable system-resilience regression, missing approved recovery
behavior, or nonstandard mechanism was found. In particular, this review does
not request Oracle graph polling, supervisor idle refusal, a follower release
acknowledgement, durable Workflow recovery, a second-tenant credential harness,
or any new check, file, setting, or option.

## Overall result

**PASS**


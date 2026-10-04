# System-resilience review — TASK-004 R5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in the preceding review directories
- Review mode: complete cumulative diff, current source, applicable architecture,
  prior validated findings, and recorded verification only. No build, test,
  Cargo, or mise command was run.

The candidate resolved to the requested commit before and after source review.
`.codegraph/` is absent, so immutable Git objects, `rg`, and direct source
inspection were used.

## Deployed topology and changed-path coverage

The relevant deployment remains one shared `wyrd-server` process. `WorkflowRuns`
owns process-local reservations, retained snapshots, cancellation, and tracked
preparation/execution; Skald owns the prepared executor and its total deadline;
gateway settlement remains on the gateway tracker; `RunTools` owns accepted-run
Cards and query adapters; `BoundedQuery` and `RunningQueryControls` retain and
settle Oracle responses; and `BoundServer` applies one process shutdown deadline
across those owners.

| Runtime path | Ownership and failure trace | Recovery and sibling impact | Result |
|---|---|---|---|
| Create, replay, and preparation | HTTP create -> fresh authorization/audit -> `WorkflowRuns::admit` -> reservation with tracker token -> Cards graph pinning -> tracked blocking Skald preparation -> atomic `Reservation::accept` (`components/workflow/host.rs:75-123,244-287,309-400`; `components/workflow/runs.rs:258-342,552-635`) | Request or waiter loss does not own preparation. Preparation failure or shutdown removes the key, releases capacity once, and wakes waiters. Blocking hydration remains tracker-owned, so a clean process drain cannot pass detached preparation. | PASS |
| Prepared deadline projection (R4) | `WorkflowExecutor::new` fixes one `tokio::time::Instant`; `PreparedWorkflowRun::deadline` exposes that value; after hydration and preparation, `RunTools::bind_deadline` initializes the shared `Arc<OnceLock<Instant>>` before the reservation is accepted (`skald-workflow/src/workflow.rs:140-209`; `workflow_surface.rs:672-713`; `host.rs:374-400`; `tools.rs:43-96`) | Every Agent resolver and subsequently resolved query tool shares the same cell. No tool can run before preparation returns and acceptance publishes, so there is no unbound execution window. The change removes the earlier tool-only clock sample and adds no timer task, service, channel, retry, setting, or shutdown owner. | PASS |
| Query deadline, cancellation, and settlement | `QueryTool::invoke` clips an omitted or longer deadline to the prepared deadline's remaining duration, then transfers `BoundedQuery` to the run-local tracker (`tools.rs:188-233`). `BoundedQuery` opens through `RunningQueryControls::open_cancellable`; after open, collection keeps the stream and delegates nonterminal failure to `cancel_and_settle` under the query's original bound (`query/collect.rs:275-324,373-430`; `oracle/lifecycle_controls.rs:35-162`). | Step/waiter loss synchronously cancels the child token but does not drop the response owner. Valid success still requires a trustworthy terminal and clean EOF. Deadline, transport, protocol, or owner loss returns an honest bounded failure and no partial rows. Once the original deadline is exhausted, Workflow can terminalize without falsely claiming remote cleanup; the Oracle owner and its diagnostics remain authoritative. | PASS |
| Explicit cancel, total deadline, and step interruption | The accepted run and Skald executor share the reservation cancellation token. Skald reserves attempt one before publishing `Running`; interruption aborts and drains step futures and settles published work as `Cancelled`. Workflow then drains query owners before `AcceptedRun::finish` publishes the terminal snapshot and releases active capacity (`host.rs:263-287`; `skald-workflow/src/workflow.rs:219-340`; `runs.rs:638-688`). | Completion, cancellation, and deadline converge on one terminal compare-and-set. No result can mutate the Workflow after capacity release. R4 changes only the absolute deadline projection and preserves prior attempt semantics. | PASS |
| Forwarded Oracle query and pod loss | The ingress query retains its stream; authenticated lifecycle cancellation routes to the current owner. The leader settles its attempts/exchanges, drops participant grant streams, and releases its own graph/registry owner. A follower's grant-stream close is its release signal; the leader does not wait for a follower acknowledgement (`oracle/lifecycle_controls.rs`; `vala-bifrost-redux/src/oracle/analytical.rs:2256-2277,2695-2764`). | Cancel and deadline remain bounded by the one query/run deadline. Follower loss produces the existing failed/unavailable settlement without rows; surviving membership recovers and later queries remain serviceable. The deleted graph-drain polling and supervisor idle refusal remain absent, as explicitly directed. | PASS |
| Gateway call | Skald's server adapter calls the existing gateway invocation boundary with ordinary per-call authorization and separate cancellation. Gateway accounting, capture, live deployment/credential eligibility, and post-response settlement remain gateway-owned. | Workflow cancellation signals in-flight calls, but Workflow does not wait for gateway settlement or mutate after terminal publication. Gateway failure stays scoped to the call/run and does not stop Cards, query, or unrelated gateway service. | PASS |
| Shutdown | `BoundServer::run` fixes one shutdown deadline and drains Workflow admission, preparations, runs, and run-owned query work while gateway and Bifrost dependencies are still available; later MCP, gateway, capture, and Bifrost phases use the remaining same deadline (`app/server.rs:744-895`; `components/workflow/runs.rs:397-431`). | Deadline exhaustion is reported as a shutdown failure; Bifrost is awaited through abort rather than reported clean. R4 creates no independently refreshed cleanup budget and does not change this order. | PASS |
| Process restart and multiple replicas | Runs, scoped idempotency keys, captured authority, and retained snapshots exist only in `WorkflowRuns`. | A restart or owner-pod loss intentionally loses them. A non-owning replica returns the common not-found response; deployment affinity remains required. No resume, provider replay, cross-replica lookup, or ownership transfer was added. | PASS |
| Dependency outage and resource pressure | Registry failure before acceptance fails and releases the reservation. Missing query controls, Oracle failure, and gateway refusal remain typed owner failures. Active/preparing runs hold bounded capacity and cannot be retention-evicted; terminal entries are expired or evicted oldest-first. | Failures are request/run/component scoped. Recorded sibling-service journeys cover Cards, gateway, and Bifrost availability during held preparation, active query settlement, and near-ceiling terminalization. R4 does not change resource accounting. | PASS |

## Failure and recovery evidence

- Reservation/shutdown ordering is source-visible through the tracker token taken
  under the same run-table lock whose drain closes admission. Recorded owner
  tests cover the pre-spawn reservation interval and abandoned blocking work.
- The R4 focused owner test advances time between tool hydration and preparation
  and asserts a hydrated clone holds `prepared.deadline()` exactly. The recorded
  red result with the old early sampler and the passing current result directly
  exercise the corrected ownership seam.
- The forwarded Oracle journey now supplies a post-tool `DONE` response in the
  deadline case. An early tool-only timeout would therefore permit success;
  the journey instead records `TimedOut` no earlier than the run boundary, no
  output, exact Oracle failure accounting, survivor ownership recovery, and a
  later successful query.
- Original S1-S7 server journeys and R1-R3 owner evidence cover creator/waiter
  disconnect, preparation failure, accepted queued/running shutdown, completion
  races, gateway cancellation, malformed/partial query terminals, follower pod
  loss, retention, and sibling serviceability.
- Explicit shorter query deadlines remain the existing `requested.min(remaining)`
  path. The R4 change replaces only the producer of `remaining`; it does not
  alter the query validator, opening/settlement owner, or MCP and scheduled-query
  consumers.

## Affected capabilities

The cumulative change affects accepted Workflow create/get/cancel, Skald step
execution, in-process gateway calls, built-in Cards and Bifrost reads, Oracle
forwarding and lifecycle control, server shutdown, and process-local retention.
Source tracing found no changed failure path that crashes the shared server,
takes an unrelated capability offline, returns partial query data, releases
Workflow capacity before its run-owned query owner completes its bounded
outcome, or claims restart/multi-replica recovery that V1 does not provide.

## Material findings

None.

No mechanism in the reviewed runtime path lacks an established standard or a
widely used equivalent. In particular, the one-time deadline handoff uses the
standard library's `OnceLock` to bridge the already-required hydration order;
it does not introduce a bespoke lifecycle service or configurable protocol.

## Recorded proof and verification limits

- No build, test, Cargo, or mise command was run during this strictly read-only
  review.
- The original task and R1-R4 records report passing Workflow, Skald, Wyrd,
  Bifrost Oracle, gateway, principals, shared, codegen, boundary, format, and
  lint lanes. R4 specifically records the focused deadline-owner test (1
  passed), `test:skald` (338 passed), Wyrd server tests (684 passed, 23 skipped),
  and the Oracle journey lane (43 passed, including its post-change rerun).
- Recorded results were treated as claims and checked against current source and
  assertions. The focused equality test does not itself invoke every explicit
  query-deadline value, but the unchanged minimum calculation is source-direct
  and its argument bounds/negative paths are covered by the prior server tool
  evidence. This is not a material resilience gap.
- The approved deadline behavior intentionally permits bounded, honestly
  unconfirmed Oracle cleanup after the original deadline; it does not permit a
  clean-settlement claim. Likewise, follower release is grant-stream close and
  has no leader acknowledgement. Neither approved behavior was reopened.
- Foreign-tenant journeys do not run model steps because the fixture provisions
  gateway credentials only for its primary tenant. Their isolation assertions
  remain independent of a foreign model call, as explicitly directed.

## Overall result

**PASS**

The cumulative candidate satisfies the reviewed system-resilience obligations,
and R4 closes the earlier dual-deadline path without changing deployment,
recovery, shutdown, or sibling-service ownership.

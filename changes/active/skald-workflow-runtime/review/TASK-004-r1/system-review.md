# System-resilience review — TASK-004

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Review mode: source, cumulative diff, and recorded evidence only. No build, test, Cargo, or mise command was run.

## Deployed-path coverage

| Changed runtime path | Deployment and ownership trace | Failure/recovery assessment | Result |
|---|---|---|---|
| Workflow create and replay | HTTP route → `WorkflowRunHost::create` → transactional authorization/audit → `WorkflowRuns::admit` → tracked `Preparation` → pinned Cards graph → Skald preparation → atomic `Reservation::accept` | The request owns only a waiter. Dropping the request does not drop preparation. Preparation failure/drop removes its scoped key and releases tenant/global capacity once. Shutdown closes the shared admission token and wakes preparation waiters with unavailable. | PASS |
| Accepted run execution, GET, and cancel | `Preparation::run` → Skald executor → whole-snapshot `AcceptedRun::observe` → tool-owner drain → `AcceptedRun::finish`; fresh GET/cancel authorization precedes owner-qualified lookup | Cancel is recorded synchronously on the run token and the handler then waits for the terminal snapshot. Terminal publication and slot release happen after tracked query-tool owners drain. Process loss intentionally loses the table and deduplication state; another replica returns the common 404. | PASS |
| Retention and idempotency | One process-local mutex guards runs, scoped keys, active counters, and terminal timestamps; sweeps run on admit/get/cancel/finish | Active entries are ineligible for eviction. Terminal expiry and tenant-first/global oldest-first eviction remove both run and key. Restart has no recovery or replay, matching the approved V1 boundary. | PASS |
| Built-in query tool | Agent loop → `QueryTool` → run-owned `TaskTracker` → `BoundedQuery` → query service → Oracle stream; early failure/cancel → `RunningQueryControls::cancel_and_settle` | Dropping the AgentTool waiter cancels its child token while the tracked owner retains the stream and settlement future. Successful results require validated terminal plus clean EOF; error paths call the existing lifecycle control and remain bounded by the original query deadline. | PASS at the Workflow owner; the Bifrost capacity regression below prevents overall acceptance. |
| In-process gateway call | Skald WyrdGateway adapter → `GatewayInvocation::run(..., authorized = false, ...)` → existing gateway admission/accounting/capture | Workflow cancellation signals the call, but gateway settlement remains on the gateway tracker and does not delay Workflow terminalization, as required. Later gateway accounting cannot mutate the retained Workflow snapshot. | PASS |
| Server shutdown | `BoundServer::run` establishes one absolute deadline → Workflow admission closes and tracked preparations/runs drain first → transport supervision → MCP and gateway trackers → capture → Bifrost role shutdown/abort | Gateway and Bifrost remain available while Workflow children settle. Deadline exhaustion is surfaced as a terminal shutdown error and Bifrost abort is awaited; the process does not claim a clean drain after budget exhaustion. | PASS |
| Distributed Oracle graph release | Analytical terminal/cleanup → `AnalyticalSupervisor::release_graph` removes graph/runtime → dropping `OracleQueryResources` returns the slot and tenant charge, while nested `GovernedMemoryView` holders may remain | Candidate removes the prior child-idle gate and explicitly admits replacement work before dependency-owned children have disappeared. This can oversubscribe Oracle execution and let readiness/shutdown report no graph or active query while old work still owns memory or CPU. | **FAIL — SYS-004-001** |

## Failure-path evidence

- Request disconnect: `WorkflowRuns::spawn` owns preparation/execution independently of the create future (`components/workflow/host.rs:99-122`, `runs.rs:331-338`).
- Preparation error or shutdown: `Reservation::release` removes the key and capacity exactly once and publishes an error to all waiters (`runs.rs:534-560`).
- Explicit cancel/deadline: the run token reaches Skald and query owners; the terminal snapshot is held until `RunTools::drain` completes (`host.rs:261-279`, `tools.rs:90-97`).
- Query response loss or malformed terminal: collection signals stream cancellation and delegates authoritative settlement to `RunningQueryControls::cancel_and_settle` (`query/collect.rs:312-329`, `oracle/lifecycle_controls.rs:48-116`).
- Shutdown timeout: Workflow drain uses the process deadline, and a missed drain becomes the server terminal error before Bifrost is aborted (`app/server.rs:744-895`).
- Replica/process loss: all Workflow state is process-local in `WorkflowRuns`; no cross-replica fallback, durable queue, or hidden resume path was introduced.

## Material finding

### SYS-004-001 — REGRESSION: Oracle releases query admission capacity before all query-owned children have ended

- Violated obligation: Bifrost's capacity contract says every query charges one slot unit on every node where it runs for the query lifetime, and TASK-004 requires an admitted query owner to complete validated or honestly unconfirmed settlement before Workflow capacity releases. Shutdown/readiness must not claim a clean drain while owned work survives.
- Locations:
  - `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:972-1007`
  - `crates/vala/vala-bifrost-redux/src/resources.rs:3790-3840`
  - `architecture/bifrost-design.md:379-408`
- Evidence: `release_graph` now checks only that the supervisor's attempt map is empty, removes the graph, invalidates its runtime, and drops its `OracleQueryResources`. `OracleQueryResources::release` then decrements the interactive/analytical query count, drops the leader admission charge, and wakes queued admission even when a nested child still owns the shared memory view. The candidate's own comments explicitly say such a child may still be “being torn down.” `GovernedMemoryView::drop` can poison only when that view eventually drops while holding bytes; it does not keep the Oracle slot or tenant admission charge and cannot detect a child that never drops.
- Reachable failure: dependency-owned DataFusion/cache/exchange/spill work can outlive graph removal. A newly awakened query is then admitted into the returned slot while the old child still consumes governed memory and potentially CPU/IO. The analytical registry and active-query counters already report the old graph gone, so readiness, capacity telemetry, and shutdown inspection can claim quiescence even though process work remains. A child that never drops also never reaches the new drop-time poison check.
- Affected capabilities: all Interactive/Analytical query callers, including HTTP/gRPC/MCP, scheduled queries, and the new Workflow `bifrost.query`; sibling query latency and process shutdown integrity can degrade under cancellation, timeout, transport loss, or rolling replacement.
- Required correction: keep the existing Oracle query slot and tenant admission charge tied to the actual query execution lifetime. Reuse the existing structured task/settlement ownership so the final child release, not graph-map removal, returns that capacity. Do not restore or add a bespoke polling/check mechanism: either join the already-owned child tasks before releasing the graph or let those children retain the existing RAII admission owner until the last one drops. Graph lookup may be invalidated at terminal, but capacity/readiness/shutdown must continue to represent surviving work.
- Focused closure proof: exercise a real admitted analytical query whose dependency-owned child remains alive after graph terminalization. While that child is held, prove the Oracle slot and tenant charge remain occupied, replacement work is not admitted through the returned slot, readiness/shutdown do not report a clean drain, and releasing the child returns capacity exactly once. Retain the recorded forwarded Workflow cancel/deadline/pod-loss journey and the ordinary sibling-query recovery proof.

This finding does not require a new check, file, setting, option, queue, lease, or cleanup service. It asks the existing lifecycle and RAII owners to retain capacity for the work they already own, consistent with structured-concurrency practice and the repository's pre-existing ownership model.

## Recorded proof and limits

The task records passing focused Workflow server journeys, the forwarded three-cause Oracle journey, Bifrost/server/shared/principal/gateway lanes, code generation, boundary checks, formatting, and lints. Those results were not rerun in this strictly read-only review. The recorded late-child unit assertion proves memory bytes eventually return when the test explicitly drops the child; it does not prove admission, readiness, or shutdown remain truthful while the child is still alive, and it does not cover a child that never drops.

No other material system-resilience finding was identified. The process-local durability and replica-affinity limitations are approved behavior, not findings.

## Overall result

**FAIL**

`SYS-004-001` is a reachable capacity and shutdown-accounting regression in the cumulative candidate.

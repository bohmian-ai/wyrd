# Domain review: Bifrost query settlement

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Domain: Bifrost query protocol, forwarded Oracle ownership, cancellation, terminal proof, settlement, durability, capacity release, and sibling recovery
- Review mode: source-and-recorded-evidence only. No build, test, Cargo, or mise command was run.

The candidate identity was rechecked before this report was written and remained unchanged.

## Authority coverage

| Authority | Domain obligation reviewed | Coverage |
|---|---|---|
| `AGENTS.md` §§2, 6, 10, 11, 12 | Oracle remains the query owner; async exists only at IO/lifecycle boundaries; query journeys must cover realistic negative and recovery paths; recorded checks cannot substitute for absent assertions | Covered |
| `architecture/agent-rules.md` | One existing owner per workflow, no duplicate query/audit path, user-facing capability requires a real journey, no gate weakening | Covered |
| `architecture/references/languages/spec-driven-development.md` | Approved specification and architecture outrank implementation evidence; tests must express the mapped obligation rather than silently narrow it | Covered |
| `architecture/wyrd-design.md` doctrine 20 and Bifrost client model | Real client/server/agent journey is the primary proof; Gate/Oracle remain server-owned | Covered |
| `architecture/bifrost-design.md` §§Query: Oracle, Distributed analytical execution, Admission and memory, Read audit and terminal contract | One pinned deadline and cancellation tree; no successor attempt; partial frames are unusable without a trustworthy terminal; failed/ambiguous settlement is not clean success; shared capacity and sibling service remain available | Covered |
| Approved spec REQ-048, REQ-052, INV-021, and the accepted-job/query-settlement text | A dropped step waiter cannot abandon an admitted query; cancellation during open is routed; the response is retained under the original query deadline; Workflow terminalization follows query-owner completion; no partial rows become success | Covered |
| TASK-004 Query ownership across step abort and Scenarios 4/6 | Forwarded cancel, deadline, transport/owner loss, honest terminal classification, tracked owner join, capacity release, and continued sibling availability | Covered |

## Source coverage

| Boundary | Source inspected | Result |
|---|---|---|
| Shared bounded tool query | `crates/wyrd/wyrd-server/src/query/collect.rs:200-584`; base and candidate MCP collector in `src/mcp/bifrost.rs` | Terminal validation, row-count validation, Arrow EOS, clean EOF, and no-partial-result behavior are preserved in one owner shared by MCP and Workflow. |
| Workflow tool lifetime | `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:42-207`; `components/workflow/host.rs:244-280` | `TaskTracker` owns the query independently of the Agent waiter; waiter drop signals the child token; `tools.drain()` precedes terminal commit and active-slot release. |
| Query service and existing consumers | `query/service.rs`; `query/scheduled.rs:49-181`; candidate call-site search for `BoundedQuery`, `ScheduledQueryCaller`, `open_cancellable`, and `cancel_and_settle` | Workflow/MCP use the existing authenticated query service; scheduled queries also adopt the opening-cancellation seam. No second engine or audit owner was introduced. |
| Oracle controls | `oracle/lifecycle_controls.rs:21-306`; `oracle/lifecycle_transport.rs:18-258` | Exact local/remote cancellation and retained-stream settlement exist, but the cancellation-during-open phase is not bounded by the query's original deadline. See QSET-001. |
| Running owner and telemetry | `vala-bifrost-redux/src/oracle/running.rs`; `oracle/mod.rs:627-797, 2259-2310`; `oracle/query_stream.rs` | Registry cancellation marks pre-stream telemetry before signalling. This correctly prevents an owner cancellation before stream construction from being mislabeled as a generic failure. |
| Analytical pre-stream failure | `vala-bifrost-redux/src/oracle/mod.rs:4375-4416`; `oracle/query_stream.rs:800-868` | A selected Analytical graph is now settled before a pre-stream error returns. The original error remains the public result, while graph settlement runs first. |
| Analytical lifecycle/resources | `oracle/analytical.rs:2689-2782`; `oracle/analytical_supervisor.rs:972-1007`; `resources.rs` release changes; candidate `architecture/bifrost-design.md` | The candidate deletes the bespoke child-idle polling loop and release-time nested-child refusal, while retaining attempt joins, exchange closure, participant release, shared-root accounting, and poison-on-leaked-view drop. This is not DRIFT under the standing direction: native reference-counted ownership and the installed memory pool already carry late-child lifetime, while the deleted poll constants and repeated idle checks were bespoke machinery. |
| Forwarded Workflow journey | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:1-249`; cluster/pause support and recorded TASK-004 evidence | Real Scribe-only ingress, forwarded Oracle leader, held follower, explicit cancel, deadline, and pod loss are exercised. The pod-loss recovery assertion required by the task is absent. See QSET-002. |
| Sibling MCP behavior | candidate `src/mcp/bifrost.rs`; recorded MCP query journeys and collector unit evidence in the task | Shared extraction preserves the MCP projection and uses the same terminal-safe collector. No contradictory source path was found. |

## Protocol and settlement assessment

- Successful tool data is not exposed until schema, batches, one valid non-failed terminal, Arrow EOS, and clean stream EOF have all been observed (`query/collect.rs:339-459`). Any protocol, decode, row, or byte failure signals cancellation and returns no accumulated rows.
- A failed terminal retains the owner's typed failure and is treated as terminal evidence; success remains provisional until EOF. This matches the architecture distinction between failure residue and trustworthy successful completion.
- Workflow query work is transferred to a process-tracked owner before the Agent waits (`workflow/tools.rs:190-199`). Dropping the waiter cancels the child token but does not drop the response owner. The Workflow host waits for all such owners before terminal commit (`workflow/host.rs:277-279`).
- Remote lifecycle cancellation reuses the established authenticated Oracle control transport and does not introduce a query engine, durable cleanup queue, retry loop, or second audit path.
- The Bifrost resource simplification removes a repository-specific polling/check mechanism rather than adding one. No comparable-project or established-standard basis supports requiring that deleted poll loop as remediation; the remaining Arc/memory-pool lifetime is the conventional owner.

## Material findings

### QSET-001 — INCORRECT: cancellation during stream open can extend beyond the original query deadline

- **Violated obligation:** The task's query-ownership contract requires cancellation during open to retain and settle the response under its original query deadline. The shared shutdown deadline may shorten that bound, and once the original deadline is exhausted no fresh cleanup timeout may extend it.
- **Location:** `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:132-140, 279-305`; `crates/wyrd/wyrd-server/src/oracle/lifecycle_transport.rs:18-20, 134-152, 234-258`; callers at `query/collect.rs:242-256` and `query/scheduled.rs:140-175`.
- **Evidence:** Once cancellation wins `cancel_while_opening`, it awaits `cancel_owner` to completion before polling `open` again (`lifecycle_controls.rs:293-305`). Remote cancellation fans out through calls that each use a fresh fixed three-second `LIFECYCLE_CALL_TIMEOUT` (`lifecycle_transport.rs:18-20, 248`). `open_cancellable` receives neither the request's existing deadline nor an absolute bound and applies no `timeout_at`. By contrast, `cancel_and_settle` correctly derives and enforces the stream's absolute deadline (`lifecycle_controls.rs:60-115`). A query with less than three seconds remaining can therefore hold the Workflow query owner, terminal commit, and active Workflow capacity past its original bound while an unavailable lifecycle peer consumes that independent timeout.
- **Reachable consequence:** A cancel, Workflow deadline, or shutdown that arrives while a forwarded Analytical query is opening can wait beyond the accepted query/run deadline before the opening future is resumed. The current healthy-peer journey cannot falsify this because its lifecycle cancellation acknowledges promptly. This violates bounded settlement even if Oracle ultimately returns an honest failure.
- **Required correction:** Reuse the request/query's already-existing deadline to bound the complete cancellation-during-open phase, including remote lifecycle signalling and retention of the same open future. Do not add a new timeout setting, retry, durable cleanup owner, or replacement transport. On expiry, return the existing honest incomplete/unavailable/timeout outcome without restarting a cleanup budget. Preserve one cancellation attempt and the same open future while time remains.
- **Focused closure proof:** Extend the existing opening-cancellation seam with a deterministic delayed/unavailable lifecycle-control case whose remaining query deadline is shorter than the transport's ordinary call bound, and prove the operation finishes at that original deadline without starting another bound. Retain the real forwarded Workflow cancel/deadline journey as the cross-process proof.

### QSET-002 — MISSING: the forwarded pod-loss journey does not prove post-loss capacity recovery or sibling availability

- **Violated obligation:** TASK-004 Scenarios 4 and 6 require the forwarded owner-loss case to prove honest settlement, tracked owner completion before Workflow capacity release, and continued sibling query availability. The task's recorded evidence additionally claims that every Oracle returns to baseline and a later query succeeds.
- **Location:** `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:220-249`, especially the `if cause != TerminalCause::PodKill` guard at lines 239-247.
- **Evidence:** The journey runs a sibling query only before the terminal cause (`workflow.rs:152-157`). After terminalization it checks baseline ownership and runs a later query only when the cause is not `PodKill`; the exact remote-owner-loss branch skips both assertions. Its pod-loss error assertion accepts any string containing `WYRD_VALA_` and no `columns` (`workflow.rs:220-225`), so it does not establish the required incomplete/unavailable/protocol classification or that surviving Oracle/query capacity recovered. Nevertheless, the task evidence says all three causes prove later availability and baseline drain.
- **Reachable consequence:** A regression that returns the Workflow result but leaves leader capacity, readiness, or a surviving owner stranded after follower loss would pass the named acceptance journey. The pre-loss sibling query cannot prove recovery after the failure boundary.
- **Required correction:** Extend the same repository-native `PeerCluster` journey, without a new fixture, probe type, setting, or standalone check, to wait for the surviving Oracle owners to reach their expected post-loss baseline and execute a later ordinary query through the surviving topology. Assert the existing exact stable settlement class expected for the lost forwarded owner rather than only an error-code prefix, while continuing to prove no rows reach the model.
- **Focused closure proof:** The existing `workflow_forwarded_query_settles_before_the_run_ends` selector must fail if the pod-loss branch strands a surviving owner/capacity or cannot serve the later query, and must distinguish the approved honest owner-loss result from an arbitrary `WYRD_VALA_*` failure.

## Verification evidence and limits

The task records successful focused Workflow, MCP, Oracle, scheduled-query, Bifrost, shared, Wyrd, principals, gateway, codegen, boundary, formatting, and lint lanes. Those results were treated as recorded evidence only and were not rerun.

The recorded evidence credibly covers healthy terminal validation, explicit cancellation with a responsive lifecycle owner, deadline termination, pre-stream Analytical settlement, MCP reuse, and ordinary sibling service while a query is held. It does not close QSET-001 because no case delays or loses the lifecycle-control acknowledgement while the query has a shorter remaining deadline. It does not close QSET-002 because the named source skips post-loss recovery assertions. The task's prose claim is broader than the checked journey.

No independent build or runtime result was generated in this read-only review.

## Overall result

**FAIL**

The candidate correctly centralizes terminal-safe collection and gives Workflow queries a tracked owner that survives waiter abort, but the opening-cancellation phase can exceed the original query deadline and the required remote-owner-loss recovery is not actually proved by the named journey.

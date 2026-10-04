# Domain review: Bifrost query settlement and durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in the preceding review directories
- Domain: run-bound `bifrost.query`; bounded collection; local and forwarded
  Oracle opening, cancellation, terminal settlement, follower release,
  capacity recovery, and shutdown ordering
- Review mode: complete cumulative diff, candidate source, governing authority,
  prior findings/remediations, and recorded verification evidence only. No
  build, test, Cargo, or mise command was run.

The candidate resolved to the requested commit before inspection. The
repository has no `.codegraph/` directory, so the review used the immutable Git
range, `rg`, and direct source inspection. No other TASK-004 r5 report was used
as review input.

## Overall result

**PASS**

No material finding was identified in this domain. The R4 correction gives
every hydrated Agent clone the exact absolute deadline owned by its
`PreparedWorkflowRun`, using one ordinary shared one-time binding before
acceptance. Query invocation then projects the remaining duration through the
existing millisecond query contract, while the established Workflow,
`BoundedQuery`, Oracle, grant-stream, and shutdown owners retain cancellation
and settlement. The cumulative candidate does not restore or replace the
deleted graph-drain poll, supervisor idle refusal, or a follower-release
acknowledgement.

## Authority coverage

| Authority | Domain obligation | Assessment |
|---|---|---|
| `AGENTS.md` §§2, 5, 6, 9–12 | Server/Oracle retain durable query behavior; accepted async work has an owner; cancellation, terminal safety, and recovery use the narrow established seams; recorded checks do not replace source review | Covered. Workflow composes the existing query service and Oracle controls, and the run tracker owns the tool query until settlement. |
| `architecture/agent-rules.md` | Reuse existing owners, keep tracked work bounded, avoid duplicate lifecycle machinery, and use journey evidence for caller-visible behavior | Covered. R4 changes only the existing preparation/tool seam and one existing journey. |
| `architecture/references/languages/spec-driven-development.md` | Revision 13 and the approved remediation/human decisions outrank historical mechanics | Applied. The accepted one-time prepared-deadline bind is reviewed as the required correction; deleted Oracle machinery is not treated as missing. |
| `architecture/bifrost-design.md` §§Durability and visibility; Query: Oracle; Distributed analytical execution; Admission and memory; Read audit and terminal contract; Resource and failure invariants | One query attempt, one deadline/cancellation tree, terminal-safe rows, local/forwarded owner settlement, follower release by grant-stream close, and honest capacity/recovery evidence | Covered across the traced owners and recorded journeys. |
| `architecture/wyrd-design.md` Workflow/Bifrost boundaries | Accepted Workflow authority is captured without a bearer token, while every Bifrost call still enters the server-owned authorization/audit/query service | Covered by the run-bound `Caller` passed to `stream_query`; no client-side or Skald query engine exists. |
| Revision 13 tool contract, REQ-019, REQ-048, REQ-052, INV-010A, INV-021, and TASK-004 Scenarios 4/6 | A tool query uses the lower remaining Workflow/query bound, survives waiter loss under a tracked owner, returns no partial success, and finishes its owner before Workflow terminal commit/capacity release | Covered. |
| TASK-004-R4 / `FIND-TASK-004-21` | The prepared run is the only absolute run-deadline owner; every `RunTools` clone receives that exact value once; omitted/long query deadlines use the remaining bound and a shorter explicit bound still wins | Closed by the current source and recorded proof. |
| Human standing decisions | Use standard/native mechanisms; graph-drain polling and idle refusal stay deleted; follower release is stream close without leader acknowledgement; built-in query tools take the prepared deadline through a one-time bind | Applied without re-litigation. |

## Source and consumer coverage

| Boundary | Source inspected | Assessment |
|---|---|---|
| Prepared deadline production and binding | `crates/skald/skald-workflow/src/workflow.rs:140-208`; `workflow_surface.rs:678-705`; `crates/wyrd/wyrd-server/src/components/workflow/host.rs:230-400` | `WorkflowExecutor::new` fixes the only absolute total deadline. `PreparedWorkflowRun::deadline` exposes it. After hydration/planning returns and before `reservation.accept` or execution, `Preparation::prepare` calls `RunTools::bind_deadline(&prepared)`. |
| Shared tool deadline state | `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:43-124` | `Arc<OnceLock<Instant>>` is shared by every clone already installed in hydrated Agents. `RunTools::new` samples no clock. The single bind uses the prepared value verbatim and adds no timer, task, channel, setting, or alternate deadline owner. |
| Query invocation and tracked ownership | `components/workflow/tools.rs:156-233`; `components/workflow/host.rs:244-287` | `QueryTool::invoke` derives the current remaining milliseconds solely from the bound prepared deadline, applies `min` to an explicit request, creates a child cancellation token/drop guard, and spawns `BoundedQuery::run` on the run's `TaskTracker`. Dropping the Agent waiter signals cancellation without dropping the query owner. `tools.drain()` precedes `accepted.finish`, so terminal publication and active-slot release follow owner completion. |
| Shared bounded query collection | `crates/wyrd/wyrd-server/src/query/collect.rs:40-618`; `src/mcp/bifrost.rs:207-247` | Workflow and MCP retain one closed argument contract and one collector. Schema/batch order, emitted row count, Arrow EOS, exactly one terminal, clean EOF, row/byte ceilings, and failed-terminal mapping are checked before a result is usable. Every pre-terminal/protocol/budget failure signals cancellation and routes through `cancel_and_settle`; retained rows never become partial success. |
| Cancellation while opening | `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:23-161,300-434`; callers in `query/collect.rs:287-323` and `query/scheduled.rs:132-180` | The existing relative public query duration fixes the query's monotonic opening deadline once. If cancellation wins, one authenticated owner cancellation and the retained open future share the same `timeout_at`; there is no retry or cleanup budget. The R4 bind does not alter this owner. |
| Query-service/Oracle deadline capture | `query/service.rs:244-280`; `oracle/forwarding.rs:208-290`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:1727-1773` | The verified request enters the ordinary query service. Local Oracle and the forwarding ingress capture the request duration at their established dispatch boundary and propagate one absolute Oracle deadline through the selected attempt. Keeping the millisecond public query contract is ordinary existing behavior; R4 adds no second absolute run clock or bespoke propagation protocol. |
| Local/forwarded failure settlement | `vala-bifrost-redux/src/oracle/mod.rs:2126-2158,2241-2362,4136-4259,4375-4416`; `oracle/query_stream.rs:628-691,763-869`; server `oracle/forwarding.rs:400-485` | Admission and running-query ownership precede execution. Cancellation, deadline, schema/first-batch/transport error, and Analytical pre-stream failure settle the selected graph and preserve the typed public failure. Forwarded frames carry the executing Oracle's deadline and are validated before collection. |
| Running-query owner | `crates/vala/vala-bifrost-redux/src/oracle/running.rs:21-370`; `oracle/query_stream.rs:290-344,628-691` | Cancellation marks the exact registry owner before signaling. Terminal removal is exactly once and follows leader-local distributed/Analytical settlement; dropped terminal ownership records failure rather than silently erasing a live query. |
| Leader/follower graph release | `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:1412-1465,2256-2276,2492-2530,2606-2785`; `oracle/dispatcher.rs:581-649` | Participant grants are the open streams. The leader closes them by drop; followers observe close and independently cancel/join their local graph. No release RPC, acknowledgement, retry, timer, or leader wait exists or is required. |
| Capacity and late children | `crates/vala/vala-bifrost-redux/src/resources.rs:3553-3840`; `oracle/analytical_supervisor.rs:972-1007`; `oracle/mod.rs:2761-2935` | Slots, query memory, scratch, exchanges, and graph owners return through their established owners. Late children retain the shared memory view and return their own charges. The removed reserved-byte poll and supervisor idle refusal remain absent, while real retained residue remains observable. |
| Shutdown | `crates/wyrd/wyrd-server/src/app/server.rs:744-895`; `components/workflow/runs.rs:417-432`; `state.rs:689-785`; `vala-bifrost-redux/src/oracle/mod.rs:2862-2935` | Workflow admission closes and tracked run/tool work is cancelled and drained under the server's one remaining deadline while query/Bifrost dependencies are still live. Oracle shutdown then cancels owner work and closes grant streams; deadline exhaustion is reported as unclean rather than a false clean drain. |
| Recorded query journeys | `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs`; `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:68-269`; peer-cluster Oracle workflow/network tests | The server journeys cover tool authority and negative paths. The forwarded journey enters through a Scribe-only ingress, holds real remote Analytical work, covers explicit cancel, total deadline, and follower pod loss, proves no pre-settlement result, checks surviving baselines and later service, and now supplies a post-tool continuation in the deadline case. |

## Settlement, deadline, and recovery assessment

| Reachable path | Required boundary | Candidate result | Recorded evidence |
|---|---|---|---|
| Tool instances are created before Skald preparation finishes | Hydration/planning time must not create an earlier tool-only absolute deadline | Tools contain an unbound shared cell; the exact `PreparedWorkflowRun` deadline is installed afterward, once, before acceptance | `tools_use_the_prepared_run_deadline` advances paused time between hydration and preparation and compares the shared clone with `prepared.deadline()` |
| Omitted or longer explicit query deadline | Use the remaining prepared-run bound | `QueryTool::invoke` calculates remaining time only from the bound instant and applies it when omitted or lower than a longer request | Direct source plus recorded server/Oracle lanes |
| Shorter explicit query deadline | Preserve the caller's shorter bound | `requested.min(remaining)` retains the explicit shorter value | Direct source plus existing bounded-query behavior |
| Agent waiter/step is dropped | Signal the exact query and retain settlement ownership | Drop guard cancels the child token; `TaskTracker` retains `BoundedQuery`; host drains owners before terminal commit | Server Workflow tool journey and forwarded cancel case |
| Cancellation while the stream is opening | One owner signal and the retained open end at the original query bound | `open_cancellable`/`cancel_while_opening` share one `timeout_at` instant | `cancel_while_opening_requests_cancellation_and_keeps_the_open`; `cancel_while_opening_ends_at_the_original_deadline` |
| Malformed, incomplete, oversized, or failed stream | No partial success; validated or honestly unconfirmed settlement | Collector cancels on protocol/decode/budget failure; valid failed terminal is authoritative; success additionally requires EOS and clean EOF | Collector matrix, MCP journeys, Workflow negatives |
| Prepared total deadline expires during a held forwarded query | Run remains `TimedOut`; a premature tool failure cannot continue to a successful final model result | The step's exact prepared deadline interrupts the Agent call and signals its query owner; the host waits for that owner. The deadline journey now has a ready `DONE` continuation, so the prior earlier tool-only boundary would be observable as success | `workflow_forwarded_query_settles_before_the_run_ends` deadline branch |
| Follower disappears mid-graph | Typed failed query, no rows, surviving capacity and service recover | Leader settles failed; surviving owners reach baseline after membership removes the pod; a later query succeeds | Forwarded pod-loss branch and peer-cluster recovery helpers |
| Leader finishes while follower-local cleanup continues | Close grants; do not await a follower acknowledgement | Dropping grant streams releases the leader side; follower structured cleanup remains follower-owned | Scheduled-query and forwarded journeys use bounded observation, not a production acknowledgement |
| Process shutdown | Drain run/query owners before query dependencies, under the shared server shutdown bound | Workflow drain is ordered before Bifrost shutdown; retained residue makes shutdown unclean rather than successful | Recorded lifecycle/shutdown lanes |

## Prior-finding and remediation closure

| Finding | Candidate evidence | Result |
|---|---|---|
| `FIND-TASK-004-3` / `QSET-001`: cancellation during open could outlive the original query deadline | `open_cancellable` fixes one deadline before open and bounds the owner signal plus retained open with it | **CLOSED** |
| `FIND-TASK-004-4` / `QSET-002`: pod-loss proof omitted exact failure and recovery | The forwarded journey checks the exact execution-failed code, no columns, surviving baselines, membership departure, snapshot refresh, and a later successful query | **CLOSED** |
| `FIND-TASK-004-18` / `QSET-R2-001`: owner documentation described a nonexistent follower release acknowledgement | Current documentation and implementation describe grant-stream close, asynchronous follower settlement, and no leader acknowledgement | **CLOSED** |
| `FIND-TASK-004-20` query-domain import shape | `lifecycle_controls.rs` uses its module-scope `Instant` import | **CLOSED** |
| `FIND-TASK-004-21`: run tools sampled a deadline before hydration/planning, earlier than the prepared run | `RunTools::new` samples none; all clones share `Arc<OnceLock<Instant>>`; `Preparation::prepare` binds `prepared.deadline()` before acceptance; the deadline journey answers the continuation | **CLOSED** |

R3's published-attempt correction remains outside this domain and is not
reopened. The candidate preserves the approved rule that a published
`Running` step has attempt one and settles `Cancelled` if interrupted.

## DRIFT assessment

No material DRIFT was found. The implementation uses ordinary Rust/Tokio
mechanisms already present in the repository: `OnceLock` for one-time shared
initialization, `CancellationToken`, `TaskTracker`, absolute monotonic
deadlines, stream lifetime, structured task joins, and reference-counted
resource ownership. The one-time bind is narrower than a deadline service,
timer task, channel protocol, or persistent owner and matches the approved R4
decision.

The unsupported Wyrd-specific graph-drain polling, nested-child release
refusal, and implied follower acknowledgement remain deleted. This review does
not require a new mechanism, check, file, setting, option, retry, poll,
protocol, release acknowledgement, fixture system, or cleanup service.

## Material findings

None.

## Verification evidence and limits

The task and remediations record successful focused evidence for the shared
prepared deadline, server Workflow journeys, forwarded Oracle
cancel/deadline/pod-loss behavior, original-deadline cancellation during open,
collector protocol/byte accounting, MCP query journeys, scheduled-query
cleanup, peer loss/cancellation and recovery, plus the broader Skald, Wyrd,
Bifrost, formatting, and lint lanes. R4 specifically records the paused-time
red/green deadline-owner test and 43 passing Oracle journeys after the
post-tool continuation was added.

Those results were treated as recorded claims and matched to the named source
and assertions; this review did not execute them. The unit proof establishes
the nonzero pre-preparation interval and exact shared binding, while the real
forwarded journey establishes runtime terminal behavior with a continuation
that would expose the former early tool timeout. The ordinary public Bifrost
deadline remains millisecond-valued; requiring a second absolute-deadline wire
surface or another query owner would be outside the approved task and contrary
to the standing DRIFT direction.

The foreign-tenant model-step limitation is not a gap here: the harness seeds
gateway credentials only for the fixture tenant, while the approved
cross-tenant query/Card/run refusals do not require a foreign model call. No
required authority, source, caller, prior finding, remediation record, or
recorded evidence was unavailable.

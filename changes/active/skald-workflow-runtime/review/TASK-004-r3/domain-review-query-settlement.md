# Domain review: query settlement and durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md` and `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Prior verdicts and ledgers: TASK-004 r1 and r2
- Domain: bounded `bifrost.query` collection; query-open cancellation; Workflow waiter loss; Oracle running-query and Analytical graph settlement; participant grant release; pod loss; capacity recovery; MCP and scheduled-query siblings
- Review mode: complete cumulative diff, candidate source, governing authority, and recorded evidence only. No build, test, Cargo, or mise command was run.

The candidate identity was checked before and after source inspection and remained
`f17726fb25df1fa513875dca8d92f0073340ee0a`. `.codegraph/` is absent, so this
review used immutable Git objects, `rg`, and direct source inspection. No other
TASK-004 r3 report was read.

## Authority and boundary coverage

| Authority | Domain obligation | Assessment |
|---|---|---|
| `AGENTS.md` §§2, 5, 6, 10–12 | Oracle remains the query owner; async work is bounded and cancellation-aware; one owner carries lifecycle state; user-facing recovery is proved at journey level | Covered. The Workflow host composes the existing query and Oracle owners rather than adding another engine or cleanup service. |
| `architecture/agent-rules.md` | Reuse existing owners, keep accepted work tracked, retain real journey proof, do not weaken gates, and keep Rust documentation aligned with actual ownership | Covered. `RunTools`, `BoundedQuery`, `RunningQueryControls`, and the Oracle graph lifecycle have distinct cohesive ownership. |
| `architecture/references/languages/spec-driven-development.md` | Approved Revision 13 and human decisions outrank historical task mechanics or tests | Applied. Deleted graph-drain polling and supervisor idle refusal were not treated as missing; no follower acknowledgement was required. |
| `architecture/wyrd-design.md` Workflow and Bifrost sections | Accepted jobs retain captured authority while Bifrost performs its own per-call decision; Gate and Oracle remain server owners | Covered by the run-bound verified `Caller` passed through the ordinary query service. |
| `architecture/bifrost-design.md` §§Distributed analytical execution, Admission and memory, Read audit and terminal contract | One pinned cut/deadline/cancellation tree; terminal-safe results; leader-owned grant streams; follower-local cleanup; honest residue; shared capacity recovery | Covered in the traced runtime and recovery paths. |
| Revision 13 REQ-019, REQ-048, REQ-052, INV-010A, INV-021, AC-020 and TASK-004 query-ownership text | An admitted query survives waiter abort, is cancelled at its owner, settles within its original bound, exposes no partial success, and completes its owner before Workflow terminalization/capacity release | Covered. |
| Human standing direction | Wyrd uses established/native mechanisms; unsupported bespoke mechanisms are DRIFT; grant-stream close is follower release; leader awaits no follower acknowledgement; foreign-tenant model execution is outside the fixture's credential capability | Applied without re-litigation. |

## Source and caller coverage

| Boundary | Source inspected | Assessment |
|---|---|---|
| Workflow tool ownership | `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:42-215`; `components/workflow/host.rs:230-287` | `QueryTool::invoke` creates a child cancellation token and drop guard, then places `BoundedQuery::run` on the run's `TaskTracker`. Dropping the Agent waiter synchronously cancels the token without dropping the owner. `Preparation::run` calls `tools.drain()` before `accepted.finish`, so the run snapshot and active slot cannot terminalize before all admitted query owners finish. |
| Shared bounded collector | `crates/wyrd/wyrd-server/src/query/collect.rs:40-351,353-618`; `src/mcp/bifrost.rs:207-247` | Workflow and MCP share one closed argument contract and one collector. Schema/batch ordering, row count, Arrow EOS, exactly one terminal, clean EOF, row/byte bounds, and failed-terminal mapping are enforced before any result is returned. Partial rows are never exposed as success. |
| Cancellation during open | `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:33-161,257-333`; callers in `query/collect.rs:287-323` and `query/scheduled.rs:132-180` | The request duration fixes one monotonic deadline before open. If cancellation wins, the existing authenticated lifecycle cancellation and the retained open future share `timeout_at` on that deadline. There is one cancellation attempt, no retry, cleanup timer, or replacement owner. |
| Open/first-batch failure | `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2126-2158,2241-2362,4138-4259,4375-4416` | Admission and running-query ownership are acquired before execution. Cancellation, deadline, stale object, first-batch error, schema error, and pre-stream Analytical failure cancel/join distributed children and pass through the existing Analytical settlement before returning the original error. |
| Running-query lifecycle | `crates/vala/vala-bifrost-redux/src/oracle/running.rs:21-175,196-370`; `oracle/query_stream.rs:630-691` | Registry cancellation marks telemetry before signalling, preventing a pre-stream cancellation from being mislabeled as a generic failure. Terminal removal is exactly once and occurs after leader-local distributed and Analytical settlement. |
| Leader graph settlement | `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2256-2267,2492-2530,2690-2785,7778-7795`; `oracle/query_stream.rs:763-869` | The lifecycle joins the leader attempt, closes graph exchanges, drops participant grant streams, releases the leader graph/admission owner, and only then retires its running-query entry. Cleanup failure remains retained and cannot become a successful terminal. |
| Follower grant ownership | `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:581-649`; `oracle/analytical.rs:1412-1465` | `ParticipantGrant` is the open stream. Dropping it is the leader-side release; the follower observes close, cancels, joins, and frees its own graph asynchronously. There is no release RPC, acknowledgement, retry, timer, or leader wait. |
| Query resources | `crates/vala/vala-bifrost-redux/src/resources.rs:3553-3661,3709-3840`; `oracle/analytical_supervisor.rs:972-1007` | Slots return through the ordinary governor owner. A late child retains the shared memory view and returns its own bytes; dropping a view still holding bytes poisons the governor. The deleted reserved-byte poll and supervisor idle refusal remain absent. |
| Shutdown order | `crates/wyrd/wyrd-server/src/app/server.rs:744-895`; `components/workflow/runs.rs:417-432` | Workflow admission closes, cancellation is signalled, and tracked work drains first under the single server deadline while gateway/query/Bifrost services remain available. Only afterwards do transport and Bifrost shutdown proceed. Timeout is reported as an unclean drain rather than success. |
| Forwarded cancel/deadline/pod loss | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:68-269`; shared `PeerCluster` membership/baseline helpers | The real Workflow is submitted through a Scribe-only ingress and forwarded to an Analytical leader. The journey covers explicit cancel, original-deadline expiry, and follower pod loss; no tool result reaches the model before settlement, the stable pod-loss code contains no rows, surviving owners return to baseline, membership excludes the lost follower, and a later query succeeds. |
| Scheduled and MCP siblings | `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:740-912`; `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:207-247` | Scheduled queries use the same open-cancellation and terminal rules. Their strict leader proof is immediate; bounded test observation may wait for asynchronous follower cleanup. MCP retains the shared collector and ordinary request cancellation, with no second result or lifecycle implementation. |

## Settlement and failure-path assessment

| Reachable path | Required terminal boundary | Source result | Recorded proof |
|---|---|---|---|
| Agent waiter or Skald step is dropped | Signal query owner; keep response and settlement continuation alive | `DropGuard` cancels the child token while `TaskTracker` retains the owner; host drains the tracker before terminal commit | `declared_tools_use_captured_scopes_and_owned_services`; forwarded Workflow journey |
| Cancellation while query stream opens | Route one owner cancellation and retain the same open only until the original query deadline | `RunningQueryControls::open_cancellable` and `cancel_while_opening` use one `timeout_at` deadline | `cancel_while_opening_requests_cancellation_and_keeps_the_open`; `cancel_while_opening_ends_at_the_original_deadline` |
| Malformed, incomplete, oversized, or failed stream | Return no partial success; settle or honestly report unconfirmed settlement | `ResultCollector` cancels on every pre-terminal/protocol/budget failure; a valid failed terminal is authoritative; success additionally requires EOS and clean EOF | Collector unit matrix, MCP journeys, Workflow tool negatives |
| First-batch cancellation/deadline/error | Cancel and join distributed work, settle selected Analytical graph, preserve original error | `settle_distributed_failure` then `release_error`/`settle_analytical` | Oracle and forwarded Workflow recorded lanes |
| Explicit Workflow cancel | Await bounded query owner before `Cancelled` run commit and active-capacity release | Run tool owner drains before `accepted.finish` | Forwarded cancel cause and server Workflow lifecycle journey |
| Workflow deadline | Preserve `TimedOut`; bounded unconfirmed Oracle cleanup does not become clean success or start a fresh timer | Query deadline is clipped to remaining run time; owner exits at that bound; Oracle retains any unconfirmed residue | Forwarded deadline cause and original-deadline unit proof |
| Follower pod loss | Failed query terminal, no rows to model, surviving capacity and service recover | Leader graph settles failed; surviving owners reach baseline after membership removes the pod; later query succeeds | Forwarded pod-loss cause |
| Leader settlement with live follower cleanup | Release by closing grant streams; do not wait for follower acknowledgement | `drop(grants)` closes streams; follower driver settles independently | Scheduled-query and forwarded journeys, with bounded follower cleanup observation |
| Process shutdown | Cancel and drain Workflow owners before closing their gateway/query/Bifrost dependencies, under one shared deadline | `BoundServer::run` orders Workflow drain before Bifrost shutdown and reports deadline exhaustion as failure | `lifecycle_races_retention_and_shutdown` and recorded server/Bifrost lanes |

## Prior-finding and remediation closure

| Finding | Candidate evidence | Result |
|---|---|---|
| `FIND-TASK-004-3` / `QSET-001`: cancel during open could outlive the original deadline | `open_cancellable` fixes the deadline before open and `cancel_while_opening` bounds the owner signal plus retained open with that same instant | **CLOSED** |
| `FIND-TASK-004-4` / `QSET-002`: pod-loss journey lacked post-loss recovery and exact failure proof | The pod-loss branch checks `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, absence of columns, surviving baselines, membership departure, snapshot refresh, and a later three-row query | **CLOSED** |
| `FIND-TASK-004-18` / `QSET-R2-001`: Analytical owner documented a nonexistent release-ack protocol | `AnalyticalGraphLifecycle` now documents retained grant streams, drop-as-release, asynchronous follower settlement, no leader wait, and reservation/admission-only transports | **CLOSED** |

The r2 remediation changed only the last documentation contract in this domain;
its other relevant edits import types by bare name without altering behavior.
No later candidate commit reopens the runtime findings.

## DRIFT assessment

No material DRIFT remains in this domain. The implementation uses established
Tokio cancellation tokens, task tracking, absolute deadlines, stream lifetime,
structured task joins, DataFusion/Arrow stream decoding, and reference-counted
memory ownership. The test-only pause/fault seams are ordinary deterministic
fault injection attached to existing owners, not production protocols or
operator options. Nextest's existing test-group mechanism is reused to serialize
the port-sharing peer-cluster journey and bound ordinary Postgres fixture
concurrency.

The unsupported Wyrd-specific graph-drain poll, nested-child release refusal,
and implied follower release acknowledgement remain deleted. This review does
not require their restoration or any new check, file, setting, option, retry,
poll, protocol, probe, fixture, or cleanup service.

## Material findings

None.

## Verification evidence and limits

The candidate records successful focused evidence for the Workflow query-tool
scenario, the forwarded Oracle Workflow journey across cancel/deadline/pod-loss,
the absolute-deadline open-cancellation unit proof, collector protocol and byte
accounting, four MCP query journeys, the scheduled-query cleanup journey, peer
loss/cancellation, and the broader Wyrd, Bifrost, shared, principals, gateway,
format, lint, codegen, tenant-isolation, client-tier, and unwrap-audit lanes.
The r2 evidence additionally records 683 Wyrd tests and 43 Oracle journeys after
the behavior-neutral source/documentation remediation.

These results were treated as recorded evidence claims and checked against the
named source and assertions. This review did not independently execute them, per
the strict read-only instruction. The foreign-tenant journey limitation is not
a verification gap for this domain: the harness provisions gateway credentials
only for the fixture tenant, and the approved cross-tenant query/Card/run
refusals do not require a foreign model step.

No required authority, source, diff, prior finding, remediation record, or
verification record was unavailable.

## Overall result

**PASS**

The cumulative candidate satisfies the distributed query settlement and
durability obligations. An accepted `bifrost.query` remains owned after waiter
loss, cancellation and first-batch failure preserve the original bound, Workflow
terminalization follows the query owner's validated or honestly bounded outcome,
leader and follower resources release under their actual stream-ownership model,
and pod loss does not strand surviving capacity or sibling query service.

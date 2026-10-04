# Domain review: Bifrost query ownership, settlement, and durability

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1, R2, and R3 in the preceding review directories
- Domain: Workflow `bifrost.query` ownership; shared bounded collection; scheduled-query sibling behavior; local and forwarded Oracle opening, streaming, and terminal settlement; running-query controls; participant grants; capacity recovery; shutdown ordering
- Review mode: complete cumulative diff, candidate source, governing authority, prior findings, remediation records, and recorded verification evidence only. No build, test, Cargo, or mise command was run.

The candidate identity was checked before source inspection and remained
`5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58` when this report was written.
`.codegraph/` is absent, so the review used immutable Git objects, `rg`, and
direct source inspection. No other TASK-004 r4 report was used as review input.

## Review findings

No material finding was identified in this domain.

There are no open correctness or risk questions. The candidate does not add or
retain a nonstandard release acknowledgement, graph-drain poll, idle refusal,
cleanup setting, retry option, query engine, or lifecycle service.

## Authority and boundary coverage

| Authority | Domain obligation | Assessment |
|---|---|---|
| `AGENTS.md` §§2, 5, 6, 9–12 | Oracle remains the durable query owner; accepted async work has one tracked owner; cancellation and shutdown preserve bounded settlement; caller-visible query results require credible journey proof | Covered. The Workflow host composes the existing query service, collector, lifecycle controls, and Oracle owners rather than duplicating them. |
| `architecture/agent-rules.md` | Reuse existing owners and top-level imports, retain tracked work, and keep lifecycle documentation aligned with the implementation | Covered. The R3 import-only edit to `lifecycle_controls.rs` changes no behavior, and the corrected stream-close documentation remains accurate. |
| `architecture/references/languages/spec-driven-development.md` | Approved Revision 13 and explicit human decisions outrank historical task mechanics and tests | Applied. Deleted graph-drain polling and supervisor idle refusal were not treated as missing; no follower acknowledgement was required. |
| `architecture/wyrd-design.md` Workflow and Bifrost sections | Accepted jobs retain captured token-free authority while Bifrost performs its own live per-call decision through the server-owned service | Covered by the run-bound verified `Caller` passed through `stream_query`; no bearer or client-owned query implementation is introduced. |
| `architecture/wyrd-security-posture.md` accepted-Workflow boundary | Bifrost queries inside an accepted run authorize and audit at the owning service under the captured authority | Covered by the ordinary public query-service path. |
| `architecture/bifrost-design.md` §§Distributed analytical execution, Admission and memory, Read audit and terminal contract | One pinned cut, one deadline and cancellation tree, terminal-safe results, grant-stream follower ownership, honest retained residue, and capacity returned through the native governor owners | Covered in the traced local, forwarded, failure, recovery, and shutdown paths. |
| Revision 13 REQ-019, REQ-048, REQ-052, INV-010A, INV-021, AC-020 and TASK-004 query-ownership contract | An admitted tool query survives waiter loss, is cancelled at its owner, settles within its original bound, returns no partial success, and completes its owner before Workflow terminalization and active-capacity release | Covered. |
| Human standing direction | Use established/native mechanisms; unsupported Wyrd-only mechanisms are DRIFT; follower release is grant-stream close; the leader awaits no follower release acknowledgement | Applied without re-litigation. |

## Source and caller coverage

| Boundary | Source inspected | Assessment |
|---|---|---|
| Workflow request to tracked owner | `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:42-215`; `components/workflow/host.rs:230-287` | `QueryTool::invoke` clips the query bound to the run's remaining deadline, creates a child cancellation token plus drop guard, and spawns `BoundedQuery::run` on the run's `TaskTracker`. A dropped Agent waiter signals cancellation without dropping the owner. `tools.drain()` completes before the accepted run commits its terminal snapshot and releases active capacity. |
| Shared bounded collection | `crates/wyrd/wyrd-server/src/query/collect.rs:40-618`; `src/mcp/bifrost.rs:207-247` | Workflow and MCP use the same closed arguments and `BoundedQuery`/`ResultCollector`. The collector enforces schema/batch order, row count, Arrow EOS, one valid terminal, clean EOF for success, and exact row/byte ceilings. Every pre-terminal, protocol, decoding, or ceiling failure routes through `cancel_and_settle`; retained rows never become partial success. |
| Scheduled-query sibling | `crates/wyrd/wyrd-server/src/query/scheduled.rs:17-353`; `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:740-912` | The scheduled owner uses the same public query entry for authenticated callers, the same `open_cancellable` and `cancel_and_settle` controls, and its own streaming batch callback rather than another engine. It accepts success only after a valid terminal and clean EOF. |
| Cancellation during open | `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:23-177,300-434`; callers in `query/collect.rs:287-323` and `query/scheduled.rs:132-180` | The original requested duration fixes one monotonic deadline before open. If cancellation wins, one authenticated owner cancellation and the retained open future share the same `timeout_at`; no retry or new cleanup budget is created. The R3 remediation only imports and uses bare `Instant`. |
| Local and forwarded open | `crates/wyrd/wyrd-server/src/oracle/forwarding.rs:208-315,400-485`; `oracle/peer_service.rs:604-635`; `vala-bifrost-redux/src/oracle/mod.rs:2010-2462` | Local and remote selection converge on the same Oracle attempt. Forwarding projects the executing Oracle's deadline and validates each converted frame. A cancellation before remote acceptance either reaches a registered owner or drops the still-unopened request at the original bound; a stream that does open is retained for ordinary settlement. |
| Pre-stream and first-batch failures | `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:2126-2158,2241-2362,4136-4259,4375-4416` | Admission and running-query ownership are taken before execution. Cancellation, deadline, stale object, first-batch error, schema error, and selected-Analytical pre-stream failure cancel/join distributed children and invoke the existing Analytical settlement before preserving the original public error. |
| Running-query terminal owner | `crates/vala/vala-bifrost-redux/src/oracle/running.rs:21-370`; `oracle/query_stream.rs:290-344,628-691` | Registry cancellation marks telemetry before signaling. Terminal removal is exactly once and occurs only after distributed and leader-local Analytical settlement. A dropped terminal owner settles failed rather than silently leaving an active entry. |
| Leader Analytical settlement | `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2256-2276,2492-2530,2606-2785,7777-7804`; `oracle/query_stream.rs:763-869` | One lifecycle task joins the leader attempt, closes exchanges, drops participant grant streams, releases the leader graph and admission owner, and then retires its running-query entry. A cleanup failure remains retained and prevents a success terminal. |
| Follower grant settlement | `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:581-649`; `oracle/analytical.rs:1412-1465,2690-2765` | `ParticipantGrant` is the open admit stream. Dropping it is the leader-side release; each follower observes stream close and settles its own graph asynchronously. There is no release RPC, acknowledgement, retry timer, polling loop, or leader wait. |
| Capacity and late children | `crates/vala/vala-bifrost-redux/src/resources.rs:3553-3840`; `oracle/analytical_supervisor.rs:972-1007`; `oracle/mod.rs:2761-2897` | Slots, query memory, scratch, exchanges, and graph owners return through their established owners. Late children retain the shared memory view and return their own charges. The removed nested-child polling and supervisor idle refusal remain absent; an actual retained cleanup remains observable rather than falsely clean. |
| Shutdown | `crates/wyrd/wyrd-server/src/app/server.rs:744-895`; `components/workflow/runs.rs:417-432`; `crates/wyrd/wyrd-server/src/state.rs:689-785`; `vala-bifrost-redux/src/oracle/mod.rs:2862-2935` | Workflow admission closes and tracked run/tool work is cancelled and drained under the server's one remaining deadline while query and Bifrost dependencies remain available. Oracle then closes readiness, cancels owner work, closes held grant streams, and reports residual admission state rather than claiming a clean timeout. |
| Forwarded cancel, deadline, and pod loss | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:68-269`; shared `PeerCluster` ownership and membership helpers | The real Workflow enters through a Scribe-only pod and reaches a remote Analytical leader. The journey checks explicit cancel, original-deadline expiry, and follower pod loss; no premature model result; exact pod-loss failure without rows; surviving-owner baselines; membership removal; and a later successful query. |

## Settlement and durability assessment

| Reachable path | Required boundary | Candidate result | Recorded proof |
|---|---|---|---|
| Agent waiter or Skald step drops | Signal the query owner while retaining its response and settlement continuation | The drop guard cancels the child token; the `TaskTracker` keeps the query owner; host drain precedes run terminal commit | `declared_tools_use_captured_scopes_and_owned_services`; forwarded Workflow journey |
| Cancellation while opening | Route one owner cancellation and retain the same open only to the original deadline | `open_cancellable` and `cancel_while_opening` use one absolute monotonic instant | `cancel_while_opening_requests_cancellation_and_keeps_the_open`; `cancel_while_opening_ends_at_the_original_deadline` |
| Malformed, incomplete, oversized, or failed stream | Return no partial success and either validate settlement or report it honestly unconfirmed | `ResultCollector` cancels every pre-terminal/protocol/budget failure; a failed terminal is authoritative; success additionally requires EOS and clean EOF | Collector unit matrix, MCP journeys, Workflow tool negatives |
| First-batch cancellation/deadline/error | Cancel and join distributed work, settle the selected graph, preserve the original error | `settle_distributed_failure` then `release_error`/`settle_analytical` | Recorded Oracle and forwarded Workflow lanes |
| Explicit Workflow cancel | Finish the tracked query owner before committing `Cancelled` and releasing active capacity | `tools.drain()` precedes `accepted.finish` | Forwarded cancel cause and server Workflow lifecycle journey |
| Workflow deadline | Preserve `TimedOut`; do not restart the query budget or turn unconfirmed cleanup into success | Tool deadline is clipped to remaining run time; the query owner stops at that original bound; Oracle keeps attributable residue if cleanup cannot be confirmed | Forwarded deadline cause and original-deadline unit proof |
| Follower pod loss | Produce a failed query result without rows; recover surviving capacity and service | Leader graph settles failed; survivors return to baseline after membership removes the pod; a later query succeeds | Forwarded pod-loss cause |
| Leader return while follower cleanup continues | Close grant streams and leave follower-local structured cleanup to the follower | `drop(grants)` closes streams; follower settlement runs independently | Scheduled and forwarded journeys use bounded observation only |
| Process shutdown | Drain Workflow query owners before their dependencies, within the one process deadline | `BoundServer::run` orders Workflow drain before Bifrost shutdown; timeout remains an unclean result | `lifecycle_races_retention_and_shutdown` and recorded server/Bifrost lanes |

## Prior-finding and remediation closure

| Finding | Candidate evidence | Result |
|---|---|---|
| `FIND-TASK-004-3` / `QSET-001`: cancel during open could exceed the original query deadline | `open_cancellable` fixes the deadline before open and bounds the single owner signal plus retained open with it | **CLOSED** |
| `FIND-TASK-004-4` / `QSET-002`: forwarded pod-loss proof omitted exact failure and recovery | The pod-loss branch checks `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, no columns, surviving baselines, membership departure, snapshot refresh, and a later three-row query | **CLOSED** |
| `FIND-TASK-004-18` / `QSET-R2-001`: the lifecycle owner documented a nonexistent follower release acknowledgement | The owner now documents retained grant streams, drop-as-release, asynchronous follower settlement, no leader wait, and reservation/admission-only transport | **CLOSED** |
| `FIND-TASK-004-20` query-domain portion: qualified `tokio::time::Instant` in changed interfaces | R3 imports `Instant` at module scope in production and test modules and uses the bare name; the diff from the r3 candidate to this candidate is source-shape-only | **CLOSED** |

R3's step-attempt correction does not touch this domain. Comparing the r3
candidate with the current candidate shows only the behavior-neutral `Instant`
import cleanup in the query-settlement source set; no later change reopens the
runtime findings.

## DRIFT assessment

No material DRIFT remains in this domain. The implementation uses established
Tokio cancellation tokens, `TaskTracker`, absolute deadlines, stream lifetime,
structured task joins, DataFusion/Arrow decoding, tonic streaming, and
reference-counted memory ownership. The test-only pause and fault seams attach
to existing owners for deterministic journey synchronization; they are not
production protocols or operator settings.

The unsupported Wyrd-specific graph-drain poll, nested-child release refusal,
and implied follower release acknowledgement remain deleted. This review does
not require their restoration or any new check, file, setting, option, retry,
poll, protocol, probe, fixture, or cleanup service.

## Verification notes and limits

The immutable task and remediations record successful focused evidence for the
Workflow query-tool scenario; forwarded Oracle cancel, deadline, and pod-loss;
original-deadline cancellation during open; collector protocol and byte
accounting; four MCP query journeys; scheduled-query cleanup; peer loss and
cancellation; capacity recovery; and broader Wyrd, Bifrost, shared, principals,
gateway, formatting, lint, codegen, tenant-isolation, client-tier, and unwrap
audit lanes. R2 records 683 Wyrd tests and 43 Oracle journeys after the
documentation/import remediation; R3 records the affected Wyrd and Oracle
journeys after the final import correction.

Those results were treated as recorded evidence claims and matched to the named
source and assertions. This review did not independently execute them, as
required. The foreign-tenant model-step limitation is not a gap in this domain:
the harness provisions gateway credentials only for the fixture tenant, while
the approved foreign-tenant query, Card, key, and run refusals do not require a
foreign model invocation.

No required authority, source, cumulative diff, prior finding, remediation
record, or verification record was unavailable.

## Overall result

**PASS**

The cumulative candidate satisfies the Bifrost query ownership, settlement,
and durability obligations for TASK-004. An accepted tool query remains owned
after waiter loss; local and forwarded cancellation preserve the original
deadline; terminal-safe collection exposes no partial success; leader and
follower resources release through the actual grant-stream ownership model;
Workflow terminalization follows owner completion or an honestly bounded
unconfirmed outcome; and pod loss does not strand surviving capacity or sibling
query service.

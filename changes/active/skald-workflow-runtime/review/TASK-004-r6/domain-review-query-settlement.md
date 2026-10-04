# Domain review: query settlement, deadlines, and distributed Oracle

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 14
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in their preceding review directories
- Domain: accepted-run `bifrost.query` ownership, prepared-run/query deadline
  precedence, complete-result settlement, local and forwarded Oracle
  cancellation, distributed graph release, peer loss, capacity recovery, and
  sibling query consumers
- Review mode: source and recorded evidence only. No build, test, Cargo, mise,
  formatter, linter, or code-generation command was run.

The candidate resolved to the requested commit before and after inspection.
The repository has no `.codegraph/` directory, so the review used the immutable
Git range, `rg`, and direct source reading. No other TASK-004 r6 report was used
as review input.

## Overall result

**PASS**

No material finding was identified in this domain. The cumulative candidate
keeps one prepared Workflow deadline, binds that exact instant once into every
hydrated `RunTools` clone before acceptance, and applies a positive caller
`deadline_ms` only when it is shorter. The R5 journey now drives omitted,
explicit-longer, and explicit-shorter arguments through the actual Workflow
tool, Scribe-only forwarding ingress, Oracle leader, and held distributed
follower. Existing tracked query ownership, terminal-safe collection,
cancel-and-settle behavior, peer-loss recovery, and sibling availability remain
intact.

Revision 14 changes provider request and Prompt dispatch representation, not
the query service, Oracle ownership, deadline, cancellation, or settlement
contract. No query-domain regression follows from that revision.

## Authority and boundary coverage

| Authority | Domain obligation reviewed | Assessment |
|---|---|---|
| `AGENTS.md` §§2, 5, 6, 9–12 | Server and Oracle retain durable query behavior; async work has a bounded owner; complete-result and recovery proof use production-shaped journeys | Covered. The Workflow composes the existing server query and Oracle owners rather than adding an engine or lifecycle service. |
| `architecture/agent-rules.md` | Reuse existing owners; keep accepted work tracked and bounded; do not weaken gates or replace journey evidence with helper-only checks | Covered. The R5 change extends the existing Oracle Workflow journey only. |
| `architecture/references/languages/spec-driven-development.md` | Current human decisions, Revision 14, and cumulative remediation authority outrank historical mechanics | Applied. Fixed graph-release, foreign-tenant, attempt, deadline-bind, and provider decisions were not reopened. |
| `architecture/bifrost-design.md` §§Query: Oracle, Distributed analytical execution, Admission and memory, Read audit and terminal contract, Resource and failure invariants | One selected attempt, one cancellation tree and bounded deadline; no partial success; leader stream closes follower grants; peer loss is terminal; surviving capacity and service recover | Covered in source and the recorded forwarded-query journey. |
| `architecture/references/domain/olap-serving.md` | Oracle owns typed, tenant-bound, budgeted reads and one terminal-safe result; selected Analytical work has no successor or Interactive fallback | Covered. Workflow enters the ordinary query service and consumes the existing Oracle stream contract. |
| `architecture/references/domain/arrow-analytical-interop.md` | Bounded Arrow batches and an explicit trustworthy terminal; cancellation joins producers and releases reservations | Covered by `BoundedQuery`/`ResultCollector` and Oracle lifecycle controls. |
| `architecture/references/domain/analytical-operations-reliability.md` | Query-owned runtime, memory, spill, queues, graph controls, cancellation, and deadline; cancellation releases descendants; production qualification includes peer loss | Covered by the retained Oracle owners and journey assertions. |
| Revision 14 spec, REQ-048, REQ-052, INV-021, and TASK-004 Scenarios 4/6 | Built-in queries use the captured caller and the lower remaining bound, return no partial success, remain owned after waiter loss, and finish bounded settlement before Workflow capacity release | Covered. |
| TASK-004 R1–R5 and prior query findings | Preserve prior cancellation/opening, pod-loss, follower-release, and prepared-deadline closures; positively prove omitted/longer/shorter deadline precedence | Covered; prior closures remain intact and R5 adds the missing positive branches. |
| Standing human direction | Deleted supervisor polling/idle refusal stay deleted; grant-stream close releases followers without leader acknowledgement; the prepared deadline is bound once and a shorter positive `deadline_ms` wins; unsupported bespoke mechanisms are DRIFT | Applied without re-litigation. No such mechanism was added or required. |

## Source and consumer coverage

| Boundary | Source inspected | Assessment |
|---|---|---|
| Prepared deadline owner | `crates/skald/skald-workflow/src/workflow.rs:140-208`; `workflow_surface.rs:676-705` | `WorkflowExecutor::new` fixes the sole absolute total deadline and `PreparedWorkflowRun::deadline` exposes it without a second sampler. |
| One-time server bind | `crates/wyrd/wyrd-server/src/components/workflow/host.rs:255-287,309-400`; `components/workflow/tools.rs:43-114` | Agents receive clones sharing `Arc<OnceLock<Instant>>`; after hydration and preparation, `bind_deadline` installs `prepared.deadline()` before `reservation.accept`. `tools.drain()` still precedes `accepted.finish`, so tracked query owners finish before terminal publication and active-slot release. |
| Tool deadline precedence | `components/workflow/tools.rs:156-230` | The actual `QueryTool` validates the closed arguments, clips the request to the prepared deadline's remaining public millisecond duration, and retains a shorter explicit positive duration via `requested.min(remaining)`. The run cancellation token remains the exact prepared-bound signal for the tracked owner. |
| Tracked complete-result query | `crates/wyrd/wyrd-server/src/query/collect.rs:264-618` | `BoundedQuery` transfers the response to tracked server work. `ResultCollector` requires valid schema/batches, row agreement, exactly one terminal, Arrow EOS, clean EOF, and row/byte bounds. Every pre-terminal failure cancels and settles; accumulated rows never become partial success. |
| Opening and post-open cancellation | `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:35-162,300-434` | `open_cancellable` fixes one deadline before opening and shares it across the one owner-cancel attempt and retained open. `cancel_and_settle` drains under the stream's original deadline, treats owner acknowledgement/absence as insufficient, and requires clean EOF for successful settlement. |
| MCP and scheduled siblings | `crates/wyrd/wyrd-server/src/mcp/bifrost.rs`; `query/scheduled.rs:88-255`; their recorded journeys | MCP reuses `QueryArguments` and `BoundedQuery`; scheduled queries reuse `open_cancellable` and `cancel_and_settle`. R5 changes neither sibling surface nor their deadline owners. |
| Oracle running owner and selected attempt | `crates/vala/vala-bifrost-redux/src/oracle/running.rs`; `oracle/mod.rs`; `oracle/query_stream.rs` | Registry cancellation targets the exact running request, pre-stream cancellation telemetry remains attributable, and local/forwarded streams retain one terminal owner with no successor attempt. |
| Distributed leader/follower release | `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs`; `analytical_supervisor.rs`; `resources.rs`; `architecture/bifrost-design.md:359-432` | Participant grant streams retain follower-local ownership. Closing/dropping the stream releases the follower graph; the leader does not await an acknowledgement. Late children return charges through the shared root. The deleted bespoke drain polling and idle refusal remain absent. |
| Forwarded Workflow journey | `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:72-378`; peer-cluster pause, membership, and baseline helpers | One production-shaped journey now covers cancel, omitted deadline, explicit longer deadline, explicit shorter deadline, and follower pod loss on fresh topologies. It exercises the real tool and forwarded Oracle path, not a helper substitute. |
| Revision 14 request changes | `crates/skald/skald-spec/src/request.rs`; `skald-workflow/src/route.rs`; `skald-agent/src/loop_runtime.rs`; R5 Revision 14 evidence | Provider-schema consolidation changes model dispatch only. It does not add a query selector, retry, deadline owner, cancellation path, or alternate collector. |

## Deadline, settlement, and recovery assessment

| Reachable path | Required result | Candidate evidence | Assessment |
|---|---|---|---|
| Tools are cloned before preparation fixes the run deadline | Every clone consumes the prepared executor's exact deadline, not a hydration-time sample | Shared `OnceLock`; bind after `Workflow::prepare`; focused `tools_use_the_prepared_run_deadline` recorded green | PASS |
| Tool omits `deadline_ms` | Query remains bounded by the prepared run and cannot yield an early tool failure that lets the scripted continuation succeed | `QueryTool::invoke` projects remaining time; `TerminalCause::Deadline` reaches `TimedOut` with no output despite a queued `DONE` continuation | PASS |
| Tool requests a longer positive deadline | Prepared run bound wins | `requested.min(remaining)`; `TerminalCause::LongerDeadline` sends `120_000` ms against a 20 s run and still reaches `TimedOut` at the run boundary with no output | PASS |
| Tool requests a shorter positive deadline | Caller query bound wins while the Workflow remains live | `TerminalCause::ShorterDeadline` sends `10_000` ms, observes `WYRD_VALA_504_QUERY_TIMEOUT` without columns, then the scripted continuation returns `DONE` and the run succeeds before its 20 s bound | PASS |
| Agent waiter is aborted by explicit cancel or total deadline | Drop signals the child token; tracked query owner retains response and settlement; Workflow commit follows owner completion or bounded honest incompleteness | `TaskTracker`, cancellation drop guard, `BoundedQuery`, `tools.drain()` before `accepted.finish`; forwarded cancel/deadline branches | PASS |
| Follower pod disappears after Analytical selection | No fallback/retry or rows; exact failed terminal; survivors recover capacity and answer later work | Pod-loss branch expects `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, no columns, baseline ownership on survivors, membership departure, snapshot refresh, and a later three-row query | PASS |
| Leader ends while follower-local cleanup continues | Grant-stream close is the release; leader waits for no acknowledgement | Analytical owner source and architecture match the fixed decision; tests observe eventual follower-local cleanup only | PASS |
| MCP or scheduled query uses the same lower-level owners | Workflow deadline binding must not alter those surfaces | Their established arguments/opening/settlement owners are unchanged | PASS |

## Prior-finding and remediation closure

| Finding | Current evidence | Result |
|---|---|---|
| `FIND-TASK-004-3`: cancellation during opening could exceed the original query deadline | `open_cancellable` fixes one deadline before open and bounds its one cancel plus retained open | **CLOSED** |
| `FIND-TASK-004-4`: pod-loss proof omitted exact failure and post-loss recovery | Current pod-loss branch asserts exact failure, no rows, surviving baselines, membership removal, refresh, and later query success | **CLOSED** |
| `FIND-TASK-004-18`: owner documentation implied a follower release acknowledgement | Current architecture/source use grant-stream close and asynchronous follower-local settlement without leader acknowledgement | **CLOSED** |
| `FIND-TASK-004-20` query portion: changed interface import/style drift | Current lifecycle control uses module-scope `Instant` and the prior closure remains unchanged | **CLOSED** |
| `FIND-TASK-004-21`: `RunTools` sampled before the prepared run | The prepared executor is the only sampler; all clones receive its exact instant through the one-time bind | **CLOSED** |
| `FIND-TASK-004-22`: positive omitted/longer/shorter deadline proof was missing | The existing forwarded Workflow-query journey now drives all three actual argument cases and distinguishes their required outcomes | **CLOSED** |

R3's published-attempt rule and Revision 14's provider representation do not
alter this domain and were not reopened.

## DRIFT assessment

No material DRIFT was found. The implementation and R5 proof use established
Rust/Tokio and repository mechanisms: `OnceLock`, `CancellationToken`,
`TaskTracker`, the existing relative millisecond query contract, Oracle stream
ownership, the existing `PeerCluster` journey, and existing pause/membership
observations. The R5 change adds two cases to the already-required journey; it
does not add a runtime protocol, deadline service, retry, polling owner,
release acknowledgement, check, setting, option, dependency, fixture system,
or alternate query engine.

The public query wire remains a relative millisecond duration. Requiring a new
absolute-deadline wire merely to restate the prepared bound would be an
unsupported Wyrd-specific mechanism and is not required here. The exact
prepared instant remains the run's cancellation boundary, while the existing
query contract carries the remaining positive duration and the tracked owner
settles under the established Oracle controls.

## Material findings

None.

## Verification evidence and limits

The R5 remediation records successful focused execution of
`workflow::workflow_forwarded_query_settles_before_the_run_ends`, the exact
prepared-deadline owner test, the server external-route journey, and broader
`fmt`, `lints`, `test:skald`, scoped `test:wyrd`, and
`test:bifrost:journey:oracle` lanes. Its Revision 14 evidence records the final
broader format, lint, codegen, boundary, Skald/shared/server/gateway, Python,
TypeScript, and Oracle journey lanes as green. Commit `c6a3276ad` is confined
to extending the existing Oracle Workflow journey with the omitted,
longer-positive, and shorter-positive deadline outcomes.

Those results were treated as recorded claims and checked against the named
source and assertions; this review did not rerun them. The evidence credibly
distinguishes all three required deadline-precedence outcomes through the real
tool and distributed Oracle path, retains cancel/pod-loss settlement and
post-settlement recovery, and uses no new runtime-only proof seam.

The existing test-support pause controls deterministically hold established
Oracle execution/cleanup boundaries. They do not prove performance and are not
used as such. The ordinary public wire's millisecond resolution means the
journey proves observable deadline precedence rather than sub-millisecond
clock equality; the separate owner test proves exact prepared-instant identity.
Together these are the focused and cross-process proof required by R4/R5.

No required authority, source, cumulative diff, prior remediation, caller,
consumer, or recorded evidence was unavailable.

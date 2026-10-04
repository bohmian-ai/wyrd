# TASK-004 R5 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Cumulative remediations: TASK-004 R1 through R4 in the preceding review directories

The candidate identity remained unchanged during this pass. `.codegraph/` is
absent, so navigation used the immutable Git diff, repository search, and
direct source inspection. Per the review direction, I ran no build, test,
Cargo, or mise command.

## Navigation and caller-to-result paths

- HTTP create/get/cancel enter through
  `components/workflow/routes.rs`, authorize in
  `WorkflowRunHost`, and use `WorkflowRuns` for scoped admission,
  reservation promotion, snapshot reads, cancellation, retention, and
  terminal release.
- `Preparation::prepare` pins the tenant's exact active graph, resolves only
  selected external bindings, rejects unsupported routes/tools, hydrates the
  Agents, and constructs one Skald `PreparedWorkflowRun`. `Preparation::run`
  publishes its queued snapshot, executes that same prepared run, drains query
  owners, and commits one terminal result.
- Skald's `WorkflowExecutor` owns the plan, total deadline, cancellation,
  bounded `JoinSet`, attempt counters, and `RunLedger`. Whole snapshots flow
  through `AcceptedRun::observe`; only `AcceptedRun::finish` stores a terminal
  snapshot and releases capacity.
- A declared `bifrost.query` resolves through the run-bound `RunTools`, then
  through the shared `BoundedQuery`, `RunningQueryControls`, and Oracle owners.
  `cards.get` uses the authorized exact-ref Cards path. The captured `Caller`
  supplies attribution and accepted scopes but contains no bearer credential.
- WyrdGateway steps enter the existing in-process `GatewayInvocation`; external
  routes use tenant-assigned direct egress bindings. Provider-tagged requests
  remain typed through persistence and dispatch, including the distinct Vertex
  branch.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Revision 13 provider-tagged request contract and exact stored route execution | `skald-spec/src/request.rs:18-103`; provider client dispatch; server Vertex path in `pg_workflow_runs.rs:2215-2410` | Recorded provider round-trip/mismatch, generated-schema, SDK, and stored-Vertex evidence; R2-R4 closure records | PASS |
| Scenario 1: admission is authenticated, audited, bounded, and side-effect-free on refusal | `host.rs:60-123,206-227,290-400`; `config.rs:1660-1789`; canonical permission/audit owners | `admission_is_audited_and_side_effect_free_on_refusal` and config evidence recorded in TASK-004 | PASS |
| Scenario 2: one tracked preparation and accepted run per tenant/principal/key; disconnect and shutdown cannot strand it | `runs.rs:275-367,530-635`; `host.rs:98-123,230-288` | `tracked_preparation_replay_and_disconnect`; R1 reservation/blocking-work closure evidence | PASS |
| Scenario 3: accepted authority survives submission only, cannot widen or transfer, and later requests authenticate afresh | captured `Caller` in `host.rs:104-111,230-400`; fresh create/get/cancel authorization in `host.rs:75-203`; run-bound tools/gateway | `accepted_authority_outlives_submission_only`; R1 real tenant and pinned-authority evidence | PASS |
| Scenario 4: declared read tools preserve authorization, audit, tenant scope, exact input/result bounds, cancellation, and terminal settlement | `tools.rs:98-350`; `query/collect.rs:173-324,353-440`; `RunningQueryControls::open_cancellable` and `cancel_and_settle` | `declared_tools_use_captured_scopes_and_owned_services`; forwarded cancel/deadline/pod-loss journey; R1 query closure | PASS |
| Scenario 5: WyrdGateway and ExtGateway keep their existing owners, per-call fallback/deadline/cancellation, dialect, credential, and egress boundaries | `host.rs:334-358,403-429`; `components/gateway/workflow.rs`; provider-tagged dispatch | `server_routes_keep_gateway_and_external_ownership`; stored Vertex and dialect journeys | PASS |
| Scenario 6: cancellation/deadline races, whole snapshots, retention, restart loss, and shutdown converge on one terminal result after required drains | `WorkflowExecutor::drive`; `RunLedger::finish`; `AcceptedRun::observe/finish` at `runs.rs:638-688`; `WorkflowRuns::drain` | `lifecycle_races_retention_and_shutdown`; forwarded Oracle journey; R1-R4 closure evidence | PASS |
| Scenario 7: graph preparation, input, step result, aggregate snapshot, concurrency, and terminal reserve stay bounded and sibling services remain usable | bounded pinning in `components/cards/resolve.rs`; limits assembled at `host.rs:318-373`; ledger reserve/accounting in `skald-workflow/src/run.rs` | `graph_and_snapshot_limits_preserve_sibling_services`; recorded focused/broader lanes | PASS |
| Public run lifecycle uses the exact registered graph and the same Skald engine; no server-only executor or hidden Workflow principal | `PinnedWorkflowGraph` -> `SkaldWorkflow::from_card_bodies` -> `prepare`/`execute` in `host.rs:319-400`; `PreparedWorkflowRun` in `workflow_surface.rs:672-713` | Real-server Rust client journeys and recorded regression lanes | PASS |
| Published `Running` reserves attempt one; an interrupted published step remains `Cancelled` with its timestamps; retries retain exact accounting | `workflow.rs:274-305,342-380`; `run.rs:125-188,221-280` | `prepared_run_keeps_its_id` and `bounded_attempt_lifecycle`; R3 evidence | PASS |
| Every run-bound query-tool clone receives the prepared run's absolute deadline once, after preparation and before acceptance/execution | `PreparedWorkflowRun::deadline` at `workflow_surface.rs:689-698`; `RunTools` shared `OnceLock` and `bind_deadline` at `tools.rs:43-95`; bind at `host.rs:382-400` | `tools_use_the_prepared_run_deadline` demonstrates nonzero pre-bind time and clone equality; its recorded RED distinguishes the former early sampling | PASS |
| Omitted/longer query deadlines are clipped to the prepared-run boundary, while an explicit shorter deadline remains shorter, with focused proof of all three cases | `QueryTool::invoke` uses `requested.min(remaining)` at `tools.rs:200-215` | The new test at `tools.rs:434-464` only compares stored instants and never invokes the tool; the forwarded journey supplies no explicit `deadline_ms`; `pg_workflow_runs.rs` only covers `deadline_ms: 0` rejection | **FAIL — BEHAVIOR-R5-001** |
| Crossing the former early boundary cannot let a tool timeout continue into a successful/failed run; shared-boundary expiry remains `TimedOut` | Deadline case now scripts the post-tool `DONE` continuation at `wyrd-testing/tests/bifrost/oracle/workflow.rs:134-148` and still requires `TimedOut`/no outputs at `:203-225` | Recorded rerun of `workflow_forwarded_query_settles_before_the_run_ends`; combined with the paused-clock deadline-identity test, this distinguishes the old implementation | PASS |
| Prior `FIND-TASK-004-1` through `-20` remain closed | Reservation ownership, open cancellation, tenant/authority tests, tool schemas, route/provider paths, import fixes, Revision 13 metadata, grant-stream-close documentation, and attempt lifecycle remain in the cumulative source | R1-R4 validated ledgers and current cited source/assertions | PASS |
| Non-goals remain excluded: no durable queue/recovery, second executor/query engine, Workflow principal, compatibility route, new lifecycle protocol, setting, option, checker, foreign-tenant credential harness, or follower-release acknowledgement | Cumulative diff and current owners; no database migration or new public remote language surface | Direct diff inspection | PASS |
| Human decisions remain fixed: deleted Oracle graph-drain polling and idle refusal stay deleted; follower release is grant-stream close; the leader awaits no release acknowledgement; foreign-tenant model execution remains outside this fixture; published-running and one-time deadline-bind semantics remain as directed | Oracle release source/docs, server journey scope, R3 attempt code, R4 bind code | Current source and prior validated closure records | PASS |

## Proposed findings

### BEHAVIOR-R5-001 — MISSING: R4 does not supply its required explicit-shorter deadline proof

- **Violated obligation:** TASK-004 R4 acceptance requires omitted and longer
  query deadlines to be clipped to the prepared-run boundary while an explicit
  shorter deadline remains shorter. Its focused-proof section expressly
  requires proof that the explicit shorter deadline wins.
- **Location:**
  `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:434-464`,
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:134-225`, and
  `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1957-1976`.
- **Evidence:** `tools_use_the_prepared_run_deadline` stops after comparing the
  shared `OnceLock` value with `PreparedWorkflowRun::deadline`; it never invokes
  `QueryTool` and therefore never observes the `requested.min(remaining)`
  branch. The forwarded journey omits `deadline_ms`, so it proves only the
  unqualified path. The server journey's sole explicit deadline-tool case is
  zero, which validates rejection rather than shorter-deadline precedence.
  The R4 evidence table attributes this obligation to broad lanes that contain
  no matching assertion.
- **Observable consequence:** the current source implements the minimum
  correctly, but the required evidence cannot distinguish it from a regression
  that always overwrites a caller's shorter bound with the run deadline. Such a
  regression would make the accepted query run longer than its caller-declared
  bound while all recorded R4 assertions still pass.
- **Required testable correction:** extend an existing run-tool/query test or
  existing Workflow query journey—without a new harness, setting, option, or
  checker—to drive an explicit positive deadline shorter than the prepared
  run's remaining time and observe that the existing query owner uses that
  shorter bound. Retain the existing omitted-deadline continuation proof and
  deadline-identity assertion. No production behavior or new mechanism is
  required unless the focused proof exposes a defect.

## Prior-finding assessment

`FIND-TASK-004-1` through `FIND-TASK-004-21` are source-closed. The R4
production correction uses the approved one-time shared bind and removes the
earlier independently sampled tool deadline. `BEHAVIOR-R5-001` is a new proof
gap, not a reopening of the approved mechanism or any human-settled Oracle,
tenant-fixture, attempt, or follower-release decision.

## Verification notes

- No build, test, Cargo, or mise command was run.
- Recorded command results were treated as evidence claims and checked against
  the named current assertions.
- The cumulative behavior and prior remediation paths are source-supported.
  The one failed matrix row is not a generic verification limitation: the R4
  remediation explicitly required the missing focused case.

## Overall result

**FAIL**

The cumulative implementation behavior inspected is consistent with Revision
13 and the original task, but TASK-004 R4's mandatory explicit-shorter query
deadline proof is absent.

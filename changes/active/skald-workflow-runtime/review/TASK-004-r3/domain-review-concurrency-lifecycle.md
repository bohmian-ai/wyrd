# Concurrency and Lifecycle Domain Review — TASK-004 R3

## Review Findings

### Critical

None.

### Important

#### DOMAIN-CONCURRENCY-R3-001 — INCORRECT: a published `running` step has zero attempts and can regress to `unstarted`

- **Violated obligation:** Revision 13 says that running and cancelled active
  steps have at least one attempt, interrupted active steps become `cancelled`
  while retaining their timestamps, and observable snapshot transitions do not
  regress (`spec.md:977-999,1659-1687`; TASK-004 Scenario 6 / AC-019 / AC-022).
- **Exact location:**
  `crates/skald/skald-workflow/src/workflow.rs:273-289,343-369,451-464` and
  `crates/skald/skald-workflow/src/run.rs:119-130,215-232`.
- **Evidence and reachability:** `WorkflowExecutor::drive` changes the ledger
  step to `Running` and synchronously publishes that complete snapshot before
  spawning/polling `StepTask`. `RunLedger::step_started` does not set
  `attempts`, so every such public running snapshot carries `attempts == 0`.
  The task increments the shared counter only when its future is first polled.
  If cancellation or the total deadline aborts the just-spawned task before
  that poll, `settle` sees zero attempts and leaves the ledger step `Running`;
  `RunLedger::finish` then rewrites that already-published active step to
  `Unstarted`, clears `started_at`, and leaves attempts at zero. This is the
  exact race encoded by `bounded_attempt_lifecycle` at
  `workflow.rs:1457-1507`, while `prepared_run_keeps_its_id` at
  `workflow.rs:788-820` checks statuses but never checks the running attempt
  invariant.
- **Observable consequence:** GET/transition observers can receive a public
  `running` step with an impossible zero attempt count. An immediate cancel,
  deadline, or shutdown can then return a terminal snapshot claiming that the
  same externally running step never started, losing its start timestamp
  instead of reporting a cancelled active step.
- **Required testable correction:** use the existing scheduler/ledger and
  attempt counter; treat the transition that publishes `Running` as the first
  attempt having begun. Set the step's attempt count and shared counter to one
  before publishing that snapshot, so an abort before the task's first poll is
  settled as `Cancelled` with start/end timestamps. Preserve the existing retry
  counter for later attempts. Extend the existing transition and pre-poll-abort
  tests to assert that every running snapshot has at least one attempt and that
  a published-running step cannot terminalize as `Unstarted`. Do not add a
  handshake protocol, channel, lifecycle owner, setting, or repository check.

#### DOMAIN-CONCURRENCY-R3-002 — DRIFT: the supervisor still describes follower release acknowledgement

- **Violated obligation:** the binding human decision and the implemented
  Oracle contract say follower release is closing the participant grant stream;
  the leader neither receives nor awaits a follower release acknowledgement.
  Unsupported lifecycle mechanisms must not be preserved as requirements.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:580-585`.
- **Evidence:** `AnalyticalSupervisor::draining_graphs` says that an
  "unacknowledged participant release" removes readiness and remains counted.
  The method actually counts only local `Draining` entries carrying
  `settlement_failure: Some(_)` (`analytical_supervisor.rs:590-603`). The
  implemented release owner says dropping grant streams is release and needs no
  acknowledgement (`analytical.rs:2256-2266,2690-2743`; `dispatcher.rs:611-642`),
  and the corrected R2 owner documentation says followers settle
  asynchronously and the leader never waits (`analytical.rs:2492-2506`). The
  real journeys accordingly wait for follower cleanup only before asserting a
  baseline (`wyrd-testing/tests/bifrost/server/query.rs:843-879`).
- **Observable consequence:** the supervisor's lifecycle documentation still
  advertises the bespoke acknowledgement concept R2 was meant to remove. A
  maintainer can read it as justification for restoring a release RPC, wait,
  retry, poll, or readiness refusal that has no implementation or approved
  contract.
- **Required testable correction:** delete the acknowledgement wording and
  describe the existing condition exactly: `draining_graphs` counts a graph
  only after this node records a local cleanup/settlement failure; ordinary
  asynchronous follower cleanup after grant-stream close is not such a
  failure. Close by source comparison with `AnalyticalParticipantGrants`,
  `ParticipantGrant`, and `AnalyticalGraphLifecycle::settle`. Add no release
  request, acknowledgement, retry, timer, poll, probe, option, test, or check.

### Suggestions

None.

## Open Questions

None. Both findings are bounded corrections under approved Revision 13 and do
not require a new concurrency, resource-ownership, public-contract, or
persistent-data decision.

## Reviewed Boundary and Authority Coverage

Immutable subject: base `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`,
candidate `f17726fb25df1fa513875dca8d92f0073340ee0a`, approved
`SPEC-skald-workflow-runtime` Revision 13, original TASK-004, and its R1/R2
remediations and validated ledgers. `.codegraph/` is absent, so source and
immutable Git diffs were used.

| Boundary | Authority and source traced | Result |
|---|---|---|
| One preparation and one accepted job per scoped key | Spec `1501-1657`, INV-018/019, AC-021; `workflow/runs.rs` key states, tracker token, reservation accept/fail/drop; `host.rs::create`; `tracked_preparation_replay_and_disconnect` | PASS. Scoped hash replay/conflict, one tracked preparer, slot transfer, waiter independence, and uncached failure align. |
| Permits, capacity, and retention | Spec `1501-1589,1703-1707`; `RunTable::{release,sweep,oldest_terminal}`, `WorkflowRuns::admit`, `AcceptedRun::finish`; lifecycle/graph-bound journeys | PASS. Preparations and active runs share the bounded accounting; terminal entries alone expire/evict and remove their keys. |
| Cancellation, deadline, snapshot publication, and terminalization | Spec `977-999,1659-1696`, AC-008/019/020/022; Skald `WorkflowExecutor`, `RunLedger`; server `AcceptedRun`; lifecycle race journey | **FAIL — DOMAIN-CONCURRENCY-R3-001.** Run-level terminal ownership is singular, but step publication violates the exact running/attempt and interrupted-active invariants. |
| Shutdown drain | Spec `1709-1715`; `WorkflowRuns::drain`, tracked blocking preparation, `BoundServer::run` shared deadline and ordering; shutdown cases in `pg_workflow_runs.rs` | PASS. Admission closes under the table lock, reservations already own tracker tokens, Workflow work drains before gateway/query/Bifrost owners under one deadline. |
| Accepted token-independent authority | Spec `837-889`, INV-022, AC-027; `Caller`, `Preparation`, `RunTools`, server gateway caller, fresh GET/cancel/create authorization; accepted-authority journey | PASS. The retained `Caller` contains verified principal/scopes/delegation and no bearer; later route requests authorize afresh and replay does not replace captured authority. The approved foreign-tenant model-step harness limit was not reopened. |
| Graph preparation and execution ownership | Spec `1568-1589`; `PinnedWorkflowGraph`, tracked `spawn_blocking`, `PreparedWorkflowRun`, one Skald executor; graph/snapshot-bound journey | PASS. Resolution/IO stays outside the run lock, pure preparation is tracked, and no second executor or queue appears. |
| Query owner cancellation and settlement | TASK-004 query-ownership section; `BoundedQuery`, `ResultCollector`, `ScheduledQueryCaller`, `RunningQueryControls::{open_cancellable,cancel_and_settle}`; forwarded Workflow/pod-loss evidence | PASS. The response is retained by tracked query work, cancellation uses the original deadline, partial rows are not returned as success, and Workflow terminal commit follows tool-owner drain. |
| Oracle admission, running-query state, resources, graph grants, and follower cleanup | `bifrost-design.md:329-475`; `oracle/{admission,running,analytical,analytical_supervisor,dispatcher,query_stream}.rs`; `resources.rs`; server and peer-cluster journeys | **FAIL — DOMAIN-CONCURRENCY-R3-002.** Runtime grant-stream-close ownership aligns and the deleted graph-drain polling/supervisor idle refusal remain deleted, but the sibling supervisor documentation still implies an acknowledgement mechanism. |
| State composition and route publication | `AppState` Workflow owner, boot/router composition, Workflow routes, accepted-run journey | PASS. One process owner is shared by routes and shutdown; no persistence or cross-replica recovery claim was introduced. |

## Prior-Finding and Human-Decision Assessment

- Recorded source and evidence continue to close `FIND-TASK-004-1` through
  `FIND-TASK-004-14`; R2's four targeted corrections are present at their named
  locations.
- The deleted Oracle graph-drain polling and supervisor idle refusal remain
  deleted. No replacement poll, retry, release RPC, option, setting, or custom
  check was introduced.
- Follower release remains grant-stream close and the leader does not await
  follower cleanup. `DOMAIN-CONCURRENCY-R3-002` is the remaining contradictory
  description, not a request to add an acknowledgement.
- The foreign-tenant journeys correctly avoid requiring a model step because
  the fixture provisions gateway credentials only for its fixture tenant.

## Verification Notes

- Per the strict review instruction, no build, test, Cargo, mise, or execution
  command was run. Review relied on the cumulative diff, current source, and
  recorded evidence.
- R1 records passing Skald/shared/Wyrd/principals/gateway/Bifrost and language
  lanes, including the focused preparation, shutdown, query settlement, and
  lifecycle journeys. R2 records `mise run lints`, 683 Wyrd tests, and 43 Oracle
  journeys after its behavior-neutral source corrections.
- Those recorded lanes support the passing boundaries but do not close
  `DOMAIN-CONCURRENCY-R3-001`: the transition test omits attempt assertions and
  the pre-poll-abort unit test explicitly expects the contradicted
  `Running -> Unstarted` outcome. `DOMAIN-CONCURRENCY-R3-002` is directly
  source-visible and requires no new behavioral test.
- Candidate identity was checked before writing this report and remained
  `f17726fb25df1fa513875dca8d92f0073340ee0a`.

## Overall Result

**FAIL**

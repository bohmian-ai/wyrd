# Concurrency and Lifecycle Domain Review — TASK-004 R4

## Review Findings

### Critical

None.

### Important

#### DOMAIN-CONCURRENCY-R4-001 — INCORRECT: the query-tool owner starts a different, earlier run deadline

- **Violated obligation:** Revision 13 defines one total Workflow deadline and
  requires an unqualified `bifrost.query` deadline to be the run's remaining
  time. Total-deadline expiry must abort active work and produce
  `WorkflowRunStatus::TimedOut`; a tool timeout is not permitted to pre-empt
  that terminal boundary merely because graph/runtime preparation took time
  (spec REQ-016–020, INV-020–021, AC-020; TASK-004's query-ownership contract
  and forwarded-Oracle acceptance evidence).
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/workflow/host.rs:358-399`,
  `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:171-205`, and
  `crates/skald/skald-workflow/src/workflow.rs:140-197`.
- **Evidence and reachable path:** `Preparation::prepare` constructs
  `RunTools` with `std::time::Instant::now() + timeout` at `host.rs:373-379`.
  It then moves those tools through `SkaldWorkflow::from_card_bodies` and runs
  all synchronous Workflow planning before `WorkflowExecutor::new` independently
  computes `tokio::time::Instant::now() + timeout` at `workflow.rs:157-168`.
  Therefore the query-tool deadline is earlier by all intervening hydration and
  planning time. `QueryTool::invoke` treats that earlier instant as the run's
  remaining time and passes it to `BoundedQuery` (`tools.rs:187-205`). A step
  that reaches `bifrost.query` without an explicit shorter deadline can thus
  receive a query timeout before the Skald total deadline. Tool failures are
  non-retryable under the approved contract; depending on the Agent response,
  the run can settle `Failed` or even complete after handling the tool error
  instead of reaching the later `TimedOut` boundary. The divergence is largest
  for the admitted deep graphs whose pure preparation is explicitly allowed.
- **Observable consequence:** the same accepted run has two competing absolute
  deadlines. A caller can observe terminal status and timing depend on graph
  preparation duration: the read tool can be cut short before
  `created_at + timeout_seconds`, contradicting the recorded journey's explicit
  assertion that the tool deadline is the run's remaining time and that a
  deadline run cannot end early
  (`crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:212-222`).
- **Required testable correction:** make the existing prepared run/executor's
  one fixed absolute total deadline the source consumed by `RunTools`; do not
  call the clock a second time to create a tool-only deadline. Preserve the
  existing `RunTools`, `PreparedWorkflowRun`, task trackers, cancellation tree,
  query owner, and public duration-based options. Add focused evidence at the
  existing preparation/query boundary that time spent in synchronous graph
  preparation cannot make an unqualified query expire before the run's total
  deadline, and that total expiry remains `TimedOut`. Do not add a deadline
  service, timer task, setting, option, retry, polling loop, or new test
  harness.

### Suggestions

None.

## Open Questions

None. The correction selects an already-approved owner and deadline; it does
not require a new public contract, concurrency semantic, persistent owner, or
cross-service decision.

## Reviewed Boundary and Authority Coverage

Immutable subject: base `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`,
candidate `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`, approved
`SPEC-skald-workflow-runtime` Revision 13, original TASK-004, and R1/R2/R3
remediations. `.codegraph/` is absent, so the cumulative Git diff and current
candidate source were used.

| Boundary | Authority and source traced | Result |
|---|---|---|
| Scoped idempotency and preparation waiters | Spec normative create flow, INV-018/019, AC-021; `WorkflowRunHost::create`; `WorkflowRuns::admit`; `Reservation::{accept,fail,drop}`; `tracked_preparation_replay_and_disconnect` | PASS. A scoped key has one preparer, matching waiters share its watch outcome, handler cancellation does not cancel ownership, and failures remove the key and release capacity. |
| Admission and active capacity | Spec server run-owner contract and REQ-034A/050; `RunTable::{release,sweep}`; `WorkflowRuns::{admit,drain}`; `AcceptedRun::finish` | PASS. Reservation promotion transfers rather than increments the active slot; only terminal runs are retention candidates; tenant-first/global eviction removes the matching key. |
| Tracked preparation and process-local task ownership | INV-013/018/019; `WorkflowRuns` tracker/token and tracked `spawn_blocking`; `Preparation::run`; `BoundServer::run` | PASS. The reservation owns a tracker token before handler suspension, blocking preparation remains counted after its waiter is cancelled, and shutdown closes admission under the same lock. |
| Whole-snapshot publication and terminal ownership | REQ-018–023/045/048/050, AC-019/022; Skald `WorkflowExecutor`/`RunLedger`; server `AcceptedRun`; lifecycle race journey | PASS. Non-terminal transitions replace complete snapshots, one consumed `AcceptedRun` commits the terminal snapshot after query-owner drain, and capacity releases afterwards. |
| Step attempt and retry lifecycle | Revision 13 step-result invariants and R3; `WorkflowExecutor::{drive,settle}`, `StepTask::run`, `RunLedger::{step_started,finish}`; `prepared_run_keeps_its_id`, `bounded_attempt_lifecycle` | PASS. Publication reserves attempt one before `Running`; the first task poll consumes that attempt; later retries increment it; pre-poll interruption settles `Cancelled` with attempt one and timestamps; pending steps alone become `Unstarted`. |
| Cancellation, total deadline, and query-owner settlement | Workflow deadline/retry contract, INV-020/021, TASK-004 query ownership; `RunTools`, `QueryTool`, `BoundedQuery`, Oracle lifecycle controls, forwarded Workflow journey | **FAIL — DOMAIN-CONCURRENCY-R4-001.** Cancellation/drop ownership and settlement ordering pass, but the query owner does not consume the executor's one total deadline. |
| Shutdown sequencing | Normative shutdown flow, REQ-034A/048/050; `WorkflowRuns::drain`; `BoundServer::run`; shutdown cases in `pg_workflow_runs.rs` | PASS. Workflow admission and tracked work drain before gateway/query/Bifrost dependencies under the same process deadline, and exhaustion is reported as unclean. |
| Fixed human decisions and Oracle lifecycle | Current Bifrost authority; cumulative Oracle diff; R2/R3 remediation constraints | PASS. Deleted graph-drain polling and supervisor idle refusal remain deleted; follower release remains grant-stream close with no leader acknowledgement; no replacement protocol, option, setting, or check was added. |
| Accepted authority and foreign-tenant harness boundary | INV-022, AC-027; retained `Caller`, fresh create/get/cancel authorization, server gateway/tool callers, accepted-authority journey | PASS. Accepted execution retains scoped attribution rather than a bearer token, later requests authorize afresh, and the approved foreign-tenant credential limitation was not reopened. |

Applicable authority reviewed included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-security-posture.md`, the Workflow-relevant Bifrost lifecycle
authority, the spec-driven development and maintainer references, Revision 13,
TASK-004, and all three remediation tasks. No unsupported production mechanism,
check, file, setting, or option was accepted or requested. The reservation
tracker token, `JoinSet`, cancellation tokens, task trackers, watch snapshots,
and absolute `timeout_at` boundaries are established Tokio/repository patterns.

## Prior-Finding and Human-Decision Assessment

- R3's observable attempt finding is closed at its producer: `Running`
  publication and the shared counter reserve attempt one before the task can be
  aborted. The corrected tests assert attempt one and retained cancellation
  timestamps.
- The R1 tracked-reservation, tracked-blocking-preparation, and query-open
  settlement findings remain closed. R2's source/authority corrections remain
  present.
- The deleted Oracle graph-drain polling, supervisor idle refusal, and
  release-acknowledgement narrative remain absent from changed authority. No
  replacement mechanism was introduced.
- Follower release is still participant grant-stream close; the leader does
  not await an acknowledgement. Foreign-tenant model execution remains outside
  the fixture's seeded-credential capability, as directed.

## Verification Notes

- Per the strict review instruction, no build, test, Cargo, or mise command was
  run. Review relied on the cumulative diff, candidate source, and recorded
  evidence.
- Recorded R1–R3 evidence covers Skald executor behavior, preparation tracking,
  Workflow server journeys, shutdown, Oracle settlement, and the corrected
  step-attempt assertions. That evidence supports the passing rows above.
- The forwarded-Oracle journey asserts that a deadline case ends no earlier
  than `created_at + timeout`, but its scripted provider does not return a
  terminal Agent response after the earlier query-tool timeout. It therefore
  does not falsify the two-clock path in `host.rs`; the source divergence
  remains reachable through an Agent that handles or immediately terminates on
  that tool error.
- Candidate identity was checked before writing and remained
  `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`.

## Overall Result

**FAIL**

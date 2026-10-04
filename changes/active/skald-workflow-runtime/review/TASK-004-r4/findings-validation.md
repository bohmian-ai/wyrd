# Structured Ponytail Validation — TASK-004 R4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1, R2, and R3 in the preceding review directories
- Prior validated ledgers: `TASK-004-r1/findings-validation.md` through
  `TASK-004-r3/findings-validation.md`
- Review mode: complete cumulative diff, applicable authority, every required
  R4 discovery report, the focused follow-up, current source and callers, and
  recorded evidence only. No build, test, Cargo, or mise command was run.

The candidate resolved to the requested immutable commit before validation.
`.codegraph/` is absent, so immutable Git objects, `rg`, and direct source
inspection were used.

## Validation completeness

Every R4 report was read. The behavior, invariant, standards, maintainer,
system-resilience, security/tenancy, query-settlement, and provider-contract
reviews proposed no finding. The concurrency/lifecycle review proposed
`DOMAIN-CONCURRENCY-R4-001`; the focused follow-up traced its disputed clock
origin and reachability and proposed no additional finding.

Validation independently read the complete bodies and callers of
`Preparation::{run,prepare}`, `Workflow::{from_card_bodies,prepare}`,
`WorkflowExecutor::{new,drive}`, `PreparedWorkflowRun`, `RunTools`,
`AgentRunTools`, `QueryTool::invoke`, `BoundedQuery::run`, the Agent tool loop,
and the forwarded-Oracle journey. It also checked the sibling MCP and scheduled
query consumers to ensure the correction does not move or duplicate their
query deadlines or settlement ownership.

The fixed human decisions were treated as authority and not reopened. Deleted
Oracle graph-drain polling and supervisor idle refusal stay deleted; follower
release is participant grant-stream close and the leader awaits no release
acknowledgement; foreign-tenant journeys need no unprovisioned model step; and
a published `Running` step reserves attempt one and settles `Cancelled` when
interrupted.

## Proposal validation

### `DOMAIN-CONCURRENCY-R4-001` — REVISED

The proposal's core defect is confirmed, with the focused follow-up's narrower
causal statement: the problem is not that Tokio and standard-library instants
necessarily have different production clock origins. The server and Skald
sample two different instants at different points in preparation.

#### Producer-to-consumer trace

1. `WorkflowRunHost::create` validates one relative timeout and moves it into
   tracked `Preparation` (`components/workflow/host.rs:75-123`).
2. After graph pinning, binding resolution, and suitability checks,
   `Preparation::prepare` creates `RunTools` with
   `std::time::Instant::now() + timeout` (`host.rs:358-380`).
3. Those tools are cloned into each hydrated Agent by
   `Workflow::from_card_bodies` and `CardBodyResolver`; their deadline value is
   therefore already fixed before Skald planning (`skald-workflow/src/bodies.rs:79-109,194-225`).
4. `Workflow::prepare` then builds the complete `ExecutionPlan` and only after
   that calls `WorkflowExecutor::new`
   (`skald-workflow/src/workflow_surface.rs:555-595`).
5. `WorkflowExecutor::new` independently samples
   `tokio::time::Instant::now() + timeout` and stores that as the total run
   deadline (`skald-workflow/src/workflow.rs:140-197`). The queued snapshot and
   its `created_at` are created at this later boundary in `RunLedger::new`.
6. An unqualified `bifrost.query` computes `deadline_ms` from the earlier
   `RunTools.deadline` and passes it through the existing `BoundedQuery` and
   Oracle lifecycle owners (`components/workflow/tools.rs:171-215`;
   `query/collect.rs:275-324`).
7. A tool error becomes an `ok: false` conversation turn rather than forcing
   immediate run terminalization. The Agent loop can issue another model call
   and can subsequently return success or an ordinary failure before the
   executor's later total deadline (`skald-agent/src/loop_runtime.rs:418-559`).

Let `T_tools` be the sample in `host.rs` and `T_run` the sample in
`WorkflowExecutor::new`. Hydration and `ExecutionPlan::build` occur between
them, so ordinary execution permits `T_run > T_tools`. The two deadlines are
therefore `T_tools + timeout` and `T_run + timeout`; they are not projections
of one fixed boundary. This path is reachable for an Agent that declares
`bifrost.query`, omits `deadline_ms`, has no shorter step or Agent deadline,
and issues a query spanning the earlier boundary.

The forwarded-Oracle deadline journey does not close the gap. It provides only
the initial tool call in the deadline case and holds later provider work, so an
early query error is followed by a pending model request until the later Skald
deadline. Its final `TimedOut` assertion therefore cannot distinguish one
shared deadline from the observed two-deadline sequence.

#### Task relevance and consequence

Revision 13 requires a lower remaining step/run deadline to constrain
`bifrost.query`, fixes the total Workflow deadline as the only source of the
`TimedOut` run status, and makes the absolute total deadline part of prepared
run ownership (`spec.md:788-817,1314-1393,1814-1854`). TASK-004 likewise says
the query is bounded by the remaining Workflow deadline and records the
forwarded deadline journey as its proof. The implementation is new in the
cumulative candidate and the path is used by accepted server jobs, so it is
neither dormant, test-only, speculative, nor unrelated debt.

At the earlier tool boundary, the query may return a redacted failure while
the total Workflow deadline has not expired. The existing Agent loop can then
produce a `Succeeded` or `Failed` run before the later boundary, or remain
pending and eventually produce `TimedOut`. Terminal status and timing can
therefore depend on allowed hydration/planning duration rather than the one
prepared run deadline.

#### Ponytail ladder and corrected boundary

1. **Delete:** deleting the query deadline projection is invalid. Query
   opening and settlement require an absolute bound, and an unqualified query
   must still be clipped to the Workflow boundary.
2. **Existing repository owner:** `WorkflowExecutor`, wrapped by
   `PreparedWorkflowRun`, already fixes the authoritative absolute total
   deadline. That value must be reused; a second deadline owner is unnecessary.
3. **Standard/native mechanism:** because Agent tool instances are cloned
   during hydration before the prepared executor exists, the existing
   `RunTools` clones may share a one-time binding using the standard library's
   ordinary one-assignment primitive. No timer service, task, channel, or
   mutable configuration is justified.
4. **Installed dependency:** no dependency solves an ownership error better
   than projecting the existing value directly.
5. **Minimum correction:** project the already-fixed absolute deadline from
   the prepared run/executor and bind that exact value once into the existing
   `RunTools` shared by the hydrated Agents before acceptance or execution.
   `QueryTool` must derive remaining time only from that bound. Preserve the
   public duration-based run option, `PreparedWorkflowRun`, `RunTools`, task
   trackers, cancellation tree, `BoundedQuery`, Oracle settlement, and all
   sibling MCP/scheduled-query behavior. Do not resample either clock and do
   not add a deadline service, retry, poll, setting, option, check, or new
   harness.

The one-time binding is warranted only by the existing hydration order: tools
are materialized before Skald can finish preparing the run, but no tool can be
invoked before preparation returns and execution starts. It is not a new
extension point or configurable mechanism. The correction projects an already
approved invariant across the existing server/Skald seam; it does not require
a new product behavior, public wire contract, security decision, persistence
model, or concurrency semantic.

#### Focused closure proof

Add focused proof at the existing server preparation/tool seam that the
deadline held by every cloned `RunTools` instance is the exact absolute
deadline fixed by its `PreparedWorkflowRun`, including after nonzero
synchronous hydration/planning work. Then exercise an unqualified
`bifrost.query` across that boundary and prove it cannot produce a tool timeout
before the prepared run deadline; expiry of that shared boundary still yields
`WorkflowRunStatus::TimedOut`. Reuse the existing Workflow/tool fixtures and
forwarded-Oracle journey. No new test binary, clock service, pause protocol,
fixture system, or repository check is warranted.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-20` remain **CLOSED**. The new finding
does not reopen or renumber them.

| Prior IDs | Closure retained at | Status |
|---|---|---|
| `FIND-TASK-004-1`, `-2` | Reservation-time tracker ownership and tracked blocking preparation | **CLOSED** |
| `FIND-TASK-004-3`, `-4` | Original query-deadline cancellation during open and forwarded pod-loss settlement/recovery | **CLOSED** |
| `FIND-TASK-004-5`, `-8`, `-14` | Real tenant boundaries, pinned/captured authority, and aligned design/security authority | **CLOSED** |
| `FIND-TASK-004-6`, `-7`, `-10` | Built-in declarations, exact schemas/bounds, and negative tool/query journeys | **CLOSED** |
| `FIND-TASK-004-9`, `-11`, `-12`, `-13` | Idempotency, gateway, lifecycle, graph/snapshot, and sibling-service journeys | **CLOSED** |
| `FIND-TASK-004-15` | Adjacent `provider`/`body` Prompt examples and fixtures | **CLOSED** |
| `FIND-TASK-004-16`, `-20` | Required module-scope imports and bare interface types, without a new enforcement mechanism | **CLOSED** |
| `FIND-TASK-004-17` | Active task/remediation Revision 13 authority metadata | **CLOSED** |
| `FIND-TASK-004-18` | Grant-stream-close follower release documentation with no leader acknowledgement | **CLOSED** |
| `FIND-TASK-004-19` | Attempt one is reserved before `Running`; interrupted published work settles `Cancelled` with timestamps | **CLOSED** |

The deleted Oracle polling/refusal behavior remains absent, follower release
remains stream close, and no foreign-tenant credential expansion or
remediation-only mechanism is required.

## Final deduplicated finding ledger

### FIND-TASK-004-21 — REVISED — INCORRECT: the query tool uses an earlier deadline than the prepared run

- **Discovery sources:** `DOMAIN-CONCURRENCY-R4-001` and the R4 focused
  follow-up.
- **Violated obligation:** Revision 13's built-in-tool and deadline contracts
  require `bifrost.query` to use the lower remaining step/run deadline and
  reserve `WorkflowRunStatus::TimedOut` for expiry of the one total Workflow
  deadline. TASK-004 requires the same remaining-run bound for accepted query
  ownership.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/workflow/host.rs:358-399`;
  `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:42-78,171-215`;
  `crates/skald/skald-workflow/src/workflow_surface.rs:555-595,676-704`;
  `crates/skald/skald-workflow/src/workflow.rs:140-197`; and the insufficient
  proof at
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:134-145,200-222`.
- **Evidence:** `RunTools` samples `now + timeout` before synchronous Agent
  hydration and execution-plan construction. `WorkflowExecutor::new` samples
  `now + timeout` afterward and owns the prepared run's total deadline.
  `QueryTool` clips an unqualified query to the earlier value. The Agent loop
  can continue after that tool failure, while the recorded journey holds the
  continuation until the later executor deadline.
- **Observable consequence:** one accepted run has two deadline boundaries. A
  query can fail before the prepared total deadline, and the run may then
  succeed, fail, or time out depending on the model continuation rather than
  solely on the approved total-deadline contract.
- **Decision-complete smallest correction:** reuse the absolute deadline
  already owned by the prepared Skald executor. Bind that exact value once
  into the existing shared `RunTools` after preparation and before
  acceptance/execution; all cloned query tools derive remaining duration from
  it. Preserve duration-based options and existing owners. Add no second
  sampler, timer service, task, channel protocol, retry, poll, setting, option,
  checker, dependency, or new harness.
- **Focused closure proof:** at the existing preparation/tool seam, prove the
  cloned tools carry the prepared run's exact deadline despite intervening
  synchronous planning, and prove an unqualified query crossing the former
  early boundary does not return a tool timeout before the run boundary while
  shared-boundary expiry remains `TimedOut`. Retain the existing forwarded
  Oracle journey as broader settlement/recovery proof.

## Validated ledger summary

| Stable ID | Status | Classification |
|---|---|---|
| `FIND-TASK-004-1` through `FIND-TASK-004-20` | **CLOSED** | Prior remediation findings |
| `FIND-TASK-004-21` | **REVISED** | INCORRECT |

No retained correction requires a specification revision. It corrects a
bounded implementation seam to project one already-approved deadline from its
existing owner and changes no public wire behavior, product scope, security or
tenancy decision, persistence boundary, or follower-release contract.

## Verification limits and blockers

- No build, test, Cargo, or mise command was run, as required.
- Recorded evidence was treated as a claim and checked against current source
  and assertions. It credibly closes prior findings and the other reviewed
  boundaries, but no recorded assertion compares the producer of
  `RunTools.deadline` with the producer of `WorkflowExecutor.deadline`.
- No required report, authority, source, caller trace, cumulative diff, prior
  ledger, or recorded evidence was unavailable. The report conflict is
  resolved from source and the focused follow-up. There is no validation
  blocker.

## Validation result

**COMPLETE — one retained finding: `FIND-TASK-004-21`.**

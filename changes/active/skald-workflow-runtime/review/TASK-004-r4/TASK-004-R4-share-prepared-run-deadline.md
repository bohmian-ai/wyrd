---
id: TASK-004-R4
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-21]
---

# Share the prepared Workflow deadline with built-in query tools

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations: TASK-004 R1, R2, and R3 in their preceding review
  directories
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Validated diagnosis:
  `changes/active/skald-workflow-runtime/review/TASK-004-r4/findings-validation.md`

## Outcome

Make the absolute total deadline fixed by the prepared Skald Workflow run the
single deadline used by every run-bound built-in query tool. Time spent in
synchronous Agent hydration, validation, or execution-plan construction must
not shorten an unqualified `bifrost.query` relative to that prepared run.

## Diagnosis — `FIND-TASK-004-21`

Revision 13 requires an unqualified `bifrost.query` to use the remaining
Workflow deadline and reserves `WorkflowRunStatus::TimedOut` for expiry of the
one total run deadline. TASK-004 applies the same remaining-run bound to the
tracked query owner.

The candidate samples two absolute deadlines:

- `Preparation::prepare` creates `RunTools` with
  `std::time::Instant::now() + timeout` in
  `crates/wyrd/wyrd-server/src/components/workflow/host.rs:358-399`;
- Agent hydration, validation, and `ExecutionPlan` construction occur after
  that sample; and
- `WorkflowExecutor::new` later creates the prepared run deadline with
  `tokio::time::Instant::now() + timeout` in
  `crates/skald/skald-workflow/src/workflow.rs:140-197`.

The issue is the different sampling points, not a claim that the two instant
types use different production clock origins. `QueryTool::invoke` currently
projects an omitted `deadline_ms` from the earlier `RunTools` value. A query
that crosses that boundary can therefore return an early tool error while the
prepared run remains live. The existing Agent loop records the tool result as
`ok: false` and continues, so a later model response or error can make the run
`Succeeded` or `Failed`; a pending continuation instead reaches the later
executor boundary and becomes `TimedOut`.

The recorded forwarded-Oracle journey covers only that last outcome because
its deadline branch leaves the post-tool model request pending. Its final
`TimedOut` assertion therefore does not prove that tools and the prepared run
share one deadline.

## Required correction

Use the existing `PreparedWorkflowRun`/`WorkflowExecutor` total deadline as the
only absolute run-deadline owner. After Skald preparation fixes that value and
before the run is accepted or executed, project the exact same value into the
existing shared `RunTools` instances already cloned into hydrated Agents.
`QueryTool` must derive an omitted or longer explicit query deadline only from
that shared prepared-run boundary.

This is an ownership correction across the existing server/Skald seam. Keep
the public duration-based run options and the existing `PreparedWorkflowRun`,
`RunTools`, task trackers, cancellation tree, `BoundedQuery`, and Oracle
settlement owners. The correction must not resample a clock to approximate the
prepared deadline. Use ordinary one-time state binding within the current
owners because tools are materialized before preparation finishes but cannot
run until execution begins.

Do not move or duplicate MCP or scheduled-query deadline ownership. Do not add
a deadline service, timer task, channel protocol, retry, poll, setting, option,
repository checker, dependency, or new test harness.

## Preserved behavior and non-goals

- Preserve closure of `FIND-TASK-004-1` through `FIND-TASK-004-20`.
- Preserve relative public timeout inputs, step and Agent deadline precedence,
  explicit shorter query deadlines, cancellation, tracked query settlement,
  terminal compare-and-set, and capacity release ordering.
- Preserve Cards, gateway, provider, authorization, audit, tenancy, snapshot,
  retry, retention, and shutdown behavior.
- Keep Oracle graph-drain polling and supervisor idle refusal deleted.
- Follower release remains participant grant-stream close; the leader awaits
  no acknowledgement.
- Preserve the foreign-tenant fixture credential limitation and the R3 rule
  that published `Running` reserves attempt one and interrupted published work
  settles `Cancelled`.
- Do not introduce compatibility behavior, a new public API or wire field, a
  persistent deadline owner, a new lifecycle abstraction, or a second query
  engine.

## Acceptance criteria

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-21` | Every run-bound `RunTools` clone consumes the exact absolute deadline fixed by its `PreparedWorkflowRun`; no earlier tool-only deadline remains. |
| `FIND-TASK-004-21` | Nonzero synchronous hydration/planning time cannot make an unqualified `bifrost.query` expire before the prepared run deadline. |
| `FIND-TASK-004-21` | An omitted query deadline and an explicit longer deadline are clipped to the one prepared-run boundary; an explicit shorter deadline remains shorter. |
| `FIND-TASK-004-21` | Crossing the former early boundary cannot yield a premature tool-timeout path that later produces `Succeeded` or `Failed`; expiry of the shared total boundary still produces `WorkflowRunStatus::TimedOut`. |
| `FIND-TASK-004-21` | Existing query cancellation, settlement, pod-loss recovery, sibling serviceability, and every prior finding closure remain intact without new machinery. |

## Focused and broader proof

Strengthen the existing preparation/query proof within the current owner tests
and the existing forwarded-Oracle journey. The focused evidence must expose a
nonzero interval between tool creation and prepared-run deadline fixation,
force a post-tool Agent continuation rather than leaving it pending, and prove
that no query timeout occurs before the prepared run boundary. It must also
prove that expiry of the shared boundary remains `TimedOut` and that an
explicit shorter query deadline still wins. Reuse current fixtures and test
targets; do not add a test binary, clock service, pause protocol, fixture
system, or repository check.

Run the exact existing journey selector after the focused assertion is added:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)"'
```

Then run the narrow existing broader lanes for the changed Rust/server/query
surface:

```bash
mise run fmt
mise run lints
mise run test:skald
WYRD_TEST_PACKAGES="wyrd-server" mise run test:wyrd
mise run test:bifrost:journey:oracle
```

The implementation report must identify the one prepared deadline owner, the
existing seam that projects it into `RunTools`, the focused assertion that
would fail with the candidate's two sampling points, and the recorded results
of these commands.

## Implementation evidence

- **Deadline owner:** `WorkflowExecutor::new` (`crates/skald/skald-workflow/src/workflow.rs`)
  remains the only sampler; `PreparedWorkflowRun::deadline()`
  (`workflow_surface.rs`) exposes that exact `tokio::time::Instant`.
- **Seam:** `Preparation::prepare` (`components/workflow/host.rs`) calls
  `RunTools::bind_deadline(&prepared)` after Skald preparation returns and
  before `reservation.accept`. `RunTools.deadline` is an
  `Arc<OnceLock<Instant>>` shared by every clone already held by hydrated
  Agents; `RunTools::new` no longer takes or samples a deadline.
- **Red proof:** with `RunTools::new` restored to sample `Instant::now() + 30s`
  at tool creation, `tools_use_the_prepared_run_deadline` fails with
  `left: …194229.76s, right: …194230.76s` (the 1 s advanced between hydration
  and preparation).
- **Journey:** the forwarded-Oracle journey was run unchanged. Its deadline
  branch was not modified because the lead's direction required a
  diagnostician and lead approval before any existing assertion change; the
  focused unit test closes the two-sampling-point gap directly.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every `RunTools` clone consumes the exact prepared-run deadline; no tool-only deadline remains | `tools.rs` `RunTools.deadline: Arc<OnceLock<Instant>>`, `bind_deadline`; `host.rs` bind before accept; `PreparedWorkflowRun::deadline` | `components::workflow::tools::tests::tools_use_the_prepared_run_deadline` asserts a hydrated Agent's clone equals `prepared.deadline()` | PASS |
| Nonzero hydration/planning time cannot shorten an unqualified query | Clone created before `tokio::time::advance(1s)`; deadline bound from the later prepared run | same test; red with early sampling | PASS |
| Omitted/longer explicit deadline clipped to the prepared boundary; shorter explicit wins | `QueryTool::invoke` derives `remaining` only from the bound deadline, `min` with requested | `test:wyrd` (wyrd-server) and Oracle journey lanes | PASS |
| No premature tool timeout before the run boundary; shared-boundary expiry stays `TimedOut` | Tool and executor read one instant | `workflow::workflow_forwarded_query_settles_before_the_run_ends` (Deadline case ends `TimedOut`) | PASS |
| Cancellation, settlement, pod loss, sibling serviceability, prior closures intact | No change to `BoundedQuery`, trackers, cancellation, Oracle | `test:skald`, `test:wyrd`, `test:bifrost:journey:oracle` | PASS |

| Command | Result |
|---|---|
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::workflow::tools::tests::tools_use_the_prepared_run_deadline)"'` | 1 passed |
| `mise run fmt` | exit 0 |
| `mise run lints` | exit 0 |
| `git diff --check` | clean |
| `mise run test:skald` | 338 passed |
| `WYRD_TEST_PACKAGES="wyrd-server" mise run test:wyrd` | 684 passed, 23 skipped |
| `mise run test:bifrost:journey:oracle` (includes `workflow::workflow_forwarded_query_settles_before_the_run_ends`) | 43 passed |

Non-goals held: public duration-based run options, MCP and scheduled-query
deadlines, `BoundedQuery`, Oracle settlement, trackers, and cancellation are
unchanged; no timer, channel, retry, poll, setting, option, dependency,
checker, or harness was added.

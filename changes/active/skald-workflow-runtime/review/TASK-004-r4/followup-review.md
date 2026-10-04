# Focused follow-up — Workflow/query deadline origin

## Subject and uncertainty

- Immutable subject: base `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`,
  candidate `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`, approved
  `SPEC-skald-workflow-runtime` Revision 13, original TASK-004 plus R1/R2/R3.
- Conflict investigated: `DOMAIN-CONCURRENCY-R4-001` reports that the server
  query tool and Skald executor derive different absolute run deadlines, while
  the behavior, invariant, system, and query-settlement reviews accepted the
  deadline path.
- Review mode: source, cumulative diff, and recorded evidence only. No build,
  test, Cargo, or mise command was run. `.codegraph/` is absent.

## Source path inspected

1. `WorkflowRunHost::create` validates the relative timeout and moves it into
   tracked `Preparation` (`crates/wyrd/wyrd-server/src/components/workflow/host.rs:75-123`).
2. `Preparation::run` calls `Preparation::prepare`, accepts the resulting queued
   snapshot, executes the prepared Skald run, drains `RunTools`, and only then
   commits the terminal snapshot (`host.rs:244-287`).
3. `Preparation::prepare` constructs `WorkflowRunOptions` with the relative
   `timeout`, then constructs `RunTools` with
   `std::time::Instant::now() + timeout` before entering tracked blocking
   hydration/planning (`host.rs:308-399`, especially `358-393`).
4. The blocking closure runs `SkaldWorkflow::from_card_bodies`, including
   resolved Agent/Prompt hydration and validation
   (`crates/skald/skald-workflow/src/bodies.rs:66-107,187-242`), then calls
   `Workflow::prepare`.
5. `Workflow::prepare` builds the complete `ExecutionPlan` before calling
   `WorkflowExecutor::new` (`crates/skald/skald-workflow/src/workflow_surface.rs:555-595`).
   Only `WorkflowExecutor::new` samples
   `tokio::time::Instant::now() + timeout` and stores that value as the
   executor's total deadline (`crates/skald/skald-workflow/src/workflow.rs:140-197`).
   `RunLedger::new` then records the queued snapshot's `created_at`
   (`crates/skald/skald-workflow/src/run.rs:63-83`).
6. `QueryTool::invoke` computes an unqualified call's relative deadline from
   the earlier `RunTools.deadline`, clips an explicit deadline to it, and passes
   that duration through `BoundedQuery` to the ordinary query service and
   `RunningQueryControls::open_cancellable`
   (`crates/wyrd/wyrd-server/src/components/workflow/tools.rs:171-215`;
   `crates/wyrd/wyrd-server/src/query/collect.rs:173-257,275-324`;
   `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:119-162`).
7. On a query failure, `QueryTool` returns a redacted structured `ToolError`
   (`tools.rs:208-215,319-332`). The Agent loop does not immediately return
   that error: it records an `ok: false` tool-result turn and continues to the
   next model iteration (`crates/skald/skald-agent/src/loop_runtime.rs:418-559`).
   A subsequent ordinary model stop is a successful `AgentRun`, which
   `AttemptOutcome::from_agent` projects to a successful step
   (`crates/skald/skald-workflow/src/attempt.rs:67-125`). The executor races
   that work against its later total deadline and only that boundary selects
   `RunEnding::TimedOut` (`workflow.rs:245-333,432-580`).

## Resolution

### Clock origin

The instant types are not inherently based on unrelated production clocks.
The pinned Tokio implementation wraps `std::time::Instant`; when time is not
paused, `tokio::time::Instant::now()` samples `std::time::Instant::now()`.
Tokio's test clock can diverge from `std::time::Instant` when paused, which is
an additional reason not to treat two independent calls as one captured value,
but the production defect does not depend on that distinction.

The material difference is the sampling point. Let `T_tool` be the instant at
`host.rs:378` and `T_run` the instant at `workflow.rs:161`. In an ordinary
unpaused runtime, the blocking closure performs hydration, resolved validation,
and `ExecutionPlan::build` between them, so `T_run >= T_tool`. The stored values
are therefore `T_tool + timeout` and `T_run + timeout`, not two projections of
one absolute deadline. For sufficiently non-trivial allowed preparation,
especially the approved deep-graph path, the query bound can precede the
executor bound by the intervening preparation time. Millisecond projection and
the query service's later relative-duration capture may shift the exact query
instant slightly, but they do not guarantee convergence with the executor's
independently captured deadline.

Revision 13 requires the built-in query to remain subject to the lower remaining
step/run deadline (`spec.md:791-798`), fixes the total deadline's precedence and
terminal meaning (`spec.md:1352-1362,1386-1393`), and explicitly admits bounded
deep-graph preparation (`spec.md:1579-1589`). Skald's own public preparation
contract says the one absolute total deadline is fixed by preparation
(`workflow_surface.rs:555-595`). The query tool therefore has to consume that
same run boundary; an earlier independently derived duration is not the run's
remaining time.

### Reachability and observable outcome

The path is reachable for an Agent that declares `bifrost.query`, omits
`deadline_ms`, has no shorter step/Agent deadline, and issues a query that stays
active across the earlier query boundary. At that boundary the query can return
a redacted failure while the executor's later total deadline has not fired.
Because the Agent loop exposes a failed tool result to the model and continues,
the remaining interval is observable:

- a prompt/model that answers normally after seeing the tool error can produce
  a `Succeeded` step and run before the executor boundary;
- a prompt/model or loop that ends in an ordinary error before that boundary
  can produce `Failed`; or
- a provider call that remains pending reaches the later executor deadline and
  produces `TimedOut`.

The recorded forwarded-Oracle deadline journey covers only the third outcome.
It scripts the first model response as the tool call, scripts a second `DONE`
response only for the pod-loss case, and deliberately holds any request after
the scripted replies are exhausted
(`crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:134-145,559-575`).
Thus, after an earlier query timeout in the deadline case, the next model call
remains pending until the executor's later deadline. The assertion that the run
is `TimedOut` no earlier than `created_at + timeout`
(`crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:200-222`) cannot
distinguish one shared deadline from this two-deadline sequence. The other
recorded timeout tests hold provider work and
likewise prove executor expiry, not the preparation-to-query clock seam.

The PASS reports correctly traced query-owner retention, cancellation, and
settlement after `QueryTool::invoke`; they did not compare the producer of
`RunTools.deadline` with the later producer of `WorkflowExecutor.deadline`.
Their query-settlement conclusions therefore do not disprove this narrower
deadline-origin issue.

## Finding disposition

- `DOMAIN-CONCURRENCY-R4-001`: the core proposed finding is supported.
- Causal wording is narrowed: the defect is not that `std::time::Instant` and
  `tokio::time::Instant` necessarily use different production origins; it is
  that the host samples a tool-only absolute deadline before preparation and
  Skald independently samples the run deadline afterward.
- Observable consequence is supported, including the reported possibility of
  `Succeeded`, `Failed`, or `TimedOut` according to what the existing Agent loop
  does after receiving the early tool failure.
- No new proposal was identified. Any remediation should reuse the one existing
  prepared/executor total-deadline owner and existing query path; this review
  does not require a timer service, setting, option, retry, polling loop,
  repository check, or new harness.

## Outcome

**RESOLVED** — the conflict is resolved in favor of retaining the deadline-origin
finding with the causal clarification above.

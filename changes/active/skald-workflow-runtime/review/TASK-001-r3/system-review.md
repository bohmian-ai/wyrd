# System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 11 (`9a621a28a`)
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority: `review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md` plus `TASK-001-R1-addendum-revision-11.md`

The complete cumulative base-to-candidate range was reviewed. The candidate
remained checked out at the stated commit throughout this audit.

## Deployed-path evidence

| Path | Runtime owner and failure boundary | Evidence |
|---|---|---|
| Local Rust Workflow | `Workflow::run_with_options` performs synchronous graph, input, route, and limit preparation before one `WorkflowExecutor` owns the ledger, cancellation token, absolute run deadline, and bounded `JoinSet`. | `crates/skald/skald-workflow/src/workflow_surface.rs:520-559`; `crates/skald/skald-workflow/src/workflow.rs:137-309` |
| Local Python Workflow | The synchronous Python API releases the GIL and enters the same Rust async engine through the repository-owned runtime; it adds no second executor or ad hoc runtime. | `sdks/wyrd-sdk-python/src/workflow.rs:508-533` |
| Agent/provider loop | Each Workflow attempt invokes the existing Agent loop. The Agent bounds its model/tool loop, drops in-flight futures on timeout, journals a terminal outcome, and emits payload-free `invoke_agent`, `chat`, and `execute_tool` spans. | `crates/skald/skald-workflow/src/workflow.rs:493-541`; `crates/skald/skald-agent/src/loop_runtime.rs:33-203,220-529,590-636` |
| Native route | The step uses the shared native registry; aborting or dropping the step future drops the Agent and provider futures. No detached Skald task is created. | `crates/skald/skald-workflow/src/route.rs:381-408`; `crates/skald/skald-workflow/src/workflow.rs:520-534` |
| WyrdGateway route | An attempt-local provider carries fallback, the fixed effective deadline, cancellation, and run/step/attempt correlation into the narrow caller. A passed deadline refuses locally; dropping the caller signals only the public/in-process boundary, leaving later gateway settlement to its separately tracked owner. | `crates/skald/skald-workflow/src/route.rs:381-408,505-567` |
| ExtGateway route | Plan preparation fixes a screened client for the exact binding and origin. The shared provider transport resolves, screens, pins, disables proxies and redirects, and bounds responses; request/retry futures remain inside the abortable step. Refusal bodies are withheld while status/retry metadata survives. | `crates/skald/skald-workflow/src/route.rs:299-343,463-503,569-606`; `crates/skald/skald-providers/src/clients/external.rs:28-173`; `crates/skald/skald-providers/src/endpoint.rs:41-218` |
| Tracing replacement | `workflow.run` owns per-attempt `workflow.step` children; drop records an interrupted attempt as `cancelled`; retry backoff is an event. Agent spans nest below attempts and carry identifiers/status/error codes without request, response, prompt, tool-argument, or credential payloads. No observer callback or observer-owned task remains. | `crates/skald/skald-workflow/src/workflow.rs:197-221,411-490,583-636`; `crates/skald/skald-agent/src/loop_runtime.rs:590-636` |
| Existing consumers | Vala judge/simulator call the surviving three-argument `Agent::run_prompt`; the server and test server consume the relocated `skald_providers::EndpointPolicy`; the Wyrd umbrella removes Observer exports. These are compile-time consumer moves and add no new process or durability owner. | `crates/vala/vala-eval/src/orchestrator/judge.rs:184-194`; `crates/vala/vala-eval/src/orchestrator/simulator.rs:141-172`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1462-1472`; `crates/wyrd/wyrd/src/agent.rs:10-25` |

TASK-001 remains a local, process-owned runtime. A process crash or rolling
replacement loses an in-flight local run and does not replay provider or tool
effects. That is the approved V1 local boundary, not a durability defect. The
server run registry, accepted-job shutdown ownership, replica behavior, and
durable retained state belong to later tasks and are not credited here.

## Failure and recovery paths

| Failure or interruption | Observed behavior | Recovery and proof assessment |
|---|---|---|
| Provider or gateway outage | Typed connection, timeout, decode, 408/429/5xx and the three approved gateway codes retry at the Workflow boundary after route-owned retries finish; auth, permission, binding, route, tool, callback, session, journal, max-iteration, cancellation, and invariant failures terminate. | Classification and exhaustion are covered by `workflow::tests::bounded_attempt_lifecycle`; external reflected-secret and reserved-header paths are covered by `bound_external_gateway_security`. |
| Ordinary step failure | Scheduling stops, already-running peers drain, completed data remains, and all untouched work becomes `unstarted`. No shared server process is crashed by a typed failure or step panic. | `workflow.rs:247-307,319-357`; the focused lifecycle test covers drain and panic containment. Uncapped local execution may wait for a non-cooperative peer, matching the approved ordinary-drain contract. |
| Explicit cancellation | The parent select gives cancellation priority, aborts every owned step, drains joins, preserves settled work, and terminalizes begun attempts as `cancelled` while never-polled work becomes `unstarted`. | `workflow.rs:237-307,319-345`; focused pre-poll and begun-attempt cases are in `bounded_attempt_lifecycle`. |
| Total deadline | The fixed run deadline aborts and drains the same task set and returns one complete `timed_out` snapshot. Backoff and active attempts also race it, so no next attempt begins after expiry. | `workflow.rs:154-165,228-309,421-490`; representability and precedence are covered by `bounded_attempt_lifecycle` and resolved-graph validation tests. |
| Step-attempt timeout | The attempt future is dropped, the timeout is retryable, and exhaustion projects `WYRD_WORKFLOW_504_STEP_TIMEOUT`; provider/tool work does not continue in a detached task. | `workflow.rs:421-474`; `bounded_attempt_lifecycle` asserts two abandoned hanging calls and zero remaining in-flight calls. |
| Agent timeout inside Workflow | The Workflow computes an absolute Agent deadline and passes it to gateway calls, but does not itself race that deadline. Native and ExtGateway execution instead starts the Agent's relative timeout later inside `run_prompt`. | This leaves a reachable wall-clock overrun under preemption or synchronous work between deadline calculation and Agent polling; see `SYS-R3-001`. |
| Parent future drop | Dropping `WorkflowExecutor::execute` drops its `JoinSet`; Tokio aborts the owned steps. There is no Observer `spawn_blocking` work left to outlive it. | `workflow.rs:228-309`; parent-drop coverage is recorded in `bounded_attempt_lifecycle`. |
| External endpoint refusal/outage | DNS/address policy fails closed before secret transmission; redirects are not followed; bounded non-success bodies cannot reflect a credential into public errors; retryable status metadata remains available. | `bound_external_gateway_security` covers refusal, redirect, reserved headers, and credential reflection. Live cloud/network availability is intentionally not required. |
| Process restart | In-flight local state disappears and effects are not replayed. | Approved local non-durability; callers must submit a new local run. |

## Material proposed findings

### SYS-R3-001 — The fixed Agent deadline is not enforced by the Workflow owner

- Classification: `INCORRECT`
- Violated obligation: the approved retry/deadline contract requires each
  attempt to fix the Agent deadline and enforce the earliest of the step,
  Agent, and total-run deadlines across the complete attempt. REQ-016 requires
  that timeout behavior in both local environments, and the task's Scenario 3
  requires exact timeout precedence with no surviving provider/tool operation.
- Exact location: `crates/skald/skald-workflow/src/workflow.rs:421-462,497-540`;
  relative Agent timer at
  `crates/skald/skald-agent/src/loop_runtime.rs:138-203`; ExtGateway adapter at
  `crates/skald/skald-workflow/src/route.rs:569-606`.
- Evidence and reachability: `StepTask::run` computes `agent_deadline` before
  opening/polling the attempt, but its biased `select!` races only explicit
  cancellation, the total deadline, and `attempt_deadline`. The fixed
  `agent_deadline` is merely passed into `attempt`, where it constrains a
  WyrdGateway call. `Agent::run_prompt` separately starts a fresh relative
  `tokio::time::timeout(duration, result)` only when that future is polled.
  Native execution has no other absolute Agent timer, and
  `ExternalGatewayProvider` carries no deadline at all. If the task is
  preempted, or synchronous span/subscriber work delays it, after the Workflow
  fixes `agent_deadline` but before `run_prompt` starts its timer, Native and
  ExtGateway paths receive the full relative duration again. A WyrdGateway
  request correctly refuses at the already-fixed deadline, so the same
  declared Workflow has route-dependent timeout behavior.
- Observable system consequence: under runtime contention or process
  scheduling delay, a Native or ExtGateway Agent attempt may continue beyond
  the approved effective wall-clock deadline, delay retry/terminalization, and
  retain local provider/tool capacity longer than the equivalent WyrdGateway
  attempt. Cancellation and total/step deadlines still contain it when they
  are earlier; the gap is specifically the Agent deadline when it is the
  earliest bound.
- Testable correction: keep the Agent's standalone relative timeout, but have
  the existing `StepTask` owner race the already-computed absolute
  `agent_deadline` in the same biased select after total and step deadlines and
  before the Agent future. Expiry must produce the same projected retryable
  Agent timeout code and drop the attempt future; do not add a timer owner,
  route-specific guard, or second execution path. Add one focused paused-time
  case that delays first Agent polling after the deadline is fixed and proves
  Native and ExtGateway both settle at the fixed deadline, abandon their work,
  retain Agent-timeout classification, and preserve cancellation > total >
  step > Agent precedence.

## Affected capabilities

- The Agent-deadline gap affects local Rust and synchronous Python Workflow
  execution on Native and ExtGateway routes. A later server host would inherit
  it for ExtGateway execution from the same Skald owner.
- WyrdGateway calls already receive the fixed absolute deadline and explicit
  cancellation. Ordinary failures, total/step deadlines, cancellation,
  parent-drop cleanup, result limits, and tracing do not share this defect.
- Vala's direct standalone Agent calls retain their existing caller-owned
  `tokio::time::timeout`; this finding does not require changing that consumer.

## Prior-finding closure and recovery assessment

- Prior system findings corresponding to observer liveness/panic escape,
  unchecked deadline construction, unrepresentable retry count, and pre-poll
  cancellation state are closed. REQ-053 deletes the callback system; planning
  and executor construction use checked deadlines; `u32::MAX` retries are
  rejected; settlement leaves an aborted zero-attempt step for `RunLedger` to
  normalize as `unstarted`.
- The step-result ceiling now runs directly in `AttemptOutcome::from_agent` and
  discards oversize data before it enters the ledger or telemetry. Exact JCS
  growth is charged when the retained payload enters the run snapshot.
- The evidence record reports every focused selector plus `test:skald`, Python
  unit/typecheck, codegen, client/PyO3 boundaries, format/lints, docs, examples,
  Vala and Wyrd consumer tests, and an explicit cumulative `git diff --check` as
  green. This audit independently confirmed the cumulative diff check exits 0.
- The current focused tests prove ordinary Agent timeout and step timeout, but
  none distinguishes the fixed Workflow `agent_deadline` from the later
  relative timer inside `Agent::run_prompt`; therefore they do not close
  `SYS-R3-001`.

## Overall result

**FAIL**

The cumulative candidate closes the prior resilience findings and replaces the
Observer system with non-payload tracing without adding a detached runtime
owner. One bounded deadline-composition defect remains: the Workflow calculates
but does not enforce the fixed Agent deadline for Native and ExtGateway paths.

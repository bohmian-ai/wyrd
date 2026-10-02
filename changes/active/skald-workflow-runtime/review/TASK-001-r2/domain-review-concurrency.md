# Concurrency, Cancellation, and Runtime-Lifecycle Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`

The candidate was at the stated hash before and after this review. Revision 10
changes only the `ExternalGatewayBinding.secret_headers` and
`ProviderError::RemoteProblem` public seams. The reviewed production runtime
files are byte-identical between the r1 production commit
`eb22b03f2bb766886d839bda23aafbd4ba130ab3` and this candidate.

## Reviewed boundary

This pass traced the local Workflow execution lifecycle from
`Workflow::run_with_options` through `ExecutionPlan`, `WorkflowExecutor`,
`StepTask`, `RunLedger`, the Agent loop, route/provider futures, and observer
implementations. It covered ready scheduling, the pre-poll ownership window,
attempt accounting, retry exhaustion, run/step/Agent deadline construction and
precedence, cancellation and parent-drop cleanup, ordinary peer drain, observer
panic/hang behavior, and terminal snapshot projection. Callers and sibling
consumers inspected include the Rust local APIs, scoped/composite observers,
the Python observer bridge, and the focused executor/observer tests.

## Review findings

### Important

- **CONC-001 — Workflow observer callbacks can panic, fail, or indefinitely stall the authoritative run.** The `Observer` contract calls itself best-effort and states that callback panics are caught by the runtime (`crates/skald/skald-observer/src/observer.rs:7-23`), but `WorkflowExecutor` directly awaits start, binding-failure, attempt, result, backoff, and finish callbacks (`crates/skald/skald-workflow/src/workflow.rs:180-188,230-243,300-303,365-425`). A start or finish panic escapes the whole local run; a step callback panic is converted by the `JoinSet` into `WYRD_WORKFLOW_500_INTERNAL`; and a callback that never resolves delays dispatch, cancellation/deadline observation, retry, or return. The Python observer is a concrete reachable blocking implementation because each hook awaits a `spawn_blocking` join (`crates/skald/skald-observer/src/python.rs:248-339`), while `CompositeObserver` sequentially awaits each child without panic isolation (`crates/skald/skald-observer/src/composite.rs:132-173`). **Observable consequence:** adding an observer can change an otherwise successful Workflow into an error/panic or prevent the bounded cancellation/deadline path from producing its terminal snapshot. **Testable correction:** reuse one observer-owned best-effort invocation boundary for every Workflow callback, isolate panics, and bound nonterminal callbacks by the already-owned cancellation/total/attempt deadlines; terminal notification must not delay return. Prove start, attempt, result/backoff, and finish with deterministic panicking and pending observers while preserving the underlying run outcome and preventing later attempts after cancellation/deadline. This independently reconfirms r1 `FIND-TASK-001-6`.

- **CONC-002 — Cancellation can terminalize scheduled-but-never-polled work as active with zero attempts.** The parent marks a step `running` and records its start timestamp before spawning it (`crates/skald/skald-workflow/src/workflow.rs:205-228`; `crates/skald/skald-workflow/src/run.rs:120-125`). The child does not increment the shared attempt counter until its first poll (`crates/skald/skald-workflow/src/workflow.rs:365-381`). Cancellation or total-deadline expiry can win in that interval, after which the aborted join is unconditionally settled through `step_cancelled` with the still-zero counter (`crates/skald/skald-workflow/src/workflow.rs:249-285`; `crates/skald/skald-workflow/src/run.rs:166-172`). That contradicts Revision 10's exact invariant that cancelled active steps have at least one attempt and only pending/unstarted steps have zero. **Observable consequence:** equivalent cancellation can return contract-distinct snapshots solely from Tokio poll order. **Testable correction:** at the executor settlement owner, treat an aborted spawned task whose attempt counter is zero as never begun so `RunLedger::finish` produces `unstarted` with no timestamps; retain `cancelled` only after an attempt began. Add a deterministic spawn-before-first-poll cancellation test and retain the existing in-flight cancellation case. This independently reconfirms r1 `FIND-TASK-001-14`.

- **CONC-003 — Public timeout values can panic during unchecked deadline construction.** `WorkflowStep.timeout_seconds` accepts the full `u64` range, and pure validation imposes no representability constraint (`crates/wyrd-spec/src/card/workflow.rs:69-104,224-261,329-368`). Planning converts it directly to `Duration` (`crates/skald/skald-workflow/src/plan.rs:270-288`). Execution then uses unchecked `Instant + Duration` for the total run deadline, step attempt deadline, and Agent deadline (`crates/skald/skald-workflow/src/workflow.rs:183-188,382,434-444`). `WorkflowExecutionLimits.deadline` exposes the same path directly to Rust callers. **Observable consequence:** validly decoded YAML or a public options value can unwind the caller/runtime instead of returning the promised stable pre-dispatch error or portable terminal result. **Testable correction:** use checked deadline construction at the existing owning boundaries, reject an unrepresentable authored timeout through the existing field-specific Workflow validation path, and reject an unrepresentable local run deadline through the existing pre-dispatch `WorkflowResult`; do not clamp or silently drop the bound. Test `u64::MAX` authored timeout and `Duration::MAX` local deadline without dispatch or panic. This independently reconfirms r1 `FIND-TASK-001-15`.

- **CONC-004 — The accepted maximum retry count cannot fit the public attempt counter.** `WorkflowRetryPolicy.max_retries` accepts all `u32` values without validation (`crates/wyrd-spec/src/card/workflow.rs:360-368`). `StepTask::run` increments a `u32` attempt before checking `attempt > max_retries` (`crates/skald/skald-workflow/src/workflow.rs:365-378,409-427`). With `max_retries == u32::MAX`, the required `max_retries + 1` attempt panics in debug or wraps in release; the terminal reserve's saturation (`crates/skald/skald-workflow/src/run.rs:318-340`) cannot repair execution semantics. **Observable consequence:** an accepted Workflow can panic or fail to terminate and cannot report the Revision 10 exact attempt count. **Testable correction:** reject only `u32::MAX` at pure `WorkflowSpec` validation using the existing field-specific error, because the first attempt beyond those retries is unrepresentable; do not saturate execution or widen an internal counter that still cannot project to the public `u32`. Prove acceptance of `u32::MAX - 1`, rejection of `u32::MAX` before dispatch, and unchanged ordinary retry accounting. This independently reconfirms r1 `FIND-TASK-001-16`.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Execution ownership, bounded scheduling, ordinary drain, explicit abort-and-drain, parent drop | `AGENTS.md` §§5-6, 11-12; `architecture/agent-rules.md`; Revision 10 REQ-017-019/047-048 and AC-020; `workflow.rs:140-305`; `run.rs:114-238`; `workflow.rs:1082-1197` | **FAIL** — CONC-001, CONC-002 |
| Attempt accounting, retry eligibility, backoff, and overflow | Revision 10 snapshot invariants, retry/deadline contract, REQ-016/021/045 and AC-019/020; `workflow.rs:357-489`; `attempt.rs`; `run.rs:318-340`; `workflow.rs:980-1080` | **FAIL** — CONC-004; ordinary typed classification and saturating backoff otherwise match the authority |
| Deadline and cancellation precedence | Revision 10 REQ-016/019/020/043/048; `workflow.rs:183-304,357-481`; Agent `run_prompt` at `skald-agent/src/loop_runtime.rs:161-241`; route attempt context | **FAIL** — CONC-001, CONC-003; once representable deadlines and callback completion are assumed, the biased cancellation/run/step ordering is implemented |
| Observer lifecycle and runtime liveness | Revision 10 retry/deadline and observation contracts, REQ-016/019/022/043/048, AC-020; `skald-observer/src/{observer,composite,python}.rs`; every Workflow callback caller in `workflow.rs` | **FAIL** — CONC-001 |
| Route/provider future lifetime and parent-drop cleanup | Revision 10 REQ-043/048 and INV-020/021; `workflow.rs:430-481`; route adapters; `bounded_attempt_lifecycle` parent-drop case | **PASS** for the scoped local owner when callbacks complete: the `JoinSet` owns Native/ExtGateway step futures and is aborted on executor drop; WyrdGateway receives cancellation without transferring settlement ownership |
| Repository async, error, and proof rules | `architecture/references/{doctrine/architecture-constraints,architecture/patterns}.md`; `architecture/references/languages/{rust-core,testing-workflows,spec-driven-development,maintainer-style}.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx` | **FAIL** — public input can panic, best-effort callbacks govern correctness/liveness, and the focused proof omits these boundary cases |

Revision 10's `HashMap<HeaderName, SecretString>` and
`RemoteProblem(Box<RemoteProblem>)` changes do not alter this domain. They
resolve the authority conflicts behind r1 `FIND-TASK-001-3/4`, but they do not
close any concurrency finding.

## Verification limits

- The task records `workflow::tests::bounded_attempt_lifecycle` plus green
  Skald/shared/Wyrd/Python/codegen/boundary/format/lint lanes. The focused test
  covers the configured concurrency ceiling, ordinary failure drain,
  cancellation/deadline after an attempt is in flight, retry classification,
  step timeout, and parent-drop provider cleanup.
- Existing observer tests cover happy-path delivery, ordering, scoping, and
  serialization exclusion only (`crates/skald/skald-workflow/tests/workflow_observers.rs:224-329`).
- No existing proof deterministically covers a pending or panicking Workflow
  callback, cancellation between spawn and first poll, unrepresentable
  run/step/Agent deadlines, or `max_retries == u32::MAX`.
- This review did not run Cargo-backed commands because review agents share one
  checkout/target and repository rules require those commands to run
  sequentially. Source inspection plus the recorded candidate evidence is
  sufficient to establish the reachable gaps above; broad green lanes do not
  exercise them.

## Overall result

**FAIL** — CONC-001 through CONC-004 are bounded implementation defects under
approved Revision 10. They require no new product, architecture, concurrency,
or persistent-data decision.

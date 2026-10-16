---
id: TASK-002
kind: implementation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-014, REQ-015, INV-001, INV-005, INV-006, INV-007, INV-008, AC-002, AC-003, AC-007, AC-010]
depends_on: [TASK-001]
---

# An independently testable verification engine

## Outcome and Value

Given resolved evidence and injected action/check implementations, the Vala
engine evaluates the complete approved Verifier graph and returns an honest,
attributable breakdown. It is usable without a server, registry, database,
object store, real provider, or language interpreter lifetime.

## Owners, Scope, Consumers, and Prohibited Changes

Own the Vala graph planner/driver, scope, requirement execution, aggregation,
assertion/trace checks, and drift-check composition. Keep statistical math and
fitted baselines owned by `vala-drift`. Task 3 owns Agent/code/extraction
implementations; task 5 owns authorization, evidence reads, persistence,
scheduling, and composing those implementations into hosted runs.

Reuse `TaskExecutor`/`Executors`, the stage driver, scope/context, and registry
rather than adding a second graph or scheduler. Consume task 1's shared
boundary; do not edit that boundary privately during the parallel wave.
Adapt existing scenario consumers necessary to keep this crate buildable;
do not remove scenario evaluation or add a new scenario action. Do not touch
server routes, SQL, SDKs, or task 3's hosted runtime.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Graph execution | `vala-eval/src/executor.rs::execute_plan`, `execute_stage`, `Executors`, `RunLedger`; `store.rs::TaskRegistry` | `executor.rs` unit modules, assertion/judge/trace executors, scenario consumers | Driver assumes every task is a check and aborts on errors | Generalize existing stages to actions plus step/final requirements and cause-preserving skips | No new graph store or orchestration owner |
| Scoped values | `context.rs::ContextSnapshot`, `ExecutionContext`, `TaskOutput`; existing required-selector handling | Assertion/trace task executors and context tests | Context carries kind-specific task outputs, not the approved scope | Extend the existing scope into task 1's reference model | Replace the internal representation in its owner; no second selector language |
| Deterministic checks | `tasks/assertion.rs::AssertionTaskExecutor`; `tasks/trace.rs::TraceTaskExecutor`; `TraceSource` | Their in-module tests and executor composition | Check input/outputs are EvalTask/AssertionResult-specific | Reuse comparators and injected trace access under generic requirements | New check adapters only where old task contracts cannot express the new requirement |
| Drift composition | `vala-drift::score_drift`, `fit_baseline`, report types; server `DriftEngine::verify` is the current IO host | Drift public-surface/scoring tests; server drift completeness tests | Multiple drift requirements need distinct state and generic detail | Add the requirement-level adapter over resolved samples/fitted state; keep IO outside | Adapter earns its place; statistical engines remain unchanged |

## Approach

1. Consume the contract handoff and revalidate driver/context/scenario callers.
2. Evolve the existing DAG execution into data preparation and two requirement
   positions using injected executors.
3. Preserve dependency cause, retry/deadline/cancellation bounds, and visible
   execution attempts without turning engine failures into subject failures.
4. Reuse deterministic and statistical executors and aggregate exactly once.
5. Prove the module with in-memory inputs, scripted executors, and deterministic
   synchronization; hand task 5 one cohesive callable engine.

## Ordered Implementation Scenarios

### Scenario 1 — A Verifier with no preparation can judge inputs

**Behavior.** Assertions and trace checks use declared evidence, return one
result per requirement, distinguish a failed assertion from an execution
failure, and retain permitted actual/provenance values (REQ-002/005/015).

**RED.** Add focused IO-free engine tests in the existing Vala library target.
Use in-memory trace data and explicit inputs. Observe the missing generic
check/result behavior before implementation; collect and record exact names.

**GREEN.** Adapt existing comparison and trace execution to the generic
contracts; return complete results without requiring a hosted service.

**REFACTOR.** Keep pure selection/comparison synchronous and consolidate
shared behavior in its existing cohesive owner.

### Scenario 2 — Data and judgments run in the right order

**Behavior.** References establish dependencies; independent actions overlap
within budgets; a dependent waits for the upstream action and its step
requirements; final requirements run once after every stage (REQ-003/004).

**RED.** Add focused driver tests using scripted executors and explicit
barriers/notifications. Prove overlap and ordering without sleeps, artificial
host load, real providers, or test-thread inflation.

**GREEN.** Extend the existing driver and scope to execute the two positions.
Rerun scenarios 1 and 2; preserve deterministic output/declaration ordering
where callers rely on it.

**REFACTOR.** Remove obsolete task-kind dispatch duplication. Keep injected
capabilities narrow and state on meaningful owning types.

### Scenario 3 — Failure cannot disappear through a skipped dependency

**Behavior.** Intentional skips are not applicable; failure/timeouts and their
blocked descendants are inconclusive; failure wins when causes mix. Missing
values do not call checks. All-skipped/empty is inconclusive; pass rate counts
inconclusive requirements and each verdict only once (REQ-006).

**RED.** Add focused table-driven engine tests for direct/transitive and
mixed-cause dependencies, optional/null and missing required evidence, both
decision rules, and threshold boundaries. Fail for the wrong observable
outcome, not an absent provider.

**GREEN.** Preserve causes through the existing ledger/scope and implement
the approved rollup. Rerun all earlier scenarios after each change.

**REFACTOR.** Eliminate duplicate skip/aggregation decisions rather than
introducing a fallback that silently passes missing evidence.

### Scenario 4 — Drift and runtime budgets remain honest

**Behavior.** PSI/SPC/Custom adapters retain statistical detail, requirement-
specific fitted state, minimum-sample/readiness behavior, and inconclusive
insufficient data. Bounded retries/timeouts stop work and retain attempts;
unrelated requirements may finish (REQ-004/005/014/015).

**RED.** Add focused tests using existing drift fixtures, fitted in-memory
state, and scripted retry/cancellation behavior. Tests require no SQL,
external file reads, model calls, or Python/Node lifetime.

**GREEN.** Compose existing scorers through the generic engine seam and
apply execution budgets. Rerun all earlier scenarios and affected scenario
simulator regressions.

**REFACTOR.** Remove superseded executor logic once its local consumers move;
hand any remaining old live-server consumer removal to task 5 explicitly.

## Contract Handoff From TASK-001

- Drive steps and Agent/code checks through `vala_eval::blocks::ActionExecutor`
  and `CheckExecutor`. Both return `vala_eval::blocks::BlockError`.
- Retry rule: `BlockError::is_retryable()` (only `Retryable`) spends the step's
  `retries` budget; `Terminal`, or `Retryable` after the budget is spent, makes
  the step `failed` and its dependents follow the REQ-006 skip table. A check
  error makes that requirement `inconclusive` with the error text as `reason`.
- The engine owns the deadline: race each attempt against `BlockCall::deadline`
  and drop the future on expiry; executors never report timeouts. Record an
  expired step as `failed` with reason `timed out`.
- The LLM judge is not kept (owner decision). Do not call, extend, or depend on
  `JudgeInvoker`, `JudgeError`, `JudgeTaskExecutor`, `Executors::judge`, or
  `EvalTask::LlmJudge` from new engine code. Leave them compiling until
  TASK-005 deletes them with their last consumers (removal map in TASK-005).

## Acceptance Criteria

- The same engine handles no-step and multi-step Verifiers without server IO.
- Scripted capabilities prove isolation, overlap, ordering, cancellation,
  retries, failure causes, and complete result provenance.
- Existing drift math is reused unchanged; assertions remain deterministic.
- No SDK, persistence, policy, or reaction logic moves into the engine.
- Task 5 can compose task 3's implementations through the shared boundary;
  no special production engine is created just for tests.

## Expected Write Set and Consumer Closure

`crates/vala/vala-eval` executor, scope/context, registry/planning integration,
existing deterministic tasks, result/capture adapters, and their local tests;
`vala-drift` only for a necessary existing-owner adapter or regression.
Preserve unrelated offline/scenario entry points. Own the Vala root re-exports
and its crate manifest; task 3 owns orchestrator invoker internals and server
runtime dependencies. Coordinate lockfile effects through README ownership.

## Verification and Evidence

All new runtime logic uses ordered TDD. Record each actual focused test selector
before GREEN; this packet intentionally does not invent future function names.
The following are isolated owner-level checks, not the final integrated gate:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib
mise exec -- cargo nextest run --locked -p vala-drift --lib
mise exec -- cargo clippy --locked -p vala-eval -p vala-drift --all-features --lib -- -D warnings
mise run fmt
mise run check:deps
```

Do not run the Vala integration target or full journey suites here; keep their
existing consumers compiling and supply any required regression changes for
the once-integrated run. No network credentials are required.

## Material Stop Conditions

A new graph runtime/store, altered statistical algorithm or baseline policy,
new source selection, changes to verdict semantics, unbounded concurrency,
external writes, or a new public format requires spec authority. Routine
private context/driver organization stays implementation-owned.

## Authority Links

- [Approved specification r6](../spec.md), [supporting proposal](../proposal.md),
  [worked examples](../example.md), and [flow diagram](../architecture/verification-flow.html).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [Wyrd design](../../../../architecture/wyrd-design.md), and
  [Bifrost design](../../../../architecture/bifrost-design.md).
- [Execution and final proof coordination](README.md). Ready after owner-approved
  completion of TASK-001; execute independently alongside the other parallel tasks.

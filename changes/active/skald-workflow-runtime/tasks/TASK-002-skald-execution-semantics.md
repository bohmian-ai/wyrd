---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-skald-workflow-runtime
spec_revision: 5
requirements: [REQ-003, REQ-005, REQ-006, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013A, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-035, REQ-040, REQ-043, INV-001, INV-002, INV-008, INV-014, AC-005, AC-006, AC-007, AC-008, AC-013]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Outcome and Value

The existing Skald DAG executor runs the approved Workflow contract with
explicit Prompt-variable bindings, deterministic namespaced results, bounded
parallelism, retries, timeouts, cancellation, observations, and partial terminal
results. There is no second renderer or workflow engine.

## Owners, Scope, Consumers, and Prohibited Changes

`skald-workflow` owns reusable execution. It consumes pure `wyrd-spec`
contracts and existing `skald-spec::Prompt` binding plus the existing Agent,
provider registry, tool registry, and observer boundaries. Local callers and
the future server host consume this behavior.

Do not add server tenancy, HTTP, registry IO, provider-specific route branches,
durable state, a template engine, implicit message handoff, flat shared output
mutation, shell execution, or new dependencies/features. Do not move Prompt
rendering out of `skald-spec`.

## Approach

1. Lower the complete validated Workflow graph without discarding bindings,
   timeouts, retry policy, or outputs.
2. Resolve each binding to a stable context snapshot and pass only declared
   values through the existing Prompt binder.
3. Preserve bounded dependency-ready concurrency while namespacing all step
   results and explicitly projecting workflow outputs.
4. Extend execution outcomes for retries, timing, errors, failure, timeout,
   cancellation, and unstarted downstream steps.
5. Carry cancellation and observation through existing provider/Agent seams and
   keep pure validation synchronous.

## Ordered Implementation Scenarios

### Scenario 1 — Explicit bindings reuse the Prompt binder

**Behavior.** Workflow inputs and dependency outputs bind only to the Prompt
variables named in `WorkflowStep.inputs`; strings remain strings, null becomes
empty text, other JSON becomes compact JSON, missing nested values fail before
the provider call, and dependency completion adds no implicit messages or flat
parameters. This proves REQ-003, REQ-008 through REQ-013A, INV-014, AC-006, and
AC-007.

**RED.** Add
`explicit_step_bindings_use_prompt_binder_without_implicit_handoff` to the
existing `parameter_injection` target and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow \
  --test parameter_injection \
  -E 'test(=explicit_step_bindings_use_prompt_binder_without_implicit_handoff)'
```

It must fail because current execution looks up a flat context by Prompt
variable name and implicitly carries dependency messages.

**GREEN.** Resolve approved bindings and call the existing Prompt binder without
changing provider-native request ownership.

**REFACTOR.** Remove superseded flat-context/handoff paths once all existing and
new binding tests remain green.

### Scenario 2 — Parallel DAG execution is deterministic and namespaced

**Behavior.** Independent reviewers overlap, a dependent starts only after its
declared dependencies finish, same-named structured fields never collide, and
declared outputs are stable regardless of completion order. This proves
REQ-005, REQ-006, REQ-009, REQ-010, REQ-012, REQ-017, REQ-023, AC-005, and
AC-007.

**RED.** Add `parallel_steps_keep_namespaced_outputs_and_gate_dependents` to the
existing `dag` target and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow --test dag \
  -E 'test(=parallel_steps_keep_namespaced_outputs_and_gate_dependents)'
```

It must fail on the current collision-prone parameter map or implicit final
step behavior.

**GREEN.** Keep the existing staged DAG scheduler and change only the context
and result semantics needed by the scenario.

**REFACTOR.** Preserve one executor and one graph validation path; simplify
obsolete last-step and shared-parameter machinery.

### Scenario 3 — Retry, timeout, and failure return complete terminal evidence

**Behavior.** Attempts and timing are observable; a timed-out attempt stops;
exhausted step timeout becomes failed with a stable timeout error; terminal
failure prevents new work but retains completed siblings and marks downstream
steps unstarted; non-success workflow outputs are empty. This proves REQ-016,
REQ-018, REQ-020 through REQ-022, AC-008, and AC-013.

**RED.** Add `terminal_failure_retains_partial_results_and_unstarted_steps` to
the existing `retries` target and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow --test retries \
  -E 'test(=terminal_failure_retains_partial_results_and_unstarted_steps)'
```

It must fail because current terminal errors discard the partial WorkflowRun.

**GREEN.** Return the approved terminal envelope and stop scheduling after the
first terminal step outcome while respecting already-running peers.

**REFACTOR.** Centralize terminal-state assembly on the executor owner without
introducing a parallel result type.

### Scenario 4 — Cancellation and total deadline terminate the graph

**Behavior.** Caller cancellation and total deadline stop new scheduling,
propagate to active operations where supported, terminalize every step, and
distinguish cancelled from timed-out runs without reporting provider failure.
This proves REQ-017, REQ-019 through REQ-022, and REQ-043.

**RED.** Add `cancellation_and_deadline_terminalize_every_step` to the existing
`workflow_run` target and run:

```bash
mise exec -- cargo nextest run --locked -p skald-workflow --test workflow_run \
  -E 'test(=cancellation_and_deadline_terminalize_every_step)'
```

It must fail because the current executor has no complete cancellation/deadline
result semantics.

**GREEN.** Thread the existing cancellation capability through the executor and
Agent/provider operations and emit the approved final statuses.

**REFACTOR.** Keep cancellation ownership cohesive and remove duplicated
terminal-state branches while all four scenarios stay green.

## Acceptance Criteria

- The existing Prompt binder is the only template renderer.
- Explicit dependency bindings are the only inter-step data flow.
- Concurrency is bounded and deterministically observable without serializing
  independent steps.
- Success, failure, timeout, and cancellation all return one normalized
  WorkflowRun with complete terminal step states.
- Existing observers receive attempts and transitions without provider-specific
  payloads entering the portable result.
- Existing Python-enabled compilation and retained local Workflow behavior are
  not broken; no new Python API is added.

## Expected Write Set and Consumer Closure

Likely surfaces include `crates/skald/skald-workflow/src`, its existing focused
integration targets, and narrowly required Agent/provider cancellation seams.
Existing Python wrappers may receive only compilation-preserving adjustments
forced by the Rust owner contract. Paths are guidance, not an implementation
allowlist.

## Verification and Evidence

Run each focused scenario sequentially, then:

```bash
mise run test:skald
mise run check:pyo3-scope
mise run check:error-coverage
mise run fmt
mise run lints
git diff --check
```

All executable behavior follows RED/GREEN/REFACTOR. Existing Python behavior is
a regression obligation, not authorization to add a Python Workflow surface.

## Material Stop Conditions

- Correct execution would require a second Prompt renderer or provider-specific
  branches in the DAG executor.
- Cancellation cannot be expressed through existing Agent/provider ownership
  without changing its approved public semantics.
- A durable queue, database, server dependency, new Cargo feature, or new
  dependency appears necessary.
- The approved result statuses, failure semantics, or dependency visibility
  must change.

## Authority Links

- `changes/active/skald-workflow-runtime/spec.md` Revision 5
- `AGENTS.md` §§3–6, 10–12
- `architecture/agent-rules.md`
- `architecture/references/architecture/patterns.md` §Provider Runtime Pattern
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`

# Eval Verifier

**Status:** agreed on 2026-09-14

Eval becomes one implementation of a `Verifier` Card. The current `EvalSpec`,
scenario, task, workflow, sampling, gate, context-capture, and result types
remain the foundation.

## Verifier contract

```rust
pub enum VerifierImplementation {
    Eval(EvalSpec),
    // Other Verifier implementations.
}

pub struct EvalSpec {
    pub dataset: Option<DatasetRef>,
    pub tasks: BTreeMap<TaskId, EvalTask>,
    pub workflow: Option<Workflow>,
    pub sampling: Option<EvalSampling>,
    pub pass_gate: Option<EvalPassGate>,
    pub context_capture: Option<EvalContextCapture>,
}
```

The current `EvalSpec` and its nested types remain unchanged. Runtime types
that currently carry an Eval Card reference will instead carry the enclosing
Verifier Card reference.

## Offline evaluation flow

1. The user defines an `EvalScenarioCollection` containing one or more
   `EvalScenario` values.
2. The user registers those scenarios as a `Data` Card. Registration stores
   the scenario rows in a Bifrost table backed by canonical Parquet storage.
3. The user registers an Eval-backed `Verifier` whose `EvalSpec.dataset`
   references that Data Card through the existing `DatasetRef` type.
4. The server resolves and UID-pins the Data Card reference through the normal
   loader and registry flow.
5. An offline Eval run reads the scenarios from the registered Bifrost table,
   runs the Eval scenario workflow against the selected subject, evaluates the
   existing mechanic and passenger tasks, and aggregates `EvalResults`.
6. The server persists the common Verification Result together with the typed
   Eval results and exact Verifier, subject, dataset, and run identities.

`dataset` remains optional because continuous record-based evaluation does not
require an offline scenario dataset. Starting an offline scenario run requires
it.

## Scenario contract

The existing scenario types remain unchanged:

```rust
pub struct EvalScenarioCollection {
    pub collection_id: String,
    pub scenarios: Vec<EvalScenario>,
}

pub struct EvalScenario {
    pub id: ScenarioId,
    pub initial_query: String,
    pub expected_outcome: Option<String>,
    pub predefined_turns: Vec<String>,
    pub simulated_user_persona: Option<String>,
    pub termination_signal: Option<String>,
    pub max_turns: u32,
    pub tasks: Vec<ScenarioTask>,
}
```

The registered Data Card is the only durable scenario source. The existing
provisional `eval/scenario_collection.json` object path is removed rather than
maintained as a second storage path.

## Continuous evaluation

A Service component, Service, or standalone Agent declares a `verified_by`
binding containing the Eval-backed Verifier, an `observations_ready` `runs_on`,
and optional `on_failure`. The Eval engine applies `sampling`, executes `tasks`,
applies `pass_gate`, and records results without requiring `dataset`.

The typed observation uses the existing `EvalRecordObservation` after removing
its obsolete `eval_ref` and duplicate `run_id`; the observed Card and run ID
are supplied as Bifrost correlation by the scoped run.
`observe.eval(...)` converts caller context into that canonical record and
projects the fixed `vala.eval.observations` rows **before** inserting them through
the state-owned Bifrost facade and its existing `WriterPool`. Shared
`wyrd-client` and `wyrd-queue`
do not dispatch on Verifier kind, deserialize Eval context, or project Eval
rows. The generic queue publishes Arrow IPC through Gate and Scribe, which
commits the rows to the Bifrost table
`vala.eval.observations`. After Scribe acknowledges the observation, the server
attempts a best-effort asynchronous, idempotent `verifier_runs` insert per
matching active binding. That insert cannot block or roll back ingest. If it
fails or the process stops first, the observation remains committed but an Eval
run is not guaranteed; the failure is traced. There is no Eval outbox in this
initial change.

The generic Verifier runner claims that run, applies sampling, waits for trace data
through `EvalStatus::AwaitingTrace` when required, and invokes the existing
Eval planner and executors. It does not introduce a second Eval engine.

Postgres stores work status, fenced claims, attempts, retries, and result
pointers. Bifrost stores the input observation in `vala.eval.observations`, the
canonical verdict in `vala.verification.results`, and task/workflow details in
`vala.eval.result_items`. The result batches are separately acknowledged;
partial rows after a failed write or crash are accepted. Only an unacknowledged
sealed batch retried with the same payload and batch ID is deduplicated by
Scribe; a fresh write call is not.

Eval runs from `observations_ready` and has no schedule. A failed authored
`pass_gate` dispatches the binding's effective `on_failure` when present. An
absent `pass_gate` persists results but produces no dispatch or Alert; Wyrd does
not invent a default gate.

## Context capture

The existing `EvalContextCapture` contract remains:

- `full` retains `AssertionResult.actual`.
- `hash` replaces it with its digest.
- `redact` removes it.
- An absent value means `full`.

The server implementation must apply this policy before any result crosses a
public or persistence boundary. Intermediate public results must not expose
uncaptured values, and messages must not interpolate raw observed values when
the policy is `hash` or `redact`. Capture changes stored evidence only; it does
not change task execution or verdicts.

## Required verification work

- Register an `EvalScenarioCollection` as a Data Card and store its rows in
  Bifrost.
- Resolve `EvalSpec.dataset` and read those scenario rows for an offline run.
- Execute the existing Eval workflow and collect `EvalResults`.
- Persist the common Verification Result and typed Eval results.
- Apply `EvalContextCapture` consistently to intermediate, returned, and
  persisted results.
- Replace top-level Eval Card references in runtime contracts with the
  enclosing Verifier Card during resolution without renaming the existing
  observation field.
- Add a real SDK-to-server journey covering one deterministic assertion and
  one LLM judge, binding-driven durable execution, Bifrost results, and one
  Notify-created Alert. Cover inline and referenced Trigger and Operator
  definitions.

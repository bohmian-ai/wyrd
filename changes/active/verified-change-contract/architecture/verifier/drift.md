# Drift Verifier

**Status:** agreed on 2026-09-14

Drift becomes one implementation of a `Verifier` Card. The existing
`DriftSpec` contract and `vala-drift` fitting and scoring types remain the
foundation.

## Card shape

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: model-drift
  version: "1.0.0"
spec:
  implementation:
    kind: drift
    description: PSI population stability against registered training data
    method: Psi
    signal:
      kind: Distribution
      baseline_ref:
        kind: Data
        name: training
        version: "1.0.0"
      features: [age, income]
    condition:
      kind: Statistical
    profile:
      kind: Psi
      binning_strategy:
        kind: Quantile
        n_bins: 10
      threshold:
        kind: Fixed
        value: 0.2
```

## Contract

```rust
pub enum VerifierImplementation {
    Drift(DriftSpec),
    // Other Verifier implementations.
}

pub struct DriftSpec {
    pub description: Option<String>,
    pub method: DriftMethod,
    pub signal: DriftSignal,
    pub condition: DriftCondition,
    pub profile: Option<DriftProfile>,
}

pub enum DriftMethod {
    Psi,
    Spc,
    Custom,
}

pub enum DriftSignal {
    Distribution {
        baseline_ref: Ref,
        features: Vec<FeatureName>,
    },
    Metric {
        name: String,
    },
}
```

`DriftCondition`, `DriftProfile`, `PsiProfile`, `PsiBinningStrategy`,
`PsiThreshold`, and `CustomProfile` remain as currently defined.
`SpcProfile` carries only `sample_size`, an authored subgroup size of at
least two; `SpcWecoRule` and `SpcAlertThreshold` are removed.

The only removals from the current contract are:

- `DriftMethod::External`
- `DriftSignal::External`
- `DriftSignal::EvalScore`
- `DriftSpec.details`

## Registration and resolution

1. The user registers a `Data` Card through any supported authoring interface,
   including Pandas, Polars, or Arrow, with its registered data artifact in
   Parquet format.
2. The user registers a Drift-backed `Verifier` whose
   `signal.Distribution.baseline_ref` points to that Data Card and whose
   `profile` contains the fitting and scoring strategy.
3. The loader validates the reference shape. An external `ref` remains a
   reference; a loader-local `path` is resolved through the normal composite
   registration flow.
4. The server resolves the Data Card from the tenant registry, UID-pins the
   reference, derives the Verifier-to-Data relationship, and persists the
   resolved Verifier Card.
5. Registration commits the Verifier and enqueues baseline fitting without
   waiting. The server loads the Data Card's Parquet data as an Arrow
   `RecordBatch` and calls `fit_baseline(batch, drift_spec)`.
6. The server persists the returned `FittedBaseline` against the exact
   Verifier Card version and moves status through `pending`, `building`,
   `ready`, or `failed`.
7. During PSI or SPC evaluation, the server loads that fitted baseline and
   calls `score_drift(fitted_baseline, observations, drift_spec)`.

`DriftProfile` is the user-authored fitting and scoring configuration.
`FittedBaseline` is the server-produced PSI/SPC baseline state.

## Runtime publication

Drift remains a continuous monitor. A Service component, Service, or standalone
Agent declares a `verified_by` binding containing the Verifier, its `runs_on`
Trigger, and optional `on_failure` Operator. The binding supplies subject
identity and routing; the registered Verifier supplies the baseline Data
reference and Drift configuration.

The typed observation uses the existing `DriftRecordObservation` after removing
its obsolete `drift_ref` and duplicate `run_id`. `run.observe.drift(...)` constructs that record and
projects fixed-schema feature rows before inserting them through the
state-owned Bifrost facade and its existing `WriterPool`. Shared `wyrd-client`
and `wyrd-queue` handle only
generic buffering and publication through Gate and Scribe; neither performs
Drift/Verifier-specific conversion. The server uses the authorized observed
subject Card to select active `verified_by` bindings. Raw observations carry
no Verifier reference and are not duplicated per binding.

## Execution and alert flow

1. Each occurrence of a binding's effective scheduled Trigger enqueues one
   durable subject-scoped run with a bounded observation window. A direct
   `POST /v1/verification/runs` with a `verifier` target remains analysis-only;
   the same operation with a `binding` target follows its `on_failure`
   Operators. Both accept a bounded `drift_window` and return a run ID before
   scoring. The current baseline state is available on the Verifier Card's
   server-derived status through the existing Card read.
2. The worker loads the exact `FittedBaseline`, queries the window through
   Oracle, reconstructs the scorer input, and calls `score_drift`.
3. The Drift report and common Verification Result are written to Bifrost.
4. `passed` means no drift, `failed` means drift detected, and `inconclusive`
   means no valid decision was possible.
5. A failed binding-created run dispatches its effective `on_failure` Operator
   exactly once when present. A Notify Operator creates the Alert. Schedule and
   reaction configuration remain on the binding, not `DriftSpec`.

Postgres stores the baseline job, fitted-baseline status, scheduled/manual run
claims, retries, and schedule cursor. Bifrost stores observations and reports.

## Validation retained

Existing `DriftSpec` validation remains, adjusted only for the removed enum
variants and field:

- PSI requires a Distribution signal and matching PSI profile.
- Distribution `baseline_ref` must resolve to a Data Card.
- Distribution features must be non-empty and unique.
- SPC and Custom require their matching profiles.
- Conditions and method-specific numeric configuration retain their current
  validation.
- Unknown fields and unknown enum variants are rejected.

## Required user journey

Run the complete Data Card -> Service `verified_by` binding -> Verifier
registration -> baseline status -> WyrdState observations -> manual and cron
execution -> optional `on_failure` flow for both PSI and SPC. Cover inline and
referenced Trigger and Operator definitions.

# Drift and monitoring

Load for Drift Cards, versioned baselines, windows, statistical methods,
thresholds, alert quality, or reactions to observed change.

## Declare the subject and signal first

A `Drift` Card names one versioned `subject_ref`, one typed signal, one method,
one resolved condition, and method-specific profile. Select statistics only
after defining the signal's unit, population, sampling grain, completeness,
delay, and failure semantics.

- **Data drift** compares a versioned baseline distribution with a bounded
  comparison window. Preserve feature set, bin edges or sketch configuration,
  missingness, sample count, and cohort identity.
- **Concept or performance drift** compares delayed labels, outcomes, or Eval
  scores against the same subject, cohort, and time basis.
- **Operational drift** measures latency, errors, token use, cost, tool calls,
  or workflow behavior from authorized telemetry with stable deployment and
  resource dimensions.

External measurements enter through a read-only `Source`; Bifrost observations
use the native archive. Never let a vendor-specific push create a parallel
drift protocol.

## Baselines and windows are durable inputs

The baseline is a versioned Data Card, immutable observation selection, or
typed source query whose identity and selection parameters are persisted with
the Drift result. It is never "the rows in a mutable table at evaluation
time." Record baseline and comparison-window boundaries, watermarks, late-data policy, cohort,
schema fingerprint, method/profile version, threshold, and source snapshot or
query identity.

Choose windows from process cadence and decision latency. Event-time windows
use explicit lateness and completeness rules. Do not evaluate until minimum
count, required dimensions, and source-completeness conditions hold. A partial
window, schema mismatch, source outage, or insufficient sample returns a typed
`indeterminate`/`insufficient_data` result—not "no drift."

## Statistical hygiene

- Report sample count, missingness, effect size, uncertainty/control limits,
  and practical threshold beside the drift statistic.
- PSI is bin-sensitive and unstable at small counts. Persist binning, apply
  minimum expected/observed floors, and pair it with a practical effect rule.
- SPC requires a stable baseline, correct subgrouping, and explicit handling
  of autocorrelation, seasonality, and recalibration. Do not silently update
  limits after an alarm.
- Distribution tests can detect a difference without proving business impact,
  causality, or model degradation. Keep those decisions separate.
- Segment by a bounded, declared cohort set. Do not discover and alert over
  unlimited label combinations.

Thresholds and profiles are versioned with the Card. A threshold update starts
a new comparable regime and does not rewrite prior observations.

## Scheduling and reaction

Drift computes and emits an Observation. Trigger owns scheduling and condition
evaluation. Operator owns one reaction. Keep notification, rollback,
retraining, and workflow dispatch out of the Drift evaluator.

Reaction policy uses hysteresis, cooldown, minimum consecutive breaches,
deduplication key, and recovery conditions appropriate to the signal. It
records the exact Drift observation and rule that caused the action. A drift
finding is evidence for investigation or policy action; it is not an automatic
diagnosis.

## Monitoring the monitor

Measure source freshness, selected rows, late/drop counts, evaluation latency,
indeterminate rate, threshold breaches, notification suppression, reaction
success/failure, and baseline age. Alert separately on monitor failure versus
subject drift so a broken pipeline cannot look healthy.

## Rejected shapes

Reject global thresholds across heterogeneous tenants, mutable implicit
baselines, mixed deployment cohorts, windows evaluated before completeness,
silent baseline rebasing, unbounded segmentation, schedules or notification
code inside Drift, an unversioned subject, and statistical significance treated
as causality or business impact.

## Stable Wyrd anchors

- Drift contract: `architecture/wyrd-design.md` §Drift.
- Drift types: `crates/wyrd-spec/src/vala/drift/`.
- Trigger and Operator: `architecture/wyrd-design.md` §§Trigger and Operator.
- Observation store: `crates/vala/`.

## Primary grounding

- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- [Gama et al., A survey on concept drift adaptation](https://dl.acm.org/doi/10.1145/2523813)
- [NIST/SEMATECH control charts](https://www.itl.nist.gov/div898/handbook/pmc/section3/pmc3.htm)
- [OpenTelemetry metrics](https://opentelemetry.io/docs/concepts/signals/metrics/)

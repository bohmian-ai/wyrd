# Drift and monitoring

Load for Drift Cards, baselines, thresholds, data or concept drift, alert
quality, and monitoring strategy.

## Choose the signal before the statistic

A `Drift` Card names one `subject_ref`, a signal, a method, a condition, and a
profile. The signal is the measured fact: feature distribution, a runtime
metric, an Eval score, or a read-only external source. The method (for example
PSI, SPC, or a bounded custom method) is selected only after the signal's
sampling grain and failure semantics are clear.

Distinguish:

- **Data drift:** the input distribution changes; compare a versioned baseline
  with a current window and disclose binning, missingness, and sample size.
- **Concept or performance drift:** the relationship or outcome changes; join
  delayed labels or Eval scores to the same subject and time window.
- **Operational drift:** latency, error rate, token use, cost, or tool-call
  behavior changes; use telemetry with stable resource and deployment labels.

## Statistical hygiene

Choose a window that matches the process cadence, not an arbitrary dashboard
interval. Report sample count, missing values, confidence or control limits,
and the practical effect size. PSI is useful for coarse population comparison
but is sensitive to bins and small samples; combine it with a minimum-count
floor and a business-relevant metric. SPC needs a stable baseline and an
explicit treatment of autocorrelation and seasonality. Thresholds should be
resolved into a typed condition and versioned with the Card.

## Operations

Separate measurement from scheduling and reaction: Drift produces an
observation, Trigger schedules evaluation, and Operator performs one action.
Use hysteresis, cooldowns, and deduplication at the reaction boundary so a
noisy metric does not page repeatedly. Record the baseline, window, method,
threshold, and source in the emitted observation. A drift finding is evidence
for investigation, rollback, retraining, or policy review; it is not an
automatic diagnosis.

## Failure modes and anti-patterns

Reject a single global threshold across tenants, silently changing baselines,
mixing deployment cohorts, alerting on a metric before its ingest is complete,
and treating statistical significance as business significance. Missing labels,
late data, schema changes, and source outages must produce an explicit
unknown/degraded state rather than a false “no drift” result.

Do not put schedules or notification code inside Drift. Do not point a monitor
at an unversioned subject, or let an external provider push arbitrary events
onto the Wyrd wire; read it through `Source` and retain the same subject
identity.

## Stable Wyrd anchors

- Drift envelope and signal vocabulary: `architecture/wyrd-design.md` §Drift.
- Drift types and profiles: `crates/wyrd-spec/src/vala/drift/`.
- Scheduling and reactions: `architecture/wyrd-design.md` §§Trigger and
  Operator.
- Observation storage and query: `crates/vala/` and
  `crates/wyrd/wyrd-server/`.

## Primary grounding

- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- [Gama et al., A survey on concept drift adaptation](https://dl.acm.org/doi/10.1145/2523813)
- [OpenTelemetry metrics](https://opentelemetry.io/docs/concepts/signals/#metrics)
- [Apache DataFusion pruning features](https://datafusion.apache.org/user-guide/features.html)
- Wyrd anchors: `architecture/wyrd-design.md` §Drift;
  `crates/wyrd-spec/src/vala/drift/`; `crates/vala/`.

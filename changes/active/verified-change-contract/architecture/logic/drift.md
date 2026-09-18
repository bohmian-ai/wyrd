# Drift runtime logic

**Status:** agreed initial Drift implementation

`run.observe.drift(...)` converts caller input to the existing
`DriftRecordObservation`, projects fixed-schema rows, then inserts them through
the state-owned Bifrost facade and its existing `WriterPool`. Bifrost
describes and caches the fixed system table at `start_bifrost`, so the
observation call makes no schema request. The queue and shared `wyrd-client`
remain Drift/Verifier-kind-agnostic and publish through Gate and Scribe.
The generic VerificationRuntime owns run scheduling and
dispatch; its Drift implementation loads a fitted baseline, asks Oracle to
aggregate a fixed window, and scores only the aggregate output in Rust. There
is no second observation format, per-Verifier input copy, or projector.

## Constraints

- Reuse `DriftSpec`, `DriftRecordObservation`, `FittedBaseline`, and the
  existing `vala-drift` algorithms.
- Do not publish PSI bin counts, SPC subgroup means, or custom-metric batch
  means from clients.
- Store raw observations and Drift results in Bifrost.
- Store bindings, baseline readiness, run state, and Operator dispatches in
  Postgres. Do not add an Alert table or notification outbox.
- Every analysis uses an immutable, half-open window: `[start, end)`.
- A failed run is a completed Verifier result. An engine error is a retryable
  run failure, not a drift finding.

## Run creation and windows

Scheduled and manual binding execution create the same durable Drift run.

For a scheduled run:

1. The first successful exact service-principal token exchange activates a
   scheduled binding and sets its next future cron boundary. It creates no
   historical run.
2. For a ready, runtime-active binding due at boundary `end`, the scheduler
   creates at most one run for `[previous_boundary, end)` and advances the
   cursor in the same short Postgres transaction. An hourly run due at 01:00
   always reads `[00:00, 01:00)`, even if claimed at 01:03.
3. Inactive and baseline-unready due occurrences create no run. Downtime
   occurrences are skipped, not backfilled; the cursor advances to the next
   future boundary.
4. A claimed run and all its retries retain the stored window. The worker
   lease and engine retry never hold the binding-row lock during analysis.

Every manual run requires explicit `start` and `end`. It does not advance the
cron cursor. Manually invoking a verification binding uses the supplied window
and runs its `on_failure` behavior; invoking the Verifier directly is
analysis-only.

The durable run records its origin as either `direct` or `binding`. Binding
origin includes the stable binding identity, the effective Trigger Card UID and
version or inline spec digest, plus the scheduled occurrence or manual
invocation identity. Every cron run has binding origin. Retries retain the
original value.

Window membership uses server-stamped managed `wyrd_event_time`, not the
client-provided `created_at`. This aligns analysis with the daily physical
partition and the no-backfill policy: a delayed queue delivery is analyzed in
the window in which Scribe receives it. `created_at` remains lineage and SPC
ordering evidence. A manual rerun can inspect any explicit retained window.

## Common execution flow

```text
cron occurrence or manual binding request
        -> Postgres verifier_runs row with fixed [start, end)
        -> generic runner claims row and loads exact Verifier + fitted baseline
        -> Drift plan selects subject-scoped Bifrost observations
        -> Oracle/DataFusion returns method-specific aggregates
        -> vala-drift scores aggregates into existing DriftReport semantics
        -> write vala.drift.result_features, then vala.verification.results
           as separately acknowledged Arrow batches via Gate/Scribe
        -> Postgres TX: settle run and insert 0..N operator_dispatches
        -> generic Operator worker independently claims each dispatch
        -> Notify sends alert/notification to its destination

manual Verifier request
        -> the same query, aggregation, scoring, and result persistence
        -> no Operator dispatch
```

Oracle authorizes and tenant-scopes the raw table scan. Each plan filters the
exact observed subject `card_uid`, one configured feature `series`, and the
stored `wyrd_event_time` window. Raw rows have no Verifier or binding ID;
multiple bindings can analyze the same observation. Exact Verifier UID/version
is frozen on `verifier_runs`, not used as a raw-input predicate.

## Bifrost observation schema and projection

Keep the existing tall `vala.drift.observations` table: one physical row per
`DriftRecordObservation.features` entry, with the same `record_id` for every
feature from one logical observation. Change its hourly partition to UTC day
on managed `wyrd_event_time`; keep the declared `series` Bloom filter and
remove the obsolete `drift_ref` column and Bloom filter. Bifrost's managed
`card_uid`, `run_id`, and `principal_id` Bloom floor remains. Do not partition
by subject, feature, Verifier, or binding.

| Column | Type | Meaning |
|---|---|---|
| `record_id` | Utf8 | Logical observation ID, repeated across feature rows |
| `series` | Utf8 | Existing `FeatureName`; this is a row value, not a physical column name |
| `num_value` | Float64 nullable | Numeric analysis projection |
| `str_value` | Utf8 nullable | Canonical categorical-key projection |
| `session_id` | Utf8 nullable | Existing optional session correlation |
| `created_at` | timestamp (microseconds, UTC) | Existing client observation time; used for SPC order, not window membership |
| managed `card_uid` | Card UID | Observed Model/Agent/Service subject, never Verifier |
| managed `run_id` | Run ID | Client invocation correlation |
| managed `principal_id` | Principal ID | Authenticated client writer |
| managed `data_tenant_id`, `wyrd_event_time` | tenant ID, UTC timestamp | Tenant scope and server ingest-time partition/window |

The `drift(...)` call projects `FeatureValue` into these existing value
columns before inserting the fixed-schema row through the state-owned
Bifrost facade's queue.
Neither shared `wyrd-client` nor `wyrd-queue` performs Drift projection.
`Cat` and `Bool` set only
`str_value`; `Int` and `Float` set `num_value` and also the canonical string
needed if that feature is declared categorical by a baseline. The baseline
fitter and client projection must use the same Arrow-compatible string
conversion for numeric and boolean categories. Non-finite floats and integers
that cannot be represented exactly in Float64 are rejected at the observation
boundary rather than silently altered; callers with large categorical
identifiers publish `Cat`. Nullness, not a new tag or four scalar columns,
selects the available analytical projections. Gate authorizes the subject
CardRef and stamps the managed identity; it does not fan out a copy per
Verifier.

## Standardized analysis plans

The Drift implementation has three fixed, server-owned DataFusion plan
families: PSI, subgroup SPC, and Custom. There is no user-authored SQL and no
per-feature physical schema discovery. `DriftSignal` names the features;
registration fits and persists a `FittedBaseline`. At run time the runner
loads that exact fitted baseline. For each PSI feature, its fitted `bin_type`
chooses `num_value` or `str_value`, and its fitted bins supply numeric bounds
or category labels. SPC's fitted `chunk_size` chooses subgroup size. Custom's
authored `Metric.name` chooses `series` and its authored baseline supplies the
comparison value. The plan operators are fixed; these values are typed
runtime literals, not executable query text.

```text
for (feature_name, fitted_feature) in fitted_baseline.features:
  PSI Numeric      -> count num_value using fitted_feature.numeric bins
  PSI Categorical  -> count str_value using fitted_feature.category labels
  SPC              -> aggregate num_value in fitted_baseline.chunk_size groups
Custom             -> aggregate num_value where series = profile.metric_name
```

Use Oracle's existing internal `query_plan(LogicalPlan, ...)` authorization,
admission, snapshot, and Arrow-stream seam, and wire it as a real
VerificationRuntime dependency. Build the plan from typed DataFusion
expressions: canonical Bifrost table scan -> tenant-authorized subject,
`series`, and `[start, end)` filter -> method-specific projection/aggregate.
Numeric edges and category labels become typed `CASE` literals, not a
temporary profile TableScan or interpolated SQL. The current `query_plan`
seam has no production Verifier caller and must be integration-tested,
including its execution path and admission behavior. Only aggregate rows
return to Rust; SPC rows stream through a bounded eight-point rule window
instead of accumulating an unbounded raw-sample vector. The existing
`feature.rs` helpers read wide baseline Arrow batches; they are not a mapper
for this tall observation table and may
be retained for fitting or replaced there.

The fixed plan shapes and Arrow outputs are:

| Method | DataFusion operators after common filter | Aggregate output to Rust |
|---|---|---|
| PSI numeric | Preflight `COUNT(*) = COUNT(num_value)`; `CASE` over fitted `(lower, upper]` edges; `GROUP BY bin_id`; `COUNT(*)` | `(feature, bin_id, count)` and input-validity counts |
| PSI categorical | `str_value IS NOT NULL`; `CASE` over fitted labels, with unmatched `bin_id = -1`; `GROUP BY bin_id`; `COUNT(*)` | `(feature, bin_id, count)` including unknown count |
| SPC X̄–S | Preflight `COUNT(*) = COUNT(num_value)`; deterministic `ROW_NUMBER` over numeric rows per feature; group consecutive fixed-size subgroups; `COUNT(*)`, `AVG`, `STDDEV_SAMP`; retain complete groups; order by subgroup | `(feature, subgroup_index, n, mean, sample_stddev)` and input-validity counts |
| Custom | `COUNT(*)`, `COUNT(num_value)`, `AVG(num_value)` | `(metric, observed_count, numeric_count, window_mean)` |

The shared filter is the tenant-authorized table scan plus exact subject UID,
feature name, and `wyrd_event_time >= start AND wyrd_event_time < end`. It
never filters raw input by Verifier or binding ID. The plan builder uses
fitted baseline values only as data. No DataFusion raw observation batch is
pulled into the Rust scorer.

## PSI

The fitted baseline owns the numeric bin edges or categorical bins and their
baseline proportions. The server computes target counts for each run window.

1. Select each fitted feature's `series` and its fitted `bin_type`-appropriate
   value column within the run window.
2. DataFusion assigns numeric values to fitted `(lower, upper]` bins or
   categorical values to fitted labels, then counts each bin. Unmatched
   categorical labels contribute to the target total through `bin_id = -1`
   but not to a fitted category's numerator.
3. Rust zero-fills absent fitted bins and sums all counts for the non-null
   target total. It applies the existing minimum-sample, smoothing, PSI
   formula, threshold, and verdict logic unchanged.

The existing PSI implementation must expose an aggregate-count input that
shares its current formula and report construction. Passing count rows into
the raw-value scorer would bin them twice. The fitted profile is frozen at
registration; observed counts never refit the bins.

The existing fitted numeric edges already cover values below and above the
baseline range through infinite boundary bins. For categorical PSI, categories
absent from the fitted baseline remain part of the target total but do not
create a new fitted bin, matching the current scorer.

## SPC: initial NIST subgroup X̄–S method

The initial SPC method is a fixed-size subgroup X̄–S chart with Western
Electric (WECO) rules. It is not the NIST individuals/moving-range chart.
Each feature uses one fixed subgroup size `n >= 2`, frozen in its fitted
baseline. An authored `sample_size` supplies `n`; the current `0` adaptive
form resolves to `n = 4` at fitting, not to the existing volume-dependent
25/100/1,000/... chunk sizes. The baseline must contain at least 25 complete
subgroups for the initial readiness gate. NIST presents 25 groups of four as
historical guidance, not a universal mathematical constant; this gate is
Wyrd's conservative initial product policy. The registered SPC baseline Data
Card must provide `created_at` and unique `record_id` columns alongside its
wide feature columns. The baseline fitter and runtime plan both sort by
`created_at, record_id` before forming consecutive groups; the fitted-baseline
job reports not-ready with a validation error if those columns are absent or
invalid. DataFusion produces baseline subgroup means and sample deviations
before Rust fits limits. Incomplete trailing baseline and window subgroups
are excluded; they are not scored with full-`n` limits or carried into another
immutable run window. A window without a complete subgroup is inconclusive.
Consecutive time-ordered rows are Wyrd's operational subgroup policy; users
must collect comparable observations because ordering alone does not prove a
rational subgroup.

For each of the `m` complete baseline subgroups, DataFusion produces its mean
`x̄_i` and sample standard deviation `s_i`. The Rust fitter calculates
`x̄̄ = mean(x̄_i)`, `s̄ = mean(s_i)`,
`c4(n) = √(2/(n−1)) Γ(n/2)/Γ((n−1)/2)`, and `σ̂ = s̄ / c4(n)`; it persists
these fitted values, `n`, and the X̄ and S chart centers/limits in the
server-owned fitted baseline. X̄ zone spacing is `σ̂ / √n`, **not** `σ̂` as
the current implementation uses. S zone spacing is
`σ̂ √(1 - c4(n)²)`. For either chart, the one-, two-, and three-sigma limits
are center ± 1/2/3 times its spacing. The X̄ center is `x̄̄`; the S center
is `s̄`. A negative S lower control limit is operationally clamped to zero
because a sample standard deviation cannot be negative. The fitted limits
remain frozen for later windows; target observations never recalculate them.
Non-finite statistics or `s̄ = 0` fail baseline readiness with a typed
validation reason; a zero-variance baseline cannot define these control
zones.

For each runtime feature, DataFusion filters non-null numeric rows, orders
them by `created_at ASC, record_id ASC` with stable managed row identity as a
tie-breaker if needed, assigns `ROW_NUMBER`, groups every `n` consecutive
values, retains only groups with `COUNT(*) = n`, and returns their ordered
`AVG(num_value)` and `STDDEV_SAMP(num_value)` with subgroup index and count.
Rust checks the S chart first and the X̄ chart second, applying the four
canonical WECO tests to each chart's ordered points. If S fails, report the
variability finding and do not interpret X̄ as an independent pass; its limits
assume stable within-subgroup variation.

1. One point beyond a three-sigma limit.
2. At least two of three consecutive points beyond two sigma on the same
   side of center.
3. At least four of five consecutive points beyond one sigma on the same
   side of center.
4. Eight consecutive points on the same side of center.

Only fully available sliding windows can satisfy a multi-point rule. Compare
the actual point values and sides: "beyond" and "same side" are strict, so a
point exactly on a limit or center does not satisfy that condition. Do not
infer trend or alternation from discretized zone numbers. Six-point trend and
14-point alternation are separate
NIST supplementary rules and are out of scope. A failed S or X̄ rule makes
that feature drift; persist chart, rule, subgroup index, observed statistic,
center, and limits as explainable detail. An empty/insufficient window is
inconclusive, not `NoDrift`. NIST warns that enabling the supplementary
signals increases false-alarm frequency, so every finding names its rule.

The existing `SpcWecoRule.rule_string` is an eight-number zone/alternation
language, **not** the four NIST WECO tests. Remove `weco_rule` and the
zone-only `alert_threshold` from the initial Verifier Drift SPC profile;
retain `sample_size` (`0` means the fitted default `n = 4`, `1` is invalid,
and an authored value `>= 2` is frozen). Failure is defined by the four fixed
rules above, with no second zone threshold that can suppress a WECO finding.
A configurable rule set or individuals chart can be added later as an
explicit new contract. Refactor `vala-drift` so baseline
fitting uses subgroup mean and sample deviation, and aggregate-input scoring
shares report construction without passing subgroup means through the old
raw-value scorer. Replace the current WECO evaluator and its tests, including
its same-side alternation and boundary behavior, with tests for these four
precise rules. NIST notes that multiple rules increase false alarms; report
which chart and rule fired rather than hiding this behind one score.

Sources: [NIST variable control charts and WECO rules](https://www.itl.nist.gov/div898/handbook/pmc/section3/pmc32.htm),
[NIST X̄–S equations](https://www.itl.nist.gov/div898/handbook/pmc/section3/pmc321.htm),
and [NIST individuals chart (excluded)](https://www.itl.nist.gov/div898/handbook/pmc/section3/pmc322.htm).

## Customer metrics

Custom drift monitors one named numeric metric using the existing
`DriftSignal::Metric`, `CustomProfile`, `DriftRecordObservation`, and
`score_custom` contracts. Clients publish raw values. The server computes one
window mean and compares its absolute deviation from the authored baseline.

```yaml
method: Custom
signal:
  kind: Metric
  name: latency_ms
condition:
  kind: Statistical
profile:
  kind: Custom
  metric_name: latency_ms
  baseline_value: 100.0
  alert_threshold: 20.0
```

### Registration control flow

```text
register Verifier
  -> require implementation.kind = drift
  -> require method = Custom
  -> require signal = Metric { name }
  -> require profile = Custom { metric_name, baseline_value, alert_threshold }
  -> require condition = Statistical
  -> require signal.name == profile.metric_name
  -> validate metric name as FeatureName
  -> require finite baseline_value
  -> require finite alert_threshold >= 0
  -> persist Verifier as ready
```

Custom has no baseline Data reference and no fitted-baseline job. The runtime
may use the existing stateless `FittedBaseline::Custom` marker when dispatching
through `score_drift`, but it does not persist fitted state.

Registration rejects `Above`, `Below`, and `Outside` for Custom. Those variants
would introduce a second threshold source alongside
`CustomProfile.alert_threshold`; the initial implementation has one rule.

### Observation control flow

```text
run.observe.drift(input)
  -> inside drift(...): normalize and validate into DriftRecordObservation
  -> inside drift(...): project each feature into the fixed tall schema above
  -> insert subject-scoped flat rows once into state-owned Bifrost's existing queue
  -> generic wyrd-queue builds Arrow IPC and publishes without Drift logic
  -> Gate authenticates the publisher and admits the table write
  -> Scribe authorizes the subject CardRef and appends to vala.drift.observations
  -> any matching Drift bindings may read those rows in their own runs
```

The client does not know which metrics are Custom, does not calculate a mean,
and does not discard unrelated features. Later binding changes do not mutate
the stored subject identity. The Custom run selects its configured `series`
later; no new projector or record type is involved.

### Window aggregate and scoring

The standardized Custom plan reads the run's subject, metric `series`, and
immutable ingest-time window through Oracle. DataFusion returns one row:

```sql
SELECT
    COUNT(*) AS observed_count,
    COUNT(num_value) AS numeric_count,
    AVG(num_value) AS window_mean
FROM "vala.drift.observations"
WHERE card_uid = <subject_uid>
  AND series = <metric_name>
  AND wyrd_event_time >= <window_start>
  AND wyrd_event_time < <window_end>;
```

This SQL is the logical plan shape, **not** runtime-assembled SQL or a new
public query surface. `<...>` are typed plan inputs from the frozen run and
Verifier. Oracle supplies the normal tenant boundary and live-tail fence.
The worker never loads raw values or averages client-batch averages.

```text
claim Custom Drift run
  -> load exact Verifier UID/version and authored Custom profile
  -> execute the fixed aggregate plan for stored [window_start, window_end)
  -> if observed_count == 0:
       complete with common verdict Inconclusive and no DriftReport
  -> if numeric_count != observed_count or window_mean is not finite:
       complete with common verdict Inconclusive and no DriftReport
  -> otherwise apply the existing Custom score formula to window_mean
  -> write required detail and common result batches through Gate/Scribe
  -> after both ACKs, settle the run and dispatch configured Operators
     only for a failed binding-created result
```

Extract a narrow aggregate-input scoring entry point so `score_custom` and
the server path share this existing formula without manufacturing a fake raw
Arrow batch:

```text
score   = abs(window_mean - baseline_value)
Drift   when score > alert_threshold
NoDrift when score <= alert_threshold
```

Equality is not drift. Invalid or insufficient *input* is inconclusive and
cannot dispatch an Operator; an Oracle, storage, or scorer *engine error* is a
retryable errored run without a manufactured verdict. A direct manual
Verifier run persists the same result but does not create a dispatch.

Run status and verdict remain separate:

| Outcome | Run status | Common verdict | DriftReport |
|---|---|---|---|
| Score above threshold | `completed` | `failed` | persisted |
| Score at or below threshold | `completed` | `passed` | persisted |
| Empty window | `completed` | `inconclusive` | absent |
| Invalid or non-numeric stored metric value | `completed` | `inconclusive` | absent |
| Oracle or scoring engine failure | `retrying` or terminal `errored` | absent | absent |

### Result semantics

Successful and inconclusive common results retain enough typed evidence to
explain the decision:

```text
Verifier UID and version
subject identity
metric name
window_start and window_end
observed_count
window_mean when present
baseline_value
score when present
alert_threshold
verdict
```

This evidence supplements the existing `DriftReport`; it does not replace or
fork that type. Inconclusive results carry an explanation and counts without
pretending an invalid window passed. Engine errors live in run status rather
than manufacturing a Verification Result with a verdict.

### Required customer-metric tests

The production journey must prove:

1. A valid Custom Verifier becomes ready without a Data Card or baseline job.
2. Mismatched metric names, non-Statistical conditions, non-finite configuration,
   and negative thresholds fail registration.
3. `run.observe.drift` projects raw Int/Float values once through the normal
   queue and Bifrost path without client aggregation; non-representable
   integers and non-finite floats are rejected before enqueue.
4. One subject observation produces one physical row per feature, regardless
   of how many Verifier bindings use it. Rows for another subject do not
   contribute.
5. Rows stamped before `start` and at or after `end` do not contribute; a
   delayed client observation belongs to its Scribe-ingest window.
6. Unequal observation batches produce a correctly weighted raw-value mean,
   not a mean of means.
7. Score below or equal to the threshold is `NoDrift`; score above it is
   `Drift`.
8. An empty, non-numeric, or otherwise invalid window is `Inconclusive`, not
   a pass or an engine error, and creates no Operator dispatch.
9. A failed binding-created result creates one dispatch per distinct configured
   Operator in the run-settlement transaction; a direct run creates none.
10. Manual and cron runs, restart recovery, authorization, and tenant isolation
    use the same server analysis path.

## Result, Operator, and retry semantics

The Drift engine's `DriftReport` remains the semantic scoring output. The
server maps its feature/chart/metric findings to `vala.drift.result_features`
and one canonical summary to `vala.verification.results`; dashboards start at
the summary and join details on `(data_tenant_id, result_id)`. The result
table's managed `card_uid` is the exact Verifier UID, while analytical payload
retains the subject UID and binding ID. The tenant-scoped internal SYSTEM
principal writes each Arrow batch through `wyrd_client::Bifrost` -> Gate ->
Scribe, not to a local Scribe instance.

Write the required detail batch first and the summary batch second. Each
requires its own Scribe ACK. Only then may the runner settle `verifier_runs`
and insert one `operator_dispatches` row per distinct configured Operator in
the same Postgres transaction. A passed, inconclusive, direct analysis-only,
or terminal engine-error run inserts none. Engine failures retry the same
leased run with bounded attempts and backoff; a failed *verdict* does not
retry. Retry an unacknowledged batch only with its same sealed Arrow payload,
table, and batch ID. A process crash can leave partial Bifrost result rows;
this initial change accepts partial visibility and does not claim cross-table
atomicity or add per-table repair.

The generic Operator worker independently claims each committed dispatch and
retries delivery. Notify sends an alert/notification containing the failed
result and findings; it does not persist an Alert row or poll an Alert table.
Sibling Operators proceed independently. External delivery can be at-least-once
after an ambiguous failure; use the stable dispatch ID as an idempotency key
where the destination supports one. A later passing result never mutates a
prior failed result or dispatch.

## Required journeys

PSI numeric, PSI categorical, SPC, and Custom each need a real
Rust/Python/TypeScript client-to-server journey that proves:

1. Baseline Data registration and asynchronous fitted-baseline readiness.
   Custom instead proves readiness without baseline fitting.
2. A subject-scoped `DriftRecordObservation` reaches exactly one set of tall
   feature rows through wyrd-queue, Gate, Scribe, and Bifrost; two bindings
   can consume the same rows without copies or Verifier IDs on input.
3. Fixed `[start, end)` ingest-time filtering, strict end exclusion, server
   aggregates, baseline-driven feature/value-column selection, and no raw
   batch transfer to the Rust scorer.
4. PSI numeric fitted-edge and categorical fitted-label counts, zero bins,
   unknown category totals, minimum sample, pass, drift, and inconclusive.
5. SPC baseline X̄–S limits against a known fixture, `n = 4` adaptive
   default, full-subgroup handling, S-before-X̄ analysis, all four WECO
   tests, boundary equality, and no-data inconclusive. Tests must catch the
   old missing `1/√n` factor, partial-group contamination, zone-only trend,
   and same-side pseudo-alternation.
6. Custom raw-value averaging, strict threshold equality, missing/invalid
   input, and pass/drift/inconclusive behavior.
7. Direct manual and binding-driven cron runs use the same analysis path;
   inactive/unready/missed cron occurrences create no run and no backfill.
8. Detail then summary Arrow/gRPC ACKs, permitted partial visibility after
   failure, leased run retries, and Postgres settlement plus Operator fanout.
9. Notify delivery, independent sibling dispatch retries, restart recovery,
   authorization, and tenant isolation. Passing and inconclusive runs never
   rewrite an earlier failure.

## Implementation sequence

1. Revise only the existing Drift observation table layout and projection:
   remove `drift_ref`, keep the tall two-value schema, switch to UTC-day
   partition, and prove canonical category string parity with Data Card
   baseline fitting. Reuse the existing client queue, Gate, and Scribe path.
2. Wire the generic runner's Drift dispatch to fitted-baseline readiness and
   Oracle's typed `query_plan` path. Test tenant authorization, subject/series
   predicates, fixed ingest-time window, live-tail visibility, bounded
   aggregate output, and Oracle admission. Do not add a public SQL API or
   profile `MemTable` scan.
3. Add the fixed PSI numeric/categorical and Custom aggregate plans and
   narrow count/mean scoring inputs in `vala-drift`, sharing the existing PSI
   and Custom formulas and `DriftReport` construction.
4. Refactor SPC fitting and scoring together: DataFusion emits baseline and
   target complete-subgroup mean/sample-SD rows; Rust fits NIST X̄–S limits
   and evaluates the four WECO rules on ordered aggregate rows. Update the
   incompatible eight-number rule contract, frozen fitted-baseline shape,
   and focused statistical fixtures in the same work.
5. Map scored reports to the canonical detail and summary Bifrost tables,
   require both ACKs, settle the existing run, and rely on the generic
   Operator dispatch worker. Prove the method-specific journeys above through
   all three first-class SDKs.

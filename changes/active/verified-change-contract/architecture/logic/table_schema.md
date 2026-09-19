# Verification table schemas

These are the complete physical Arrow schemas for the five Bifrost tables in
`SPEC-verified-change-contract`, revision 32. Each table has exactly its
listed authored columns, followed in order by the shared managed columns
below. This document is the schema authority for the spec; the locked
control-flow diagram remains the runtime-flow authority. `Nullable: no` means
the Arrow field is non-nullable.

## Shared managed columns, appended to every table

| Column | Arrow type | Nullable | Meaning |
|---|---|---|---|
| `run_id` | Utf8 | yes | Client invocation for observation rows; Verifier run for result and detail rows. Populated for these workflows. |
| `card_uid` | Utf8 | yes | Observed subject for observation rows; exact Verifier Card for result and detail rows. Populated for these workflows. |
| `principal_id` | Utf8 | no | Authenticated client publisher for observations; tenant-scoped internal SYSTEM writer for results and details. |
| `wyrd_request_id` | Utf8 | no | Server-stamped request correlation. |
| `wyrd_event_time` | Timestamp(Microsecond, UTC) | no | Server-stamped observation receipt time, or the one server-chosen event time shared by a result and all its detail rows. |
| `wyrd_ingested_at` | Timestamp(Microsecond, UTC) | no | Bifrost ingestion time. |
| `wyrd_batch_id` | FixedSizeBinary(16) | no | Bifrost batch identity. |
| `wyrd_row_ordinal` | Int32 | no | Row position within the batch. |
| `data_tenant_id` | Utf8 | no | Physical tenant key, including for result/detail joins. |

These are Bifrost's existing `CorrelationPolicy::Observation` managed fields;
clients do not author them as table payload columns. All five tables partition
by UTC day on `wyrd_event_time`. Bifrost's managed Bloom floor is `run_id`,
`card_uid`, and `principal_id`. `vala.drift.observations` additionally Blooms
`series`; `vala.verification.results` additionally Blooms `result_id`,
`subject_card_uid`, and `binding_id`; both detail tables additionally Bloom
`result_id`. `vala.eval.observations` has no additional Bloom column.

## `vala.drift.observations`

One row per feature in one `DriftRecordObservation`. Rows from one observation
share `record_id`. The table uses the managed columns above after these fields.

| Column | Arrow type | Nullable | Meaning |
|---|---|---|---|
| `record_id` | Utf8 | no | Logical observation ID. |
| `series` | Utf8 | no | Feature name. |
| `num_value` | Float64 | yes | Numeric analysis value; null for categorical and boolean values. |
| `str_value` | Utf8 | yes | Canonical categorical value; numeric values also carry their canonical string. |
| `session_id` | Utf8 | yes | Optional session correlation. |
| `created_at` | Timestamp(Microsecond, UTC) | no | Client emission time; distinct from server-stamped `wyrd_event_time`. |

There is no `drift_ref` column. Managed `card_uid` is the subject, never the
Verifier or binding.

## `vala.eval.observations`

One row per committed `EvalRecordObservation`. The table uses the managed
columns above after these fields.

| Column | Arrow type | Nullable | Meaning |
|---|---|---|---|
| `record_id` | Utf8 | no | Logical input record ID. |
| `session_id` | Utf8 | yes | Optional session correlation. |
| `context` | Utf8 | no | Canonical JSON of `EvalRecordObservation.context`. |
| `trace_id` | FixedSizeBinary(16) | yes | Optional trace identity. |
| `span_id` | FixedSizeBinary(8) | yes | Optional span identity; requires `trace_id` when populated. |
| `created_at` | Timestamp(Microsecond, UTC) | no | Client emission time; distinct from server-stamped `wyrd_event_time`. |
| `media` | Utf8 | yes | Canonical JSON of optional `Vec<MediaRef>` descriptors; URIs, not media bytes. |

Managed `card_uid` is the subject. The payload has no `eval_ref`, Verifier or
binding ID, or duplicate user `run_id`. `context` and `media` use Bifrost's
existing sensitive-payload classification.

## `vala.verification.results`

Exactly one row for each completed Drift or Eval Verifier run. Other run
statuses have no result row. The table uses the managed columns above after
these fields.

| Column | Arrow type | Nullable | Meaning |
|---|---|---|---|
| `result_id` | Utf8 | no | Stable result identity. |
| `implementation` | Utf8 | no | `drift` or `eval`; selects the `details` type. |
| `execution_status` | Utf8 | no | `completed` for every row in this table. |
| `verdict` | Utf8 | no | Common `passed`, `failed`, or `inconclusive` verdict. |
| `verifier_version` | Utf8 | no | Exact Verifier Card version used. |
| `owner_card_uid` | Utf8 | yes | Service or standalone Agent binding owner; null for a direct Verifier run. |
| `subject_card_uid` | Utf8 | no | Verified subject Card. |
| `binding_id` | Utf8 | yes | Binding identity; null for a direct Verifier run. |
| `trigger_identity` | Utf8 | yes | Canonical JSON of frozen Trigger Card UID/version or inline digest; null for a direct run. |
| `source_record_id` | Utf8 | yes | Eval input record ID; null for Drift. |
| `window_start` | Timestamp(Microsecond, UTC) | yes | Drift window start; null for Eval. |
| `window_end` | Timestamp(Microsecond, UTC) | yes | Drift window end; null for Eval. |
| `started_at` | Timestamp(Microsecond, UTC) | no | Verifier execution start. |
| `ended_at` | Timestamp(Microsecond, UTC) | no | Verifier execution end. |
| `details` | Utf8 | yes | Canonical JSON of the existing `DriftReport` when Drift scoring produced one, or `EvalWorkflowSummary` for Eval; null only for completed pre-scoring inconclusive Drift. |

`details` is the single implementation-specific summary payload. A sampled-out
Eval writes the existing zero-count summary. A completed Drift execution that
cannot score valid input writes null rather than fabricating a report. There are no
separate Drift method, Eval count, pass-rate, duration, or pass-gate columns in
this shared table. Managed `run_id` is the Verifier run and managed `card_uid`
is the Verifier Card. Results join details on (`data_tenant_id`, `result_id`).

## `vala.drift.result_features`

One row per entry in `DriftReport.features`. The table uses the managed
columns above after these fields.

| Column | Arrow type | Nullable | Meaning |
|---|---|---|---|
| `result_id` | Utf8 | no | Parent `vala.verification.results` identity. |
| `owner_card_uid` | Utf8 | yes | Binding owner; null for a direct Verifier run. |
| `subject_card_uid` | Utf8 | no | Verified subject. |
| `binding_id` | Utf8 | yes | Binding identity; null for a direct run. |
| `window_start` | Timestamp(Microsecond, UTC) | no | Analyzed Drift window start. |
| `window_end` | Timestamp(Microsecond, UTC) | no | Analyzed Drift window end. |
| `method` | Utf8 | no | `DriftReport.method`, repeated for cross-run method queries. |
| `feature` | Utf8 | no | `FeatureDriftReport.feature`. |
| `score` | Float64 | yes | Feature score; engine NaN for an inconclusive score becomes null. |
| `threshold` | Float64 | yes | Feature threshold; engine NaN becomes null. |
| `verdict` | Utf8 | no | `no_drift`, `drift`, or `inconclusive`. |

## `vala.eval.result_items`

One row per `EvalReport.outcomes` `TaskRunOutcome` for **one Eval workflow run**,
including both `Ran` and `Skipped`. There are no workflow-summary, scenario,
subject-summary, or synthetic task rows here. The table uses the managed
columns above after these fields.

| Column | Arrow type | Nullable | Meaning |
|---|---|---|---|
| `result_id` | Utf8 | no | Parent `vala.verification.results` identity. |
| `owner_card_uid` | Utf8 | yes | Binding owner; null for a direct Verifier run. |
| `subject_card_uid` | Utf8 | no | Verified subject. |
| `binding_id` | Utf8 | yes | Binding identity; null for a direct run. |
| `source_record_id` | Utf8 | no | Committed Eval input record ID. |
| `task_id` | Utf8 | no | `AssertionResult.task_id` for `Ran`; `TaskRunOutcome::Skipped.task_id` for `Skipped`. |
| `outcome_kind` | Utf8 | no | `ran` or `skipped`. |
| `passed` | Boolean | yes | `AssertionResult.passed` for `Ran`; null for `Skipped`. |
| `actual` | Utf8 | yes | JSON of captured `AssertionResult.actual`; null when absent or skipped. A captured JSON null is the text `null`. |
| `expected` | Utf8 | yes | Canonical JSON of `AssertionResult.expected` for `Ran`; null for `Skipped`. |
| `operator` | Utf8 | yes | Canonical JSON of `AssertionResult.operator` for `Ran`; null for `Skipped`. |
| `message` | Utf8 | yes | `AssertionResult.message` for `Ran`; null for `Skipped`. |
| `stage` | Int32 | yes | `AssertionResult.stage` for `Ran`; null for `Skipped`. |
| `started_at` | Timestamp(Microsecond, UTC) | yes | `AssertionResult.started_at` for `Ran`; null for `Skipped`. |
| `duration_ms` | Int64 | yes | `AssertionResult.duration_ms` for `Ran`; null for `Skipped`. |
| `skip_reason` | Utf8 | yes | `condition_false` or `dependency_skipped` for `Skipped`; null for `Ran`. |
| `upstream_task_id` | Utf8 | yes | `SkipReason::DependencySkipped.upstream`; null otherwise. |

`actual` follows the existing `context_capture` policy before persistence.
`actual`, `expected`, and `message` use Bifrost's existing sensitive-payload
classification. The current record-scoring path discards skipped outcomes
while producing `EvalResults`; the Verifier runner must retain its existing
`EvalReport` through persistence so every outcome appears here. This uses the
same Eval engine. `EvalWorkflowSummary` in the common result counts only
executed tasks; skipped outcomes remain visible in this table.

A sampled-out Eval run writes no item rows. An all-skipped workflow writes its
`Skipped` rows. Errored and timed-out runs write neither the common result nor
item rows.

## Retention

All five tables use Bifrost's existing retention and maintenance behavior.
This change defines no verification-specific TTL, deletion job, or retention
setting.

---
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
status: approved
---

# Supplemental telemetry schemas and column production

This is a normative supplement to [specification revision 12](../spec.md), not
an independently approved schema. It fixes the proposed persisted fields and
their production semantics for `bifrost.traces.spans`, `bifrost.logs.records`, and
`bifrost.metrics.points`. It does not define private builders or module layout.

## Reading the complete schemas

Each proposed table is the union of **all shared rows** and **all rows in its
signal section**. Together the tables below enumerate every current and
proposed top-level field, including retained fields. `Existing` means the name,
logical type, and nullability are unchanged. `New` means the current schema
does not have that field. `Renamed` preserves an existing field's identity and
values under its proposed name. The only existing-column rename is `card_uid`
to `subject_card_uid`; no current field is removed.

Types: `str` = Arrow Utf8; `bool` = Boolean; `i32/i64/f64` = Int32/Int64/Float64;
`bin16/bin8` = FixedSizeBinary(16/8); `Tns/Tus` = UTC timestamps with nanosecond
or microsecond precision; `V` = the existing canonical Variant extension.
`?` marks a nullable column; otherwise the column is required. List elements
are non-null. Nested field nullability is shown separately below.

This is the logical Arrow/Iceberg contract, not a restatement of Variant's
physical Parquet encoding. Use stable field IDs and the existing logical encodings in the fresh baseline.
Future compatible evolution preserves IDs; no pre-change field-ID migration
is required. Avoid cosmetic raw-field renames beyond the explicit contract.

Current sources were inspected in the caller's worktree:

- [Shared projection](../../../../crates/vala/vala-bifrost-redux/src/tables/signal.rs)
  and [managed fields](../../../../crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs).
- [Span fields](../../../../crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs)
  and [projection](../../../../crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs).
- [Log fields](../../../../crates/vala/vala-bifrost-redux/src/tables/logs/records.rs)
  and [projection](../../../../crates/vala/vala-bifrost-redux/src/tables/logs/projection.rs).
- [Metric fields](../../../../crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs)
  and [projection](../../../../crates/vala/vala-bifrost-redux/src/tables/metrics/projection.rs).

## Shared columns on all three signals

`R` means the enclosing OTLP Resource; `S` means InstrumentationScope. Their
schema URLs come from the enclosing ResourceSpans/Logs/Metrics and
ScopeSpans/Logs/Metrics wrapper, not from R/S themselves. Their values repeat
on every descendant row. Native producers supply the same logical values.

| Current field | Proposed field | Type | Production |
| --- | --- | --- | --- |
| `resource_present` | Existing | bool | Whether R was present; preserve absent versus present-empty. |
| `resource_attributes` | Existing | V | Canonical typed R attributes; empty object when absent. |
| `resource_dropped_attributes_count` | Existing | i64 | R dropped count, or zero when absent; checked widening. |
| `resource_schema_url` | Existing | str | Enclosing resource wrapper's schema URL. |
| `resource_entity_refs` | Existing | list<EntityRef> | R entity references in input order, or empty list. |
| `scope_present` | Existing | bool | Whether S was present. |
| `scope_name` | Existing | str | S name, or empty string. |
| `scope_version` | Existing | str | S version, or empty string. |
| `scope_attributes` | Existing | V | Canonical typed S attributes; empty object when absent. |
| `scope_dropped_attributes_count` | Existing | i64 | S dropped count, or zero; checked widening. |
| `scope_schema_url` | Existing | str | Enclosing scope wrapper's schema URL. |
| `service_name` | Existing | str? | R attribute `service.name`, string promotion. |
| `service_version` | Existing | str? | R attribute `service.version`, string promotion. |
| `deployment_environment` | Existing | str? | First correctly typed string from `deployment.environment.name`, then `deployment.environment`. |
| — | **New `service_namespace`** | str? | R attribute `service.namespace`, string promotion; never a Card identity field. |
| — | **New `service_instance_id`** | str? | R attribute `service.instance.id`, string promotion. |
| — | **New `process_pid`** | i64? | R attribute `process.pid`, signed-integer promotion. |
| — | **New `telemetry_sdk_name`** | str? | R attribute `telemetry.sdk.name`, string promotion. |
| — | **New `telemetry_sdk_language`** | str? | R attribute `telemetry.sdk.language`, string promotion. |
| — | **New `telemetry_sdk_version`** | str? | R attribute `telemetry.sdk.version`, string promotion. |
| `run_id` | Existing | str? | Final record-level `wyrd.run_id`; validate existing RunId text grammar. |
| `card_uid` | **Renamed `subject_card_uid`** | str? | Final record-level `wyrd.card_uid`; validate grammar and authorize exact observation-target UID. The fresh baseline uses the subject name and unchanged value/type semantics. |
| — | **New `workload_card_uid`** | str? | Server-stamped validated exact Service/Agent execution root; not an attribute promotion. |
| `sub_principal_id` | Existing | str | Server-stamped authenticated account subject. |
| `act_principal_id` | Existing | str | Server-stamped authenticated account actor; equals subject for a direct publisher. |
| `wyrd_request_id` | Existing | str | Existing propagated/minted request correlator. |
| `wyrd_ingested_at` | Existing | Tus | Existing PostgreSQL admission instant, once per admitted batch. |

There are **20 current shared columns and 27 proposed shared columns**. These
seven-field managed envelope is shared by all system and dynamic tables as
specified in [table storage and correlation](table-storage-and-correlation.md).
The additional resource/convenience fields remain specific to these signals;
other tables do not acquire the full signal resource schema. Tenant remains
a verified physical-table binding, not a row column.

## `bifrost.traces.spans`

Add these fields to the shared schema. `P` means the OTLP Span. Required
primitive/presence values retain existing validation and absent-value behavior.

| Current field | Proposed field | Type | Production |
| --- | --- | --- | --- |
| `trace_id` | Existing | bin16 | P trace ID; existing nonzero/exact-width validation. |
| `span_id` | Existing | bin8 | P span ID; existing nonzero/exact-width validation. |
| `parent_span_id` | Existing | bin8? | P parent ID, null when absent; retain validation. |
| `trace_state` | Existing | str | P trace state. |
| `flags` | Existing | i64 | P flags, checked widening. |
| `name` | Existing | str | P name; retain name validation. |
| `kind` | Existing | i32 | P's numeric OTel SpanKind; do not replace with span/log row kind. |
| `start_time_unix_nano` | Existing | Tns | Exact P start nanoseconds, converted to UTC timestamp. |
| `end_time_unix_nano` | Existing | Tns | Exact P end nanoseconds, converted to UTC timestamp. |
| `duration_nano` | Existing | i64 | Validated end minus start, in nanoseconds. |
| `status_present` | Existing | bool | Whether P status was present. |
| `status_code` | Existing | i32? | Status code when status is present. |
| `status_message` | Existing | str? | Status message when status is present. |
| `attributes` | Existing | V | Canonical typed P attributes. |
| `dropped_attributes_count` | Existing | i64 | P dropped-attribute count, checked widening. |
| `events` | Existing | list<SpanEvent> | Ordered nested P events; no synthetic table rows. |
| `dropped_events_count` | Existing | i64 | P dropped-event count, checked widening. |
| `links` | Existing | list<SpanLink> | Ordered nested P links. |
| `dropped_links_count` | Existing | i64 | P dropped-link count, checked widening. |
| `gen_ai_operation_name` | Existing | str? | P attribute `gen_ai.operation.name`; strict GenAI string rule. |
| `gen_ai_provider_name` | Existing | str? | P attribute `gen_ai.provider.name`; strict GenAI string rule. |
| `gen_ai_request_model` | Existing | str? | P attribute `gen_ai.request.model`; strict GenAI string rule. |
| `gen_ai_conversation_id` | Existing | str? | P attribute `gen_ai.conversation.id`; strict GenAI string rule. |
| `gen_ai_usage_input_tokens` | Existing | i64? | P attribute `gen_ai.usage.input_tokens`; strict GenAI integer rule. |
| `gen_ai_usage_output_tokens` | Existing | i64? | P attribute `gen_ai.usage.output_tokens`; strict GenAI integer rule. |
| `http_request_method` | Existing | str? | P attribute `http.request.method`, string promotion. |
| `http_route` | Existing | str? | P attribute `http.route`, string promotion. |
| `http_response_status_code` | Existing | i64? | P attribute `http.response.status_code`, signed-integer promotion. |
| `url_full` | Existing | str? | P attribute `url.full`, string promotion. |
| — | **New `url_path`** | str? | P attribute `url.path`, string promotion. |
| — | **New `url_query`** | str? | P attribute `url.query`, string promotion. |
| `exception_type` | Existing | str? | Last event named exactly `exception`: attribute `exception.type`. |
| `exception_message` | Existing | str? | Same selected event: attribute `exception.message`. |
| `exception_stacktrace` | Existing | str? | Same selected event: attribute `exception.stacktrace`. |
| — | **New `is_exception`** | bool | True if P has any event named exactly `exception`, independent of status. |

Current complete schema: **52 columns**. Proposed complete schema:
**62 columns**. The shared Card field is renamed; no raw value, precision, or nested list is removed.

## `bifrost.logs.records`

Add these fields to the shared schema. `L` means the OTLP LogRecord. Its trace
and span IDs remain the original OTel correlation; do not manufacture an
independent span ID or parent relationship to resemble Logfire's own logs.

| Current field | Proposed field | Type | Production |
| --- | --- | --- | --- |
| `time_unix_nano` | Existing | Tns | Exact L event time, retaining existing timestamp validation. |
| `observed_time_unix_nano` | Existing | Tns | Exact L observed time; do not replace event time. |
| `severity_number` | Existing | i32 | L numeric OTel severity. |
| `severity_text` | Existing | str | L severity text. |
| `event_name` | Existing | str? | L event-name presence/value using current projection. |
| `body` | Existing | V? | Typed L AnyValue body, null when absent. |
| `trace_id` | Existing | bin16? | L trace correlation, nullable; retain validation. |
| `span_id` | Existing | bin8? | L span correlation, nullable; retain validation. |
| `flags` | Existing | i64 | L flags, checked widening. |
| `attributes` | Existing | V | Canonical typed L attributes. |
| `dropped_attributes_count` | Existing | i64 | L dropped count, checked widening. |
| `exception_type` | Existing | str? | L attribute `exception.type`, string promotion. |
| `exception_message` | Existing | str? | L attribute `exception.message`, string promotion. |
| `exception_stacktrace` | Existing | str? | L attribute `exception.stacktrace`, string promotion. |
| `body_text` | Existing | str? | Body only when it is a string; no JSON/string conversion. |
| — | **New `is_exception`** | bool | True if any of the three promoted exception strings is non-null, including empty strings. |

Current complete schema: **35 columns**. Proposed complete schema:
**43 columns**. Error severity alone does not imply an exception.

## `bifrost.metrics.points`

Add these fields to the shared schema. One row remains one metric point, not a
metric family or a precomputed rate. `M` means Metric and `D` its selected
data point. Keep exact integers alongside doubles; no generic Float64 scalar
column replaces integer counters.

| Current field | Proposed field | Type | Production |
| --- | --- | --- | --- |
| `metric_name` | Existing | str | M name. |
| `description` | Existing | str | M description. |
| `unit` | Existing | str | M unit. |
| `metadata` | Existing | V | Canonical typed M metadata attributes. |
| `metric_type` | Existing | str | Existing closed signal-kind spelling selected from M data. |
| `time_unix_nano` | Existing | Tns | Exact D observation timestamp, not its start timestamp. |
| `start_time_unix_nano` | Existing | Tns | Exact D start timestamp; retain existing zero/validation semantics. |
| `flags` | Existing | i64 | D flags, checked widening. |
| `attributes` | Existing | V | Canonical typed D attributes. |
| `int_value` | Existing | i64? | Scalar integer oneof branch, otherwise null. |
| `double_value` | Existing | f64? | Scalar double oneof branch, otherwise null; no integer coercion. |
| `aggregation_temporality` | Existing | i32? | Existing OTel temporality on applicable metric kinds. |
| `is_monotonic` | Existing | bool? | Sum monotonicity; null for non-Sum kinds. |
| `histogram_count` | Existing | i64? | Explicit/Exponential Histogram count; checked u64-to-i64 conversion. |
| `histogram_sum` | Existing | f64? | Histogram sum with original protocol presence. |
| `histogram_min` | Existing | f64? | Histogram minimum with original presence. |
| `histogram_max` | Existing | f64? | Histogram maximum with original presence. |
| `bucket_counts` | Existing | list<i64>? | Explicit Histogram buckets, checked counts. |
| `explicit_bounds` | Existing | list<f64>? | Explicit Histogram bounds in original order. |
| `exponential_scale` | Existing | i32? | Exponential Histogram scale. |
| `exponential_zero_count` | Existing | i64? | Exponential Histogram zero count, checked conversion. |
| `exponential_zero_threshold` | Existing | f64? | Exponential Histogram zero threshold. |
| `positive_buckets` | Existing | ExponentialBuckets? | Original positive offset/counts; do not flatten/relabel. |
| `negative_buckets` | Existing | ExponentialBuckets? | Original negative offset/counts. |
| `summary_count` | Existing | i64? | Summary count, checked conversion. |
| `summary_sum` | Existing | f64? | Summary sum. |
| `quantile_values` | Existing | list<QuantileValue>? | Summary quantiles in original order. |
| `exemplars` | Existing | list<Exemplar> | Original point exemplars, empty when absent. |

Current complete schema: **48 columns**. Proposed complete schema:
**55 columns**. Existing metric-kind validation still controls which nullable
fields may be present. Summary data, temporality, exemplars, and absent versus
zero remain distinct.

## Complete nested types, unchanged

All fields are required except those marked `?`. All List elements are
required. These definitions apply to current and proposed schemas alike.

```text
EntityRef = struct<
  type: str,
  id_keys: list<str>,
  description_keys: list<str>,
  schema_url: str
>

SpanEvent = struct<
  time_unix_nano: Tns,
  name: str,
  attributes: V,
  dropped_attributes_count: i64
>

SpanLink = struct<
  trace_id: bin16,
  span_id: bin8,
  trace_state: str,
  flags: i64,
  attributes: V,
  dropped_attributes_count: i64
>

ExponentialBuckets = struct<
  offset: i32?,
  bucket_counts: list<i64>?
>

QuantileValue = struct<quantile: f64, value: f64>

Exemplar = struct<
  time_unix_nano: Tns,
  int_value: i64?,
  double_value: f64?,
  filtered_attributes: V,
  trace_id: bin16?,
  span_id: bin8?
>
```

Nullable bucket structs retain nullable children for correct null-parent
Parquet reads. A present bucket struct has its required logical members;
partial structures are refused under the existing validation contract.

## Extraction and production contract

### Raw values and promoted columns

1. Decode raw OTLP using the existing signal owners. Normalize attribute maps
   by final duplicate-key occurrence. Retain the canonical typed values in
   Variant, including keys whose values were also promoted. Missing key and
   explicit null remain distinct in the raw value.
2. Project R and S once and repeat their values on descendant rows. Do not
   search record attributes to fill a missing resource attribute, or resource
   attributes to fill a record attribute. String promotion accepts only a
   string; integer promotion accepts only an integer. Missing or wrong type
   produces SQL null without discarding the original attribute. No numeric-text
   parsing, stringification, URL parsing fallback, or process/environment lookup
   occurs in the server projection.
3. Environment preserves the existing correctly typed stable-key-first,
   older-key-second fallback. A wrongly typed stable value may fall back to a
   correctly typed older value; both remain in the source Variant. This
   accepts OTel semantic-convention evolution, not a Wyrd space alias.
4. Preserve the existing exception-event rule: all three span exception
   strings come from the last event named `exception`. A missing attribute in
   that event remains null; do not fill it from an earlier event. Logs read
   their own attributes. The new exception booleans use the table's specified
   rule, not severity or a truthy string conversion.
5. GenAI promotions are the existing strict exception: absence yields null,
   but a present final value of the wrong declared type rejects that span.
   Keep the existing pinned keys, numeric validation, and no legacy fallbacks.
   The new resource and URL promotions do not inherit this strict policy.
6. Validate identifiers, time ranges, counts, Variant bounds, and signal-kind
   applicability before accepting a record. Do not truncate oversized data,
   coerce overflowing counts, reinterpret timestamp integers as seconds, or
   make invalid correlation null to obtain acceptance.

### Observation correlation versus execution ownership

`subject_card_uid` is the renamed stored `card_uid` column. It and `run_id`
retain their current record-level sources and grammar. In particular, the
canonical OTLP correlation attribute remains `wyrd.card_uid`; this column
rename does not introduce a second attribute spelling. Public native Arrow
schemas and SQL use only `subject_card_uid` in the fresh baseline.
Absent attributes yield null. A present wrong type or malformed value rejects
the record. A present Card UID must be an authorized observation-target UID.
It never becomes the root merely because that is the only supplied Card.

`workload_card_uid` is different: it is managed authorization context. For a
root-bound machine token, take its validated exact root. For an authorized
User/state request, validate the explicitly supplied root under that User's
grants and stamp the accepted context. A requested binding may establish its
exact owner only after the same binding/tenant/subject checks. With no proven
root, stamp null. Resource `service.*`, arbitrary owner attributes, reverse
graph membership, and `run_id` cannot establish it.

An immutable root-bound client can batch different components and Runs under
the same root. Different bound clients cannot share a token or buffered auth
context accidentally. A batch is not split by observation Card or Run. If
transport grouping is needed for distinct validated roots, preserve the
existing logical-record atomicity and idempotency contracts. Internal outbox
batches may carry different per-row roots and principal pairs only when each
row retains the originating verified context; they remain one tenant.

Generic telemetry can have `workload_card_uid` without `subject_card_uid`, or neither.
For an owned, Card-attributed row, the subject must belong to that exact
root's graph even when a privileged User could separately write unowned
evidence about that subject. This is truthful attribution, not a blanket
restriction on the User's operation grants.

Wyrd SDKs default service resource name/version from a loaded root and accept
explicit OTel resource configuration. Preserve actual supplied resource values;
do not silently rewrite them to make ownership appear consistent. Canonical
Service membership is always decided by the verified owner UID. Optional
`service_namespace` is not a replacement for removed Card space.

### Ingestion surfaces and trust

The same table-owned projection semantics govern OTLP, SDK-produced Arrow,
public native frames, and internal capture. Clients may compute columns for
efficient encoding, but the server must reproduce or validate derived values
against canonical source fields before acknowledgement. A forged promotion
cannot become an accepted filter value. Public input attempting to supply
managed owner or principal columns is refused, not trusted.

Keep transport-specific rejection semantics: OTLP reports existing exact
accepted/rejected signal counts and rejects the affected record or descendants
for a shared envelope defect. A malformed authentication/root context refuses
the request before data admission. Native Arrow/schema/managed-field mismatch
refuses the batch before ACK; a retry never acknowledges a partly changed
batch. Retain deterministic first-error reporting and existing bounded output.

The application does not gain authority by setting `service_name`, version,
namespace, environment, exception fields, or a metric attribute. Query table
permissions remain independent of labels and physical partitions.

## Filtering and physical layout

The authoritative layouts are the exact nine-table ledger, complete per-table
Bloom defaults, shared layout contract, and writer policy in
[table storage and correlation](table-storage-and-correlation.md#exact-system-default-layouts).
For these three signals, partitions remain hourly event time by default. The ledger specifies default
environment/Service-prefixed sort orders, exact null ordering, and Bloom
default sets. Authorized tenants may configure partitions and Blooms using the
same rules as dynamic tables; no dimension is automatically inferred.

Resource columns remain ordinary typed filters. Exact workload/subject IDs
remain first-class columns; the default layout does not partition by versions
or accounts. Authorized tenant overrides follow the shared validation rules.
Raw nested attributes remain queryable through existing Variant operators,
shredding, and nested-field pruning.

Service-family queries resolve registered root versions through Cards and
bind their UIDs into SQL, for example `workload_card_uid IN ($1, $2)`. An exact
release uses one UID. A general OTel query can independently filter
`service_name`, `service_namespace`, `service_version`, and environment.
Do not imply that Oracle joins directly to the Postgres account registry.

## Fresh baseline and acceptance detail

This is a v0.3.0 schema replacement with no historical data, field-ID, catalog,
or attribution migration requirement. Development tables may be recreated.
Use only `bifrost.*`, `subject_card_uid`, and `workload_card_uid` in the new
public contract; do not add legacy alias columns or dual catalog readers.
Current field names in the comparison tables describe the inspected code,
not an obligation to convert old data.

All new writes must produce the complete declared schema and truthful nullable
correlations. Hot, recovered, published, and compacted reads must agree.
Future runtime schema/layout evolution preserves accepted records, field IDs,
partition history, and idempotency as defined by the storage supplement; that
runtime capability is distinct from migrating a pre-change installation.

The owning acceptance criterion exercises every listed column, all nested
types, wrong/missing types, final duplicates, environment fallback, GenAI
strictness, exception selection, exact integers above 2^53, scope ownership,
managed-field spoofing, and hot/published/recovery/compaction equality. Inspect
actual partition manifests, sort order, pruning, and file sizes against the
resolved tenant layout. No measured performance gain is claimed.

## External grounding and deliberate differences

- [Logfire records columns](https://pydantic.dev/docs/logfire/reference/sql/#records-columns)
  inform service/resource, URL, and exception conveniences. Wyrd keeps existing
  field names except the explicit subject rename, and retains typed nested data;
  it does not reproduce Logfire-specific
  message/tags/level functions, pending spans, internal fields, or omission of
  OTel SpanKind.
- [Logfire metrics schema](https://pydantic.dev/docs/logfire/observe/explore/#metrics-schema)
  informs queryable resource dimensions while Wyrd retains exact integers,
  Summary, and Exemplar fields. Its scalar convenience is not a requirement
  to convert integer counters to floating point.
- [OTel service](https://opentelemetry.io/docs/specs/semconv/resource/service/),
  [environment](https://opentelemetry.io/docs/specs/semconv/resource/deployment-environment/),
  and [metric data model](https://opentelemetry.io/docs/specs/otel/metrics/data-model/)
  define signal meaning, not Wyrd's physical partitions or authorization.
- [Iceberg partitioning](https://iceberg.apache.org/docs/latest/partitioning/)
  permits physical evolution independently of ordinary SQL predicates.

## Revision history

| Spec revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Complete current/proposed schemas, source and null/type rules, managed ownership, production/trust boundaries, and proposed layout without cosmetic renames. |
| 2 | draft | Associates the unchanged telemetry schema with the full auth supplement and packet revision 2. |
| 3 | draft | Renames existing `card_uid` to `subject_card_uid` and names the new root column `workload_card_uid`; preserves field IDs, source attributes, values, and schema counts through coordinated cutover. |
| 4 | draft | Links the authoritative nine-table storage ledger, hourly telemetry partitions, environment/Service sort prefixes, expanded shared attribution/Bloom policy, and system/dynamic migration. Signal field counts remain unchanged. |
| 5 | draft | Uses the bifrost catalog and resettable v0.3.0 baseline, removes historical backfill/migration requirements, and permits authorized tenant partition/Bloom configuration under the shared rules. |
| 6 | draft | Uses configurable per-table Bloom defaults without a mandatory correlation-column floor; telemetry field schemas and partition proposals are unchanged. |
| 7 | draft | Aligns with explicit verification evidence-read optimization; signal schemas and partition/Bloom proposals are unchanged. |
| 8 | draft | Uses lean per-table Bloom defaults; span IDs remain stored/correlated but opt-in for indexing. Signal fields and partition/sort proposals are unchanged. |
| 9 | draft | Aligns with whole-trace verification; Card tags do not narrow trace evidence. Telemetry schemas and layouts are unchanged. |
| 10 | draft | Aligns packet revision with trace-readiness findings and agreed performance-validation approach; schemas and layouts are unchanged. |
| 11 | draft | Aligns with agreed hourly signal defaults and absent-trace retry policy; stored signal schemas and Bloom sets are unchanged. |
| 12 | approved | Aligns the packet revision with closed table/trace contracts; signal columns, extraction and layout defaults are unchanged. |

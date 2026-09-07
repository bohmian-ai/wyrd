---
id: BIFROST-OTEL-T01
title: Make the three canonical signal tables lossless and self-validating
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 7
depends_on: []
requirements: [REQ-001, REQ-006, REQ-007, REQ-008, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-018]
acceptance: [AC-001, AC-003, AC-004, AC-008, AC-011]
---

# Canonical signal schemas and table-owned projection

## Outcome and value

`vala.traces.spans`, `vala.logs.records`, and `vala.metrics.points` become the
only schema and semantic-mapping authorities for accepted OpenTelemetry data.
Each table can project the pinned decoded OTLP type, validate the equivalent
public Arrow form by field identity and name, and round-trip every supported
value without narrowing or silent loss.

Required execution skill: `$wyrd-implement`.

## Owners, scope, consumers, and non-goals

- `crates/vala/vala-bifrost-redux/src/tables/` owns the logical fields, stable
  Iceberg IDs, Arrow schema, sensitivity, fingerprint, validation, and pure
  projection for the three tables.
- Existing generated types in `wyrd-tonic` remain the pinned OTLP input. Reuse
  `prost::Message` and the installed Arrow/Parquet/Iceberg stack; add no
  dependency, feature, mapper trait, or schema DSL.
- Catalog creation, Parquet writing, Forge, Oracle, Gate, and Scribe consume the
  table-owned schemas. They do not copy field order or signal semantics.
- This task does not move the Gate/Scribe boundary, alter public query response
  types, delete the old Scribe projectors, or claim an end-to-end journey.

## Selected representation and schema authority

Extend `BuiltinTableDefinition` with its exact table-owned physical Arrow
schema, including positive `PARQUET:field_id` metadata on every field and
nested child. User-facing validation matches stable ID plus name, type,
nullability, and semantic metadata; it never aligns by position. Built-in
catalog creation uses the explicit IDs through
`iceberg::arrow::arrow_schema_to_schema`. User-defined tables retain their
current auto-assigned Iceberg IDs and catalog fingerprint contract; this change
does not redesign their registration or existing catalog rows.

IDs are table-local, immutable, and assigned by the ledgers below. New fields
take a previously unused ID and never renumber or reuse one. All three tables
append the Observation envelope with the same IDs and exact current types:
`1000 run_id Utf8?`, `1001 card_uid Utf8?`, `1002 principal_id Utf8`, `1003
wyrd_request_id Utf8`, `1004 wyrd_event_time Timestamp(Microsecond, UTC)`,
`1005 wyrd_ingested_at Timestamp(Microsecond, UTC)`, `1006 wyrd_batch_id
FixedSizeBinary(16)`, `1007 wyrd_row_ordinal Int32`, and `1008 data_tenant_id
Utf8`. `?` means nullable; every other listed field is non-null unless marked.
These envelope fields are physical schema authority but are stripped from
the canonical user batch and stamped from trusted Gate context in T02. The
writable Arrow projection exposes required `card_ref`, required-nullable
`run_id`, and optional-column `wyrd_event_time`; Gate extracts them before
table user-field validation. Every other correlation/managed physical field is
prohibited. Scribe window-validates and stamps a supplied event-time candidate
unchanged.

Replace the current fingerprint helper with versioned recursive bytes hashed by
SHA-256: byte `1`, top-level field count as big-endian `u32`, then each field in
declared order as `id:i32`, length-prefixed UTF-8 name, type bytes, nullability
byte, sensitivity byte, metadata count plus length-prefixed key/value pairs in
ascending raw UTF-8 key-byte order, followed depth-first by nested child count
and child records. All counts and UTF-8/byte lengths are unsigned big-endian
`u32`; field IDs and fixed-binary widths are signed big-endian `i32`;
nullability and sensitivity encode false=`00`, true=`01`. Include the full physical schema,
including envelope fields. Store this as a distinct
`CanonicalPhysicalFingerprint` on the three canonical built-in definitions and
use it in their Arrow validator, Gate validation proof, Parquet/Iceberg checks,
and recovery. `BifrostTableEntry.fingerprint` and dynamic-table catalog rows
retain their existing user-schema meaning. `ResolvedSchemaIdentity` contains
`catalog_fingerprint: SchemaFingerprint` for every table and
`canonical_physical_fingerprint: Option<CanonicalPhysicalFingerprint>` only for
a canonical built-in, so the two meanings cannot be confused.

Every integer/length is big-endian. Type bytes are exactly `01 Bool`, `02
Int32`, `03 Int64`, `04 UInt32`, `05 UInt64`, `06 Float64`, `07 Utf8`, `08
Binary`, `09 FixedSizeBinary` followed by signed `i32` width, `0a Timestamp`
followed by unit byte (`00` second, `01` millisecond, `02` microsecond, `03`
nanosecond), then timezone presence (`00` absent, `01` present) and, when
present, its `u32` byte length plus UTF-8 bytes; `0b List`; and `0c Struct`.
List and Struct parameters exist only in their following child records. These
tags cover every selected canonical and envelope type; adding another type
requires a new unused tag, never reinterpretation.

Use the existing Arrow types directly:

- OTLP `fixed64` identifiers are `FixedSizeBinary(16)` trace IDs and
  `FixedSizeBinary(8)` span IDs. Optional IDs are nullable, not zero-filled.
- Every OTLP `fixed64 time_unix_nano` is `UInt64`; duration is checked
  `end-start` in `UInt64`. This preserves the complete wire domain that an
  Arrow signed nanosecond timestamp cannot represent. The separate candidate
  `wyrd_event_time` remains `Timestamp(Microsecond, UTC)` and is not part of
  the signal's lossless timestamp value.
- Raw protocol discriminants and flags are `Int32`/`UInt32`; do not convert
  them to display strings while storing.
- One `AnyValue` is nullable `Binary` containing its deterministic pinned Prost
  encoding. An attribute collection or metric metadata is non-null `Binary`
  containing a pinned `KeyValueList` encoding. Null means absent; zero-length
  bytes mean a present empty protobuf value/list. Canonical Arrow validation
  decodes and re-encodes these bytes and rejects malformed or non-canonical
  encodings. This is the minimal Iceberg-compatible lossless representation;
  do not add an Arrow union or JSON copy.
- Repeated structural values use Arrow `List<Struct>` in input order. Their
  nested attributes use the same binary encoding. `resource_entity_refs` is a
  `List<Binary>` of canonical pinned `EntityRef` messages because it is an
  opaque repeated protocol value and has no query predicate.

The trace user schema ledger is:

```text
1 trace_id FixedSizeBinary(16)                       metadata
2 span_id FixedSizeBinary(8)                        metadata
3 parent_span_id FixedSizeBinary(8)?                metadata
4 trace_state Utf8                                  metadata
5 flags UInt32                                      metadata
6 name Utf8                                         metadata
7 kind Int32                                        metadata
8 start_time_unix_nano UInt64                       metadata
9 end_time_unix_nano UInt64                         metadata
10 duration_nano UInt64                             metadata
11 status_present Bool                              metadata
12 status_code Int32?                               metadata
13 status_message Utf8?                             payload
14 attributes Binary                               payload
15 dropped_attributes_count UInt32                  metadata
16 events List<17 event Struct>                     payload
  18 event.time_unix_nano UInt64                    payload
  19 event.name Utf8                                payload
  20 event.attributes Binary                        payload
  21 event.dropped_attributes_count UInt32          payload
22 dropped_events_count UInt32                      metadata
23 links List<24 link Struct>                       payload
  25 link.trace_id FixedSizeBinary(16)              payload
  26 link.span_id FixedSizeBinary(8)                payload
  27 link.trace_state Utf8                          payload
  28 link.flags UInt32                              payload
  29 link.attributes Binary                         payload
  30 link.dropped_attributes_count UInt32           payload
31 dropped_links_count UInt32                       metadata
32 resource_present Bool                            metadata
33 resource_attributes Binary                       payload
34 resource_dropped_attributes_count UInt32         metadata
35 resource_schema_url Utf8                         metadata
36 resource_entity_refs List<37 entity_ref Binary>  payload
38 scope_present Bool                               metadata
39 scope_name Utf8                                  metadata
40 scope_version Utf8                               metadata
41 scope_attributes Binary                          payload
42 scope_dropped_attributes_count UInt32            metadata
43 scope_schema_url Utf8                            metadata
44 service_name Utf8?                               metadata
45 gen_ai_operation_name Utf8?                      metadata
46 gen_ai_provider_name Utf8?                       metadata
47 gen_ai_request_model Utf8?                       metadata
48 gen_ai_conversation_id Utf8?                     metadata
49 gen_ai_usage_input_tokens Int64?                 metadata
50 gen_ai_usage_output_tokens Int64?                metadata
```

The list element fields 17, 24, and 37 are non-null. Presence booleans preserve
absent resource/scope/status versus a present empty message; their subordinate
scalars use canonical empty values when absent and are interpreted only when
the presence bit is true. Extract promotions only from the exact string or
signed integer source types named in revision 7; a wrong GenAI promoted type
rejects the span, while missing/non-string `service.name` remains null.

The log user schema ledger is:

```text
1 time_unix_nano UInt64                             metadata
2 observed_time_unix_nano UInt64                    metadata
3 severity_number Int32                             metadata
4 severity_text Utf8                                metadata
5 event_name Utf8?                                  metadata
6 body Binary?                                      payload
7 trace_id FixedSizeBinary(16)?                     metadata
8 span_id FixedSizeBinary(8)?                       metadata
9 flags UInt32                                      metadata
10 attributes Binary                                payload
11 dropped_attributes_count UInt32                  metadata
12 resource_present Bool                            metadata
13 resource_attributes Binary                       payload
14 resource_dropped_attributes_count UInt32         metadata
15 resource_schema_url Utf8                         metadata
16 resource_entity_refs List<17 entity_ref Binary>  payload
18 scope_present Bool                               metadata
19 scope_name Utf8                                  metadata
20 scope_version Utf8                               metadata
21 scope_attributes Binary                          payload
22 scope_dropped_attributes_count UInt32            metadata
23 scope_schema_url Utf8                            metadata
```

The metric user schema ledger is:

```text
1 metric_name Utf8                                  metadata
2 description Utf8                                 metadata
3 unit Utf8                                        metadata
4 metadata Binary                                  payload
5 metric_type Utf8                                 metadata
6 time_unix_nano UInt64                            metadata
7 start_time_unix_nano UInt64                      metadata
8 flags UInt32                                     metadata
9 attributes Binary                                payload
10 int_value Int64?                                metadata
11 double_value Float64?                           metadata
12 aggregation_temporality Int32?                  metadata
13 is_monotonic Bool?                              metadata
14 histogram_count UInt64?                         metadata
15 histogram_sum Float64?                          metadata
16 histogram_min Float64?                          metadata
17 histogram_max Float64?                          metadata
18 bucket_counts List<19 item UInt64>?              payload
20 explicit_bounds List<21 item Float64>?           payload
22 exponential_scale Int32?                        metadata
23 exponential_zero_count UInt64?                  metadata
24 exponential_zero_threshold Float64?             metadata
25 positive_buckets Struct?                        payload
  26 positive_buckets.offset Int32                  payload
  27 positive_buckets.bucket_counts List<28 item UInt64> payload
29 negative_buckets Struct?                        payload
  30 negative_buckets.offset Int32                  payload
  31 negative_buckets.bucket_counts List<32 item UInt64> payload
33 summary_count UInt64?                           metadata
34 summary_sum Float64?                            metadata
35 quantile_values List<36 quantile_value Struct>? payload
  37 quantile_value.quantile Float64                payload
  38 quantile_value.value Float64                   payload
39 exemplars List<40 exemplar Struct>               payload
  41 exemplar.time_unix_nano UInt64                 payload
  42 exemplar.int_value Int64?                      payload
  43 exemplar.double_value Float64?                 payload
  44 exemplar.filtered_attributes Binary            payload
  45 exemplar.trace_id FixedSizeBinary(16)?         payload
  46 exemplar.span_id FixedSizeBinary(8)?           payload
47 resource_present Bool                            metadata
48 resource_attributes Binary                       payload
49 resource_dropped_attributes_count UInt32         metadata
50 resource_schema_url Utf8                         metadata
51 resource_entity_refs List<52 entity_ref Binary>  payload
53 scope_present Bool                               metadata
54 scope_name Utf8                                  metadata
55 scope_version Utf8                               metadata
56 scope_attributes Binary                          payload
57 scope_dropped_attributes_count UInt32            metadata
58 scope_schema_url Utf8                            metadata
```

`metric_type` is exactly one of `gauge`, `sum`, `histogram`,
`exponential_histogram`, or `summary`. Kind-specific columns are null for other
kinds and the validator fixes the required/null shape for each kind. Reject
absent oneofs, both/zero numeric alternatives, count/bucket inconsistencies,
and every shape the selected metric kind cannot own. Preserve IEEE bits for
permitted doubles; reject only protocol/schema-invalid values, not NaN or
infinity merely because they are unusual.

Mark span attributes/events/links/resource/scope payloads, log body and
attribute payloads, and metric attribute/metadata/exemplar payloads sensitive
in the table definition. Promotions are non-sensitive only where the existing
typed metadata/query permission permits them; no identifier becomes a metric
label.

## Ordered implementation scenarios

### Scenario 1 — A maximal span is projected once and mapped by stable identity

**Behavior.** A maximal pinned span, including absent-versus-empty protobuf
values, byte-bearing nested attributes, ordered events/links, raw enum values,
resource entity refs, status description, and approved promotions, becomes one
exact canonical row. Wrong GenAI scalar types reject the complete span. Maps
REQ-001, REQ-006–REQ-008, REQ-014–REQ-016, INV-001–INV-005, INV-009, AC-001,
and AC-008.

**RED.** Add
`tables::traces::tests::maximal_span_projection_is_lossless_and_identity_mapped`
and
`tables::traces::tests::wrong_genai_promotion_type_rejects_the_complete_span`.
Assert the complete nested arrays and canonical bytes, input order, exact
nanoseconds/raw discriminants, null/empty distinctions, promotions, stable IDs,
sensitivity, and name-based validation after deliberately permuting an Arrow
input schema. The current flat schema/projector cannot pass.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::traces::tests::maximal_span_projection_is_lossless_and_identity_mapped)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::traces::tests::wrong_genai_promotion_type_rejects_the_complete_span)'
```

**GREEN.** Put a cohesive, state-free `project_resource_spans` and canonical
Arrow validator beside `SpansTable`; return the accepted batch plus the
existing `IngestOutcome`, not a new outcome contract. Share only narrow binary
encode/decode and resource/scope helpers within `tables`, then derive the table
definition and physical schema from the same declared fields.

**REFACTOR.** One table declaration feeds schema, projection, validation,
fingerprint, Iceberg IDs, and sensitivity. No caller indexes a column by a
separately copied ordinal.

### Scenario 2 — Logs preserve body, event identity, and context exactly

**Behavior.** A maximal log remains a log record and retains all body,
attribute, correlation, resource, and scope values. Maps REQ-010, REQ-011,
REQ-013–REQ-014, INV-002, INV-004, INV-006, INV-009, AC-003, and AC-008.

**RED.** Add
`tables::logs::tests::maximal_log_projection_preserves_body_context_and_presence`.
Assert exact canonical bytes for every `AnyValue` form, event name, raw flags,
nanoseconds, optional IDs, and resource/scope presence. Assert malformed
canonical Arrow bytes and schema identity drift fail without a row.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::logs::tests::maximal_log_projection_preserves_body_context_and_presence)'
```

**GREEN.** Add the pure log projector and validator beside `LogsTable`, using
the common table-owned binary/resource/scope helpers and `LogsOutcome`.

**REFACTOR.** Do not translate logs to span events or create a GenAI path.

### Scenario 3 — Every metric point kind retains its exact value shape

**Behavior.** Integer/double gauge and sum points, histogram, exponential
histogram, summary, metadata, and exemplars round-trip without normalization.
Invalid shapes reject only their complete point. Maps REQ-012, REQ-013,
REQ-018, INV-002, INV-004, INV-009, AC-004, and AC-008.

**RED.** Add
`tables::metrics::tests::all_pinned_metric_point_kinds_project_without_narrowing`
and
`tables::metrics::tests::inconsistent_metric_shapes_are_record_rejections`.
Use `i64::MAX`, signed values, IEEE infinities/NaN bit checks where the protocol
permits them, ordered buckets/quantiles/exemplars, metadata, and nested
attributes. Assert exact accepted/rejected counts and stable first reason.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::metrics::tests::all_pinned_metric_point_kinds_project_without_narrowing)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::metrics::tests::inconsistent_metric_shapes_are_record_rejections)'
```

**GREEN.** Add the pure metric projector/validator beside `MetricsTable`.
Reuse the current material/count validation facts where they are semantic and
move them from Scribe only in T02; do not introduce a metric object hierarchy.

**REFACTOR.** Keep one wide point schema because the public contract requires
one physical table; nullable kind-specific columns must be validated as a
closed shape.

### Scenario 4 — Arrow, IPC, Parquet, and Iceberg agree on nested schemas

**Behavior.** The three definitions retain exact IDs, nullability, metadata,
values, and fingerprint across the installed storage stack. Maps REQ-001,
INV-003, AC-008, and AC-011.

**RED.** Extend the existing table test owner with
`tables::tests::canonical_signal_schemas_round_trip_arrow_parquet_and_iceberg`.
Encode one maximal batch per table through existing Arrow IPC and Parquet
helpers, read it back, convert the explicit-ID schema to Iceberg and back, and
assert recursively by field identity/name rather than index. It fails on the
flat schemas, auto IDs, and timestamp narrowing.

Add `tables::tests::canonical_physical_fingerprint_bytes_are_stable` over one
small nested schema containing nullable/sensitive fields, absent and present
empty timezone forms, and metadata inserted out of order. Assert the complete
pre-hash encoded-byte hex and its exact 64-character lowercase SHA-256 golden.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::canonical_signal_schemas_round_trip_arrow_parquet_and_iceberg)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::canonical_physical_fingerprint_bytes_are_stable)'
```

**GREEN.** Make canonical built-in catalog creation and Parquet schema
projection consume the definition's explicit physical schema. Keep existing
user-table auto-ID/fingerprint behavior. Do not change the dependency cone.

**REFACTOR.** The table definition is the only recursive schema walk that
assigns IDs; storage consumers only verify or project it.

## Expected write set and consumer closure

- `crates/vala/vala-bifrost-redux/src/tables/{mod,fields,managed_columns}.rs`
- `crates/vala/vala-bifrost-redux/src/tables/{traces,logs,metrics}.rs` and small
  table-local projection modules if required for file size
- `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs`
- `crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs`

No public wire contract, server route, SDK, generated artifact, migration, new
test target, or dependency belongs in this task.

## Verification and evidence

Run every named command, then:

```bash
mise run fmt
mise run lints
git diff --check
```

Record RED/GREEN results, the recursive schema/value round-trip, exact rejected
records, and confirmation that no table-specific schema copy entered a
consumer.

## Material stop conditions

- The pinned Iceberg/Parquet cone cannot represent a selected nested type or
  retain explicit field IDs.
- The pinned generated OTLP type exposes a presence/value distinction that the
  selected schema cannot encode.
- Correct event-time derivation would require narrowing an otherwise accepted
  OTLP timestamp instead of rejecting it explicitly.

These are specification/capability conflicts, not reasons to add JSON, another
table, a dependency, or a positional fallback.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `architecture/references/domain/iceberg.md`
- `architecture/references/languages/rust-core.md`
- `architecture/references/languages/testing-workflows.md`
- `changes/active/bifrost-canonical-otel-signals/spec.md`

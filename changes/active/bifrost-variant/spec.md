---
id: SPEC-bifrost-variant
revision: 2
status: draft
---

# Queryable open-shaped data in Bifrost

## Human intent and user value

Everything stored in Bifrost must be queryable the way it is in any OLAP
warehouse. Today OpenTelemetry attributes, log bodies, and several built-in
JSON payloads are stored as opaque protobuf bytes or JSON text that SQL cannot
look inside, and a data scientist who puts a JSON payload in a column of their
own table is refused or silently loses keys.

Bifrost shall adopt Pydantic Logfire's practices, except where Wyrd already has
a faster layout:

- known shapes are stored as typed Struct columns;
- open shapes (OTel attributes, log bodies, user JSON payloads) are stored as
  Parquet/Iceberg Variant columns that keep each value's type;
- the heaviest Variant keys of each file are shredded into typed leaf columns
  when the file is written and again when it is compacted;
- users query open data with Logfire's `->` and `->>` operators;
- a query on a shredded key or a Struct field reads only that leaf, filters
  while decoding, and skips row groups by the leaf's statistics;
- common semantic-convention fields are promoted to typed columns; and
- Bloom filters are sized to the data.

Bifrost has not shipped. No existing table, file, or data is migrated.

## Current repository facts

Paths are relative to the repository root unless prefixed. `B/` is
`crates/vala/vala-bifrost-redux/src/`, `T/` is `B/tables/`, `ICE/` is the
pinned iceberg-rust fork (rev `97c32f6`), and `CC/` is the pinned
iceberg-compaction fork (rev `380a4d0`). These facts describe current
implementation, not desired behavior.

Storage and write path:

- Every Bifrost table is created as Iceberg format v2
  (`B/catalog/bifrost_catalog.rs:1045`); `validate_physical_table` checks no
  format version. Forge garbage collection skips v3 tables (`B/forge/gc.rs:247`).
  Bifrost writes only Iceberg data files, never delete files.
- Appends set `first_row_id`, but compaction-core does not carry the v3
  row-lineage columns (`_row_id`, `_last_updated_sequence_number`) through a
  rewrite (`ICE/metadata_columns.rs:63`, `ICE/arrow/reader/row_lineage.rs`).
- Scribe encodes staged runs with `encode_batch` (`B/scribe/parquet_writer.rs:240`)
  and streams claimed merges with `encode_ordered_claim` (`:303`); the artifact
  schema is fixed when the artifact opens (`:558`). Staged runs whose exact
  fingerprint differs are refused by the merge (`B/scribe/claim_merge.rs:242`).
- Forge compaction writes through compaction-core's `ParquetWriterBuilder`
  (`CC/executor/datafusion/mod.rs:428`) with the table schema.
- The fork maps the Arrow Variant extension to Iceberg Variant
  (`ICE/arrow/schema.rs:578`), maps Iceberg Variant back only as unshredded
  (`:770`), and refuses shredded files at read
  (`ICE/arrow/reader/projection.rs:197`).
- Both schema fingerprints would change if shredded leaves were part of the
  file schema (`B/fingerprint.rs:49`); footer identity uses the exact one.
- Bloom filter capacity is sized from the opening batch (at most 8 Ki rows,
  giving about 1 000 distinct values) in Scribe and 10 485 in Forge, although
  row groups hold far more rows. Parquet 59.3 folds Bloom filters on write.

Schemas:

- OTel attribute collections are encoded as protobuf `KeyValueList` bytes by
  `encode_attributes` (`T/signal.rs:49`) and log bodies as an encoded
  `AnyValue` by `encode_any_value` (`:68`), declared as Binary in
  `T/traces/spans.rs:28,41,76,83,94`, `T/logs/records.rs:35,39,42,53`, and
  `T/metrics/points.rs:72,92,97,118,129`. `resource_entity_refs` is a list of
  encoded binaries.
- `CanonicalType` (`T/fields.rs:102`) has no Variant; the fingerprint encoder
  (`T/mod.rs:705`) assigns tags `0x01`–`0x0c`.
- JSON text columns: `vala.verification.results.details`,
  `vala.eval.observations.context` and `.media`,
  `vala.eval.result_items.actual` and `.expected`,
  `vala.gateway.calls.request_payload_json` and `.response_payload_json`,
  `vala.dev.agent_traces.messages` and `.tool_io`, and
  `vala.system.audit_log.detail`.
- Logs and metrics have no promoted `service_name`.

User-defined tables:

- `DataTypeSpec` (`crates/wyrd-spec/src/vala/api.rs:194`) has no Variant,
  JSON, or Map type.
- The one JSON Schema mapper (`crates/shared/wyrd-queue/src/schema.rs`) refuses
  free-form objects (`dict[str, Any]`), keeps only the first non-null branch of
  `anyOf`, refuses `oneOf` and `allOf`, and ignores `additionalProperties`.
- The row builder (`crates/shared/wyrd-queue/src/batch_builder.rs`) silently
  drops keys the schema does not declare and cannot build List, Struct, Binary,
  Decimal, Date64, Time, or non-microsecond timestamp columns, so `insert` into
  a table registered from a nested model always fails.
- The fork refuses UInt64, Date64, Time32, second and millisecond timestamps,
  and non-UTC time zones when a table is created (`ICE/arrow/schema.rs:519`),
  although the SDK documentation presents some of them as supported.
- No JSON or Variant SQL functions are registered.

Query path:

- Oracle reads published files through the fork's `ArrowReader`
  (`OracleIcebergScanExec`) and hot files through `HotParquetExec`, not
  DataFusion's `ParquetSource`.
- `classify_filter` (`B/oracle/exec.rs:3331`) pushes down only flat
  column-versus-literal predicates; the fork's `expr_to_predicate` handles only
  plain column references.
- Both readers project whole top-level columns (`ProjectionMask::roots`,
  `B/oracle/exec.rs:3312`; root-id expansion in `ICE/arrow/reader/projection.rs`).
- The hot reader matches a Parquet column by the last segment of its name only
  (`B/oracle/exec.rs:3971`), so a Struct field named like a top-level column is
  matched to the wrong column.
- Bloom filters are probed at read time on both paths for `=` on binary,
  fixed-length, and 64-bit integer columns, but not for `IN` lists on the hot
  path.
- Production query sessions are built at `B/oracle/resources.rs:4245`,
  `B/oracle/mod.rs:3097`, `B/oracle/analytical.rs:7682`, and the distributed
  worker at `B/oracle/analytical.rs:492`; none registers custom functions or an
  expression planner. DataFusion already parses `->` and `->>` to
  `Operator::Arrow` and `Operator::LongArrow`. The follower plan codec decodes
  functions from the session registry.
- The signed follower scan assignment carries pushed-down predicates as
  `ScanPredicate` (`crates/wyrd-spec/src/vala/assignment_authority.rs:77`) with a
  digest over them.
- DataFusion 55 cannot skip row groups by nested-leaf statistics (upstream
  `apache/datafusion#20871` is open).

## Scope

- Iceberg format v3 for every Bifrost table, with row lineage preserved through
  compaction.
- A Variant column type across the canonical schema model, fingerprints, wire
  contracts, and all three SDKs.
- The built-in table schemas listed in REQ-006 to REQ-011.
- Struct and Variant columns in user-defined tables, including row `insert`.
- Variant SQL operators and functions in every Oracle query session.
- Per-file Variant shredding in Scribe and Forge.
- Bifrost-owned leaf projection, decode-time filtering, and row-group and page
  pruning on Struct fields and shredded Variant keys on both read paths.
- Bloom filter sizing and `IN`-list probing.
- Updating `architecture/bifrost-design.md` to describe the result.

## Non-goals

- Migrating any existing table, file, or data.
- Moving the published read path to DataFusion's `ParquetSource`.
- Bloom filters on shredded Variant leaves; leaves are pruned by statistics
  and page indexes only.
- Shredding arrays, values reached through an array, or Variant values nested
  inside lists (span event and link attributes, exemplar attributes). They are
  stored and queried correctly from the residual value.
- Inferring JSON from text: a string column holding JSON text stays text.
- A configurable shredding key count.
- A TypeScript `TableConfig.fromArrow`.
- Upstreaming changes to DataFusion, iceberg-rust, or compaction-core; fork
  changes stay on the pinned forks.
- Splitting `vala.metrics.points` into per-kind tables.

## Definitions

- **Struct column:** a column whose field names and types are fixed when the
  table is created. Each field is its own Parquet leaf.
- **Variant column:** a column holding self-describing semi-structured values
  in the Parquet/Iceberg Variant encoding: null, boolean, integer, floating
  point, decimal, string, binary, date, timestamp, array, and object. On the
  Arrow wire it is the canonical `arrow.parquet.variant` extension type.
- **Path:** the sequence of object keys from a Variant column's root to a
  value at any depth, for example `order` → `customer` → `tier`. Each element
  is one whole key, so the flat OTel key `http.route` is a one-element path,
  distinct from the two-element path `http` → `route`.
- **Shredding:** storing the scalar values at selected paths of a Variant
  column in one file as typed Parquet leaf columns (`typed_value`, nested as
  groups for nested objects), with every value that
  is not shredded, or does not match the shredded type, kept in the residual
  Variant value. Shredding is a per-file physical layout and never changes the
  logical value.
- **Logical schema:** the table's schema with Variant columns unshredded. It is
  the schema users see and the one fingerprints cover.
- **Leaf:** one Parquet column chunk path: a Struct field at any depth or a
  shredded Variant path.
- **Promoted column:** a nullable typed column copied from a canonical
  attribute; the attribute itself stays in its Variant column.
- **Hot path / published path:** Oracle's read of Scribe-owned hot Parquet
  files and of Iceberg-published files.

## Required behavior

### Storage format

#### REQ-001 — Iceberg v3

Every Bifrost table, built-in and user-defined, is created as Iceberg format
v3. Physical-table validation refuses a table that is not v3. Forge
maintenance, including garbage collection, operates on v3 tables.

#### REQ-002 — Row lineage survives compaction

Appended files assign row ids through the v3 first-row-id mechanism. A Forge
rewrite preserves each surviving row's `_row_id` and
`_last_updated_sequence_number`, so a row keeps the same lineage identity
across any number of compactions.

#### REQ-003 — Variant column type

Variant is a canonical Bifrost column type. It round-trips through the
canonical schema model, Arrow, Iceberg, and Parquet; it has its own stable
fingerprint encoding; and it is expressible on the public table wire contract
(`DataTypeSpec`) and in the Rust, Python, and TypeScript SDK type surfaces.
The fingerprints of a table cover its logical schema only, so files with
different shredding layouts belong to the same table and the same fingerprint.

#### REQ-004 — Variant values keep their meaning

A value written to a Variant column reads back with the same type, nesting,
and value:

- integers stay integers and are never converted through floating point;
- floating-point numbers keep their IEEE meaning;
- strings, booleans, bytes, arrays, and objects keep their type;
- an object key that is absent reads as missing, distinct from a key whose
  value is null.

Object keys in a Variant value are unique. When an OTel attribute collection
repeats a key, the final occurrence is stored, matching the OTel data model
and the existing correlation rule. This supersedes the "earlier duplicate keys
remain in the lossless attribute payload" clause of
`SPEC-bifrost-canonical-otel-signals` and `architecture/bifrost-design.md`.

JSON input converts as follows: an integer within the 64-bit signed range is
an integer; another integer that fits a Variant decimal is a decimal; a
non-integer number is a double; an integer that fits no Variant numeric type is
refused (REQ-019).

#### REQ-005 — Bloom filters sized to the data

Every Bloom column is written with capacity for the configured maximum
row-group row count, in Scribe and in Forge, and Parquet folds each filter to
the distinct values the row group actually holds. False-positive rate
configuration is unchanged.

### Built-in tables

#### REQ-006 — Spans

In `vala.traces.spans`:

- `attributes`, `resource_attributes`, and `scope_attributes` are Variant;
- the attributes of each span event and each span link are Variant (inside
  their lists, never shredded);
- `resource_entity_refs` is a list of Structs with the OTel entity-reference
  fields (type, ID keys, description keys, schema URL);
- trace, span, and parent span ids stay fixed-size binary;
- the existing promoted columns (`service_name`, `gen_ai_*`) stay, and these
  nullable promoted columns are added, each copied from its canonical source
  and null when the source is absent or has another type:

| Column | Type | Source |
|---|---|---|
| `service_version` | Utf8 | resource `service.version` |
| `deployment_environment` | Utf8 | resource `deployment.environment.name`, else `deployment.environment` |
| `http_request_method` | Utf8 | span `http.request.method` |
| `http_route` | Utf8 | span `http.route` |
| `http_response_status_code` | Int64 | span `http.response.status_code` |
| `url_full` | Utf8 | span `url.full` |
| `exception_type` | Utf8 | `exception.type` on the span's last event named `exception` |
| `exception_message` | Utf8 | `exception.message` on that event |
| `exception_stacktrace` | Utf8 | `exception.stacktrace` on that event |

Instrumentation scope name and version already exist as `scope_name` and
`scope_version` and are not duplicated.

#### REQ-007 — Logs

In `vala.logs.records`:

- `body`, `attributes`, `resource_attributes`, and `scope_attributes` are
  Variant, and `resource_entity_refs` is as in REQ-006;
- these nullable promoted columns are added: `service_name`,
  `service_version`, and `deployment_environment` from the resource (as in
  REQ-006); `exception_type`, `exception_message`, and `exception_stacktrace`
  from the record's own attributes; and `body_text` (Utf8), which equals the
  body when the body is a string and is null otherwise.

#### REQ-008 — Metrics

`vala.metrics.points` remains one table for all five metric kinds, told apart
by `metric_type`. `metadata`, `attributes`, `resource_attributes`,
`scope_attributes`, and each exemplar's `filtered_attributes` are Variant;
`resource_entity_refs` is as in REQ-006; and nullable `service_name`,
`service_version`, and `deployment_environment` are promoted from the
resource as in REQ-006.

#### REQ-009 — Verification results

In `vala.verification.results`, `details` is replaced by two nullable Struct
columns: `drift_report`, typed from the `DriftReport` contract, and
`eval_summary`, typed from the `EvalWorkflowSummary` contract. A member of
either contract whose keys are not fixed is a Variant field inside the Struct.
A scored Drift result sets only `drift_report`; a scored Eval result sets only
`eval_summary`; a completed Drift run that could not score valid input sets
neither. Both columns keep the sensitive classification `details` had.

#### REQ-010 — Other JSON payload columns

These columns become Variant, keeping their nullability and sensitive
classification:

- `vala.eval.observations`: `context` and `media`;
- `vala.eval.result_items`: `actual` and `expected`;
- `vala.gateway.calls`: `request_payload` and `response_payload` (renamed from
  `request_payload_json` and `response_payload_json`);
- `vala.dev.agent_traces`: `messages` and `tool_io`;
- `vala.system.audit_log`: `detail`.

Audit entry hashes keep their existing inputs (computed from the canonical
event before storage), and a stored `detail` decodes to the same JSON value
that was hashed.

#### REQ-011 — OTLP and canonical Arrow writes stay equivalent

OTLP ingest projects attribute collections and bodies directly into Variant.
The canonical Arrow write path accepts the same columns as the Arrow Variant
extension. Equivalent OTLP and canonical Arrow input still produces the same
logical rows.

### User-defined tables

#### REQ-012 — Declaring a Variant column

A user declares a Variant column with the ordinary open-data type of their
language; no Bifrost-specific schema syntax is needed:

| Declaration | Column |
|---|---|
| Pydantic `dict[str, Any]`, `dict[str, T]`, `Any`, `JsonValue`, `Json[...]` | Variant |
| Zod `z.record(...)`, `z.unknown()`, `z.any()`, `z.json()` | Variant |
| Rust `serde_json::Value`, `HashMap<String, T>` (via `schemars`) | Variant |
| Arrow field with the `arrow.parquet.variant` extension | Variant |
| `DataTypeSpec` Variant, or `"variant"` in the Python and TypeScript type unions | Variant |
| Nested model or object with fixed properties | Struct |
| Typed array | List of the item type |

In JSON Schema terms: an empty or always-true schema, an object with no
`properties`, a string with `contentMediaType: application/json`, and a union
(`anyOf` or `oneOf`) with more than one non-null branch each map to Variant. A
union of exactly one type and null maps to that type, nullable. An `allOf`
with one member maps to that member. An array without `items` maps to Variant.

#### REQ-013 — Inserting nested and open data

Row `insert` in every SDK writes Struct, List, and Variant columns. A dict,
object, list, or scalar given for a Variant column is stored with its JSON
types (REQ-004). A string given for a Variant column is stored as a Variant
string; it is not parsed as JSON.

#### REQ-014 — Arrow writes to Variant columns

`write_batch` in every SDK accepts, for a declared Variant column, either the
Arrow Variant extension or a Utf8/LargeUtf8 column of JSON text, which the
shared client converts to Variant before sending. The server wire contract
accepts only the Variant extension for Variant columns.

#### REQ-015 — No silent key loss

A row key that the table does not declare is refused with
`WYRD_VALA_400_BIFROST_UNDECLARED_FIELD`, naming the key, before the row is
queued. Registering a model that allows extra keys alongside declared
properties (`additionalProperties` with `properties`) is refused with
`WYRD_VALA_400_SCHEMA_PARSE`, whose remediation says to declare a Variant
field for open data.

#### REQ-016 — Unsupported types refused up front

Registering a table with a type Iceberg cannot store (UInt64, Date64, Time32,
second- or millisecond-precision timestamps, a timestamp with a non-UTC zone,
or any other type the canonical schema model does not support) is refused with
`WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`, naming the field and type, at the SDK
before any request and again at the server. SDK documentation lists exactly
the supported types.

### Query

#### REQ-017 — Operators and functions

Every Oracle query session (leader planning, admission execution, follower,
analytical leader, analytical planning, and distributed worker) provides:

- `v -> 'key'` and `v -> n`: the Variant value at an object key or array
  index, or null when absent;
- `v ->> 'key'` and `v ->> n`: the same value as text: a string as itself,
  any other non-null value as its JSON text, and SQL null when absent or null;
- chaining, so `attributes -> 'http' ->> 'route'` follows nested keys;
- `parse_json(text)`: the Variant for JSON text, raising an error on invalid
  JSON;
- `try_parse_json(text)`: the same, returning null on invalid JSON;
- `to_json(v)`: the JSON text of a Variant value;
- casts of `->>` results to numeric, boolean, and timestamp types with
  ordinary SQL cast semantics; and
- Struct field access with DataFusion's existing `s['field']` syntax.

`->` and `->>` apply to Variant columns. A JSON text column is queried with
`parse_json(column) ->> 'key'`.

#### REQ-018 — Query results

A query result carries a Variant column as the Arrow Variant extension.
Typed row terminals decode it into the language's native open value: Python
`sql(query, model)` into `dict`, `list`, and scalars; TypeScript
`sql(query, rows)` into plain objects, arrays, and scalars, with 64-bit
integers as `bigint`; Rust `sql_as` into `serde_json::Value`. Arrow terminals
(`to_arrow`, `toArrow`, Rust `QueryResult`) return the extension unchanged.
Any surface that renders rows as JSON, including MCP and HTTP JSON results,
renders a Variant as its JSON value.

#### REQ-019 — Stable failures

Invalid JSON in `parse_json`, a numeric value no Variant type can hold, a
Variant nested beyond the configured depth limit, and a Variant exceeding the
configured size limit fail with stable catalog codes before acknowledgement
(writes) or as a query error (queries). They are never truncated or stored
partially.

### Shredding

#### REQ-020 — Per-file layout

For each top-level Variant column of each file Scribe or Forge writes, the
writer considers every path that reaches a scalar value through objects only,
at any depth up to the configured Variant depth limit, and shreds at most 128
of them: those with the largest total value size in that file's input, ties
broken by path. A nested path is shredded as nested typed groups, so
`order` → `customer` → `tier` becomes its own leaf. A path's shredded type is
the type of its values in that input, widened from integer to double when both
occur. A path whose scalar types conflict beyond that widening is not
shredded. Values that do not match a shredded path's type, and every value at
an unshredded path, stay in the residual value.

#### REQ-021 — Choosing the layout without a second pass

Scribe's staged runs are written unshredded and record, for each Variant
column, a bounded summary of total value size per path. When Scribe assembles
a published artifact it combines its runs' summaries to choose the layout
before writing. Every file Scribe or Forge writes records its per-path size
summary in its Parquet footer, and Forge chooses a compaction output's layout
from its input files' summaries. The layout chosen for the same inputs is
always the same.

#### REQ-022 — Shredded files are read correctly everywhere

Both read paths read shredded files. For every query, the result is identical
whether a path is shredded, partly shredded, or unshredded, at any depth; whether the file is
hot or published; and before and after compaction.

### Pruning

#### REQ-023 — Leaf projection

A query that reads a Struct field or a shredded Variant path, at any depth,
reads only that leaf's column chunks on both read paths, never the whole
Struct or Variant column. An unshredded path reads the residual value.

#### REQ-024 — Leaf filtering and row-group skipping

On both read paths, Bifrost pushes these predicates down to a Struct field or
shredded Variant path, at any depth:

- `=`, `<>`, `<`, `<=`, `>`, `>=`, `IN`, `IS NULL`, and `IS NOT NULL`
  comparing a literal with `v ->> 'k'` or a chain such as
  `v -> 'a' -> 'b' ->> 'k'` (string-shredded paths), `CAST(<that> AS t)`
  (paths shredded as a matching type), or `s['a']['b']`.

Pushed-down predicates filter rows while decoding and skip row groups and
pages whose leaf statistics or page indexes exclude them. In a file where the
path is not shredded, the predicate is evaluated after reading, with the same
result. Any other predicate shape is evaluated by the query engine, with the
same result.

#### REQ-025 — Distributed scans carry leaf predicates

The signed follower scan assignment carries leaf predicates (a column plus a
Struct field or Variant key path, an operator, and literals), covered by the
assignment digest. The wire and digest format change in place.

#### REQ-026 — Exact column matching

The hot read path matches Parquet columns by their full path, so a Struct
field or shredded leaf is never confused with a top-level column of the same
name.

#### REQ-027 — Bloom `IN` lists

The hot read path probes Bloom filters for `IN` lists as it does for `=`.

## Invariants and prohibited outcomes

#### INV-001 — No opaque payloads

No built-in Bifrost column stores protobuf-encoded bytes or JSON text for data
that has a field structure. Every value in a Bifrost table is reachable from
SQL.

#### INV-002 — No silent field loss

No written field, attribute, body, or user key is dropped, narrowed, or
retyped. A value Bifrost cannot store is refused with a stable code.

#### INV-003 — Shredding is invisible to results

Shredding layouts, their per-file differences, and compaction never change
query results, schema fingerprints, or the logical schema users see.

#### INV-004 — Sensitive data stays gated

A Variant or Struct column classified sensitive stays sensitive in all of its
leaves. Reading any part of it, including through `->`, `->>`, `to_json`, a
Struct field, or a shredded leaf, requires the same permission as reading the
whole column.

#### INV-005 — Promoted values equal their source

A promoted column always equals its canonical source value as defined in
REQ-006 to REQ-008, or is null.

#### INV-006 — Tenant isolation is unchanged

Every path keeps the existing tenant derivation and the physical table, file,
footer, Scribe, Oracle, and plan-root tripwires.

#### INV-007 — Bounded processing

Variant conversion, key summaries, and shredding layout selection are bounded
in nesting depth, value size, and tracked keys before allocation or durable
mutation.

#### INV-008 — One reader per path

The change extends the existing Oracle hot reader and the fork's Iceberg
reader through one shared Bifrost leaf resolver and pruner. No parallel reader,
harness, or ingest path is introduced.

## Externally observable behavior

- An OTLP client exports a span; in every SDK,
  `SELECT ... FROM vala.traces.spans WHERE attributes ->> 'customer.tier' = 'gold'`
  finds it, `(attributes ->> 'retries')::bigint` reads an integer attribute as
  an integer, and an absent key reads as null.
- A data scientist registers a Pydantic model with a `payload: dict[str, Any]`
  field, inserts rows with nested JSON payloads, and queries
  `payload -> 'order' ->> 'id'`. The same works with Zod and Rust.
- A row with a key the table does not declare is refused before it is queued.
- A query on a shredded key or a Struct field reads only that leaf and skips
  row groups its statistics exclude.
- Verification results are queried by field: `drift_report['method']`.

## Material constraints

- Server behavior stays in Rust; `wyrd-spec` remains IO-free and PyO3-free.
- Existing Arrow 59, Parquet 59.3, DataFusion 55, and the pinned iceberg-rust
  and compaction-core forks remain the dependency set. Variant support comes
  from `parquet-variant`, `parquet-variant-compute`, and `parquet-variant-json`,
  already in the lockfile through the iceberg fork. `datafusion-variant` is not
  adopted.
- Fork changes (shredded Variant read, Variant path predicates through the
  metrics, Bloom, page-index, and row-filter evaluators, nested Struct field
  references, shredded writer statistics, row-lineage preservation in
  compaction) land on the pinned forks and are re-pinned.
- Generated schemas, OpenAPI, stubs, and goldens are regenerated, never
  hand-edited.
- The change ships in at most three tasks: (1) Variant storage, Iceberg v3,
  built-in tables, query operators, and Bloom sizing; (2) user-defined Struct
  and Variant tables in all SDKs, depending on 1; (3) shredding, leaf
  projection, pushdown, and pruning, depending on 1 and parallel to 2.
- `architecture/bifrost-design.md` is updated to describe Iceberg v3, the
  Variant type and its query surface, shredding, leaf pruning, and the
  duplicate-key rule before completion.

## Required system boundaries and flow

```text
OTLP or canonical Arrow
  -> table-owned projection to the logical schema (Variant, Struct, promoted)
  -> Scribe WAL and staged runs (unshredded, per-path size summaries)
  -> Scribe artifact assembly (layout from combined summaries, shredded write,
     footer summary)
  -> Forge compaction (layout from input footers, shredded write, row lineage
     preserved)
  -> Oracle: session operators and functions; leaf predicates in the signed
     assignment; shared leaf resolver per file; leaf projection, decode filter,
     row-group and page pruning on hot and published paths
  -> SDK result terminals (native values or Arrow extension)
```

## Acceptance obligations

Journeys run through the real SDKs against a real server (`WyrdTestServer`
with repository-managed Postgres), crossing acknowledgement, flush and
publication, Oracle read, and client decode.

#### AC-001 — OTel attributes are queryable

In Rust, Python, and TypeScript, OTLP spans, logs, and metrics are found by
`->>` on attributes, resource attributes, and log bodies; typed values read
back with their types; absent keys read as null; and every promoted column of
REQ-006 to REQ-008 equals its source. The same rows written through canonical
Arrow read back identically.

#### AC-002 — Iceberg v3 and row lineage

A Rust Bifrost test shows every built-in and user table is v3, and that each
row's `_row_id` is unchanged after a Forge compaction.

#### AC-003 — Built-in payloads are typed

`drift_report` and `eval_summary` are queried by field in each SDK, and every
column of REQ-010 is queried with `->>`.

#### AC-004 — User tables with open and nested data

In each SDK, a table registered from a model with a free-form field, a union
field, and a nested model accepts `insert` rows and Arrow batches (Variant
extension and JSON text), and the values are queried by `->`, `->>`, and Struct
field access and decoded into native values by the typed row terminal.

#### AC-005 — Refusals

In each SDK, an undeclared row key, a model allowing extra keys, and each
unsupported type of REQ-016 are refused with their exact catalog codes, and no
row is queued for a refused write. Invalid JSON in `parse_json` returns a
stable query error; `try_parse_json` returns null.

#### AC-006 — Shredding equivalence

A Rust Bifrost test writes nested data whose paths vary in size and type
across files,
and shows that the chosen layouts follow REQ-020, are identical when the same
input is rewritten, and that a fixed set of queries returns identical results
over hot files, published files, and compacted files, and with shredding
disabled for comparison in the test.

#### AC-007 — Leaf reads and pruning

Rust Bifrost tests on both read paths show, from Oracle read metrics, that a
query on a shredded top-level key, a shredded nested path, or a nested Struct
field reads only that leaf and skips row
groups and pages its statistics exclude, including in a distributed query
whose follower receives the leaf predicate in its signed assignment. A
regression test shows a Struct field named like a top-level column is matched
correctly on the hot path. A benchmark records bytes read and latency for a
shredded-key filter against the same filter on an unshredded key.

#### AC-008 — Sensitive data stays gated

A caller without payload permission is refused when reading a sensitive
Variant or Struct column through a whole-column read, `->`, `->>`, `to_json`,
a Struct field, or a shredded leaf.

#### AC-009 — Bloom sizing

A Rust Bifrost test shows Bloom filters in Scribe and Forge output are sized
for the row group and folded to the data, and that a hot-path `IN` query on a
Bloom column skips row groups that hold none of its values.

#### AC-010 — Verification

Format, lints, `codegen:check`, `test:bifrost` (all tiers and languages), and
the boundary checks for the touched surfaces pass.

## Open material decisions

None.

## Revision history

- **Revision 2 (2026-10-05, draft):** Shredding chooses scalar paths at any
  depth, not only top-level keys, because nested JSON payloads are the common
  case for user tables; `parquet-variant-compute` 59.3 builds nested shredded
  groups by path. Arrays stay unshredded. Leaf projection and pushdown apply to
  nested paths and nested Struct fields.
- **Revision 1 (2026-10-05, draft):** Created from the closeout research on
  `wyrd-forge` HEAD `30d31ebac`, following Pydantic Logfire's struct-field
  pushdown and Bloom-folding practices. Moves the Bifrost parts of
  `SPEC-verified-change-contract` revision 64 (its REQ-206 struct columns,
  REQ-207, AC-058 and AC-059 Bifrost evidence, and two open decisions) into
  this change. Decided with the user: Iceberg v3 directly; Variant for open
  data and Struct for known shapes; Logfire's top-128 per-file shredding at
  write and compaction; Bifrost-owned leaf pruning because DataFusion 55 lacks
  it; keep the fork reader rather than `ParquetSource`; refuse undeclared keys;
  one metrics table; no migration.

## Material authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/bifrost-design.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/iceberg.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/arrow-analytical-interop.md`
- `changes/active/bifrost-canonical-otel-signals/spec.md`
- Pydantic, "Struct field pushdown": https://pydantic.dev/articles/struct-field-pushdown
- Pydantic, "Bloom filter folding": https://pydantic.dev/articles/bloom-filter-folding-parquet-logfire
- `apache/datafusion#20871`

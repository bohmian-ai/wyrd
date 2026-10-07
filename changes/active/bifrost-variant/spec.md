---
id: SPEC-bifrost-variant
revision: 17
status: approved
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
- each final Scribe hot object and Forge output independently infers useful
  Variant fields from its first rows and shreds them into standard typed leaf
  columns; recovery-stage Scribe runs remain unshredded;
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
  DataFusion's `ParquetSource`. Ensure this works for both Interactive and Analytical paths.
  There should be one shared way.
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
- The unsigned follower scan assignment carries pushed-down predicates as
  `ScanPredicate` (`crates/wyrd-spec/src/vala/assignment_authority.rs:77`) with a
  digest over them. Peer mTLS authenticates its origin; the digest binds the
  assignment bytes to the authenticated peer context.
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
- Per-file Variant shredding in final Scribe hot objects and Forge outputs;
  recovery-stage Scribe runs remain unshredded.
- DataFusion shared nested-field projection, decode-time filtering, and
  conservative row-group/page pruning for Struct fields and semantic Variant
  access on both read paths.
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

## Locked contracts

These values and layouts are public or persisted behavior. Implementations do
not choose alternatives.

### Variant representation, limits, and failures

- `CanonicalType::Variant` and `DataTypeSpec::Variant` represent the Arrow
  extension named `arrow.parquet.variant`. Its schema-fingerprint tag is the
  single byte `0x0d`, with no child or parameter bytes. Physical shredded
  leaves and Iceberg metadata columns are excluded from that fingerprint.
- Variant maximum depth is the fixed constant `64`; maximum encoded value size
  is the fixed constant `8_388_608` bytes. They are not configuration knobs.
  Depth counts the root container as one; size is the canonical encoded Variant
  value bytes, metadata plus value, before queue reservation or durable write.
- Conversion uses the already-locked `parquet-variant`,
  `parquet-variant-compute`, and `parquet-variant-json` crates. No parallel
  Variant model or validator is introduced.
- Public failures are catalogued in `wyrd-spec` and keep these exact codes and
  detail fields:

| Code | Detail fields |
|---|---|
| `WYRD_VALA_400_VARIANT_INVALID_JSON` | `field`, `row`, `path` |
| `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` | `field`, `row`, `path`, `numeric_kind` |
| `WYRD_VALA_400_VARIANT_TOO_DEEP` | `field`, `row`, `path`, `depth`, `limit` |
| `WYRD_VALA_413_VARIANT_TOO_LARGE` | `field`, `row`, `bytes`, `limit` |
| `WYRD_VALA_400_BIFROST_UNDECLARED_FIELD` | `field`, `row` |
| `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE` | `field`, `data_type` |

For a write, the existing request-envelope size limit is checked first. Rows
are then checked in input order and fields in logical-schema order: undeclared
field, unsupported/wire-mismatched type, Variant byte limit, JSON/extension
validity, numeric range, then depth. The first failure is returned. Query
`parse_json` uses `WYRD_VALA_400_VARIANT_INVALID_JSON`; `try_parse_json`
returns SQL null.

Canonical Arrow Variant input follows the same exact numeric domain as JSON.
Integer primitives must fit `i64`. A Decimal16 is accepted only when its scale
is zero and its coefficient is from `i64::MAX + 1` through `u64::MAX`, which is
the canonical Variant representation of a JSON integer in that range. Every
other Decimal16 is refused before acknowledgement with
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` and `numeric_kind: "decimal"`.
This keeps every accepted number exact in Rust, Python, TypeScript, HTTP, MCP,
and CLI without adding an arbitrary-precision public number type.

### Persisted built-in Structs

Field order, names, nullability, and child nullability are fixed:

```text
resource_entity_refs: non-null List<non-null Struct<
  type: non-null Utf8,
  id_keys: non-null List<non-null Utf8>,
  description_keys: non-null List<non-null Utf8>,
  schema_url: non-null Utf8
>>

drift_report: nullable Struct<
  method: nullable Utf8,
  features: nullable Variant,
  verdict: nullable Utf8
>

eval_summary: nullable Struct<
  total_tasks: nullable Int32,
  passed_tasks: nullable Int32,
  failed_tasks: nullable Int32,
  pass_rate: nullable Float64,
  duration_ms: nullable Int64
>
```

Children of a nullable Struct are nullable. Producers write every child of a
present Struct and null every child of an absent one, so a field query such as
`eval_summary['total_tasks']` reads SQL null for a row without that Struct on
hot and published data alike.

`DriftReport.features` is Variant because feature names are open. The owning
enum display strings are persisted for `method` and `verdict`. Producers in
`wyrd-server/src/verification/results.rs`,
`wyrd-client/src/observe/eval.rs`, gateway capture, agent traces, audit
publication, and OTLP/canonical signal projection must emit the new shapes;
their readers, generated contracts, examples, and documentation must consume
the same shapes. No producer may continue writing the replaced JSON-text or
protobuf-binary form.

### Queue admission and Arrow normalization

The shared queue has one prepared-row boundary. Its contract is:

```text
RowPreflight::prepare(destination logical schema, complete input)
  -> PreparedRows
Producer::enqueue_prepared(PreparedRows)
```

`prepare` synchronously validates and normalizes the entire row set, computes
its exact queue charge, and performs no queue or budget mutation. The producer
then reserves once and enqueues once. A multi-row call is all-or-none.
Cancellation before handoff releases the reservation and admits nothing;
after handoff, the existing stable-batch retry and acknowledgement rules apply.
No fallible schema or value conversion remains after reservation.

`Bifrost::write_batch(table, batch)` keeps its public signature and sends the
batch verbatim. The client neither describes nor conforms it; the server judges
it at its trust boundary, accepts only the Variant extension for a Variant
column, and repeats validation there. `wyrd-queue` owns row Variant JSON/value
preparation.

### Oracle registration and distributed wire

One dependency-owning Oracle session registration method installs the `->`
and `->>` expression planner plus `parse_json`, `try_parse_json`, and `to_json`
before any logical-plan creation, physical-plan encode/decode, provider
registration, or execution. Every production leader, admission, follower,
analytical, and worker session calls it. `ORACLE_VARIANT_SQL_VERSION` is `1`
and is included in the peer contract fingerprint; a worker refuses a different
version before decoding a plan.

Follower assignments remain unsigned. Peer mTLS authenticates the sender; the
assignment digest uses domain `wyrd.oracle.assignment-authority.v8\0` and
binds the canonical assignment bytes to the authenticated context. The actual
Scribe/Oracle protobuf at `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto` changes
in place; Bifrost has not shipped, so no compatibility fields or alternate wire
model are retained:

```proto
enum ScanPredicateOp {
  SCAN_PREDICATE_OP_UNSPECIFIED = 0;
  SCAN_PREDICATE_OP_EQ = 1;
  SCAN_PREDICATE_OP_NOT_EQ = 2;
  SCAN_PREDICATE_OP_LT = 3;
  SCAN_PREDICATE_OP_LT_EQ = 4;
  SCAN_PREDICATE_OP_GT = 5;
  SCAN_PREDICATE_OP_GT_EQ = 6;
  SCAN_PREDICATE_OP_IS_NULL = 7;
  SCAN_PREDICATE_OP_IS_NOT_NULL = 8;
  SCAN_PREDICATE_OP_IN = 9;
}

message ScanColumnRef { string column = 1; }
message ScanStructRef { string column = 1; repeated string fields = 2; }
message ScanVariantRef { string column = 1; repeated string keys = 2; }
message ScanLeafRef {
  oneof kind {
    ScanColumnRef column = 1;
    ScanStructRef struct_field = 2;
    ScanVariantRef variant = 3;
  }
}
message ScanPredicate {
  ScanPredicateOp op = 1;
  ScanLeafRef leaf = 2;
  repeated ScanLiteral literals = 3;
}
```

Scalar comparisons require exactly one literal, `IN` requires at least one
same-typed literal, and null tests require none. Protobuf/domain conversion
lives only in `crates/wyrd/wyrd-tonic/src/private_conversion.rs` and rejects
unknown or unspecified operators, a missing leaf, empty column, an empty path,
empty path segments, the wrong literal cardinality, and mixed `IN` literal
types. After
table-schema resolution and before provider construction or execution,
assignment validation rejects a leaf kind that does not match its declared
logical root. `FollowerScanAssignment.required_columns` remains the ordered
top-level output, predicate, and hidden-tenant closure; no second required-leaf
field is added.

Path elements are whole UTF-8 keys/fields. Canonical digest encoding writes the
leaf tag (`0`, `1`, `2`), length-prefixed column, segment count and
length-prefixed segments, then the domain predicate tag (`0..=8` for `Eq`,
`NotEq`, `Lt`, `LtEq`, `Gt`, `GtEq`, `In`, `IsNull`, `IsNotNull`) and the
existing canonical literal encoding; `In` adds a `u32` count and preserves
literal order. The follower verifies mTLS context, peer contract version, and
digest before plan decode, provider construction, or file IO.

### Semantic Variant access on shared nested-field pushdown

Struct and Variant keep distinct logical semantics and share DataFusion's
physical nested-field machinery:

- Struct syntax remains DataFusion `GetFieldFunc` / `get_field`, which promises
  exact field extraction.
- Literal Variant `->` and `->>` paths lower to one Oracle semantic
  `variant_get` UDF backed by Arrow-rs `variant_get`. `->>` converts that result
  to text afterward. Array-index and dynamic paths retain full-root evaluation.
- The Variant UDF remains in the plan and declares every required physical
  field: the selected `typed_value`, each path-local residual `value` needed for
  fallback, and top-level `metadata`.
- TASK-003's DataFusion fork exposes one narrow owned-plan facade from
  `datafusion-datasource-parquet`:

```text
PerFileParquetReadPlanner::plan(PerFileParquetReadInput<'_>)
  -> Result<PerFileParquetReadPlan>

PerFileParquetReadInput:
  logical projection physical expressions
  optional logical filter physical expression
  file Arrow schema
  Parquet schema and metadata
  PhysicalExprAdapterFactory

PerFileParquetReadPlan:
  ProjectionMask
  projected Arrow schema
  optional decoder RowFilter
  optional row-group PruningPredicate
  optional page-pruning predicate
  conservative-fallback reason
```

  The facade returns this owned plan and never mutates or opens a reader.
  Missing or invalid field requirements return a successful full-root/no-prune
  plan with a reason. Genuine metadata or schema errors return a typed
  DataFusion error.
- Missing or invalid field requirements select full-root decoding with no
  statistics pruning, never an error or guessed leaf.

```text
Struct syntax  -> get_field --------------------\
                                                   -> logical-column auth
Variant syntax -> variant_get + field requirements /  -> per-file read plan
                                                      -> nested projection
                                                      -> decoder filtering
                                                      -> conservative pruning
```

DataFusion owns that one generic physical projection, decoder-filter, and
statistics-pruning path without pretending Variant is exact Struct extraction.
Variant owns path interpretation, standard layout requirements,
missing/null/conversion semantics, and residual fallback. `HotParquetExec` and
the pinned Iceberg `ArrowReader` call the exact same facade after their own
footer/schema discovery, apply its returned plan, and report through their
existing metrics. The facade owns no Wyrd metrics or IO. Interactive,
Analytical, leader, and follower paths therefore cannot drift. No Wyrd
Variant-specific projector, filter, pruner, general read-planning abstraction,
or second reader is added.

TASK-001 implements `variant_get`, SQL lowering, session registration, codec
round trips, and full-root/residual correctness against the current workspace
DataFusion source. TASK-003 alone owns the DataFusion fork, physical field
requirements, facade, and workspace repin. That DataFusion 55 fork backports
PR `#25013` at reviewed head
`cfc4298af54ee1301c38b675372288d1b385c6e9`, preserving its separate
`struct_field_access()` and
`required_input_fields(ReturnFieldArgs) -> InputFieldRequirement` contracts.
It also imports the nested row-group statistics patch at
`0b0506a9acab9d5892ecf7e89243c3b34664bcc6`, rebased on that capability.
The writable fork is `https://github.com/bohmian-ai/datafusion`, based on the
immutable DataFusion `55.0.0` commit
`d5552342012888b7d1a3ab88d92e3d292fc0cde0`. The reviewed source remote for
both patch series is `https://github.com/peterxcli/datafusion`: PR `#25013`
ends at `cfc4298af54ee1301c38b675372288d1b385c6e9`, and nested-statistics PR
`#2` is `0b0506a9acab9d5892ecf7e89243c3b34664bcc6`. The expected sibling checkout
is `/home/thorrester/Documents/GitHub/datafusion`.

One immutable workspace-level `[patch.crates-io]` source override pins
`datafusion`, `datafusion-common`, `datafusion-datasource-parquet`,
`datafusion-expr`, `datafusion-functions`, `datafusion-physical-expr`,
`datafusion-physical-expr-adapter`, and `datafusion-pruning` to the same tested
Wyrd fork revision. The workspace, iceberg-rust, iceberg-datafusion,
compaction, and datafusion-distributed consequently share one DataFusion source
and type universe.
These patches are removed when a released DataFusion provides equivalent
capabilities; the UDF, standard files, and query behavior do not change.

### Per-output-file shredding policy

Scribe recovery-stage Parquet runs written by `encode_batch` retain the stable,
unshredded logical Variant schema. `StagedRunMerge` therefore continues to
merge exact schemas, and `staging_runtime::restore_context` can reconstruct and
publish those runs after the WAL is retired without reconciling independently
inferred physical layouts. No staging format, schema union, or recovery-only
layout protocol is added.

The pinned iceberg-rust fork is the single owner of the pure analyzer and the
Arrow `ShreddedSchemaBuilder` / `shred_variant` schema-application wrapper,
beside `ParquetWriterBuilder`. Scribe `encode_ordered_claim` calls that pure
fork-owned API at its final-object boundary. `encode_batch` bypasses it.

For Forge, the same fork provides one concrete
`VariantParquetWriterBuilder` implementing the existing `FileWriterBuilder`.
Each `build(output)` returns a fresh deferred writer with no opened Parquet
encoder. That writer buffers only its output's prefix, infers the physical
schema, constructs the ordinary Parquet writer, replays once, and streams.
Builder clones contain policy constants only and never sample state.
Compaction-core supplies this builder to the existing `RollingFileWriterBuilder`;
each rollover therefore creates a new inference owner without changing the
five-field Forge result handoff or commit authority. No Wyrd strategy framework
or reverse dependency from the fork to Wyrd is added. Wyrd supplies the row,
byte, frequency, candidate, emitted-child, and depth bounds as internal
constructor values, never as public or persisted configuration.

Each final Scribe hot object opened by `encode_ordered_claim` and each Forge
output file independently retains a prefix ending
when either 4,096 rows or 67,108,864 bytes (64 MiB) of retained Arrow memory is
reached. Retained memory uses the writer's existing accounting measure for
Arrow backing buffers and is charged to existing memory admission. The buffer
does not copy Variant values unnecessarily and is not a second untracked memory
owner.

Before retaining a row or batch slice that would cross the byte bound, the
writer stops buffering and opens from the prefix it already owns. If the first
row alone exceeds 64 MiB, it is the sole progress exception: retain it, fully
charge its backing memory, infer from it, and open immediately. Reaching either
limit runs inference, constructs the standard physical schema, opens the
Parquet writer, replays the buffer once, releases its retained-memory charge,
and streams subsequent rows. Close infers from a shorter prefix; empty output
creates no file. Every rollover starts a new buffer. Success, error,
cancellation, and retry release or transfer every charge exactly once.
Scribe charges the prefix to its existing writer admission. Forge's existing
task reservation includes one 67,108,864-byte prefix for every concurrently
open output writer. On rollover, the completed writer releases or transfers
its charge and the new deferred writer begins with fresh sample state; close
completion may remain concurrent under the existing rolling writer.

For each top-level Variant column, inference ignores nulls for type choice.
A field is eligible only when sampled observations belong to one compatible
family. Integer widths widen together and decimal widths widen together;
incompatible scalar families and scalar/container mixtures remain residual.
Fields present in at least 10% of sampled non-null root Variant values are
eligible. At each object node, inference tracks at most 1,000 candidate
children, keeps at most 300 eligible children by observation frequency with
unsigned UTF-8/alphabetical tie-breaking, and emits them alphabetically.
Traversal stops at depth 50. Arrays remain residual in this change.

Arrow 59.3 `ShreddedSchemaBuilder` and `shred_variant` apply the inferred
schema. Later incompatible values use the standard residual `value`; sampling
changes performance only, never correctness. The standard Parquet
`metadata`/`value`/`typed_value` physical schema is the sole persisted layout
authority. There is no Wyrd summary, footer key, staged-run merge protocol,
table-wide shredded schema, spill, strategy abstraction, or configuration.

The 4,096-row and 64-MiB limits are internal writer-policy constants, not
public, wire, table, fingerprint, or persisted contracts. Measurements may tune
them later without changing file compatibility or query behavior.

### Safe pruning and hidden row lineage

A standard shredded path may still carry incompatible later values in its
path-local residual `value`. Its typed-leaf statistics or page index may
exclude a row group/page only when that corresponding residual `value`
statistics prove all null and the predicate literal/cast has the same type.
Otherwise Oracle evaluates the semantic `variant_get` with residual and
metadata inputs. Missing, malformed, incomplete, or type-incompatible evidence
always means no pruning.
Authorization of the logical source column happens before leaf resolution or
IO.

Iceberg metadata columns remain internal. Forge reads `_row_id` and
`_last_updated_sequence_number` beside the logical projection, carries them in
its internal physical batch, and writes those exact values for every surviving
row. They never enter the public schema or fingerprint. The existing five-field
Forge rewrite handoff is unchanged; lineage evidence stays in its output
`DataFile`s.

### Nested-field performance evidence

The performance evidence is one direct, opt-in nested-field microbenchmark,
not the 10-million-row Bifrost capacity workload and not a before/after suite.
It follows the published Logfire/DataFusion shape: `262_144` rows, `8_192`-byte
string siblings, local Parquet files, optimized code only, Criterion medians,
and a compact result table. It runs the actual nested-field planning and
physical read path without a server, Postgres, network, or concurrent load.

The benchmark has narrow, wide, and nested logical objects in both forms:

- fixed Arrow/Parquet Struct; and
- standard Parquet Variant whose selected paths are shredded into equivalent
  `typed_value` struct leaves while metadata/residual and large siblings remain
  physically present.

`narrow` contains one 8 KiB string sibling and one `Int32`; `wide` contains
four 8 KiB string siblings and one `Int32`; `nested` contains an inner 8 KiB
string plus `Int32` and an outer 8 KiB string. A top-level `Int32` id supports
the two-column case. Values vary enough that Parquet cannot collapse the large
siblings to a constant dictionary entry.

It reports these cases for each applicable Struct and Variant form:

| Shape/case | Published optimized reference |
|---|---:|
| `wide/select_small_field` | 419 µs |
| `wide/sum_small_field` | 638 µs |
| `wide/select_one_string_field` | 671 µs |
| `nested/select_extra_string` | 685 µs |
| `nested/select_inner_small_field` | 450 µs |
| `nested/sum_inner_small_field` | 662 µs |
| `narrow/select_small_field` | 422 µs |
| `narrow/select_id_and_small_field` | 445 µs |

Every case first asserts the exact result, the correct logical expression
(`get_field` for Struct or `variant_get` for Variant), the expected physical
field requirements, and read metrics showing that no unselected sibling leaf
was read. Variant may read only the metadata and path-local residual fields its
semantic UDF declared. Timing starts only after fixture creation and session
registration. There is no full-object baseline and no speedup ratio.

The benchmark uses whatever CPU and memory are available, like an ordinary
Criterion benchmark. It records local medians and selected-leaf metrics without
hardware detection, resource reservation, host-load qualification, or absolute
latency pass/fail thresholds. The published optimized values above identify the
workload and expected order of magnitude; they are comparison context, not a
gate across unlike machines. Functional result, plan-shape, and selected-leaf
IO assertions fail on every machine.

## Required behavior

### Storage format

#### REQ-001 — Iceberg v3

Every Bifrost table, built-in and user-defined, is created as Iceberg format
v3. Physical-table validation refuses a table that is not v3. Forge
maintenance, including garbage collection, operates on v3 tables. This is the
only initial format: non-v3 input is unsupported, not a migration or mixed-
version rollout case. Creation and REQ-002 rewrite behavior land together.

#### REQ-002 — Row lineage survives compaction

Appended files assign row ids through the v3 first-row-id mechanism. A Forge
rewrite preserves each surviving row's `_row_id` and
`_last_updated_sequence_number`, so a row keeps the same lineage identity
across any number of compactions. The hidden lineage columns follow the locked
internal transport and never appear in the logical table schema.

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
an integer; an integer above that range but within the 64-bit unsigned range is
a scale-zero Variant decimal; a non-integer number is a double; any other
integer is refused with `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`
(REQ-019). Every accepted integer therefore reads back exactly in every
terminal, including Rust `serde_json::Value`, without serde_json
`arbitrary_precision`. This matches BigQuery `PARSE_JSON`'s default exact mode;
callers needing wider integers send them as strings.

Already-encoded Arrow Variant input uses that same domain: its only accepted
Decimal16 form is scale zero with a coefficient from `i64::MAX + 1` through
`u64::MAX`. Other decimals are refused with
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` and `numeric_kind: "decimal"`.
Callers needing an exact fractional decimal or a wider integer send it as a
string. Wyrd does not add a second arbitrary-precision numeric surface.

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

`write_batch` in every SDK sends an Arrow batch verbatim. A declared Variant
column carries the Arrow Variant extension, as a Bifrost query result does, so
a query selecting a table's columns in their declared order copies into a table
of the same declaration unchanged. The server judges the batch
against the table's declared schema and refuses a mismatch with its stable
code; no SDK converts, reorders, or fills columns.

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
`WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`, naming the field and type, at every
SDK declaration boundary capable of representing that type before any
request, and again at the server. Rust and Python Arrow declaration boundaries
cover every listed Arrow type. TypeScript's JSON Schema/Zod declaration
boundary covers only forms representable by JSON Schema and does not gain a
`TableConfig.fromArrow` surface for this requirement. SDK documentation lists
exactly the types its public declaration formats support.

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
`parse_json(column) ->> 'key'`. Literal object-key chains lower to the semantic
`variant_get` UDF backed by Arrow-rs; `->>` adds text conversion after that
result. Struct access remains exact DataFusion `get_field`. Dynamic Variant
paths retain full-root evaluation.

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
Variant nested beyond depth 64, and a Variant exceeding 8,388,608 encoded bytes
fail with stable catalog codes before acknowledgement
(writes) or as a query error (queries). They are never truncated or stored
partially.

A query failure keeps the same catalog code and details whether it occurs
before or after the first result batch is sent. The late query terminal
carries the existing catalog error (code and details) for every failure, not a
fixed code list plus free text, in interactive and distributed execution over
HTTP and gRPC. Every SDK raises that catalog error unchanged and discards the
partial result. This applies to all catalog errors, not only Variant ones; a
failure with no catalog identity remains `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.

### Shredding

#### REQ-020 — Per-file layout

Scribe recovery-stage runs written by `encode_batch` remain unshredded and use
the stable logical Variant schema. Each final Scribe hot object written by
`encode_ordered_claim` and each Forge output independently retains a prefix
until it reaches either 4,096 rows or 67,108,864 bytes of retained Arrow
backing memory. The writer uses its existing memory-accounting measure and admission.
It stops before accepting a row or batch slice that would cross the byte bound;
if the first row alone exceeds it, that row is retained and fully charged as a
progress exception. Reaching either limit infers the standard shredded schema,
opens the writer, replays the prefix once, and streams the remainder. Each
rollover infers again; close analyzes a shorter prefix; empty output creates no
file. Buffer charges are released exactly once on success, error, cancellation,
and retry. Scribe charges its existing writer admission. Forge's existing task
reservation includes one 67,108,864-byte prefix per concurrently open output.

For each top-level Variant column, nulls do not choose a type. Fields observed
in at least 10% of sampled non-null root Variant values are eligible when their
observations belong to one compatible family. Integer widths and decimal widths
widen within their families; incompatible scalar families and
container/scalar mixtures remain residual. At each object node the writer
tracks at most 1,000 candidate children and keeps at most 300 by frequency,
breaking ties by unsigned UTF-8/alphabetical name and emitting alphabetically.
Traversal stops at depth 50. Arrays remain residual.

#### REQ-021 — Choosing the layout without a second pass

The pinned iceberg-rust fork owns the one pure analyzer and Arrow 59.3
`ShreddedSchemaBuilder` / `shred_variant` wrapper. Scribe calls them for final
hot objects. Forge uses the fork's `VariantParquetWriterBuilder`: each existing
rolling-writer `build(output)` creates a fresh deferred writer that samples
before opening its ordinary Parquet encoder; clones contain only internal
policy constants. Later incompatible values go to the standard residual
`value`, so the sample changes performance only. The Parquet
`metadata`/`value`/`typed_value` schema is the only persisted layout authority;
no Wyrd summary or layout metadata is written or required. Forge independently
re-infers every output of its 1-GiB compaction target and never inherits or
merges input-file selection evidence. Scribe infers only after its unshredded
staged runs enter a final hot-object merge; it never infers or persists a
shredded recovery run.

#### REQ-022 — Shredded files are read correctly everywhere

Both read paths read shredded files. For every query, the result is identical
whether a path is shredded, partly shredded, or unshredded, at any depth; whether the file is
hot or published; and before and after compaction.

### Pruning

#### REQ-023 — Leaf projection

A Struct field remains exact `get_field`. A literal Variant path remains
semantic `variant_get` and declares the selected `typed_value`, necessary
path-local residual `value`, and top-level `metadata` fields to DataFusion's
shared nested projection machinery. The one DataFusion per-file read-planning
facade supplies both readers with the leaf projection mask, projected Arrow
schema, adapted decoder row filter, and row-group/page pruning predicates.
Unshredded, missing, invalid, or incompatible layouts use full-root residual
evaluation with no statistics pruning.

#### REQ-024 — Leaf filtering and row-group skipping

On both read paths, Bifrost pushes these predicates down to a Struct field or
shredded Variant path, at any depth:

- `=`, `<>`, `<`, `<=`, `>`, `>=`, `IN`, `IS NULL`, and `IS NOT NULL`
  comparing a literal with `v ->> 'k'` or a chain such as
  `v -> 'a' -> 'b' ->> 'k'` (string-shredded paths), `CAST(<that> AS t)`
  (paths shredded as a matching type), or `s['a']['b']`.

DataFusion's shared nested-field pipeline handles both logical forms while
retaining `get_field` and `variant_get` semantics. It filters while decoding and
skips row groups and pages only when the corresponding Variant residual is
all-null and the predicate type matches, or when fixed Struct statistics are
complete. Otherwise the residual value is read and the predicate is evaluated
normally. Any other predicate shape is evaluated by the query engine with the
same result.

#### REQ-025 — Distributed scans carry leaf predicates

The unsigned follower scan assignment carries the locked `ScanLeafRef` oneof,
`ScanPredicateOp`, and literal-cardinality contract defined above in the actual
`wyrd.v1.proto`, covered by the v8 assignment digest. The sole protobuf/domain
conversion validates every operator, leaf, path, and literal list; resolved
schema validation rejects incompatible logical root kinds before execution.
Peer mTLS authenticates the sender. The wire and digest format change in place;
no signature, compatibility message, or unsigned side channel is added.

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
query results, schema fingerprints, or the logical schema users see. Scribe
recovery-stage runs remain unshredded, mergeable, and restartable after WAL
retirement; only their final hot object selects a shredded layout.

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

Variant conversion and per-file inference are bounded by value depth/size, the
4,096-row or 64-MiB admitted Arrow buffer (with the single-row progress
exception), 50-level inference depth, 1,000 tracked children per object node,
and 300 emitted children per object node.

#### INV-008 — One nested-field pushdown pipeline

Struct remains exact DataFusion `get_field`; Variant remains semantic
`variant_get` backed by Arrow-rs and declares its required physical Struct
fields. Both feed DataFusion's shared nested projection, decoder filtering, and
conservative statistics pruning through the one TASK-003-owned per-file
read-planning facade used by both Oracle readers. No Variant-specific physical
optimizer, parallel reader, general Wyrd planner abstraction, harness, or
ingest path is introduced.

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
- Add direct workspace dependencies on the existing Arrow 59.3
  `parquet-variant`, `parquet-variant-compute`, and `parquet-variant-json`
  versions only where imported: JSON/value preparation in `wyrd-queue`, and
  Variant query kernels in `vala-bifrost-redux`. The pinned iceberg-rust fork
  owns the shared analyzer and standard shredding wrapper used by Scribe and
  Forge.
  DataFusion, Parquet, and Iceberg remain out of client-tier crates.
- TASK-003 alone bootstraps the writable
  `https://github.com/bohmian-ai/datafusion` fork from immutable v55 commit
  `d5552342012888b7d1a3ab88d92e3d292fc0cde0`, then rebases the reviewed
  `https://github.com/peterxcli/datafusion` changes ending at PR `#25013` head
  `cfc4298af54ee1301c38b675372288d1b385c6e9` and nested-statistics patch
  `0b0506a9acab9d5892ecf7e89243c3b34664bcc6`, plus the locked owned-plan
  facade; record and push the tested fork revision. The exact eight
  `[patch.crates-io]` entries listed above use that one revision, and focused
  metadata verification rejects another source or revision. TASK-001 remains
  on the current workspace source until TASK-003 performs the single repin.
- In the pinned Iceberg fork, the concrete `VariantParquetWriterBuilder`
  implements `FileWriterBuilder`, defers ordinary Parquet encoder creation
  until one output's prefix is analyzed, and retains the logical Iceberg schema
  for field identity and `DataFile` metadata. The reader validates standard
  shredded layouts with `VariantArray::try_new`; whole-Variant projections
  `unshred_variant` before cross-file union, while pushed literal paths project
  required leaves and run `variant_get` before union. It never constructs a
  table-wide union of incompatible `typed_value` schemas.
- Repin to released DataFusion and iceberg-rust APIs once they provide the same
  capabilities. Standard Parquet files and Wyrd SQL behavior remain unchanged.
- Generated schemas, OpenAPI, stubs, and goldens are regenerated, never
  hand-edited.
- The change ships in at most three tasks: (1) Variant storage, Iceberg v3 and
  lineage-safe compaction as one atomic capability, built-in tables, query
  operators, and Bloom sizing; (2) user-defined Struct and Variant tables in
  all SDKs, depending on 1; (3) shredding, leaf projection, pushdown, and
  pruning, depending on 1 and parallel to 2. No releasable state enables v3
  creation before lineage-safe Forge rewriting exists.
- `architecture/bifrost-design.md` is updated to describe Iceberg v3, the
  Variant type and its query surface, shredding, leaf pruning, and the
  duplicate-key rule before completion.

## Required system boundaries and flow

```text
OTLP or canonical Arrow
  -> table-owned logical schema (Variant, Struct, promoted)
  -> Scribe encode_batch writes unshredded recovery-stage runs
  -> encode_ordered_claim merges them into one final hot-object stream
  -> fork-owned analyzer handles the final Scribe object
  -> RollingFileWriter build(output) creates a fresh deferred Forge writer
  -> that final Scribe/Forge output retains at most 4,096 rows or 64 MiB
  -> fork-owned Arrow ShreddedSchemaBuilder + shred_variant wrapper
  -> standard Parquet metadata/value/typed_value file
  -> Oracle keeps Struct get_field or semantic Variant variant_get
  -> one DataFusion per-file facade plans projection/filter/pruning
  -> shared nested projection, decoder filter, conservative pruning
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
field, and a nested model accepts `insert` rows and Arrow batches carrying the
Variant extension, and the values are queried by `->`, `->>`, and Struct
field access and decoded into native values by the typed row terminal.

#### AC-005 — Refusals

In each SDK, an undeclared row key and a model allowing extra keys are refused
with their exact catalog codes, and no row is queued for a refused write.
Rust and Python additionally declare and refuse every Arrow type listed in
REQ-016 before any request. TypeScript proves declaration-time refusals for
unsupported forms expressible through its public JSON Schema/Zod boundary; it
is not required to synthesize Arrow declarations that boundary cannot
represent. Invalid JSON in
`parse_json` returns a stable query error; `try_parse_json` returns null.

#### AC-006 — Shredding equivalence

A Rust Bifrost test covers row-first and byte-first limits, the oversized-first-
row progress exception, short close, empty output, rollover, charge release on
every terminal, later incompatible values, and independent Scribe/Forge
inference. A recovery test stages unshredded Variant runs whose values would
select different physical layouts, retires the WAL after staged durability,
stops before publication, restores and merges those exact-schema runs, and
publishes one shredded hot object exactly once with no duplicate. Standard
Forge tests force two rolls to choose different physical layouts while close
completion may remain concurrent, with exact logical values and unchanged
`DataFile` and five-field handoff evidence. Standard
unshredded, partially shredded, differently shredded, hot, published, and
twice-compacted files return identical logical values while v3 row lineage
remains stable.

#### AC-007 — Leaf reads and pruning

Rust Bifrost tests on both read paths show that Struct uses `get_field` and
Variant uses `variant_get`, while both produce the same physical nested-field
requirements and the same DataFusion per-file facade outputs: projection mask,
projected schema, decoder row filter, row-group predicate, and page predicate.
Oracle metrics prove
selected-leaf reads, correct residual fallback, and conservative row-group/page
skipping, including in a distributed query whose follower receives the leaf
predicate through the revised protobuf and its unsigned, digest-protected
assignment. Conversion tests round-trip every leaf/operator form and reject
every malformed cardinality, path, tag, and logical-root combination. A
regression test shows a Struct field named like a top-level column is matched
correctly on the hot path. The locked nested-field microbenchmark proves the
optimized Struct and shredded-Variant cases read only selected leaves and
records the compact after-state timing table; no 10-million-row capacity run or
before/after comparison is part of this acceptance obligation.

#### AC-008 — Sensitive data stays gated

A caller without payload permission is refused when reading a sensitive
Variant or Struct column through a whole-column read, `->`, `->>`, `to_json`,
a Struct field, or a shredded leaf.

#### AC-009 — Bloom sizing

A Rust Bifrost test shows Bloom filters in Scribe and Forge output are sized
for the row group and folded to the data, and that a hot-path `IN` query on a
Bloom column skips row groups that hold none of its values.

#### AC-010 — Verification

Format, lints, `codegen:check`, `test:bifrost` (all tiers and languages),
`docs:check`, `check:docs`, `check:examples`, and the boundary checks for the
touched surfaces pass.

## Open material decisions

None.

## Revision history

- **Revision 17 (2026-10-06, approved):** By explicit human direction,
  `write_batch` sends its batch verbatim and the server judges it. The
  client-side describe-then-conform step of revisions 7 and 15 (JSON text for
  Variant columns, by-name matching, null-filling omitted columns) is removed:
  it duplicated the server's contract, and its describe call failed for
  write-only and gRPC-only callers. REQ-014, AC-004, and AC-005 are narrowed
  to match.
- **Revision 16 (2026-10-06, approved):** Resolves TASK-002 repeat-review
  `FIND-TASK-002-6` without adding the explicitly excluded TypeScript
  `TableConfig.fromArrow`. REQ-016 and AC-005 now require exhaustive
  unsupported-type declaration proof only at public SDK declaration boundaries
  capable of representing those types. Rust and Python retain the full Arrow
  matrix; TypeScript retains JSON Schema/Zod refusals, while write-time wire
  mismatches remain REQ-014 evidence rather than a substitute for declaration
  evidence. No runtime behavior or supported type changes.
- **Revision 15 (2026-10-06, approved):** From TASK-002 implementation. Arrow
  `write_batch` follows the same column rules as row `insert`: columns match by
  name in any order and take their declared nullability, an omitted nullable
  column is written as nulls, an omitted required column or a null in one is
  `SCHEMA_PARSE`, and an undeclared column is
  `UNDECLARED_FIELD`. The shared client conforms the batch in the same
  describe-then-normalize step, so every SDK has one behavior; the server wire
  contract is unchanged and stays exact.
- **Revision 14 (2026-10-06, approved):** By explicit human direction, Forge
  copies rewrite lineage through without a missing/null check, as Java and
  Spark v3 rewrites do. Bifrost writes only v3 tables whose manifest lists
  always assign row ids, so the refusal guarded an unreachable state; it and
  its injection proof are removed.
- **Revision 13 (2026-10-06, approved):** From TASK-001 review r5 and explicit
  human direction to choose the smallest user-facing contract. Canonical Arrow
  Variant input now uses the same exact numeric domain as JSON: integer
  primitives fit `i64`; Decimal16 is accepted only as the scale-zero encoding
  of `i64::MAX + 1..=u64::MAX`; every other Decimal16 is refused before ACK
  with `VARIANT_NUMERIC_OUT_OF_RANGE` and `numeric_kind: "decimal"`. Exact
  fractional or wider numbers travel as strings; no arbitrary-precision public
  terminal is added.
- **Revision 12 (2026-10-06, approved):** From TASK-001 review r4. The
  children of the nullable `drift_report` and `eval_summary` Structs become
  nullable. A non-null child is a required Parquet leaf, whose parent-masked
  nulls the pinned Parquet reader drops while DataFusion `get_field` returns
  the child without the parent's nulls, so published field queries read
  another row's values for an absent Struct (FIND-TASK-001-14). Nullable
  children keep their own nulls through Parquet with no read layer.
- **Revision 11 (2026-10-06, approved):** From TASK-001 review r2. JSON
  integers outside the signed/unsigned 64-bit ranges are refused with
  `NUMERIC_OUT_OF_RANGE` instead of stored as wide decimals, so every accepted
  value reads back exactly in every terminal (FIND-TASK-001-11). The late query
  terminal carries the full catalog error so failures after the first batch
  keep their code and details (FIND-TASK-001-12). serde_json
  `arbitrary_precision` stays rejected.
- **Revision 10 (2026-10-05, approved):** Assigns the one pure Variant analyzer
  and schema wrapper to the iceberg-rust fork and adds its concrete deferred
  `VariantParquetWriterBuilder`, so each existing Forge rollover owns an
  independent sample without changing the handoff. Locks Forge prefix memory
  reservation. Creates and names the writable `bohmian-ai/datafusion` fork,
  immutable v55 base, patch source, sibling checkout, eight source overrides,
  and owned `PerFileParquetReadPlan` facade contract. Corrects OTLP proof
  selectors and adds the docs-site and example gates. TASK-002 is unchanged.
- **Revision 9 (2026-10-05, approved):** Keeps `encode_batch` recovery-stage
  Parquet runs unshredded so exact-schema staged merges and restart after WAL
  retirement remain safe; only `encode_ordered_claim` final hot objects and
  Forge outputs infer layouts. Assigns all DataFusion fork/repin work to
  TASK-003 and requires one narrow `datafusion-datasource-parquet` per-file
  read-planning facade used by both Oracle readers for projection, decoder
  filtering, and row-group/page pruning. Specifies the in-place
  `wyrd.v1.proto` `ScanLeafRef`, `IN`, literal-cardinality, conversion,
  validation, and peer round-trip contract. TASK-002 is unchanged.
- **Revision 8 (2026-10-05, approved):** Supersedes the fixed 100-row sample
  with a per-output-file prefix bounded by 4,096 rows or 64 MiB of retained
  Arrow backing memory, whichever arrives first. It fixes stop-before-crossing,
  the oversized-first-row progress exception, existing admission accounting,
  exact charge release, rollover/close behavior, and independent Forge
  re-inference. These limits are internal tuning policy, not persisted or
  public compatibility contracts. The standard Variant layout and Iceberg-
  style analyzer remain unchanged.
- **Revision 7 (2026-10-05, approved):** Corrects the logical query contract:
  Struct remains exact `get_field`, while Variant remains Arrow-backed semantic
  `variant_get` and declares typed, residual, and metadata fields to
  DataFusion's shared physical nested-field machinery. Replaces the Wyrd
  top-128 summary/footer protocol with Apache Iceberg's independently inferred
  per-output-file policy: 100 buffered rows, 10% frequency, 300 emitted and
  1,000 tracked children per object node, depth 50, standard Arrow shredding,
  and no Wyrd layout metadata. Fixes direct dependency/fork seams,
  `write_batch` schema acquisition through `describe(table)`, clean Python and
  TypeScript setup, and fresh-v3-only wording.

- **Revision 6 (2026-10-05, approved):** Makes the focused nested-field
  benchmark an ordinary resource-adaptive Criterion run. It uses whatever host
  resources are available and reports local medians; there is no prescribed
  machine, reservation, load qualification, or absolute latency gate. The
  published optimized values remain workload context only.
- **Revision 5 (2026-10-05, approved):** Replaces the unrelated 10-million-row
  query-capacity benchmark with one readable after-state nested-field
  microbenchmark aligned to Logfire/DataFusion: 262,144 rows, 8 KiB string
  siblings, narrow/wide/nested Struct and equivalent shredded-Variant cases,
  selected-leaf IO assertions, and published optimized reference values. No
  baseline or speedup ratio.
- **Revision 4 (2026-10-05, approved):** Locks one nested-field query
  architecture with no permitted Variant-specific optimizer. Literal-key
  Variant `->`/`->>` and nested Struct access normalize to DataFusion's
  flattened `get_field` representation, then share authorization, per-file leaf
  binding, projection, decode filtering, and row-group/page pruning. Standard
  Parquet Variant shredded `typed_value` fields enter this Struct pushdown path;
  absent or unsafe typed leaves bind to the residual Variant. `LeafRef` is only
  its distributed wire projection.
- **Revision 3 (2026-10-05, approved):** Addresses the readiness review by
  making Iceberg v3 activation atomic with lineage-safe compaction; fixing the
  Variant limits, errors, fingerprint, Struct layouts, queue preflight, Oracle
  registration, unsigned follower wire/digest, and producer/consumer closure;
  and defining one bounded exact summary. Summary overflow conservatively
  writes an unshredded file, avoiding approximation, spills, or new operational
  state. Safe pruning requires homogeneous complete leaf evidence. Follow-up
  primary-source research confirmed that Logfire publicly specifies per-file
  top-128 selection and residual fallback, but not its closed-source memory or
  footer implementation; the bounded summary and nested Parquet Variant format
  here are therefore explicit Wyrd contracts, not attributed Logfire internals.
- **Revision 2 (2026-10-05, approved):** Shredding chooses scalar paths at any
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
- Pydantic, "Dynamic JSON attribute shredding in Logfire": https://pydantic.dev/articles/dynamic-shredding-2026-01-26
- Apache Parquet, `VariantShredding.md`: https://github.com/apache/parquet-format/blob/master/VariantShredding.md
- Apache Iceberg configuration: https://github.com/apache/iceberg/blob/main/docs/docs/configuration.md
- Wyrd DataFusion fork: https://github.com/bohmian-ai/datafusion
- Apache DataFusion PR `#25013`: https://github.com/apache/datafusion/pull/25013
- Nested-statistics PR: https://github.com/peterxcli/datafusion/pull/2
- Apache DataFusion issue `#20871`: https://github.com/apache/datafusion/issues/20871

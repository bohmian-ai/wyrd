---
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
status: approved
---

# Unified tables, physical layouts, and correlation

This is a normative approved supplement to [specification revision 12](../spec.md),
alongside [auth](auth-system.md) and [telemetry schemas](telemetry-schemas.md).
It records the approved storage and table-unification contracts.
Approval covers the complete packet; implementation follows its derived tasks.

## One table contract

System and dynamic tables use the same schema, managed-attribution, layout,
admission, publication, query, compaction, and evolution contracts. System
tables retain their named domain schemas and Wyrd-supplied default layouts;
dynamic tables supply user fields and a default or explicit layout. Both use
the same bounded layout declaration, validation, and tenant configuration
mechanism below. Tenant-authorized partition and Bloom overrides are supported
for system tables as well as dynamic tables.
There is no second warehouse engine, automatic wide-table expansion, or new
universal record ID.

| Contract | System tables | Dynamic tables |
| --- | --- | --- |
| Tenant ownership | Verified physical table, namespace, and object prefix | Same |
| Logical schema | Table owner's declared domain fields plus shared envelope | User fields plus shared envelope |
| Table description | Full resolved schema, layout, and revisions | Same |
| Admission | Typed validation, permission checks, accepted attribution, atomic batch identity | Same |
| Physical layout | Ledger default plus authorized tenant configuration | Explicit bounded declaration or documented default |
| Storage lifecycle | Scribe, Oracle, and Forge owners | Same |
| Read authorization | Existing permissions for every referenced table | Same |
| Evolution | Explicit coordinated revision | Same |

Tenant is not another row column or partition key. A query cannot change the
authenticated tenant through a table name, label, join key, or partition value.

## Shared managed envelope

Every system and dynamic table has these seven columns. The table's domain
fields remain separate. This replaces the earlier signal-only attribution scope; the fresh baseline
uses one envelope for all tables.

| Column | Logical type | Nullability | Meaning and production |
| --- | --- | --- | --- |
| `workload_card_uid` | Utf8 | nullable | Validated exact Service/Agent execution or binding root; null without proof. |
| `subject_card_uid` | Utf8 | nullable | Exact Card observed or judged; validated under the applicable observation/verification authority. |
| `run_id` | Utf8 | nullable | Existing application Run correlation; preserve its existing grammar. |
| `sub_principal_id` | Utf8 | required | Account on whose behalf the recorded action occurred. |
| `act_principal_id` | Utf8 | required | Account that performed the recorded action. |
| `wyrd_request_id` | Utf8 | required | Wyrd request correlator for the admitted or produced record. |
| `wyrd_ingested_at` | Timestamp(Microsecond, UTC) | required | Existing server/PostgreSQL admission instant. |

Public writers cannot supply managed identities as ordinary user fields.
Existing typed correlation inputs and verified workload context are validated
before server stamping. The canonical OTLP subject attribute remains
`wyrd.card_uid`; native Arrow schemas and SQL use `subject_card_uid`.

Generic custom rows need no Card or Run. An authorized rootless writer records
its account pair with nullable workload, subject, and Run. Selecting context
does not grant operation permissions. Owned observation subjects must agree
with the validated graph as defined by the auth supplement.

### Original action, not delivery identity

| Producer | Attribution source |
| --- | --- |
| Direct public write | Verified request account pair and validated context |
| Gateway capture | Originating accepted call context |
| Queued verification | Accepted execution authority plus validated binding ownership |
| Internal outbox/peer forwarding | Each record's retained accepted origin |
| Audit | Original permission decision, not the account delivering the outbox |

Unknown workload, subject, or Run remains null. Do not infer them from a shared
component, latest release, service labels, trace membership, or a delivery
worker's credentials. Retries cannot change a record's originating authority.

Audit retains its existing unresolved-principal sentinel when no caller was
verified. That sentinel grants no authority and is neither an authenticated
account nor System. Preserve the per-decision account pair. The new audit schema uses
`wyrd_request_id` in place of `request_id` and includes the nullable envelope
fields; no old audit data requires conversion. Its `audit_card_ref` remains the audit
target representation, not an automatically populated observation subject.
Do not force an audit target into an execution graph to populate a column.

### Reconcile existing root fields

These tables already have an explicit nullable root column:

| Table | Existing column | Canonical column |
| --- | --- | --- |
| `bifrost.verification.results` | `owner_card_uid` | `workload_card_uid` |
| `bifrost.verification.execution_inputs` | `owner_card_uid` | `workload_card_uid` |
| `bifrost.drift.result_features` | `owner_card_uid` | `workload_card_uid` |

The fresh schemas use `workload_card_uid` once per table, not an explicit
root plus a duplicate managed root. Binding-derived ownership remains validated
domain content; generic delivery context cannot overwrite it. Every new managed
observation schema uses `subject_card_uid` instead of `card_uid`. No old data or
field-ID conversion is required for this release. Future in-baseline renames
preserve IDs and values under the evolution contract.

The new nullable resource and convenience fields in the telemetry supplement
remain signal-specific. Dynamic tables and other system tables acquire the
shared envelope, not all 27 shared signal/resource columns.

## Exact system default layouts

Sort declarations are ordered. The abbreviations below are definitions, not
additional configurable mechanisms:

```text
E = deployment_environment ASC NULLS FIRST
S = service_name ASC NULLS FIRST
D(column) = column DESC NULLS LAST
A(column) = column ASC NULLS LAST
F(column) = column ASC NULLS FIRST

```

Each row lists the complete default Bloom set, not additions to a hidden floor.
There is no global mandatory Bloom-column set. All sets are tenant-configurable
under the same validation rules; the attribution schema remains mandatory.
The ledger records the agreed query-driven per-table defaults. Spans and logs
exclude `span_id` by default; tenants may enable it. Verification results retain
the agreed eight. No set is populated by a shared correlation-index rule.

| Table | Partition | Ordered sort keys | Complete default Bloom set |
| --- | --- | --- | --- |
| `bifrost.traces.spans` | `hour(start_time_unix_nano)` | E, S, D(start_time_unix_nano), A(trace_id) | `trace_id`, `workload_card_uid`, `subject_card_uid`, `run_id`, `service_name` |
| `bifrost.logs.records` | `hour(time_unix_nano)` | E, S, D(time_unix_nano), F(event_name) | `trace_id`, `workload_card_uid`, `subject_card_uid`, `run_id`, `service_name` |
| `bifrost.metrics.points` | `hour(time_unix_nano)` | E, S, D(time_unix_nano), A(metric_name) | `metric_name`, `service_name`, `workload_card_uid`, `subject_card_uid` |
| `bifrost.gateway.calls` | `hour(started_at)` | D(started_at), A(call_id) | `call_id`, `workload_card_uid`, `subject_card_uid`, `run_id` |
| `bifrost.drift.observations` | `day(created_at)` | D(created_at), F(series) | `subject_card_uid`, `series`, `workload_card_uid` |
| `bifrost.verification.results` | `day(ended_at)` | D(ended_at) | `workload_card_uid`, `subject_card_uid`, `run_id`, `result_id`, `execution_id`, `verifier_card_uid`, `binding_id`, `group_id` |
| `bifrost.verification.execution_inputs` | `day(recorded_at)` | D(recorded_at) | `execution_id`, `workload_card_uid`, `subject_card_uid`, `run_id`, `verifier_card_uid`, `binding_id`, `group_id` |
| `bifrost.drift.result_features` | `day(wyrd_ingested_at)` | D(wyrd_ingested_at) | `result_id` |
| `bifrost.system.audit_log` | `day(created_at)` | D(created_at) | `sub_principal_id`, `act_principal_id`, `wyrd_request_id`, `resource` |

The system default has no Service, environment, version, principal, binding,
Run, or root-UID partition dimension. An authorized tenant may add validated
dimensions. The proposed default telemetry sort clusters environment and Service
inside each hour; lookup filters also use statistics, Bloom filters, and page
indexes. Physical partition count does not multiply with those identities.

These are chosen defaults, not a measured performance guarantee. Verify
actual sort enforcement and pruning on representative mixed-Service,
mixed-environment, trace, Run, and verification queries. That evidence checks
the proposed contract; they do not imply user approval of a new default.
The hourly/daily default granularities in this ledger are agreed. Tenant-
configuration capability does not authorize future unrelated default changes;
those still require explicit approval. Writing this draft changes no runtime
configuration.

### Rationale and trade-offs

| Layout | Benefit | Cost |
| --- | --- | --- |
| Time only | Fewer groups; easier consolidation; stable ingestion fanout | Service/environment queries depend more on clustering, statistics, Blooms, and surviving-file reads. |
| Time plus environment identity | Direct environment pruning | Additional groups and smaller low-volume environment files. |
| Time plus Service identity | Direct Service pruning | Quiet Services fragment files; group count grows with Service count. |
| Time plus both identities | Direct pruning when both filters are supplied | Their combinations multiply ingestion/compaction groups. |
| Time plus bucketed Service | Bounds the Service dimension | Collisions retain several Services per bucket; exact row filtering remains necessary. |

For example, 24 hours times 3 environments times 500 Services permits 36,000
occupied combinations per day. Only combinations with data materialize; this
is not a predicted file count. Files cannot cross their partition boundary,
so compaction cannot merge many tiny identity partitions into one large file.

Conservative system defaults and configurable dynamic layouts are separate
choices. Iceberg has no one-column format limit. Wyrd's current limit is its
own durable time-partition contract; expanding that contract is included here.

## One layout contract and tenant configuration

| Rule | System table | Dynamic table |
| --- | --- | --- |
| Table identity | One tenant plus canonical TableRef | Same |
| Schema owner | Wyrd domain contract | Tenant-declared schema under shared validation |
| Initial layout | System ledger recommendation | Shared creation default or explicit layout |
| Tenant partition configuration | Supported, under the shared bounds | Same bounds |
| Tenant Bloom configuration | Complete set configurable, including empty | Same bounds; no mandatory floor |
| Current layout source | Persisted resolved tenant/table metadata plus revision | Same |
| Startup behavior | Validate canonical schema and respect authorized stored layout | Respect registered schema/layout |
| Read/write/compaction behavior | Resolve the accepted layout revision | Same |
| Alteration | Explicit authorized operation and concurrency precondition | Same mechanism; authorization reflects table ownership |

System layout administration requires explicit tenant-administrative authority;
ordinary query/record permission does not imply layout administration. Do not
introduce a role-name bypass. Use the existing `bifrost_table:write` permission
for both system and dynamic table alteration, as for existing table DDL.
This is tenant-wide administration; per-table DDL grants are outside this change.
Public registration of user fields remains
restricted to the owning dynamic-table capability; configuring a system layout
never replaces its canonical schema, sensitivity, managed fields, or time axis.

The system ledger supplies defaults, not a layout to reimpose on every startup.
Authorized tenant layouts must survive restart and deployment. Invalidate or
refresh cached layouts on committed revision changes. An override for one
tenant/table cannot change another tenant's physical table or defaults.

| Field handling | Creation | Explicit alteration |
| --- | --- | --- |
| Omitted `partition_by` | Use this table's creation default | Preserve its current resolved partition list |
| Supplied `partition_by` | Validate the complete list | Atomically replace with the validated complete list |
| Omitted `bloom_columns` | Use this table's declared default set | Preserve its current resolved Bloom set |
| Supplied `bloom_columns` | Use the complete validated requested set | Replace the complete set |
| Empty `bloom_columns: []` | No Bloom columns | Disable Blooms for future files; existing files remain readable |

The tenant controls the complete Bloom-column set, not the shared attribution
schema or global false-positive/encoding policy. No shared correlation column
is automatically reinserted after validation. A Bloom-only alteration cannot silently
change partitions through omission. The resolved result and revision must be
visible in table description and survive startup. Existing files need no
rewrite merely to add a Bloom policy for new files; missing historical filters
fall back to ordinary reads. Partition evolution follows its own spec rules.

The same sort validation applies to both table classes. The default partition
granularities are now agreed: hourly for signals/gateway/dynamic tables and
daily for verification/drift/audit. Tenant overrides remain available. This
agreement does not authorize unrelated sort/default changes.

### Agreed safe table-editing principles

| Change or situation | Required behavior |
| --- | --- |
| Timestamp partition source | Allow supported hour/day transforms on the declared required time column. |
| Service/environment string partition source | Allow supported identity/bucket/truncate transforms under shared bounds. |
| Arbitrary JSON/Variant partition source | Refuse; declare a supported typed column instead. |
| Add nullable custom field | Allow compatible evolution under the exact supported-type matrix. |
| Rename custom field | Preserve field ID and values; never treat the same data as an unrelated new field. |
| Automatic string-to-integer or other unsafe conversion | Refuse an implicit in-place conversion. |
| Wyrd system or managed fields | Tenant layout configuration cannot rename/drop/replace them. |
| Column used by current layout | Reject an incompatible drop/type change; no dangling partition/sort/Bloom reference. |
| Concurrent administrators | Require the expected current revision; atomically reject a stale edit rather than overwriting a committed change. |
| Authorization | Recording/querying data grants no implicit table-administration authority. |

For example, two administrators read revision 12. The first commits revision
13. The second's revision-12 mutation is refused; it must reread and reconsider
its change. The exact public contract and supported operations follow below.


The owning table-registration capability remains responsible for creation;
the same resolved layout shape is returned by table description. Replace the
single `partition_granularity` field with `partition_by` in the canonical
fresh-baseline contract. Retain `time_column`, `sort_keys`, and `bloom_columns`.
Do not add a parallel registration workflow or retain both layout spellings.

| Setting | Contract |
| --- | --- |
| Default time column | `wyrd_ingested_at` |
| Dynamic default partition | `hour(time_column)`; defaults to `hour(wyrd_ingested_at)` when no custom time column is supplied |
| Default dynamic sort | Declared time column DESC NULLS LAST |
| Default dynamic Bloom set | Empty; a generic user schema has no inferred lookup workload |
| Partition fields | Maximum 3 total |
| Required partition field | Exactly one `hour` or `day` transform of `time_column` |
| Additional partition fields | Up to 2; transforms `identity`, `bucket`, or `truncate` |
| Partition sources | Existing top-level primitive fields; transform/type compatibility validated |
| Bucket count | Integer 2 through 1024 inclusive |
| Truncate width | Positive integer; Iceberg transform semantics |
| Sort keys | Maximum 4; each declares direction and null order |
| Sort sources | Orderable scalar fields; no Variant, list, map, or struct |
| Tenant-declared Bloom columns | Maximum 16 total, for both system and dynamic tables |
| Bloom sources | Supported primitive fields; unsupported types refused |
| Per-write fanout | Maximum 32 distinct composite partitions per accepted batch |

The time column must be a required UTC microsecond/nanosecond timestamp.
Without an explicit partition list, use the table's agreed creation default
and resolved time column. Dynamic tables use `hour(time_column)`; the earlier
daily dynamic candidate is withdrawn. An explicit list must
contain exactly one hour/day transform of the resolved time column. System
tables retain their canonical time-column identity; tenant partition/Bloom
configuration cannot reinterpret event time or change domain semantics.
Resolve transforms through stable source field IDs; reject invalid types,
duplicate/redundant partition definitions, duplicate sort keys, invalid bucket
counts, unsupported Bloom types, and unknown fields before creation or
alteration. Transform nulls remain null. User fields cannot shadow managed
names. The supported matrices below are exhaustive for this change;
Iceberg supporting a broader type set does not implicitly expose it in Wyrd.

### Supported layout types and schema edits

Types below are canonical stored types after the existing Arrow-to-Iceberg
normalization. For example, accepted narrow signed integer declarations
normalize to `Int32`, and accepted large-string declarations to `Utf8`.
This does not add new schema declaration spellings or unsigned coercions.

| Operation | Allowed canonical source types |
| --- | --- |
| Required `hour` / `day` partition | Required `Timestamp(Microsecond, UTC)` or `Timestamp(Nanosecond, UTC)` on the declared time column |
| Additional `identity` partition | `Utf8`, `Int32`, `Int64` |
| Additional `bucket` partition | `Utf8`, `Int32`, `Int64` |
| Additional `truncate` partition | `Utf8`, `Int32`, `Int64` |
| Bloom column | `Utf8`, `Int32`, `Int64`, `Binary`, `FixedSizeBinary` |

Every other type/operation pairing is refused. A timezone-naive time column is
not accepted as the required UTC time axis. Nullable additional partition
sources are allowed and retain null partition values. Integer truncation uses
Iceberg's behavior for negative numbers; string width counts Unicode code
points. Existing bucket, width, field-count and batch-fanout bounds still apply.
These are eligibility rules, not a guarantee that every query gains pruning.

Schema edits are limited to adding nullable top-level custom columns using
existing supported field declarations, and renaming top-level custom columns
without changing their IDs, types, nullability or stored values. New columns
read as null in older files. Rename updates layout references to the preserved
field ID and returns its new name. Refuse name collisions, ambiguous/chained
renames, additions of required columns, deletions, type changes (including
widening), nullability changes, nested edits, and edits to canonical system or
managed fields. These limits do not prevent layout changes on eligible system
columns. No historical data rewrite is implied by an allowed schema edit.

### Explicit table alteration contract

Reuse the existing registration, catalog and schema/layout validation owners.
Creation remains `POST /v1/bifrost/tables`; it never implicitly alters a table.
Alteration uses `PATCH /v1/bifrost/tables/{namespace}/{name}` in the authenticated
tenant. Query and ingest grants cannot authorize it. Describe and successful
alteration return the existing resolved table description plus a positive
integer `revision`; creation starts at revision 1. This configuration revision
is distinct from Iceberg snapshots, partition-spec IDs and sort-order IDs.

The PATCH body has these fields; unknown fields and explicit nulls are refused:

| Field | Contract |
| --- | --- |
| `expected_revision` | Required positive integer matching the current table configuration |
| `physical_layout` | Optional partial object containing `partition_by`, `sort_keys`, and/or `bloom_columns`; omitted lists are preserved and supplied lists replace the complete list |
| `add_columns` | Optional array using the existing field-declaration shape, limited to nullable top-level custom columns |
| `rename_columns` | Optional array of objects with string `from` and `to` names, limited to top-level custom columns |

The time-column identity is immutable after creation. At least one effective
change is required. Validate the full post-edit schema and layout, including
name/field dependencies, before committing. Apply the whole request atomically
against `expected_revision`; one successful edit increments `revision` once.
Concurrent updates with a stale revision receive HTTP 409 and change nothing.
Malformed or unsupported edits receive HTTP 400; missing administration grants
receive HTTP 403. Use the existing typed error catalog and tenant-safe lookup
behavior. A successful PATCH returns HTTP 200 with the resolved description
and committed revision. Failed updates never refresh caches or publish a new
configuration. Accepted batches keep their original schema/layout identity;
new admission observes the committed configuration under the shared owner.

Example proposed layout:

```json
{
  "physical_layout": {
    "time_column": "event_time",
    "partition_by": [
      {"column": "event_time", "transform": "day"},
      {"column": "customer_id", "transform": "bucket", "buckets": 32}
    ],
    "sort_keys": [
      {"column": "customer_id", "direction": "asc", "null_order": "last"},
      {"column": "event_time", "direction": "desc", "null_order": "last"}
    ],
    "bloom_columns": ["customer_id", "order_id"]
  }
}
```

For that declaration, the first partition entry is the required time field;
the bucket entry is an explicitly requested additional dimension, not a default. Reject excessive fanout before
WAL acceptance; never acknowledge a partly accepted batch. Preserve existing
batch idempotency and logical-record atomicity.

Identity partitioning is explicit user configuration, never inferred from
column names. Prefix truncation does not bound the number of distinct
partitions. Bucket counts bound one field, but combinations of fields still
multiply. The write fanout limit is a per-batch resource bound, not a promise
of bounded total table cardinality.

## Correlation-aware Bloom policy

The shared seven-field envelope is mandatory. Its Bloom coverage is not.
System and dynamic tables use one validator/configuration mechanism while
each table declares query-driven defaults. An empty Bloom set is valid and
must not weaken permissions, attribution validation, or exact query results.

| Access pattern | Indexing recommendation |
| --- | --- |
| Record/execution point lookup | Bloom the identifiers used by that table's lookup predicates. |
| Workload, subject, Run, binding, or group filter | Include the relevant columns when these are declared core access paths. |
| Time range | Use time partitions, bounds/statistics, and exact row filtering. |
| Equality or finite `IN` | Eligible for Bloom membership pruning when supported by the reader. |
| Join | Use explicit correlation keys; Blooms help only if usable predicates or supported runtime filters reach the scan. |
| Account/request investigation | Configure the corresponding account/request columns where this is a table's access pattern; do not impose them on every table. |

A join key alone does not justify a Bloom, and a Bloom is not a join index.
Values present in nearly every row group offer little pruning even with an
accurate filter. High-cardinality IDs can prune well when their queried values
occur in few groups. Min/max and dictionaries remain complementary tools.

### Rationale for the complete default sets

| Table | Intended primary access paths |
| --- | --- |
| Spans/logs | Trace retrieval, exact workload/component views, application Run, and Service filtering. |
| Metrics | Metric name, Service, workload, and component; aggregated points need no invented Run/account/request lookup. |
| Gateway calls | Individual call and its workload/component/Run context. |
| Drift observations | Subject plus series/time windows, and explicit workload-scoped filtering. |
| Verification results/inputs | Result or execution, application context, Verifier, binding history, and grouped source discovery. |
| Drift result features | Detail retrieval through exact result ID. |
| Audit | Account, request, and exact-resource investigation. |

`deployment_environment`, log severity, metric type, and audit operation remain
stored/queryable but are not default Bloom columns. Their common values may
occur throughout row groups; clustering/statistics/dictionaries and exact
filters remain available. Account/request Blooms outside audit are opt-in.
These are workload-informed defaults, not measured universal optima.

### Span ID decision

Do not enable `span_id` Blooms by default on spans or logs. Keep the column,
trace/span correlation, validation, and tenant configuration support. Existing
trace checks fetch by trace ID plus time and select spans afterward, so a
span-ID Bloom does not accelerate that path. High cardinality can make span
IDs excellent point-lookup candidates, but uniqueness alone does not justify
paying the cost when queries do not use them.

Tenants with frequent individual-span/log lookups may enable `span_id`.
For `trace_id = ... AND span_id = ...`, benefit depends on how much additional
work remains after trace pruning. A join on the span field alone does not
promise Bloom pushdown or an exact composite-key lookup.

### Bloom order, clustering, and Service/environment queries

| Mechanism | Meaning | Cost/benefit |
| --- | --- | --- |
| Clustering here | Declared sort order groups environment/Service rows inside written files | Write/compaction sorting can tighten file/group/page statistics; it creates no per-Service partition. |
| Bloom | Independent membership filter per column chunk | Adds hashing/memory/storage; can reject impossible equality/finite-IN matches. |

Bloom list order has no leading-column requirement, priority, or composite-key
meaning. `[trace_id, service_name]` and `[service_name, trace_id]` have the same
membership semantics. Physical sort-key order does matter. Independent positive
filters do not prove a pair occurs in the same row; exact predicates remain.

For example:

```sql
SELECT *
FROM bifrost.traces.spans
WHERE service_name = $1
  AND deployment_environment = $2;
```

The proposed environment/Service sort may improve statistical pruning. The
Service Bloom can reject groups without that Service; the environment predicate
still works through statistics and exact filtering without its own Bloom.
`SELECT *` requests every output column, increasing potential decode/I/O cost.
No time predicate means retained hourly partitions remain candidates; the
engine must not silently inject a recent-window filter. When the query has a
known time window, add explicit timestamp bounds for time-partition pruning.
This discussion changes no partition/sort declaration.

Local comparison: OpenObserve `69bd367d9` defaults its Bloom field list to
`trace_id` and `session_id` when default-index fields are enabled, not `span_id`.
It permits configurable `span_id` under its stream-index rules. Native Parquet
Bloom writing has a separate flag, default false; do not confuse those filters
with its other secondary-index Bloom mechanisms. Inspected sources are
`src/config/src/config.rs` default fields/feature flags,
`src/schema/src/lib.rs` field validation, and
`src/config/src/utils/parquet.rs` writer gating. This is a concrete comparison,
not an industry-frequency claim.

### Verification results: eight default columns

```json
{
  "bloom_columns": [
    "workload_card_uid",
    "subject_card_uid",
    "run_id",
    "result_id",
    "execution_id",
    "verifier_card_uid",
    "binding_id",
    "group_id"
  ]
}
```

These cover the intended workload/release, component, Run, result, execution,
Verifier, binding-history, and grouped-verification access paths. Current
runtime evidence establishes result-ID lookup and grouped filters on workload,
subject/Verifier, and group; execution, Run, and binding defaults also serve the
intended explicit correlation/history queries. This is not a claim that every
column has measured equal benefit.

`sub_principal_id`, `act_principal_id`, and `wyrd_request_id` remain required
stored columns but are optional Bloom additions for verification results.
They may be default Blooms on another table whose queries use them. The eight
verification defaults are configurable, not an immutable performance floor.

### Cost and evidence

Each enabled column has a separate filter per row group. More filters add
hashing work, writer memory, and stored bytes. In the inspected writer, FPP
0.01 and the implicit NDV estimate based on the 1,048,576-row maximum initialize
approximately 2 MiB per enabled column. Eleven columns therefore mean about
22 MiB of initial bitsets per active row-group writer; eight mean about 16 MiB,
excluding other encoder state. Concurrent writers multiply that cost. These
are source-derived calculations, not benchmark measurements or stored-size
predictions. Folding can shrink low-cardinality serialized filters after the
insertion work has occurred.

The pinned reader loads relevant predicate-column filters for surviving row
groups, not every configured filter merely because columns are projected.
Keep the existing FPP/encoding recipe; this decision does not silently change
NDV sizing or dictionary-filter behavior. Measure write memory/CPU, stored
filter bytes, and scan pruning on representative data before claiming gains.
Missing or disabled filters always fall back safely to ordinary reads.


## Shared writer and maintenance policy

All numeric/encoding values in this table are existing Wyrd or pinned-Parquet
settings, recorded explicitly rather than new tuning. The 1 GiB compaction
default already applies through the shared Forge owner; existing authorized
table/deployment override semantics remain.

| Setting | Value |
| --- | --- |
| Compression | ZSTD level 3 |
| Encoded row-group target | 134,217,728 bytes; soft bound |
| Maximum rows per row group | 1,048,576 |
| Write batch size | 1,024 rows |
| Data-page size target | 1,048,576 bytes |
| Data-page row limit | 20,000 rows |
| Dictionary-page limit | 262,144 bytes |
| Dictionary encoding | Enabled except the declared time column |
| Time-column encoding | `DELTA_BINARY_PACKED` |
| Parquet statistics | Page |
| Offset indexes | Enabled |
| Bloom false-positive probability | 0.01 |
| Default published compaction target | 1,073,741,824 bytes |

Both row-count and encoded-byte row-group bounds apply. File and byte targets
are targets, not minimum file sizes. Do not combine records across partition
boundaries merely to meet a target. Existing authorized compaction target/type
settings remain under their owning API and operational policy; this does not
introduce per-user compression or Bloom-probability tuning.

Bloom membership is an optimization for equality/finite membership. It is not
an exact index, uniqueness constraint, range index, or authorization check.
Files without a particular historical Bloom/statistics artifact remain
readable through safe fallback. Page/row-group pruning never replaces exact
row filtering.

## Durable lifecycle and evolution

```text
validate
  -> resolve schema/layout revision
  -> partition and preserve attribution
  -> durable acceptance
  -> publish
  -> query
  -> compact
  -> retain/expire under existing policy
```

The durable partition identity contains the complete typed partition tuple and
its layout/spec revision, including null values. Scribe routing, WAL, seals,
manifests, publication, Oracle/peer reads, and Forge must agree on it. Preserve
time bounds independently and consistently. Supporting multiple fields is a
coordinated engine change, not just removal of catalog validation.

Explicit schema/layout alteration is part of the proposal. Re-registration
remains idempotent and does not silently alter an existing table. Required
evolution guarantees:

- Preserve field IDs on renames and compatible evolution; never reuse a
  retired ID for a different meaning.
- Assign new partition-spec and sort-order revisions when changed.
- Bind every accepted batch to its schema/layout revision. A retry preserves
  that binding across alteration, restart, and publication.
- Plan old and new files using their own retained specifications. Declaring a
  new sort order does not claim old files have been resorted.
- Historical rewriting is explicit maintenance, not an assumed consequence
  of metadata evolution. No old partition disappears from reads.
- Maintain snapshot/query pins and safe retention. Snapshot retention is not
  row-data TTL; this proposal adds no new automatic row-retention default.

The supported schema edits and alteration contract above define this change;
future type promotions or deletions require a separate explicit extension.

## Correlation and queries across tables

| Question | Correlation |
| --- | --- |
| Everything for one exact workload version | `workload_card_uid` |
| Everything across a Service's releases | Resolve registered root UIDs, then filter `workload_card_uid` |
| One component across workloads | `subject_card_uid` |
| One workload's application Run | `workload_card_uid` plus `run_id` |
| One distributed trace | `trace_id` on trace-bearing surfaces |
| Logs for one span | `trace_id` plus `span_id` |
| One verification execution | `execution_id` on execution-bearing surfaces |
| One verification group | `workload_card_uid` plus `group_id` |
| Records and audit for one Wyrd request | `wyrd_request_id` |

The seven envelope fields are universal. Trace, span, execution, group,
Verifier, and binding fields remain typed domain correlations on the surfaces
that define them; they are not invented for every metric/custom row. Do not
manufacture an exemplar or one trace for a metric aggregating many executions.

Joining only on workload does not identify one execution and can produce a
many-to-many result. Use the specific execution/Run/trace keys required by the
question, or an existence filter when selecting related records. Correlation
does not create cross-table foreign-key write dependencies or infer ownership
from labels. Registry family resolution occurs through its existing owner,
not a new Oracle/Postgres join.

Every referenced table must pass ordinary query authorization. A common key
grants no access. Card emission/verification scope does not become an implicit
warehouse row-read policy.

## Whole-trace verification decision

A trace check always takes the entire trace as its evidence, under ordinary
accepted tenant/table authorization. A Service/Agent or component binding
identifies what is judged and owns the result; it does not narrow the trace
input. Do not add a workload-only trace mode or a trace/workload scope switch.

A span tagged with `subject_card_uid` concerns that exact Card. This tag does
not turn that span into the whole evidence input or exclude spans tagged to
other Cards, workloads, accounts, or no Card. A trace spanning checkout and
fraud-service remains one trace input. Its result can belong to checkout's
binding while source spans retain their own original identities.

The authored check's span selector may inspect particular spans, relationships,
counts, order, or attributes within the complete trace document. That selector
is assertion logic, not an implicit workload/subject predicate on the base
trace read. The issuer's observation graph limit does not create a new
warehouse row-read policy or grant access to otherwise unauthorized tables.

The current reader applies fixed acceptance-relative time bounds and a
10,000-span ceiling. The target reader selects every retained, authorized span
for the exact trace ID without that implicit time window. Retain explicit
projection, deterministic ordering, admission limits and the span ceiling;
exceeding the ceiling is an explicit evidence-loading failure, never a
silently truncated trace. Trace-ID Blooms and ordinary pruning can accelerate
the read but cannot change which spans are eligible.

"Entire trace" means all authorized spans visible to that attempt's successful
trace read, not proof that every distributed process has finished exporting.
An ended root or a quiet interval does not establish completeness. Do not add
a completion protocol, inferred completion flag or mandatory settling delay.
Read the same trace once per verification attempt and reuse that fetched
collection for its trace checks under the same accepted authority. A later
retry performs a fresh read; no cross-execution cache or immutable historical
replay guarantee is introduced. Later spans do not silently revise a completed
result; a new verification may judge the newer evidence. Keep the existing
recorded execution/result times; they are not trace-completeness timestamps.

### Current missing-trace and retry behavior

The current trace path has no automatic evidence-availability requeue:

1. Zero rows or a missing span table yields `TraceUnavailable::NotYetLanded`.
2. The trace check converts that to an inconclusive requirement, as it also
   does for its other unavailable/error categories.
3. The engine returns a successful outcome containing that judgment; queued
   execution completes and publishes the result rather than retrying.
4. SDK waiting polls the same execution and returns that inconclusive result;
   it does not re-execute the verification.

The queue separately supports retries for typed `EngineError::EvidenceUnavailable`
and interruption. Its inspected default is three total attempts with 30- and
60-second backoff; this is not evidence that trace ingestion delay already
uses that mechanism. Trace checks currently catch their unavailable condition
before it reaches the queue classifier. Transport retry/idempotency also does
not mean rerunning a completed inconclusive execution.

### Agreed bounded missing-trace policy

The fresh implementation must connect absent/not-yet-landed traces to bounded
evidence-availability retry instead of immediately completing inconclusive.
For queued execution, defer the same accepted execution through the existing
queue; preserve its ID, accepted inputs/authority and idempotency.
Reuse the existing retry policy: three total attempts with 30- and 60-second
backoff. Retain the configured timeout of each engine attempt, currently five
minutes by default. This is not a five-minute total lifetime across attempts
or queue waiting. Do not add an absolute accepted-execution expiry or a new
timeout setting as part of this change. Waiting
must not hold a worker in a busy loop or create a new submission/token.

If the trace becomes available, retry whole-trace evaluation under the same
accepted authority and ordinary query permissions. If evidence is still
unavailable when the availability retry budget is exhausted, finish that trace check as
inconclusive with an explicit evidence-unavailable reason, rather than treating
it as a failed check or transport failure. Preserve the Verifier's existing
decision rule over all requirements: a trace-only verification is inconclusive,
but an unrelated definite pass/fail is not overwritten by an execution-wide
inconclusive override. Persist only the final completed result under the
existing execution identity and durability contract.

Do not retry a failed judgment until it passes. Invalid evidence,
authorization failures, or an oversized trace are not automatically transient
not-yet-landed conditions. Preserve existing classification for other failure
categories instead of treating every read failure as ingestion delay.

Direct execution retains its existing bounded request/engine lifetime, currently
60 seconds by default. It evaluates once; an absent trace produces an explicit
inconclusive trace check under the normal decision rule. It neither waits for
trace completeness nor silently enqueues work. Queued verification is the
existing path for callers needing missing-evidence retry. Attempt timeout
classification otherwise retains the current engine/queue behavior; an
execution timeout is not reclassified as a missing trace.

A nonempty visible trace is evaluated even if more spans may arrive later.
Do not retry failed judgments until they pass, or rerun completed results when
late spans arrive. Applications may invoke verification after their operation
finishes and exports telemetry; export success still does not prove warehouse
read visibility or distributed completeness.


## Public catalog, fresh baseline, and verification

### Public catalog name

The canonical public catalog is `bifrost`, including `bifrost.traces.spans`,
`bifrost.logs.records`, `bifrost.metrics.points`, all other built-ins,
`bifrost.datasets.*`, and operational namespaces. The old public catalog
spelling `vala` is removed, with no alias or dual-catalog mode. Internal Vala
crate and domain ownership names are outside this SQL-catalog rename.

This reaches TableRef parsing/rendering, default SQL catalogs, table lookup,
physical tenant binding, permission scope construction/authoring, query
builders, SDKs, examples, CLI/MCP, operational output, and docs. It is not a
label-only rename. A hardcoded old catalog in verification read grants would
break authorization even if query strings were renamed.

### Fresh v0.3.0 baseline

No historical data, credentials, principal IDs, field IDs, old catalogs,
old tokens, or accepted queue requires release migration. Development
installations may recreate state directly under the new contract. Do not add
backfill jobs, collision maps, legacy credential conversion, upgrade scripts,
or dual readers. This document performs no destructive reset.

Stable field IDs, accepted-batch revision binding, mixed-spec reads, and safe
retention still apply to future operations after the new baseline is created.
Adding a partition dimension later can create a new spec for future writes;
old files remain queryable under their old specs. Rewriting that in-baseline
history is explicit optional maintenance, not automatic or required for
correctness. This is distinct from migrating a pre-change installation.

### Verification-engine adjustments

| Owning path | Required adjustment and invariant |
| --- | --- |
| Public verification requests/results and SDKs | Use the new subject/workload terminology consistently; keep Verifier, subject, owner, execution, Run, and group identities distinct. |
| Input/result/drift-feature ledgers and writers | One workload column and shared subject/account envelope; preserve validated binding ownership and originating authority. |
| Trace reader and selector projection | Use the new catalog and correlation contract; always provide the complete authorized trace input; never add an implicit workload/component filter. Preserve authored selectors over that trace. |
| Drift/window readers | Filter exact subject, series, and time; workload context cannot replace subject identity. |
| Group-end readers | Filter workload, group, and subject/Verifier tuples without merging different binding occurrences. |
| System/table-scoped read authority | Construct grants for the resolved `bifrost` tables; remove hardcoded old-catalog fallback. |
| Direct, queued, and hosted execution | Preserve the existing frozen principal/delegation authority plus workload context; do not rebuild grants from bare principal IDs. |
| Accepted work and table evolution | Retain accepted schema/layout binding, retry identity, and evidence scope across restart and alteration. |

The current engine already freezes accepted principal/delegation authority.
This change extends/preserves its workload context; it does not justify adding
bare-ID role recovery or another engine. Public schema, validation, codecs,
SQL, projections, and SDK journeys must change together. Rootless privileged
Users and exact binding/graph authorization remain covered by the auth contract.

### Verification evidence access and lookup performance

The refactor includes profiling and optimizing internal evidence reads. The
objective is selective, low-overhead retrieval for the verifier's actual
inputs, not merely faster interactive warehouse SQL. Keep one Bifrost query
owner and preserve accepted caller/tenant/table authorization. No separate
point-lookup database or new durable index is prescribed without evidence.

Current inspected orchestration uses several paths:

| Path | Current data access | Existing selectivity/limits |
| --- | --- | --- |
| Inline JSON | Supplied input; no warehouse read | Declared input validation |
| Files/Artifacts | Digest-addressed or authorized Artifact storage reads | Length/hash/format/citation validation |
| Trace slot | Each check queries span columns for supplied trace ID | Equality, fixed time bounds, ordering, 10,000-span ceiling; no observed per-execution fetch deduplication |
| Drift/samples | Aggregate statements over observations | Subject plus series plus frozen time window |
| Group sources | Combined input/result query | Workload plus group plus subject/Verifier tuples; `UNION ALL` batches configured sources |
| Dynamic/custom tables | Declared Agent `bifrost.query` tool issues supplied SELECT through ordinary query service | Table/column authorization, deadline, row and byte ceilings; no partial collected result on overflow |
| Completed result | Postgres status resolves canonical result ID, then reads outcome/Run from results | Exact result-ID predicate and `LIMIT 1`; no time bound in the inspected result query |

A generic dynamic-table InputSlot that discovers rows automatically is not an
existing capability. Inline/storage inputs and Agent-tool SQL must not be
misdescribed as identical warehouse lookups. Direct and queued verification
share the hosted engine under frozen accepted authority.

Existing Oracle acceleration includes cached published cuts and Parquet
footers, Iceberg manifest/file pruning, hot-file time bounds, column projection,
row-group statistics/Blooms, page-index pruning, nested-leaf projection, and
static/dynamic live-source filtering. Each statement still goes through SQL
planning/optimization. Typed parameters become literals before optimization;
that is not a reusable prepared-plan cache. These mechanisms do not establish
constant-time key lookup or an immutable evidence snapshot for a whole Verifier.

Required optimization work and boundaries:

| Concern | Required investigation/behavior |
| --- | --- |
| Lookup geometry | Document each path's exact equality/range predicates, projection, expected cardinality, table layout, and candidate files/groups. |
| Trusted bounds | Use authoritative execution/input/source metadata for additional time/source pruning where available. Never guess a short time window that drops valid older evidence. |
| Dynamic SQL | Preserve the declared SQL semantics and normal query validation; push eligible filters/projections through the existing owner. Do not infer authorization or a workload filter from a binding alone. |
| Repeated evidence reads | Measure duplicate trace/other reads; reuse identical reads only under an explicit accepted authority, selection, and read-view consistency scope. No unscoped cross-caller cache. |
| Group/window reads | Preserve existing combined-source batching; reduce round trips and projections where equivalent, without merging distinct binding occurrences or statistical windows. |
| Default indexes | Review all system tables against proven/intended access paths. Validate each query-driven default against observed selectivity/cost during implementation; a shared column alone is not justification for indexing it. |
| Measurement | Separate cut/source discovery, planning/admission, file/row-group/page scanning, decoding, and engine orchestration; record files/bytes scanned, pruning, duplicate query count, and end-to-end latency. |
| Correctness | Compare authorized evidence and outcomes across inline/storage/dynamic/trace/window/group paths, live/hot/published sources, retries, and accepted layout revisions. |

Current per-query published/live read consistency is not whole-execution
snapshot consistency. Any new read deduplication/snapshot contract must be
settled alongside trace late-arrival/reproducibility semantics; it is not a
private cache choice that may silently change judgments.

The documented 60-second direct-verification execution envelope is not a
millisecond evidence-lookup SLO. No source inspection or this draft measures
actual lookup latency. Establish representative baselines and explicit
acceptance budgets before promising "super fast" or introducing additional
indexes/caches. Production table cardinality, history length, and skew matter.

### Separate final docsite task

The future implementation plan must have one separate, dedicated docsite task
as its final implementation task, after runtime/contracts/SDK/example behavior
settles and before integrated change review. It covers:

- Fresh v0.3.0 setup/reset and the single `bifrost` catalog.
- Card authoring without space and stable Service-account credentials.
- Ordinary User/admin auth, optional workload context, delegation, and the
  seven attribution fields with concrete account/workload/subject examples.
- Full spans/logs/metrics schemas, extraction, and native/OTLP behavior.
- System defaults, tenant layout permissions, shared system/dynamic rules,
  partition/Bloom configuration, alteration, limits, and evolution.
- Service/subject/Run/trace/verification/group/request query and correlation
  examples, including evidence selection and verification-engine behavior.
- Existing writer settings distinguished from new configuration capabilities
  and any separately approved default changes.

Update the docsite's owning pages, reference examples, links, and navigation;
verify the owning documentation and example checks against the actual shipped
contract. No implementation task file is created by this specification.

### Acceptance evidence

Required evidence includes exact resolved layout/description equality,
tenant-authorized system/dynamic configuration, unchanged canonical schemas,
persistence across restart/cache refresh, transform/type refusal,
composite/null partition routing and fanout atomicity, original audit/outbox
attribution, accepted-batch retry across alteration, historical in-baseline
spec reads, compaction/sort enforcement, and representative pruning/file sizes.

Fresh-baseline journeys must cover `bifrost` permission scopes and all direct,
queued, grouped, drift, and trace verification paths. No measured throughput
or latency gain is claimed. No historical migration journey is required.

## Research and inspected owners

| Source | Inspected fact |
| --- | --- |
| [System table registry](../../../../crates/vala/vala-bifrost-redux/src/tables/mod.rs) | Nine built-ins and the current managed floor; this proposal replaces mandatory floor injection with configurable per-table sets. |
| [Managed columns](../../../../crates/vala/vala-bifrost-redux/src/tables/managed_columns.rs) | Dynamic and canonical observations currently share an envelope; audit currently differs. |
| [Layout owner](../../../../crates/vala/vala-bifrost-redux/src/catalog/layout.rs) | One-field time restriction is deliberate Wyrd design; typed partition identity; existing sort/fanout constraints. |
| [Catalog](../../../../crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs) | Current registration rejects layout/schema changes; evolution is new work. |
| [Public registration](../../../../crates/wyrd-spec/src/vala/api.rs) | Existing registration/layout and compaction owners. |
| [Writer policy](../../../../crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs) | Shared compression, row-group, dictionary, statistics, and Bloom recipe. |
| [Audit table](../../../../crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs) | Original decision identity, current request field, and audit target. |
| [Unresolved attribution](../../../../crates/wyrd-spec/src/auth/attribution.rs) | Existing unresolved-principal sentinel has no authority. |
| [Trace reader](../../../../crates/wyrd/wyrd-server/src/verification/hosted/traces.rs) | Trace/time query and evidence limits, independent of binding root. |
| [Hosted engine](../../../../crates/wyrd/wyrd-server/src/verification/hosted/blocks.rs) | Direct and queued executions share accepted-authority evidence/tool owners. |
| [Trace check](../../../../crates/vala/vala-verify/src/blocks/engine.rs), [queue runner](../../../../crates/wyrd/wyrd-server/src/verification/runner.rs), and [queue transitions](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs) | Current trace unavailability becomes inconclusive before the separate retry classifier. |
| [Drift reads](../../../../crates/wyrd/wyrd-server/src/verification/drift.rs) and [group reads](../../../../crates/wyrd/wyrd-server/src/verification/groups.rs) | Window predicates and batched source selection. |
| [Result service](../../../../crates/wyrd/wyrd-server/src/components/verification/service.rs) | Status-to-result-ID lookup and narrow result projection. |
| [Oracle planning](../../../../crates/vala/vala-bifrost-redux/src/oracle/mod.rs), [scans](../../../../crates/vala/vala-bifrost-redux/src/oracle/exec.rs), and [live reads](../../../../crates/vala/vala-bifrost-redux/src/oracle/live.rs) | Existing planning, source selection, pushdown, caches, and pruning boundaries. |
| [Bifrost authority](../../../../architecture/bifrost-design.md) | Tenant physical separation and Scribe/Oracle/Forge boundaries. |

Pinned inspected dependencies: Iceberg 0.10.0, DataFusion 55.1.0, Arrow/Parquet
60.0.0. The numeric page/row limits above resolve the pinned Parquet defaults;
the remaining writer settings follow Wyrd's owning writer policy. Verify the
actual dependency cone and writer output during implementation rather than
assuming every upstream capability already works in Wyrd.

External grounding:

- [Iceberg partition transforms](https://iceberg.apache.org/spec/#partition-transforms)
  and [partition specification](https://iceberg.apache.org/spec/#partitioning):
  multiple typed fields, transform compatibility, and null semantics.
- [Iceberg evolution](https://iceberg.apache.org/docs/latest/evolution/):
  field-ID-preserving renames and coexistence of historical layouts.
- [Iceberg file sizing](https://iceberg.apache.org/docs/latest/spark-writes/#controlling-file-sizes):
  files cannot cross partition boundaries.
- [Parquet Blooms](https://parquet.apache.org/docs/file-format/bloomfilter/)
  and [DataFusion pruning](https://datafusion.apache.org/blog/2025/03/20/parquet-pruning/):
  membership, statistics, and page pruning are distinct from partitioning.

Local comparison: OpenObserve checkout
`/home/thorrester/Documents/GitHub/openobserve`, commit `69bd367d9`, defaults
logs/metrics/traces to hourly time partitions with no additional keys. Its
configured field vector supports multiple value/hash/prefix partitions.
Inspected owners: `src/core/src/ingestion/mod.rs::get_write_partition_key`,
`src/config/src/meta/stream.rs::StreamPartition`, and
`src/schema/src/lib.rs` settings validation. This informs the separation of
conservative defaults from configurable capabilities; Wyrd does not copy its
directory format or hashing algorithm.

## Decision closure

The maintainer agreed the table-editing contracts and visible-trace behavior
recorded in revision 12. No material decision remains open in this supplement.
The maintainer approved the complete revision for task planning. Existing partition, sort, Bloom and writer defaults are unchanged.

Performance validation remains representative before/after measurement with
correctness evidence, not an invented numerical lookup SLO. Partial arrival
is an explicit limit of the evidence available at evaluation time, not a
missing trace-completion feature. Runtime layout evolution remains required
within the fresh baseline; it is separate from an upgrade migration.

## Revision history

| Spec revision | Status | Change |
| --- | --- | --- |
| 4 | draft | Records the unified system/dynamic table contract, seven-field envelope, all nine system layouts, custom partition/sort/Bloom limits, exact writer recipe, evolution, correlation, source evidence, and remaining contract questions. |
| 5 | draft | Records one system/dynamic rule set with authorized tenant partition/Bloom overrides, classifies writer numbers as existing, uses the bifrost catalog and fresh v0.3.0 baseline, and adds verification impacts plus a separate final docsite task. New partition defaults remain pending explicit approval. |
| 6 | draft | Removes the global mandatory Bloom floor, records eight configurable verification-result defaults and optional account/request filters, and distinguishes shared correlation schema from per-table indexing and join pruning. Existing partition and sort proposals are unchanged. |
| 7 | draft | Records current verifier evidence access paths, existing Oracle acceleration, concrete lookup gaps, and required query/performance work; preserves table defaults pending their query-driven review. |
| 8 | draft | Records lean per-table defaults with span_id opt-in for spans/logs, independent Bloom ordering, clustering versus membership filtering, Service/environment query behavior, and the qualified OpenObserve comparison. Partition/sort declarations are unchanged. |
| 9 | draft | Fixes whole-trace input for every trace check regardless of Card/workload tags or binding subject; removes workload-only selection. Late-arrival/read consistency remains open; no partition/Bloom changes. |
| 10 | draft | Records actual terminal-inconclusive trace-not-landed behavior versus the separate queue retry mechanism, a pending bounded-retry recommendation, and agreed measurement-based performance validation. Layout defaults are unchanged. |
| 11 | draft | Records agreed hourly/daily defaults, safe editing/revision-conflict principles, and bounded queued retries for absent traces with terminal inconclusive exhaustion. Partial-trace/read-view/direct-call details and exact alteration matrices remain open. |
| 12 | approved | Closes the supported layout/schema-edit matrices, existing DDL permission and atomic revisioned PATCH contract. Defines visible-trace evidence, per-attempt reuse, direct inconclusive behavior and late-arrival limits; preserves per-attempt queue timeouts. Defaults are unchanged. |

# Bifrost Design

**Version:** v1

Bifrost is Wyrd's high-throughput distributed OLAP warehouse. It is both the
public analytical surface and the storage substrate Vala uses for Wyrd
observations. This document is the authority for Bifrost internals: physical
table identity, Scribe ingestion, Oracle query execution, Forge maintenance,
resource ownership, durability, recovery, and public analytical behavior.

`architecture/wyrd-design.md` remains authority for Wyrd doctrine, Cards, and
cross-service contracts. Bifrost is infrastructure, never a Card kind. Decision
history lives in git; this document states one architecture.

## System boundary

`wyrd-server` is the only network-serving surface. It owns listeners, TLS,
authentication, authorization, request limits, boot readiness, and server
lifecycle for Bifrost HTTP, gRPC, and MCP operations. Production and the test
harness start the same complete graph through one `AppState::start`; the
resulting state is immutable. Each engine owns its caches and refresh tasks
and returns them as startup workers that the one server supervisor runs: Bifrost
owns the only Scribe outbox, and Verification owns its hosted blocks, queued
submission outbox, and Operator keys. Shutdown stops transports and new claims,
then drains the gateway, MCP, and verification (runner, then the Monty pool is
closed and its workers reaped), the Scribe outbox, Bifrost, the verifier
submission outbox, and telemetry, in that order, under one deadline. Vala owns
the Bifrost engine and its three cohesive subsystems:

- **Scribe** owns pod-local ingestion, WAL durability, active and immutable
  rows, durable local staging, hot-object publication, and bounded live-tail
  service behavior.
- **Oracle** owns pinned published query cuts, admission, interactive and distributed
  execution, live Scribe routing, terminal-safe streaming, and read-audit
  acceptance.
- **Forge** owns Scribe-hot promotion, Iceberg maintenance scheduling,
  resource and lease fencing, compaction publication, reconciliation,
  retention, and garbage collection.

Apache Iceberg-managed Parquet in object storage is the analytical system of
record. Each node reaches it through one `object_store` client for the
configured backend behind one node-wide request limit; Scribe, Oracle, Forge,
and the Iceberg catalog all use it, and timeouts and retries are the cloud
client's own. `wyrd-storage` keeps its separate client for card and artifact
storage. Postgres is the tenant-isolated catalog and durable control plane.
DataFusion is the vectorized execution engine. Managed compaction and Iceberg
components perform bounded computation behind Vala-owned contracts; they do not
authenticate callers, serve the network, own tenant authority, or commit
durable Wyrd state independently.

Each role on a pod renews its own `vala.cluster_nodes` heartbeat, and one
process-wide membership poller reads the live set for every role. Internal
Scribe writes route to a peer by its Scribe capability alone, so a pod without
Oracle still reaches Scribe. At the entrance, Gate admits only a verified
principal, OTLP authenticates its API key before decoding a body, and every
Bifrost SQL path uses the repository tenant capability, except the existing
`OracleTableCuts` single-statement owner documented in the security posture.
That exception retains verified tenant-local RLS and avoids extra transaction
round trips; it permits no arbitrary bare-pool workflow.

## Table and row identity

Every Bifrost table has seven server-owned managed columns: nullable
workload_card_uid (Service only), subject_card_uid and run_id; required
sub_principal_id, act_principal_id, wyrd_request_id and wyrd_ingested_at.
The request ID is the existing request/batch-fence correlator. The ingestion
instant is taken once at admission from the PostgreSQL-anchored clock and
retained across retry. Callers never supply managed identity as user fields.

The nine system tables (spans, logs, metrics, gateway calls, drift
observations, verification results, execution inputs, drift result features
and audit) additionally share six nullable Utf8 dimensions:
service_name, service_version, deployment_environment, subject_kind,
subject_name and subject_version. Registered UIDs determine identity labels;
accepted workload context determines environment. Existing signal columns occur
once. Dynamic tables keep the seven universal fields without these mandatory
dimensions; audit targets remain separate from observation subjects.

All eight use environment ASC NULLS FIRST, Service name ASC NULLS FIRST,
Service version ASC NULLS FIRST and their existing time axis DESC NULLS LAST.
Time-only partitions remain hourly for telemetry/gateway and daily for
verification/drift. Creation defaults combine service_name, subject_name,
workload_card_uid, subject_card_uid and run_id Blooms with signal-specific lookup
keys; audit retains its accountability defaults. Ten result Blooms include
result_id, execution_id, verifier_card_uid, binding_id and group_id. These are
complete replaceable creation defaults, never a hidden validation floor.
No environment/kind/version Bloom or identity partition is implied. The exact
ledger is in the approved service-identity-and-telemetry storage supplement;
normal describe exposes the resolved immutable definition. Preserve existing
encoding/FPP, tenant overrides, table_version and retained-file semantics.

The column names, types, stable ids, and order are declared once, in
`vala_bifrost_redux::tables::managed_columns::MANAGED_COLUMNS`; every physical
schema and every reserved-name check derives from that declaration.
`wyrd_ingested_at` comes from the database clock, so it is comparable across
replicas to within one anchor's monotonic drift, but it is the admission instant, not a commit order, and is not an
incremental-read checkpoint.

Every table has exactly one time column: a non-null zoned `Timestamp`
(`TIMESTAMP_LTZ`, one UTC instant) in microseconds or nanoseconds. Any
column may hold a `TIMESTAMP_NTZ`, `TIMESTAMP_LTZ`, or `TIMESTAMP_TZ` value,
but a naive `TIMESTAMP_NTZ` column names no instant, so registration refuses
it as the time column rather than guess its zone. A registration declares
it as `time_column` on its physical layout, and omitting it selects
`wyrd_ingested_at`. The time column alone drives the hour or day partition
transform, the descending time key (after shared context on attributable built-ins), hot and published time
bounds, and `COUNT(*)`. Built-in tables declare their own: OTLP spans
`start_time_unix_nano`, OTLP logs and metrics `time_unix_nano`, gateway and
agent traces `started_at`, Drift observations and
`bifrost.system.audit_log` `created_at`, and `bifrost.verification.results`
`ended_at`. Every caller-supplied `wyrd_*` column is reserved and refused.

A physical layout is that required UTC hour or day time partition plus at most
two declared extra dimensions, each an Iceberg `identity`, `bucket`, or
`truncate` transform of an eligible scalar column, and bounded scalar sort and
Bloom declarations; one validator in `catalog::layout` resolves every
declaration and refuses anything else with no registration residue. A row's
physical partition is its complete typed tuple — partition spec id, time
partition, then each extra value — and a null value is a distinct partition,
never an alias of a literal, another type, or another spec. Every storage tier
keys rows by that tuple: seal keys, WAL records, staged manifests, promotion
records, and `vala.file_list.partition_dimensions`, which records it beside
independent time bounds.

### Table versions and publication

One positive integer `table_version` identifies the complete immutable
definition of a tenant-bound physical table: columns, names, types,
nullability, field IDs, time column, partition specification, sort order,
Bloom declaration and every writer/layout setting. It is scoped to the
existing immutable table identity, not a reusable table name. Creation starts
at 1; one effective definition edit increments it once. Invalid/no-op edits
consume no committed version. Data, snapshot and housekeeping commits preserve
it. There is no parallel configuration counter, composite layout revision,
accepted-recipe JSON or definition fingerprint standing in for this version.
Native Iceberg schema/spec/order IDs remain physical descriptors, not Bifrost
definition versions. Snapshot IDs and Scribe publication progress describe
visibility and do not select the definition of accepted data.

The domain is `1..=9223372036854775807`, represented by Rust `i64`, SQL
`BIGINT` with positivity enforced, protobuf `int64` and eight-byte persisted
version values. JSON always uses canonical unsigned decimal strings, including
`"1"`; TypeScript SDKs use `bigint` and convert directly to/from those strings.
Python retains exact integers. Never convert through JavaScript `number`.
Refuse zero, negative/out-of-range values, JSON numbers, signs, leading zeros,
whitespace, fractional and exponent forms. At the maximum, an effective edit
fails with a typed version-exhaustion conflict without wrapping or mutation.
Public responses use `table_version`; PATCH uses `expected_table_version`.
Old spellings are not accepted aliases.

The Iceberg property `wyrd.bifrost.table-version` and complete definition edits
commit in the same native metadata commit. Native metadata owns schema/spec/
order facts; Wyrd table properties own settings with no native equivalent,
including Blooms. Resolve writer defaults into that definition; a deployment
default cannot silently change an existing version's writer policy.

`BifrostCatalog` owns resolution and the retained
`(immutable table identity, table_version) -> native metadata location`
reference. Its Iceberg SQL catalog fork publishes the reference in the same
catalog transaction as the metadata-pointer CAS, including creation. Assert
the expected version and base pointer at that publication boundary; native
schema/spec/order checks alone cannot protect concurrent Bloom-only edits.
A losing CAS rolls back the reference and refreshes no cache. A committed
edit notifies every node on the existing `bifrost_cut` channel in the same
tenant transaction as its control-row projection; each node drops its cached
current definition, and an admission naming a newer version than its cache
reloads once. Describe always reads the current native metadata. A data-only
head move may retry after all assumptions and the unchanged expected version
are revalidated; a definition conflict cannot silently rebase. Uncertain
COMMIT reconciles exact pointer/reference evidence before reporting success.
The reference is an address index into native metadata, not a second layout
representation or a later independently committed SQL projection.

Admission binds every batch to that definition. WAL slices/commits, seals,
assembly, staged records, acceptance fences and promotion evidence carry the
same version; Oracle pins and Forge resolve it through the existing native
metadata cache and writer owners. Promotion records and file lineage retain
the selected version beside exact native `DataFile` evidence. No hot-ingest
PostgreSQL lookup is added.
Scribe stamps its definition's field IDs (`PARQUET:field_id`); readers resolve
by field ID so renames retain values and a column a source predates reads null.
The current definition and every rooted older definition remain resolvable
under Forge's retention rules below, independently of metadata-log rollover.

This is an in-baseline persisted-contract replacement with no release
migration, backfill, compatibility alias, automatic reset or dual authority.
Legacy evidence may be used only if it maps unambiguously to exact retained
version/definition evidence; otherwise refuse it, never infer from current
schema, IDs or Bloom properties.

The tenant is not a row column. It is a property of the physical table, of
each Parquet file, and of each in-memory Scribe bucket, all bound from the
authenticated principal.

Nullable `workload_card_uid`, `subject_card_uid` and `run_id` provide
optional execution-root, observed-Card and Run correlation. Required, non-null
`sub_principal_id` and `act_principal_id` record who the row's work was for and
who performed it. Public writes are stamped from the verified token and its
validated workload root and refuse supplied values; server-owned outbox writes,
audit included, carry each row's own accepted origin (root, subject, Run,
pair, request and environment), which Scribe validates. The six shared
dimensions are never caller input on either path: Scribe stamps the readable
Service and subject labels from the registry entries of the row's own
workload and subject UIDs when it admits the batch, and the environment from
that row's accepted context, so one batch keeps each row's origin and neither
the delivering worker nor a later registry change relabels it. A public write
that supplies a dimension is refused before ACK. A direct publisher is both, a delegated one records the
party it acts for as subject and itself as actor. None participates in row
identity.

For OTLP records, table-owned projection reads correlation only from the final
record-level `wyrd.card_uid` and `wyrd.run_id` attributes. Every source
attribute is retained in the record's Variant attribute column; when a
collection repeats a key, the final occurrence is the one stored, matching the
OTel data model and the correlation rule. The values use the `CardUid` and
`RunId` text grammars and land in `subject_card_uid` and `run_id`; the OTLP
attribute keeps its `wyrd.card_uid` spelling. Scribe stores a client Card UID only when the verified
principal scope, or for a principal bound to no Card the tenant registry,
confirms it.

Identity is batch-level. The batch identity is the client-generated UUIDv7
`wyrd_batch_id` request field; it is not stored on rows. Within a
tenant-qualified physical table, one accepted logical batch is:

```text
wyrd_batch_id
```

Globally it is:

```text
(data_tenant_id, logical_table, wyrd_batch_id)
```

Bifrost stamps no per-row position. Rows inside a batch are addressed by their
own payload; nothing downstream consumes a server-assigned row address, so none
is stored. Gate validates the batch identity and routes the authenticated
write, while table-owned validation prevents payload columns from supplying
server-owned fields. A retry preserves
the batch ID. Within the idempotency-retention window,
reusing an accepted ID requires the same source schema, row count, row
order, and payload digest; any mismatch is a stable batch-identity conflict.
Resolve accepted retries under the fence's original `table_version`, even
when current partitioning or schema would prepare different slices or reject
the old request. Stale versions on new batches are typed table-version
conflicts; an accepted ID never bypasses strict content or permission checks.

One authenticated tenant and logical `TableRef` bind exactly one physical
Iceberg table, namespace, and object-store prefix. Callers never choose another
tenant's physical identity. Every staged and published Parquet file records
its tenant once, in the `wyrd.bifrost.tenant` footer key-value, taken from the
authenticated binding that wrote it; Forge rewrites carry the same value
forward. In-memory Scribe rows are bound by their seal key's tenant. Postgres
RLS, object prefixes, Scribe ownership, Oracle source binding, and the footer
proof enforce the same tenant. Oracle compares a file's footer tenant with the
authenticated binding once when it opens the file, before decoding any row; a
missing, duplicated, or foreign footer tenant, or an encrypted file whose
footer the reader cannot prove, fails the query closed with
`WYRD_VALA_500_QUERY_TENANT_INVARIANT` and one leader security audit event.
There is no per-row tenant column and no per-row tenant check.

Built-in and user-defined tables share this physical model. "Built-in" names
definition ownership, not a weaker tenant scope or a separate storage mode.
Built-ins are created eagerly, never lazily: tenant provisioning ensures every
table in the canonical built-in inventory through the catalog's idempotent
ensure operation before the tenant becomes active, and server startup ensures
the same inventory for every active tenant before any role activates, which
backfills tables added to the inventory after a tenant was created. Either
failure fails provisioning or startup rather than exposing a partially ready
tenant or server. An unwritten built-in is therefore queryable and empty, and
neither first write nor describe owns its creation.

## Storage format and Variant

Every Bifrost table, built-in and user-defined, is created as Iceberg format
v3, and physical-table validation refuses a table that is not v3. There is no
other format and no mixed-version state. Appends assign row ids through the v3
first-row-id mechanism. The hidden lineage columns `_row_id` and
`_last_updated_sequence_number` are Iceberg metadata, never logical columns:
they are absent from the table's definition schema and every query
result, and a Forge rewrite preserves both for every surviving row.

Variant is a canonical column type (`DataTypeSpec::Variant`) for open,
self-describing values. On the Arrow wire a Variant column is the
`arrow.parquet.variant` extension over the unshredded `metadata`/`value`
binary Struct; Iceberg and Parquet store it as the v3 Variant type. A field
declared Variant must carry that extension at every nesting level: missing or
foreign extension metadata is `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`, even
when the storage Struct matches. Different file-specific shredding results
may share one table version under the same declared shredding policy; they do
not create independent definition identities. Variant extension/value checks
remain mandatory.

A Variant value reads back with the type, nesting, and value it was written
with. Integers stay integers and never pass through floating point; doubles
keep their IEEE meaning; strings, booleans, bytes, arrays, and objects keep
their type; and an absent object key stays distinct from a key whose value is
null. Object keys are unique: when an OTel attribute collection repeats a key,
the final occurrence is stored. JSON input converts an integer in the signed
64-bit (`i64`) range to an integer, an integer from `i64::MAX + 1` through
`u64::MAX` to a scale-zero Variant decimal, and a non-integer number to a
double; every other integer is refused with
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`, so every accepted integer reads
back exactly without `serde_json` `arbitrary_precision`. A value
nested beyond 64 containers is `WYRD_VALA_400_VARIANT_TOO_DEEP`, one whose
encoded metadata plus value exceeds 8,388,608 bytes is
`WYRD_VALA_413_VARIANT_TOO_LARGE`, and undecodable bytes are
`WYRD_VALA_400_VARIANT_INVALID_JSON`.

Raw Variant bytes are held to the same domain. A raw number must be one JSON
input could produce: an integer, a finite Float or Double, or a scale-zero
Decimal16 in `i64::MAX + 1..=u64::MAX`. Decimal4, Decimal8, any other
Decimal16 (`numeric_kind: "decimal"`), and a NaN or infinite Float or Double
(`numeric_kind: "double"`) are `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`;
exact fractional or wider numbers travel as strings. A raw object is canonical
or `WYRD_VALA_400_VARIANT_INVALID_JSON`: its resolved field names strictly
increase whether or not the metadata dictionary is sorted, and every field
value owns its own bytes, never shared with or overlapping another field's.
Bifrost enforces size, encoding, numeric range, and depth when writing.
Validation of raw bytes is iterative and bounded by the fixed size limit, and
because no two fields may share or overlap bytes a value cannot describe more
nodes than its bytes hold. Reads apply no Bifrost limit: arrow-rs decodes
stored Variant and refuses malformed or over-deep bytes with an error rather
than a panic or unbounded work.

When writing, one value reports one failure in a fixed order: size, then invalid JSON or
encoding, then numeric range, then depth, wherever each sits in the value.
JSON size counts the bytes actually built from accepted members; a refused
number or an over-depth container adds none. JSON syntax is decided at any
nesting depth, so valid JSON past the limit is too deep, never invalid, and
`try_parse_json` maps only invalid JSON to null.

Scribe repeats the extension, size, encoding, numeric, and depth checks over
every declared Variant of a built-in table at its admission boundary, before
the batch is acknowledged, because a raw Arrow writer can skip client
preparation, and neither a claimed version nor a storage-type match proves
value validity. Extension identity is checked across all fields first, then each value in row and field
order in the order above; the first failure names the field, row, and path.

Built-in tables store open caller content as Variant and closed shapes as
typed columns:

- `bifrost.traces.spans`, `bifrost.logs.records`, and `bifrost.metrics.points` store
  their attribute collections (record, resource, scope, span event and link,
  and exemplar filtered attributes), the log `body`, and the metric `metadata`
  as Variant. `resource_entity_refs` is a list of Structs with `type`,
  `id_keys`, `description_keys`, and `schema_url`. Well-known
  semantic-convention values are copied into nullable promoted columns. Owned
  `service_name`/`service_version` agree with validated Service UID and environment
  with accepted context; unowned telemetry promotes typed resource values. Raw
  attributes remain unchanged. Subject kind/name/version derive from validated
  subject UID. Other promotions remain null for absent/wrong-type sources;
  `gen_ai_*`, `http_request_method`, `http_route`,
  `http_response_status_code`, `url_full`, and the `exception_*` columns of the
  span's last `exception` event on spans; the record's own `exception_*`
  attributes and `body_text` (the body when it is a string) on logs.
- Three tables hold each verification fact once. A result read assembles the
  public `VerificationResult` from them: the verdict from `results`,
  `requirements` and `steps` from `execution_outputs` by `result_id`, and
  `evidence` from `execution_inputs` by `execution_id`. Detail is never
  copied between them, and no other table projects drift detail.
- `bifrost.verification.results` is the payload-free summary of one result:
  result, execution, Run, Verifier, subject, binding, group, trigger, and
  window identity, execution status, the verdict, and timing. It carries no
  Variant. Published correlation columns are `execution_id`, nullable
  `binding_id`, and nullable application `run_id`; result timing remains
  `started_at` and `ended_at`. A verification execution is never relabelled
  as an application Run.
- `bifrost.verification.execution_outputs` holds one result's detailed
  judgment, keyed by `result_id` with its `execution_id` and `ended_at`: the
  sensitive `output` Variant holds exactly `requirements` and `steps`,
  including scores, reasons, computed `actual` values, citations, check and
  provenance detail, custom Verdict `data`, a drift requirement's per-feature
  detail, and captured step outputs. It never holds the evidence map or a
  second verdict. It is staged with its summary in one write, before it.
- `bifrost.verification.execution_inputs` holds each execution's one input
  record and its closed `origin`: `direct` (`verify`), `submitted`
  (`submit`), `group`, or `schedule`. The `inputs` Variant holds the named
  input declarations (JSON values, file digests, Artifact and trace
  references) for `direct` and `submitted`, and an internal selector for
  `group` and `schedule`. A group selector records its half-open
  `wyrd_ingested_at` range and, per source slot, its authored reference and
  ordered source execution IDs with each selected result ID or null; a
  schedule selector records its `[previous boundary, due)` window and the
  selected observation `record_id`s. Selectors reference live logical rows,
  never copy their values, and are resolved on every execution, retry, and
  read. The server-owned `queue_execution` marker is true only for
  `submitted`, which alone activates queued work; its `activation` holds the
  accepted route overrides, idempotency key, request digest, and authority.
  Its published input timestamp is `recorded_at`, the server-recorded input
  time, distinct from execution timing or sample/window time. It uses the
  same execution/binding/application-Run correlation names where applicable.
- Every nullable built-in Struct — the `bifrost.metrics.points` bucket sets and
  `bifrost.gateway.calls` `resolved_model` — has nullable children, because a
  required Parquet leaf under a null parent reads back as another row's value
  through a field query. The Struct is whole or absent: a present one sets every child its
  shape requires and an absent one nulls every child, so `s['child']` reads
  SQL null for an absent Struct on hot and published data alike. Admission
  refuses a batch with a partial Struct before ACK with
  `WYRD_VALA_400_SCHEMA_PARSE` naming the row and child.
- The `bifrost.verification.execution_outputs` output, the
  `bifrost.verification.execution_inputs` inputs and activation,
  `bifrost.gateway.calls` `request_payload` and `response_payload`, and
  `bifrost.system.audit_log` `detail` are Variant.

### Variant shredding

Every Variant field — a table column, a Struct child, or a child of a List
element's Struct — is stored in the standard Parquet shredded layout:
`metadata`, a residual `value`, and `typed_value` leaves for the keys the file
chose. The footer records that file-specific shredding result; Wyrd keeps no
parallel shredding recipe blob. Files with different adaptive results can
share one table version when produced under the same definition/policy. Values
that do not fit the layout stay in the residual `value`,
so the layout changes performance only, never a result. Both readers unshred
at any depth before rows leave the scan.

- **Scribe** keeps one logical schema per table version and may retain rows
  in a physical layout. Each final hot object's
  layout is chosen before its writer opens, from one seeded, stratified sample
  of the claim: strata are `sub_principal_id` values, subjects with fewer than 30
  rows share one stratum, and each stratum samples Cochran's size for 99%
  confidence and a ±0.02 margin. A key is shredded when at least 8% of a
  stratum's sampled non-null values hold it with one compatible type. The seed
  is the claim, so a re-run claim writes the same objects. Each element of a
  Variant inside a List counts as one observation, and one layout covers every
  element. The sample holds counters, never rows. The sample and the claim
  merge read complete values — typed leaves plus residual reconstructed by the
  recursive `unshred_variants` — so a key a staged run kept only in its
  residual stays discoverable; runs of different layouts merge only after
  their footers name the claim's logical schema and tenant.
- **Learned layouts.** A successful claim assembly installs its layout in a
  process-local advisory cache keyed by exact tenant, table, and table
  version, in completion order; entries are bounded in count and charged to
  the Scribe governed root, and eviction releases only the cache's ownership.
  A shard pins the cached layout when it opens a generation for a seal key and
  keeps it until the generation closes; a generation opened cold stays
  canonical. Rows are shredded on the admitted ingress CPU lane before WAL
  submission, with an upper bound for the conversion reserved first and the
  originals released after; the WAL keeps canonical IPC, logical fingerprint,
  and digest. A failed reservation or conversion refuses the append before
  WAL. Staging writes the frozen generation's rows as is; recovery
  reconstructs the run's logical schema from its layout. Memory reads
  reconstruct logical Variant storage before rows leave the capture, so unions
  and remote output never expose a physical layout. A capture first filters a
  physical generation by the query predicate, adapted to that layout by
  DataFusion's expression adapter, so only matching rows are reconstructed;
  Wyrd classifies no query shape. A restart or eviction only
  returns new generations to canonical until a claim teaches the process
  again; no hint crosses processes, tenants, or table versions.
- **Forge** samples nothing. It sums each source footer's shredded-leaf
  non-null counts, keeps keys covering at least 10% of the combined rows,
  gives a type conflict to the type covering more rows, and writes every output
  of one rewrite with that layout. Forge only keeps or drops keys; Scribe
  discovers them.

Sampling policy and per-node child/depth bounds remain internal writer
controls, not new user tuning options. Freeze any resolved policy that affects
writer/layout behavior in the table definition; changing that policy for an
existing table is an effective versioned edit. The file-specific adaptive
shredding result remains in its footer and allocates no separate version.

## Declared types and the registered schema

`wyrd-types` owns the one mapping from a declared model or Arrow schema to
wire columns, and `wyrd_types::schema::check_supported` is the one decision of
which declared types Bifrost can store. The SDK declaration and server
registration both call it, so an unstorable type (`UInt64`, `Date64`,
`Time32`, a `Time64` or `Timestamp` unit Iceberg has no column for, at any
depth) is `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE` before any request or
table exists.

A table version's retained Iceberg schema owns its schema. Registration stores
each declared column in its Iceberg form, and a repeat declaration that spells
the same definition repeats rather than allocating a version. New native
writes carry the current version obtained with that definition; canonical
built-ins use Gate's versioned server projection. Accepted retries use their
original fence version. On every write Scribe matches the batch to its bound
schema before acknowledging: columns match by name
in any order; a column whose Arrow type is another spelling of the registered
Iceberg type (`Int8` for `int`, `LargeUtf8` for `string`, a timestamp in any
zone) is cast once with Arrow's `cast`; a column whose storage matches but
whose Variant identity does not is `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`;
and any other difference is a typed schema-conformance conflict. A stale
version on a new batch is a typed table-version conflict (HTTP 409), distinct
from malformed payload validation. Keep structural, type, nullability,
reserved-field and Variant/value checks. Version identity removes redundant
schema/layout hashing, not required casts or trust-boundary validation.
Reuse the existing conformance-plan cache by version/descriptor and actual
source shape; do not hash layout JSON or resolve current metadata per batch.

Bifrost has three timestamp types, named as Snowflake names them:

- `TIMESTAMP_NTZ`: a wall-clock reading with no instant, stored as a naive
  Arrow timestamp. A naive declared timestamp is `TIMESTAMP_NTZ`.
- `TIMESTAMP_LTZ`: one instant, stored as an Arrow timestamp zoned `+00:00`
  and read back in UTC. A declared timestamp in any zone is `TIMESTAMP_LTZ`; the zone
  label is not stored.
- `TIMESTAMP_TZ`: one instant plus the writer's wall-clock reading, stored as
  the Struct of required microsecond `utc` (`TIMESTAMP_LTZ`) and `local`
  (`TIMESTAMP_NTZ`) children, so a reader
  can group by the writer's local hour and the writer's offset is
  `local - utc`.

`wyrd_types::TimestampKind` owns each type's JSON Schema format and its
RFC 3339 rendering; the Python and TypeScript SDKs read both from the native
binding rather than restating them.

## Durability and visibility

Bifrost uses explicit authority transitions:

```text
accepted append
  -> WAL-fsynced and batch-fenced active rows
  -> immutable rows owned by a closed shard cohort
  -> fsynced, validated, query-registered staged runs
  -> committed published Scribe hot objects
  -> Iceberg promotion snapshot
  -> Forge rewrite snapshot
```

An append acknowledgement means the WAL append and batch fence are durable and
the exact rows and their recorded table version are authoritative in Scribe.
It does not wait for local Parquet, object publication, Iceberg promotion, or compaction.

Every query pins one published cut: an Iceberg snapshot and committed Scribe
hot objects not represented by that snapshot, with the table versions needed
to resolve its schema and sources. Definition pins protect native definition
metadata; snapshot/file visibility retains its existing independent rules.
Oracle also discovers active
streams by listing, concurrently and under the query deadline, the Scribes in
the attempt's frozen roster, and sends live work only to owners of relevant
table partitions at the same frozen endpoint and fence. A listing that reveals
a newer writer epoch restarts the whole attempt once on a refrozen roster;
nodes from different cuts are never mixed. Scribe scans its own memtable or staged authority and streams
bounded Arrow results to Oracle, producing each batch only when Oracle pulls
it. The scan's memtable references, staged-run leases, and follower admission
belong to that stream and are released when it completes, is cancelled, or is
dropped; the query deadline is its only timeout. An unavailable Scribe missing before
discovery is outside the known live set. A known Scribe that cannot be listed
degrades the result. Losing a Scribe after its capture was accepted fails the
query, even before its first row. Published-source,
security, tenant, schema, resource, cancellation, and deadline failures fail.

All callers, including Verifiers, use this same query service and source
behavior. A `trace` check reads its evidence as the accepted caller's
`SELECT … FROM bifrost.traces.spans WHERE trace_id = X'…' ORDER BY
start_time_unix_nano, span_id LIMIT 10001`: exact trace ID, no time or
attribution predicate, so retained history may cost more to scan. The request has no visibility, freshness, or query-class selector.
Oracle alone classifies the one DataFusion plan as Interactive or Analytical
from its physical root. A successful terminal means the published cut completed
and each selected online Scribe supplied the rows required by the completed
DataFusion plan. A plan that stops consuming a live child (for example, at
`LIMIT`) cancels and drops it without draining to a footer; unexpected EOF of
a still-needed stream fails. Success means each row identity the selected
sources held is returned once: a publication never makes a row disappear or
repeat, and verification judgments inherit that guarantee. Oracle selects
sources before execution. It pins the cut, which carries each Scribe node's
publication revision for every table, and asks every frozen Scribe for a live
capture retained under the query's attempt identity until the query deadline.
Each capture reports, from the same critical section that decided which
generations it still serves, the revision that Scribe's last locally completed
publication committed at. A reported revision above the cut's, or a stale
writer epoch, discards the selection — its captures are released and the cut
is evicted from the cache — and Oracle repins from Postgres and rebuilds under
the same deadline before admission, so no row of a discarded selection is
produced. One node's publications commit in revision order, so a cut at least
as new as every report holds every row the captures stopped serving. Each live fragment
reads only its own capture, named in its signed assignment; Scribe refuses a
foreign, substituted, released, or expired capture, and a capture lost after
acceptance fails the query rather than opening fresh live data. Captures are
released when the query's plan is dropped, on success, failure, or
cancellation, and expire at the deadline otherwise. A Scribe that cannot list a
table's streams at all is known live-source degradation. The terminal
distinguishes success, that degradation, and failure; clients accept rows only
after a valid terminal.

## Ingest: Scribe

### Shards and hierarchical ownership

Each Scribe pod runs a configured number of shards (`WYRD_SCRIBE_SHARD_COUNT`,
default 1, at most 256). The routing key is:

```text
hash(data_tenant_id, canonical_table, wyrd_batch_id)
```

reduced modulo the shard count. Distinct batches for one table use all shards;
retries use the recorded shard. Each shard owns a bounded mailbox, its own WAL
stream, memtable buckets, and cohort state. Shards are a pod-local concurrency
topology, not tenant partitions or table reservations. Replay maps a recorded
shard onto the running count, so the count may change across restarts.

Each shard syncs only its own WAL, so one group's sync covers only that
shard's requests. More shards therefore mean more syncs per acknowledged
request, which lowers throughput wherever a durable sync is expensive; the
default of one shard maximizes sync coverage. A WAL sync covers every append
that finished before it began, appends continue while it runs, and a sync
whose appends an earlier sync already covered issues no `fdatasync`. A shard
holds one WAL IO worker for each blocking append and sync, so boot runs at
least one WAL IO worker per shard.

Each shard rotates its WAL and memtable together when the WAL reaches
`WYRD_MAX_FILE_SIZE_ON_DISK` (MiB), the memtable reaches
`WYRD_MAX_FILE_SIZE_IN_MEMORY` (MiB), or the generation reaches
`WYRD_MAX_FILE_RETENTION_TIME` (seconds). Each trigger reads its own counter;
no byte is charged to two triggers. WAL disk is provisioned rather than
tracked: there is no global disk ledger, and an out-of-space write surfaces as
retryable `507` pressure. Memory stays under the global admission governor
because it competes with queries.

Shard owners, reconciliation, staging, assembly, and persistence run on one
dedicated Scribe Tokio runtime separated from request serving. One non-cloneable
owner controls that runtime; all engine consumers hold handles. Server shutdown
drains the bounded Scribe lifecycle before releasing the runtime, so request
state cannot accidentally destroy an executor or abandon durable ownership.

`ScribeAdmission` owns global resource accounting. A tenant ledger and table
ledger account every lifecycle category independently: admitted items and
bytes, active and immutable bytes, durable staging, merge scratch, stage and
upload claims, and persistence work. A table activates only when one checked
complete lifecycle vector fits. Idle capacity is work-conserving. Under
contention, equal tenant and table soft shares stop an over-share incumbent
from reacquiring capacity while acknowledged ownership drains. Bounded,
expiring identity-only demand determines which waiting table receives released
capacity; it reserves no bytes or slots.

Admission completes before WAL or mailbox mutation. It either accepts
immediately or returns typed retryable pressure. Already acknowledged ownership
is never revoked. Every category change is a checked move between owners;
temporary overlap is reserved explicitly.

Each shard schedules complete commands by tenant round-robin, table
round-robin within the tenant, and FIFO within the table. System and dynamic
tables have equal scheduling weight and admission rules.

The principal size boundaries are independent:

| Boundary | Rule |
|---|---|
| WAL segment | `WYRD_MAX_FILE_SIZE_ON_DISK`, 512 MiB default per shard WAL file |
| Active shard generation | `WYRD_MAX_FILE_SIZE_IN_MEMORY`, 512 MiB default per shard, plus age and pressure |
| Ingest request | one `scribe.ingest_request_bytes` wire ceiling, 16 MiB default |
| Ingest expanded data | derived 4x the wire ceiling (64 MiB default), enforced before WAL; no row cap |
| Parquet row group | soft 128 MiB encoded target; a single large accepted row fits a group of its own |
| Scribe hot object | `min(WYRD_MAX_FILE_SIZE_ON_DISK, Forge target)`: a key's ready members drain once their summed raw (uncompressed Arrow) bytes reach it, or at dwell, with bounded residue; output objects roll at the same value in encoded bytes |
| Forge rewrite output | the target the table's definition stores: its registered compaction target, else the deployment default of approximately 1 GiB (`WYRD_BIFROST_FORGE_TARGET_FILE_SIZE_BYTES`) resolved at creation |

Ingest has exactly one size setting. The wire ceiling bounds encoded request
bytes; the expanded ceiling is derived, never configured, and bounds decoded
Arrow memory plus managed columns (and, for OTLP, the projected output) before
anything is appended to the WAL. A request above either ceiling is refused
with `PayloadTooLarge`. Row count is not a limit.

The generation maximum age and staging maximum dwell are each 600 seconds by
default. Configuration validates checked arithmetic and proves that one maximum
ingress envelope, one complete table lifecycle vector, one immutable rotation,
and the independent merge and upload workspaces fit before serving. It never
derives tenant or table cardinality from shard count.

### Append, rotation, and staging

The Scribe write path is:

```text
validate and split by canonical physical partition
  -> global, tenant, and table admission
  -> route to one shard mailbox
  -> tenant/table/FIFO scheduling
  -> WAL append, fsync, and durable batch fence
  -> tenant/table/partition memtable insertion
  -> acknowledgement
```

A shard commits admitted requests in groups. One group cycle writes every
SLICE record, appends each complete batch's COMMIT record, and makes both
durable with one sync over every segment the cycle touched before it records
the batch fences. Each tenant's group uses one multi-row INSERT in its existing
tenant transaction, then one batched SELECT for conflicting identities if
needed. Classify each batch by full durable key in original group order;
RETURNING order is not input order. A contradictory duplicate rolls back that
tenant group's fresh rows. Retain the scheduler's repeated-identity guard.
One COMMIT per tenant remains; an ambiguous result uses batched resolution in
a fresh tenant transaction, retries identical absent fences, and poisons on
unavailable/contradictory authority. RLS, group ownership and transaction
counts do not change. Versioning adds no normal-ingest PostgreSQL call.
No request is acknowledged before WAL sync and its durable fence commit.

An honest retry of a batch that is already fenced and visible can avoid the
shard queue. The existing bounded committed-batch mirror returns its original
table version and acceptance facts before current-definition preparation.
Compare content under that retained definition, not current schema/partition
rules; preserve exact source schema, row order/count, payload and origin/time.
A matching mirror hit acknowledges the original acceptance without rewriting.

On a cold miss, do not add a preliminary accepted-ID query. Preparation and
WAL attempts remain provisional. The existing fence conflict read returns the
original version/reference, digest/count, ingestion instant and canonical WAL
coordinates. A mismatch rolls back the tenant group's uncommitted fresh
fences before bounded retained-definition comparison outside the transaction;
resume only after proving an identical retry. A genuine contradiction commits
none of that group's fresh fences. An exceptional current-contract or stale-
version refusal may resolve an accepted ID under the retained definition; an
unknown ID preserves the original refusal. Keep authentication, permissions,
limits and managed-field validation on all paths, with no new SQL on successful
fresh admission or extra unconditional full-payload copy/hash pass.

Each write request carries one profile in the Oracle query-profile pattern,
emitted once when the request settles as
`scribe_write_profile_nanoseconds_total{event}` and
`scribe_write_profiles_total`. Its own steps are `Scribe.admission`,
`Scribe.decode`, `Scribe.preprocess`, and `Scribe.shard_wait`; each group
stage, `Scribe.group_write`, `group_commit`, `group_sync`, `group_fence`, and
`group_insert`, is timed once per cycle and charged to every request the cycle
acknowledged.

Every time-column value is accepted; no acceptance window exists. One request
may touch at most 32 distinct complete partition tuples of its table, a
repeated tuple counting once; a request touching more is refused whole with the
existing invalid-frame error before WAL append or fence and stores nothing.

Each shard projects WAL bytes, Arrow and metadata ownership, age, and resource
pressure before append. Rotation closes the shard generation when any validated
limit requires it. The active memtable remains separated by exact
`SealKey { tenant, table, table_version, physical_partition }`. Rotation
freezes each nonempty
bucket independently; no file or sort may mix tenants, tables, layouts, or
physical partitions.

Frozen members follow this monotonic lifecycle:

```text
ImmutableArrow
  -> PreparingLocalRun
  -> Ready
  -> Claimed
  -> Publishing
  -> CleanupPending
  -> Retired
```

`ScribeHotStage` bounded-sorts a member into immutable local Parquet runs:
each bounded candidate's order is computed from its sort-key columns alone,
and each payload column is gathered once from the candidate's source batches
into the sorted output, so the unsorted payload is never concatenated first.
It
fsyncs the file, manifest, and directory, verifies checksum/schema/layout/footer,
performs a bounded read preflight, and registers it as live-tail authority. The
shared cohort WAL retires only after every member reaches that boundary.
The staged manifest/context carries the table version and reconstructs its
layout from retained native metadata, never a copied recipe or current Blooms.
Its complete seal key and generation ordinal remain distinct through live
capture; two versions sharing an ordinal must both be selected exactly once.
Definite pre-registration failure returns to immutable Arrow; uncertain state
keeps WAL and local evidence for reconciliation.

### Assembly and publication

`StagingAssembler` claims complete ready members under the exact key:

```text
(tenant, table, table_version, physical_partition, node_id, writer_epoch)
```

Claims may combine compatible members across generations and shards on the same
node. The table version supplies definition compatibility; no parallel layout
fingerprint or recipe digest participates. Membership is deterministic and
durable before merge; one member cannot be split across claims and a later member cannot join an existing claim.

`ParquetBatchEncoder` performs a bounded external merge in canonical
`PhysicalLayout` order; rows that tie on every layout key keep claim-member
order, then their position within the staged run, so the merged order is
deterministic without a per-row tie-breaker column. It writes Parquet row groups toward a soft 128 MiB target and
closes immutable hot objects around the 512 MiB whole-file target. The claim
trigger counts raw bytes: each staged member records the uncompressed Arrow
bytes its frozen bucket held, and a key is claimed once those sum to the target,
matching how memtable rotation measures size. Counting encoded bytes instead
would hold a well-compressing table until dwell. A completed row group is
indivisible, and a smaller object is valid for dwell, partition close,
pressure, drain, or final residue. The 512 MiB target is independent of WAL,
active-memory, row-group, and Forge output geometry.

`ScribePersistence` derives deterministic object identities, uploads through a
bounded lane, and atomically commits `file_list` plus lineage evidence under
the publication fence. Promotion evidence carries the same table version,
complete typed partition and exact writer-derived Iceberg `DataFile`; no
current-property reconstruction may relabel a staged output. Staged runs remain
authoritative until that transaction is
committed or reconciled as identical. New cuts then use the published object;
existing staged-reader leases finish before local deletion.

### Live tail, recovery, and shutdown

Every local path a node owns derives from its one exclusively locked
`WYRD_BIFROST_DATA_DIR` root, so no two live processes share a WAL identity.
Oracle never opens another node's local path and does not use WAL as its normal
query source. Scribe executes authenticated local DataFusion scan fragments
over active, immutable, or staged rows and streams Arrow batches through the
existing peer protocol. Projection, predicate, physical partition,
writer epoch, deadline, and cancellation are enforced. A live snapshot is
shallow references to rows the Scribe already holds, so it carries no batch or
byte limit; the query's execution memory pool governs what execution retains.
Open fragment streams own their source references until completion or drop; no
independent tail timeout can end an otherwise active query.

Startup replays WAL using the recorded shard ID, validates staged files and
manifests, rebuilds source and claim indexes, and reconciles publishing
operation IDs against `file_list` before opening admission. Unknown codec/table
versions, unmappable legacy evidence, checksum mismatch,
contradictory lineage or ambiguous authority fail closed. WAL SLICE and COMMIT
bind the same table version and retain per-attempt CRC, ordinal/count, terminal
and payload/WAL integrity checks; no recipe JSON travels in either record.
A cold retry may leave a complete provisional attempt with different geometry.
The durable fence's canonical coordinates select the accepted attempt;
noncanonical attempts for that spent ID restore no rows. Do not reject those
attempts solely for differing cross-attempt geometry before consulting the
fence. Contradictions at canonical coordinates still fail closed, and missing
fence authority never permits speculative suppression. Preserve genuinely
unfenced complete-commit recovery under its recorded version. No abort record,
truncation workaround or second replay ledger is introduced.

A shard keeps up to two retired WAL files and overwrites them as its next
segments, so appends reuse allocated blocks rather than growing a file. Every
record carries its segment's sequence tag; a record left from a file's earlier
use therefore reads as the end of the segment, and an acknowledged COMMIT
beyond it fails replay closed. Each shard's WAL directory holds a manifest of
per-seal-key watermarks at or below which replay skips that shard's records.
Staging does not write it, because a live staged record is itself the evidence
for its WAL range: the range is folded into its own shard's manifest before
retirement removes the record and once at startup before replay, and a replay
retirement folds its watermark before its segments retire. A watermark never
spans shards: every shard draws LSNs from the stream's one counter and
`file_list` rows carry no shard.
Because SLICE and COMMIT records share one sync, a crash may leave a complete
but invalid record in a shard's unsynced tail. Replay ends that shard's chain
at the first such record in its last segment, as it does a torn tail, only
when the shard stream's newest durable batch fence lies before it; a bad
record preceding an acknowledged COMMIT, in an earlier segment, or with no
fence authority available fails closed.

Shutdown closes admission and mailboxes, rotates nonempty generations, stages
immutable ownership, and drains admitted work within the server deadline. It
publishes nothing: staged members below their object target stay staged, and
the next process on that staging volume restores and publishes them. Unsettled work retains exact replay evidence. Shutdown
never deletes WAL or staged files merely to meet a deadline.

## Query: Oracle

### Immutable planning and source authority

Oracle authenticates and authorizes the request, validates read-only SQL,
acquires the read-audit durability boundary, pins the published cut, discovers
relevant online Scribe routes, builds one optimized logical and physical plan,
admits resources, and streams one terminal-safe result. DataFusion providers
receive the pinned files and selected Scribe live sources. Tenant authority is
installed before plan decode or source IO. Every source is read under its own
metadata and projected onto the table's current schema by field id: the
Iceberg scan binds the current schema rather than the snapshot's, Parquet
columns resolve by field id, file pruning is re-planned against each file's own
schema, and the live tail carries the required fields so memtable rows admitted
under an earlier table version project by id. A column a source predates reads
null,
so filters on it stay exact. There is no caller-selected source
mode, freshness policy, or query class.

Oracle plans every query once through the pinned `datafusion-distributed`
planner and derives its admission and terminal path from the returned physical
root:

- **Interactive** is selected for a normal DataFusion physical root and keeps
  protected capacity for low-overhead, high-throughput reads.
- **Analytical** is selected for a
  `datafusion_distributed::DistributedExec` root and executes its streamed
  stage graph across authenticated Oracle peers.

Oracle maintains no operator allowlist, candidate heuristic, second physical
build, or pre-selection fallback. The pinned planner decides whether useful
network boundaries survive; its registered codecs and workers decide what the
integrated system can execute. Planning, codec, and worker incompatibilities
return a stable structured failure. Representative end-to-end queries prove
scan/filter/projection, grouped aggregation, join, sort/limit, exchange, and
spill behavior without claiming exhaustive operator coverage.

### Variant SQL

Every Oracle session — leader planning, admission execution, follower,
analytical leader, analytical planning, and distributed worker — is built with
`DataFusion`'s default features, which include the fork's `variant` feature:

- `v -> 'key'` and `v -> n` return the Variant at an object key or array index,
  or null when absent; chains such as `attributes -> 'http' ->> 'route'` follow
  nested keys.
- `v ->> 'key'` and `v ->> n` return the same value as text: a string as
  itself, any other non-null value as its JSON text, and SQL null when absent or
  JSON null. Its result casts to numeric, boolean, and timestamp types with
  ordinary SQL cast semantics.
- `parse_json(text)` returns the Variant for JSON text and fails the query on
  invalid input; `try_parse_json(text)` returns null instead. `to_json(v)`
  returns a Variant's JSON text.

`->` and `->>` apply only to Variant operands; a JSON text column is queried as
`parse_json(column) ->> 'key'`. `->` plans as `variant_get` and `->>` as
`variant_get_text`. Struct access stays DataFusion's exact `s['field']`
`get_field`. Every Variant result keeps the `arrow.parquet.variant` extension
on the wire. Wyrd adds no Variant function, planner, or file adapter: queries
use the fork's Variant feature as-is, so a Variant evaluation failure is the
generic `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, and peers agree on function
semantics through the pinned fork revision.

### Nested-field reads

When every use of a Struct or Variant column in a query's projection and
filter is a literal path — `s['a']['b']`, `v -> 'a' ->> 'k'`,
`l[i]['f'] ->> 'k'`, or such a path on an `unnest(l)` output — each Oracle
reader decodes only that path's leaves. A Struct path reads its field's
leaves; a shredded Variant path reads its `typed_value` subtree, its
path-local residual `value`, and the top-level `metadata`; a List element path
reads those leaves of every element. Sibling leaves are not decoded. A query
that uses the whole column, or a path a file did not shred, reads the whole
root, with the same result.

Hot objects and published files read through one per-file read plan built on
the `DataFusion` fork's nested projection, decoder filter, and statistics
pruning, so leader, follower, interactive, and analytical scans cannot drift.
Struct access stays exact `get_field` and Variant access stays semantic
`variant_get`. A literal comparison, `IN`, or null test on such a path filters
while decoding. It skips row groups or pages only when a Variant path's
residual is all-null and its type matches, or a Struct field's statistics are
complete; otherwise the predicate is evaluated on the decoded values.
Unshredded against shredded Variant reads are DataFusion's behavior, timed by
DataFusion's `parquet_variant_query` benchmark.

### Distributed analytical execution

The analytical path uses streamed exchanges parameterized by DataFusion
`Partitioning`. It adds no materialized shuffle service, independent scheduler,
or query-job subsystem. Every stage assignment travels in a versioned, typed
peer context that binds:

- tenant and permission digest;
- public query, DataFusion query, stage, task, and attempt identities;
- pinned snapshot and fragment digests;
- leader and worker fences and audience;
- request digest and absolute deadline.

The context is unsigned. Peer mTLS with the fixed `wyrd-peer` cluster identity
is the only peer authentication and completes before the bounded first frame is
accepted. The receiver compares every context field with its own state: its
node identity and fence, the exact bytes received, and the expiry, which is an
admission window rather than replay state. A compromised cluster member is out
of scope. Claims and body digests are verified before lazy plan decode, task
cache lookup, provider creation, or source IO. Every worker replaces its
process runtime with the exact query-admitted `RuntimeEnv`, `MemoryPool`, spill
share, cancellation token, and deadline.

A follower never outlives its leader. The leader's stream owns every grant a
follower holds for its graph: slot unit, memory, scratch share, query runtime
and spill directory, task cache entries, and structured tasks. A follower
takes no capacity before it accepts that stream. It admits the graph when it
accepts the leader's stream, beneath its own local capacity root, or refuses
at once with retry timing when full. No pending reservation, reservation
expiry, or reclaim-by-timeout exists. When the leader stream completes, fails,
is cancelled, or is dropped, or the deadline passes, the follower cancels the
graph immediately and releases every owner inline, without waiting for nested
children still being torn down; a late child returns its bytes through the
shared memory root as it frees them. Nothing waits for a later request or
timer to reclaim it.

Exchange buffers draw from the same finite query-owned memory pool as the
operators; they are not precharged into a predicted child allocation. Before
dispatch, Oracle enforces its configured selected-worker limit, admitted
tasks/partitions, Wyrd-owned admission queue and slots, and scratch allocation
with checked count/range arithmetic. Dependency-owned exchange queues retain
their pinned byte backpressure without a Wyrd item-count guarantee. Oracle does
not claim to predict every dependency allocation or transient encoded-message
byte. Follower spill is charged to the same query-owned scratch allocation.
Memory-pool, transport, or scratch exhaustion is typed and releases all memory,
scratch, slot, task, cache, and transport owners exactly once when the graph
ends, without waiting for nested children still being torn down: a late
child returns its bytes through the shared memory root as it frees them, and
the query's memory view poisons the governor only when it is dropped still
holding bytes. Cleanup failure is never reported as a successful release:
the remaining graph stays observable to the owning supervisor, the node does
not claim a clean terminal state, and readiness or shutdown evidence surfaces
the failure.

One pinned published cut, one discovered live route set, one deadline,
cancellation tree, and execution attempt bound the complete stage graph. Head
cancellation stops and joins every descendant.
Analytical selection binds the logical query to exactly one distributed
attempt: after selection, peer transport close, reset, availability timeout,
authentication, authorization, tenant, digest, protocol, resource, corruption,
cancellation, deadline, and execution failures are all terminal. Oracle does
not construct a successor attempt, does not rebuild the stage dependency
closure, and does not fall back to Interactive. A caller that receives the
failure terminal may submit a new logical query. Retry remains a property of
unrelated Scribe, catalog, and Forge protocols, not of a selected Analytical
query.

Result frames carry query and attempt identity and are followed by one explicit
success, degraded, or failure terminal. Bounded transport buffers provide
backpressure but never spool the complete result. A caller may process frames
incrementally, but the result is usable only after a valid terminal; frames
preceding a failure terminal are invalid as a complete query result. Frames
that do not match the owning attempt identity are rejected before egress.
Protocol duplication is forbidden, and source selection is validated before
execution, so publication overlap never duplicates a row across published and
live sources.

### Admission and memory

Interactive and Analytical paths have separate queues and slot counters.
One atomic aggregate check prevents their combined occupancy from exceeding
Oracle capacity. A configured Interactive slot floor cannot be borrowed by
Analytical work; Interactive work may use idle unreserved capacity. Both paths
share the one governed Bifrost memory root, one scratch root, and one
leader/peer capacity counter.
`QueryClass` is derived from the one returned physical root and is never a
caller-controlled hint.

Local capacity is pod-local and is derived from this node's own CPU and memory,
never from a cluster-wide quota. Total slot units default to the smaller of two
units per effective CPU and the Oracle memory budget divided by the 32 MiB
working set a unit represents; an explicit operator limit replaces that
derivation outright. The resulting total splits once at boot into
`interactive_floor_units + analytical_max_units`, one unit for the Interactive
floor and the remainder for Analytical. `analytical_max_units` of zero is valid
on a single-unit pod: its Analytical admission is refused immediately rather
than queued forever.

Within each path, queries are FIFO per tenant and tenants are selected by
weighted round robin. The arbiter first fills the protected Interactive floor,
then assigns unreserved capacity to the oldest eligible path head while
preserving tenant rotation. A continuously ready path cannot be skipped
indefinitely, and Analytical work never consumes the Interactive floor.

Slots admit; the actual grant sizes only the execution memory ceiling. A slot
unit represents the 32 MiB working set used to derive local capacity; it is not
itself charged as resident query memory. Concurrency is governed by slot units,
actual cooperative reservation by the shared memory root, and spill by the
separately leased scratch share. Every query charges exactly one unit on each
node it runs on, whatever its class: an Interactive query on its leader, and an
Analytical query on its leader and on every participant whose leader stream it
has accepted. The class decides only which capacity rules apply, never the
charge, and a participant's charge therefore carries no demand of its own. A query-local memory ceiling
is derived once at admission:

```text
grant = clamp(bifrost_budget / 2, 256 MiB, bifrost_budget)
spill_share = scratch_limit / 2
```

The grant is not divided by concurrent load: concurrent queries compete in the
shared root, which refuses growth once they fill it. It is a non-reserved
per-query ceiling held for the query lifetime and never recomputed under
running operators. Every leader
and follower query receives a private view over one process-wide Oracle memory
root, never an independently sized pool: the view refuses growth past that
query's own ceiling, and the root's single tracked spill-fair pool, bounded by
the shared Bifrost memory cap that Scribe, Forge, and in-flight transport also
charge, arbitrates what all live queries hold together. No role holds a fixed
share or a precharge, with one exception: when an Oracle-serving node sets up
`DataFusion`'s Parquet footer cache and its Parquet byte-range cache at
composition, it charges each cache's whole budget, `metadata_cache_bytes` and
`data_cache_bytes`, to the Oracle memory root and holds it for the storage
owner's life, because neither cache reports a per-entry charge; a zero budget
sets up no cache and charges nothing. Each budget defaults to a twentieth of
managed memory within 32 MiB..512 MiB and may never exceed an eighth of it.

The byte-range cache keeps the exact byte ranges Oracle scans read from
warehouse objects, which are written once under unique names and never
rewritten, so a cached range never goes stale; staged scratch objects are read
uncached. A scan answered from the cache still counts the bytes in
`bytes_scanned`, and before its first cached range of an object it confirms the
object exists, so a pinned object that vanished still fails the attempt as a
stale object rather than being answered from memory. Apart from that standing
charge an idle role holds nothing, and every charge returns when its owner
ends. Aggregate governed reservations therefore cannot sum above
the Bifrost cap.

The cap comes from the detected process or pod memory limit. `wyrd-server`
keeps at least 1 GiB of that limit outside governed Bifrost memory
(`WYRD_SERVER_MEMORY_MIN_BYTES` raises the minimum); the cap defaults to the
limit less that minimum, and `WYRD_BIFROST_MEMORY_LIMIT_BYTES` may only lower
it. The minimum is accounting headroom, not preallocated memory and not a
ceiling on other server work. A plan that cannot leave both the minimum and a
positive Bifrost cap fails boot, so an 8-GiB pod defaults to a 7-GiB cap.
Every Wyrd pod has at least 4 GiB of memory; boot refuses a detected limit
below that floor, so the smallest supported pod has a 3-GiB cap and a
1.5-GiB query grant.

Only fallible cooperative reservation is hard-limited. Growth DataFusion does
not let fail is still real memory, so it is charged to an explicit process
headroom counter that makes later fallible growth refuse sooner. Dependency
allocations outside cooperative reservation fall in the server minimum; the
pool is the Bifrost safety boundary, not a guarantee against operating-system
or cgroup OOM.

Query parallelism comes from CPU and scanned bytes, not memory. The node's
effective CPU and the pinned input's locality cap it:
`cpu + (4 * cpu - cpu) * (1 - local_ratio)`, never below two, where
`local_ratio` is the pinned bytes held in the local hot tier. Before physical
planning, the Oracle leader narrows that cap to the bytes the query scans: one
partition per started `WYRD_BIFROST_ORACLE_PARTITION_SCAN_BYTES` (24 MiB by
default), at least one. Scanned bytes are measured by one owner,
`scan_sizing::ScanSizer`, on every tier: the query's projected columns in the
units its filters cannot rule out by their recorded bounds, counted in decoded
bytes. A Parquet unit is a row group of a published file, hot file, or staged
run, counted by its uncompressed column chunks from the footer (a file whose
footer cannot be read counts its size on disk); a memtable unit is an append
batch, counted by each column's Arrow data, recorded once at append so sizing
adds stored numbers. Oracle sizes published and hot
files; each Scribe reports its memtable batches and staged runs for the same
columns and filters, and its follower sizes its memtable leaf from the batches
it actually keeps. Neither the grant nor the pinned file count changes it.
Where the rule cannot apply (a filter conjunct that does not plan, a footer
not read, a batch column without a recorded size) sizing counts it in
`bifrost_scan_sizing_fallback_total{reason}`; Oracle counts the row groups it
sized in `oracle_query_row_groups_sized_total`, read beside
`oracle_query_row_groups_scanned_total`, and the query capacity benchmark
flags a window where either diverges.

Because the class is read from the physical root, it follows from this count:
a single-partition plan has no exchange for the distributed planner to cut, so
a point lookup or small scan runs Interactive, and only a scan large enough to
earn several partitions can plan Analytical. Distributed-path journeys set the
partition bytes to one, so their kilobyte tables plan the shape a
production-sized table does. Batch size and join preference are the
`DataFusion` defaults. Oracle retains the planning `SessionConfig` with the
single physical root. The actual grant supplies only the query-owned
`RuntimeEnv`, its `MemoryPool` ceiling, and a per-partition sort-merge
reservation of half the grant's partition share, capped at the `DataFusion`
default so a spilling sort can always merge; it does not reshape or rebuild
the physical plan.

Pinned published and hot scan leaves honor that partition count. The pinned
files are laid end to end and cut into equal contiguous byte ranges, one per
partition; a row group belongs to the partition whose range holds its
midpoint, which is the same rule Iceberg's reader applies to a split
`FileScanTask`. One large file and many small files therefore both use every
partition, and each row group is read exactly once.

Operators and exchange consumers share that issued query ceiling without
separate sublimits. Admission refuses before dispatch when checked graph counts
or scratch demand exceed their finite configured limits. Allocation or
transport refusal after admission is a typed query-resource failure and
cancels the full query; it never borrows from another query's ceiling. A
refusal names the requesting consumer and the pod's largest holders. Scratch
space is separately reserved because spill consumes real disk.

Every session that can spill, whether the leader, a leader-local live
fragment, or a remote Oracle follower, spills only into its node's governed
Oracle spill directory under its query's spill share; the leader and its live
fragments share one runtime and therefore one share. A Scribe follower owns no
Oracle spill directory and runs with spill disabled. Spill merges keep
`DataFusion`'s default fan-in: the per-query memory limit is the only memory
bound, and a sort or merge that needs more non-spillable memory than that
limit fails the query with the typed `QueryResourcesExhausted` error. Oracle
never guesses data shape to avoid it.

Tenant fairness is owned separately by per-tenant FIFO and weighted
round-robin admission, scheduled pod-locally: tenant slot caps are local
scheduling caps rather than cluster quotas. Grants are tenant-blind.

Busy slots are ordinary saturation, not overload. An authorized, executable
query waits for a slot in the tenant-fair queue, which holds at most 1,000
waiting queries per Oracle node across both classes; running queries hold no
waiting place. Both classes share one timeout policy: a one-hour maximum queue
wait (`WYRD_BIFROST_ORACLE_MAX_QUEUE_WAIT_MS`) and a two-hour default total
deadline (`WYRD_BIFROST_ORACLE_DEFAULT_QUERY_DEADLINE_MS`) that a valid caller
`deadline_ms` replaces. A waiter stops at the earlier of queue entry plus the
queue limit and the leader's total deadline, with `QueryTimeout`; queue wait
consumes total time and dequeue starts no new timer. Only the 1,001st waiter is
refused, immediately, with the retryable `QueryQueueFull`
(`WYRD_VALA_429_QUERY_QUEUE_FULL`, reason `queue_full`); nothing ahead of the
queue reserves or waits for a place. A
class with no executable capacity on the pod is refused immediately. Public
HTTP queries bypass the server's global load-shed and request-concurrency
layers so they reach this queue; gRPC reaches it directly.

Snapshot preparation has no admission gate of its own. One tenant-scoped
autocommit statement on the bounded runtime PostgreSQL pool, binding its own
tenant, resolves every referenced table and returns each catalog metadata
pointer with its hot rows. The statement is read-only: it takes no lock,
writes no row, and nothing is released at the query's terminal. Oracle serves
cuts from a cache kept fresh by the `bifrost_cut` notification channel; it
serves cached cuts only while listening, and a query never accepts a cut
older than its own deadline bound. The cache is not trusted for correctness:
the statement also returns each table's definition version/reference and its
per-node publication revisions from
`vala.bifrost_table_publication_revisions`, which a `vala.file_list` insert
trigger advances in the publishing transaction, and live-source selection
validates the cached cut against the revisions Scribes report. A warm query
therefore issues no catalog statement. An identical replay inserts no row and
advances nothing, a rolled-back publication leaves the revision unchanged,
and a restarted Scribe loads its node's revisions before serving captures. Oracle
then reads the selected metadata documents directly and concurrently; a
document missing after a catalog move reacquires the whole cut once, and a
second `NotFound` is terminal. All of this waits within the leader deadline,
which is capped at `max_query_deadline_ms` (default 6 h); Forge's snapshot
retention, never a per-query SQL record, keeps snapshot/data objects readable.
The same cut retains in-memory definition pins through the existing owner;
root-aware metadata cleanup protects them separately from snapshot visibility.
Pin/source binding must not combine one version's schema with another's layout.

Every query carries one execution profile, the analog of `ClickHouse`
`ProfileEvents`. The leader's own steps — `cut_read`, `cut_materialize`,
`scribe_listing`, `provider_setup`, `physical_planning`, `admission`, and
`audit_bind` — are `DataFusion` `Time` metrics timed with
the same scoped guards operators use, `selection_retries` counts selections
discarded by validation, and every operator of the executed root
contributes its own `DataFusion` metrics. The profile is emitted once, after
the leader settles every descendant fragment, as `oracle_query_profile_nanoseconds_total{event}`,
`oracle_query_profile_count_total{event}`, and `oracle_query_profiles_total`,
with events named `Oracle.<step>` or `<operator>.<metric>`, plus one DEBUG
event per query. Operator times sum across partitions, so they are not wall
time; total Oracle time is `oracle_query_duration_seconds`.
Remote peer work reuses one authenticated channel per ready peer incarnation and
endpoint; a changed fence or endpoint connects anew and never inherits the
prior peer's channel. A live leaf profiles `fragment_open_time`, connect
included, and `fragment_wait_time` spent awaiting remote frames; both sum
across its partitions.

An Analytical leader selects at most `max_workers_per_query` remote workers from
the pinned eligible cut, rotating the starting position by the attempt identity
so selection is deterministic, stable across re-projection of the same roster,
and spread across attempts. Only selected workers admit resources, receive
requests, or affect the result; a selected worker that refuses its leader
stream fails that attempt before rows, and only the leader may retry it within
the same query deadline; an unselected replica is absent from the cut
entirely. Memory governance protects
stability; pruning, vectorization, layout, and IO efficiency determine latency.

### Read audit and terminal contract

Oracle read decisions use the one process Scribe outbox. After admission the
read-decision event is staged without waiting; the outbox writer submits it to
Scribe as a `bifrost.system.audit_log` row in a per-tenant batch, like every other
audit event. Rows are not held for that write; a retryable failure is logged,
counted through `outbox_write_failures_total{outbox="scribe"}`, and retried with
the identical batch, and a terminal rejection is logged, counted through
`outbox_events_lost_total{outbox="scribe",reason="rejected"}`, and consumed. The
`outbox_pending{outbox="scribe"}` gauge reports writes the process still owns
before Scribe acknowledges them or their loss is counted, and shutdown drains
the outbox to its deadline. One logical query produces one read-audit event; distributed stages
produce none. An Interactive event records `Local` execution on one node; an
Analytical event records `Distributed` execution over every Oracle in the
frozen participant cut, leader included, with the followers as its workers.

Query streams are length-delimited, terminal-safe frames. A full queue before
framing is `QueryQueueFull`; any other admission refusal before framing is
`QueryAdmissionRejected`; governed memory, scratch, or
exchange exhaustion after framing is `QueryResourcesExhausted`. Cancellation,
deadline, peer loss, and execution failure have typed terminal outcomes. A
stream never represents partial rows as success.

A query failure carries the same complete derive-backed catalog problem, code
and details, whether it occurs before the first frame or after rows were sent,
in Interactive and Analytical execution over HTTP and gRPC. The failure
terminal holds that problem, every SDK raises it unchanged, and collecting a
result returns no partial rows. Distributed workers forward an execution error
only as text, so a catalogued Oracle error crosses as its tagged serde form
inside the existing external `DataFusion` error, and the coordinator
reconstructs it in one place before mapping the failure; no failure is
classified from its human message. Only a failure with no catalog identity
becomes `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.

A public distributed-plan or execution-path `EXPLAIN` surface is deferred and
is not part of this delivery. Selected-path evidence reaches callers only
through the success terminal of the one public query operation. The public
query deadline range is `1..=u32::MAX` milliseconds across Rust, HTTP, gRPC,
Python, TypeScript, and MCP.

`bifrost.system.audit_log` is the authoritative retained audit history. Every
audit event is staged in memory on the process Scribe outbox, whose one
background writer cuts each tenant's writes into producer lanes keyed by
request, Verifier execution, or a queued submission's idempotency key, lets a
lane's slice linger until its oldest write is 50 ms old or it holds 4,096 writes,
as a Kafka producer's `linger.ms` and `batch.size` do, groups a lane's slice by destination, and
builds each group's rows once into dense Arrow batches from the owned values
and encoded Variants each write staged, never one batch per write. Each frame
is encoded as Arrow IPC, which sizes and splits it and derives its batch id
from that content. The local Scribe receives the frame's rows in-process as
server-built canonical batches under ordinary admission, without decoding or
revalidating values the producer built; from a pod without Scribe, the IPC
bytes travel to a Scribe pod over the mutually authenticated peer plane, which
validates them in full. Both carry the same rows under the same batch id. Audit frames are submitted under the
platform audit principal, the one principal Scribe admits for the reserved
system owner's audit history. A retryable failure resubmits the identical
slice, so Scribe's durable batch-id fence absorbs a write whose
acknowledgement was lost; later arrivals in that lane wait for the next slice.
Producer lanes are independent of Scribe shards, which keep their own routing,
WAL, and fence. A row carries
only decision content: there is no chain, sequence, or publication progress,
and readers order by decision time. Writes Scribe has not acknowledged are lost
on abrupt process death or an expired shutdown deadline; observable loss is
counted. No PostgreSQL audit staging, publisher, direct-Iceberg relay, or
separate `platform.audit_log` may become a second historical authority.

Audit events are appended only where an authorization decision was made. Scribe
batch commits and Forge maintenance transitions evaluate no permission and are
recorded as lineage in `vala.scribe_batch_commits` and `vala.forge_operations`;
they emit no audit event. The publication path therefore evaluates no new
permission and appends nothing, so retained audit history cannot feed itself.

`bifrost.system.audit_log` partitions daily, deviating from the hourly granularity
of the high-rate telemetry tables. Forge cannot bin-pack across partition
boundaries, so hourly partitions would permanently cap every audit object at one
hour of a tenant's audit traffic regardless of compaction settings. Audit also
differs from the telemetry tables on every axis partition granularity responds
to: one row per authorized request rather than continuous high-rate ingest,
date-range rather than recent-window queries, and retention measured in years
rather than days.

The four verification tables — `bifrost.drift.observations`,
`bifrost.verification.execution_inputs`, `bifrost.verification.execution_outputs`,
and `bifrost.verification.results` — also partition by UTC day on each table's
time column — `created_at` for observations, `recorded_at` for execution
inputs, and `ended_at` for execution outputs and results —
never by Verifier, subject, binding, or tenant ID. A Drift schedule window does
not determine partition size.

## Maintenance: Forge

### Scheduling, admission, and fences

Forge follows `RisingWave`'s Iceberg maintenance model. One replica holds the
Forge leader term through a single Postgres election row with a heartbeat and
expiry; every replica can execute Forge work, but only the leader decides it.
The leader's schedule is process memory and starts empty on every new term:
no compaction count, in-flight dispatch, or maintenance membership survives a
leader change. A successful Iceberg commit notifies the leader in-process on
its own replica or over the private peer route from another pod. Compactors on
any replica pull due table identities up to their free capacity; the worker
loads current Iceberg metadata and plans the rewrite itself, so no leader
decision performs catalog or object-store IO. Reports settle only the commits
the dispatch captured, and a report for an unknown or timed-out dispatch
changes nothing.

Scribe hot-promotion debt is the one durable scheduling input: a new leader
and every heartbeat read outstanding `file_list` promotion debt, so a
promotion lost with a dead leader is recovered by its successor. An hourly
leader timer runs maintenance for tables that joined its sets through a commit
since the term began: manifest rewrite for opted-in tables, then snapshot
expiry, then expired-object and never-published orphan cleanup. A failure on
one table is logged and the pass continues. Durable task rows record attempt
evidence and recovery; retryable rows are reclaimed with tenant-fair
admission, but no durable queue decides what runs next.

A table's one `claimed`, `running`, or `prepared` task row is its exclusive
lease: claiming the row acquires the table, contention is a refused claim at
admission, the attempt's heartbeat renews the claim, and the attempt's single
terminal transition, committed only after that heartbeat has stopped, releases
it. A strategy that decides its own outcome returns that decision for the
settlement seam to commit; it never releases the claim itself. At most one
durable task attempt therefore owns the lease and fence for a
tenant-qualified table. Within that attempt, admitted ordinary compaction plans are independent child
operations: fitting siblings may rewrite and publish concurrently, while
bounded per-tenant and per-worker admission also permits independent tables to
progress concurrently. Each plan binds the owning tenant, table, task, plan
hash, base snapshot, target branch, attempt UUIDv7, operation ID, output
generation, lease, and fence. Loss of authority cancels and drains physical
work before settlement.

Forge owns runtime leases, pod-local plan admission, read streams, writer
fanout, upload buffers, close futures, SQL state, audit, reconciliation, and
garbage collection. Resource admission may defer a plan but never changes
table file geometry or creates a second grouping algorithm.

### Scribe hot promotion

Forge first promotes committed Scribe hot objects unchanged. The promotion
worker validates the exact footer-derived Iceberg `DataFile` projection against
the claimed `file_list` row, bound table, partition, path, checksum, and target
branch. A record's table version resolves the exact retained native definition
and
footer schema; its native spec, complete typed tuple and sort order must agree
with that definition and the durable SQL authority. Validate contradictions on
the worker revalidation path, without silently suppressing corrupt promotion
debt during planning. Objects sealed before an edit keep their original Bloom,
partition and sort policy. Do not choose a newer schema or current Bloom list
merely because field IDs happen to match. It then performs one fenced
duplicate-checking fast-append
`commit_once`, which writes one manifest per partition spec. Promotion writes no data object and invokes no compaction core.

On a definite compare-and-swap conflict, Forge refreshes the branch head and
revalidates the exact `file_list` rows, object/footer evidence, absence of an
equivalent promoted entry, branch, lease, and fence. When all assumptions hold,
the same attempt and operation ID may make at most one additional `commit_once`
within the original deadline. Another conflict, changed assumption, or expired
deadline settles the attempt as definitely uncommitted and leaves the rows as
`file_list` promotion debt that a later leader sweep retries under a new
attempt. No retry rewrites or reuploads
the Scribe object.

An ambiguous catalog result keeps the same attempt and operation ID. Forge
refreshes metadata and reconciles exact snapshot properties and manifest
entries without retrying the catalog call. A second attempt cannot begin while
the first is unresolved. After exact committed evidence exists, one fenced SQL
and lineage transaction records the promotion snapshot on every claimed row and
switches Oracle authority. During the catalog-to-SQL interval, Oracle
suppresses a hot object only when its pinned snapshot contains the exact path,
checksum, and operation evidence.

### Managed compaction and publication

Iceberg maintenance rewrites eligible live files toward the table's
`write.target-file-size-bytes` resolved in the selected table-version
definition. Creation or an effective definition edit resolves the authorized
table/deployment default (approximately 1 GiB); an environment change cannot
silently alter an existing version. Each attempt pins its output version and
resolves writer/layout policy through the shared existing owner. Row-group
size is
independent and normally 128 MiB. A physical file rolls from the managed
writer's encoded estimate after a completed write; partition and end-of-stream
residue are valid. Neither target is a universal physical object-size
guarantee. Planning groups files by their complete physical partition, spec
included, so a rewrite never crosses a partition boundary to meet a target, and
each output records its selected table version and the native spec, partition
tuple and sort order its rows actually have. Resolve input definitions from
their own versions; a rewrite uses its explicitly selected output definition,
not a relabeling of existing files. Forge never rewrites history to the current
layout on its own. Maintenance snapshot commits never increment the version.

The managed compaction core is the sole owner of candidate selection, grouping,
bin packing, delete application, sorting, partition fanout, bounded concurrent
writing, rolling, and output `DataFile` production. After it produces real
`CompactionPlan` values, each Forge worker owns one strict FIFO queue of them.
The queue starts only its head when running parallelism has room; a later
smaller plan cannot bypass a blocked head, and pending parallelism bounds the
queue itself. A plan larger than the worker's total parallelism is refused.
Memory is not a queue figure and Forge makes no memory estimate.

Each rewrite attempt runs `DataFusion` over a fresh view of the shared Bifrost
memory root, so its reservations are charged against the same cap Scribe,
Oracle, and transport charge, and return when the attempt's context drops.
Spillable operators spill beneath the data root's `forge-spill` directory,
which boot clears of stale files. A refused fallible growth fails only that
attempt: nothing partial is published, and the durable task retries through
its ordinary bounded backoff once memory is free. One plan executes within one
worker; Forge does not split a compaction plan across pods.

The managed core consumes the attempt cancellation tree, output identity, and
closed physical observer. It never owns tenant authority, leases, SQL, audit,
reconciliation, object GC, or catalog commit.

The non-committing boundary returns exactly:

```text
RewriteHandoff {
  base_snapshot_id,
  rewritten_data_files,
  applied_position_delete_files,
  applied_equality_delete_files,
  output_data_files,
}
```

Forge validates every identity against the exact base snapshot. Rewritten data
identities and output identities are globally unique; duplicates fail rather
than being hidden. Applied delete vectors are evidence-only sets and may
deduplicate by their complete canonical identity. Reading a delete file does
not make it removable because it may still apply to unselected live data.
Forge does not reselect, regroup, reconstruct, edit, or rewrite handoff files.

Forge reads the hidden `_row_id` and `_last_updated_sequence_number` beside
the logical projection, carries them in its internal physical batch, and
writes those exact values for every surviving row, so a row keeps its lineage
identity across any number of rewrites. Lineage evidence stays in the output
`DataFile`s; the five-field handoff is unchanged.

The commit adapter derives delete-file disposition from the immutable base
snapshot and the applied-delete evidence; the handoff remains exactly five
fields. A position-delete file is removed only when every referenced live data
file is in `rewritten_data_files` and the managed core applied its positions.
An equality-delete file is removed only when its partition/spec scope and data
sequence number cannot apply to any surviving unselected data file. Otherwise
the delete remains live. Output data and file sequence numbers are assigned by
the Iceberg commit so an applied delete does not reapply to replacement rows.
Missing target evidence, mixed sequence semantics, or an unproven surviving
scope fails commit validation.

Attempt output paths are table-bound and identify the selected output table
version:

```text
{table}/data/forge/{table_version}/{attempt_uuidv7}-{writer_ordinal:05}-{writer_uuidv7}.parquet
```

The version segment is the canonical decimal table version, not a second
writer-recipe fingerprint. The managed core recognizes output definition
compatibility by that version while keeping its existing selection/execution
ownership. Each physical writer owns its filename counter and UUIDv7 suffix;
`writer_ordinal` is minimum-width five-digit canonical decimal and may restart
for another writer because `writer_uuidv7` provides cross-writer uniqueness.
Forge separately assigns each opened output one attempt-global
`OutputIdentity.logical_ordinal` for observer, drain, reconciliation, and
cleanup evidence. The logical ordinal is not encoded into or reconstructed
from the object path. Cancellation drains every writer and retains exact
produced-or-possible output evidence. Forge renews and verifies the lease and
fence immediately before the initial `commit_once`. The commit adapter removes
the exact rewritten data files, adds the exact outputs, preserves delete
correctness, and writes operation and lineage properties in the same snapshot.

Committed, definitely uncommitted, fence-refused, cancelled, and uncertain are
distinct per-plan outcomes. Concurrent siblings may optimistically race on the
same branch head and cause an expected compare-and-swap conflict. On a definite
conflict, Forge refreshes the branch head and revalidates the retained planning
snapshot, selected output table version/definition, selected-input existence,
lease and fence. A definition edit invalidating output assumptions cannot be
silently adopted by the retry.
When those assumptions remain true, the same plan operation, output generation,
and objects may make at most three further `commit_once` calls after fixed
1s/2s/4s delays within the original deadline. Exhausted retries, an expired
deadline, or a changed assumption settles only that plan as definitely
uncommitted. Successful sibling snapshots remain visible; the task succeeds
when any admitted plan publishes, and later discovery replans remaining debt
from the current head. No conflict retry creates new output objects.

An uncertain attempt protects its outputs and reconciles under the same
identity; it never retries the catalog call, starts a fresh attempt, or reports
terminal success until exact catalog evidence resolves it. Terminal SQL and
lineage settlement occurs exactly once.

### Convergence, retention, and cleanup

Each rewrite snapshot binds its selected output `table_version` and records
canonical evidence fingerprints for its base
live data set, resulting live data set, produced outputs, semantic debt shape,
and lineage snapshot. Before object IO, Forge reconstructs those exact sets and
refuses a rewrite when selected files are prior outputs, semantic debt is
unchanged, and no live-set delta affects the selected groups or delete scope.
Path churn alone never authorizes another rewrite. Missing, expired, partial,
or contradictory lineage evidence fails closed.

Data-file compaction, manifest rewriting, snapshot expiration, expired-object
cleanup, and never-published orphan cleanup are separate protocols. The leader
timer orders them per pass as manifest rewrite, snapshot expiry, then cleanup.
Expiry protects readers with a time window, as Iceberg, Trino, Delta, Paimon,
and Snowflake do, and tracks no query. Forge records in Postgres when it first
observes each retained snapshot as no longer current; a replaced snapshot is
eligible once that observation is `snapshot_retention_secs` (default 6 h, never
below the maximum query deadline) old and no in-flight compaction, unsettled
promotion, or other authoritative root retains it. A query pins a snapshot
while it is current, so the snapshot cannot expire before the query's deadline.
The leader plans an expiry task only when some replaced snapshot is already
past the window, so a quiet table is rechecked every pass and every planned
task changes the table.
Snapshot expiration preserves active refs, unresolved attempts, reconciliation
evidence, and the lineage snapshot referenced by the branch head. Orphan GC deletes only
objects proven unreferenced and outside every active or uncertain attempt.
Committed Scribe hot objects in `file_list` that lack exact promotion evidence
are hard GC roots even when no Iceberg snapshot references them. Once its
snapshots expire, expired-object cleanup deletes without a further age floor and removes the matching
terminal `file_list` row in the same completion transaction.
Every destructive effect holds the table's exclusive maintenance authority
through its known outcome, and the Forge lease TTL bounds that hold: an
object-store call still running at the bound is an uncertain effect left for
idempotent replay. Oracle never takes the authority, so maintenance never
waits on a query and a query never waits on maintenance.
An open Scribe fragment retains its local Arrow batches, staged resources and
definition pins until its stream completes or drops. Local files remain owned
by Scribe; their retained table-version metadata remains a definition root.
Definition metadata has separate root-based retention. Always retain the
current table version's original native metadata location and reference, even
after data-only commits move the current metadata pointer. Retain older
versions while any WAL, staged data, fence, promotion/file-lineage, active
query pin or accepted-retry retention root remains. A fence survives WAL retirement as a root.
Settled promotion/rewrite evidence continues to root a definition while
retained files need it, so a later query can still resolve their table version.
Protect these references and objects in both expiry and orphan cleanup after
native metadata-log rollover; native auto-delete cannot bypass this protection.

Root discovery recovers before destructive cleanup. Derive work roots from
existing durable records and pin owners, not a new per-batch SQL registration.
An unavailable node's unsettled retained storage or missing root inventory is
not proof of safety. Serialize root removal/deletion through existing
maintenance/pin/fence owners so a newly needed definition cannot race with
collection. Prune only noncurrent versions after all roots and accepted-retry
retention are proven gone. Without an established retention horizon, retain;
do not invent a definition TTL. Keeping definition metadata does not retain
every historical data snapshot it mentions or change row-data retention.

No cleanup infers safety from path shape; only snapshot expiry and orphan GC
use age.

## Resource and failure invariants

- Every Wyrd-owned queue, mailbox, stream, fanout, task set, buffer, staged
  namespace, and object upload lane is bounded. Scribe scratch and Oracle
  memory pools, scratch roots, and spill paths remain hard-bounded. Forge
  rewrites charge the shared Bifrost memory root and spill under
  `forge-spill`. A pinned dependency-internal queue may
  instead provide finite byte backpressure when Wyrd cannot configure its item
  count; architecture must name that exception rather than claim ownership it
  does not have.
- Global resource owners account tenant and table attribution without creating
  an independent root pool per tenant.
- Cancellation is structured: stop admission, cancel descendants, reconcile
  uncertain effects, and release ownership exactly once. Query graph release
  does not join dependency-owned teardown; memory those children still hold
  stays charged to the shared root until they free it.
- Durable operations separate definite failure from uncertain completion.
  Uncertainty retains identity and evidence until reconciliation.
- Catalog compare-and-swap and lease fences own publication authority. An
  object-store PUT alone never makes data visible or safe to delete.
- Audit records authorization decisions. Every boundary that evaluates a
  principal's permission records exactly one allowed or denied event.
  Permissions are blocking; audits are non-blocking: the check completes before
  the operation proceeds or refuses, and the event is staged on the shared
  Scribe outbox without the operation waiting for its write, and no
  operation holds rows for that write or fails because of it.
- Engine-internal transitions — Scribe batch commits, Forge maintenance,
  reconciliation, storage lifecycle — evaluate no permission. They
  record lineage in their own operational tables and structured diagnostics,
  never canonical audit.
- Operator-level Forge lineage uses the tenant-bound fenced capability defined by
  repository SQL rules; it is not a generic cross-tenant executor.
- No retry or successor attempt in any Bifrost protocol widens tenant, table,
  snapshot, participant, deadline, permission, or resource authority.

## Telemetry

Scribe, Oracle, and Forge each own one closed telemetry registry used by
production behavior, verification, and operator documentation. Four surfaces
carry Bifrost's observability, and each fact belongs to exactly one of them.

The public Prometheus catalog answers what an operator must be able to graph
and alert on without reading code: demand, queue depth and age, active
ownership, durable results, latency, failure class, physical data flow, and
outstanding maintenance debt. It is deliberately small and closed; a subsystem
does not add a family because a value exists.

Protocol mechanics — lease and fence decisions, catalog calls, reconciliation,
cursor movement, scheduler passes, and resource envelopes — belong to
structured traces, where the identities that make them useful are legal.
Durable audit and task rows remain the authority for what actually happened,
and no metric is evidence of a durable fact. Unresolved authority or
in-progress recovery is a readiness signal, not a metric.

Every active gauge decrements on success, refusal, retry, uncertainty,
cancellation, and failure. Metric labels use only closed, bounded dimensions
such as stage, decision, outcome, close reason, query class, route reason,
fallback reason, and stage role. Tenant, table, SQL, object path,
query ID, task ID, snapshot digest, and other high-cardinality values are
scrubbed trace fields, never metric labels. Physical size, latency, throughput,
and SLA claims require measured evidence from the production path.

### Measurement meanings

Request counters count attempts, including idempotent client retries. Process
counters restart at zero with the process and are not exact durable accounting
across a restart; durable batch, file, and task rows answer that question.

- **Gate.** `bifrost_gate_requests_total{operation,outcome}` and
  `bifrost_gate_request_duration_seconds` measure request opening: a query
  request succeeds when its stream opens, not when it completes.
  `bifrost_gate_query_streams_total{outcome}` and
  `bifrost_gate_query_stream_duration_seconds{outcome}` record the
  client-facing stream's one terminal — `success`, `degraded`, `failed`,
  `rejected`, or `cancelled` — at the server edge. Neither includes client
  network or SDK time.
- **Scribe.** `bifrost_scribe_ack_seconds` measures each ACK attempt.
  `bifrost_scribe_memtable_rows_inserted_total` counts rows the shard inserted
  into the live memtable in this process; an idempotent same-process retry adds
  zero, and replay after restart counts again in the new process.
  `bifrost_scribe_memtable_bytes_inserted_total` counts the same rows'
  retained Arrow bytes, the in-memory size ingest throughput is measured in.
  Receipt
  `accepted_rows` is a client contract, not a newly-stored-row measure.
  `bifrost_scribe_staging_live_members`, `_live_bytes`,
  `_oldest_member_timestamp_seconds`, and `_outstanding_claims` are published
  from the staging assembler's own state, so members restored after restart
  appear in backlog. `bifrost_scribe_lane_queued{lane}` is waiting jobs only;
  `bifrost_scribe_lane_active{lane}` is running jobs.
  `bifrost_scribe_publication_files_total` and `_bytes_total` count committed
  publication output after its catalog transaction commits.
  `bifrost_scribe_variant_layouts_learned{namespace}` is the process's cached
  learned Variant layouts per Bifrost namespace, republished on each install.
- **Oracle.** `oracle_query_duration_seconds{class,outcome}` starts after Gate
  dispatch and measures Oracle execution through its terminal; the production
  HPA and query reports read it. Outcomes are `success`, `degraded`, `failed`,
  `cancelled`, and `client_drop`; a Degraded terminal is never counted as
  success. `oracle_queries_queued` is waiting work and `oracle_queries_active`
  is admitted work only. `oracle_admission_total{class,outcome,reason}`
  pre-registers only the decisions an admission branch can make, and
  `oracle_admission_queue_duration_seconds` records each waiter once. Scan
  counters report the executed plan's files, bytes, partitions, and row
  groups.
- **Variant layout.** `bifrost_variant_residual_share{writer}` records, for
  each final Scribe object and Forge output, the uncompressed bytes of its
  Variants' root residual `value` leaves as a share of all their
  non-`metadata` leaves, read from the file's footer; `writer` is `scribe` or
  `forge`. A file with no Variant bytes records nothing. A rising share means layouts are missing keys. Forge
  reads each output footer once after the rewrite and logs, never fails on, a
  footer it cannot read. Recovery-stage runs are not measured.
- **Storage.** Parquet footers are cached in `DataFusion`'s node-wide metadata
  cache and warehouse byte ranges in the node's byte-range cache, each charged
  in whole to the Oracle memory root at composition.
  `bifrost_footer_cache_{hits,misses}_total` and
  `bifrost_range_cache_{hits,misses}_total` count their lookups.
- **Traces.** A query is one trace: `bifrost.gate.query.stream` spans the
  client-facing stream through terminal or drop, with `bifrost.gate.query`
  dispatch as its child; `bifrost.oracle.stream` spans Oracle execution through
  terminal cleanup and records the executed plan's final integer
  `rows_scanned` and `bytes_scanned` when its scans report them (Parquet
  scan rows and bytes plus a Scribe memory read's examined rows and accessed
  Arrow buffer bytes, counted once per validated live completion; memory
  bytes are scan work, not storage IO, retained heap or transport size), and
  `bifrost.oracle.peer.fragment{role,outcome}` spans each remote fragment. Work `DataFusion` spawns inherits the query span, so remote
  fragments are child work of the query that dispatched them. Each operation
  span records exactly one `outcome`. A request that continues a sampled
  client trace keeps it: the Gate and Oracle spans join the caller's trace,
  and the request's behavior-catalog spans are also copied to the tenant's
  `bifrost.traces.spans` (see `wyrd-design.md`, Trace context). A queued write's
  batch carries up to 32 producer trace contexts, and `bifrost.gate.write`,
  the CONSUMER span, links each one from its start, so a batch linking a
  sampled producer is also copied to the producer's tenant as a linked trace.

## Public surface

Bifrost is projected consistently through Rust, HTTP, gRPC, Python, TypeScript,
MCP, generated schemas, machine-readable documentation, and stable error codes.
`wyrd-spec::vala::api` owns public wire contracts; `wyrd-server` serves every
route. The one public client composition is:

```text
Rust / Python / TypeScript SDK
  -> wyrd_client::Bifrost
  -> shared wyrd-client HTTP and gRPC transport
  -> wyrd-server public edge
  -> server-owned Scribe, Oracle, and Forge
```

`wyrd_client::Bifrost` owns table management, buffered ingestion, query
streaming, and query lifecycle behavior. Internal `QueryClient` and
`BifrostGrpcTransport` mechanics are not sibling public clients. Gate is the
server dispatcher, not a deployment target or language-SDK owner.

Buffered ingestion admits one logical record, such as every tall row of one
Drift observation, immediately and all-or-none: either every row is admitted
or none is and the caller receives `WYRD_CLIENT_429_QUEUE_FULL`, so flushing or
backing off and resubmitting the same record cannot duplicate a prefix.
Admission never waits on a network send. A send that ends without a definite
ACK or refusal, including an exhausted transport retry budget, retains its
batch and stable UUIDv7 for a later retry; only a definite refusal settles the
batch as a counted loss.

The surface includes:

- table creation/registration and describe under `/v1/bifrost/tables`, returning
  resolved schema/layout, immutable table identity and `table_version`;
- authorized `PATCH /v1/bifrost/tables/{namespace}/{name}` with
  `expected_table_version`, optional partial layout, nullable custom-column
  additions and stable-ID custom renames. Validate the complete result; omitted
  lists preserve values and supplied lists replace them. Supplied lists name
  columns after the edit's renames, and kept lists follow a rename by field
  ID. A stale edit is a typed HTTP 409 with no mutation; malformed/unsupported
  edits are 400 and missing `bifrost_table:write` is 403, audited as
  `bifrost.bifrost.alter`. Time-column identity and canonical/managed schemas
  remain protected; re-registration never alters a table;
- `AppendReceipt { batch_id, accepted_rows, durability }`, where `durability`
  is the closed `AppendDurability::Acknowledged` value; the synchronous append
  response never reports staged, published, promoted, or rewritten state;
- terminal-safe query streaming at `POST /v1/query`, with ordered typed bind
  values (`params`) for the `$1..$n` placeholders in its SQL;
- canonical SQL observation reads through the ordinary Bifrost query contract;
- gRPC ingestion through `wyrd.v1.BifrostIngestService`;
- gRPC query projection through `wyrd.v1.BifrostQueryService`;
- the `wyrd.bifrost` Python projection and matching TypeScript SDK;
- agent-facing read and write operations governed by explicit permissions.

The version representation is the single lossless contract defined under
Table versions and publication: JSON decimal strings, TypeScript `bigint`,
exact integers on native boundaries. No alternate field names or numeric-JSON
shortcut exists. The SDK writer obtains the version with schema/layout from
describe or registration, retains it with the immutable table identity and
sends it on every native frame, including buffered/retried batches. Gate binds
canonical built-in input to a versioned server projection. None of the SDKs
allocates versions or silently rebinds queued content after an edit. A stale
version on a new batch is a typed conflict directing a describe refresh and
new batch construction; an accepted retry uses its original fence version and
strict equality checks. Native input remains subject to structural/type/
nullability/reserved-field/value checks, including built-in contracts.

Observation namespaces such as `bifrost.traces`, `bifrost.metrics`, `bifrost.logs`,
`bifrost.drift`, `bifrost.verification`, `bifrost.gateway`, and
`bifrost.system` remain tenant-qualified Bifrost tables. Canonical SQL is their
only read contract;
the namespace does not create another storage or authorization model.

Permissions are scoped through `BifrostTable`, `BifrostRecord`, and
`BifrostQuery`. Generic writes cannot target reserved or system-managed tables;
`bifrost.gateway.calls` and the three verification tables
(`bifrost.verification.results`, `bifrost.verification.execution_inputs`, and
`bifrost.verification.execution_outputs`) accept no public write at all: Gate refuses every
principal. Only the server-internal capture writer writes them, together with
the capture spans in `bifrost.traces.spans`, and the tenant copies of server
spans join them there through the same outbox; result rows carry the verified
Card as `card_uid` and the exact Verifier Card as `verifier_card_uid`. Each
row carries the verified pair it was produced under, which Scribe validates as
principal ids and stamps as `sub_principal_id` and `act_principal_id`, so one
tenant's batch may hold rows of many callers: the call's pair for gateway
capture, the caller's pair for a `verify` result and its inputs, the submitting
caller's pair for `submit` inputs, and the execution's accepted subject with
the tenant `System` principal as actor for a queued result. A `submit`
creates its queued `wyrd.verifier_executions` row only after Scribe
acknowledges its `execution_inputs` batch, through the existing batched
request outbox; staged inputs not yet acknowledged are lost on abrupt process
death, and the execution then never becomes visible. The row stores no input
values: the runner resolves them, and the accepted routes, from that input
record. A group or schedule execution's selector record is staged on a slot
reserved before its PostgreSQL commit and is staged right after it; a runner
claiming the execution before the record is readable retries it as evidence
unavailable. A pod without
Scribe submits through `ScribeCapturePeerService.IngestCapture` on the peer
listener, which the same `wyrd-peer` mTLS admits; the request names its
tenant and one of those five tables or `bifrost.system.audit_log` explicitly,
carries no token, and its rows carry their own subject and actor pairs. Audit
rows carry their own identities as row content, so an audit batch submits the
no-verified-caller sentinel pair. Every outbox frame names that
no-verified-caller marker as its transport principal; it grants nothing. It is
refused for any
other table, and for the reserved system tenant unless the table is
`bifrost.system.audit_log`. Sensitive-column metadata remains descriptive; table query
permission governs every column, including GenAI fields stored on trace spans,
except that a projection reaching the `bifrost.gateway.calls` request or response
payload columns also requires the tenant-wide gateway payload-read permission.

Bifrost query permissions carry an object axis. A `bifrost_query:read` grant is
scoped either to every object (`all`) or to a named Bifrost object: a
`{ catalog, schema }` schema scope, or a
`{ catalog, schema, table_uid }` table scope keyed by the existing Bifrost
`TableUid`. A query names no tables until it is planned, so `POST /v1/query`
admits on the coarse operation capability only, and the authoritative object
decision runs inside the Oracle once `pin_cut` has resolved the complete
`PinnedSealedTable` set — before provider registration, physical planning,
admission charging, read-audit acceptance, peer dispatch, or any source read.
Every resolved table, including tables reached only through a join or an
expansion, must be covered; one uncovered table denies the whole query with
`WYRD_VALA_403_QUERY_FORBIDDEN` and streams no rows.

The permission digest bound into every stage assignment is derived from the
approved scoped permission together with the exact authorized table identities,
so a worker cannot execute against a wider object set than the leader approved.
This is object-scoped RBAC — a static role grant over named objects — and adds
no grant table, policy lookup, query-path database lookup, cache, or second
checker.

There is no `WarehouseCard`, `wyrd.warehouse` compatibility surface,
asynchronous query-job API, result polling/redirect protocol, or client-selected
physical tenant scope.

## Explicit non-goals

Bifrost does not provide:

- DML through Oracle, CTAS, distributed writes, or arbitrary code execution;
- a materialized shuffle service, independent distributed scheduler, or
  detached stage runtime;
- partial successful results, automatic retry of a selected analytical query,
  or silent cross-engine fallback after analytical selection;
- a public distributed-plan or execution-path `EXPLAIN` surface in this
  delivery;
- result caching as a correctness dependency;
- cross-region query execution or autoscaling semantics;
- cross-pod Scribe assembly or coordination of one append across ingest pods;
- a second compaction planner, a managed-core catalog commit, or global Forge
  serialization;
- external-system writes through `Source` or any Bifrost path;
- compatibility routes, legacy storage names, or alternate durable formats.

## Platform audit sentinel

`DataTenantId::SYSTEM_OWNER` is the durable platform tenant for security events
that cannot safely be attributed to caller-controlled tenant data, including
peer refusals made before the receiver's own query or stage state
binds a tenant. Its canonical row is UUID
`00000000-0000-7000-8000-000000000000`, slug `wyrd-system`, display name
`Wyrd System`, status `active`, and `deleted_at IS NULL`. Provisioning and boot
verification fail closed on conflicting identity or attributes. Unverified
payload bytes can never select an audit tenant.

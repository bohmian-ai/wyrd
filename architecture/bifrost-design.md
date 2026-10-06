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
lifecycle for Bifrost HTTP, gRPC, and MCP operations. Vala owns the Bifrost
engine and its three cohesive subsystems:

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
record. Postgres is the tenant-isolated catalog and durable control plane.
DataFusion is the vectorized execution engine. Managed compaction and Iceberg
components perform bounded computation behind Vala-owned contracts; they do not
authenticate callers, serve the network, own tenant authority, or commit
durable Wyrd state independently.

## Table and row identity

Every Bifrost table has one server-owned managed envelope with these required,
non-null columns:

- `wyrd_request_id`: server-minted or validated request correlation, the
  join key to audit and to the batch-commit fence;
- `wyrd_event_time`: when the observed thing happened — the caller's value,
  validated against the acceptance window, or `wyrd_ingested_at` when the
  caller supplies none;
- `wyrd_ingested_at`: when Wyrd accepted the batch. Scribe reads it once per
  batch from PostgreSQL (`statement_timestamp()`) at admission, stamps the same
  value on every row, and stores it on the batch's
  `vala.scribe_batch_commits` fence. A caller can never supply it.

The column names, types, stable ids, and order are declared once, in
`vala_bifrost_redux::tables::managed_columns::MANAGED_COLUMNS`; every physical
schema and every reserved-name check derives from that declaration.
`wyrd_ingested_at` comes from the database clock, so it is comparable across
replicas, but it is the admission instant, not a commit order, and is not an
incremental-read checkpoint.

The tenant is not a row column. It is a property of the physical table, of
each Parquet file, and of each in-memory Scribe bucket, all bound from the
authenticated principal.

Nullable `run_id` and `card_uid` provide optional Card/Run correlation.
Required, non-null `principal_id` identifies the authenticated publisher. None
participates in row identity.

For OTLP records, table-owned projection reads correlation only from the final
record-level `wyrd.card_ref` and `wyrd.run_id` attributes. Every source
attribute is retained in the record's Variant attribute column; when a
collection repeats a key, the final occurrence is the one stored, matching the
OTel data model and the correlation rule. The values use the existing `CardRef` and `RunId` text
grammars. Any client Card UID is ignored; Scribe stamps only the UID from the
verified principal scope.

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
reusing an accepted ID requires the same schema fingerprint, row count, row
order, and payload digest; any mismatch is a stable batch-identity conflict.

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
definition ownership, not a weaker tenant scope or a separate storage mode. A
built-in's per-tenant row is materialized on first use from its owning
definition rather than seeded at tenant provisioning, and describing one
materializes it exactly as ingesting into it does. A client that must confirm a
fixed table before it writes — an SDK describing `vala.drift.observations` and
`vala.eval.observations` at startup — therefore sees the same table a first
ingest would create, instead of a missing-table error in a tenant that has not
written yet.

## Storage format and Variant

Every Bifrost table, built-in and user-defined, is created as Iceberg format
v3, and physical-table validation refuses a table that is not v3. There is no
other format and no mixed-version state. Appends assign row ids through the v3
first-row-id mechanism. The hidden lineage columns `_row_id` and
`_last_updated_sequence_number` are Iceberg metadata, never logical columns:
they are absent from the table schema, its fingerprints, and every query
result, and a Forge rewrite preserves both for every surviving row.

Variant is a canonical column type (`DataTypeSpec::Variant`) for open,
self-describing values. On the Arrow wire a Variant column is the
`arrow.parquet.variant` extension over the unshredded `metadata`/`value`
binary Struct; Iceberg and Parquet store it as the v3 Variant type. A field
declared Variant must carry that extension at every nesting level: missing or
foreign extension metadata is `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`, even
when the storage Struct matches. Extension metadata is not part of a
fingerprint, so files with different shredding layouts share one table and one
fingerprint.

A Variant value reads back with the type, nesting, and value it was written
with. Integers stay integers and never pass through floating point; doubles
keep their IEEE meaning; strings, booleans, bytes, arrays, and objects keep
their type; and an absent object key stays distinct from a key whose value is
null. Object keys are unique: when an OTel attribute collection repeats a key,
the final occurrence is stored. JSON input converts an integer in the signed
64-bit range to an integer, another integer that fits a Variant decimal to a
decimal, and any other number to a double; an integer no Variant numeric type
holds is refused with `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`. A value
nested beyond 64 containers is `WYRD_VALA_400_VARIANT_TOO_DEEP`, one whose
encoded metadata plus value exceeds 8,388,608 bytes is
`WYRD_VALA_413_VARIANT_TOO_LARGE`, and undecodable bytes are
`WYRD_VALA_400_VARIANT_INVALID_JSON`. Scribe repeats the extension, size,
encoding, and depth checks over every declared Variant of a built-in table at
its admission boundary, before the batch is acknowledged, because a raw Arrow
writer can skip client preparation and the fingerprint compares storage types
only. Extension identity is checked across all fields first, then each value
in row and field order for size, encoding, and depth; the first failure names
the field, row, and path.

Built-in tables store open caller content as Variant and closed shapes as
typed columns:

- `vala.traces.spans`, `vala.logs.records`, and `vala.metrics.points` store
  their attribute collections (record, resource, scope, span event and link,
  and exemplar filtered attributes), the log `body`, and the metric `metadata`
  as Variant. `resource_entity_refs` is a list of Structs with `type`,
  `id_keys`, `description_keys`, and `schema_url`. Well-known
  semantic-convention values are copied into nullable promoted columns, each
  null when its source is absent or has another type: `service_name`,
  `service_version`, and `deployment_environment` on all three signals;
  `gen_ai_*`, `http_request_method`, `http_route`,
  `http_response_status_code`, `url_full`, and the `exception_*` columns of the
  span's last `exception` event on spans; the record's own `exception_*`
  attributes and `body_text` (the body when it is a string) on logs.
- `vala.verification.results` replaces its JSON `details` text with two
  nullable Structs: `drift_report` (`method`, Variant `features`, `verdict`) and
  `eval_summary` (task counts, `pass_rate`, `duration_ms`). A scored result sets
  exactly the one its implementation owns.
- `vala.dev.agent_traces` `messages` and `tool_io`, `vala.eval.observations`
  `context` and `media`, `vala.eval.result_items` `actual` and `expected`,
  `vala.gateway.calls` `request_payload` and `response_payload`, and
  `vala.system.audit_log` `detail` are Variant.

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
the exact rows are authoritative in Scribe. It does not wait for local Parquet,
object publication, Iceberg promotion, or compaction.

Every query pins one published cut: an Iceberg snapshot and committed Scribe
hot objects not represented by that snapshot. Oracle also discovers active
streams by listing, concurrently and under the query deadline, the Scribes in
the attempt's frozen roster, and sends live work only to owners of relevant
table partitions at the same frozen endpoint and fence. A listing that reveals
a newer writer epoch restarts the whole attempt once on a refrozen roster;
nodes from different cuts are never mixed. Scribe scans its own memtable or staged authority and streams
bounded Arrow results to Oracle, producing each batch only when Oracle pulls
it. The scan's memtable references, staged-run leases, and follower admission
belong to that stream and are released when it completes, is cancelled, or is
dropped; the query deadline is its only timeout. An unavailable Scribe missing before
discovery is outside the known live set. A known live source lost before rows
degrades the result; loss after rows fails the query. Published-source,
security, tenant, schema, resource, cancellation, and deadline failures fail.

All callers, including Verifiers, use this same query service and source
behavior. The request has no visibility, freshness, or query-class selector.
Oracle alone classifies the one DataFusion plan as Interactive or Analytical
from its physical root. A successful terminal means the published cut completed
and each selected online Scribe supplied the rows required by the completed
DataFusion plan. A plan that stops consuming a live child (for example, at
`LIMIT`) cancels and drops it without draining to a footer; unexpected EOF of
a still-needed stream fails. Success does not prove every acknowledged write
was included. Publication between cut pinning and live scan opening can omit
or duplicate rows. This best-effort tradeoff also applies to verification
judgments. The terminal distinguishes success, known live-source degradation,
and failure; clients accept rows only after a valid terminal.

## Ingest: Scribe

### Shards and hierarchical ownership

Each Scribe pod runs a configured number of shards (`WYRD_MEM_TABLE_BUCKET_NUM`,
default 1, at most 256). The routing key is:

```text
hash(data_tenant_id, canonical_table, wyrd_batch_id)
```

reduced modulo the shard count. Distinct batches for one table use all shards;
retries use the recorded shard. Each shard owns a bounded mailbox, its own WAL
stream, memtable buckets, and cohort state. Shards are a pod-local concurrency
topology, not tenant partitions or table reservations. Replay maps a recorded
shard onto the running count, so the count may change across restarts.

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
| Scribe hot object | `min(WYRD_MAX_FILE_SIZE_ON_DISK, Forge target)` encoded whole-file target, drained by size or dwell, with bounded residue |
| Forge rewrite output | the table's optional registered compaction target, else the deployment default of approximately 1 GiB (`WYRD_BIFROST_FORGE_TARGET_FILE_SIZE_BYTES`) |

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

Caller-supplied `wyrd_event_time` is accepted only within the server window,
defaulting to 30 days before through 24 hours after the batch's
`wyrd_ingested_at`. An out-of-window value fails with
`WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`; Scribe never clamps or normalizes it.
When the column is absent, the server stamps `wyrd_ingested_at` into it.

Each shard projects WAL bytes, Arrow and metadata ownership, age, and resource
pressure before append. Rotation closes the shard generation when any validated
limit requires it. The active memtable remains separated by exact
`SealKey { tenant, table, physical_partition }`. Rotation freezes each nonempty
bucket independently; no file or sort may mix tenants, tables, layouts, or
physical partitions.

Frozen members follow this monotonic lifecycle:

```text
ImmutableArrow
  -> PreparingLocalRun
  -> Ready
  -> Claimed
  -> Publishing
  -> Published
  -> CleanupPending
  -> Retired
```

`ScribeHotStage` bounded-sorts a member into immutable local Parquet runs,
fsyncs the file, manifest, and directory, verifies checksum/schema/layout/footer,
performs a bounded read preflight, and registers it as live-tail authority. The
shared cohort WAL retires only after every member reaches that boundary.
Definite pre-registration failure returns to immutable Arrow; uncertain state
keeps WAL and local evidence for reconciliation.

### Assembly and publication

`StagingAssembler` claims complete ready members under the exact key:

```text
(tenant, table, schema_fingerprint, physical_layout,
 physical_partition, node_id, writer_epoch)
```

Claims may combine compatible members across generations and shards on the same
node. Membership is deterministic and durable before merge; one member cannot
be split across claims and a later member cannot join an existing claim.

`ParquetBatchEncoder` performs a bounded external merge in canonical
`PhysicalLayout` order; rows that tie on every layout key keep claim-member
order, then their position within the staged run, so the merged order is
deterministic without a per-row tie-breaker column. It writes Parquet row groups toward a soft 128 MiB target and
closes immutable hot objects around the 512 MiB whole-file target. A completed row group is
indivisible, and a smaller object is valid for dwell, partition close,
pressure, drain, or final residue. The 512 MiB target is independent of WAL,
active-memory, row-group, and Forge output geometry.

`ScribePersistence` derives deterministic object identities, uploads through a
bounded lane, and atomically commits `file_list` plus audit evidence under the
publication fence. Staged runs remain authoritative until that transaction is
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
operation IDs against `file_list` before opening admission. Unknown versions,
checksum mismatch, contradictory lineage, or ambiguous authority fail closed.

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
installed before plan decode or source IO. There is no caller-selected source
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
analytical leader, analytical planning, and distributed worker — installs one
Variant SQL owner before it plans, decodes, or executes:

- `v -> 'key'` and `v -> n` return the Variant at an object key or array index,
  or null when absent; chains such as `attributes -> 'http' ->> 'route'` follow
  nested keys.
- `v ->> 'key'` and `v ->> n` return the same value as text: a string as
  itself, any other non-null value as its JSON text, and SQL null when absent or
  JSON null. Its result casts to numeric, boolean, and timestamp types with
  ordinary SQL cast semantics.
- `parse_json(text)` returns the Variant for JSON text and fails with
  `WYRD_VALA_400_VARIANT_INVALID_JSON` on invalid input; `try_parse_json(text)`
  returns null instead. `to_json(v)` returns a Variant's JSON text.

`->` and `->>` apply only to Variant operands; a JSON text column is queried as
`parse_json(column) ->> 'key'`. Literal paths lower to one semantic
`variant_get` UDF backed by Arrow-rs `variant_get`, and `->>` adds text
conversion after it; a non-literal path element is evaluated per row from the
full root value. Struct access stays DataFusion's exact `s['field']`
`get_field`. Every Variant result keeps the `arrow.parquet.variant` extension
on the wire. The functions travel in physical plans by name, so the function
set's version is bound into the plan and stage digests peers verify before
decoding. Variant failures keep their stable code and details across
interactive and distributed execution.

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
graph immediately and releases every owner once its structured tasks have
joined; nothing waits for a later request or timer to reclaim it.

Exchange buffers draw from the same finite query-owned memory pool as the
operators; they are not precharged into a predicted child allocation. Before
dispatch, Oracle enforces its configured selected-worker limit, admitted
tasks/partitions, Wyrd-owned admission queue and slots, and scratch allocation
with checked count/range arithmetic. Dependency-owned exchange queues retain
their pinned byte backpressure without a Wyrd item-count guarantee. Oracle does
not claim to predict every dependency allocation or transient encoded-message
byte. Follower spill is charged to the same query-owned scratch allocation.
Memory-pool, transport, or scratch exhaustion is typed and releases all memory,
scratch, slot, task, cache, and transport owners exactly once after the graph
drains. Cleanup timeout or failure is never reported as a successful release:
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
Protocol duplication is forbidden; publication overlap may still duplicate a
row across published and live sources under the accepted best-effort contract.

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
share or a precharge: an idle role holds nothing, and every charge returns
when its owner ends. Aggregate governed reservations therefore cannot sum above
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

Query parallelism comes from CPU, not memory. Before
physical planning, every Oracle leader, Oracle peer, and distributed stage
session sets its target partitions from the node's effective CPU and the pinned input's
locality: `cpu + (4 * cpu - cpu) * (1 - local_ratio)`, never below two, where
`local_ratio` is the pinned bytes held in the local hot tier. Neither the grant
nor the pinned file count changes it. Batch size and join preference are the
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
statement on the bounded runtime PostgreSQL pool resolves every referenced
table, registers one active read per table under the table's maintenance
authority, and returns each catalog metadata pointer with its hot rows. Oracle
then reads the selected metadata documents directly and concurrently; a
document missing after a catalog move reacquires the whole cut once, and a
second `NotFound` is terminal. The active reads live until the leader settles
every descendant fragment, and all of this waits within the leader deadline. `oracle_query_phase_seconds` times the
leader's sequential, non-overlapping steps — `snapshot_pin` (covering every
preparation substep above), `scribe_listing`, `provider_setup`,
`physical_planning`, and `admission` — so they may be read as additive; total
Oracle time is `oracle_query_duration_seconds`, not a phase.
Remote peer work reuses one authenticated channel per ready peer incarnation and
endpoint; a changed fence or endpoint connects anew and never inherits the
prior peer's channel. Connect, fragment open, first remote frame, and terminal
are timed as separate `peer_*` phases; they are per-fragment latencies that can
overlap one another and the leader steps, so they are never summed with them.

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

Oracle read decisions use the one audit outbox. After admission, a tracked,
non-blocking task commits the read-decision event to the canonical tenant
hash-chain staging table, and the `AuditPublisher` retains it like every other
event. Rows are not held for that commit; a failed commit is logged and counted
through `oracle_audit_commit_failures_total`, and shutdown waits for pending
commits. One logical query produces one read-audit event; distributed stages
produce none. An Interactive event records `Local` execution on one node; an
Analytical event records `Distributed` execution over every Oracle in the
frozen participant cut, leader included, with the followers as its workers.

Query streams are length-delimited, terminal-safe frames. A full queue before
framing is `QueryQueueFull`; any other admission refusal before framing is
`QueryAdmissionRejected`; governed memory, scratch, or
exchange exhaustion after framing is `QueryResourcesExhausted`. Cancellation,
deadline, peer loss, and execution failure have typed terminal outcomes. A
stream never represents partial rows as success.

A public distributed-plan or execution-path `EXPLAIN` surface is deferred and
is not part of this delivery. Selected-path evidence reaches callers only
through the success terminal of the one public query operation. The public
query deadline range is `1..=u32::MAX` milliseconds across Rust, HTTP, gRPC,
Python, TypeScript, and MCP.

The tenant hash-chain `vala.audit_staging` is transient transactional
write-ahead state, not retained audit history, and has no external consumer.
Contiguous tenant-scoped ranges are read by a per-tenant
watermark and projected idempotently through the owning local Scribe and the
Forge publication path into the tenant-qualified `vala.system.audit_log` Bifrost
table. That table is the authoritative retained audit history. The publisher
runs only in a process that owns a local Scribe and calls it directly; Gate
holds no retained-audit path.

Publication progress is exactly two values per tenant: the monotonic published
watermark, and at most one nullable in-flight upper bound. A publisher freezes
that bound from the bounded staging prefix under tenant serialization and
releases the transaction before any Scribe IO, so the audit-chain append lock is
never held across publication. Every competing replica and every restarted
publisher reuses the frozen bound verbatim, so all of them project the same
rows and derive the same batch identity; rows appended above the bound wait for
the next batch. The watermark alone would not be enough: a staging tail that
grows mid-flight would give two publishers two overlapping ranges and therefore
two distinct identities for the same content, which no dedup fence can absorb.

A staged row is garbage-collected once the watermark has advanced past it. One
tenant transaction advances the watermark, clears the matching in-flight bound,
and deletes through the watermark together; a stale completion neither moves the
watermark backwards nor clears a newer bound. A crash between publication and
that settlement replays the identical frozen range, which Scribe's durable
batch-id dedup fence absorbs, so recovery retries without duplicating the
retained event. No legacy direct-Iceberg relay or separate `platform.audit_log`
may become a second historical authority.

Audit events are appended only where an authorization decision was made. Scribe
batch commits and Forge maintenance transitions evaluate no permission and are
recorded as lineage in `vala.scribe_batch_commits` and `vala.forge_operations`;
they emit no audit event. The publication path therefore evaluates no new
permission and appends nothing, so retained audit history cannot feed itself.

`vala.system.audit_log` partitions daily, deviating from the hourly granularity
of the high-rate telemetry tables. Forge cannot bin-pack across partition
boundaries, so hourly partitions would permanently cap every audit object at one
hour of a tenant's audit traffic regardless of compaction settings. Audit also
differs from the telemetry tables on every axis partition granularity responds
to: one row per authorized request rather than continuous high-rate ingest,
date-range rather than recent-window queries, and retention measured in years
rather than days.

The five verification tables — `vala.drift.observations`,
`vala.eval.observations`, `vala.verification.results`,
`vala.drift.result_features`, and `vala.eval.result_items` — also partition by
UTC day on `wyrd_event_time`, never by Verifier, subject, binding, or tenant
ID. Every row of one Verification Result carries the same server-chosen event
time, so a result and its detail rows never split across day partitions. A
Drift schedule window does not determine partition size.

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

At most one durable task attempt owns the lease and fence for a
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
branch. It then performs one fenced duplicate-checking fast-append
`commit_once`. Promotion writes no data object and invokes no compaction core.

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
and audit transaction records the promotion snapshot on every claimed row and
switches Oracle authority. During the catalog-to-SQL interval, Oracle
suppresses a hot object only when its pinned snapshot contains the exact path,
checksum, and operation evidence.

### Managed compaction and publication

Iceberg maintenance rewrites eligible live files toward the table's
`write.target-file-size-bytes`: the table's optional registered compaction
target when one is set, otherwise the deployment default resolved once per
attempt (approximately 1 GiB, overridable by environment). Row-group size is
independent and normally 128 MiB. A physical file rolls from the managed
writer's encoded estimate after a completed write; partition and end-of-stream
residue are valid. Neither target is a universal physical object-size
guarantee.

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
`DataFile`s; the five-field handoff is unchanged. A missing, null, or
mistyped lineage column in any input batch fails the rewrite before commit and
publishes nothing.

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

Attempt output paths are table-bound and retain the managed core's recipe
segment:

```text
{table}/data/forge/{recipe}/{attempt_uuidv7}-{writer_ordinal:05}-{writer_uuidv7}.parquet
```

The recipe segment is the managed core's canonical writer-recipe identity and
keeps completed Forge outputs recognizable as current on the next selection
pass. Each physical writer owns its filename counter and UUIDv7 suffix;
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
snapshot, current schema identity, selected-input existence, lease, and fence.
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
audit settlement occurs exactly once.

### Convergence, retention, and cleanup

Each rewrite snapshot records canonical versioned fingerprints for its base
live data set, resulting live data set, produced outputs, semantic debt shape,
and lineage snapshot. Before object IO, Forge reconstructs those exact sets and
refuses a rewrite when selected files are prior outputs, semantic debt is
unchanged, and no live-set delta affects the selected groups or delete scope.
Path churn alone never authorizes another rewrite. Missing, expired, partial,
or contradictory lineage evidence fails closed.

Data-file compaction, manifest rewriting, snapshot expiration, expired-object
cleanup, and never-published orphan cleanup are separate protocols. The leader
timer orders them per pass as manifest rewrite, snapshot expiry, then cleanup.
Expiry has no age or retain-last window: a replaced snapshot is eligible as
soon as no active Oracle read, in-flight compaction, unsettled promotion, or
other authoritative root retains it. A table with an active read is skipped;
snapshot expiration preserves active refs, unresolved attempts, reconciliation
evidence, and the lineage snapshot referenced by the branch head. Orphan GC deletes only
objects proven unreferenced and outside every active or uncertain attempt.
Committed Scribe hot objects in `file_list` that lack exact promotion evidence,
and objects of a table with an active Oracle read, are hard GC roots even
when no Iceberg snapshot references them. Once the last reader releases,
expired-object cleanup deletes without an age floor and removes the matching
terminal `file_list` row in the same completion transaction.
Every destructive effect holds the table's exclusive maintenance authority
through its known outcome, and the Forge lease TTL bounds that hold: an
object-store call still running at the bound is an uncertain effect left for
idempotent replay, so a hung store never blocks the table's readers
indefinitely. An Oracle cut that waits on the authority waits at most until
its query deadline and then fails with the query timeout.
An open Scribe fragment retains its local Arrow batches and staged resources
until its stream completes or drops. It names no Forge-collectable object and
contributes no independent Forge GC root.
No cleanup infers safety from age or path shape alone.

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
- Cancellation is structured: stop admission, cancel descendants, join work,
  reconcile uncertain effects, and release ownership exactly once.
- Durable operations separate definite failure from uncertain completion.
  Uncertainty retains identity and evidence until reconciliation.
- Catalog compare-and-swap and lease fences own publication authority. An
  object-store PUT alone never makes data visible or safe to delete.
- Audit records authorization decisions. Every boundary that evaluates a
  principal's permission records exactly one allowed or denied event.
  Permissions are blocking; audits are non-blocking: the check completes before
  the operation proceeds or refuses, and the event is staged on the shared
  audit outbox without the operation waiting for its commit. Oracle read
  decisions do not hold rows for that commit. Surfaces not yet converted still
  append in the operation's own commit transaction and fail closed until they
  move to the outbox.
- Engine-internal transitions — Scribe batch commits, Forge maintenance, audit
  publication, reconciliation, storage lifecycle — evaluate no permission. They
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
  zero, and replay after restart counts again in the new process. Receipt
  `accepted_rows` is a client contract, not a newly-stored-row measure.
  `bifrost_scribe_staging_live_members`, `_live_bytes`,
  `_oldest_member_timestamp_seconds`, and `_outstanding_claims` are published
  from the staging assembler's own state, so members restored after restart
  appear in backlog. `bifrost_scribe_lane_queued{lane}` is waiting jobs only;
  `bifrost_scribe_lane_active{lane}` is running jobs.
  `bifrost_scribe_publication_files_total` and `_bytes_total` count committed
  publication output after its catalog transaction commits.
- **Oracle.** `oracle_query_duration_seconds{class,outcome}` starts after Gate
  dispatch and measures Oracle execution through its terminal; the production
  HPA and query reports read it. Outcomes are `success`, `degraded`, `failed`,
  `cancelled`, and `client_drop`; a Degraded terminal is never counted as
  success. `oracle_queries_queued` is waiting work and `oracle_queries_active`
  is admitted work only. `oracle_admission_total{class,outcome,reason}`
  pre-registers only the decisions an admission branch can make, and
  `oracle_admission_queue_duration_seconds` records each waiter once. Scan
  counters report the executed plan's files, bytes, partitions, and row
  groups. `bifrost_oracle_file_pruning_total{outcome}` counts hot-object
  exclusions decided from declared bounds; Iceberg pruning happens inside its
  own scan planning and is not re-walked for telemetry.
- **Storage.** `bifrost_storage_metadata_cache_effects_total{effect,reason}`
  counts cache decisions (hit, miss, join, bypass, evict) — logical lookups,
  not backend I/O. `bifrost_storage_metadata_cache_loads_total{outcome}`,
  `bifrost_storage_metadata_load_seconds`, and
  `bifrost_storage_metadata_wait_seconds` measure footer decodes and the time
  callers waited on them. `bifrost_storage_requests_total{operation}`,
  `bifrost_storage_request_terminals_total{operation,outcome}`,
  `bifrost_storage_request_seconds`, and
  `bifrost_storage_request_retries_total` measure governed backend requests; a
  cache hit adds none. Resident entries/bytes and in-flight loads are set from
  the cache's own state, and `bifrost_storage_active_requests` moves at request
  admission and settlement. No telemetry copy of cache or request state exists.
- **Traces.** A query is one trace: `bifrost.gate.query.stream` spans the
  client-facing stream through terminal or drop, with `bifrost.gate.query`
  dispatch as its child; `bifrost.oracle.stream` spans Oracle execution through
  terminal cleanup, and `bifrost.oracle.peer.fragment{role,outcome}` spans each
  remote fragment. Work `DataFusion` spawns inherits the query span, so remote
  fragments are child work of the query that dispatched them. Each operation
  span records exactly one `outcome`.

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

- table management under `/v1/bifrost/tables`;
- `AppendReceipt { batch_id, accepted_rows, durability }`, where `durability`
  is the closed `AppendDurability::Acknowledged` value; the synchronous append
  response never reports staged, published, promoted, or rewritten state;
- terminal-safe query streaming at `POST /v1/query`;
- canonical SQL observation reads through the ordinary Bifrost query contract;
- gRPC ingestion through `wyrd.v1.BifrostIngestService`;
- gRPC query projection through `wyrd.v1.BifrostQueryService`;
- the `wyrd.bifrost` Python projection and matching TypeScript SDK;
- agent-facing read and write operations governed by explicit permissions.

Observation namespaces such as `vala.traces`, `vala.metrics`, `vala.logs`,
`vala.eval`, `vala.drift`, `vala.verification`, `vala.dev`, `vala.gateway`, and
`vala.system` remain tenant-qualified Bifrost tables. Canonical SQL is their
only read contract;
the namespace does not create another storage or authorization model.

Permissions are scoped through `BifrostTable`, `BifrostRecord`, and
`BifrostQuery`. Generic writes cannot target reserved or system-managed tables;
`vala.gateway.calls` accepts no public write at all: Gate refuses every
principal, and only server-internal gateway capture writes it, together with
the capture spans in `vala.traces.spans`. A pod without Scribe submits that
capture through `ScribeCapturePeerService.IngestCapture` on the peer listener,
which the same `wyrd-peer` mTLS admits; the request names its tenant and one of
those two tables explicitly, carries no token, and is refused for the reserved
system tenant or any other table. Sensitive-column metadata remains descriptive; table query
permission governs every column, including GenAI fields stored on trace spans,
except that a projection reaching the `vala.gateway.calls` request or response
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

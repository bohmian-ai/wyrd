# Analytical operations and reliability

Load for durability, backpressure, leases, recovery, retention, SLOs, or
production operation of Bifrost.

## Name every durable and visible boundary

Use these distinct states in APIs, metrics, recovery, and operator evidence:

1. **Admitted:** global, tenant, table, and lane ownership is reserved.
2. **Acknowledged:** Scribe WAL is fsynced, the batch fence is durable, and
   active rows are authoritative.
3. **Staged:** every immutable cohort member has an fsynced, checksummed,
   structurally validated, query-registered local Parquet replacement; its WAL
   may retire.
4. **Hot published:** immutable object upload and fenced `file_list`/audit
   publication are terminal; new Oracle cuts read the hot object.
5. **Iceberg promoted:** the exact Scribe `DataFile` evidence is appended
   unchanged in a promotion snapshot.
6. **Rewritten:** the managed compaction core produced replacement files and
   Forge published one fenced rewrite snapshot.
7. **Maintenance settled:** retention and cleanup protocols have independently
   settled their durable work.

Do not collapse these into one "flushed" or "committed" state. A successful
response names the boundary it reached. Readers consume only authority named by
their pinned cut and never discover unqualified objects by listing storage.

## Bounded ownership and backpressure

Every queue and resource class is bounded by its natural unit: items, bytes,
age, slots, open files, fan-out, or deadline. Scribe accounts global, tenant,
and table admission; active and immutable memory; durable stage; merge scratch;
claim ownership; and upload lanes separately. Capacity is work-conserving, but
contention enforces tenant-then-table fairness and prevents over-share owners
from reacquiring while an admitted competitor waits.

Scribe preserves acknowledged authority under downstream pressure. It rejects
new work with a typed retryable response before WAL, stage, scratch, or object
storage exhausts. Shutdown closes admission, rotates active work, stages
immutable members, closes residue claims, and drains admitted publication up
to one absolute deadline; unsettled work retains exact replay evidence.

Oracle admits interactive and analytical slots separately under one atomic
total bound. Each query owns its runtime, one aggregate memory pool shared by
operators and exchanges, spill allocation, bounded Wyrd-owned admission queues
and graph controls, cancellation tree, and deadline. Dependency-owned exchange
queues retain their pinned byte backpressure without a Wyrd item-count
guarantee. Admission refusal never mutates the root grant. Cancellation releases
every descendant reservation and temporary file.

Forge leases resources per tenant/table/task/attempt. Physical writers may run
concurrently within an attempt, but one fence owns final catalog publication.
Resource pressure defers or refuses a task; it never silently reduces file
size, row-group geometry, selection scope, or correctness.

## Cross-system commit and recovery

Postgres and object storage do not share a transaction. Every external effect
uses deterministic identity plus explicit prepared, terminal, and uncertain
evidence. Catalog publication uses compare-and-swap/`commit_once`; timeout or
transport loss reconciles by operation identity before a new attempt begins.
Never report success merely because output objects exist.

Startup restores in dependency order: WAL, staged manifests and checksums,
live-tail registry, deterministic claims, pending uploads/publications,
`file_list`, catalog operations, then admission. Unknown versions,
contradictory lineage, checksum mismatch, cross-tenant evidence, or ambiguous
acceptance fails closed while preserving the last valid authority.

Lease and fence values are durable publication authority, not liveness hints.
A stale owner cannot publish after lease loss. Retries preserve task identity
for definite failure and operation identity for uncertain acceptance; they do
not generate fresh output until prior uncertainty settles.

## Maintenance protocols remain separate

- **Promotion** appends exact Scribe hot objects without rewriting bytes.
- **Data-file compaction** rewrites selected live data and applied deletes.
- **Manifest rewrite** reorganizes metadata without changing the logical data
  set.
- **Snapshot expiration** changes retained snapshot reachability under active
  task and reader watermarks.
- **Expired-file cleanup** deletes objects proven unreachable from retained
  snapshots and advances durable per-object evidence.
- **Never-published orphan cleanup** handles aged attempt-generation objects
  protected by no catalog snapshot or open operation.

No cleanup protocol may infer safety from a storage listing, local cache, or
failed attempt alone. Deletion requires freshly acquired catalog ancestry, retained
snapshot policy, open-operation protection, tenant equality, and an age safety
window. Missing objects are idempotent only after those proofs succeed.

## Reliability signals and qualification

Measure at minimum:

- admission refusal by resource category; active owners; queue depth and age;
- WAL append/fsync/fence latency, bytes, replay, and retirement lag;
- active, immutable, staged, scratch, claim, upload, and spill ownership;
- staged dwell, merge fan-in/passes, row groups, hot-object PUT amplification,
  `file_list` publication, reconciliation, and live-tail source counts;
- Oracle route, slot wait, planning, pruning, exchange bytes, worker fan-out,
  terminal peer failure, cancellation, TTFF, terminal latency, and incomplete
  streams;
- Forge promotion debt, rewrite debt, task/lease age, writer estimates and close
  reasons, commit conflicts, uncertain operations, no-progress refusals,
  snapshot age, cleanup cursor, and orphan backlog;
- catalog refresh age and object-store latency/error/throughput.

Metrics use stable bounded labels. Tenant, table, batch, task, attempt, path,
and request identities belong in scrubbed traces or durable evidence, not
metric labels.

Production qualification exercises crash recovery at every authority switch,
duplicate batches, schema conflicts, partial uploads, lease loss, ambiguous
catalog commits, live-tail publication races, peer loss, exchange/spill
exhaustion, cancellation, delete-file application, retained-reader safety,
backup restoration, and tenant tripwire failures. Performance claims require a
fixed workload, environment, concurrency, dataset, query mix, and promoted
measurements; configuration values are never performance evidence.

## Rejected shapes

Reject unbounded queues, sleep-based coordination, best-effort cleanup, global
locks, listing-based deletion, retries without deadline or identity, success
that hides partial progress, process-global mutable analytical resources, and
repair paths that bypass tenant or audit checks.

## Stable Wyrd anchors

- Bifrost lifecycle: `architecture/bifrost-design.md`.
- SQL and audit boundaries: `architecture/agent-rules.md`.
- Engine owners: `crates/vala/vala-bifrost-redux/`.
- Catalog/control state: `crates/vala/vala-sql/`.
- Serving lifecycle: `crates/wyrd/wyrd-server/`.

## Primary grounding

- [Apache Iceberg table specification](https://iceberg.apache.org/spec/)
- [Apache Iceberg maintenance](https://iceberg.apache.org/docs/latest/maintenance/)
- [Apache DataFusion configuration](https://datafusion.apache.org/user-guide/configs.html)
- [OpenTelemetry metrics](https://opentelemetry.io/docs/concepts/signals/metrics/)

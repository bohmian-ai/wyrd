# Analytical operations and reliability

Load for buffering, WAL durability, backpressure, compaction, catalog
coordination, recovery, SLOs, or production operation of Vala analytical paths.

## Lifecycle

An ingest lifecycle has four observable boundaries: admission, durable append,
sealed analytical file, and visible table snapshot. Name these states in APIs
and metrics. A successful response must say which boundary was reached. Readers
must tolerate a lagging snapshot without reading unqualified files.

Bound queues by bytes, rows, and age. Backpressure should reach the caller as a
typed retryable result or a clear capacity response. Flush and shutdown drain
accepted work, cancel new work, and report what remained. A batch ID or
idempotency key makes retries safe across process restarts.

## Coordination

Use one focused writer or lease per logical table for snapshot commits. Leases
carry an owner and fencing value; stale owners cannot publish after losing the
lease. Keep Postgres transactions responsible for control rows and audit
records, and make the object-store/Iceberg boundary recoverable with explicit
intent and completion markers. Do not claim atomicity across systems that do
not share a transaction.

Compaction is bounded maintenance: choose candidates by size, age, and
partition; publish a new snapshot; retain enough history for readers and
replay; then remove only files proven unreachable after a safety interval.
Catalog refresh and cache invalidation are first-class operations with metrics.

## Reliability signals

Measure admission rejects, queue depth, oldest item age, WAL fsync latency,
seal duration, snapshot commit conflicts, catalog refresh age, compaction debt,
object-store errors, query bytes/files/partitions, spill usage, and dropped or
dead-lettered records. Correlate these with tenant, logical table, Card, Run,
and request identifiers without putting unbounded values in metric labels.

Exercise crash recovery, duplicate batches, schema conflicts, partial object
uploads, lost leases, stale caches, cancellation during a stream, and a tenant
tripwire violation. A replay or repair path must be observable, bounded, and
safe to run more than once.

## Trade-offs and anti-patterns

More aggressive flushing lowers data loss exposure but increases file and
catalog overhead. Larger batches improve throughput but increase tail latency
and retry cost. Longer snapshot retention aids forensic replay but consumes
storage. Choose from measured SLOs, not a universal interval.

Reject one global lock, unbounded queues, sleep-based coordination, silent
best-effort cleanup, deletion based on a stale listing, retry loops without
deadlines, and a success response that hides partial progress. Keep operational
repair separate from normal request handling and require the same tenant and
audit checks.

## Stable Wyrd anchors

- Bifrost lifecycle and system columns: `architecture/wyrd-design.md` §Bifrost.
- Ingest and storage engines: `crates/vala/vala-bifrost-redux/` and
  `crates/vala/vala-ingest/`.
- Catalog/control records: `crates/vala/vala-sql/`.
- Server lifecycle and audit: `crates/wyrd/wyrd-server/`.

## Primary grounding

- [Apache Iceberg table specification](https://iceberg.apache.org/spec/)
- [Apache Iceberg maintenance concepts](https://iceberg.apache.org/docs/latest/maintenance/)
- [Apache DataFusion configuration](https://datafusion.apache.org/user-guide/configs.html)
- [OpenTelemetry metrics](https://opentelemetry.io/docs/concepts/signals/#metrics)
- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- Wyrd anchors: `architecture/wyrd-design.md` §Bifrost;
  `crates/vala/vala-bifrost-redux/`; `crates/vala/vala-sql/`.

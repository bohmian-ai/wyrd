# Arrow and analytical interop

Load for Arrow schemas, `RecordBatch` streams, Parquet conversion, Arrow IPC,
PyArrow, FFI, or analytical memory ownership.

## One logical schema, explicit physical projections

Build each Bifrost table from one Wyrd-owned logical schema and derive Arrow,
Parquet, Iceberg, Rust, Python, and TypeScript projections. Preserve field
names, Iceberg field IDs, logical types, nullability, metadata, decimal
precision/scale, and timestamp units/timezones. Map by stable identity or name,
never positional index when schemas can diverge.

Reserved system columns and immutable `(wyrd_batch_id, wyrd_row_ordinal)` stay
present and non-null where the table contract requires them. The server stamps
authenticated tenant, ingest time, and request identity; callers cannot
override them. Schema fingerprints cover the canonical logical schema and are
validated before append, staging recovery, promotion, and query.

Use bounded `RecordBatch` values as ownership and backpressure units. Streaming
query results must include an explicit successful or failed terminal frame so
EOF, cancellation, transport loss, and successful completion cannot be
confused.

## Ownership and copy behavior

Prefer borrowed arrays, sliced buffers, and ownership transfer when their
lifetimes are explicit. Zero-copy is an optimization, never an API promise.
Document every boundary as one of:

- shared/borrowed buffer with a retained owner;
- ownership transfer through the Arrow C Data Interface;
- Arrow IPC encode/decode, which copies or reallocates as required;
- scalar or row conversion, which is intentionally materialized.

The Python analytical surface uses Arrow IPC as its stable native boundary
unless it explicitly exports the Arrow C Data Interface. IPC serialization and
PyArrow decoding are a copy-bearing contract; do not describe them as
zero-copy. Keep copies bounded by batch and expose batch-size, timeout,
backpressure, close, and cancellation controls.

Release the GIL while Rust performs blocking IO or CPU-bound analytical work.
Convert Python inputs into Rust-native schemas, plans, and Arrow buffers before
execution. Never retain `Bound<'py, T>` across await, thread, or long-lived
engine state.

## C Data Interface trust boundary

An imported Arrow C schema/array/stream is untrusted foreign memory. Validate:

- format string, child count, field names, nullability, metadata, and nesting;
- length, offset, null count, buffer count, and offset-buffer monotonicity;
- validity bitmap and value-buffer bounds with checked arithmetic;
- dictionary index width and referenced dictionary lifetime;
- decimal precision/scale and timestamp unit/timezone;
- release callbacks, one-time release, child ownership, and producer lifetime;
- maximum rows, bytes, nesting depth, and batch count before materialization.

Do not dereference or construct a safe Arrow array until the complete physical
layout is proven within the admitted envelope. A release callback is not proof
that the underlying buffers are valid or immutable.

## Type and encoding traps

Be explicit about UTF-8 validity, signed integer conversions, decimal overflow,
NaN ordering, timezone normalization, dictionary semantics, nested lists/maps,
and null versus empty. Arrow logical and Parquet physical types are related but
not interchangeable. Round-trip tests cover schema, values, statistics, field
IDs, and nullability through Arrow -> Parquet -> Arrow and through each public
language boundary.

Scribe's 32 MiB logical/131,072-row row-group input is a pre-write memory and
geometry bound, not a compressed-size limit. Forge's encoded row-group target
and approximate file target are independent. Never use buffer capacity,
logical size, writer estimate, or Parquet object length as substitutes for one
another.

## Failure and cancellation

Fail closed on missing managed columns, incompatible nullability, invalid field
identity, out-of-range cast, malformed foreign buffers, oversized batches,
schema fingerprint conflict, tenant mismatch, or a stream without an
unambiguous terminal result. Cancellation stops producers, joins conversion
and writer tasks, releases reservations, and invokes foreign release callbacks
exactly once.

Reject bespoke row dictionaries for analytical transport, copying an entire
result through Python, mutable global schemas, positional alignment,
unbounded IPC messages, and tenant inference from object paths.

## Stable Wyrd anchors

- Managed analytical schema: `crates/wyrd-spec/src/vala/`.
- Arrow, Parquet, and providers: `crates/vala/vala-bifrost-redux/src/`.
- Rust-native analytical client mechanics: `crates/shared/wyrd-client/src/bifrost/`.
- Python aggregation: `sdks/wyrd-sdk-python/`.

## Primary grounding

- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- [Arrow C Data Interface](https://arrow.apache.org/docs/format/CDataInterface.html)
- [Arrow C Stream Interface](https://arrow.apache.org/docs/format/CStreamInterface.html)
- [Arrow IPC format](https://arrow.apache.org/docs/format/Columnar.html#serialization-and-interprocess-communication-ipc)
- [Apache Parquet documentation](https://parquet.apache.org/docs/)
- [PyArrow documentation](https://arrow.apache.org/docs/python/)

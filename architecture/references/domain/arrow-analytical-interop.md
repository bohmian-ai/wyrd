# Arrow and analytical interop

Load for Arrow schemas, RecordBatch streams, Parquet conversion, PyArrow or
Python boundaries, and zero-copy analytical data movement.

## One schema, explicit conversions

Arrow is the in-process, language-neutral columnar interchange. Build each
Bifrost table schema from one Wyrd-owned definition and derive Arrow, Parquet,
Iceberg, Rust, and Python projections from it. Preserve field names, logical
types, nullability, metadata, and field IDs; never align columns by position
when schemas can diverge.

Use `RecordBatch` for bounded units and Arrow streams for query results. Keep
reserved system columns present and non-null where the contract requires them.
Stamp event time, ingest time, batch ID, row ordinal, and authenticated tenant
before serialization so every format carries the same identity and ordering.

## Memory and boundary behavior

Prefer borrowed arrays, buffers, and zero-copy views when lifetimes permit;
otherwise make the copy explicit at the boundary. Release the Python GIL while
Rust performs blocking IO, object-store reads, or long analytical work. Convert
Python values into Rust-native schema and query types before execution and
return Arrow-compatible batches rather than bespoke row dictionaries.

Expose explicit flush, close, timeout, batch-size, and backpressure controls on
buffered writers. Cheap-looking properties must not trigger network, catalog,
or query execution. For FFI, validate buffer ownership, offsets, validity
bitmaps, and timezone units before accepting an external array.

## Type traps

Be deliberate about UTF-8 validity, signed integer widths, decimal precision,
timestamp units/timezones, dictionary encoding, nested lists/maps, and null vs
empty values. Parquet physical types and Arrow logical types are related but not
identical; round-trip tests must cover both. Schema fingerprints should include
the canonical logical schema and be checked before append.

## Failure modes and anti-patterns

Fail closed on missing required columns, incompatible nullability, out-of-range
casts, malformed buffers, oversized batches, or an Arrow stream that ends
without a clear terminal result. Do not stringify structured values, copy every
batch through Python, expose mutable global schemas, or let a consumer infer
tenant identity from an object-store path.

## Stable Wyrd anchors

- Analytical schema and reserved columns: `crates/wyrd-spec/src/vala/`.
- Arrow/Parquet conversion and providers: `crates/vala/vala-bifrost-redux/src/`.
- Python analytical surface: `crates/vala/vala-sdk/` and
  `python/py-wyrd/` registration.
- Schema fingerprint and query contracts: `crates/wyrd-spec/src/vala/api/`.

## Primary grounding

- [Apache Arrow columnar format](https://arrow.apache.org/docs/format/Columnar.html)
- [Apache Arrow format overview](https://arrow.apache.org/docs/format/)
- [Arrow C Data Interface](https://arrow.apache.org/docs/format/CDataInterface.html)
- [Apache Parquet documentation](https://parquet.apache.org/docs/)
- [PyArrow documentation](https://arrow.apache.org/docs/python/)
- Wyrd anchors: `crates/wyrd-spec/src/vala/`; `crates/vala/vala-bifrost-redux/src/`;
  `crates/vala/vala-sdk/`.

# Independent CPU design review A

Status: independent findings reconciled; reviewer A consents to the common
proposal in [throughput.md](../throughput.md).
No production edits, builds or load runs performed.

## Recommendation

Make the 80 input MiB/CPU-second design CPU-first: remove repeated parsing and
temporary column work, retain compressed IPC chunks through the WAL writer,
reuse compression/metadata context between independent slices, and coalesce
inspection publication. Keep sorting, physical query layouts, compression
settings, durable ACK and resource governance unchanged. Treat a two-group
pipeline as a separately judged wall-throughput change, not the CPU target's
primary mechanism.

The reported/current-record baseline is 58.589 input MiB/CPU-second and
20.388 CPU microseconds per row for the selected logs-c64 population. Holding
its bytes per row constant, 80 requires approximately 14.93 CPU microseconds
per row: about 5.46 microseconds, or 26.76%, less CPU per row. This is the
engineering budget; it is not a prediction that an individual change has
already saved that amount. Do not substitute the record's 88.82 stored
MiB/CPU-second metric for input efficiency.

## Concrete shared IPC ownership change

Current `scribe/preprocess.rs::prepare_slice` (789) calls `encode_ipc` (842)
for every slice. It constructs a new Arrow StreamWriter and ZSTD context,
writes compressed buffers into a contiguous Vec, then calls shrink_to_fit.
The pinned Arrow fork already streams individual encoded buffers directly
into that Vec; there is no additional whole-body intermediate in that path.
The demonstrated avoidable copies are compressed chunks into the final Vec
and a possible allocation/copy when shrinking it.

Use the pinned fork's existing `arrow_ipc::writer::StreamEncoder` (writer.rs
1854), whose output is ordered owned Arrow Buffers. Keep these chunks, their
checked total length, and allocation capacity in the existing prepared-slice
owner through WAL IO. Extend PreparedWalAppend's private payload ownership;
do not join chunks just before writing. Existing WAL SHA/CRC loops already
process ordered parts (`scribe/wal.rs` 782 onward). Extend their iterator to
the IPC chunks and retain the current partial vectored-write handling.

Each slice remains a complete independent standard IPC stream, with its own
schema/dictionaries/EOS. There is no new WAL format and the existing StreamReader
continues to decode it. The WAL data_len is the checked sum of chunk lengths.
The attempt WAL digest authenticates the actual byte sequence, while
`logical_data_identity` (preprocess.rs510) continues hashing the unchanged
canonical Arrow arrays, excluding existing volatile columns. A compressed
representation must never replace that logical identity.

Context reuse needs a small extension to the existing fork encoder: accept and
return IpcWriteContext around a complete independent stream, or reset stream
state while retaining only compressor/metadata scratch. Its current finish
consumes self and its context is private. `IpcDataGenerator::encode_to_buffers`
is private (writer.rs928); public `encode` assembles a contiguous arrow_data
buffer. Therefore a claim that existing public APIs already supply both context
reuse and segmented output is incorrect. Do not introduce a custom IPC codec
or route through public encode merely to add copies back.

Reuse context on an existing ingress worker with no global lock. Reset schema,
dictionary and end-of-stream state between slices, with no cross-tenant data
references retained. Scratch capacity remains bounded and charged to the existing
memory owner after a job; output chunks retain their existing root lease until
WAL completion. A cancellation or encoding/write failure drops exactly its
owned chunks and lease; a later job cannot reuse their live allocations.

The fork has byte-equivalence tests for StreamEncoder versus StreamWriter under
default/uncompressed and dictionary cases (writer.rs2847 onward). Add exact
ZSTD-level-1 byte-equivalence tests including sliced/nested/nullable/dictionary
arrays and reused-context consecutive independent streams. Also exercise standard
StreamReader, existing WAL replay, cross-pod retry equality, partial writes and
checked-length/memory accounting. No changed hash or compressed-byte stability
may be assumed from round-trip equality alone.

## Other CPU components

- Logs already uses a borrowed wire decoder (commit5d1d46598). Do not propose
  implementing it again. Its preflight still walks a resource group separately
  for last schema URL, merged resource and scopes (`otlp_logs_decode.rs159–208`);
  entity refs separately search singular fields before the main scan. Extend
  the existing scanner to collect common-case facts together, retain exact
  admission-before-allocation, and preserve merge/oneof/error semantics through
  the existing fallback. Reuse the pending trace design's same approach rather
  than adding a competing parser.
- Reuse the existing `otlp-ingest-cpu` design's direct column/Variant mechanisms
  for traces; logs already implements several of them. Source-level confirmation
  must distinguish remaining work from landed work. Shared Variant builders must
  preserve exact output, rollback and trust-boundary validation.
- Coalesce inspection publications for append-only mailbox drains. Queueing
  append commands does not change memtable contents, but `run` rebuilds stats,
  bucket-memory and pressure-candidate projections after each command. Preserve
  publications required by state-changing commands and before durable ACK.
- Persistence already concatenates only sort keys and gathers payload columns
  once (`parquet_writer.rs::prepare_sorted_candidate`438–520). Do not recommend
  removing a nonexistent second full-column concatenate. Keep sorted staged
  runs, row-group sizes, indexes and final object assembly: skipping these to
  improve write CPU would shift cost to readers.

## Profile evidence and limits

Read with supported capacity readers only:

```
mise exec -- target/profiling/release/capacity report target/benchmarks/bifrost/writes-parquet/20261120T194551Z-09e81560c.json
mise exec -- target/profiling/release/capacity profile target/benchmarks/bifrost/writes-parquet/20261120T132855Z/profiles/otlp-logs-c64/cpu-server-3807985.json.gz wyrd-scribe-ing
mise exec -- target/profiling/release/capacity profile target/benchmarks/bifrost/writes-parquet/20261120T132855Z/profiles/otlp-logs-c64/cpu-server-3807985.json.gz wyrd-scribe-per
```

The older profile shows 60,746 Tokio, 40,861 ingress and 38,314 persistence
samples. In ingress, ZSTD_compressBlock_fast has 16.4% self, copies12.7%,
FSE_initCState2 6.3%; persistence includes ZSTD_hash4 18.1% self,
FSE_initCState2 5.6% and string interleave construction5%. These are within-thread
shares, not independent whole-process savings. The profile also has heap sampling
enabled and predates the current transport changes. Use it to select mechanisms,
not to add percentages into a promised gain. Compressed initialization reuse
will not remove ZSTD's actual compression work or entropy-table construction.

The current record is VALID as a diagnostic selected-step dirty run, not a full
qualification. It reports no published Parquet during its logs windows; it
therefore cannot prove that improving this number preserves sustained publication
headroom or mixed read latency. The implementation acceptance must include the
existing full writes task and mixed read/write lanes, unchanged resource limits,
unchanged input populations and backlog settlement. Require input efficiency80
and no read p95/p99, visibility, error or backlog regression outside established
run spread; a write-only score cannot establish those conditions.

Skipped: fresh measurement and implementation. Risk: these mechanisms target real
CPU work, but their combined savings and sufficient margin over80 remain to be
established by implementation verification.

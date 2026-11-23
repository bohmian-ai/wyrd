# Independent CPU and read-latency review C

Status: reconciled with reviewers A and B on the CPU-first flow below. Read-only source
and existing profile inspection; no implementation or new experiments.

## Recommendation

Make reducing bytes copied, repeated interpretation and per-slice allocation the
CPU-efficiency design. Keep pipeline scheduling separate: it can improve wall
throughput, but its added ownership does not itself establish lower CPU per byte.
Target 80 MiB/CPU-s as a later implementation acceptance condition. Against the
maintainer's reported 58.59 baseline, that requires about 26.8% less CPU per input
byte (or about 36.5% more bytes per CPU-second). Neither figure is measured savings.

### Concrete shared-path change: segmented IPC payload ownership

`scribe/preprocess.rs:842` creates a fresh `StreamWriter`, writes a whole IPC
stream into one `Vec`, calls `shrink_to_fit`, and moves that vector to `Bytes`.
`scribe/wal.rs:474` then requires one contiguous `Bytes` as prepared data.

The selected Arrow 60 fork already supplies `StreamEncoder::encode` and
`finish`, returning owned `Vec<Buffer>` chunks that form an ordinary IPC stream.
Use this existing API to retain its output chunks through the existing prepared
WAL owner and write their borrowed slices through the vectored WAL writer. This
avoids assembling a second contiguous stream and removes the explicit shrink.
It preserves ZSTD level 1, independent per-slice streams, framing and recovery.
The selected dependency is pinned in `Cargo.toml:310` and `Cargo.lock` to
`d0c75b582fd4d4478b7bac2988b585ada4a3efd8`. Its local source is
`~/.cargo/git/checkouts/arrow-rs-7086235adca86d7c/d0c75b5/arrow-ipc/src/writer.rs:1854-1945`.

Important limits and obligations:

- Arrow 60 `StreamWriter` already uses its direct `write_to_sink` implementation;
  do not claim it still performs the older full intermediate record-body copy.
  The removed copy is from codec-owned buffer output into the final contiguous
  WAL stream, plus potential shrinking. Compressed payload chunks still exist.
- `IpcWriteContext` already reuses compression state within one stream. Public
  `StreamEncoder` does not expose reset for another independent stream and
  `finish` consumes it. Do not promise cross-stream context reuse without an
  additional justified dependency/API change. The reviewers agree a minimal
  context take/return or reset extension in this existing owned fork is justified
  by cross-slice compression allocation reuse. This is not a new IPC codec.
- Digest, CRC, payload length and framing must operate over the ordered chunk
  concatenation. Keep hashes byte-identical and slice-local; whole OTLP request
  digests cannot replace partitioned slice identities.
- Current vectored append uses a fixed small array of slices. Chunked IPC can
  exceed that count. Use bounded windows of borrowed `IoSlice`s with the existing
  partial-write semantics; never reject valid wide schemas at an incidental
  buffer-count ceiling or concatenate again to satisfy that ceiling.
- Charge retained owners, not just visible byte slices: a small Arrow slice can
  retain a larger backing allocation. Release chunk owners after WAL work settles;
  do not hide buffers in an ungoverned scratch cache.
- Recovery reads the same standard IPC stream. Queries keep the existing
  memtable/layout and Parquet representation; this proposal changes transport
  ownership, not the read layout or visibility boundary.

### Other useful work and exclusions

Coalesce append-only mailbox snapshot publication after a command drain. The
existing append handler only enqueues, while every publication gathers stats,
bucket memory and pressure candidates. This is valid repeated-work removal but
is not supported as a large enough CPU saving on its own.

Do not add another Variant validation bypass: `execution_lanes.rs:1108-1125`
already treats server-built canonical rows as trusted and skips the value walk.
Do not propose borrowing resource envelopes as new work: `tables/signal.rs:1038`
already retains one envelope and stores row references until final Arrow columns.
Do not replace canonical columns with compressed/dictionary query layouts merely
to improve the ingest measurement.

## Existing profile evidence

Read using the supported capacity reader only:

```
mise exec -- target/profiling/release/capacity profile target/benchmarks/bifrost/writes-parquet/20261120T132855Z/profiles/otlp-logs-c64/cpu-server-3807985.json.gz wyrd-scribe-ing
```

That reader shows ingress self time in compression, copying, allocation and
Variant processing. `prepare_slice` and IPC encoding are substantial inclusive
paths. The all-thread reader also shows protobuf traversal, copying, allocation
and projection on Tokio workers. These are existing diagnostic captures, not
qualification measurements or estimates of savings. Self and inclusive values
must not be added, nor thread-local percentages presented as process percentages.
The complete output was returned by the reader; no custom capture parser was used.

## Read latency and sustainable delivery

Keep existing process CPU admission and memory bounds. Moving decode/projection
off async request workers can protect their responsiveness, but moving work alone
does not lower CPU cost and must not create an unbounded pre-admission queue.
Workload fairness is not a read-latency guarantee: ingest and query are both
Bifrost. Do not advertise isolation that the lane scheduler does not provide.

The acceptance condition must cover competing ingestion and reads using existing
hot-read/L2 gates, plus memory/backlog drain and query freshness. It must not pass
by postponing staging, publication, Parquet encoding or cleanup beyond the write
measurement interval. Keep rotation thresholds, WAL compression, visibility,
durability and eventual background delivery unchanged. Run comparisons later with
the same workload and CPU accounting boundary; preserve full write-matrix results
and existing read/service criteria.

## Remaining uncertainty

Segmented ownership is a concrete useful mechanism, but existing profiles do not
prove it plus snapshot coalescing removes the required 26.8% CPU per byte. A
complete design needs additional source-backed CPU reductions agreed across
reviewers, rather than attributing the entire gap to waits or promising 80 now.
Implementation and qualification are deliberately deferred as the maintainer
requested. No new benchmark campaign is a prerequisite to agreeing the design.

## Reconciled disposition

C agrees with A and B on the shared flow: retain canonical Arrow ownership;
remove contiguous compressed-IPC assembly and reuse bounded worker-owned codec
context through a minimal selected-fork API extension; remove redundant protocol
and Variant interpretation where the exact same validation semantics are proven;
coalesce append-only snapshot publication; use identity or contiguous-range gather
fast paths only when the existing complete sort order proves them valid. Keep
serial durability initially. Do not make pipeline concurrency the CPU target's
central mechanism. Qualification later must reach 80 on the unchanged chosen
write workload while preserving full write-matrix and competing-read criteria,
governed memory and sustainable staging/publication drain.

Canonical Arrow ownership and the borrowed logs decoder already exist and are
preserved, not counted as new savings. The proposed protocol work carries the
existing preflight's checked counts and size facts into decoder/projector buffer
capacities and subsequent bounded interpretation, so the same facts are not
rediscovered and builders do not repeatedly grow. It does not remove preflight,
permit malformed inputs, weaken depth/size checks, or claim that all projection
work can disappear. Native inputs retain full untrusted-boundary validation.

The existing assembly path deliberately chooses a fresh claim Variant layout,
merges runs and re-encodes rolling objects (`claim_assembly.rs:265`). Blindly
publishing staged files would change that read contract. C rejects that shortcut.
Exact seeded sample selection pushed into Parquet row selection could reduce
background decoding while preserving the selected rows, but is not necessary to
the agreed first implementation and is not claimed as savings in write windows
without publication. No general scheduling priority or new read layout is added.

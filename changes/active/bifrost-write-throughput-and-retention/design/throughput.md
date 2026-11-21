# CPU-efficient Bifrost writes

Status: finalized — maintainer approved and locked on 2026-11-20 after
independent review and reconciliation by A, B and C. Implementation and
performance proof come later.

The maintainer confirmed: "i agree. lock it to the design". The selected flow,
80 input MiB/server CPU-second target, read-latency and sustainable-delivery
protections below are settled design decisions. This agreement authorizes the
design handoff; it does not approve a specification or start implementation.

Delivery is also locked in [the design notes](notes.md#locked-delivery-structure):
exactly two tasks, fence lifecycle then CPU throughput, with three logical
scenarios each. Every scenario runs the agreed logs-c64 and hot/Parquet read-c64
checkpoint after focused correctness verification. The full-matrix and competing
qualification below remains final acceptance work.

## Outcome

Reach **80 input MiB per server CPU-second** on the unchanged input population
used for the current throughput comparison, without sacrificing reads,
durability, visibility, supported inputs or sustainable background delivery.
The available 58.589 baseline is specifically OTLP logs at 64 clients. Evaluate
that population directly; do not average other payload sizes into its score or
substitute stored Arrow bytes. Shared-path changes benefit every writer, but
that does not establish identical numeric efficiency for every payload shape.
Report the full write matrix separately, including native writes' own metrics.

At that fixed input size, the current 20.388 CPU microseconds per row must fall
to approximately 14.93: **26.76% less CPU per byte**. This is a design budget,
not an assigned or measured saving for an individual optimization. Reducing
waiting alone is insufficient.

## Selected flow

1. The authenticated request goes through existing bounded preflight and
   admission. Preserve the already implemented borrowed log decoder and
   canonical Arrow projection. Reuse checked counts to reserve the existing
   decoder/projector storage. In particular, the log projector knows the row
   count but currently creates default-capacity columns. Reserve bounded column
   capacity once instead of repeatedly growing and copying it. Reuse common
   preflight facts instead of rescanning the same protobuf subtree; preserve
   fallback, merged-message, oneof, rejection and error-selection semantics.
2. Scribe retains canonical Arrow arrays for memtable visibility as it does
   today. Serialize each partition slice once for WAL using the installed
   Arrow fork's existing `StreamEncoder`. It returns ordered owned `Buffer`
   chunks. Keep these chunks through `PreparedWalAppend` and WAL IO instead
   of copying their contents into a contiguous `Vec` and shrinking that vector.
3. Extend that same encoder narrowly to accept/return its existing
   `IpcWriteContext`, or reset an independent stream while retaining that
   context. Reuse compressor and metadata scratch on the existing ingress
   worker. Schema, dictionary and end-of-stream state reset for every slice;
   no stream or tenant depends on a preceding stream. This uses the existing
   codec, not a Wyrd IPC implementation. Keep the configured compression
   settings and independent standard IPC stream format unchanged.
4. The prepared WAL owner holds every output buffer and its memory lease until
   IO settles. Compute checked lengths, CRC and payload digests over the ordered
   byte sequence. Extend the vectored writer to consume arbitrary chunk counts
   through bounded `IoSlice` windows, preserving partial-write, interrupted-write,
   zero-progress and rollback behavior. Do not flatten chunks to satisfy the
   current fixed descriptor count. Logical identity continues to use the same
   canonical arrays and existing volatile-column exclusions.
5. Preserve the serial durability flow: WAL writes and fsync, batched tenant
   PostgreSQL acceptance, memtable visibility, then ACK. The initial CPU design
   adds neither another lane nor a second in-flight shard group. Coalesce
   append-only inspection publication after the mailbox drain; keep publication
   required by control commands and before durable ACK.
6. Staging, sorted Parquet, publication and Forge continue under the current
   resource owner and read representation. This design neither postpones their
   CPU work to improve a short write window nor moves materialization to readers.

## Owners and necessary changes

- Existing OTLP scanner/projector: reuse checked count facts and reserve column
  capacity. Existing borrowed decoding is not new work or claimed savings.
- `scribe/preprocess.rs::encode_ipc` and its prepared payload owner: retain
  encoder chunks instead of the final contiguous stream.
- Existing pinned Arrow fork: one narrow independent-stream context lifecycle
  extension. `StreamEncoder::finish` currently consumes its private context;
  existing public APIs do not provide both segmented output and context reuse.
  Public `IpcDataGenerator::encode` reassembles a contiguous body, and
  `encode_to_buffers` is private. This is why the dependency change is needed.
- `scribe/wal.rs::PreparedWalAppend` and vectored append: ordered chunk iteration,
  checked lengths and retained owner accounting. No new WAL format.
- `scribe/shards.rs::run`: coalesce only redundant append inspection snapshots.

Reuse existing bounds and memory accounting for output and scratch. Charge
retained allocation capacity, including backing allocations hidden by slices;
bound worker-local retained scratch and release oversized buffers. Failure and
cancellation must return or drop context without sharing live output allocations.
There is no global encoder lock, unbounded pool, new option or worker count.

## Evidence and reviewer agreement

The existing logs-c64 profile identifies substantial CPU in protobuf traversal,
copying/allocation, IPC serialization/compression and persistence. WAL worker
samples are a small part of that capture. The current source constructs a fresh
IPC writer for each slice, copies encoded chunks into a final vector and calls
`shrink_to_fit`. It also publishes inspection statistics after each append-only
mailbox command. These are specific work-removal opportunities.

The profile predates current changes and includes heap sampling. Its frame
percentages guide the design; they are not a ledger of promised process savings.
Compressor context reuse does not eliminate actual compression or entropy-table
work. The reviewers do not assign invented percentages to these edits.

All three reviewers explicitly consented to this common core after independent
inspection and direct reconciliation:

- [A: IPC ownership, context lifecycle and compatibility](reviews/cpu-a.md).
- [B: metric budget, current source and supported reader outputs](reviews/cpu-b.md).
- [C: read-latency, memory and sustainable-delivery protections](reviews/cpu-c.md).

These records preserve independent findings and corrections. This document is
the common proposal; an exploratory alternative in a review is not selected
merely because it appears in that review.

## Read and correctness protections

Keep canonical columns, Variant representation, sort keys/order, learned layouts,
Parquet geometry, compression, indexes and statistics. Do not bypass validation,
change hash semantics, acknowledge before acceptance, relax fsync or increase
ingress/persistence concurrency. Shared workload fairness alone is not a
read-latency guarantee: ingestion and queries both belong to Bifrost.

No general staged-file passthrough is proposed. Claim assembly can select a new
Variant layout, merge/sort runs and pack objects; bypassing it would change the
read contract. The separate trace-ingest design remains its existing owner;
reuse its shared scanner/builder mechanisms where applicable rather than create
a competing parser or count unimplemented trace work as a logs saving.

## Later implementation proof

Correctness verification covers exact compressed IPC byte parity with the current
writer, dictionaries/nesting/nulls/nonzero slice offsets, consecutive independently
decodable streams with reused context, unchanged logical/retry identities, cross-pod
conflicts, checked lengths and memory capacity, many-chunk short writes and errors,
and existing accepted-write recovery/visibility/shutdown scenarios. A successful
round trip alone does not establish byte or digest parity.

Run the existing full writes comparison using the same input, settings and CPU
accounting. The fixed logs-c64 population must reach 80 with improvement outside
run spread; other populations must not materially regress. A faster ACK with
unchanged CPU efficiency does not meet this objective.

Then qualify existing `bench:l1:bifrost:hot`, `bench:l1:bifrost:parquet` and
`bench:l2:bifrost` workloads: retain the current latency/correctness criteria and
compare read tails against the matching baseline. In the competing lane, use
the existing calibration from the improved write capacity, so readers face the
new offered ingest load. Retain bounded memory, stable staging/publication
backlog, drain/recovery and query visibility. The selected dirty logs record
reported no publication within its windows; it cannot alone establish these
properties. Run experiments serially via existing mise tasks and supported
readers, after implementation, without altering the measurement boundary.

If implementation misses 80 or regresses reads, report the unmet requirement
and return the evidence to design. Do not silently substitute a lower target,
change input sizes, or begin another redesign without maintainer input.

## Deferred

The prior two-group proposal is not the selected CPU solution. The failed eager
overlap run formed smaller groups; a collection deadline changes that mechanism
but has no demonstrated CPU-budget benefit. Do not implement it as part of this
initial design. PL/pgSQL/held-connection fencing, extra shards, skipped sorting,
reduced compression and deferred query-time work also remain excluded.

No implementation, fresh benchmark, specification approval or readiness verdict
is implied by reviewer agreement.

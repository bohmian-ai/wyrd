---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 3
requirements: [REQ-001, REQ-004, REQ-010, REQ-011, REQ-012, REQ-013]
depends_on: [TASK-001]
---

# Reach 80 input MiB per server CPU-second without read regressions

## Outcome and dependency

Deliver the approved CPU reduction on the fixed OTLP logs-c64 population while
preserving accepted input, byte/identity compatibility, bounded ownership and read
latency under sustainable ingestion. Faster ACK alone does not satisfy the target.

Implement [spec revision 3](../spec.md) using the selected
[CPU flow and protections](../design/throughput.md). Task 1 must first supply
domain-history independence, accepted-only replay, durable handoff/cleanup behavior,
their regression tests and the previous accepted benchmark checkpoint. This task
changes preparation and WAL payload ownership underneath those guarantees, so it
must exercise them through the new representation. Do not duplicate fence machinery.

The [shared execution/checkpoint recipe](README.md) applies to all three scenarios.
The original pre-task-1 baseline remains the original comparison; do not reset it
to erase accumulated cost. Existing reported profiles inform the selected design,
but no additional diagnostic/profile experiment is required by this task.

## Existing owners and selected implementation

- Existing OTLP bounded preflight/borrowed decoding in
  `wyrd-server/src/otlp_logs_decode.rs` and
  `vala-bifrost-redux/src/tables/logs/projection.rs` already produce canonical Arrow.
  Reuse checked row/count facts for common-path scanning and capacity reservation;
  preserve merged protobuf/oneof/fallback/error behavior and pre-allocation admission.
  Borrowed decoding itself is existing work, not this task's claimed new gain.
- `scribe/shards.rs::run` currently republishes inspection after append-only mailbox
  commands. Coalesce redundant append snapshots after drain, preserving control
  publications and the pre-ACK visibility boundary. Do not add a second in-flight
  durability group or new ingress/persistence concurrency.
- `scribe/preprocess.rs::encode_ipc` currently builds a fresh compressed `StreamWriter`
  result in a contiguous vector and shrinks it. Use the pinned Arrow `StreamEncoder`
  output buffers, retaining their order through prepared WAL ownership instead.
- Existing pinned Arrow fork revision `d0c75b582fd4d4478b7bac2988b585ada4a3efd8`
  supplies the codec. Its public context lifecycle does not currently provide both
  segmented output and reuse: narrowly extend that owner to accept/return its existing
  `IpcWriteContext`, or reset independent streams while retaining context. Keep the
  workspace dependency/lock resolution coherent across the Arrow stack; no custom
  encoder, competing Arrow copy or unapproved dependency. Private API shape is local.
- `scribe/wal.rs` prepared append and vectored writer consume the ordered chunks with
  checked lengths/CRC/digests and bounded `IoSlice` windows. The existing vectored-write
  implementation is input to this task: retain it, extend arbitrary chunk handling,
  and preserve short-write/Interrupted/WriteZero/rollback semantics. Never flatten to
  fit a descriptor limit. Memory ownership/leases survive until IO settles.
- Reuse bounded encoder scratch on existing workers through existing memory owners.
  Charge retained capacity and backing allocations, cap reuse/release oversized
  scratch, reset dictionary/schema/EOS between streams and keep failure cleanup safe.

- Read-side CPU, so reads keep REQ-013's hot-read criteria while sharing the host
  with improved ingest. The `point-memtable-c64` server profile attributes 8.7% of
  CPU to `scribe/memtable.rs::ReadableSource::prepare`, of which ~4.9% builds a
  projected `Statistics` for every captured batch before `BatchPruning::keep`
  runs. It also attributes 3.0% to a second logical optimization pass:
  `oracle/mod.rs` plans through `into_optimized_plan()`, then
  `create_physical_plan` optimizes the same logical plan again. Prune captured
  batches against their recorded statistics through the projected-to-source
  column mapping first, and build projected statistics only for batches that
  pass. Keep cast/null-filled columns unknown and keep the identity-bound second
  pass. Physically plan the already optimized, already authorized logical plan
  through the session's registered query planner without re-optimizing it.
  `authorize_payload_columns` still inspects the plan that executes. No plan or
  statistics cache, pruning rule, query result, or Oracle routing changes.

Output canonical arrays still feed the current memtable and staged/sorted Parquet
path. IPC output still feeds the existing WAL format and task 1's accepted recovery.
There is no read-layout, compression, resource-owner or acknowledgement redesign.

## Ordered acceptance scenarios and evidence

### 1. Checked preparation avoids repeated work without changing input behavior

**Obligations:** REQ-001, REQ-010.

**Start/input:** Use existing OTLP log payloads and canonical projection fixtures,
including merged messages, oneofs, fallback decoding and bounded invalid envelopes.
Include valid batch shapes already supported by the shared preparation path and a
mailbox sequence mixing append-only and control commands.

**Action/result:** Reuse preflight facts and reserve bounded projector/decoder
capacity; decode/project through the production path. Assert identical canonical
rows, logical input interpretation, stable rejection/error selection and admission
before expensive allocation. Drain mailbox appends with coalesced inspection, then
observe accurate control-command and before-ACK inspection/row visibility. Enqueue
and durable ACK boundaries remain unchanged. No tenant's retained data becomes
another tenant's scratch or input.

**Proof owner/tier:** Add/extend inline OTLP scanner/decoder/projector and shard tests
for pure semantic branches; use the existing `wyrd-testing` OTLP `logs_export` and
negative protocol modules for actual wire-to-query output, and Scribe/server owner
inspection journeys for caller-visible state. Existing correct semantics use
regression proof rather than invented production changes.

**Expected RED:** Add the smallest owner-level assertion for the missing checked-fact
handoff/capacity use or redundant publication behavior where it is observable.
Semantic parity assertions may already pass and then serve as regression proof;
do not invent a public performance counter or mirror implementation just to force
RED. Static work-removal claims require a focused source/diff check and the benchmark.

**Evidence:** Pending. Append actual RED/GREEN or justified verification-only/static
proof, then the three benchmark checkpoint reports/comparisons before scenario 2.

### 2. Independent IPC chunks reach WAL without copying into one stream buffer

**Obligations:** REQ-001, REQ-004, REQ-011.

**Start/input:** Prepare canonical batches containing nested arrays, nulls,
dictionaries and nonzero slice offsets using current compression settings. Include
multiple slices, tenants and enough chunks to exceed one vectored descriptor window.
Use the current writer as the exact-byte reference; an IPC round trip alone is
insufficient. Use real accepted WAL and task 1's canonical replay authority.

**Action/result:** Encode each slice as an independent standard IPC stream and carry
the owned ordered chunks through preparation, lengths, CRC/digests and WAL IO without
flattening. Compare exact compressed stream bytes, logical identity and framing to
the reference. Exercise short writes, interruption, zero progress, IO failure and
rollback; prove payload owners and charged memory remain until IO settles. Restart
the actual WAL consumer and query the accepted rows; keep lost-response acceptance,
cross-pod duplicate suppression, visibility and shutdown behavior intact. No change
to WAL format, compression settings or task 1's acceptance boundary is permitted.

**Proof owner/tier:** Inline preprocess/WAL tests own byte parity, many-chunk checked
lengths, scripted writes, lease lifetime and error cleanup. Existing redux Scribe
integration owns encode → WAL → actual replay, and `wyrd-testing` Scribe recovery
journeys prove real client input → durable ACK → restart → exact query result.
Retain task 1's focused accepted-only/cross-pod tests through this payload change.

**Expected RED:** The current prepared payload is contiguous and the existing vectored
descriptor assumptions do not provide arbitrary owned IPC chunks. New owner tests
must expose the unsupported chunk ownership/window behavior before changing it;
existing exact-byte/identity behavior remains the reference, not a newly chosen codec.

**Evidence:** Pending. Append focused RED/GREEN, parity and connected replay proof,
then the three benchmark checkpoint reports/comparisons before scenario 3.

### 3. Bounded context reuse preserves independent streams and meets the target

**Obligations:** REQ-012–013.

**Start/input:** Use scenario 2's chunked IPC/WAL flow on existing ingress workers,
alternating tenant/schema/dictionary inputs and ordinary/oversized batches. Hold
earlier output in flight while later independent streams are encoded. Include
failure/cancellation and the production staging/publication consumer path.

**Action/result:** Reuse only bounded, charged compressor/metadata scratch through
the narrow pinned-encoder lifecycle extension. Decode each resulting stream alone;
compare its bytes to the unreused reference. No schema/dictionary/EOS or live output
allocation leaks across streams/tenants. Assert actual retained capacity accounting,
including sliced backing allocations, release of oversized scratch, and cleanup on
failure/cancellation. No global lock, unbounded pool or extra worker count.

After focused correctness and the scenario checkpoint pass, establish the combined
80 input MiB/server CPU-second result on unchanged logs-c64, outside repetition
spread. Complete the full write matrix and read/mixed-load qualification below.
Other payloads do not materially regress; reads retain existing criteria and baseline
tails at the newly calibrated ingest load. Demonstrate bounded memory, stable staged/
publication backlog, visibility, drain and recovery. Do not defer CPU beyond the
measurement, move it to queries, average payloads together or count stored Arrow bytes.

**Proof owner/tier:** Dependency-owner tests cover the new context lifecycle, independent
stream state and parity. Inline ingress/preprocess/memory tests cover actual Wyrd
ownership and retained allocations. Existing redux staging and real Scribe/OTLP
journeys cover downstream consumption; final existing benchmark experiments establish
CPU/read/sustained-load outcomes. The implementer records the narrow dependency test
runner from its actual owning checkout rather than using an unrelated installed copy.

**Expected RED:** The current encoder lifecycle cannot return/reuse context across
independent streams and Wyrd does not charge/bound that retained scratch. Add the
focused lifecycle/ownership assertion before the extension. Performance remains
unproven until measured; no fabricated failing test substitutes for 80 qualification.

**Read-side protection (REQ-013), within this scenario:**

- *Start/input:* existing memtable capture fixtures, with filtered and
  unfiltered projections, cast and null-filled columns, batches without
  recorded statistics, and identity predicates. Existing Oracle SQL over memtable
  and Parquet sources, including column-restricted (payload-authorized) tables
  and parameterized queries.
- *Action/result:* a capture read returns exactly the same batches and rows as
  before. Projected statistics are built only for batches that survive pruning
  on their recorded bounds. A query's executed physical plan and its rows are
  unchanged, the logical optimizer runs once per query, and payload authorization
  still refuses unauthorized columns. `point-memtable-c64` server CPU per query
  falls outside repetition spread, against both the TASK-001 checkpoint and the
  original baseline, with read tails no worse.
- *Proof owner/tier:* inline `scribe/memtable.rs` tests for pruning parity and
  for statistics built only for kept batches, starting from
  `scribe::memtable::tests::capture_read_prunes_batches_before_projecting`. Inline
  `oracle` tests for single-pass optimization and unchanged plans, starting from
  `oracle::exec::tests::physical_root_alone_selects_query_class`. The existing
  Oracle journey lane covers end-to-end rows, refusals and publication
  transitions. `bench:l1:bifrost:hot` measures the outcome.
- *Expected RED:* an owner-level assertion that projected statistics are built
  for a batch pruned by recorded bounds, and an assertion that the logical
  optimizer runs once per planned query. Both fail on current code. Row and
  plan parity tests may already pass and serve as regression proof.

**Evidence:** Pending. Append focused RED/GREEN, context/memory proof, the read-side
proof above, the three checkpoint reports/comparisons and final qualification
records here.

## Verification and final qualification

Use the [shared focused-test and checkpoint recipe](README.md). Affected owner
runners, confirmed in `mise.toml`, are:

```bash
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:otlp
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:server
```

Read-side focused tests (existing names; add new ones beside them):

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::capture_read_prunes_batches_before_projecting) | test(=oracle::exec::tests::physical_root_alone_selects_query_class)'
```

Retain focused task-1 regression checks and run affected owners once at task
verification. Finish `mise run fmt`, `mise run lints`, `mise run check:deps`, and
`mise exec -- git diff --check`. Include dependency-owner tests for the approved
Arrow extension and verify the resolved graph retains one coherent Arrow stack.
Other generated/surface checks apply only if their inputs change. Integrated review
owns complete shipped-surface journey lanes and the broader required aggregate.

Run the following once on the completed candidate, serially, with matching baseline
controls and supported reports/comparisons:

```bash
mise run bench:l1:bifrost:writes
mise run bench:l1:bifrost:hot
mise run bench:l1:bifrost:parquet
mise run bench:l2:bifrost
```

These commands require the clean-candidate evidence described in README. L2 consumes
the newest compatible improved **full** writes record; verify its source/control
match and calibration before interpreting the competing lane. Scenario subsets and
the earlier dirty logs record cannot establish publication sustainability or serve
as full calibration. Keep the original baseline comparisons as well as the task-1
checkpoint comparison; report unmet/uncertain outcomes explicitly, not merely VALID.

Keep compression level/windows, canonical Arrow/Variant columns, sorting, Parquet
geometry, learned layouts, indexes/stats, validation, serial durability and existing
resource/concurrency limits. No two-group pipeline, extra shards, held PostgreSQL
connections, staged-file passthrough or independent trace-parser redesign. If the
combined implementation misses 80 or regresses protected behavior, stop and return
the measured failure to design; do not lower the target or chain another redesign.

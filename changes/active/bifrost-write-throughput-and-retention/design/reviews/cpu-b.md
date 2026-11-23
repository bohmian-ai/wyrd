# Independent CPU design review B

Status: reconciled — B consents to the shared CPU-first core below.

## Recommendation

Target 80 input MiB per server CPU-second for the fixed OTLP logs c64
population that currently measures 58.589, with the unchanged numerator and
whole-server CPU denominator. Preserve the remaining write populations and
read workloads as no-regression requirements. Native rows/s and differently
sized OTLP payloads must not be averaged into that target.

Choose a CPU-first columnar ownership design. Keep the serial durability
boundary initially; the bounded two-group proposal is not the primary CPU
solution. It overlaps waits but adds ownership, wakeups and failure states.

### Connected implementation

1. Preserve bounded raw-input preflight and every trust-boundary refusal.
   Keep the existing borrowed log decoder: it already avoids the generated
   request on its successful path. Canonical Arrow local handoff is also
   already implemented; preserving it is an invariant, not a new saving.
2. Pre-size the log projection columns: project_log_input currently creates
   LogColumns::default() at projection.rs:146 despite input.records.len()
   already being known. Reserve bounded upper capacities before appending
   accepted rows, using the existing charged output ceiling; retain partial
   rejection semantics. Apply equivalent changes to other projector owners
   only where their current construction demonstrates the same reallocations.
   Preserve canonical Arrow buffers through existing local handoff and common
   preparation. Do not claim removal of a local IPC round trip that is already
   absent.
3. Replace the per-slice contiguous WAL IPC payload with the installed Arrow
   StreamEncoder's ordered Buffer output. Carry those owned buffers through
   PreparedWalAppend to the existing vectored WAL writer. Sum the exact
   lengths and fold payload SHA, WAL digest and CRC over the ordered bytes.
   Do not flatten the chunks into another Vec before writing. Existing WAL
   framing, COMMIT identity and logical digest remain authoritative.
4. Reuse compressor/FlatBuffer scratch across independent WAL slice streams
   within the existing bounded ingress CPU owner. The pinned Arrow API needs
   a narrow extension: StreamEncoder owns IpcWriteContext privately and
   finish consumes self. Add a constructor/finish context handoff or a reset
   that clears schema/dictionary state while retaining that scratch. Reuse
   the existing IPC codec; do not implement another codec in Wyrd. Preserve
   ZSTD level 1 and the existing compression/window settings.
5. Charge actual retained Buffer capacities and reused scratch to existing
   resource owners. Bound per-worker retained scratch and release oversized
   capacity; do not use an unbounded global pool or lock all encodes behind
   one compressor. A failed encode must return/drop its scratch safely.
6. Coalesce append-only inspection snapshot publication after mailbox drain.
   Preserve visibility before ACK and publication required by state-changing
   control commands. This is secondary repeated-work removal.
7. Preserve staging under the existing Parquet writer: sort keys, sorted
   output, bloom/filter/statistics policy, compression, file geometry, learned
   Variant layout, row accounting and physical schema. The final agreed core
   adds no speculative gather or claim-passthrough optimization. Do not remove
   sorting or move conversion to query time to improve the ingest number.

Read latency is protected by preserving the current read representation and
existing resource governor/workload admission. No greater ingress or
persistence concurrency, bigger unbounded batches, delayed staging, or
query-time materialization is part of this proposal. Mixed ingest/read
qualification is required because write-only measurements cannot prove
read-latency preservation.

## Evidence

- Latest existing selected dirty record:
  target/benchmarks/bifrost/writes-parquet/20261120T194551Z-09e81560c.json.
  Supported report: 58.589 input MiB/CPU-s, 20.388 CPU microseconds/row,
  11.915 cores, no errors; all three windows report no Parquet publication.
  Staging does run and is included in the server CPU denominator.
- Fixed-input arithmetic: 58.589 / 80 = 0.7323625. The design needs about
  26.76% less server CPU per input MiB, or about 14.93 microseconds/row for
  this fixed payload. A 36.55% increase in efficiency is not a 36.55% CPU cut.
- Existing profiled record 20261120T132855Z, logs-c64 capture:
  tokio-rt-worker 60,746 samples; wyrd-scribe-ing 40,861;
  wyrd-scribe-per 38,314; wyrd-scribe-wal 1,703.
  These are diagnostic samples, not qualification or an additive latency
  model. Top self frames identify copying, allocation, protobuf traversal,
  ingress ZSTD and persistence encoding/interleave work. This is why the
  design attacks representation and encoding work before queue overlap.
- scribe/preprocess.rs:842 encode_ipc creates a new StreamWriter for each
  slice, writes to a contiguous Vec and shrink_to_fit before returning Bytes.
- Pinned Arrow d0c75b5 writer.rs:1854 StreamEncoder returns ordered Buffer
  chunks; its constructor initializes private IpcWriteContext at 1886 and
  finish consumes the encoder at 1918. It does not presently expose cross
  stream context reuse.
- Pinned Arrow writer.rs:928 encode_to_buffers is PRIVATE. Public
  IpcDataGenerator::encode supports an external context but materializes an
  arrow_data Vec; that is not the final copy-efficient design.
- Pinned Arrow compression.rs:33 IpcWriteContext already owns the reusable
  ZSTD compressor, FlatBufferBuilder and optional scratch.
- tables/logs/projection.rs:141 already projects borrowed LogInput into
  canonical columns. otlp_logs_decode.rs:578 already builds borrowed views,
  with generated decode only as a fallback.
- tables/logs/projection.rs:146 starts default empty LogColumns even though
  LogInput.records provides the bounded row count. Its scalar/optional column
  Vecs grow as push appends records.
- gate/mod.rs:1122 hashes the projected batch; preprocess.rs:804 hashes final
  partition slices again. These are not automatically redundant: stamping,
  managed correlation columns and slicing can change the identity inputs.
  No hash reuse is recommended without proving exact identical inputs; never
  change the established retry identity just to remove a hash pass.
- scribe/parquet_writer.rs:441 builds concatenated sort keys, lexsort indices,
  positions and interleaves columns. Those costs may be reduced only while
  preserving sorted output; deletion of the sort is outside this design.
- scribe/shards.rs:1417 snapshot publication collects stats, bucket-memory and
  pressure candidates; run publishes after each append command even though
  Append only queues the request.
- capacity/query/mod.rs:828 defines the input-wire-MiB/server-CPU metric;
  :856 defines server CPU per acknowledged row. :1085 supplies current sanity
  floors, which are not an 80 MiB/CPU-s acceptance criterion.

## Required implementation proof

Use the same full writes benchmark and supported capacity readers, serially,
after implementation. The fixed logs-c64 population must reach 80 with no
weakened work/CPU accounting. Other write populations must not materially
regress. Existing hot, Parquet and mixed ingest/read workloads must retain
their point/tail/scan latency criteria, correctness and visibility under
load. Qualification must also check stable staging/retention backlog rather
than move CPU outside the observed window.

Test compressed IPC parity and round-trip for dictionaries, nulls, nested
Variant and nonzero slice offsets; exact chunk concatenation versus ordinary
Arrow stream framing; no cross-stream dictionary dependence; buffer capacity
accounting; short vectored writes/errors; identical retry/conflict decisions;
and crash recovery at existing acceptance boundaries.

The profile names enough costly owners to justify this implementation design,
but cannot assign exact savings to unimplemented edits. The design targets
the required CPU budget; performance proof follows implementation. If it
misses, report the unmet target instead of renaming a wall-throughput gain.

## Rejected approaches

- Eager fence/next-group overlap previously formed smaller groups and
  regressed. A bounded collection deadline changes that design, but remains
  a wall-throughput hypothesis and is not the CPU-first recommendation.
- PL/pgSQL fence/held-connection implementation previously regressed badly.
- No ACK-before-fence, fsync removal, reduced compression, skipped validation,
  changed read layout, deferred materialization onto readers, new shard count,
  or suppression of background CPU in the metric.
- No new benchmark run, build, production code change or custom record parser
  was used in this review.

## Existing supported reader output

The following appendices are unedited output of supported readers, included
for evidence. These are pre-existing diagnostic/dirty records, not new
qualification runs.

<details><summary>capacity report, latest selected dirty record</summary>

```text
# bifrost L1 writes-parquet — VALID

Under the Parquet Scribe settings, whose windows pay for the staging and publication they cause, what does each native, OTLP spans, and OTLP logs write cost the server per CPU-second and per provisioned vCPU at 1 and 64 closed-loop clients up to its durable acknowledgement, with no error, how much publication ran and what backlog remained, and do the native and OTLP write peaks clear their sanity floors?

commit 09e81560c3d75961a5bd1b7ba32d4e4a874f9c67 dirty (override, diff ef7c3eca83f0f73633a9decb9e4031604656992feb443cca3e15383960948234) | host AMD Ryzen 9 9950X 16-Core Processor, 32 CPU / 91 GiB, linux Linux (Pop!_OS 24.04) x86_64 | elapsed 47.5s of 480s | started 2026-11-20T19:45:51.233105674+00:00

timing (schema 3): server benchmark histograms: integer nanoseconds from the owning server boundary's std::time::Instant timers, recorded into bounded HDR histograms and read as cumulative snapshots from the loopback metrics listener; each window is the bucket-count difference of two snapshots of one process

| step | params | rate | server p50/p95/p99 ms (client) | errors | server CPU | peak mem | driver CPU | criterion | observed | outcome |
|---|---|---|---|---|---|---|---|---|---|---|
| otlp-spans-c1 | clients=1 op="otlp-spans" window_seconds=10 | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-spans-c64 | clients=64 op="otlp-spans" | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-spans-peak | — none | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-spans-1k-c1 | clients=1 op="otlp-spans-1k" window_seconds=10 | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-spans-1k-c64 | clients=64 op="otlp-spans-1k" | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-spans-1k-peak | — none | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-logs-c1 | clients=1 op="otlp-logs" | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| otlp-logs-c64 | clients=64 op="otlp-logs" | 1148.592 requests/s; 21.953 MiB/s/vCPU; 58.589 MiB/cpu-s; 18377.473 rows/s/vCPU; 49047.342 rows/cpu-s; 20.388 µs/row; 33.461 MiB/s/vCPU; 88.82 MiB/cpu-s | (37.487, 37.519] / (108.2, 108.265] / (139.985, 140.116] (client 46.539 / 119.091 / 151.905) | 0 total (median 0) | 60.29 s / 11.915 cores used / 32 vCPU | 4421.48 MiB (sampled 10 ms) | 19.07 s | no-errors: errors ≤ 0 errors every rep | 0 errors spread 0 | met |
| otlp-logs-peak | — none | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| native-write-c1 | clients=1 op="native-write" window_seconds=10 | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| native-write-c64 | clients=64 op="native-write" window_seconds=10 | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |
| native-write-peak | — none | — no rate for this step | — no request latency for this step | — not counted | — not measured | — not sampled | — not measured | — no criterion applies | — no criterion applies | — omitted by selection |

## Step detail

- otlp-logs-c64 (Measured, 3 repetitions):
  - busy_retries: median 0 min 0 max 0
  - client_p50_ms: median 46.539 ms min 45.266 max 46.592
  - client_p95_ms: median 119.091 ms min 110.044 max 123.727
  - client_p99_ms: median 151.905 ms min 144.739 max 177.107
  - cores: median 11.915 cores min 11.577 max 12.279
  - cpu_ms_per_query: median — work reading `queries` is missing
  - cpu_us_per_row: median 20.388 µs/row min 20.26 max 20.504
  - drain_seconds: median 0.014 min 0.011 max 0.045
  - errors: median 0 errors min 0 max 0
  - input_mib_per_cpu_s: median 58.589 MiB/cpu-s min 58.261 max 58.96
  - input_mib_per_s_per_vcpu: median 21.953 MiB/s/vCPU min 21.196 max 22.355
  - late_requests: median 64 min 64 max 64
  - late_rows: median 32768 min 32768 max 32768
  - max_ms: median (210.239, 210.371]
  - memory_per_mib_s: median 4252879.802 B/(MiB/s) min 4094502.214 max 4261215.87
  - memtable_backlog_mib: median 1824.618 MiB min 1793.695 max 2094.401
  - p50_ms: median (37.487, 37.519] ms
  - p95_ms: median (108.2, 108.265] ms
  - p99_ms: median (139.985, 140.116] ms
  - phase_ack_p50_ms: median (0.145, 0.145] ms
  - phase_ack_p99_ms: median (0.489, 0.49] ms
  - phase_admission_p50_ms: median (0.094, 0.095] ms
  - phase_admission_p99_ms: median (0.299, 0.299] ms
  - phase_decode_p50_ms: median (0.652, 0.652] ms
  - phase_decode_p99_ms: median (2.2, 2.202] ms
  - phase_otlp_decode_p50_ms: median (0.595, 0.596] ms
  - phase_otlp_decode_p99_ms: median (1.431, 1.432] ms
  - phase_preprocess_p50_ms: median (2.068, 2.07] ms
  - phase_preprocess_p99_ms: median (4.092, 4.094] ms
  - phase_scribe_p50_ms: median (32.866, 32.883] ms
  - phase_scribe_p99_ms: median (135.922, 136.053] ms
  - phase_shard_wait_p50_ms: median (7.406, 7.41] ms
  - phase_shard_wait_p99_ms: median (104.202, 104.268] ms
  - phase_visibility_p50_ms: median (5.956, 5.96] ms
  - phase_visibility_p99_ms: median (16.015, 16.024] ms
  - phase_wal_append_p50_ms: median (3.506, 3.508] ms
  - phase_wal_append_p99_ms: median (30.507, 30.523] ms
  - phase_wal_sync_p50_ms: median (10.568, 10.576] ms
  - phase_wal_sync_p99_ms: median (44.761, 44.794] ms
  - published_bytes_per_row: median 0 B/row min 0 max 0
  - published_files: median 0 files min 0 max 0
  - published_mib: median 0 MiB min 0 max 0
  - queries_per_cpu_s: median — work reading `queries` is missing
  - queries_per_s_per_vcpu: median — work reading `queries` is missing
  - rate: median 1148.592 requests/s min 1108.979 max 1169.652
  - requests: median 5812 requests min 5605 max 5925
  - retained_mib: median 5418.078 MiB min 5196.841 max 5522.253
  - rows: median 2975744 rows min 2869760 max 3033600
  - rows_per_cpu_s: median 49047.342 rows/cpu-s min 48771.704 max 49357.174
  - rows_per_s: median 588079.148 rows/s min 567797.278 max 598861.742
  - rows_per_s_per_vcpu: median 18377.473 rows/s/vCPU min 17743.665 max 18714.429
  - server_failures: median 0 requests min 0 max 0
  - stage_syncs: median 209 syncs min 193 max 214
  - staged_backlog_mib: median 1339.432 MiB min 835.508 max 1803.054
  - staged_bytes_per_row: median 175.482 B/row min 169.402 max 177.57
  - storage_ratio: median — measured by the `parquet` experiment after its settlement; a write window never waits for publication
  - stored_mib_per_cpu_s: median 88.82 MiB/cpu-s min 88.782 max 89.867
  - stored_mib_per_s_per_vcpu: median 33.461 MiB/s/vCPU min 32.132 max 34.067
  - visibility_p99_ms: median — L1 measures closed-loop acknowledgement; visibility is judged at L2
  - wal_bytes_per_row: median 256.169 B/row min 255.406 max 256.235
  - wal_syncs: median 199 syncs min 186 max 209
  - wire_bytes: median 3727384796 B min 3594592477 max 3799833123
  - wire_bytes_per_row: median 1252.582 B/row min 1252.576 max 1252.589
  - repetition 1 observed: server timing: 68606 observations in 20 series, 0 scanned queries
  - repetition 1 observed: driver timing: 5989 observations in 1 series, 0 scanned queries
  - repetition 1 observed: no Parquet file was published in the `otlp-logs` window, so its server CPU includes no publication cost
  - repetition 2 observed: server timing: 67354 observations in 20 series, 0 scanned queries
  - repetition 2 observed: driver timing: 5876 observations in 1 series, 0 scanned queries
  - repetition 2 observed: no Parquet file was published in the `otlp-logs` window, so its server CPU includes no publication cost
  - repetition 3 observed: server timing: 65023 observations in 20 series, 0 scanned queries
  - repetition 3 observed: driver timing: 5669 observations in 1 series, 0 scanned queries
  - repetition 3 observed: no Parquet file was published in the `otlp-logs` window, so its server CPU includes no publication cost
  - diagnostics (wall time; nearest-rank bucket bounds; rows overlap and never add or subtract):
    - driver client.otlp.export: driver: `client.otlp.export` start until its successful end, the cohort's calls
      - repetition 1: n 5989, p50 (46530.559, 46563.327] µs, p95 (110034.943, 110100.479] µs, p99 (144703.487, 144834.559] µs, mean (53493.372, 53531.175] µs
      - repetition 2: n 5876, p50 (45252.607, 45285.375] µs, p95 (119078.911, 119144.447] µs, p99 (151781.375, 151912.447] µs, mean (54539.977, 54578.721] µs
      - repetition 3: n 5669, p50 (46563.327, 46596.095] µs, p95 (123666.431, 123731.967] µs, p99 (177078.271, 177209.343] µs, mean (56986.54, 57026.57] µs
    - driver client.otlp.export.build: driver: `client.otlp.export` `build` work, accumulated per call across attempts, for every call that reached it; never added to the call's elapsed
      - repetition 1: n 5989, p50 (933.375, 933.887] µs, p95 (1778.687, 1779.711] µs, p99 (3371.007, 3373.055] µs, mean (1045.046, 1045.786] µs
      - repetition 2: n 5876, p50 (908.799, 909.311] µs, p95 (1649.663, 1650.687] µs, p99 (2994.175, 2996.223] µs, mean (1007.085, 1007.796] µs
      - repetition 3: n 5669, p50 (871.423, 871.935] µs, p95 (1653.759, 1654.783] µs, p99 (3133.439, 3135.487] µs, mean (987.274, 987.972] µs
    - driver client.otlp.export.export: driver: `client.otlp.export` `export` work, accumulated per call across attempts, for every call that reached it; never added to the call's elapsed
      - repetition 1: n 5989, p50 (45154.303, 45187.071] µs, p95 (108396.543, 108462.079] µs, p99 (143785.983, 143917.055] µs, mean (52077.919, 52115.076] µs
      - repetition 2: n 5876, p50 (44171.263, 44204.031] µs, p95 (117047.295, 117112.831] µs, p99 (150863.871, 150994.943] µs, mean (53100.83, 53138.651] µs
      - repetition 3: n 5669, p50 (45088.767, 45121.535] µs, p95 (121634.815, 121700.351] µs, p99 (173670.399, 173801.471] µs, mean (55295.139, 55334.602] µs
    - driver client.otlp.export.token: driver: `client.otlp.export` `token` work, accumulated per call across attempts, for every call that reached it; never added to the call's elapsed
      - repetition 1: n 5989, p50 (2.433, 2.435] µs, p95 (3004.415, 3006.463] µs, p99 (7786.495, 7790.591] µs, mean (365.5, 365.752] µs
      - repetition 2: n 5876, p50 (2.393, 2.395] µs, p95 (3305.471, 3307.519] µs, p99 (6230.015, 6234.111] µs, mean (427.468, 427.777] µs
      - repetition 3: n 5669, p50 (2.563, 2.565] µs, p95 (4370.431, 4374.527] µs, p99 (9027.583, 9035.775] µs, mean (698.378, 698.87] µs
  - repetition 1 server measured window (nearest-rank bucket bounds):
    - timing bifrost.otlp.logs success case otlp-logs phase repetition-1: n 5989, p50 (37486.591, 37519.359] µs, p95 (99549.183, 99614.719] µs, p99 (136052.735, 136183.807] µs, mean (43792.107, 43823.525] µs
    - timing bifrost.otlp.logs.decode success case otlp-logs phase repetition-1: n 5989, p50 (591.359, 591.871] µs, p95 (950.783, 951.295] µs, p99 (1430.527, 1431.551] µs, mean (617.921, 618.358] µs
    - timing bifrost.scribe.write success case none phase none: n 303, p50 (31342.591, 31358.975] µs, p95 (101318.655, 101384.191] µs, p99 (133365.759, 133431.295] µs, mean (39130.305, 39157.32] µs
    - timing bifrost.scribe.write success case otlp-logs phase repetition-1: n 5989, p50 (32866.303, 32882.687] µs, p95 (95092.735, 95158.271] µs, p99 (131006.463, 131071.999] µs, mean (39282.767, 39310.328] µs
    - timing bifrost.scribe.write.ack success case none phase none: n 303, p50 (24.255, 24.271] µs, p95 (272.127, 272.383] µs, p99 (706.047, 706.559] µs, mean (63.454, 63.499] µs
    - timing bifrost.scribe.write.ack success case otlp-logs phase repetition-1: n 5989, p50 (144.895, 145.023] µs, p95 (350.719, 350.975] µs, p99 (730.623, 731.135] µs, mean (158.127, 158.238] µs
    - timing bifrost.scribe.write.admission success case none phase none: n 303, p50 (52.895, 52.927] µs, p95 (158.975, 159.103] µs, p99 (217.215, 217.343] µs, mean (67.379, 67.426] µs
    - timing bifrost.scribe.write.admission success case otlp-logs phase repetition-1: n 5989, p50 (94.463, 94.527] µs, p95 (181.759, 181.887] µs, p99 (308.735, 308.991] µs, mean (108.073, 108.148] µs
    - timing bifrost.scribe.write.decode success case none phase none: n 303, p50 (188.543, 188.671] µs, p95 (799.231, 799.743] µs, p99 (1499.135, 1500.159] µs, mean (281.523, 281.718] µs
    - timing bifrost.scribe.write.decode success case otlp-logs phase repetition-1: n 5989, p50 (648.703, 649.215] µs, p95 (1544.191, 1545.215] µs, p99 (2211.839, 2213.887] µs, mean (743.686, 744.21] µs
    - timing bifrost.scribe.write.preprocess success case none phase none: n 303, p50 (254.975, 255.103] µs, p95 (942.591, 943.103] µs, p99 (1330.175, 1331.199] µs, mean (354.824, 355.075] µs
    - timing bifrost.scribe.write.preprocess success case otlp-logs phase repetition-1: n 5989, p50 (2068.479, 2069.503] µs, p95 (3244.031, 3246.079] µs, p99 (4321.279, 4325.375] µs, mean (2174.419, 2175.959] µs
    - timing bifrost.scribe.write.shard_wait success case none phase none: n 303, p50 (10067.967, 10076.159] µs, p95 (65929.215, 65961.983] µs, p99 (98762.751, 98828.287] µs, mean (15804.456, 15815.718] µs
    - timing bifrost.scribe.write.shard_wait success case otlp-logs phase repetition-1: n 5989, p50 (7679.999, 7684.095] µs, p95 (59539.455, 59572.223] µs, p99 (88997.887, 89063.423] µs, mean (13080.235, 13089.555] µs
    - timing bifrost.scribe.write.visibility success case none phase none: n 303, p50 (6979.583, 6983.679] µs, p95 (14475.263, 14483.455] µs, p99 (19185.663, 19202.047] µs, mean (7660.963, 7666.521] µs
    - timing bifrost.scribe.write.visibility success case otlp-logs phase repetition-1: n 5989, p50 (6660.095, 6664.191] µs, p95 (13647.871, 13656.063] µs, p99 (17874.943, 17891.327] µs, mean (7429.645, 7434.979] µs
    - timing bifrost.scribe.write.wal_append success case none phase none: n 303, p50 (3387.391, 3389.439] µs, p95 (6549.503, 6553.599] µs, p99 (14917.631, 14925.823] µs, mean (4271.547, 4274.487] µs
    - timing bifrost.scribe.write.wal_append success case otlp-logs phase repetition-1: n 5989, p50 (3586.047, 3588.095] µs, p95 (11968.511, 11976.703] µs, p99 (15892.479, 15900.671] µs, mean (4554.996, 4558.013] µs
    - timing bifrost.scribe.write.wal_sync success case none phase none: n 303, p50 (8380.415, 8384.511] µs, p95 (24379.391, 24395.775] µs, p99 (48496.639, 48529.407] µs, mean (10478.48, 10485.566] µs
    - timing bifrost.scribe.write.wal_sync success case otlp-logs phase repetition-1: n 5989, p50 (8634.367, 8642.559] µs, p95 (24379.391, 24395.775] µs, p99 (48496.639, 48529.407] µs, mean (10612.786, 10620.087] µs
    - gauge bifrost_gate_active_requests{operation=write}: 0 peak 0
    - gauge bifrost_gate_active_requests{operation=query}: 0 peak 0
    - gauge bifrost_gate_active_streams{operation=query}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=get}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=put}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=list}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=delete}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=head}: 0 peak 0
    - gauge bifrost_resource_planned_bytes{resource=memory,role=server}: 1073741824 peak 1073741824
    - gauge bifrost_resource_planned_bytes{resource=memory,role=bifrost}: 97668386816 peak 97668386816
    - gauge bifrost_resource_planned_bytes{resource=scratch,role=shared}: 377675857920 peak 377675857920
    - counter vala_postgres_pool_acquire_total{pool=runtime}: +212
    - histogram vala_postgres_pool_acquire_seconds{outcome=success,pool=runtime}: n 212, p50 (2077.695, 2078.719] µs, p95 (5279.743, 5283.839] µs, p99 (6832.127, 6836.223] µs, mean (2486.119, 2487.871] µs
    - gauge vala_postgres_pool_size{pool=runtime}: 5 peak 5
    - gauge vala_postgres_pool_idle{pool=runtime}: 3 peak 3
    - gauge wyrd_process_resource_plan{resource=effective_cpu}: 32 peak 32
    - gauge wyrd_process_resource_plan{resource=memory_limit_bytes}: 98742128640 peak 98742128640
    - gauge wyrd_process_resource_plan{resource=managed_memory_bytes}: 97668386816 peak 97668386816
    - gauge wyrd_process_resource_plan{resource=scratch_limit_bytes}: 377675857920 peak 377675857920
    - gauge bifrost_scribe_lane_queued{lane=ingress}: 0 peak 20
    - gauge bifrost_scribe_lane_active{lane=ingress}: 0 peak 10
    - gauge bifrost_scribe_lane_queued{lane=persistence}: 0 peak 4
    - gauge bifrost_scribe_lane_active{lane=persistence}: 3 peak 12
    - gauge bifrost_scribe_lane_queued{lane=wal_io}: 0 peak 1
    - gauge bifrost_scribe_lane_active{lane=wal_io}: 0 peak 1
    - counter bifrost_resource_acquisitions_total{resource=memory,result=acquired,role=scribe}: +19599
    - gauge bifrost_resource_current_bytes{resource=memory,role=scribe}: 2535726320 peak 3630932869
    - gauge bifrost_scribe_persistence_queue_depth: 3 peak 12
    - gauge bifrost_scribe_ingress_active: 0 peak 68
    - gauge bifrost_scribe_staging_live_members: 98 peak 98
    - gauge bifrost_scribe_staging_live_bytes: 876093566 peak 876093566
    - gauge bifrost_scribe_staging_oldest_member_timestamp_seconds: 1795203967 peak 1795203967
    - gauge bifrost_scribe_staging_outstanding_claims: 0 peak 0
    - counter bifrost_resource_acquisitions_total{resource=memory,result=released,role=scribe}: +18840
    - histogram bifrost_scribe_lane_wait_seconds{lane=wal_io}: n 682, p50 (13.223, 13.231] µs, p95 (68.351, 68.415] µs, p99 (624.127, 624.639] µs, mean (37.258, 37.283] µs
    - counter bifrost_scribe_lane_jobs_total{lane=wal_io,status=completed}: +682
    - histogram bifrost_scribe_lane_job_seconds{lane=wal_io}: n 682, p50 (2904.063, 2906.111] µs, p95 (14409.727, 14417.919] µs, p99 (33783.807, 33816.575] µs, mean (4179.033, 4181.874] µs
    - gauge bifrost_scribe_active_memtable_bytes: 87952277 peak 432174538
    - gauge bifrost_scribe_immutable_memtable_bytes: 2108186131 peak 2108186131
    - gauge bifrost_scribe_immutable_generation_count: 20 peak 20
    - gauge bifrost_cluster_roles_live{role=scribe}: 1 peak 1
    - gauge bifrost_cluster_roles_live{role=oracle}: 1 peak 1
    - gauge bifrost_forge_leader_held: 1 peak 1
    - gauge bifrost_forge_worker_ready: 1 peak 1
    - gauge bifrost_forge_worker_restart_backoff_seconds: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=scribe_promotion}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=small_files}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=snapshot_expiry}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=expired_cleanup}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=orphan_cleanup}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=manifest_rewrite}: 0 peak 0
    - gauge oracle_queries_active{class=interactive}: 0 peak 0
    - gauge oracle_queries_queued{class=interactive}: 0 peak 0
    - gauge oracle_queries_active{class=analytical}: 0 peak 0
    - gauge oracle_queries_queued{class=analytical}: 0 peak 0
    - gauge bifrost_oracle_analytical_attempts_active: 0 peak 0
    - gauge wyrd_verification_capability_up{capability=scheduler}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=fitter}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=operator_worker}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=runner}: 1 peak 1
    - gauge bifrost_resource_memory_bytes{kind=managed}: 97668386816 peak 97668386816
    - gauge bifrost_resource_memory_bytes{kind=governed_used}: 2873194601 peak 3295842156
    - gauge bifrost_resource_memory_bytes{kind=scribe_used}: 2849467976 peak 3277244402
    - gauge bifrost_resource_memory_bytes{kind=oracle_used}: 0 peak 0
    - gauge bifrost_resource_memory_bytes{kind=forge_used}: 0 peak 0
    - gauge bifrost_resource_memory_bytes{kind=transport_used}: 23726625 peak 35913336
    - gauge bifrost_resource_memory_bytes{kind=infallible_headroom}: 0 peak 0
    - gauge bifrost_resource_scratch_bytes{kind=total}: 377675857920 peak 377675857920
    - gauge wyrd_workload_ledger_bytes: 2535735859 peak 3664912137
    - gauge wyrd_workload_committed_bytes: 5266604032 peak 5266604032
    - gauge wyrd_workload_pressure_ratio: 0.053 peak 0.053
    - gauge wyrd_workload_attributed_bytes{workload=gateway}: 0 peak 0
    - gauge wyrd_workload_attributed_bytes{workload=verification}: 9539 peak 9539
    - gauge wyrd_workload_attributed_bytes{workload=bifrost}: 2535726320 peak 3664825418
    - gauge bifrost_role_ready{role=scribe}: 1 peak 1
    - gauge bifrost_role_ready{role=oracle}: 1 peak 1
    - gauge bifrost_role_ready{role=peer}: 0 peak 0
    - gauge bifrost_role_ready{role=forge_coordinator}: 1 peak 1
    - gauge bifrost_role_ready{role=forge_worker}: 1 peak 1
    - histogram wyrd_verification_store_duration_seconds{op=runnable_scan,outcome=ok}: n 5, p50 (633.343, 633.855] µs, p95 (859.647, 860.159] µs, p99 (859.647, 860.159] µs, mean (658.841, 659.301] µs
    - histogram wyrd_verification_store_duration_seconds{op=queue_depth,outcome=ok}: n 5, p50 (646.655, 647.167] µs, p95 (869.375, 869.887] µs, p99 (869.375, 869.887] µs, mean (674.918, 675.429] µs
    - gauge wyrd_verification_queue_depth{queue=executions,status=pending}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=executions,status=retrying}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=executions,status=running}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=pending}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=retrying}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=running}: 0 peak 0
    - histogram wyrd_verification_store_duration_seconds{op=due_group_scan,outcome=ok}: n 5, p50 (497.919, 498.175] µs, p95 (860.671, 861.183] µs, p99 (860.671, 861.183] µs, mean (631.654, 632.012] µs
    - histogram wyrd_verification_schedule_pass_seconds{outcome=ok,pass=group}: n 5, p50 (524.287, 524.799] µs, p95 (888.319, 888.831] µs, p99 (888.319, 888.831] µs, mean (653.875, 654.284] µs
    - gauge wyrd_operator_active_dispatches: 0 peak 0
    - histogram wyrd_verification_store_duration_seconds{op=due_scan,outcome=ok}: n 5, p50 (440.063, 440.319] µs, p95 (646.143, 646.655] µs, p99 (646.143, 646.655] µs, mean (508.979, 509.337] µs
    - histogram wyrd_verification_schedule_pass_seconds{outcome=ok,pass=cron}: n 5, p50 (442.367, 442.623] µs, p95 (647.679, 648.191] µs, p99 (647.679, 648.191] µs, mean (511.539, 511.897] µs
    - histogram bifrost_forge_leader_decision_seconds{operation=pull}: n 1, p50 (5.887, 5.891] µs, p95 (5.887, 5.891] µs, p99 (5.887, 5.891] µs, mean (5.887, 5.891] µs
    - gauge outbox_pending{outbox=scribe}: 0 peak 166
    - gauge wyrd_postgres_pool_size{pool=app}: 4 peak 4
    - gauge wyrd_postgres_pool_idle{pool=app}: 0 peak 0
    - histogram wyrd_scribe_outbox_stage_seconds{stage=frames}: n 303, p50 (12.399, 12.407] µs, p95 (25.407, 25.423] µs, p99 (34.815, 34.847] µs, mean (13.601, 13.61] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=permit}: n 303, p50 1.703 µs, p95 (3.815, 3.817] µs, p99 (5.207, 5.211] µs, mean (1.822, 1.822] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=encode_queue}: n 303, p50 (25.135, 25.151] µs, p95 (616.447, 616.959] µs, p99 (1600.511, 1601.535] µs, mean (113.476, 113.558] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=encode}: n 303, p50 (95.359, 95.423] µs, p95 (184.831, 184.959] µs, p99 (251.263, 251.391] µs, mean (106.594, 106.669] µs
    - histogram bifrost_scribe_lane_wait_seconds{lane=ingress}: n 12584, p50 (23.983, 23.999] µs, p95 (653.823, 654.335] µs, p99 (1299.455, 1300.479] µs, mean (138.373, 138.47] µs
    - counter bifrost_scribe_lane_jobs_total{lane=ingress,status=completed}: +12584
    - histogram bifrost_scribe_lane_job_seconds{lane=ingress}: n 12584, p50 (922.111, 922.623] µs, p95 (2398.207, 2400.255] µs, p99 (3131.391, 3133.439] µs, mean (1170.66, 1171.465] µs
    - histogram bifrost_scribe_queue_wait_seconds: n 6292, p50 (25.103, 25.119] µs, p95 (654.335, 654.847] µs, p99 (1337.343, 1338.367] µs, mean (137.954, 138.05] µs
    - counter bifrost_scribe_group_close_total{reason=exhausted}: +203
    - histogram bifrost_scribe_group_size{measure=queued_requests}: n 209, p50 0, p95 0, p99 (3.999, 4.001], mean (0.096, 0.096]
    - histogram bifrost_scribe_group_size{measure=queued_bytes}: n 209, p50 0, p95 0, p99 (4395847.488, 4398046.511], mean (105153.294, 105205.902]
    - histogram bifrost_scribe_group_size{measure=requests}: n 209, p50 (31.994, 32.011], p95 (48.989, 49.023], p99 (63.988, 64.022], mean (30.095, 30.115]
    - histogram bifrost_scribe_group_stage_seconds{stage=rotate}: n 209, p50 (14.951, 14.959] µs, p95 (975.359, 975.871] µs, p99 (8577.023, 8585.215] µs, mean (357.88, 358.147] µs
    - counter bifrost_scribe_wal_append_total{outcome=success}: +12620
    - histogram bifrost_scribe_wal_append_seconds{outcome=success}: n 12620, p50 (2.523, 2.525] µs, p95 (52.511, 52.543] µs, p99 (81.407, 81.471] µs, mean (18.206, 18.218] µs
    - counter bifrost_scribe_wal_append_bytes_total: +777112932
    - histogram bifrost_scribe_group_size{measure=slices}: n 209, p50 (31.994, 32.011], p95 (48.989, 49.023], p99 (63.988, 64.022], mean (30.267, 30.288]
    - histogram bifrost_scribe_group_size{measure=rows}: n 209, p50 (15358.803, 15367.393], p95 (23605.14, 23622.32], p99 (30786.326, 30803.505], mean (14695.185, 14704.779]
    - histogram bifrost_scribe_group_size{measure=logical_bytes}: n 209, p50 (25948474.416, 25966066.602], p95 (39793524.832, 39828709.205], p99 (51932133.203, 51967317.575], mean (24798821.88, 24815777.569]
    - histogram bifrost_scribe_group_size{measure=wal_bytes}: n 209, p50 (3874678.976, 3876878], p95 (5963751.069, 5968149.116], p99 (7771348.185, 7775746.232], mean (3710006.765, 3712464.35]
    - histogram bifrost_scribe_group_stage_seconds{stage=convert}: n 209, p50 (2.123, 2.125] µs, p95 (4.347, 4.351] µs, p99 (5.687, 5.691] µs, mean (7.216, 7.22] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=wal_write}: n 209, p50 (3129.343, 3131.391] µs, p95 (11665.407, 11673.599] µs, p99 (15671.295, 15679.487] µs, mean (3962.829, 3965.472] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=prepare}: n 209, p50 (141.567, 141.695] µs, p95 (278.783, 279.039] µs, p99 (374.015, 374.271] µs, mean (157.833, 157.944] µs
    - counter bifrost_scribe_wal_fsync_total{outcome=success}: +209
    - histogram bifrost_scribe_wal_fsync_seconds{outcome=success}: n 209, p50 (7503.871, 7507.967] µs, p95 (20398.079, 20414.463] µs, p99 (37158.911, 37191.679] µs, mean (9421.471, 9427.933] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=fence_conn}: n 209, p50 (2082.815, 2083.839] µs, p95 (5238.783, 5242.879] µs, p99 (6557.695, 6561.791] µs, mean (2380.071, 2381.765] µs
    - counter bifrost_scribe_memtable_rows_inserted_total: +3072357
    - counter bifrost_scribe_memtable_bytes_inserted_total: +5790501454
    - histogram bifrost_scribe_group_stage_seconds{stage=publish}: n 209, p50 (38.591, 38.623] µs, p95 (76.735, 76.799] µs, p99 (138.111, 138.239] µs, mean (51.314, 51.35] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=commit}: n 209, p50 (79.103, 79.167] µs, p95 (220.543, 220.671] µs, p99 (1400.831, 1401.855] µs, mean (118.05, 118.135] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=sync}: n 209, p50 (7553.023, 7557.119] µs, p95 (20430.847, 20447.231] µs, p99 (37224.447, 37257.215] µs, mean (9539.163, 9545.722] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=fence}: n 209, p50 (5226.495, 5230.591] µs, p95 (12304.383, 12312.575] µs, p99 (15654.911, 15663.103] µs, mean (6002.654, 6006.861] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=insert}: n 209, p50 (1488.895, 1489.919] µs, p95 (2535.423, 2537.471] µs, p99 (4042.751, 4044.799] µs, mean (1576.407, 1577.508] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=ack}: n 209, p50 (69.375, 69.439] µs, p95 (331.007, 331.263] µs, p99 (556.031, 556.543] µs, mean (103.845, 103.918] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=cycle}: n 209, p50 (20365.311, 20381.695] µs, p95 (39157.759, 39190.527] µs, p99 (59408.383, 59441.151] µs, mean (21914.276, 21930.277] µs
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.admission}: +667896038
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.decode}: +4540849422
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_commit}: +718406544
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_fence}: +35901003965
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_insert}: +10933303112
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_sync}: +66757103024
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_write}: +27864749067
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.preprocess}: +13134703093
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.shard_wait}: +83155764960
    - counter scribe_write_profiles_total: +6292
    - histogram wyrd_scribe_outbox_stage_seconds{stage=submit}: n 303, p50 (31391.743, 31408.127] µs, p95 (101384.191, 101449.727] µs, p99 (133431.295, 133496.831] µs, mean (39178.902, 39205.917] µs
    - histogram outbox_write_seconds{outbox=scribe}: n 303, p50 (32227.327, 32243.711] µs, p95 (101580.799, 101646.335] µs, p99 (133693.439, 133758.975] µs, mean (39851.383, 39878.939] µs
    - counter outbox_items_written_total{outbox=scribe}: +5989
    - counter outbox_bytes_written_total{outbox=scribe}: +6797515
    - gauge wyrd_process_reaped_children_cpu_seconds: 0 peak 0
    - counter bifrost_resource_acquisitions_total{resource=memory,result=acquired,role=transport}: +11978
    - gauge bifrost_resource_current_bytes{resource=memory,role=transport}: 0 peak 41049086
    - histogram bifrost_gate_resolution_seconds{stage=auth}: n 5989, p50 (20.543, 20.559] µs, p95 (61.535, 61.567] µs, p99 (104.255, 104.319] µs, mean (29.591, 29.611] µs
    - counter bifrost_resource_acquisitions_total{resource=memory,result=released,role=transport}: +11978
    - histogram bifrost_scribe_persist_stage_seconds{stage=resolve_recipe}: n 58, p50 (9.991, 9.999] µs, p95 (31.951, 31.967] µs, p99 (42.655, 42.687] µs, mean (14.179, 14.188] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=encode}: n 58, p50 (351535.103, 351797.247] µs, p95 (505151.487, 505413.631] µs, p99 (510918.655, 511180.799] µs, mean (275285.38, 275464.262] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=sync_runs}: n 58, p50 (22052.863, 22069.247] µs, p95 (74579.967, 74645.503] µs, p99 (128909.311, 128974.847] µs, mean (32215.605, 32237.638] µs
    - counter bifrost_scribe_lane_jobs_total{lane=persistence,status=completed}: +58
    - histogram bifrost_scribe_lane_job_seconds{lane=persistence}: n 58, p50 (365690.879, 365953.023] µs, p95 (527695.871, 527958.015] µs, p99 (535298.047, 535560.191] µs, mean (307910.303, 308096.317] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=stage_member}: n 58, p50 (365690.879, 365953.023] µs, p95 (527695.871, 527958.015] µs, p99 (535560.191, 535822.335] µs, mean (308034.313, 308220.327] µs
    - counter bifrost_scribe_persistence_raw_bytes_total: +5737689383
    - counter bifrost_scribe_persistence_encoded_bytes_total: +532342538
    - counter bifrost_scribe_stage_fsyncs_total: +209
    - histogram bifrost_scribe_persist_stage_seconds{stage=register_member}: n 58, p50 (24215.551, 24231.935] µs, p95 (73203.711, 73269.247] µs, p99 (91619.327, 91684.863] µs, mean (31418.933, 31440.754] µs
    - counter bifrost_scribe_persistence_jobs_total{status=published}: +58
    - histogram bifrost_scribe_persistence_publication_seconds: n 58, p50 (436731.903, 436994.047] µs, p95 (557318.143, 557842.431] µs, p99 (570425.343, 570949.631] µs, mean (342549.045, 342779.267] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=completion_send}: n 58, p50 (2.403, 2.405] µs, p95 (6.599, 6.603] µs, p99 (11.415, 11.423] µs, mean (2.803, 2.804] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=visibility_ack}: n 58, p50 (9650.175, 9658.367] µs, p95 (58851.327, 58884.095] µs, p99 (63242.239, 63275.007] µs, mean (12991.038, 12999.816] µs
    - counter bifrost_scribe_retirements_total: +55
    - counter bifrost_scribe_retired_bytes_total: +5382565490
    - counter bifrost_scribe_group_close_total{reason=limit}: +6
    - histogram bifrost_scribe_persist_stage_seconds{stage=predecessor_wait}: n 1, p50 (130088.959, 130154.495] µs, p95 (130088.959, 130154.495] µs, p99 (130088.959, 130154.495] µs, mean (130088.959, 130154.495] µs
  - repetition 2 server measured window (nearest-rank bucket bounds):
    - timing bifrost.otlp.logs success case otlp-logs phase repetition-2: n 5876, p50 (37093.375, 37126.143] µs, p95 (108199.935, 108265.471] µs, p99 (139984.895, 140115.967] µs, mean (45343.009, 45374.932] µs
    - timing bifrost.otlp.logs.decode success case otlp-logs phase repetition-2: n 5876, p50 (595.455, 595.967] µs, p95 (931.839, 932.351] µs, p99 (1205.247, 1206.271] µs, mean (612.836, 613.268] µs
    - timing bifrost.scribe.write success case none phase none: n 302, p50 (33685.503, 33718.271] µs, p95 (103874.559, 103940.095] µs, p99 (131923.967, 131989.503] µs, mean (42006.677, 42034.873] µs
    - timing bifrost.scribe.write success case otlp-logs phase repetition-2: n 5876, p50 (32325.631, 32342.015] µs, p95 (102563.839, 102629.375] µs, p99 (135921.663, 136052.735] µs, mean (40874.111, 40902.05] µs
    - timing bifrost.scribe.write.ack success case none phase none: n 302, p50 (25.183, 25.199] µs, p95 (94.271, 94.335] µs, p99 (300.543, 300.799] µs, mean (38.608, 38.634] µs
    - timing bifrost.scribe.write.ack success case otlp-logs phase repetition-2: n 5876, p50 (147.583, 147.711] µs, p95 (325.375, 325.631] µs, p99 (489.471, 489.727] µs, mean (148.459, 148.564] µs
    - timing bifrost.scribe.write.admission success case none phase none: n 302, p50 (55.775, 55.807] µs, p95 (158.463, 158.591] µs, p99 (284.927, 285.183] µs, mean (70.288, 70.338] µs
    - timing bifrost.scribe.write.admission success case otlp-logs phase repetition-2: n 5876, p50 (93.887, 93.951] µs, p95 (175.103, 175.231] µs, p99 (290.559, 290.815] µs, mean (105.614, 105.687] µs
    - timing bifrost.scribe.write.decode success case none phase none: n 302, p50 (168.191, 168.319] µs, p95 (776.703, 777.215] µs, p99 (1242.111, 1243.135] µs, mean (256.867, 257.042] µs
    - timing bifrost.scribe.write.decode success case otlp-logs phase repetition-2: n 5876, p50 (651.775, 652.287] µs, p95 (1496.063, 1497.087] µs, p99 (2166.783, 2168.831] µs, mean (744.065, 744.585] µs
    - timing bifrost.scribe.write.preprocess success case none phase none: n 302, p50 (251.135, 251.263] µs, p95 (913.407, 913.919] µs, p99 (1467.391, 1468.415] µs, mean (341.401, 341.636] µs
    - timing bifrost.scribe.write.preprocess success case otlp-logs phase repetition-2: n 5876, p50 (2034.687, 2035.711] µs, p95 (3115.007, 3117.055] µs, p99 (3981.311, 3983.359] µs, mean (2124.536, 2126.042] µs
    - timing bifrost.scribe.write.shard_wait success case none phase none: n 302, p50 (9601.023, 9609.215] µs, p95 (78512.127, 78577.663] µs, p99 (97910.783, 97976.319] µs, mean (17680.102, 17692.749] µs
    - timing bifrost.scribe.write.shard_wait success case otlp-logs phase repetition-2: n 5876, p50 (7405.567, 7409.663] µs, p95 (69009.407, 69074.943] µs, p99 (104202.239, 104267.775] µs, mean (14105.18, 14115.236] µs
    - timing bifrost.scribe.write.visibility success case none phase none: n 302, p50 (5849.087, 5853.183] µs, p95 (13271.039, 13279.231] µs, p99 (23805.951, 23822.335] µs, mean (7067.322, 7072.37] µs
    - timing bifrost.scribe.write.visibility success case otlp-logs phase repetition-2: n 5876, p50 (5955.583, 5959.679] µs, p95 (12230.655, 12238.847] µs, p99 (16015.359, 16023.551] µs, mean (6733.397, 6738.256] µs
    - timing bifrost.scribe.write.wal_append success case none phase none: n 302, p50 (3391.487, 3393.535] µs, p95 (5693.439, 5697.535] µs, p99 (16613.375, 16621.567] µs, mean (4198.172, 4200.949] µs
    - timing bifrost.scribe.write.wal_append success case otlp-logs phase repetition-2: n 5876, p50 (3506.175, 3508.223] µs, p95 (10838.015, 10846.207] µs, p99 (52920.319, 52953.087] µs, mean (4664.093, 4667.185] µs
    - timing bifrost.scribe.write.wal_sync success case none phase none: n 302, p50 (10608.639, 10616.831] µs, p95 (28852.223, 28868.607] µs, p99 (37388.287, 37421.055] µs, mean (11712.882, 11721.161] µs
    - timing bifrost.scribe.write.wal_sync success case otlp-logs phase repetition-2: n 5876, p50 (10567.679, 10575.871] µs, p95 (25821.183, 25837.567] µs, p99 (43941.887, 43974.655] µs, mean (11602.689, 11611.047] µs
    - gauge bifrost_gate_active_requests{operation=write}: 0 peak 0
    - gauge bifrost_gate_active_requests{operation=query}: 0 peak 0
    - gauge bifrost_gate_active_streams{operation=query}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=get}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=put}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=list}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=delete}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=head}: 0 peak 0
    - gauge bifrost_resource_planned_bytes{resource=memory,role=server}: 1073741824 peak 1073741824
    - gauge bifrost_resource_planned_bytes{resource=memory,role=bifrost}: 97668386816 peak 97668386816
    - gauge bifrost_resource_planned_bytes{resource=scratch,role=shared}: 377675857920 peak 377675857920
    - counter vala_postgres_pool_acquire_total{pool=runtime}: +202
    - histogram vala_postgres_pool_acquire_seconds{outcome=success,pool=runtime}: n 202, p50 (1988.607, 1989.631] µs, p95 (4599.807, 4603.903] µs, p99 (6238.207, 6242.303] µs, mean (2288.379, 2289.986] µs
    - gauge vala_postgres_pool_size{pool=runtime}: 5 peak 5
    - gauge vala_postgres_pool_idle{pool=runtime}: 3 peak 3
    - gauge wyrd_process_resource_plan{resource=effective_cpu}: 32 peak 32
    - gauge wyrd_process_resource_plan{resource=memory_limit_bytes}: 98742128640 peak 98742128640
    - gauge wyrd_process_resource_plan{resource=managed_memory_bytes}: 97668386816 peak 97668386816
    - gauge wyrd_process_resource_plan{resource=scratch_limit_bytes}: 377675857920 peak 377675857920
    - gauge bifrost_scribe_lane_queued{lane=ingress}: 0 peak 20
    - gauge bifrost_scribe_lane_active{lane=ingress}: 0 peak 10
    - gauge bifrost_scribe_lane_queued{lane=persistence}: 0 peak 5
    - gauge bifrost_scribe_lane_active{lane=persistence}: 3 peak 12
    - gauge bifrost_scribe_lane_queued{lane=wal_io}: 0 peak 1
    - gauge bifrost_scribe_lane_active{lane=wal_io}: 0 peak 1
    - counter bifrost_resource_acquisitions_total{resource=memory,result=acquired,role=scribe}: +19263
    - gauge bifrost_resource_current_bytes{resource=memory,role=scribe}: 1943356506 peak 3630932869
    - gauge bifrost_scribe_persistence_queue_depth: 3 peak 12
    - gauge bifrost_scribe_ingress_active: 0 peak 68
    - gauge bifrost_scribe_staging_live_members: 157 peak 157
    - gauge bifrost_scribe_staging_live_bytes: 1404496101 peak 1404496101
    - gauge bifrost_scribe_staging_oldest_member_timestamp_seconds: 1795203967 peak 1795203967
    - gauge bifrost_scribe_staging_outstanding_claims: 0 peak 0
    - counter bifrost_resource_acquisitions_total{resource=memory,result=released,role=scribe}: +18515
    - histogram bifrost_scribe_lane_wait_seconds{lane=wal_io}: n 652, p50 (12.743, 12.751] µs, p95 (44.991, 45.023] µs, p99 (304.895, 305.151] µs, mean (23.668, 23.684] µs
    - counter bifrost_scribe_lane_jobs_total{lane=wal_io,status=completed}: +652
    - histogram bifrost_scribe_lane_job_seconds{lane=wal_io}: n 652, p50 (2854.911, 2856.959] µs, p95 (16228.351, 16236.543] µs, p99 (33849.343, 33882.111] µs, mean (4660.064, 4663.365] µs
    - gauge bifrost_scribe_active_memtable_bytes: 398320862 peak 514374949
    - gauge bifrost_scribe_immutable_memtable_bytes: 1514929839 peak 2108186131
    - gauge bifrost_scribe_immutable_generation_count: 13 peak 20
    - gauge bifrost_cluster_roles_live{role=scribe}: 1 peak 1
    - gauge bifrost_cluster_roles_live{role=oracle}: 1 peak 1
    - gauge bifrost_forge_leader_held: 1 peak 1
    - gauge bifrost_forge_worker_ready: 1 peak 1
    - gauge bifrost_forge_worker_restart_backoff_seconds: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=scribe_promotion}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=small_files}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=snapshot_expiry}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=expired_cleanup}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=orphan_cleanup}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=manifest_rewrite}: 0 peak 0
    - gauge oracle_queries_active{class=interactive}: 0 peak 0
    - gauge oracle_queries_queued{class=interactive}: 0 peak 0
    - gauge oracle_queries_active{class=analytical}: 0 peak 0
    - gauge oracle_queries_queued{class=analytical}: 0 peak 0
    - gauge bifrost_oracle_analytical_attempts_active: 0 peak 0
    - gauge wyrd_verification_capability_up{capability=scheduler}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=fitter}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=operator_worker}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=runner}: 1 peak 1
    - gauge bifrost_resource_memory_bytes{kind=managed}: 97668386816 peak 97668386816
    - gauge bifrost_resource_memory_bytes{kind=governed_used}: 2610861527 peak 3295842156
    - gauge bifrost_resource_memory_bytes{kind=scribe_used}: 2577512395 peak 3277244402
    - gauge bifrost_resource_memory_bytes{kind=oracle_used}: 0 peak 0
    - gauge bifrost_resource_memory_bytes{kind=forge_used}: 0 peak 0
    - gauge bifrost_resource_memory_bytes{kind=transport_used}: 33349132 peak 41044426
    - gauge bifrost_resource_memory_bytes{kind=infallible_headroom}: 0 peak 0
    - gauge bifrost_resource_scratch_bytes{kind=total}: 377675857920 peak 377675857920
    - gauge wyrd_workload_ledger_bytes: 1943366045 peak 3664912137
    - gauge wyrd_workload_committed_bytes: 5266735104 peak 5266735104
    - gauge wyrd_workload_pressure_ratio: 0.053 peak 0.053
    - gauge wyrd_workload_attributed_bytes{workload=gateway}: 0 peak 0
    - gauge wyrd_workload_attributed_bytes{workload=verification}: 9539 peak 9539
    - gauge wyrd_workload_attributed_bytes{workload=bifrost}: 1943356506 peak 3664825418
    - gauge bifrost_role_ready{role=scribe}: 1 peak 1
    - gauge bifrost_role_ready{role=oracle}: 1 peak 1
    - gauge bifrost_role_ready{role=peer}: 0 peak 0
    - gauge bifrost_role_ready{role=forge_coordinator}: 1 peak 1
    - gauge bifrost_role_ready{role=forge_worker}: 1 peak 1
    - histogram wyrd_verification_store_duration_seconds{op=runnable_scan,outcome=ok}: n 5, p50 (1296.383, 1297.407] µs, p95 (1464.319, 1465.343] µs, p99 (1464.319, 1465.343] µs, mean (1162.957, 1163.826] µs
    - histogram wyrd_verification_store_duration_seconds{op=queue_depth,outcome=ok}: n 5, p50 (796.671, 797.183] µs, p95 (880.127, 880.639] µs, p99 (880.127, 880.639] µs, mean (726.323, 726.783] µs
    - gauge wyrd_verification_queue_depth{queue=executions,status=pending}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=executions,status=retrying}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=executions,status=running}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=pending}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=retrying}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=running}: 0 peak 0
    - histogram wyrd_verification_store_duration_seconds{op=due_group_scan,outcome=ok}: n 5, p50 (629.247, 629.759] µs, p95 (1036.287, 1036.799] µs, p99 (1036.287, 1036.799] µs, mean (696.883, 697.343] µs
    - histogram wyrd_verification_schedule_pass_seconds{outcome=ok,pass=group}: n 5, p50 (657.919, 658.431] µs, p95 (1059.839, 1060.863] µs, p99 (1059.839, 1060.863] µs, mean (719.001, 719.564] µs
    - gauge wyrd_operator_active_dispatches: 0 peak 0
    - histogram wyrd_verification_store_duration_seconds{op=due_scan,outcome=ok}: n 5, p50 (535.551, 536.063] µs, p95 (559.103, 559.615] µs, p99 (559.103, 559.615] µs, mean (498.841, 499.25] µs
    - histogram wyrd_verification_schedule_pass_seconds{outcome=ok,pass=cron}: n 5, p50 (537.599, 538.111] µs, p95 (562.175, 562.687] µs, p99 (562.175, 562.687] µs, mean (500.941, 501.349] µs
    - histogram bifrost_forge_leader_decision_seconds{operation=pull}: n 1, p50 (3.735, 3.737] µs, p95 (3.735, 3.737] µs, p99 (3.735, 3.737] µs, mean (3.735, 3.737] µs
    - gauge outbox_pending{outbox=scribe}: 0 peak 166
    - gauge wyrd_postgres_pool_size{pool=app}: 4 peak 4
    - gauge wyrd_postgres_pool_idle{pool=app}: 0 peak 0
    - histogram wyrd_scribe_outbox_stage_seconds{stage=frames}: n 302, p50 (12.095, 12.103] µs, p95 (24.191, 24.207] µs, p99 (30.735, 30.751] µs, mean (13.196, 13.205] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=permit}: n 302, p50 1.644 µs, p95 (3.765, 3.767] µs, p99 (4.959, 4.963] µs, mean (1.756, 1.757] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=encode_queue}: n 302, p50 (24.335, 24.351] µs, p95 (242.815, 242.943] µs, p99 (903.679, 904.191] µs, mean (78.833, 78.889] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=encode}: n 302, p50 (97.023, 97.087] µs, p95 (185.087, 185.215] µs, p99 (224.895, 225.023] µs, mean (102.808, 102.879] µs
    - histogram bifrost_scribe_lane_wait_seconds{lane=ingress}: n 12356, p50 (22.527, 22.543] µs, p95 (626.175, 626.687] µs, p99 (1204.223, 1205.247] µs, mean (128.2, 128.29] µs
    - counter bifrost_scribe_lane_jobs_total{lane=ingress,status=completed}: +12356
    - histogram bifrost_scribe_lane_job_seconds{lane=ingress}: n 12356, p50 (958.463, 958.975] µs, p95 (2373.631, 2375.679] µs, p99 (3024.895, 3026.943] µs, mean (1161.232, 1162.039] µs
    - histogram bifrost_scribe_queue_wait_seconds: n 6178, p50 (22.511, 22.527] µs, p95 (610.303, 610.815] µs, p99 (1215.487, 1216.511] µs, mean (126.403, 126.492] µs
    - counter bifrost_scribe_group_close_total{reason=exhausted}: +194
    - histogram bifrost_scribe_group_size{measure=queued_requests}: n 199, p50 0, p95 0, p99 (3.999, 4.001], mean (0.1, 0.101]
    - histogram bifrost_scribe_group_size{measure=queued_bytes}: n 199, p50 0, p95 0, p99 (4395847.488, 4398046.511], mean (110437.379, 110492.631]
    - histogram bifrost_scribe_group_size{measure=requests}: n 199, p50 (31.994, 32.011], p95 (39.997, 40.03], p99 (63.988, 64.022], mean (31.034, 31.055]
    - histogram bifrost_scribe_group_stage_seconds{stage=rotate}: n 199, p50 (15.503, 15.511] µs, p95 (4796.415, 4800.511] µs, p99 (10141.695, 10149.887] µs, mean (564.722, 565.105] µs
    - counter bifrost_scribe_wal_append_total{outcome=success}: +12392
    - histogram bifrost_scribe_wal_append_seconds{outcome=success}: n 12392, p50 (2.543, 2.545] µs, p95 (47.039, 47.071] µs, p99 (71.871, 71.935] µs, mean (14.349, 14.359] µs
    - counter bifrost_scribe_wal_append_bytes_total: +762490906
    - histogram bifrost_scribe_group_size{measure=slices}: n 199, p50 (31.994, 32.011], p95 (39.997, 40.03], p99 (63.988, 64.022], mean (31.215, 31.237]
    - histogram bifrost_scribe_group_size{measure=rows}: n 199, p50 (15865.609, 15874.199], p95 (19980.188, 19997.368], p99 (30803.505, 30820.685], mean (15142.116, 15152.263]
    - histogram bifrost_scribe_group_size{measure=logical_bytes}: n 199, p50 (26810491.532, 26828083.718], p95 (33741812.833, 33759405.019], p99 (51932133.203, 51967317.575], mean (25553222.431, 25569980.342]
    - histogram bifrost_scribe_group_size{measure=wal_bytes}: n 199, p50 (3995625.255, 3997824.279], p95 (5035763.255, 5040161.302], p99 (7771348.185, 7775746.232], mean (3823080.124, 3825678.061]
    - histogram bifrost_scribe_group_stage_seconds{stage=convert}: n 199, p50 2.004 µs, p95 (3.765, 3.767] µs, p99 (6.151, 6.155] µs, mean (2.241, 2.242] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=wal_write}: n 199, p50 (3115.007, 3117.055] µs, p95 (10657.791, 10665.983] µs, p99 (52756.479, 52789.247] µs, mean (4064.644, 4067.426] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=prepare}: n 199, p50 (154.495, 154.623] µs, p95 (318.463, 318.719] µs, p99 (963.071, 963.583] µs, mean (180.58, 180.71] µs
    - counter bifrost_scribe_wal_fsync_total{outcome=success}: +199
    - histogram bifrost_scribe_wal_fsync_seconds{outcome=success}: n 199, p50 (10002.431, 10010.623] µs, p95 (25772.031, 25788.415] µs, p99 (43909.119, 43941.887] µs, mean (10724.686, 10732.389] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=fence_conn}: n 199, p50 (2008.063, 2009.087] µs, p95 (4681.727, 4685.823] µs, p99 (7127.039, 7131.135] µs, mean (2317.288, 2318.92] µs
    - counter bifrost_scribe_memtable_rows_inserted_total: +3014388
    - counter bifrost_scribe_memtable_bytes_inserted_total: +5681266503
    - histogram bifrost_scribe_group_stage_seconds{stage=publish}: n 199, p50 (39.711, 39.743] µs, p95 (74.495, 74.559] µs, p99 (117.439, 117.503] µs, mean (48.309, 48.342] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=commit}: n 199, p50 (81.023, 81.087] µs, p95 (191.359, 191.487] µs, p99 (682.495, 683.007] µs, mean (99.312, 99.382] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=sync}: n 199, p50 (10297.343, 10305.535] µs, p95 (25821.183, 25837.567] µs, p99 (43941.887, 43974.655] µs, mean (10861.959, 10869.774] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=fence}: n 199, p50 (4632.575, 4636.671] µs, p95 (10977.279, 10985.471] µs, p99 (14262.271, 14270.463] µs, mean (5374.847, 5378.685] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=insert}: n 199, p50 (1483.775, 1484.799] µs, p95 (2260.991, 2263.039] µs, p99 (2742.271, 2744.319] µs, mean (1488.105, 1489.157] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=ack}: n 199, p50 (71.103, 71.167] µs, p95 (290.303, 290.559] µs, p99 (521.471, 521.727] µs, mean (91.082, 91.145] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=cycle}: n 199, p50 (20938.751, 20955.135] µs, p95 (40108.031, 40140.799] µs, p99 (86048.767, 86114.303] µs, mean (22813.248, 22829.631] µs
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.admission}: +642036835
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.decode}: +4451247500
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_commit}: +603127909
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_fence}: +31913284620
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_insert}: +9801469862
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_sync}: +71740364319
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_write}: +28081024908
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.preprocess}: +12591287547
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.shard_wait}: +88252934993
    - counter scribe_write_profiles_total: +6178
    - histogram wyrd_scribe_outbox_stage_seconds{stage=submit}: n 302, p50 (33718.271, 33751.039] µs, p95 (103874.559, 103940.095] µs, p99 (131989.503, 132055.039] µs, mean (42056.168, 42084.419] µs
    - histogram outbox_write_seconds{outbox=scribe}: n 302, p50 (34045.951, 34078.719] µs, p95 (104267.775, 104333.311] µs, p99 (132186.111, 132251.647] µs, mean (42557.413, 42586.206] µs
    - counter outbox_items_written_total{outbox=scribe}: +5876
    - counter outbox_bytes_written_total{outbox=scribe}: +6669260
    - gauge wyrd_process_reaped_children_cpu_seconds: 0 peak 0
    - counter bifrost_resource_acquisitions_total{resource=memory,result=acquired,role=transport}: +11752
    - gauge bifrost_resource_current_bytes{resource=memory,role=transport}: 0 peak 41049086
    - histogram bifrost_gate_resolution_seconds{stage=auth}: n 5876, p50 (20.463, 20.479] µs, p95 (57.919, 57.951] µs, p99 (91.519, 91.583] µs, mean (27.456, 27.474] µs
    - counter bifrost_resource_acquisitions_total{resource=memory,result=released,role=transport}: +11752
    - histogram bifrost_scribe_persist_stage_seconds{stage=resolve_recipe}: n 59, p50 (11.319, 11.327] µs, p95 (27.439, 27.455] µs, p99 (47.391, 47.423] µs, mean (13.966, 13.975] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=encode}: n 59, p50 (329252.863, 329515.007] µs, p95 (469499.903, 469762.047] µs, p99 (471597.055, 471859.199] µs, mean (259224.871, 259396.659] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=sync_runs}: n 59, p50 (32194.559, 32210.943] µs, p95 (79167.487, 79233.023] µs, p99 (96403.455, 96468.991] µs, mean (38280.661, 38307.457] µs
    - counter bifrost_scribe_lane_jobs_total{lane=persistence,status=completed}: +59
    - histogram bifrost_scribe_lane_job_seconds{lane=persistence}: n 59, p50 (354942.975, 355205.119] µs, p95 (548929.535, 549453.823] µs, p99 (568328.191, 568852.479] µs, mean (297900.136, 298095.077] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=stage_member}: n 59, p50 (355205.119, 355467.263] µs, p95 (548929.535, 549453.823] µs, p99 (568328.191, 568852.479] µs, mean (298009.27, 298204.211] µs
    - counter bifrost_scribe_persistence_raw_bytes_total: +5695827021
    - counter bifrost_scribe_persistence_encoded_bytes_total: +528402535
    - counter bifrost_scribe_stage_fsyncs_total: +214
    - histogram bifrost_scribe_persist_stage_seconds{stage=register_member}: n 59, p50 (25591.807, 25608.191] µs, p95 (69533.695, 69599.231] µs, p99 (99614.719, 99680.255] µs, mean (31831.127, 31853.688] µs
    - counter bifrost_scribe_persistence_jobs_total{status=published}: +59
    - histogram bifrost_scribe_persistence_publication_seconds: n 59, p50 (394788.863, 395051.007] µs, p95 (596639.743, 597164.031] µs, p99 (614465.535, 614989.823] µs, mean (331411.664, 331630.487] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=completion_send}: n 59, p50 (2.083, 2.085] µs, p95 (5.339, 5.343] µs, p99 (6.039, 6.043] µs, mean (2.339, 2.34] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=visibility_ack}: n 59, p50 (12378.111, 12386.303] µs, p95 (60030.975, 60063.743] µs, p99 (88866.815, 88932.351] µs, mean (16988.966, 17000.929] µs
    - counter bifrost_scribe_retirements_total: +55
    - counter bifrost_scribe_retired_bytes_total: +5519023361
    - counter bifrost_scribe_group_close_total{reason=limit}: +5
    - histogram bifrost_scribe_persist_stage_seconds{stage=predecessor_wait}: n 1, p50 (43057.151, 43089.919] µs, p95 (43057.151, 43089.919] µs, p99 (43057.151, 43089.919] µs, mean (43057.151, 43089.919] µs
    - counter bifrost_forge_leader_renewals_total{outcome=ok}: +1
    - histogram bifrost_forge_leader_renewal_seconds{outcome=ok}: n 1, p50 (5640.191, 5644.287] µs, p95 (5640.191, 5644.287] µs, p99 (5640.191, 5644.287] µs, mean (5640.191, 5644.287] µs
  - repetition 3 server measured window (nearest-rank bucket bounds):
    - timing bifrost.otlp.logs success case otlp-logs phase repetition-3: n 5669, p50 (38207.487, 38240.255] µs, p95 (113311.743, 113377.279] µs, p99 (162922.495, 163053.567] µs, mean (47595.815, 47629.642] µs
    - timing bifrost.otlp.logs.decode success case otlp-logs phase repetition-3: n 5669, p50 (596.991, 597.503] µs, p95 (944.127, 944.639] µs, p99 (1448.959, 1449.983] µs, mean (619.983, 620.42] µs
    - timing bifrost.scribe.write success case none phase none: n 296, p50 (35061.759, 35094.527] µs, p95 (111214.591, 111280.127] µs, p99 (181665.791, 181796.863] µs, mean (44695.497, 44727.323] µs
    - timing bifrost.scribe.write success case otlp-logs phase repetition-3: n 5669, p50 (33439.743, 33456.127] µs, p95 (108855.295, 108920.831] µs, p99 (157417.471, 157548.543] µs, mean (43067.432, 43097.081] µs
    - timing bifrost.scribe.write.ack success case none phase none: n 296, p50 (25.855, 25.871] µs, p95 (83.071, 83.135] µs, p99 (313.343, 313.599] µs, mean (36.267, 36.292] µs
    - timing bifrost.scribe.write.ack success case otlp-logs phase repetition-3: n 5669, p50 (142.719, 142.847] µs, p95 (283.903, 284.159] µs, p99 (406.015, 406.271] µs, mean (148.249, 148.354] µs
    - timing bifrost.scribe.write.admission success case none phase none: n 296, p50 (58.463, 58.495] µs, p95 (160.127, 160.255] µs, p99 (213.247, 213.375] µs, mean (71.607, 71.656] µs
    - timing bifrost.scribe.write.admission success case otlp-logs phase repetition-3: n 5669, p50 (94.591, 94.655] µs, p95 (182.655, 182.783] µs, p99 (298.751, 299.007] µs, mean (106.383, 106.457] µs
    - timing bifrost.scribe.write.decode success case none phase none: n 296, p50 (183.679, 183.807] µs, p95 (774.143, 774.655] µs, p99 (1265.663, 1266.687] µs, mean (259.88, 260.059] µs
    - timing bifrost.scribe.write.decode success case otlp-logs phase repetition-3: n 5669, p50 (687.615, 688.127] µs, p95 (1543.167, 1544.191] µs, p99 (2199.551, 2201.599] µs, mean (778.173, 778.719] µs
    - timing bifrost.scribe.write.preprocess success case none phase none: n 296, p50 (244.863, 244.991] µs, p95 (1038.335, 1038.847] µs, p99 (1490.943, 1491.967] µs, mean (366.032, 366.284] µs
    - timing bifrost.scribe.write.preprocess success case otlp-logs phase repetition-3: n 5669, p50 (2080.767, 2081.791] µs, p95 (3213.311, 3215.359] µs, p99 (4091.903, 4093.951] µs, mean (2168.101, 2169.644] µs
    - timing bifrost.scribe.write.shard_wait success case none phase none: n 296, p50 (11288.575, 11296.767] µs, p95 (66322.431, 66355.199] µs, p99 (149291.007, 149422.079] µs, mean (17783.46, 17796.089] µs
    - timing bifrost.scribe.write.shard_wait success case otlp-logs phase repetition-3: n 5669, p50 (7299.071, 7303.167] µs, p95 (64716.799, 64749.567] µs, p99 (116785.151, 116850.687] µs, mean (14129.968, 14139.784] µs
    - timing bifrost.scribe.write.visibility success case none phase none: n 296, p50 (5521.407, 5525.503] µs, p95 (11411.455, 11419.647] µs, p99 (13492.223, 13500.415] µs, mean (6308.573, 6313.18] µs
    - timing bifrost.scribe.write.visibility success case otlp-logs phase repetition-3: n 5669, p50 (5672.959, 5677.055] µs, p95 (10854.399, 10862.591] µs, p99 (13975.551, 13983.743] µs, mean (6359.658, 6364.248] µs
    - timing bifrost.scribe.write.wal_append success case none phase none: n 296, p50 (3424.255, 3426.303] µs, p95 (13058.047, 13066.239] µs, p99 (30228.479, 30244.863] µs, mean (4886.998, 4890.36] µs
    - timing bifrost.scribe.write.wal_append success case otlp-logs phase repetition-3: n 5669, p50 (3424.255, 3426.303] µs, p95 (12623.871, 12632.063] µs, p99 (30507.007, 30523.391] µs, mean (4678.859, 4682.036] µs
    - timing bifrost.scribe.write.wal_sync success case none phase none: n 296, p50 (11190.271, 11198.463] µs, p95 (30457.855, 30474.239] µs, p99 (32440.319, 32456.703] µs, mean (13321.562, 13330.694] µs
    - timing bifrost.scribe.write.wal_sync success case otlp-logs phase repetition-3: n 5669, p50 (11468.799, 11476.991] µs, p95 (29212.671, 29229.055] µs, p99 (44761.087, 44793.855] µs, mean (13659.246, 13668.808] µs
    - gauge bifrost_gate_active_requests{operation=write}: 0 peak 0
    - gauge bifrost_gate_active_requests{operation=query}: 0 peak 0
    - gauge bifrost_gate_active_streams{operation=query}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=get}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=put}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=list}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=delete}: 0 peak 0
    - gauge wyrd_storage_operations_active{backend=local,operation=head}: 0 peak 0
    - gauge bifrost_resource_planned_bytes{resource=memory,role=server}: 1073741824 peak 1073741824
    - gauge bifrost_resource_planned_bytes{resource=memory,role=bifrost}: 97668386816 peak 97668386816
    - gauge bifrost_resource_planned_bytes{resource=scratch,role=shared}: 377675857920 peak 377675857920
    - counter vala_postgres_pool_acquire_total{pool=runtime}: +190
    - histogram vala_postgres_pool_acquire_seconds{outcome=success,pool=runtime}: n 190, p50 (1735.679, 1736.703] µs, p95 (4939.775, 4943.871] µs, p99 (7434.239, 7438.335] µs, mean (2272.978, 2274.601] µs
    - gauge vala_postgres_pool_size{pool=runtime}: 5 peak 5
    - gauge vala_postgres_pool_idle{pool=runtime}: 3 peak 3
    - gauge wyrd_process_resource_plan{resource=effective_cpu}: 32 peak 32
    - gauge wyrd_process_resource_plan{resource=memory_limit_bytes}: 98742128640 peak 98742128640
    - gauge wyrd_process_resource_plan{resource=managed_memory_bytes}: 97668386816 peak 97668386816
    - gauge wyrd_process_resource_plan{resource=scratch_limit_bytes}: 377675857920 peak 377675857920
    - gauge bifrost_scribe_lane_queued{lane=ingress}: 0 peak 20
    - gauge bifrost_scribe_lane_active{lane=ingress}: 0 peak 10
    - gauge bifrost_scribe_lane_queued{lane=persistence}: 0 peak 5
    - gauge bifrost_scribe_lane_active{lane=persistence}: 4 peak 12
    - gauge bifrost_scribe_lane_queued{lane=wal_io}: 0 peak 1
    - gauge bifrost_scribe_lane_active{lane=wal_io}: 0 peak 1
    - counter bifrost_resource_acquisitions_total{resource=memory,result=acquired,role=scribe}: +18604
    - gauge bifrost_resource_current_bytes{resource=memory,role=scribe}: 2211290007 peak 3630932869
    - gauge bifrost_scribe_persistence_queue_depth: 8 peak 12
    - gauge bifrost_scribe_ingress_active: 1 peak 68
    - gauge bifrost_scribe_staging_live_members: 210 peak 210
    - gauge bifrost_scribe_staging_live_bytes: 1890639310 peak 1890639310
    - gauge bifrost_scribe_staging_oldest_member_timestamp_seconds: 1795203967 peak 1795203967
    - gauge bifrost_scribe_staging_outstanding_claims: 0 peak 0
    - counter bifrost_resource_acquisitions_total{resource=memory,result=released,role=scribe}: +17867
    - histogram bifrost_scribe_lane_wait_seconds{lane=wal_io}: n 620, p50 (12.799, 12.807] µs, p95 (31.375, 31.391] µs, p99 (309.247, 309.503] µs, mean (21.48, 21.494] µs
    - counter bifrost_scribe_lane_jobs_total{lane=wal_io,status=completed}: +620
    - histogram bifrost_scribe_lane_job_seconds{lane=wal_io}: n 620, p50 (2885.631, 2887.679] µs, p95 (16605.183, 16613.375] µs, p99 (30916.607, 30932.991] µs, mean (5173.164, 5176.815] µs
    - gauge bifrost_scribe_active_memtable_bytes: 515348678 peak 523085433
    - gauge bifrost_scribe_immutable_memtable_bytes: 1365477002 peak 2108186131
    - gauge bifrost_scribe_immutable_generation_count: 13 peak 20
    - gauge bifrost_cluster_roles_live{role=scribe}: 1 peak 1
    - gauge bifrost_cluster_roles_live{role=oracle}: 1 peak 1
    - gauge bifrost_forge_leader_held: 1 peak 1
    - gauge bifrost_forge_worker_ready: 1 peak 1
    - gauge bifrost_forge_worker_restart_backoff_seconds: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=scribe_promotion}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=small_files}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=snapshot_expiry}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=expired_cleanup}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=orphan_cleanup}: 0 peak 0
    - gauge bifrost_forge_active_tasks{task_type=manifest_rewrite}: 0 peak 0
    - gauge oracle_queries_active{class=interactive}: 0 peak 0
    - gauge oracle_queries_queued{class=interactive}: 0 peak 0
    - gauge oracle_queries_active{class=analytical}: 0 peak 0
    - gauge oracle_queries_queued{class=analytical}: 0 peak 0
    - gauge bifrost_oracle_analytical_attempts_active: 0 peak 0
    - gauge wyrd_verification_capability_up{capability=scheduler}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=fitter}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=operator_worker}: 1 peak 1
    - gauge wyrd_verification_capability_up{capability=runner}: 1 peak 1
    - gauge bifrost_resource_memory_bytes{kind=managed}: 97668386816 peak 97668386816
    - gauge bifrost_resource_memory_bytes{kind=governed_used}: 2008443102 peak 3295842156
    - gauge bifrost_resource_memory_bytes{kind=scribe_used}: 1987279969 peak 3277244402
    - gauge bifrost_resource_memory_bytes{kind=oracle_used}: 0 peak 0
    - gauge bifrost_resource_memory_bytes{kind=forge_used}: 0 peak 0
    - gauge bifrost_resource_memory_bytes{kind=transport_used}: 21163133 peak 41045917
    - gauge bifrost_resource_memory_bytes{kind=infallible_headroom}: 0 peak 0
    - gauge bifrost_resource_scratch_bytes{kind=total}: 377675857920 peak 377675857920
    - gauge wyrd_workload_ledger_bytes: 2447221622 peak 3664912137
    - gauge wyrd_workload_committed_bytes: 5266735104 peak 5266735104
    - gauge wyrd_workload_pressure_ratio: 0.053 peak 0.053
    - gauge wyrd_workload_attributed_bytes{workload=gateway}: 0 peak 0
    - gauge wyrd_workload_attributed_bytes{workload=verification}: 9539 peak 9539
    - gauge wyrd_workload_attributed_bytes{workload=bifrost}: 2447210948 peak 3664825418
    - gauge bifrost_role_ready{role=scribe}: 1 peak 1
    - gauge bifrost_role_ready{role=oracle}: 1 peak 1
    - gauge bifrost_role_ready{role=peer}: 0 peak 0
    - gauge bifrost_role_ready{role=forge_coordinator}: 1 peak 1
    - gauge bifrost_role_ready{role=forge_worker}: 1 peak 1
    - histogram wyrd_verification_store_duration_seconds{op=runnable_scan,outcome=ok}: n 6, p50 (824.319, 824.831] µs, p95 (1353.727, 1354.751] µs, p99 (1353.727, 1354.751] µs, mean (843.178, 843.775] µs
    - histogram wyrd_verification_store_duration_seconds{op=queue_depth,outcome=ok}: n 5, p50 (569.855, 570.367] µs, p95 (1402.879, 1403.903] µs, p99 (1402.879, 1403.903] µs, mean (785.152, 785.714] µs
    - gauge wyrd_verification_queue_depth{queue=executions,status=pending}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=executions,status=retrying}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=executions,status=running}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=pending}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=retrying}: 0 peak 0
    - gauge wyrd_verification_queue_depth{queue=dispatches,status=running}: 0 peak 0
    - histogram wyrd_verification_store_duration_seconds{op=due_group_scan,outcome=ok}: n 6, p50 (488.959, 489.215] µs, p95 (1330.175, 1331.199] µs, p99 (1330.175, 1331.199] µs, mean (681.045, 681.514] µs
    - histogram wyrd_verification_schedule_pass_seconds{outcome=ok,pass=group}: n 6, p50 (507.647, 507.903] µs, p95 (1364.991, 1366.015] µs, p99 (1364.991, 1366.015] µs, mean (706.688, 707.156] µs
    - gauge wyrd_operator_active_dispatches: 0 peak 0
    - histogram wyrd_verification_store_duration_seconds{op=due_scan,outcome=ok}: n 5, p50 (567.295, 567.807] µs, p95 (709.119, 709.631] µs, p99 (709.119, 709.631] µs, mean (583.117, 583.577] µs
    - histogram wyrd_verification_schedule_pass_seconds{outcome=ok,pass=cron}: n 5, p50 (569.343, 569.855] µs, p95 (712.191, 712.703] µs, p99 (712.191, 712.703] µs, mean (585.216, 585.676] µs
    - histogram bifrost_forge_leader_decision_seconds{operation=pull}: n 1, p50 (4.247, 4.251] µs, p95 (4.247, 4.251] µs, p99 (4.247, 4.251] µs, mean (4.247, 4.251] µs
    - gauge outbox_pending{outbox=scribe}: 1 peak 166
    - gauge wyrd_postgres_pool_size{pool=app}: 4 peak 4
    - gauge wyrd_postgres_pool_idle{pool=app}: 0 peak 0
    - histogram wyrd_scribe_outbox_stage_seconds{stage=frames}: n 297, p50 (11.807, 11.815] µs, p95 (22.447, 22.463] µs, p99 (30.575, 30.591] µs, mean (12.568, 12.576] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=permit}: n 297, p50 1.563 µs, p95 (3.947, 3.949] µs, p99 (7.431, 7.435] µs, mean (1.729, 1.729] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=encode_queue}: n 297, p50 (23.599, 23.615] µs, p95 (164.095, 164.223] µs, p99 (670.207, 670.719] µs, mean (53.365, 53.399] µs
    - histogram wyrd_scribe_outbox_stage_seconds{stage=encode}: n 297, p50 (90.751, 90.815] µs, p95 (165.759, 165.887] µs, p99 (274.943, 275.199] µs, mean (100.31, 100.381] µs
    - histogram bifrost_scribe_lane_wait_seconds{lane=ingress}: n 11932, p50 (25.855, 25.871] µs, p95 (721.919, 722.431] µs, p99 (1215.487, 1216.511] µs, mean (149.412, 149.516] µs
    - counter bifrost_scribe_lane_jobs_total{lane=ingress,status=completed}: +11932
    - histogram bifrost_scribe_lane_job_seconds{lane=ingress}: n 11932, p50 (988.159, 988.671] µs, p95 (2387.967, 2390.015] µs, p99 (3041.279, 3043.327] µs, mean (1174.27, 1175.087] µs
    - histogram bifrost_scribe_queue_wait_seconds: n 5966, p50 (26.015, 26.031] µs, p95 (727.551, 728.063] µs, p99 (1275.903, 1276.927] µs, mean (152.338, 152.444] µs
    - counter bifrost_scribe_group_close_total{reason=exhausted}: +182
    - histogram bifrost_scribe_group_size{measure=queued_requests}: n 187, p50 0, p95 0, p99 (3.999, 4.001], mean (0.107, 0.107]
    - histogram bifrost_scribe_group_size{measure=queued_bytes}: n 187, p50 0, p95 0, p99 (4393648.465, 4395847.488], mean (117488.991, 117547.789]
    - histogram bifrost_scribe_group_size{measure=requests}: n 187, p50 (32.984, 33.001], p95 (40.97, 41.004], p99 (63.988, 64.022], mean (31.887, 31.909]
    - histogram bifrost_scribe_group_stage_seconds{stage=rotate}: n 187, p50 (14.863, 14.871] µs, p95 (754.687, 755.199] µs, p99 (40108.031, 40140.799] µs, mean (860.324, 860.973] µs
    - counter bifrost_scribe_wal_append_total{outcome=success}: +11964
    - histogram bifrost_scribe_wal_append_seconds{outcome=success}: n 11964, p50 (3.015, 3.017] µs, p95 (45.951, 45.983] µs, p99 (73.727, 73.791] µs, mean (17.389, 17.4] µs
    - counter bifrost_scribe_wal_append_bytes_total: +735668437
    - histogram bifrost_scribe_group_size{measure=slices}: n 187, p50 (32.984, 33.001], p95 (41.977, 42.01], p99 (63.988, 64.022], mean (32.068, 32.09]
    - histogram bifrost_scribe_group_size{measure=rows}: n 187, p50 (15925.739, 15934.329], p95 (20976.62, 20993.8], p99 (30803.505, 30820.685], mean (15546.981, 15556.303]
    - histogram bifrost_scribe_group_size{measure=logical_bytes}: n 187, p50 (26828083.718, 26845675.904], p95 (35465847.066, 35483439.252], p99 (51932133.203, 51967317.575], mean (26234759.021, 26251874.948]
    - histogram bifrost_scribe_group_size{measure=wal_bytes}: n 187, p50 (4028610.604, 4030809.627], p95 (5304044.092, 5308442.139], p99 (7771348.185, 7775746.232], mean (3925262.023, 3927648.463]
    - histogram bifrost_scribe_group_stage_seconds{stage=convert}: n 187, p50 (2.083, 2.085] µs, p95 (3.815, 3.817] µs, p99 (5.339, 5.343] µs, mean (2.27, 2.271] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=wal_write}: n 187, p50 (3071.999, 3074.047] µs, p95 (11722.751, 11730.943] µs, p99 (30261.247, 30277.631] µs, mean (4088.795, 4091.611] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=prepare}: n 187, p50 (162.815, 162.943] µs, p95 (356.095, 356.351] µs, p99 (497.407, 497.663] µs, mean (181.636, 181.759] µs
    - counter bifrost_scribe_wal_fsync_total{outcome=success}: +187
    - histogram bifrost_scribe_wal_fsync_seconds{outcome=success}: n 187, p50 (10772.479, 10780.671] µs, p95 (26247.167, 26263.551] µs, p99 (44728.319, 44761.087] µs, mean (12748.822, 12757.878] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=fence_conn}: n 187, p50 (1748.991, 1750.015] µs, p95 (4947.967, 4952.063] µs, p99 (7442.431, 7446.527] µs, mean (2301.332, 2302.972] µs
    - counter bifrost_scribe_memtable_rows_inserted_total: +2908196
    - counter bifrost_scribe_memtable_bytes_inserted_total: +5481246703
    - histogram bifrost_scribe_group_stage_seconds{stage=publish}: n 187, p50 (50.207, 50.239] µs, p95 (82.815, 82.879] µs, p99 (114.239, 114.303] µs, mean (53.585, 53.621] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=commit}: n 187, p50 (80.895, 80.959] µs, p95 (161.535, 161.663] µs, p99 (535.551, 536.063] µs, mean (97.808, 97.878] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=sync}: n 187, p50 (11001.855, 11010.047] µs, p95 (26296.319, 26312.703] µs, p99 (44761.087, 44793.855] µs, mean (12846.83, 12855.886] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=fence}: n 187, p50 (4308.991, 4313.087] µs, p95 (9814.015, 9822.207] µs, p99 (13819.903, 13828.095] µs, mean (5067.387, 5070.946] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=insert}: n 187, p50 (1459.199, 1460.223] µs, p95 (2242.559, 2244.607] µs, p99 (2768.895, 2770.943] µs, mean (1477.526, 1478.578] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=ack}: n 187, p50 (67.455, 67.519] µs, p95 (169.471, 169.599] µs, p99 (282.367, 282.623] µs, mean (90.927, 90.99] µs
    - histogram bifrost_scribe_group_stage_seconds{stage=cycle}: n 187, p50 (21626.879, 21643.263] µs, p95 (43614.207, 43646.975] µs, p99 (89063.423, 89128.959] µs, mean (24801.521, 24819.831] µs
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.admission}: +624497470
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.decode}: +4489959642
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_commit}: +578077360
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_fence}: +28539465070
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_insert}: +9395248938
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_sync}: +81406161310
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.group_write}: +27402203025
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.preprocess}: +12403691477
    - counter scribe_write_profile_nanoseconds_total{event=Scribe.shard_wait}: +85396692263
    - counter scribe_write_profiles_total: +5965
    - histogram wyrd_scribe_outbox_stage_seconds{stage=submit}: n 296, p50 (35061.759, 35094.527] µs, p95 (111280.127, 111345.663] µs, p99 (181665.791, 181796.863] µs, mean (44738.228, 44770.054] µs
    - histogram outbox_write_seconds{outbox=scribe}: n 296, p50 (35782.655, 35815.423] µs, p95 (111869.951, 111935.487] µs, p99 (181796.863, 181927.935] µs, mean (45205.227, 45237.33] µs
    - counter outbox_items_written_total{outbox=scribe}: +5668
    - counter outbox_bytes_written_total{outbox=scribe}: +6433180
    - gauge wyrd_process_reaped_children_cpu_seconds: 0 peak 0
    - counter bifrost_resource_acquisitions_total{resource=memory,result=acquired,role=transport}: +11338
    - gauge bifrost_resource_current_bytes{resource=memory,role=transport}: 0 peak 41049086
    - histogram bifrost_gate_resolution_seconds{stage=auth}: n 5669, p50 (20.863, 20.879] µs, p95 (59.039, 59.071] µs, p99 (103.295, 103.359] µs, mean (30.616, 30.637] µs
    - counter bifrost_resource_acquisitions_total{resource=memory,result=released,role=transport}: +11338
    - histogram bifrost_scribe_persist_stage_seconds{stage=resolve_recipe}: n 58, p50 (11.535, 11.543] µs, p95 (28.207, 28.223] µs, p99 (40.287, 40.319] µs, mean (13.41, 13.418] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=encode}: n 57, p50 (362807.295, 363069.439] µs, p95 (475529.215, 475791.359] µs, p99 (496500.735, 496762.879] µs, mean (273467.212, 273640.645] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=sync_runs}: n 57, p50 (36536.319, 36569.087] µs, p95 (60751.871, 60784.639] µs, p99 (77725.695, 77791.231] µs, mean (37073.543, 37099.699] µs
    - counter bifrost_scribe_lane_jobs_total{lane=persistence,status=completed}: +57
    - histogram bifrost_scribe_lane_job_seconds{lane=persistence}: n 57, p50 (399507.455, 399769.599] µs, p95 (513802.239, 514064.383] µs, p99 (542113.791, 542638.079] µs, mean (311990.452, 312181.885] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=stage_member}: n 57, p50 (399507.455, 399769.599] µs, p95 (513802.239, 514064.383] µs, p99 (542113.791, 542638.079] µs, mean (312102.84, 312294.273] µs
    - counter bifrost_scribe_persistence_raw_bytes_total: +5701985353
    - counter bifrost_scribe_persistence_encoded_bytes_total: +528807891
    - counter bifrost_scribe_stage_fsyncs_total: +198
    - histogram bifrost_scribe_persist_stage_seconds{stage=register_member}: n 53, p50 (24215.551, 24231.935] µs, p95 (52527.103, 52559.871] µs, p99 (78577.663, 78643.199] µs, mean (29016.219, 29037.239] µs
    - counter bifrost_scribe_persistence_jobs_total{status=published}: +53
    - histogram bifrost_scribe_persistence_publication_seconds: n 53, p50 (414449.663, 414711.807] µs, p95 (549453.823, 549978.111] µs, p99 (581435.391, 581959.679] µs, mean (337757.087, 337975.334] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=completion_send}: n 53, p50 (2.633, 2.635] µs, p95 (5.399, 5.403] µs, p99 (6.859, 6.863] µs, mean (2.723, 2.724] µs
    - histogram bifrost_scribe_persist_stage_seconds{stage=visibility_ack}: n 53, p50 (7380.991, 7385.087] µs, p95 (97910.783, 97976.319] µs, p99 (108199.935, 108265.471] µs, mean (17880.196, 17892.824] µs
    - counter bifrost_scribe_retirements_total: +60
    - counter bifrost_scribe_retired_bytes_total: +5801651475
    - counter bifrost_scribe_group_close_total{reason=limit}: +5
    - histogram bifrost_scribe_persist_stage_seconds{stage=predecessor_wait}: n 1, p50 (56918.015, 56950.783] µs, p95 (56918.015, 56950.783] µs, p99 (56918.015, 56950.783] µs, mean (56918.015, 56950.783] µs

verdict: VALID (exit 0)


```
</details>

<details><summary>capacity profile, persistence worker group</summary>

```text
== wyrd-scribe-per: 38314 samples
-- self
   23.1%     8868 fun_1a1640
   18.1%     6973 ZSTD_hash4
   13.3%     5128 fun_198680
    5.6%     2163 FSE_initCState2
    5.0%     1924 from_iter<&arrow_array::array::byte_array::GenericByteArray<arrow_array::types::GenericStringType<i32>>, core::iter::adapters::map::Map<core::slice::iter::Iter<&dyn arrow_array::array::Array>, arrow_select::interleave::{impl#0}::new::{closure_env#0}<arrow_array::array::byte_array::GenericByteArray<arrow_array::types::GenericStringType<i32>>>>>
    2.4%      938 mi_page_start
    1.5%      600 intern<parquet::arrow::arrow_writer::byte_array::ByteArrayStorage>
    1.5%      587 write_gather
    1.4%      571 write<u128>
    1.4%      543 copy_nonoverlapping<u8>
    1.3%      510 alloc
    1.3%      509 HIST_count_parallel_wksp
    1.0%      392 from_iter<&arrow_array::array::byte_array::GenericByteArray<arrow_array::types::GenericBinaryType<i32>>, core::iter::adapters::map::Map<core::slice::iter::Iter<&dyn arrow_array::array::Array>, arrow_select::interleave::{impl#0}::new::{closure_env#0}<arrow_array::array::byte_array::GenericByteArray<arrow_array::types::GenericBinaryType<i32>>>>>
    0.9%      356 _write
    0.9%      354 fun_1a1e80
    0.8%      319 ZSTD_decodeSequence
    0.8%      315 ZSTD_seqToCodes
    0.7%      296 {closure#3}<false, false, arrow_cmp::compare_bytes::{closure_env#0}<arrow_array::types::GenericStringType<i32>>>
    0.6%      267 {closure#0}<false, false, arrow_cmp::compare_bytes::{closure_env#0}<arrow_array::types::GenericStringType<i32>>>
    0.6%      231 syscall
    0.6%      230 {closure#0}
    0.5%      219 {closure#2}<arrow_select::interleave::{impl#0}::new::{closure_env#1}<arrow_array::array::byte_array::GenericByteArray<arrow_array::types::GenericBinaryType<i32>>>>
    0.5%      214 from_iter<&arrow_array::array::primitive_array::PrimitiveArray<arrow_array::types::Int64Type>, core::iter::adapters::map::Map<core::slice::iter::Iter<&dyn arrow_array::array::Array>, arrow_select::interleave::{impl#0}::new::{closure_env#0}<arrow_array::array::primitive_array::PrimitiveArray<arrow_array::types::Int64Type>>>>
    0.5%      193 hash_one<&[u8]>
    0.5%      192 ZSTD_splitBlock
    0.4%      180 fold<u32, u32, core::iter::adapters::map::map_fold::{closure_env#0}<&u32, u32, u32, parquet::bloom_filter::{impl#5}::count_ones::{closure_env#0}, core::iter::traits::accum::{impl#36}::sum::{closure_env#0}<core::iter::adapters::map::Map<core::slice::iter::Iter<u32>, parquet::bloom_filter::{impl#5}::count_ones::{closure_env#0}>>>>
    0.4%      177 intern<parquet::encodings::encoding::dict_encoder::KeyStorage<parquet::data_type::Int64Type>>
    0.4%      176 read
    0.4%      175 _madvise
    0.3%      152 FSE_buildCTable_wksp
    0.3%      118 recordFingerprint_generic
    0.2%      114 set_bits
    0.2%      114 small_sort<usize, core::slice::{impl#0}::sort_unstable_by::{closure_env#0}<usize, arrow_ord::sort::sort_fixed_column::{closure_env#0}<4>>>
    0.2%      111 try_extend
    0.2%      106 to_data<arrow_array::types::GenericStringType<i32>>
    0.2%      101 try_new_with_metadata_at_depth
    0.2%       96 {closure#2}<arrow_select::interleave::{impl#0}::new::{closure_env#1}<arrow_array::array::list_array::GenericListArray<i32>>>
    0.2%       92 write
    0.2%       91 Unwind_Find_FDE
    0.2%       90 {closure#0}<false, true, arrow_cmp::compare_primitive::{closure_env#0}<arrow_array::types::Date64Type>>
-- inclusive
   99.8%    38266 {closure#0}
   99.8%    38261 take_local_job
   99.8%    38261 wait_until_cold
   99.8%    38260 main_loop
   99.8%    38260 run
   99.8%    38260 wait_until<rayon_core::latch::OnceLatch>
   99.8%    38260 wait_until_out_of_work
   99.5%    38144 _rust_begin_short_backtrace<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>
   99.4%    38122 call_once<(), std::thread::lifecycle::spawn_unchecked::{closure#1}::{closure_env#0}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>>
   99.4%    38122 call_once<std::thread::lifecycle::spawn_unchecked::{closure_env#1}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>, ()>
   99.4%    38122 catch_unwind<(), core::panic::unwind_safe::AssertUnwindSafe<std::thread::lifecycle::spawn_unchecked::{closure#1}::{closure_env#0}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>>>
   99.4%    38122 catch_unwind<core::panic::unwind_safe::AssertUnwindSafe<std::thread::lifecycle::spawn_unchecked::{closure#1}::{closure_env#0}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>>, ()>
   99.4%    38122 do_call<core::panic::unwind_safe::AssertUnwindSafe<std::thread::lifecycle::spawn_unchecked::{closure#1}::{closure_env#0}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>>, ()>
   99.4%    38122 {closure#0}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>
   99.4%    38122 {closure#1}<rayon_core::registry::{impl#2}::spawn::{closure_env#0}, ()>
   99.3%    38083 <alloc::boxed::Box<dyn core::ops::function::FnOnce<(), Output = ()> + core::marker::Send> as core::ops::function::FnOnce<()>>::call_once
   99.3%    38083 <std::sys::thread::unix::Thread>::new::thread_start
   99.3%    38048 call_once<(), vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>
   99.3%    38048 catch_unwind<(), core::panic::unwind_safe::AssertUnwindSafe<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>>
   99.3%    38048 catch_unwind<core::panic::unwind_safe::AssertUnwindSafe<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>, ()>
   99.3%    38048 catch_unwind<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>
   99.3%    38048 do_call<core::panic::unwind_safe::AssertUnwindSafe<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>, ()>
   99.3%    38048 execute<rayon_core::spawn::spawn_job::{closure_env#0}<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>>
   99.3%    38048 halt_unwinding<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}, ()>
   99.3%    38048 {closure#0}<vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure_env#0}>
   99.2%    38045 on_lane<core::result::Result<vala_bifrost_redux::scribe::execution_lanes::ScribePersistenceCpuResult, vala_bifrost_redux::contracts::ScribeError>, vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure#0}::{closure#0}::{closure_env#0}>
   99.2%    38044 assemble_claim
   99.2%    38044 execute_persistence_operation
   98.8%    37882 fun_9c7e0
   98.8%    37865 fun_129e85
   98.1%    37618 encode
   88.9%    34093 run_blocking<core::result::Result<vala_bifrost_redux::scribe::execution_lanes::ScribePersistenceCpuResult, vala_bifrost_redux::contracts::ScribeError>, vala_bifrost_redux::scribe::execution_lanes::{impl#3}::submit::{async_fn#0}::{closure#0}::{closure#0}::{closure_env#0}>
   88.9%    34090 schema_fingerprint
   88.9%    34089 encode_runs
   88.9%    34088 encode_member
   87.8%    33669 encode_batch
   87.8%    33669 fold<(usize, arrow_schema::SortOptions), usize, core::iter::adapters::map::map_fold::{closure_env#0}<&(usize, arrow_schema::SortOptions), usize, usize, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}, &mut core::iter::traits::accum::{impl#48}::sum::{closure_env#0}<core::iter::adapters::flatten::FlatMap<core::slice::iter::Iter<arrow_array::record_batch::RecordBatch>, core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}>>>>
   87.8%    33669 fold<arrow_array::record_batch::RecordBatch, usize, core::iter::adapters::map::map_fold::{closure_env#0}<&arrow_array::record_batch::RecordBatch, core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, usize, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}, core::iter::adapters::flatten::{impl#18}::iter_fold::flatten::{closure_env#0}<core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, usize, core::iter::adapters::flatten::{impl#20}::fold::flatten::{closure_env#0}<core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, usize, core::iter::traits::accum::{impl#48}::sum::{closure_env#0}<core::iter::adapters::flatten::FlatMap<core::slice::iter::Iter<arrow_array::record_batch::RecordBatch>, core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}>>>>>>
   87.8%    33669 fold<core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, core::slice::iter::Iter<arrow_array::record_batch::RecordBatch>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}, usize, core::iter::adapters::flatten::{impl#18}::iter_fold::flatten::{closure_env#0}<core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, usize, core::iter::adapters::flatten::{impl#20}::fold::flatten::{closure_env#0}<core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, usize, core::iter::traits::accum::{impl#48}::sum::{closure_env#0}<core::iter::adapters::flatten::FlatMap<core::slice::iter::Iter<arrow_array::record_batch::RecordBatch>, core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}>>>>>
   87.8%    33669 fold<core::iter::adapters::map::Map<core::slice::iter::Iter<arrow_array::record_batch::RecordBatch>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}>, core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, usize, core::iter::traits::accum::{impl#48}::sum::{closure_env#0}<core::iter::adapters::flatten::FlatMap<core::slice::iter::Iter<arrow_array::record_batch::RecordBatch>, core::iter::adapters::map::Map<core::slice::iter::Iter<(usize, arrow_schema::SortOptions)>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure#0}::{closure_env#0}>, vala_bifrost_redux::scribe::parquet_writer::{impl#3}::prepare_sorted_candidate::{closure_env#0}>>>

```
</details>


# Bifrost benchmarking handoff (2026-10-11)

Source branch: `wyrd/bifrost-variant/TASK-003` at `cfe21dc90`.
Integration branch: `wyrd/bifrost-variant/integrate-main`, which is main plus
the merge research in `merge-research/`.

## Plan

1. **Finalize the merge plan.** `merge-research/RESOLUTION-MAP.md` holds the
   human decisions recorded on 2026-10-11 (§2). The `iceberg-compaction` fork
   prerequisite is done: `wyrd/main` at `22cd178` (Arrow 60, no REST catalog /
   Ring), already pinned in `Cargo.toml` here (map §2.5).
2. **Merge main with the current Variant work** on `integrate-main`. Follow map
   §4 for the procedure and §5 for the proof lanes.
3. **Continue Bifrost benchmarking and optimization** on the merged branch.
   Start by proving the merge kept performance: reads (query capacity, hot and
   Parquet) first, then writes. See the sections below.
4. **Benchmark wyrd-gateway.** See the section at the end.

## How this work started: reads

Benchmarking on this branch began with **read latency** (TASK-003,
`tasks/TASK-003-revision-query-latency.md` on `wyrd/bifrost-variant/TASK-003`,
runs 1–28). The goal was ClickHouse-like query latency, with throughput coming
from less CPU per query.

The writes work came later. Run 27 showed OTLP span export at about 44k
spans/s, below the 200k floor. That turned into the OTLP write-capacity
work recorded further down.

**Read work that landed:**

| Change | Commit |
|---|---|
| A `RwLock` on the memtable, with projection outside the lock | `ce3474fa7` |
| One-round-trip read cuts, cached and invalidated through LISTEN/NOTIFY, plus lower per-query planning cost | `ce3474fa7` |
| No pod-wide lock on governed memory | `2f9e2889f` |
| One `ScanSizer` sizing partitions from scanned bytes, for every tier | `1190c539a` |
| Memtable column bytes recorded once at append | `05f2ba96b` |
| The OTLP telemetry (spans) read fixture | `179bfdc11` |

**Latest committed read numbers:**

- **Run 24, hot tier.** See `benchmarks/pre-merge/read-standard-hot.md`.

  | Case | Result |
  |---|---|
  | selective x1 | 633 qps, p50 1.5 ms |
  | selective peak | 6,359 qps at x32 |
  | small-aggregate x1 | 357 qps, p50 2.7 ms |
  | small-aggregate x64 | 3,673 qps |
  | 1m-aggregate x8 | 872 qps |
  | small-aggregate while writing | 953 qps |

- **Runs 25/26, Parquet tier, 24 vs 12 MiB chunk.** See the task file. The
  24 MiB chunk stayed the default.

See "Open optimization opportunities" below for what is still open.

## First: prove the merge kept performance

Before any new optimization, show that the merged branch is at least as fast
as pre-merge `wyrd/bifrost-variant/TASK-003` at `cfe21dc90`. Run everything on
the same host with nothing compiling, in this order:

1. **The pre-merge read baseline is recorded.** The standard and telemetry
   fixtures, hot and Parquet tiers, were run on a clean `cfe21dc90` tree on
   2026-10-11. See `benchmarks/pre-merge/`.
2. **Run the same read sweeps on the merged branch** and compare them case by
   case. Use qps, p50/p95/p99, server CPU per query and peak memory, plus the
   per-query latency Pareto and the CPU Pareto by crate.
3. **Writes, only after reads match.** Each one has a pre-merge baseline in
   `benchmarks/pre-merge/`:
   - `mise run bench:bifrost:write-capacity -- --fixture logs`
     (`write-logs-run-37.md`)
   - then `--fixture telemetry` (`write-telemetry.md`)
   - then the native sweep, with no fixture (`write-native.md`)
   - then `mise run bench:bifrost:client-queue` (`client-queue.txt`)

Repeat a case before calling it a regression. On this disk, fsync time varies
by up to about 2.5x between runs (run-36 vs run-37), and write rows/s varied
25% between runs 23 and 25 on the same code.

If the merged branch is worse, diagnose it against the
`wyrd-bifrost-variant-task-003` worktree:

- That worktree is kept at `cfe21dc90` as the reference. Don't delete it until
  parity is proven.
- Diff the latency and CPU Paretos and the profiles under
  `target/bifrost-query-capacity/<mode>/<tier>/profiles/` between the two
  worktrees.
- Trace the regressing stage with `WYRD_LOG` before changing any code.

## Write-capacity target and harness

- **Target:** at least 30 MiB/s of OTLP wire per server core, and at least
  200k rows/s at the fastest step. The bench judges both.
- **Write sweep:**
  `mise run bench:bifrost:write-capacity -- --fixture logs`.
  - Fixture: 2M Kubernetes log records of about 1 KiB each, sent over OTLP
    gRPC into `vala.logs.records`.
  - Steps: x1, x4, x16 and x64 writers.
  - Server: one release `wyrd-server` with production Scribe settings (one
    shard, Parquet rotation and publication on).
  - Each step reports rows/s, OTLP wire MiB/s per server core, the mean time
    per batch for each Scribe stage, fsyncs per batch, and a CPU Pareto by
    crate from perf.
  - Output: `target/bifrost-query-capacity/logs-writes/parquet/profiles/`.
  - Other fixtures: `--fixture telemetry` (spans) and the default native
    sweep.
- **Read sweep:** `mise run bench:bifrost:query-capacity`. It runs hot
  (memtable) and Parquet tiers. `--fixture telemetry` exports spans instead.
- **Client queue:** `mise run bench:bifrost:client-queue` covers WyrdState
  Drift emission (AC-041).
- These three benches are kept after the merge (decision D3). Main's
  `bench:capacity` sits beside them.
- **Rules:**
  - Benches go through production paths only.
  - Don't compile while a bench runs.
  - Never generate synthetic host load.
  - Compare only runs from the same host. Runs so far are on an AMD Ryzen 9
    9950X with 32 CPUs and 91 GiB.

## Results, OTLP logs write sweep at x64

| Run | Change | rows/s | OTLP MiB/s per core | Server CPU per batch | Client mean | Verdict |
|---|---|---|---|---|---|---|
| run-34 | before one-fsync | — | — | — | — | slice sync 18.4 ms plus commit sync 8.0 ms per request |
| run-35 | one fsync per group commit | 257,848 | 37.4 | 16.33 ms | 126.6 ms | PASS |
| run-36 | + fixes 1, 3, 4 | 193,135 | 42.2 | 14.47 ms | — | FAIL: `group_sync` was 49 ms; the disk was slow on that run (fsyncs per batch unchanged) |
| run-37 | same code as run-36 | **260,475** | **42.6** | **14.35 ms** | 125.6 ms | **PASS** |

Run-37 at every step (code at `cfe21dc90`):

| Writers | rows/s | OTLP MiB/s per core | p50/p95/p99 ms | Server CPU per batch | Server cores |
|---|---|---|---|---|---|
| x1 | 11,668 | 40.6 | 44.0 / 45.5 / 47.0 | 14.93 ms | 0.34 |
| x4 | 41,673 | 45.5 | 50.0 / 59.9 / 66.9 | 13.40 ms | 1.09 |
| x16 | 137,713 | 46.9 | 59.9 / 91.4 / 167.0 | 13.04 ms | 3.51 |
| x64 | 260,475 | 42.6 | 112.8 / 246.6 / 303.4 | 14.35 ms | 7.30 |

Scribe stage means per batch at x64, run-37:

| Stage | Mean |
|---|---|
| `shard_wait` | 26.2 ms |
| `group_sync` | 19.6 ms |
| `group_fence` | 14.1 ms |
| `group_write` | 6.3 ms |
| `preprocess` | 2.7 ms |
| `group_insert` | 1.2 ms |

There were 0.06 fsyncs per batch.

CPU Pareto at x64, run-37, by crate:

| Crate | Share of server CPU |
|---|---|
| ZSTD (WAL compression) | 18.4% |
| wyrd_server (mostly OTLP protobuf decode) | 12.9% |
| sha2 | 9.9% |
| kernel | 8.6% |
| parquet_variant | 4.2% |

## Optimizations so far

| Item | Status | Evidence |
|---|---|---|
| One fsync per Scribe group commit, with replay stopping at a bad record in the unacked tail | In `cfe21dc90` | run-35; `newest_wal_fence_spans_tenants_on_one_stream` |
| ZSTD level 1 for WAL payloads | In `88f6e0657` | — |
| Scan sizing through one `ScanSizer`, and memtable column bytes recorded at append | In `1190c539a` and `05f2ba96b` | — |
| Partitioning on each table's time column (TASK-005, REQ-036) | In `cfe21dc90` | — |
| Fix 1: hash the logical identity once, reusing the Gate's digest in Scribe | In `cfe21dc90`, **but ineffective for OTLP** | See open item 1 |
| Fix 3: presized Variant and UTF-8 builders | In `cfe21dc90` | About −11% CPU per batch, run-35 → run-37 |
| Fix 4: cache conform plans per (tenant, table) | In `cfe21dc90` | `remembered_conform_plan_matches_a_fresh_plan_and_is_reused` |
| Fix 2: skip re-validating OTLP Variant bytes | **Dropped** | Validation already happens once, at Variant build (`EncodedVariant`). The check rejects NaN, oversize and over-deep values that OTLP can send (`non_finite_log_body_rejects_only_its_record`). |
| Fix 5: single-pass OTLP decode | **Skipped** | The preflight pass reserves exact decode memory before allocating (`grpc/otlp.rs:668-678`). Removing it weakens memory admission for about 3.5% of CPU. |

## Open optimization opportunities

Start these only after parity is proven. Each one comes from a profile, not a
guess. Before changing code, re-profile it on the merged branch and make one
change per run.

The read evidence comes from the pre-merge baseline at `cfe21dc90`
(`benchmarks/pre-merge/`). Its profiles are in the TASK-003 worktree under
`target/bifrost-query-capacity-premerge-cfe21dc90/`, and its logs hold the
latency, CPU and off-CPU Paretos for every case.

### Reads, in order

1. **Variant key filters.**
   - Evidence:
     - On the spans fixture, `attributes ->> 'customer.id'` and
       `attributes ->> 'region'` take 825 and 921 ms on hot, and 286 and
       260 ms on Parquet.
     - The promoted-column queries on the same table take 4–98 ms.
     - Parquet shreds the core keys in 24 of 24 span files ("variant
       shredding" row).
   - Opportunity: find out whether the filter reads the shredded column or
     decodes the whole Variant, and push the key filter down to the shredded
     column. This is the largest per-query gap measured.
   - This is the telemetry query benchmark: task step 5. Its case should run
     close to the plain-column lookup.
2. **ZSTD decompression on Parquet reads.**
   - Evidence: `c:ZSTD` is 29% of CPU on selective x1, 56% on 1m-aggregate x8
     (11.2 of 19.9 ms per query) and 24% on full-scan.
   - Opportunity: task step 4, measuring codecs on the telemetry data (ZSTD-3
     today vs ZSTD-1, LZ4_RAW, Snappy) for size and decode time. One option is
     a faster codec for fresh Scribe files and ZSTD on Forge compaction, since
     storage cost sits in old data.
3. **Hot selective: fragment open and planning.**
   - Evidence: of a 1.54 ms mean, `LiveScribeExec.fragment_open_time` is
     0.55 ms (36%), `Oracle.physical_planning` 0.31 ms (20%) and
     `Oracle.scribe_listing` 0.14 ms.
   - Opportunity: the local memtable fragment still opens through
     `ScribeFragmentExecutor`. Find out what that hop costs when the data is
     in the same process.
4. **Postgres pool wait.**
   - Evidence: hot selective x1 makes 0.50 pool acquires per query with a
     0.68 ms mean wait. Small-aggregate makes 0.84 acquires with 0.56 ms.
   - The read cut itself is cached: `Oracle.cut_read` is 0.002 ms.
   - Unknown: which path acquires the connection. Trace it with `WYRD_LOG`
     first. If it's on the query's critical path, it's comparable to all of
     planning.
5. **Physical planning on every small query.**
   - Evidence: 0.31–1.06 ms, 18–27% of latency for selective and
     small-aggregate on both tiers.
   - Opportunity: per-query planning cost, together with step 6.
6. **Per-query CPU grows with concurrency.**
   - Evidence: hot small-aggregate CPU per query is 2.86 ms at x1 and 7.14 ms
     at x64. `core`, `alloc` and `c:mi` together are about 35% at x64.
   - Opportunity: task step 6, allocator churn and per-query metrics
     collection (gather plan metrics only when profiling is asked for).
7. **Large hot scans.**
   - Evidence: table-aggregate uses 57.7 ms CPU per query for 39.8 ms
     latency, of which `datafusion_physical_plan` is 33%, `arrow_ord` 22% and
     kernel 15%. Full-scan uses 34.0 ms CPU, with `datafusion_functions` at
     37% and kernel 23%.
   - Not diagnosed yet. Look at the symbol-level profile first.
8. **Parquet full-scan.**
   - Evidence: 687 ms CPU per query, made up of `c:ZSTD` 24%, kernel 24% and
     `parquet` 12%.
   - The kernel share is not diagnosed yet: check the off-CPU and symbol
     profile for page faults or reads.
9. **Parquet chunk size.**
   - Evidence: 1m-aggregate x8 runs at 408 qps, p50 20.7 ms with the 24 MiB
     chunk. In runs 25/26 a 12 MiB chunk gave 500 qps at p50 14.8 ms, but
     doubled the CPU on table-aggregate and full-scan.
   - Opportunity: the chunk sweep on the telemetry benchmark, both tiers (task
     step 5).
10. **Unmeasured:** the 100M-row heavy fixture (`-- --heavy`), and native IPC
    Variant validation cost (no native profile yet).

### Writes, in order

1. **Confirm that fix 1's reuse hits.**
   - Today the Gate batch carries a `card_ref` column. Decode drops it,
     because it resolves to `card_uid`.
   - So `LogicalIdentity::for_rows` always misses, and both hash passes run.
     A diagnostic confirmed it: kl=30 vs ol=29, and sha2 cost is unchanged at
     about 1.4 ms per batch.
   - Main removed `card_ref`, so after the merge the reuse should hit.
   - Prove it with a profile: sha2 should fall by about half, roughly 4% of
     x64 CPU.
2. **Find the missing latency at x1.**
   - At x1 the client mean is 43.9 ms, but the Scribe stages add up to about
     9 ms.
   - The `Gate.write_request` profile is recorded (about 3,900 profiles), but
     its time reads 0.000 ms. The Gate/transport stage timing is not captured.
   - Measure that first, then attribute the remaining time.
   - The same per-request ack latency caps span export. In the pre-merge read
     baseline, 4 OTLP exporters reach about 60k spans/s at p50 42 ms, about
     0.8–1.2 server cores, so the server is not CPU-bound there. Throughput
     is roughly exporters × 512 spans / ack latency.
3. **Group commit at x64.**
   - `shard_wait`, `group_sync` and `group_fence` take about 60 ms of the
     125.6 ms client mean.
   - Pick the next target from a profile, not by guessing. `group_sync`
     varies between runs (run-36 vs run-37), so compare several runs.
4. **ZSTD is the largest write CPU item** (18–24% across steps).
   - It is WAL ZSTD level 1 already.
   - Any change is a durability/space trade-off and needs evidence first.
5. **Spans miss the per-core wire target.**
   - Evidence: the pre-merge `write-telemetry.md` peaks at x64 at 425,901
     rows/s, but only 16.5 MiB/s OTLP wire per core, against the 30 MiB/s
     target.
   - Spans are about 340 B of wire per row, against 1,253 B for logs. Per-row
     cost is similar (about 51k rows/s per core for spans, 36k for logs), so a
     bytes-per-core target penalizes small rows.
   - At x64, server CPU is 10.0 ms per batch. `wyrd_server` is 24.8% of it,
     mostly OTLP protobuf decode, and `parquet_variant` is 6.9%.
   - Where server CPU goes, from the x64 spans profile
     (`telemetry-writes/parquet/profiles/03-otlp spans-64/report.txt` in the
     preserved pre-merge profiles):

     Inclusive CPU per stage, from `perf report --no-inline --children` on
     that `perf.data`. Self-time listings understate stages: the preflight is
     0.7% self but 8.2% inclusive.

     | Stage | Inclusive share | Biggest parts |
     |---|---|---|
     | OTLP decode (`TraceRequestDecoder::decode`) | 28.1% | `decode_trace_protobuf` 19.7%, `preflight_trace_protobuf` 8.2%; `WireFields::next` + `read_varint` 17.8% self across both |
     | Projection (`project_resource_spans`) | 30.7% | `attributes_variant` 20.8%: Variant build (`append_object`) 11.7%, then `finish_variant` 9.7%, of which `EncodedVariant::validate` is 8.2% |
     | Scribe ingress pool | 14.6% | `prepare_append` 10.2%: Arrow IPC encode 6.5% (WAL ZSTD 5.8%), `stamp_correlation_columns` 2.3%, `logical_data_identity` 1.7%; `decode_rows` 3.2% |
     | Scribe persistence pool | 13.7% | Parquet `encode_batch` 13.4%: sort `take` 2.5%, Parquet ZSTD 3.1%, byte-array encode |
     | Gate dispatch (`dispatch_canonical`) | 4.3% | `LogicalIdentity::of` 1.8% |

   - The scanner cost is mostly repeated scanning of the same bytes, in
     `crates/wyrd/wyrd-server/src/otlp_decode.rs`:
     - `decode_span` makes about 10 full `WireFields` passes per span: 3
       `repeated_message_count`, 5 `last_bytes_field`,
       `decode_merged_status` (2+), then the main loop.
     - `decode_key_value` makes about 7 passes per attribute: 2 in
       `decode_merged_any_value`, 1 in `last_bytes_field`, 1 validation
       loop, then 2 over the value body (`select_any_value_bodies` and the
       fill).
     - Every other `decode_*` function uses the same count-then-scan pattern.
     - The counts exist so every `Vec` is sized exactly and the decode
       reservation is exact (commit `9d8a864f6`, "Prove exact trace decode
       capacity"). Any change must keep that.
   - Levers, in order. Measure one per run against
     `benchmarks/pre-merge/write-telemetry.md`, after the merge:
     1. **Two passes per message instead of 7–10.**
        - Pass 1 walks the message once. It collects all repeated counts, the
          last offset of each singular field, and the wire-type checks.
        - Pass 2 fills the struct.
        - This keeps exact capacity, last-wins semantics and every malformed
          check.
        - Add a fast path for a singular `value` or `status` that occurs once,
          so the AnyValue merge machinery runs only when a field repeats.
        - Targets the 17.8%. If scanner cost scales with pass count, the
          saving is about 10–13% of total CPU. That is an estimate, not a
          measurement.
     2. **Single-byte varint fast path in `WireFields::read_varint`.** Every
        OTLP tag here is ≤ 16, so every key fits in one byte, and so do most
        lengths. A `byte < 0x80` early return before the loop does the job;
        prost uses the same trick. Targets part of the 6.9%.
     3. **Decode straight into the projection's Arrow/Variant builders**,
        skipping the intermediate `Span` / `KeyValue` / `String` structs.
        - This removes the typed-struct build and drop (`from_utf8`, `String`
          allocation, `Vec` pushes and drops) and the projection's walk over
          those structs. Its share is not separated in the profile.
        - It is larger than 1 and 2: `OtlpDecodePlan.decode_bytes` is defined
          as the size of those structs, so the reservation accounting has to
          be redesigned. Do 1 and 2 first.
     - The preflight stays in every case. It is the memory-admission guard.
       It costs 8.2% inclusive and runs on the same `WireFields` scanner, so
       lever 2 also speeds it up.
   - **Levers outside decode**, from the same profile:
     4. **Stop fully re-validating Variants the server just built.**
        - `finish_variant` (`tables/signal.rs`) passes the bytes it just built
          from `VariantBuilder` through `EncodedVariant::from_bytes`. That runs
          the full raw-bytes gate: the `scan_encoded` walk plus upstream
          full validation. It costs 8.2% for input the server constructed
          itself.
        - The size limit is a length check. The depth limit can be tracked
          while building, or proved from the OTLP preflight depth limit.
        - Condition: the Variant size and depth limits must still hold exactly.
     5. **Compute the logical identity once.** Gate's `LogicalIdentity::of`
        (1.8%) and Scribe's `logical_data_identity` (1.7%) both SHA-256 the
        same batch. This is write item 1, fix 1's reuse not hitting.
     6. **WAL ZSTD (5.8%)** is write item 4. **Parquet encode (13.4%)** is
        flush work on the persistence pool, so it is per-row CPU but off the
        request path.
   - **Can spans reach 30 MiB/s per core?**
     - At ~340 B per row, that target means about 92k rows/s per core, against
       about 51k today. That needs roughly 45% less CPU per row.
     - Decode plus projection is 59% of CPU. Levers 1, 2, 4 and 5 add up to
       roughly 20–25% (about 10–13% from 1, part of the scanner from 2, about
       8% from 4, about 1.7% from 5). These are estimates, not measurements.
     - So the per-byte target needs lever 3 and more on top, and it
       is still open whether small signals are judged per row or per byte.
       Spans already clear the 200k rows/s floor, at 426k.
6. **Native write tail.** Native passes at 3.31M rows/s at x64, but p99 is
   667 ms at x16 and 717 ms at x64. Not diagnosed yet.
7. **Hot-only and Parquet-only write runs** of the same sweeps, plus a metrics
   fixture.
8. **Native validation cost:** native IPC pays a full `EncodedVariant`
   validation at the trust boundary. Don't change it unless a native profile
   shows that it matters.

## Benchmarking wyrd-gateway (step 4)

- **Existing work:** branch `change/gateway-benchmarks`, worktree
  `wyrd-gateway-benchmarks`.
  - Its packet is `changes/active/gateway-benchmarks/` (spec, tasks, review).
  - The last commit, `916155620` (2026-09-25), records the TASK-002-R1
    remediation evidence and a boot blocker.
  - The branch is 2+ weeks old and predates main's bench consolidation.
- **Main's `bench:capacity`** (REQ-171) is the one mixed server capacity
  bench. Decide whether the gateway bench extends it or stays separate, the
  way the Bifrost benches do.
- First, read that packet and its boot blocker against the merged tree. Don't
  start from scratch.

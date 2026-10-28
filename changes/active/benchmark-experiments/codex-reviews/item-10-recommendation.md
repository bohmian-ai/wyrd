I recommend one shredded memtable representation, with keys learned from recent data and fixed for each generation. Keep recovery files unshredded and let uploaded files retain their existing, independently chosen layout. The code supports this direction, but does not establish how much of the latency gap shredding alone will remove.

“Shredding” means extracting selected JSON paths into typed columns while retaining everything else in a residual value.

**The row’s path today**

1. **Request → WAL: unshredded.** Preprocessing partitions decoded Arrow batches, encodes them as Arrow IPC, and retains those same rows for insertion ([preprocess.rs:789](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:789)). Scribe writes and synchronizes WAL commits, inserts the committed batches, publishes visibility, then acknowledges ([shards.rs:3219](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/shards.rs:3219)).

2. **Active → frozen memtable: unshredded throughout.** Append retains the batch unchanged. Freeze collects shared batch references; it does not rewrite values ([memtable.rs:1379](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:1379), [memtable.rs:1430](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:1430)).

3. **Local staged Parquet: unshredded.** The member stager encodes frozen batches. Its writer receives `None` for the shredding layout. No keys are selected here ([member_stager.rs:220](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/member_stager.rs:220), [parquet_writer.rs:368](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:368)).

4. **Merged upload output: shredded.** Before merging staged runs, claim assembly samples the **whole claim’s runs** and chooses one layout. Each ordered output batch is shredded immediately before writing. All files rolled from that claim share that layout ([claim_assembly.rs:253](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/claim_assembly.rs:253), [parquet_writer.rs:693](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs:693)).

5. **Upload → publication: unchanged.** Scribe uploads assembled artifacts, commits publication, advances the readable authority, and retires staged members. It neither samples nor encodes again ([claim_publication.rs:386](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/claim_publication.rs:386)).

6. **Forge compaction: layout chosen again, from footers—not sampled rows.** Forge combines layouts across every distinct input file in a rewrite group. Footer counts select existing typed leaves occurring in at least **10%** of combined non-null roots. It cannot discover a key absent from every input’s shredded layout. Output batches use that combined layout ([compaction executor:421](/home/thorrester/.cargo/git/checkouts/iceberg-compaction-3b4eb9269abd88f3/c213af0/core/src/executor/datafusion/mod.rs:421), [variant_shredding.rs:178](/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/850d308/crates/iceberg/src/writer/file_writer/variant_shredding.rs:178)).

**What the existing sampler actually does**

It reads staged files twice: first `sub_principal_id` to count groups, then that column plus Variant columns. Each principal with at least **30 rows** gets its own sampling group; smaller principals share one group. Selection hashes the claim-derived seed and row position. The same selected rows feed every Variant column ([claim_sample.rs:40](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/claim_sample.rs:40), [claim_sample.rs:60](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/scribe/claim_sample.rs:60)).

For a group containing \(N\) rows:

\[
n_0=\lceil2.5758^2\times0.25/0.02^2\rceil=4147,\qquad
n=\min(N,\lceil4147/(1+4146/N)\rceil).
\]

Each row is independently retained with probability \(n/N\): **n is an expected count, not a hard cap**. It approaches 4,147 **per group**, rather than growing proportionally with input ([policy:98](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:98), [formula:86](/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/850d308/crates/iceberg/src/writer/file_writer/variant_shredding.rs:86), [selection:548](/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/850d308/crates/iceberg/src/writer/file_writer/variant_shredding.rs:548)).

The denominator is sampled **non-null Variant roots**, separately per column and group. A key qualifies at **8%** in any group: the configured 10% frequency minus the 2% margin. Candidates rank by estimated rows covered. Each object tracks the first **1,000** distinct child names and emits at most **300**; later names can therefore be missed. Object depth stops at **50**. Arrays inside Variant remain residual. Incompatible types also fall back to residual ([threshold:647](/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/850d308/crates/iceberg/src/writer/file_writer/variant_shredding.rs:647), [caps:835](/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/850d308/crates/iceberg/src/writer/file_writer/variant_shredding.rs:835), [types:956](/home/thorrester/.cargo/git/checkouts/iceberg-rust-a48518f2dbe5117a/850d308/crates/iceberg/src/writer/file_writer/variant_shredding.rs:956)).

**Observed input scale**

The supplied log’s final counters show **801.59 million inserted rows**, **282.24 GB persisted raw data**, **1,413 staged members**, and **366 assembled claims** ([log:107077](</tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-rev19/3b4a802a-ab40-4e5b-b8d0-574ea2f1a6e6/scratchpad/flow-sustain-1.log:107077>), [log:107249](</tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-rev19/3b4a802a-ab40-4e5b-b8d0-574ea2f1a6e6/scratchpad/flow-sustain-1.log:107249>)).

Using inserted bytes per row gives approximately **565,000 row-equivalents/member** and **2.18 million/claim**. These are aggregate estimates, not exact Variant populations. At those populations, a single sampling group expects about **4,117** and **4,140** samples respectively. Multiple principals increase the total.

The JSON records **134, 117, and 113 publications** across its three native-write windows. It measures publication during each interval, so dividing acknowledged rows by publications does not establish file row counts ([JSON:250](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/target/benchmarks/bifrost/writes-parquet/20261028T101743Z-64ca466fb.json:250), [JSON:1136](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/target/benchmarks/bifrost/writes-parquet/20261028T101743Z-64ca466fb.json:1136)).

The query fixture contains **one million spans**. `region` appears on approximately **25%**, across four values; `eu-west` matches approximately **6.25%**. The expected answer is computed from generated rows ([query/mod.rs:74](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/wyrd/wyrd-testing/src/bin/capacity/query/mod.rs:74), [telemetry.rs:305](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/wyrd/wyrd-testing/src/bin/capacity/query/telemetry.rs:305)).

**Options**

All options can leave staged files unshredded and upload sampling unchanged.

| Option | Key quality versus today | Memory, writes, reads |
|---|---|---|
| **(a) Unshredded** | No selection | Lowest construction cost; preserves expensive row-by-row extraction. |
| **(b) Original plus shredded copy** | Depends on sampling policy | Extra resident representation and construction work; fast selected paths. Useful fallback, expensive permanent design. |
| **(c) Recent-data layout** | More current; smaller historical coverage than today’s whole claim | One stored representation plus sampling/construction scratch. Shredding costs ingest CPU; selected reads use typed columns. Refresh between generations. |
| **(d) Last uploaded layout** | Representative of an older claim; stale during changes and unavailable at startup | Avoids new selection work. Still pays shredding cost; new keys remain slow until publication catches up. |
| **(e) Fixed table list** | Excellent for known stable workloads; poor for evolving tenant data | Predictable memory and writes; consistently fast listed paths. Adds configuration and misses unlisted paths. |
| **(f) Fused text extraction** | Handles every path without selecting keys | No extra resident copy. Improve the shared Arrow/DataFusion kernel to avoid an intermediate Variant allocation and second row loop. Still searches each row; speedup unmeasured. |

That last opportunity is real: unshredded `->>` builds an intermediate Variant, then text; fully shredded strings can return their typed column ([Arrow:251](/home/thorrester/.cargo/git/checkouts/arrow-rs-7086235adca86d7c/ca5feff/parquet-variant-compute/src/variant_get.rs:251), [DataFusion:272](/home/thorrester/.cargo/git/checkouts/datafusion-44e2ebb5250b7f6f/19796a1/datafusion/functions-variant/src/get.rs:272)).

**Schemas and exact results**

Clients must receive identical counts, scalar types, and whole JSON values. Missing, null, mixed types, nested values, and arrays must remain exact through residual fallback.

DataFusion supports native shredded arrays, but each scan stream needs one schema. Different layouts require separate internal leaves, filtering before normalization, then a canonical schema before union. Today’s ordinary Arrow cast is not semantic unshredding ([exec.rs:2081](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/crates/vala/vala-bifrost-redux/src/oracle/exec.rs:2081)).

Reconstruct whole Variant outputs and canonical wire batches; avoid reconstructing columns ultimately discarded, once filter ownership permits it. Preserve the wire contract and clarify that internal memtables may use shredded layouts. Recovery staging need not change ([architecture:283](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/architecture/bifrost-design.md:283), [architecture:415](/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-rev19/architecture/bifrost-design.md:415)).

No files changed or execution performed; forbidden proposals were excluded. Exact per-claim populations and runtime attribution remain unknown.

**Recommendation:** choose **(c)**, replacing rather than duplicating retained Variant data, with bounded recent sampling and immutable generation layouts. Confirm with hot-filter p99, ingest throughput/acknowledgement p99, peak charged memory including scratch and captures, key-change workloads, and exact hot/staged/published result equivalence.
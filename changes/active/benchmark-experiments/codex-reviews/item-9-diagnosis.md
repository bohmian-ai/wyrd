**Scribe is waiting on durability I/O.** One shard serializes its WAL cycle; sixteen shards fragment those batches and saturate the shared four-worker WAL lane. Staging and publication add competing writes and syncs on the same device. The evidence establishes these software limits, but does **not** establish the NVMe’s maximum bandwidth or flush capacity.

**1. Limiting stages and row lifecycle**

Each shard waits for `process_group` to finish before starting another: SLICE writes → COMMIT writes → WAL sync → tenant-scoped PostgreSQL fences → memtable insertion → ACK. See [shards.rs:1405](crates/vala/vala-bifrost-redux/src/scribe/shards.rs#L1405), [shards.rs:3211](crates/vala/vala-bifrost-redux/src/scribe/shards.rs#L3211), and [shards.rs:3736](crates/vala/vala-bifrost-redux/src/scribe/shards.rs#L3736).

The sustained records show:

| Native rows/s, three 120-second repetitions | One shard | Sixteen shards |
|---|---:|---:|
| 10:17 / 10:24 pair | 2.348M, 2.128M, 1.993M | 1.801M, 2.151M, 2.072M |
| 10:58 / 11:05 pair | 1.756M, 2.176M, 2.064M | 1.762M, 2.233M, 2.417M |

That is **−6.9% and +6.9%** aggregate scaling within the respective pairs. Server CPU uses only **3.4–5.34 of 32 logical CPUs**.

Combining the first pair’s retained timing histograms gives request-weighted means:

| Pre-ACK component | One shard | Sixteen shards |
|---|---:|---:|
| Shard wait | 171 ms | 188 ms |
| WAL append, including lane waiting | 80 ms | 173 ms |
| WAL sync, including lane waiting | 149 ms | 134 ms |
| PostgreSQL fences **plus** memtable insertion | 10.9 ms | 3.9 ms |
| ACK delivery | 0.26 ms | 0.04 ms |

These are shared group timings charged to requests, **not independent work to sum across requests**. The mapping is explicit in [profile.rs:93](crates/vala/vala-bifrost-redux/src/scribe/profile.rs#L93).

After ACK, rotation freezes tenant/table/version/partition buckets. Staging sorts and encodes Parquet, syncs each run and its directory, preflights decoding, then durably publishes its member record. See [member_stager.rs:219](crates/vala/vala-bifrost-redux/src/scribe/member_stager.rs#L219).

That staged authority permits Arrow retirement; **object publication is not required for memory release**. Completion returns to the shard, which detaches immutable accounting; actual buffers remain charged while another holder retains them. See [memtable.rs:1007](crates/vala/vala-bifrost-redux/src/scribe/memtable.rs#L1007) and [shards.rs:2523](crates/vala/vala-bifrost-redux/src/scribe/shards.rs#L2523).

Compatible staged members subsequently merge, pass local election/staging, upload, commit fenced `file_list`/lineage, and retire local evidence: [claim_publication.rs:386](crates/vala/vala-bifrost-redux/src/scribe/claim_publication.rs#L386).

For `flow-sustain-{1,16}.log`, using snapshot index 30 through the penultimate snapshot:

| Sustained stage | One shard | Sixteen shards |
|---|---:|---:|
| Arrow inserted / staged / retired | 728 / 729 / 726 MiB/s | 683 / 686 / 683 MiB/s |
| Staged encoded output | 59.4 MiB/s | 55.8 MiB/s |
| Published output | 56.8 MiB/s | 52.9 MiB/s |
| Members staged | 3.82/s | 2.74/s |
| Mean encode / run-sync-and-preflight | 345 / 496 ms | 509 / 817 ms |
| Mean member registration / completion ACK | 211 / 138 ms | 463 / 151 ms |
| Claims assembled; mean assembly time | 1.02/s; 2.57 s | 0.72/s; 4.82 s |

Insertion and retirement match closely. This is **not sustained throughput financed by indefinitely accumulating Arrow**. Publication bytes are compressed; comparing them directly with raw Arrow bytes would misdiagnose the drain rate.

**2. Why shards do not scale**

The same intervals expose the shared limit:

- WAL lane active jobs: **0.84 → 4.00**; mean queued jobs: **0 → 11.1**.
- WAL syncs: **6.29/s → 50.79/s**.
- Approximate acknowledged requests per WAL sync: **22.5 → 2.6**.
- Actual WAL-sync mean: **87 → 59 ms**, versus much longer request-level sync timing.
- Device writes: approximately **312 → 291 MiB/s**; device flushes: **42.6 → 78.1/s**.

Thus sixteen shards produce about eight times as many WAL syncs for comparable accepted work. All four workers become occupied, and both writes and syncs queue behind them. The configured default is explicitly four: [config.rs:1464](crates/wyrd/wyrd-server/src/config.rs#L1464).

Staging adds **43.7/30.4 counted namespace syncs per second**, excluding run syncs and other durability paths. Every member-state rewrite syncs a temporary file and directory: [hot_stage.rs:1162](crates/vala/vala-bifrost-redux/src/scribe/hot_stage.rs#L1162). Parquet encoding also calls `sync_data` every 8 MiB: [parquet_writer.rs:509](crates/vala/vala-bifrost-redux/src/scribe/parquet_writer.rs#L509).

CPU, ingress/persistence executor queues, and admission bounds are not the demonstrated sustained limits. PostgreSQL may contribute shared-device contention, but its combined fence/insertion latency is much smaller than WAL waiting.

**3. Ranked fixes**

1. **Highest leverage: bounded, pipelined WAL group commit at the Scribe WAL/execution owner.** Separate ordered append preparation from durability completion; allow subsequent bounded work while earlier groups sync/fence. For shard scaling, batch durability across shard submissions rather than independently shrinking every shard’s groups. ACK must still wait for its WAL durability frontier, durable tenant fence, and insertion. Preserve ordered recovery, duplicate handling, ambiguity retention, and exact accounting. Physical WAL multiplexing would require revising the owning architecture’s per-shard-stream contract. Expected effect: recover serial idle time and reduce durability operations per accepted batch; no defensible multiplier yet.

2. **Reduce staging/publication durability amplification at `ScribeHotStage`, manifest, and staging owners.** Batch durable lifecycle transitions/checkpoints and avoid redundant full artifact copies where ownership transfer suffices. Keep runs durable before authority transfer and checkpoint replay evidence before deletion. Expected effect: lower competing sync/write traffic and improve both WAL latency and downstream drain.

3. **Then tune lane concurrency and storage placement.** More WAL workers could reduce the observed queue, but could also increase device contention. Cloud object storage removes the benchmark’s local upload writes; WAL and durable staging still need efficient local durability.

**4. Counterevidence and open questions**

Short windows sometimes show higher ingest while retaining tens of GiB; they do not establish sustained scaling. The second sustained pair reverses the small throughput difference, reinforcing “no meaningful scaling,” not a reliable sixteen-shard regression.

Device counters are host-wide; write ticks are not utilization, and concurrent flush time can exceed wall time. **Flush/writeback interference is strongly suggested, not isolated causally.** One portable timestamped trace separating WAL enqueue/start, append, sync, staging-sync, and PostgreSQL commit waits would settle their attribution. A device-separated sustained comparison would test the shared-device hypothesis.

No files changed, builds/tests/benchmarks run, or excluded conclusions read. Expected gains remain unmeasured.
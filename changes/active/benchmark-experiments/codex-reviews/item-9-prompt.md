Independent read-only diagnosis. Do NOT edit files, build, or run tests or benchmarks. Repository: this directory (Wyrd).

Goal (read only lines 216-252 of changes/active/benchmark-experiments/tasks/TASK-006-R2-revision-19-supplement.md, section "## 9. Single-shard write throughput and shard scaling"). Do NOT read the "Item 9(...)" sections later in that file, git commit messages on this branch, or any *diagnosis* / *prompt* file in the scratchpad: they contain another agent's conclusions, and this review must be independent of them. Form your own view from code and raw data.

Question: what limits Scribe native write throughput on one shard, and why does raising WYRD_SCRIBE_SHARD_COUNT from 1 to 16 not raise sustained throughput? Then: what change, at which owner, would most raise single-shard throughput and make shard count scale?

Constraints set by the maintainer (these are requirements, not hypotheses):
- Must work identically on macOS arm64, Linux x86_64, Linux arm64. No Linux-only or macOS-only mechanisms (no io_uring, O_DIRECT tuning, cgroups, /proc, cfg(target_os)).
- Kafka or any external log is ruled out. Production is usually deployed with a cloud object store backend; local disk is staging/WAL.
- Acknowledged writes must remain durable (WAL before ack). Tenant isolation and correctness may not be weakened.

Authority: architecture/bifrost-design.md (Scribe ingest, staging, publication, resource invariants), AGENTS.md.
Code: crates/vala/vala-bifrost-redux/src/scribe/ (wal.rs, shards.rs, memtable.rs, persistence.rs, member_stager.rs, parquet_writer.rs, hot_stage.rs, staging*.rs, claim_*.rs, assembly.rs, execution_lanes.rs, admission.rs, manifest.rs, mod.rs), crates/vala/vala-bifrost-redux/src/resources.rs, crates/wyrd/wyrd-server/src/boot/mod.rs, crates/wyrd/wyrd-server/src/config.rs. Benchmark driver: crates/wyrd/wyrd-testing/src/bin/capacity/ (run via `mise run bench:l1:bifrost:writes`).

Host: 32 vCPU, 91 GiB RAM, one consumer NVMe under dm-crypt + ext4 (no power-loss protection). Postgres runs on the same host in Docker. Object store in the benchmark is file:// on the same disk.

Raw data (all read-only):
- Benchmark records (JSON, every step's metrics): target/benchmarks/bifrost/writes-parquet/*.json. Sustained 3 x 120 s native-write-c64 runs: 20261028T101743Z (1 shard), 20261028T102420Z (16 shards), 20261028T105850Z (1 shard), 20261028T110528Z (16 shards). Shorter 10 s-window runs: the other 20261028T0* records (each pair is 1 then 16 shards, each on a different code revision, so compare within a pair only).
- Server /metrics scraped once per second during runs (lines "T <unix>" followed by Prometheus text): /tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-benchmark-rev19/3b4a802a-ab40-4e5b-b8d0-574ea2f1a6e6/scratchpad/flow-sustain-{1,16}.log and flow-nopublished-{1,16}.log (sustained), flow-*-{1,16}.log (others). Server logs: server-*-{1,16}.log in the same directory.

Work from the code and the numbers. Trace a row from request to WAL ack to memtable to staged Parquet to publication to memory release; quantify each stage's rate and latency from the metrics; identify which resource saturates (CPU, device flushes, bandwidth, locks, serial owners, queue bounds, Postgres) and show the evidence. State explicitly any assumption you cannot verify from the data and what single measurement would settle it.

Return under 1200 words: (1) the limiting stage(s) with file:line and metric evidence; (2) why shards do not scale; (3) ranked fixes with owner and expected effect; (4) counterevidence and open questions.

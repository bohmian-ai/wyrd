# Bifrost controlled reference benchmark

`mise run bench:bifrost:cluster:reference` is the only Bifrost
production-readiness benchmark. It drives the public Rust SDK through Gate,
Scribe, Forge, and Oracle. The Scribe, Forge, Oracle, WAL/fsync, OTLP, plan,
scan, and rewrite benches are component diagnostics; they do not establish a
product SLO or headline capacity result.

## Reference profile v1

The controlled lane records and compares this closed identity:

- Rust major/minor and the locked Arrow, DataFusion, and Iceberg revisions;
- OS, kernel, architecture, normalized CPU vendor/model, logical CPU count,
  and total host memory;
- the benchmark-only `postgres:16` container from
  `docker-compose.reference.yml`, pinned to 2 CPU and 4 GiB, with tmpfs data;
- the in-process loopback memory storage implementation and configuration;
- Git SHA and dirty state, capture timestamp, seed `0xB1F057`, exact topology,
  tenant count, traffic mix, 64-row write shape, bounded strict-query shape,
  and each absolute offered request rate.

Linux CPU identity comes only from `/proc/cpuinfo` `vendor_id` and `model
name`. macOS identity comes only from `sysctl -n machdep.cpu.brand_string`.
Missing CPU classification or Docker-inspected CPU/memory limits makes the run
`Unsupported`. A dirty worktree may emit diagnostics but cannot bless a
reference.

The matrix contains balanced 50/50 traffic for one pod/one tenant, one
pod/eight tenants, 3 Server + 3 ForgeWorker/eight tenants, and 3+3/32 tenants;
plus 90/10 write-heavy and 10/90 read-heavy 3+3/eight-tenant scenarios. Each
reference rate has three trials with a five-second warmup and 20-second measured
window. Candidate comparisons replay the exact persisted 50/75/100-percent
absolute reference rates.

## Capture and promotion

`mise run bench:bifrost:cluster:capture` writes a candidate under
`target/bifrost-benchmarks`; it never changes this directory. Compare two
captures with:

```bash
mise run bench:bifrost:cluster:compare -- \
  --before benches/bifrost/reference-v1.json \
  --after target/bifrost-benchmarks/cluster-candidate.json \
  --output target/bifrost-benchmarks/comparison.json
```

Promoting `reference-v1.json` is manual. The change must include the candidate
manifest, prior comparison, reason, exact hardware/container identity, and
review approval. Never copy a shortened smoke, incompatible profile, dirty
capture, censored scaling claim, or undersampled report into the reference.

The initial reference file is created only by the first successful controlled
full capture and review. It must not be synthesized from component reports or
hand-authored measurements.

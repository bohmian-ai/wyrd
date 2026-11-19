---
id: TASK-001
spec: SPEC-benchmark-heap-profiling
status: implemented
---

# Opening memory and sampled heap profiles

Implements every requirement of the spec in one task.

## Reuse

- Allocator: the existing `CountingMiMalloc` (`crates/wyrd/wyrd-server/src/main.rs`)
  reports to the sampler and still delegates to mimalloc.
- Endpoint: the existing loopback benchmark listener (`app/metrics.rs`),
  beside `/benchmark/timings`, gated by the existing `bench` feature.
- Harness: `--profile`, `ProfileNote` and the `profiles/` directory
  (`wyrd-testing/src/profile.rs`); `PeakSampler` keeps its first reading.
- Encoding: `prost` (workspace) and `flate2` (already a server dependency).
  `backtrace` is the one new crate, optional behind `bench`.

## Execution Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 opening memory | `release_server/os.rs` `MemoryReadings`, `capacity.rs` `Usage`/`Resources`, `experiment.rs` `Repetition.opening_memory_bytes` | `release_server::os::tests::reads_this_process_and_host`, `experiment::tests::repetitions_without_opening_memory_still_read` | PASS |
| AC-002 sampler and endpoint | `wyrd-server/src/heap_profile.rs`, `app/metrics.rs` | `heap_profile::tests::samples_live_allocations_from_the_start`, `heap_profile::tests::stacks_start_at_the_requesting_code`, `app::metrics::tests::bench_heap_profile_starts_and_serves`, `app::metrics::tests::benchmark_snapshots_are_local_read_only_and_opt_in` (with and without `bench`) | PASS |
| AC-003 harness capture | `profile.rs` `HeapProfiler`, `experiment.rs` runner, `Workload::heap_profile_url` | `profile::tests::heap_captures_and_failures_become_profile_evidence`, `experiment::tests::profiling_is_optional_diagnostic_evidence` | PASS |
| AC-004 real run | — | Gateway L1 `--profile --allow-dirty` (`target/benchmarks/gateway/overhead/20261107T035901Z`): VALID, 8 `heap-<step>.pb.gz` saved, stacks symbolized to arrow builders, Parquet variant shredding and `vala_bifrost_redux::tables` schema stamping. Revision 2 (`alloc_*`): Gateway L1 `--profile` at 91ee9c39b (`target/benchmarks/gateway/overhead/20261107T131644Z`): VALID, 8 heap profiles with four sample types; `alloc_space` between steps attributes to Wyrd source lines (`components/gateway/routes.rs` `token_bound`, `wyrd-gateway/src/adapter/http.rs` `send`, Scribe `claim_merge`/`schema::logical_column`/`layout_cache`) | PASS |
| AC-005 disabled cost | — | clean unprofiled Gateway L1 at 6d12ff842 against the last benchmark-records entry (92b62f9c0): VALID, zero errors and losses, 7 of 8 step rates within its repetition range; `buffered-1kib-capture-metadata.c64` (55,441 req/s) re-run as back-to-back pairs against a hook-free `ae8e137d3` bench server: 57,327 / 57,814 and 58,031 / 57,062 req/s (base / candidate), both candidates inside the record's 56,626..58,069 | PASS |

Commands: `mise run fmt`, `mise run lints`, `mise run check:deps`,
`git diff --check`,
`cargo nextest run -p wyrd-server --features bench --lib -E 'test(heap_profile::) | test(bench_heap_profile) | test(benchmark_snapshots_are_local)'`,
`cargo nextest run -p wyrd-server --lib -E 'test(benchmark_snapshots_are_local)'`,
`scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --lib` (65 passed),
`cargo nextest run --locked -p wyrd-testing --bin capacity` (50 passed).

Non-goals stayed excluded: no allocator change,
no driver or worker profiles, no criteria, history or results-page change.

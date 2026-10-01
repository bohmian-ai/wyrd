---
id: TASK-006
title: Benchmark real wyrd-server processes and delete the process harness
kind: implementation
status: in-progress
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 19
requirements: [REQ-008]
acceptance: [AC-010]
depends_on: []
blocks: [TASK-004]
---

## Outcome and Value

Every benchmark number describes the binary users run. The capacity benchmark
starts one release `wyrd-server` through the local-development journey
(`migrate`, serve, `setup`), setting only `WYRD_STORAGE_URL`, limited to
4 CPUs/8 GiB by a systemd scope, and drives it through the public Rust client.
Multi-pod benchmarks belong to kind. The multi-process test harness
(`bifrost_peer_test_node`, `bifrost::process_cluster`, its control protocol)
is deleted; nothing in the repository simulates a server process.

## Requirements

1. The benchmark starts one real `wyrd-server` in a systemd user scope
   through the local-development journey. It detects its own cgroup CPU and
   memory; nothing is injected and no server default is set.
2. Tenant and credential come from `wyrd-server setup`. Writes and queries use
   the public client. Server evidence comes from `/metrics`, the process's
   cgroup files, and the data and storage directories the benchmark
   configured.
3. No publication wait, flush, or snapshot refresh: acknowledged rows are
   read back exactly through the public client, as an application would.
4. Journeys that used the harness move to real `wyrd-server` processes,
   observed through the public client, `/metrics`, filesystem, and OS signals
   (`kill -9`, `SIGSTOP`), or to in-process `WyrdTestServer` tests at the
   owning seam when they need a precise fault. No proof is dropped without a
   recorded replacement.
5. Production state that a journey needs and no metric exports gets a
   production metric, not a test hook.
6. Delete `bifrost_peer_test_node`, `bifrost::process_cluster`, and every
   reference to them.

## Verification

- `mise run bench:bifrost:query-capacity` and
  `mise run bench:bifrost:query-capacity -- --heavy`, one at a time, report
  and server log preserved.
- Every migrated journey and the owning Oracle lanes pass.
- `mise run fmt`, `mise run lints`, `git grep` finds no harness reference.

## Evidence

| Requirement | Implementation | Verification | Result |
|---|---|---|---|
| 1. Real process, local-dev journey | `src/bin/bifrost_query_capacity/server.rs` `LocalServer::start` (`migrate`, systemd-scoped serve, `setup`; only `WYRD_STORAGE_URL`; `confirm_envelope`) | `bench:bifrost:query-capacity` | pending |
| 2. Tenant via `setup`, public evidence | `LocalServer::start` parses `admin_credential`; `run.rs` `Window` (`/metrics`, `cpu.stat`, `memory.peak`, `memory.events`) | `bench:bifrost:query-capacity` | pending |
| 3. No publication wait | `run.rs` `Bench::check_answers` and every closed-loop query compare exact answers right after `Bench::write` | `bench:bifrost:query-capacity` | pending |
| 4. Journeys off the harness | `tests/bifrost/oracle/peer_cluster.rs` `PeerCluster` (in-process `WyrdTestCluster` pods, real loopback mTLS peer sockets) drives every former process-harness journey | `mise run test:bifrost` journey:oracle 40/40 | PASS |
| 5. Production metrics, not hooks | No new test hook; journeys read `/metrics` and existing test-support owners | `mise run test:bifrost` | PASS |
| 6. Harness deleted | `bifrost_peer_test_node`, `bifrost::process_cluster` (+ `child.rs`), its `[[bin]]`, the now-unused `provision_tenant_service_principal`; nextest group renamed `peer-clusters` | `git grep process_cluster\|BifrostProcessCluster\|bifrost_peer_test_node` empty outside `changes/`; `wyrd-testing --lib --bins` 61/61; `mise run lints` | PASS |

Known fidelity gap: `PeerCluster::kill` cancels the pod and aborts its serve
task, but tonic's per-connection tasks outlive it, so an open peer stream is not
reset the way a real `SIGKILL` resets it. `remote_live_scribe_drop_releases_query`
therefore proves its "remote Scribe lost" ending at the leader's 120 s deadline
(132 s total, versus 23 s under the deleted process harness).

### Benchmark failure diagnosis (2026-09-30)

**Failure 1 — live reads refused with 503.**
- **Symptom:** live-path queries in the benchmark returned 503 with `Scribe live-tail snapshot exceeds its bound`.
- **Evidence:** the leader signed `LIVE_FRAGMENT_MAX_BATCHES = 4096` and `LIVE_FRAGMENT_MAX_RETAINED_BYTES = 256 MiB` into every Scribe cut. The memtable rotates at 512 MiB, so live data routinely exceeds that cap. The Scribe then refused with `IngestBusy`, and `live_open_error` mapped that to Capacity.
- **Cause:** a snapshot cap that protects nothing. The live snapshot holds shallow references to rows the Scribe already has, and the query's memory pool governs what execution retains.
- **Fix site:** the cap was deleted everywhere:
  - proto fields 7 and 8 are now reserved;
  - removed from the spec `ScribeProviderCut`;
  - the assignment digest domain is now v6 (the vector is 460 bytes);
  - removed from the tonic conversions, the leader constants and the test override;
  - removed from `FetchLiveTailRequest`, `ShardRuntime::snapshot`, `ReadableBatchLimits` and `ReadableBatchCollector`.

  `IngestBusy` still means a full shard mailbox.

**Failure 2 — staged reads `NotFound`.**
- **Symptom:** 14,719× `Scribe follower execution stream failed error=External(NotFound { detail: "the parquet object was not found" })`.
- **Evidence:** staged member paths were relative (`.wyrd/bifrost/scribe-stage/...`), and the Iceberg local-filesystem reader resolves such a path from `/`. The files still existed on disk.
- **Cause:** `BifrostDataRoot::prepare` kept a relative root, so every derived path was relative.
- **Fix site:** `crates/wyrd/wyrd-server/src/boot/data_root.rs` `prepare` now resolves the root once with `std::path::absolute`. Every managed path (WAL, stage, scratch, spill) derives from it, so no caller needs its own fix.
- **Diagnostician:** the read-only agent `notfound-diag` reached the same cause and fix site.

**Slow in-process kill (fidelity gap above) — closed.**
- `OraclePeerGrpc` now holds the server shutdown token and ends open fragment streams with unavailable status.
- `peer_network::analytical::remote_live_scribe_drop_releases_query`: 12.5s, down from 132s.

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| No live-read cap | spec/proto/tonic/redux as above | `wyrd-spec --lib` 897/897; `wyrd-tonic private_conversion` 14/14; redux `scribe::(memtable\|shards\|tail_rpc\|tests::scribe_persistence_path)::\|oracle::(live\|follower)::\|catalog::bifrost_catalog::` 105/105 (PG wrapper); oracle `distributed::*` 7/7 journey | PASS |
| Absolute data root | `boot/data_root.rs` | `boot::data_root::tests::prepare_resolves_a_relative_root_to_absolute_paths` (RED without fix, GREEN with) | PASS |
| Shutdown ends peer streams | `oracle/peer_service.rs`, `grpc/mod.rs` | `peer_network::analytical::remote_live_scribe_drop_releases_query` 12.5s | PASS |
| Contracts | proto `.bin` regenerated | `mise run codegen:check`, `fmt`, `lints`, `git diff --check` | PASS |

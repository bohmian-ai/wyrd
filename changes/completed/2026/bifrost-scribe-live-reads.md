# Best-effort live reads on Scribes

| | |
|---|---|
| Change ID | `SPEC-bifrost-scribe-live-reads` |
| Completed | 2026-10-01 |
| Reviewed target | `14f0312bf` (`vcc/task-004`) |
| Approved specification | revision 24, approved 2026-10-01 |
| Delivery reference | PR #95 (TASK-001–004, merged) and the `vcc/task-004` → `main` follow-up PR (TASK-005–008) |
| Completion authority | **human override** — see "Completion authority" |

## Intent and operator value

Bifrost queries previously chose between published-only, Fused, Strict, and
AllowDegraded behavior and relied on a leased, exact Fused cut with a 30-second
tail-fence protocol. This change gives every caller — SDKs, HTTP, CLI, MCP, and
verification — one query that combines a pinned published cut with live rows
scanned on the online Scribes that hold them, and makes that one path fast,
memory-safe, observable, and measured on a modest real node.

## Shipped externally observable behavior

- One public query contract with no `visibility` or `freshness` field. Oracle
  alone picks Interactive or Analytical from the DataFusion physical root.
  Terminals are `Success`, `Degraded` (`LiveTailUnavailable`, a known live
  source lost before yielding rows), and `Failed`.
- Oracle discovers active streams on ready Scribes, prunes live partitions
  safely, and sends live scan work only to selected owners. Scribes scan their
  own memtable and staged runs and stream bounded Arrow to an Oracle-owned live
  source inside one physical plan; published files keep distributed
  Oracle-worker execution.
- Live coverage is best effort: publication overlap may briefly omit or
  duplicate a row. No query claims to contain every acknowledged write.
- Drift and every other verifier use the same query service and terminals.
- Saturated queries wait in a tenant-fair queue of 1,000 waiters per Oracle
  node, with a one-hour queue limit and a two-hour default total deadline the
  caller may override; a full queue returns a retryable overload.
- Scribe, Oracle, Forge, and in-flight transport share one governed memory
  budget (`WYRD_SERVER_MEMORY_MIN_BYTES`, default 1 GiB server headroom;
  optional `WYRD_BIFROST_MEMORY_LIMIT_BYTES` cap). Pods under 4 GiB refuse to
  boot. Admitted exhaustion returns `WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED`.
- Query parallelism derives from effective CPU and input locality; each query's
  memory limit is half the managed budget (min 256 MiB) with governed spill.
- Scribes read staged runs with the same Parquet scan Oracle uses (statistics,
  bloom filters, page index, pushdown); same-process live batches pass as
  in-memory Arrow.
- The per-row `data_tenant_id` column is gone. Every Parquet file records its
  tenant in footer metadata, checked once at open; a missing or mismatched
  tenant fails with `WYRD_VALA_500_QUERY_TENANT_INVARIANT`. Files without it
  are refused; there is no compatibility path.
- Idle acknowledged rows publish on the Scribe lifecycle clock without new
  writes.
- Production telemetry follows writes, queries, and Forge attempts to their real
  terminal outcomes; Prometheus families come from production owners with no
  shadow state, zero-only signals, or identity labels.
- `mise run bench:bifrost:query-capacity` benchmarks one real release
  `wyrd-server` in an 8-CPU/16-GiB systemd scope through the public Rust client.
  The in-process benchmark harness was deleted.

## Lasting invariants and constraints

- Write ACK, WAL durability, publication order, and Iceberg promotion are
  unchanged by live reads.
- Tenant authority, typed peer assignments, authorized projection, the per-file
  tenant check, and sensitive-column denial hold before source IO and through
  execution.
- One physical plan; no caller chooses a query class or source mode.
- Oracle never opens another pod's WAL or staged files; clients accept results
  only after a valid terminal.
- Every queue, snapshot, in-flight batch, and grant is bounded and query-owned.
- The shared governor is the only memory authority; a buffer is charged once
  while held. Idle roles reserve nothing.
- Telemetry observes owner state and is never a second authority or an extra
  hot-path read, write, or queue.
- Non-goals: Arrow Flight, a durable batch-owner index, a replacement lease,
  Scribe-side aggregate/join pushdown, a public source-selection field, or an
  exact all-acknowledged-write promise.

## Dashboard measurement contract

The six operator questions TASK-005 fixed, with the production families that
answer them. All are pod aggregates; tenant, table, query, and task identities
appear only as scrubbed trace context. Scribe staging gauges are published by
`StagingAssembler` under its lock, including after restart restoration.

| Operator question | Production family {labels} or trace | Unit and meaning | Focused test |
| --- | --- | --- | --- |
| Are writes arriving and getting durable responses? | `bifrost_gate_requests_total{operation,outcome}`, `bifrost_gate_request_duration_seconds`, `bifrost_gate_rejections_total{reason}`; `bifrost_scribe_ack_seconds`; `bifrost_scribe_wal_append_total`/`_bytes_total`/`_seconds`, `bifrost_scribe_wal_fsync_total{outcome}`/`_seconds` | Counts/seconds of request attempts (retries included) and WAL work | `telemetry::scribe_hot_path_telemetry_reconciles` |
| Is new data entering a shard and moving out of memory? | `bifrost_scribe_memtable_rows_inserted_total`; `bifrost_scribe_active_memtable_bytes`, `_immutable_memtable_bytes`, `_immutable_generation_count`; `bifrost_scribe_lane_queued{lane}` (waiting), `_lane_active{lane}` (running) | Rows newly inserted this process; bytes resident; jobs waiting vs running | `telemetry::scribe_hot_path_telemetry_reconciles`, `write_read::acknowledged_rows_survive_stage_pressure_and_restart` |
| Is staging or publication falling behind? | `bifrost_scribe_staging_live_members`, `_live_bytes`, `_oldest_member_timestamp_seconds`, `_outstanding_claims`; `bifrost_scribe_publication_files_total`, `_bytes_total`; `bifrost_scribe_seal_failed_total` | Current backlog from assembler ownership (restored included); committed publication output | `scribe::staging_runtime::pg_tests::restored_stage_republishes_backlog`, `write_read::acknowledged_rows_survive_stage_pressure_and_restart` |
| Is Forge keeping up? | `bifrost_forge_pending_tasks{task_type}`, `_oldest_pending_task_timestamp_seconds{task_type}`, `_active_tasks{task_type}`, `_task_attempts_total{task_type,result}`, `_output_files_total`/`_output_bytes_total{task_type}`, compaction debt; trace `bifrost.forge.task.execute{result}` | Backlog, age, committed attempt result, output | `live_rewrite::forge_promoted_files_rewrite_and_remain_exact_across_recovery` |
| Are clients getting answers promptly? | `bifrost_gate_query_streams_total{outcome}`, `bifrost_gate_query_stream_duration_seconds{outcome}`; trace `bifrost.gate.query.stream` | Server-edge stream terminal and lifetime; request success means stream opened | `published::published_cache_pruning_and_shutdown_are_production_governed` (phase 1b), `write_read::scribe_undialable_private_peer_degrades_live_coverage` |
| Where is query work waiting or failing? | `oracle_queries_queued`, `oracle_queries_active`, `oracle_admission_total{class,outcome,reason}`, `oracle_admission_queue_duration_seconds`, `oracle_query_duration_seconds{class,outcome}` (HPA), `oracle_query_cancellations_total{reason}`, scan files/bytes counters; storage `bifrost_storage_metadata_cache_effects_total{effect,reason}`, `bifrost_storage_requests_total{operation}`, `_request_terminals_total{operation,outcome}`, `_request_seconds`, `_request_retries_total`, `_active_requests`; trace `bifrost.oracle.peer.fragment{role,outcome}` | Waiting vs admitted work, queue wait, Oracle execution by class and true outcome, scan and storage I/O (a cache hit adds no request) | `capacity::saturated_query_waits_on_http_and_grpc`, `published::…`, `peer_network::analytical::remote_live_scribe_drop_releases_query`, `storage::cache::tests::metadata_cache_reconciles_single_flight_identity_and_bypass` |

## Delivery

| Task | Outcome |
|---|---|
| TASK-001 | One published-plus-live query flow; tail-fence protocol removed |
| TASK-002 | Superseded by TASK-003 |
| TASK-003 | Single resource owner, queue and deadline policy, capacity benchmark |
| TASK-004 | Eval server integration and unified Bifrost memory |
| TASK-005 | Truthful, small telemetry; R1 and R2 remediation |
| TASK-006 | Real-server benchmark; process harness deleted |
| TASK-007 | One Parquet scan for staged and published data |
| TASK-008 | Tenant proven per file; per-row tenant column deleted |

## Completion authority

The user approved completion on 2026-10-01 without a final `$wyrd-change-review`.
At that time:

- The TASK-005 R1 review returned `FIX_REQUIRED`. The user overrode it: FIND-5
  (module-top imports) was fixed; FIND-4 (selective benchmark latency) and
  FIND-6 (dashboard draft in the candidate) were rejected.
- The last standard benchmark missed the one-client selective target
  (p50/p95/p99 9.3/12.6/15.5 ms against <7/<10/<10 ms). Benchmark
  qualification is deferred to another PR; the miss is not attributed.
- `mise run gate` was deferred by the user. All 147 Bifrost journeys passed.
- The backlog draft `changes/backlog/bifrost-operations-dashboard/spec.md`
  ships with this branch but is not part of this change.

## Authority

- `architecture/bifrost-design.md` — Fused/live-read, memory, and telemetry
  sections updated by this change.
- `SPEC-verified-change-contract` revision 44, REQ-160/REQ-161 — peer protocol.

## Post-completion amendment

Revision 25 (approved by the user 2026-10-01) replaces pre-accept Oracle peer
reservations with follower grants owned by the leader stream: `ReserveSlots`
is a held stream that admits or refuses on accept, the grant ends with the
stream or the query deadline, and there is no pending reservation, TTL, or
`ReleaseSlots`. AC-013 and INV-008 read accordingly. Delivered by
`SPEC-verified-change-contract` TASK-012 (`3322efcaf`, PR #106); the
authoritative text is `architecture/bifrost-design.md`.

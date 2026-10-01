# Review subject and navigation

Base: `a7582db587c6170a290760f1741673125612b797`. Candidate: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307` (detached immutable HEAD). Review cumulative `git diff HEAD~1`; user limits acceptance to TASK-007 ownership, excluding bundled TASK-006 benchmark/harness migration, lifecycle timer and unrelated lint fixes except blockers.

Authority: spec revision 20 (approved), REQ-013, REQ-014, AC-016; TASK-007-one-parquet-scan-for-live-reads.md; AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, bifrost-design.md, reference router and relevant Rust/DataFusion/Iceberg/reliability/testing references. No `.codegraph` directory exists.

Starting map (expand independently):

- `oracle/follower.rs`: ScribeTailResolver::live_leaf, staged_run_io, in-memory and HotParquetExec composition; tests for pruning, leases and projection.
- `oracle/exec.rs`: HotParquetExec, IcebergParquetReader, scan_predicate_conjunction, row-group/page selection; storage/cache.rs ObjectPin::Staged.
- `scribe/tail_rpc.rs`: live snapshot lifetime/lease, deleted custom decoding; `scribe/staged_tail.rs` deleted; persistence.rs and mod.rs source ownership.
- `oracle/live.rs`: OracleTableProvider::scan, LiveScribeExec partitions, LiveFrameDecoder terminal and memory accounting.
- `oracle/dispatcher.rs`: LiveFrame, ScribeFragmentExecutor local transport, ReservationRegistry::drain_pending; `oracle/mod.rs` shutdown caller.
- `resources.rs` ScribeResources::follower_execution; server `oracle/peer_service.rs` local execution and remote AttemptEncoder.
- `oracle/analytical_supervisor.rs` test-support attempt_counts producer; testing cluster and analytical_activation.rs consumer.
- Three journey corrections: selected_peer_failure_is_terminal, peer_transport_uses_immutable_fenced_destinations, distributed::published_workers_and_live_scribes_share_one_plan. Trace routing through register_cut_providers, OracleRouteTasks, AnalyticalCutTaskCount and LiveUnionBoundary to assess one-worker assertion.
- Cargo.toml/Cargo.lock and redux manifest: iceberg-storage-opendal direct edge (OpenDAL 0.58 versus workspace 0.57); compare existing local-file reader and nonblocking requirements.

Verification constraint: NEVER run Postgres wrappers or full mise lanes. No commits or source edits. Any permitted command must use CARGO_TARGET_DIR inside this worktree. Only orchestrator runs focused non-Postgres tests/clippy to avoid duplicate builds; reviewers inspect source and recorded evidence. Reports belong only in this directory.

Task evidence claims previous full lane 1425 passes/3 failures then exact journey reruns 3 passes; independently assess evidence, do not treat it as proof of diagnosis. Benchmark deferred by explicit task/user scope.

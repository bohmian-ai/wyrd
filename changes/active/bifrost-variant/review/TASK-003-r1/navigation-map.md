# TASK-003 r1 review: subject and navigation map

A starting point only. Expand and verify it against the source. It is
not complete and draws no conclusions.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003`
- Branch: `wyrd/bifrost-variant/TASK-003`
- Base: `b145767dd`. Candidate: `861a0342f`.
- Range: `git diff b145767dd 861a0342f`, 120 files, +8484 / −23488.
- Commits:
  - `4559cff37`
  - `9f2757feb`
  - `029da306f`
  - `0eb07ce57`
  - `861a0342f`
- Fork pins (bohmian-ai, `[patch.crates-io]` in `Cargo.toml`). Each fork's
  local clone lives under `/home/thorrester/Documents/GitHub/`:
  - DataFusion `a4ff14550` (`datafusion`, branch
    `wyrd-arrow60-variant-read-plan`, fork base `38b7a7a0a`);
  - arrow-rs `0d0eb24` (`arrow-rs`, branch `wyrd-unshred-presize`, base tag
    60.0.0 `ef1fa15`);
  - datafusion-distributed `0b81b12` (`datafusion-distributed`, branch
    `wyrd-arrow60`).
- Do not modify source. Do not check out other commits in this worktree. Use
  `git show <rev>:<path>` and `git diff`.

## Authority

- Approved spec: `changes/active/bifrost-variant/spec.md`. The working-tree
  copy is authoritative; it is intentionally uncommitted.
- Latest revision: `changes/active/bifrost-variant/spec-revision-26-datafusion-variant.md`.
- Governing task plan (wins on conflict):
  `changes/active/bifrost-variant/tasks/TASK-003-native-datafusion-read-path-plan.md`.
- Original task and history:
  - `changes/active/bifrost-variant/tasks/TASK-003-shredding-and-leaf-pruning.md`
  - `changes/active/bifrost-variant/tasks/TASK-003-decisions.md`
  - `changes/active/bifrost-variant/tasks/TASK-003-revision-query-latency.md`
- Repository rules:
  - `AGENTS.md`
  - `architecture/agent-rules.md`
  - `architecture/wyrd-design.md`
  - `architecture/bifrost-design.md`
  - `architecture/references/README.md` (router)
- User directives recorded in the plan:
  - Variant logic lives in the DataFusion fork, not in Wyrd.
  - Analytical is rebuilt on native `datafusion-distributed`. Wyrd must not
    invent DataFusion or `datafusion-distributed` logic.
  - Do not bring back the deleted Wyrd analytical scan code.
- Excluded from the candidate by user instruction: `spec.md` and
  `tasks/TASK-004-forge-parallelism.md`.

## Changed surfaces and owners

- **Oracle read path** (`crates/vala/vala-bifrost-redux/src/oracle/`):
  - `exec.rs`:
    - `OracleQueryScanStats`
    - `RemoteScanMetrics`
    - `PruningEvidence`
    - `set_event_time_bounds`
    - `TenantWarehouseReads` (worker plan rewrite: tenant footer-proof
      reader)
    - `record_distributed_scan_metrics`
    - `HotFileSource`
  - `nested_pushdown.rs` (`FileReadPlan`)
  - `live.rs`:
    - `LiveScribeExec::handle_child_pushdown_result`
    - `LiveTableRoutes::selected_by`
    - `fragment()`
  - `analytical.rs`:
    - `WorkerSessionBuilder` / tenant binding
    - `AnalyticalGraphLifecycle::settle`, which now drops the unfolded
      metric fold
    - `AnalyticalGraphRelease`
  - `analytical_transport.rs`
  - `follower.rs`
  - `peer.rs`
  - `codec.rs`
  - `mod.rs`
  - `session.rs` (new: the shared `SessionState` template)
  - `query_stream.rs`
  - `bindings.rs`
- **Deleted:**
  - `oracle/variant_sql.rs`
  - `oracle/leaf_paths.rs`
  - `oracle/analytical_scan.rs`
  - `oracle/pruning.rs`
  - `storage/cache.rs`
  - `storage/telemetry.rs`
  - `benches/nested_field_pushdown.rs`
  - most of `wyrd-spec/src/vala/assignment_authority.rs`
  - parts of `wyrd-spec/src/vala/api.rs`
  - `wyrd-tonic/src/private_conversion.rs`
  - `wyrd-tonic/proto/wyrd.v1.proto`
- **Storage and catalog:**
  - `storage/mod.rs`
  - `storage/error.rs`
  - `catalog/iceberg_storage.rs`
  - `catalog/bifrost_catalog.rs`
  - `parquet/object_uploader.rs`
  - `wyrd-storage/src/{signer,local,telemetry}.rs`
- **Forge:** `forge/{compact,orphan_gc,path}.rs`
- **Server:**
  - `wyrd-server/src/boot/mod.rs`
  - `config.rs`
  - `otlp_json.rs`
  - `otlp_metrics_decode.rs`
  - `verification/publisher.rs`
- **Shared:**
  - `wyrd-queue/src/{variant,batch_builder}.rs`
  - `wyrd-bench/src/lane.rs`
- **Benches:**
  - `benches/bifrost_reads/{main,events,report}.rs`
  - `src/bench_support.rs`
- **Tests:**
  - `wyrd-testing/tests/bifrost/oracle/*`: distributed, published, capacity,
    analytical_activation, analytical_lifecycle,
    peer_network/security.rs, mcp.rs
  - `wyrd-testing/src/bifrost/*`, including the new `object_reads.rs`
  - `wyrd-client/tests/pg_bifrost_e2e.rs`
  - the Python and TypeScript integration tests
  - `vala-bifrost-redux/tests/integration/forge/*`
- **Docs:** `architecture/bifrost-design.md` and the task files.

## Verification already run on the candidate tree

Logs are under
`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-bifrost-variant-task-003/47a7157d-7209-4e02-889a-b40216f53f0b/scratchpad/`.

- `mise run test:bifrost`: every lane passes, `exit=0`, Oracle journeys 53/53
  (`bifrost_full3.log`).
- Step 4 focused journeys: 2/2 (`step4.log`).
- `oracle::` unit tests: 174/174.
- `mise run fmt` and `mise run lints` are clean (`lints.log`). `git diff
  --check` is clean.
- Fork checks (`verify6.sh` / `verify6.log`, `ALL-OK`): fmt, clippy, the
  changed crates' tests with and without `variant`, and sqllogictests.
- LTO benchmarks: `bench/` and the "Step 5 evidence" section of the plan.
- Not run: `mise run gate`, `codegen:check`, `check:client-tier`,
  `check:pyo3-scope`, `check:unwrap-audit`. Check whether the diff requires
  any of these.

## Required report location

Write your report to the exact path you are given, inside
`changes/active/bifrost-variant/review/TASK-003-r1/`.

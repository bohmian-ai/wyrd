# TASK-007 independent maintainer review

Result: **FAIL** (one repository-mandated documentation finding).

Subject: base `a7582db587c6170a290760f1741673125612b797` → candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`, `git diff HEAD~1`. Source remained unchanged. Scope follows the caller's TASK-007 ownership exclusions. This report owns no implementation changes.

Authorities: AGENTS.md §§3–6, 11, 15–16; architecture/agent-rules.md; architecture/references/languages/maintainer-style.md and spec-driven-development.md; Rust, DataFusion and Iceberg references; applicable Bifrost ownership, live-source resource and telemetry contracts; approved spec revision 20 REQ-013/REQ-014/AC-016 and original TASK-007.

## Changed-surface coverage

| Surface | Source and caller tracing | Assessment |
|---|---|---|
| Scribe snapshot and staged-decoder deletion | `FetchLiveTailService::open_live_batches`, `LiveTailBatches::into_parts`, Scribe shard snapshot and staged lease; old `staged_tail.rs` deletion; follower resolver and its fixtures | Snapshot responsibility remains on Scribe; decoding moves to the existing scan. `into_parts` documents the lease-transfer obligation. No replacement decoder or utility abstraction was introduced. |
| Staged Parquet scan | `ScribeTailResolver::live_leaf` → `HotParquetExec` → `hot_stream` → `IcebergParquetReader`/`HotObjectSource`; metadata-cache staged identity and scan/lease tests | Cohesive resolver owns storage and FileIO. Hot scan owns partitioning, cache, range/decode accounting, pruning and lifetime. Lease documentation explains why plan and streams retain it. |
| Memory source and filtering | `MemorySourceConfig`, closure projection and `scan_predicate_conjunction`/`FilterExec`; nonzero-ordinal projection/filter fixture | Reuses DataFusion and the existing closed-predicate compiler. Pure conjunction conversion is appropriately a free function. No generated declaration changes. |
| Local and remote transport | directory routing → `ScribeFragmentExecutor::execute` → `LiveFrame` → `LiveFragmentRead`/`LiveFrameDecoder`; gRPC encoder and decoder tests | The closed enum explains the real runtime distinction. Remote encoding is located at the gRPC owner. Completion and after-footer behavior remain discoverable in the decoder. |
| Parallelism | provider scan → `LiveScribeExec` route distribution; `ScribeResources::follower_execution` → session shape → follower plan | CPU-derived partition policy stays on the resource owner and route distribution stays on the live execution leaf. Docs distinguish physical partitions from distributed task count. |
| Shutdown regression correction | pending reservation owner and activation/release methods; `Oracle::shutdown` drain before admission report; immutable-destination journey | Correction is placed on the reservation owner, with shutdown invoking the lifecycle operation. No consumer-by-consumer workaround. |
| Journey evidence attribution | `AnalyticalSupervisor::spawn_attempt` and `finish_attempt` → supervisor `attempt_counts` → `PeerCluster::attempt_counts` → `attempt_totals` and selected-peer failure journey | Test-only counter is accurately documented as supervisor-local attribution, with admitted and successful counts. It does not stand in for the required production pruning metric. |
| One-worker assertion | distributed journey, `Oracle::register_cut_providers`, `AnalyticalCutTaskCount::handle`, `LiveUnionBoundary`, graph-lease delta evidence | One table is frozen to one published destination. Leader-only live task cap and the documented boundary explain the assertion. Graph leases improve attribution over process-wide body polls; the public filtered result and both Scribe fragment deltas remain checked. |
| Dependency | workspace/crate manifests and base/candidate lockfiles; current ranged reader, governed warehouse adapter, pinned upstream LocalFs/OpenDAL source | Justified existing dependency reuse; details below. |
| Changed test documentation | live frame test and resource-shape test, plus new staged pruning/lease tests | New staged tests document panic conditions; two materially modified existing tests do not. MNT-001. |

## Explicit concern assessments

The attempt counter is a test hook, but the task prohibits a test hook specifically as the proof of staged pruning. Staged pruning is proved through `OracleQueryScanStats`/the production scan metrics. Per-supervisor attempts instead solve an in-process harness attribution problem while preserving the production telemetry registry's bounded labels. Adding a node identifier to process-wide production metrics is not required for this task. Existing `settled_graphs` is the nearby supervisor-local test evidence pattern. No maintainer finding is proposed for this counter.

The one-worker assertion is supported by source, rather than accepted solely from the task diagnosis: `register_cut_providers` selects `destinations[index % len]` per table, and `AnalyticalCutTaskCount` caps the live leaf's stage at one distributed task. `LiveScribeExec` physical partitions are a separate concept. The test retains exact result counts, both Scribe execution deltas, and a peer-socket observation. No maintainer finding is proposed for this assertion change.

`iceberg-storage-opendal` and OpenDAL 0.58.2 already occur in the **base** lockfile, alongside workspace OpenDAL 0.57.0. The candidate adds a direct dependency edge, not a new package universe. The existing `BifrostIcebergStorage` adapter is warehouse-bound and cannot simply be used for arbitrary Scribe local staged paths. Upstream pinned `LocalFsFileRead::read` performs `std::fs` seek/read_exact under a mutex inside its async method (`crates/iceberg/src/io/storage/local_fs.rs:327`); using it would block a runtime worker. The pinned `OpenDalReader::read` awaits the existing ranged-reader API (`crates/storage/opendal/src/lib.rs:1326`). Reusing that adapter is smaller than constructing a new local-storage adapter or restoring the deleted custom decoder. No maintainer finding is proposed for this edge.

## Proposed finding

### MNT-001 — materially modified test items omit required panic contracts

- Classification: **VIOLATION**.
- Governing rule: AGENTS.md §16 explicitly applies rustdoc to tests and requires `# Panics` whenever a panic remains possible; maintainer-style's documentation section adopts that hard criterion.
- Changed locations: `crates/vala/vala-bifrost-redux/src/oracle/live.rs:867`, `live_frames_release_batches_incrementally_and_validate_the_footer`; `crates/vala/vala-bifrost-redux/src/resources.rs:5787`, `scribe_follower_execution_shape_contract`.
- Evidence: both bodies are materially changed for TASK-007 and both contain `expect` and assertion panics. Their rustdoc describes the intended outcomes but has no `# Panics` section. The newly added staged pruning and staged lease tests in `oracle/follower.rs` show the compliant nearby pattern.
- Concrete maintenance cost: the changed test contracts fail the explicit repository documentation gate and omit the setup/execution versus assertion conditions that terminate the test, unlike the adjacent newly added regression proofs.
- Smallest correction: add substantive `# Panics` sections only to these two changed test items, naming fixture/execution construction failure and a violated result/partition/footer assertion. Preserve implementation, assertions, transport, resource ownership and task scope. Do not refactor tests or relax the rule.
- Focused closure proof: inspect the two rustdoc blocks against their bodies; run formatting and `git diff --check`. This documentation-only correction requires no additional runtime lane.

## Verification limits and calibration

This was a source/diff review. No build, Postgres wrapper, full mise lane, source edit or commit was performed. Recorded task test/lint evidence was read but not independently rerun by this reviewer. Build verification belongs to the orchestrator under the caller's concurrency constraints. TASK-006 harness internals, benchmark migration, lifecycle publication timer and unrelated lint fixes were excluded except necessary callers for the three expressly included journey corrections.

No optional refactor, preferred naming or speculative improvement is proposed. The maintainer finding does not imply a runtime defect; acceptance/reliability reviewers separately assess the scan and counter behavior.

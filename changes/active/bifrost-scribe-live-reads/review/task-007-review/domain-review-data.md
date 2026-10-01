# TASK-007 analytical data-domain review

Result: **FAIL**.

Subject: base `a7582db587c6170a290760f1741673125612b797` → candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. The review uses the complete committed diff, scoped to TASK-007 as instructed. Unrelated benchmark, timer, Forge, and lint work is excluded. No source changes, builds, Postgres wrappers, full lanes, or commits were performed.

## Boundary and authority coverage

The boundary is the signed Oracle live assignment → Scribe snapshot → native physical leaf → partition streams → local Arrow or remote IPC → Oracle incremental acceptance. Applicable authorities are approved spec revision 20 (REQ-013/014, AC-016, INV-004/005/006), the original TASK-007, AGENTS.md, agent rules, Bifrost design's durability/visibility and query/resource boundaries, Wyrd protocol/doctrine service ownership, and references `domain/{datafusion,iceberg,olap-serving,arrow-analytical-interop,analytical-operations-reliability}.md` and `languages/{spec-driven-development,maintainer-style}.md`.

| Surface | Source/caller coverage | Assessment |
|---|---|---|
| Source selection and snapshot | `ScribeTailResolver::resolve/live_leaf`, `FetchLiveTailService::open_live_batches`, `LiveTailBatches::into_parts`, hot-source registry leases | Signed schema/closure and stream identity checked before snapshot; only unserved leased runs enter the scan. |
| Shared staged decoder | `HotParquetExec::new/with_partitions/execute`, `hot_stream`, `IcebergParquetReader`, deleted `staged_tail.rs` | Real reuse of the existing scan; custom decoder and per-batch predicate compiler removed. |
| Projection/filter correctness | `hot_projection_mask`, `project_batch`, `scan_predicate_conjunction`, follower `FilterExec`, provider pushdown classification | Projection is name-based; supported conjunction enforced once above both sources. Pruning is not mistaken for exact row filtering. |
| Row-group/page pruning | `row_groups_in_byte_range`, `select_row_groups_for_predicates`, `select_pages_for_predicates`, storage `decode_metadata` | Shared metadata path loads optional page index; disjoint midpoint ownership avoids duplicate groups. Focused staged test observes production row-group counters. |
| Partitions | `OracleTableProvider::scan`, `LiveScribeExec::new/execute`, follower `live_leaf`, `ScribeResources::follower_execution` | Leader and staged scan receive session count; memtable/empty leaves still collapse it (DATA-001). |
| Run lifetime | `HotParquetExec::with_staged_lease`, `hot_stream`, `StagedSourceLease::Drop`, hot-source `drain_leases` | Shared lease held by plan and each stream; publication cannot delete leased local runs. |
| Memory | `HotParquetPlan::Follower`, `reserve_range/own_range/reserve_decoded`, live snapshot bounds, incremental dispatch | Staged encoded ranges and yielded batches use admitted follower pool; no whole-run collect added. Existing Arrow memtable ownership remains bounded by snapshot limits and Scribe source ownership. |
| Transport/terminal | `ScribeFragmentExecutor`, `LiveFrame`, server `execute_physical`, remote `AttemptEncoder`, `LiveFrameDecoder::accept`, `LiveFragmentRead::into_stream` | Native transfer removes IPC/hash; remote encoding stays at gRPC boundary. Native completion loses byte and footer-counter validation (DATA-002). |
| Dependency | Workspace/redux manifests and lock diff; pinned Iceberg `LocalFsStorage`, `storage/opendal`; existing `BifrostIcebergStorage`/`BifrostFileRead`; removed Tokio staged reader | New direct edge is to an already-installed async adapter, not a newly introduced native version universe. See below. |

## Proposed findings

### DATA-001 — sparse and empty memtable leaves retain source-derived partition counts

Classification: **INCORRECT**. Violated obligation: REQ-014 requires the engine's in-memory source to use the session partition count; TASK-007 requirement 3 forbids literal-one leaf partitioning; AC-016 requires the live leaf/follower count to come from the session.

Locations: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:787` and `:849`.

Producer-to-consumer evidence: `resolve` obtains a bounded live snapshot, then `live_leaf` reads `session.config().target_partitions()` at line 783. Its memtable branch immediately reduces the number of groups to `partitions.min(rows.len())`; `MemorySourceConfig::try_new_exec` advertises that group count. A real Scribe containing one acknowledged batch therefore yields one partition even on a session configured for four or more. If publication removes the selected live rows before opening, the empty branch constructs `[vec![empty]]`, again exactly one partition. The staged sibling correctly uses `.with_partitions(partitions)`.

Consequence: physical source partitioning is still a function of source batch count, contrary to the approved CPU/session-derived shape. Existing closure/projection tests prove values but cannot prove the count invariant.

Smallest correction: preserve the session-derived number of in-memory partition groups, including empty groups, in the existing `live_leaf` owner. Preserve name-based projection, filtering, snapshot bounds and staged partitioning. Do not change session policy or introduce another planner/decoder.

Focused closure proof: resolve memtable-only and empty snapshots under a session target greater than one, with fewer batches than target; assert the native memory source advertises the session count and execution returns every selected row exactly once. Reuse follower fixtures. Also retain staged split/group-ownership and closure projection tests.

### DATA-002 — native completion removes byte accounting and footer count validation

Classification: **VIOLATION**. Violated obligation: TASK-007 requirement 4 and REQ-014 explicitly preserve row/byte counters and terminal footer rules while eliminating IPC/hash.

Locations: `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:590` (`LiveFrame::Complete` payload), `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:375` and `:386` (native producer), `crates/vala/vala-bifrost-redux/src/oracle/live.rs:754` and `:758` (consumer).

Producer-to-consumer evidence: the producer yields `LiveFrame::Batch(batch)` and then only `LiveFrame::Complete(scan_evidence.finalize())`. There are no producer row/byte totals or immutable attempt identity in this native completion. `LiveFrameDecoder` increments rows for native batches but never calls `add_bytes`; completion discards its payload and unconditionally marks the decoder complete. By contrast, its remote `WorkerFooter` branch validates fragment identity, completed flag, rows and bytes (and the wire-only digest). Local EOF and after-completion rules remain, but counter/identity validation did not remain. This is a production path selected by the local Scribe directory, not just PeerCluster test code.

Consequence: a nonempty native transfer records zero delivered bytes in the decoder and accepts completion without a producer/consumer row or byte agreement. The current local completion test cannot detect a contradictory count because the completion cannot represent one. Avoiding content hashing does not require removing these inexpensive counters.

Smallest correction: retain checked native batch byte/row totals and immutable attempt binding on the existing producer/completion/decoder path, and validate them before completion is accepted. Use an explicit native Arrow byte measure without serializing or hashing. Keep scan evidence, empty-result completion, incremental delivery, missing/late terminal errors and remote IPC/footer behavior intact; no second transport or public API is needed.

Focused closure proof: local batches produce nonzero checked byte totals, valid empty and nonempty native completions succeed, and contradictory row/byte/attempt completion plus missing/repeated/late completion fail. Keep existing remote footer tests and no-IPC/hash native path inspection.

## Dependency assessment

No dependency finding proposed. The lockfile changes only redux's direct dependency edge: `iceberg-storage-opendal` and OpenDAL 0.58 already existed through the managed compaction core. The pinned native Iceberg `LocalFsStorage` uses `std::fs`, `Read` and `Seek` inside async interfaces and documents itself as a test implementation. Substituting it would block runtime workers. The removed staged reader had a genuine `tokio::fs::File` path, but reusing that as the shared scan's `FileIO` would require introducing another reader variant or storage adapter. `BifrostFileRead` is restricted to a validated warehouse-relative key and cannot directly read staged paths on a separate pod-local staging volume, particularly when the warehouse is remote. The installed async OpenDAL filesystem adapter plugs into the existing `IcebergParquetReader` without recreating the deleted decoder, and exposes Iceberg/Bytes types rather than mixing 0.57 and 0.58 OpenDAL types across an API. A second direct edge alone is not evidence of a native Arrow/DataFusion compatibility defect.

## Verification and limits

Recorded evidence reports a production-counter pruning test, projection/closure and staged lease tests, relevant unit suites, three exact journey reruns, and lint/format passes. These reports were inspected as claims, not rerun here. User constraints prohibit Postgres/full-lane verification; this reviewer performed static trace review only. The staged pruning test proves row-group skipping but not native count/terminal validation or sparse memtable partition shape. Capacity benchmark proof is deliberately deferred by the task/user and is not a finding. No new PyO3/foreign-runtime boundary is introduced.

The scan and staged durability boundary otherwise preserve the existing owners and exact local-run leases. Acceptance remains **FAIL** for DATA-001 and DATA-002 pending independent validation.

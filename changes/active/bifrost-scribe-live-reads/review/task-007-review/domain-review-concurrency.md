# TASK-007 concurrency and resource ownership review

Result: **FAIL**.

Subject: base `a7582db587c6170a290760f1741673125612b797`, candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`, `git diff HEAD~1`. Reviewed only TASK-007 ownership and its three journey corrections. Source remained unchanged.

## Boundary and authority coverage

| Boundary | Source and callers reviewed | Governing obligation |
|---|---|---|
| Session-derived parallelism | `resources.rs::ScribeResources::follower_execution`, `oracle/exec.rs::OracleTableProvider::scan`, `oracle/live.rs::LiveScribeExec::{new,execute}`, `oracle/follower.rs::ScribeTailResolver::{resolve,live_leaf}`, `PhysicalPlanFollower::execute` | Approved spec revision 20 REQ-013, REQ-014; TASK-007 requirements 2–3 |
| Live Arrow handoff and retained memory | `wyrd-server/src/oracle/peer_service.rs::ScribeFragmentExecutor::execute`, local directory dispatch, `LiveFragmentRead::into_stream`, `LiveFrameDecoder::accept`, `exec.rs::{hot_stream,HotParquetGovernance::reserve_decoded,own_range}`; installed DataFusion 55 `CoalescePartitionsExec::execute`, `RecordBatchReceiverStreamBuilder::run_input`, `FilterExec` | TASK-007 requirement 5; spec REQ-005, INV-007/008; Bifrost bounded ownership and cancellation authority |
| Staged lease and cancellation | `tail_rpc.rs::LiveTailBatches::into_parts`, `FetchLiveTailService::open`, `HotParquetExec::with_staged_lease`, `hot_stream`; follower staged tests | INV-004; query-owned source lifetime |
| Reservation cleanup and process shutdown | `ReservationRegistry::{reserve,begin_graph_activation,restore,drain_pending}`, activation rollback/drop, `Oracle::{begin_shutdown,shutdown}`, `OracleWorkerService::reserve`, peer `reserve_slots`, `WyrdServer` supervised transport drain, `Bifrost::{shutdown,drain_selected_owners}` | Preserve shutdown ownership; architecture cancellation stops admission, joins descendants, then releases ownership |
| Distributed worker assertion | `distributed::published_workers_and_live_scribes_share_one_plan`, `AnalyticalCutTaskCount::handle`, `OracleRouteTasks`, `LiveUnionBoundary` | Preserve published-worker + live-Scribe execution in one plan |

Read the task, approved requirements, agent rules, spec-driven development and maintainer guides, reference router, and applicable DataFusion, Arrow ownership, and analytical reliability references. The latter derive authority from Bifrost's query-lifetime, memory, distributed ownership, and structured cancellation rules. Source inspection, not summaries, supports the findings below.

## Proposed findings

### CONC-001 — INCORRECT: memtable parallelism is still batch-count-derived

**Obligation:** REQ-014 explicitly requires the engine's in-memory source to use the same session partition count; TASK-007 requirement 3 eliminates source-count-derived partitioning.

**Location:** `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:787` (`live_leaf`), with mixed-source composition at :854.

**Evidence and reachability:** `live_leaf` reads `session.config().target_partitions()` but builds memtable groups with `vec![Vec::new(); partitions.min(rows.len())]`. A normal Scribe with one live batch and a four-partition session therefore builds a one-partition `MemorySourceConfig`. The shallow snapshot is reachable through authenticated `resolve` → `tail.open` → `live_leaf`. The empty path similarly advertises one partition. For mixed input `UnionExec` concatenates the child partitions; installed DataFusion 55 `union.rs:245–250` explicitly sums their counts. Thus a session with four partitions and four memtable groups plus staged runs yields eight union partitions rather than four. Session configuration alone does not reshape this substituted physical leaf; `PhysicalPlanFollower::decode` substitutes it in an already planned serialized closure.

**Consequence:** memtable scan parallelism varies with snapshot batch count; mixed snapshot execution also varies with the source combination. The current execution-shape test proves the session setting but never checks the resolved source shape. Small live snapshots retain the single-partition path this task expressly removes.

**Minimum correction:** keep `ScribeTailResolver` as owner, allocate exactly the session partition count for the memory source, including empty partitions. Preserve the staged scan's existing session-derived byte tiling. If the combined live source is required to advertise the session count, reuse DataFusion's existing partition-preserving composition rather than a plain concatenating union. Do not change route routing, scan row ownership, or the distributed stage-task cap to compensate.

**Focused proof:** resolve memory-only snapshots with one batch and fewer batches than the session count, and an empty snapshot, and assert source partition count equals the session count. Resolve a mixed snapshot and assert its intended partition shape; execute every partition and check each source row appears exactly once.

### CONC-002 — INCORRECT: staged Arrow bytes lose their charge while batches remain alive

**Obligation:** TASK-007 requirement 5 says memory stays charged to the query grant; spec INV-007/008 requires held bytes to remain governed until ownership ends. REQ-005 requires bounded live resource ownership under backpressure.

**Changed entry point:** `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:827–841` now routes staged reads into the existing parallel `HotParquetExec`. The defective inherited ownership is `oracle/exec.rs:3102–3105`; new direct handoff is `wyrd-server/src/oracle/peer_service.rs:375` and `oracle/dispatcher.rs:594`.

**Producer-to-consumer proof:** `hot_stream` reserves decoded batch bytes, yields a plain `RecordBatch`, and explicitly drops `decoded_reservation` when the producer is next polled. That reservation is a separate local `MemoryReservation`; it is not attached to the returned Arrow arrays. `PhysicalPlanFollower::execute` calls DataFusion `execute_stream`, which coalesces a multi-partition leaf. Installed DataFusion 55 `coalesce_partitions.rs:240–251` launches one producer per partition and `stream.rs:355–377` sends each batch into a channel before polling that producer again. A sent batch can remain queued while the next poll drops its only decoded-byte charge. This is reachable on the normal fully local Scribe session (at least two partitions), including a staged-only scan with no predicates. After the source reaches EOF, previously queued batches can remain alive with no decoded reservation at all. The new local transport then forwards exactly those Arrow buffers; `LiveFrame::Batch` contains no retained charge and the decoder only returns the batch. A slow leader or a caller retaining an emitted batch exposes the mismatch. This finding concerns the staged/local path newly required by TASK-007, not an unrelated redesign of all published reads.

**Sibling consumers:** the same HotParquet owner also serves hot published data; fixes must preserve its leader/follower governance modes. The remote fragment adapter immediately encodes batches but does not cure the coalescing channel ownership gap before encoding. The range-buffer path already demonstrates correct owner coupling (`HotParquetGovernance::own_range` uses `Bytes::from_owner`), whereas decoded output does not.

**Consequence:** live staged buffers can remain resident beyond their reported query charge. Concurrent readers can reuse the released capacity while those buffers remain queued or retained, defeating the cooperative shared cap and the query ceiling. Bounded channel item count does not restore held-byte accounting. A test of lease drop or counter/footer integrity does not prove this obligation.

**Minimum correction boundary:** fix charge ownership where the decoded Arrow buffers are produced, under the existing HotParquet governance owner. Couple the reservation to the buffer lifetime across downstream clones, projection, coalescing, and local Arrow handoff; release it only after the last retained owner ends. Reuse the existing owner-backed buffer principle rather than add a second transport pool or charge the same batch again in the leader. Preserve existing query/follower ceilings, range accounting, cancellation, staged file leases, and remote encoding. Filter-generated output must likewise be governed while held; source-byte ownership alone does not account newly allocated filtered arrays.

**Focused proof:** use a governed follower pool and at least two partitions. Receive and retain a staged batch, drain or drop the producer/stream, and assert decoded-byte accounting still covers the retained Arrow buffers; clone/project the retained batch and show charges release only at final drop. Exercise the actual coalescing path under slow consumption and check query ceiling/refusal remains effective. Existing retained-range checks are not a substitute for decoded-output lifetime.

## Challenged hypotheses not retained as findings

- **Pending reservation drain race:** the registry itself does not close, and reserve/rollback can technically repopulate it after a one-shot clear. However the deployed `WyrdServer` drains supervised transports before invoking Bifrost role shutdown (`app/server.rs:724–751`, followed by lifecycle shutdown; `state.rs:2021–2025`). The Analytical worker is joined before `drain_pending`. I did not establish a production reachable insert-after-drain regression under that ordering. This is not retained as a finding. The new clear correctly releases already pending probe reservations in the reviewed shutdown path; no new registry concurrency protocol is demanded.
- **One leased published worker:** acceptable for this journey's single-table frozen destination. The live leaf's stage-task cap is one; `LiveUnionBoundary` gives published siblings remote boundaries; the stage destination is frozen for that table. A graph-lease delta attributes actual admitted ownership better than a process-global poll delta. The journey still verifies results, both Scribe fragments, Analytical classification, and private-peer traffic. It does not establish general multi-worker parallelism, and TASK-007 does not require that particular fixture to do so.
- **Staged lease lifetime:** the plan and each spawned scan stream retain the `Arc<StagedSourceLease>`. Drop/cancellation releases it when the last owner ends. The single-partition immediate-drop test does not prove task joins are synchronous for multi-partition execution, but DataFusion's coalescing stream owns/aborts its producer tasks; no detached producer was found.

## Verification limits

No builds, Postgres-wrapped tests, full mise lanes, or source edits were performed. Recorded focused pruning/lease and journey results were read as evidence, with no claim of independent execution. Installed exact-version DataFusion source was inspected for partition and buffering semantics. Neither a selective pruning counter nor the three repaired journeys tests retained decoded-buffer accounting or the memtable leaf partition shape. HEAD remained `f7bebf704d6f3b1dd20d041e70c6ca512c0da307` at the end of source inspection.

# TASK-007 invariant review

Overall result: **FAIL**.

Subject: base `a7582db587c6170a290760f1741673125612b797`, candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; full `git diff HEAD~1`, restricted to the caller's TASK-007 ownership. Reviewed independently, without other reviewers' conclusions. No source edits, builds, Postgres wrappers, full lanes, or commits were performed.

Authority: approved spec revision 20 REQ-013, REQ-014, AC-016, INV-004; original TASK-007; AGENTS.md, architecture/agent-rules.md, spec-driven-development and maintainer-style references; applicable Wyrd doctrine and Bifrost durability/visibility, admission, partitioning and terminal contracts. No CodeGraph directory exists.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Staged reads reuse the published/hot Parquet scanner, its pruning and metadata cache | `ScribeTailResolver::live_leaf` constructs `HotParquetExec`; `hot_stream` uses governed `IcebergParquetReader`, cached metadata, row-group and page selection | `scribe_staged_scan_prunes_non_matching_row_groups` observes production scan metrics; task records focused pass | PASS |
| Delete custom staged decoder and per-batch predicate compilation | `staged_tail.rs` deleted; `ScanPredicateFilter` and `retain_signed` removed; one `FilterExec` conjunction is constructed | Diff/source inspection | PASS |
| In-memory rows use the engine source with the same pushdown **and partition count** | `MemorySourceConfig` plus common `FilterExec`; source count is capped by batch count and empty source is hard-coded to one | Projection/filter test covers values but does not assert target count for sparse or empty memory snapshots | FAIL: INV-007-1 |
| Live leader leaf and Scribe session derive partitions from session/CPU rather than route count or fixed one | `LiveScribeExec::new` receives session target; routes are dealt modulo target; `ScribeResources::follower_execution` derives fully-local CPU count | Resource shape test; recorded journeys | PASS (memory-source defect separately above) |
| Local transport carries Arrow without IPC or content hash | Directory dispatches this node's own Scribe to `ScribeFragmentExecutor`; it yields `LiveFrame::Batch`; encoding remains only in remote gRPC handler | `live_frames_release_batches_incrementally_and_validate_the_footer`; server tests recorded | PASS |
| Local row/byte counters and terminal footer rules remain | Local decoder updates rows only; completion carries only scan statistics and is accepted without count reconciliation | Local test explicitly accepts `Complete(WorkerScanStats::default())` after two rows, and never asserts bytes | FAIL: INV-007-2 |
| Remote transport remains encoded and footer validated | gRPC handler alone runs `AttemptEncoder`, wire path validates fingerprint, rows, bytes, digest, completion and rejects trailing frames | Existing wire decoder checks retained in focused test | PASS |
| INV-004: local staged files are read only by owning Scribe | Resolver runs inside authenticated Scribe follower; leader only dispatches signed fragments | Source tracing through `ScribeFragmentExecutor::execute` → `PhysicalPlanFollower::execute` → resolver | PASS |
| Staged lease protects publication/read race and drops with scan/stream | `LiveTailBatches::into_parts` transfers lease into `HotParquetExec`; each stream holds an Arc clone before async reading | New dropped-scan lease test and retained publication lease test | PASS |
| Staged read allocations use query grant | Follower governance captures session memory pool; shared reader reserves fetched/decoded data through that pool | Shared scan implementation and focused tests; no new execution in this review | PASS |
| Three journey failure corrections are diagnosed at source and do not redefine task behavior | Per-supervisor test attempt counts; shutdown drains pending reservations; distributed assertion matches frozen table destination | Recorded traced diagnoses and exact three passing reruns; source paths inspected | PASS |
| No second decoder or unnecessary dependency layer | OpenDAL factory supplies async FileIO to same reader; dependency already in lock graph, only one direct edge added | Manifests/lock and existing storage adapter inspection | PASS |
| Task-scoped verification and deferred benchmark | Task records tests/lints; user excludes full lane/Postgres here and defers benchmark | No new runtime verification; recorded evidence is bounded evidence rather than a fresh run | PASS within caller constraints |

## Producer-to-consumer tracing

Authenticated Scribe fragment claims identify the stream, schema, closure, predicates and ranges before any source is opened. `FetchLiveTailService::open_live_batches` snapshots bounded memtable rows, leases staged sources, excludes sources already served by memory, and returns both kinds to the resolver. `live_leaf` turns them into engine sources. `HotParquetExec::partition_pieces` assigns row-group midpoints to disjoint ranges and retains staged leases through every stream. `PhysicalPlanFollower::execute` captures scan metric handles before execution and executes this actual substituted plan; it does not rebuild its leaf partitioning to repair the counts below.

Local execution bypasses IPC in the directory, but shares the same verified Scribe executor with remote execution. That executor yields Arrow and a completion enum. The gRPC consumer reconstructs a counted footer through `AttemptEncoder`. The leader-local consumer does not reconstruct or reconcile equivalent count evidence: its local branch accepts bare completion. The remote branch retains its prior footer checks.

## Proposed findings

### INV-007-1 — INCORRECT: memory source changes session parallelism based on batch count

Violated obligation: REQ-014 explicitly requires the engine in-memory source to use the same session partition count; TASK-007 requirement 3 excludes fixed-one partitioning.

Location: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:787` and `:849`; mixed source assembly at `:854`.

Evidence: `partitions` comes from session at :783, but memory groups are `partitions.min(rows.len())`. For a valid session target of four and one nonempty memtable batch, the produced memory leaf advertises one partition. A valid empty read similarly builds exactly one group. With one memory group and a four-partition staged source, `UnionExec` concatenates the child partitions rather than preserving four matching partitions. No physical optimization in `PhysicalPlanFollower::execute` changes this source contract: it executes the decoded substituted plan.

Consequence: valid sparse/empty memory cuts retain the retired one-partition shape; mixed cuts have a source-count-derived execution shape. Current shape test checks the session, not the actual memory or composed source.

Minimal correction: keep the in-memory source at the session target even with fewer batches (empty groups are valid), and retain that target in the mixed composition using existing DataFusion partition composition rather than allowing batch count/union-child count to define it. Preserve the existing staged scan, disjoint row-group ownership, shallow snapshot, common filter and lease ownership. No custom decoder or new execution framework is needed.

Focused proof: build actual resolver plans at a target above one for empty, one-batch memory-only, staged-only and mixed snapshots; assert the relevant leaf/composed source partition count equals that target and drain all partitions to prove exact-once rows and signed filtering. Reuse existing resolver fixture and engine sources.

### INV-007-2 — MISSING: local completion drops byte counts and counted-footer reconciliation

Violated obligation: REQ-014 / TASK-007 requirement 4: removing IPC/hash must preserve row and byte counts and terminal footer rules.

Locations: `crates/vala/vala-bifrost-redux/src/oracle/live.rs:754-760`; `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:598-603` (`LiveFrame`); `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:375-385` (local producer).

Evidence: local `Batch` only calls `add_rows`; it never calls `add_bytes`. Local `Complete` contains only `WorkerScanStats`, so it cannot carry a producer's row/byte totals, fragment identity or completion assertions. Decoder accepts it unconditionally, whereas wire footer at live.rs:791-800 reconciles those fields. The same actual Scribe executor feeds both consumers: remote gRPC runs `AttemptEncoder` and recovers this evidence; local leader has no equivalent counted completion. This is a production reachable path because directory always selects local Scribe when node/role match.

Consequence: successful process-local live reads no longer maintain their byte counter or reconcile the delivered row/byte totals with terminal evidence. The test proves EOF/completion ordering only, and actually makes acceptance of count-free completion explicit.

Minimal correction: at the existing local producer/decoder boundary, preserve cheap checked row/byte accounting and a completion carrying the corresponding totals and fragment identity, and compare those totals before accepting completion. Use direct Arrow byte accounting, with no IPC or content hash. Keep remote wire protocol and `AttemptEncoder` unchanged; preserve omission/error-after-output/cancellation/trailing-frame behavior. This is an internal counted terminal repair, not a new public protocol.

Focused proof: extend the existing decoder test to assert nonzero byte accounting and agreement for valid local completion, rejection for altered row/byte totals or fragment identity, missing completion and frames after completion; keep wire cases and verify the local path never invokes IPC/hash.

## Explicitly assessed concerns with no finding

- **Attempt counter:** `AnalyticalSupervisor::attempt_counts` is indeed test-support instrumentation, but it attributes already existing production metric events to a supervisor when multiple simulated pods share one metrics process. `AnalyticalAttemptTelemetry::start/finish` remain the production metric owners. TASK-007's prohibition on a pruning test hook is met by the production scan metrics used in the pruning test, and does not ban all test-support attribution. No production node identity metric label is required for this harness concern.
- **One leased worker:** `OracleRouteTasks` binds every task of a remote-bearing stage to its frozen destination. `AnalyticalCutTaskCount` caps the live leaf at one leader task; `LiveUnionBoundary` isolates published siblings. This test has one table/source destination and a live union. Exactly one published worker activation matches this concrete plan; the prior per-worker metric assertion confused process-global observations with worker attribution. It still verifies peer IO, exact results and both Scribe fragments.
- **Dependency:** using `iceberg-storage-opendal` adds a direct edge, not a new resolved OpenDAL version: 0.58 was already required by compaction. Existing `FileIO::new_with_fs` uses a blocking filesystem backend; existing `BifrostIcebergStorageFactory` delegates warehouse-key reads to the configured storage backend and cannot open arbitrary pod-local staged paths when warehouse storage is cloud-backed. The new narrow async filesystem factory avoids custom local FileIO adaptation and does not duplicate the Parquet decoder. No blocking dependency finding is supported.
- **Shutdown drain:** pending reservations have no activated graph; draining them after refusing new work returns capacity without replacing active graph settlement. This closes the concrete restart report gap.

Verification limits: fresh runtime proof was intentionally not run by this reviewer. Recorded full-lane results included three failures later rerun successfully; that does not cover sparse/mixed source partition shape or local counted completion. Benchmark evidence remains explicitly deferred by the caller.

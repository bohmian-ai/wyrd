# TASK-007 independent behavior review

Overall result: **FAIL**.

Subject: base `a7582db587c6170a290760f1741673125612b797` to candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; complete `git diff HEAD~1`, restricted to TASK-007 ownership per the caller. Approved spec revision 20, REQ-013, REQ-014, AC-016, INV-004, and original TASK-007. TASK-006 migration, unrelated lint changes, and lifecycle publication timer are excluded except the three specifically requested journey repairs.

## Coverage and paths

Read the review skill, original task and recorded diagnoses, approved requirements, repository agent rules and maintainer/spec-development authorities, Bifrost durability/live-source architecture, and the DataFusion, Iceberg, reliability and testing references. No CodeGraph index exists. Source tracing covered OracleTableProvider → LiveScribeExec → transport directory → ScribeFragmentExecutor → PhysicalPlanFollower → ScribeTailResolver → FetchLiveTailService → leased staged runs / memtable cut; HotParquetExec reader, scan metrics and lease ownership; local decoder and gRPC encoder; supervisor attempt attribution; pending-reservation shutdown; and distributed provider/task-count routing and its repaired journey.

No source edits, builds, tests, Postgres commands, full lanes, benchmark, or commits were performed by this reviewer. The orchestrator reports `git diff --check` passed and focused non-Postgres nextest compilation is ongoing. Task-recorded previous focused tests and exact journey reruns are evidence supplied with the candidate, not independent execution here.

## Obligation matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Shared Parquet scan for staged runs; delete custom decoder/per-batch compile | `follower.rs:800-843` constructs HotParquetExec; staged_tail.rs removed; tail_rpc.rs no longer compiles filters; shared conjunction above source | `scribe_staged_scan_prunes_non_matching_row_groups`; staged reader source inspection | PASS |
| Selective staged reads skip nonmatching groups/pages using node metadata cache | Hot scan delegates metadata acquisition and row-group/page selection to existing scan mechanisms; ObjectMetadataKey::staged identifies leased file | Focused test asserts 1 matching/2 pruned groups; page selection is shared with existing hot scan tests | PASS |
| In-memory source, same signed pushdown | MemorySourceConfig and one FilterExec over source/union, name-based projection | `scribe_provider_projects_and_filters_a_nonzero_ordinal` and source tracing | PASS |
| In-memory/live follower source uses session partition count, independent of row/file/route counts | ScribeResources computes CPU/locality shape; LiveScribeExec advertises session partitions; in-memory branch instead caps groups by batch count and empty branch creates one | Existing shape contract checks session configuration but not resolved in-memory plan shape | **FAIL — BEH-007-1** |
| In-process Arrow, no IPC or content hashing | directory selects own-node Scribe; executor yields LiveFrame::Batch; only gRPC encodes | Local decoder unit test; remote encoder retained | PASS |
| Preserve row/byte counters and terminal/footer validation | Local Batch counts rows only; local Complete carries scan stats only and is accepted unconditionally | Decoder test checks completion and frames afterward, but no local byte total or terminal-total contradiction | **FAIL — BEH-007-2** |
| Remote transport unchanged | gRPC translates Batch/Complete through existing AttemptEncoder and wire decoder still checks schema, fingerprint, rows, bytes and digest | Peer service tests and source tracing | PASS |
| INV-004: Scribe opens its own files, memory under follower grant | resolver is constructed in Scribe executor; run paths come from local leased snapshot; HotParquetPlan::Follower uses session pool | Staged lease tests and existing live journeys | PASS |
| Selected peer failure remains terminal, correctly attributed in in-process harness | per-supervisor attempt totals replace process-total sampling; same production attempt telemetry remains | Exact failing journey reportedly rerun successfully; source producer/consumer tracing | PASS |
| Immutable fenced destinations survive restart/shutdown probe | Oracle shutdown drains pending reservation owner after refusing/draining work; no activation retry or destination refreeze introduced | Exact failing transport journey reportedly rerun successfully | PASS |
| Published and live inputs remain one plan; repaired worker assertion reflects deployed route | one frozen destination per table; LiveUnionBoundary separates published sibling; live stage remains leader-owned | grouped row results, exactly one worker lease delta, socket-body polling and both Scribe fragment deltas | PASS |
| Focused pruning proof uses existing production measurement | OracleScanMetricsHandle supplies scan evidence; test asserts it rather than a test-only pruning counter | Focused pruning test source | PASS |
| Benchmark deferred, no widened task scope | Caller explicitly defers benchmark until TASK-008 | Not run here | PASS |

## Proposed findings

### BEH-007-1 — VIOLATION: in-memory partition count still depends on snapshot batch count

- Obligation: REQ-014 requires engine in-memory rows to use the same partition count; TASK-007 requirement 3 forbids fixed-value live partitioning.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:787` and `:849` (`ScribeTailResolver::live_leaf`).
- Producer-to-consumer: FetchLiveTailService snapshots the valid local cohort; into_parts returns its Arrow batches. live_leaf computes the session count, then creates only `min(partitions, rows.len())` memory groups. MemorySourceConfig advertises the supplied groups as its output partitions. A valid memory-only snapshot of one batch in an eight-partition session therefore resolves to one partition, and the empty snapshot also explicitly creates one partition. The returned source is the plan substituted into the follower execution, so setting the session shape correctly does not repair this source.
- Consequence: live memtable execution remains tied to input batch count and can stay single-partition despite the approved CPU/session shape. The empty-source branch also contradicts the advertised session partition contract. Mixed snapshots union this undersized source with the correctly partitioned Parquet source.
- Smallest correction: reuse MemorySourceConfig with the session-sized group vector, keeping unused partitions empty, including the empty-source case. Preserve signed projection/filtering, one snapshot, staged leases and file midpoint assignment. No new source type, configuration, dependency or task router is needed.
- Closure proof: focused resolver test with target partitions 8 and memory-only cohorts containing 0, 1 and fewer than 8 batches; assert the in-memory source advertises 8 and collecting all partitions returns each row exactly once under the signed filter. Retain existing staged pruning and follower session-shape tests.

### BEH-007-2 — MISSING: local frames dropped the byte counter and terminal-total checks

- Obligation: REQ-014 and TASK-007 requirement 4 explicitly retain row/byte counters and terminal footer rules while removing IPC and hashing.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/live.rs:754-760`; `oracle/dispatcher.rs:600` (`LiveFrame::Complete`); `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:384` (completion production).
- Producer-to-consumer: the directory now always sends this node's Scribe through ScribeFragmentExecutor. That producer forwards Arrow and ends with only WorkerScanStats, which contains physical scan evidence, not delivered-row/byte totals or fragment completion identity. LiveFrameDecoder adds local batch rows but never local bytes; it discards Complete's payload and marks completion without comparing any counters. The remote sibling still compares footer row_count/encoded_bytes/fingerprint/completed/digest against delivered frames. This is a reachable production branch, rather than dormant fixture behavior.
- Consequence: local live reads have no delivered-byte count and no completion-counter agreement check. The new unit test demonstrates that arbitrary default scan stats suffice to close a two-row local result; it cannot prove the retained terminal-count rule. EOF and after-completion checks survived, but they cover only part of the required footer behavior.
- Smallest correction: keep the existing local Batch/Complete transport shape, count Arrow bytes and rows at the local producer and decoder, and carry the corresponding lightweight completion identity/totals for agreement validation; retain the existing scan evidence. No IPC, content digest or serialization should return to this path. Reuse the existing fragment identity that has already been authenticated instead of introducing an identity surface. Preserve gRPC AttemptEncoder and the wire footer untouched.
- Closure proof: extend the existing decoder test to assert local bytes/rows, reject contradictory completion totals/identity, accept matching empty/nonempty completion, and retain missing-completion/after-completion behavior. The producer completion and decoder must use the same documented Arrow byte-count convention without encoding work.

## Requested diagnosis checks

The test-support attempt pair is a test hook, but this review found no task violation merely from its existence. The task's explicit production-metric condition governs the staged-pruning proof, which uses OracleScanMetricsHandle. A production attempt metric already exists (`AnalyticalAttemptTelemetry`), and the new pair provides per-supervisor attribution unavailable from the process-wide registry without node identity labels. It does not change failure or retry policy. No replacement production metric is required by TASK-007's approved acceptance.

The one-leased-worker assertion is correct for this query: register_cut_providers selects one destination for this single table; the live leaf is capped at one leader task, and its stage cap reaches the aggregate. The revised assertion also retains actual result counts, private peer-plane evidence, and both remote Scribe fragment deltas. Requiring two worker leases would demand a different scheduling contract, not validate the present one.

The new direct iceberg-storage-opendal edge does not add a new OpenDAL version to the lockfile: 0.58 was already present through managed compaction. Existing BifrostIcebergStorage uses the configured warehouse owner and explicitly refuses absolute paths outside its warehouse; staged paths are pod scratch authority and cannot simply be routed through that adapter. The pinned Iceberg local factory's blocking implementation is not an equivalent safe async reader. The direct edge to the already-resolved FS adapter is therefore justified for this scoped outcome; forcing warehouse storage to accept scratch paths would weaken its existing authority boundary. No dependency finding proposed.

The task contains explicit diagnoses for all three repaired journey failures. The changed assertion is not a silent removal of proof; source routing corroborates its corrected expectation. Those repairs do not close the two source-level gaps above.

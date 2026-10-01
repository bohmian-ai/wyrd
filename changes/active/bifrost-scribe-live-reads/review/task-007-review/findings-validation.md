# TASK-007 independent structured validation

Result: **FIX_REQUIRED**. Six bounded findings remain. No new specification or architectural decision is required.

Subject: base `a7582db587c6170a290760f1741673125612b797` to candidate `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`, cumulative `git diff HEAD~1`. HEAD was checked and remains that candidate. Scope is the user's TASK-007 ownership: Oracle/Scribe scan, partitions, native transport, decoder deletion and the three specifically included journey repairs. Adjacent TASK-006 migration, lifecycle timer and unrelated lint changes are excluded. No source edits, builds, tests, Postgres commands, full lanes or commits were performed by this validator.

Inputs: both task reviews, standards, maintainer, system, concurrency and persistent-data domain reviews; claim comparison; focused follow-up; navigation and verification reports; approved spec revision 20, original task, repository rules, maintainer/spec-development references and applicable DataFusion, Iceberg, Arrow ownership and Bifrost authorities. Every proposed finding below was checked against source rather than accepted by agreement. There are no prior stable FIND IDs to preserve.

## Proposal dispositions

| Discovery proposal | Disposition | Independently validated outcome |
|---|---|---|
| BEH-007-1, INV-007-1, DATA-001, CONC-001 | REVISED | FIND-007-1: sparse/empty memory source count is incorrect. Broader demand to force the composed union root to N partitions is not retained. |
| BEH-007-2, INV-007-2, DATA-002, SYS-007-01 | CONFIRMED | FIND-007-2: native bytes and counted terminal reconciliation are absent. |
| CONC-002, focused follow-up | REVISED | FIND-007-3: retained staged output loses its charge; correction is bounded to live scan/output ownership, including predicate output, rather than a generic DataFusion rewrite. |
| REPO-007-1 | CONFIRMED | FIND-007-4: newly introduced/materially changed Rust dependency declarations violate explicit import/signature rules. |
| REPO-007-2 | REVISED | FIND-007-5: the packet omits exact reproducible recipes. No claim is made that historical runs did not occur, and prohibited reruns are not required during this review. |
| MNT-001 | CONFIRMED | FIND-007-6: two materially changed test items omit required panic documentation. Standards' broad rustdoc PASS is superseded for these two items by direct inspection. |

Rejected hypotheses are not findings: node-local attempt attribution must become a new production metric; this single-table journey must lease both candidate workers; OpenDAL 0.58 is newly introduced by this dependency edge; pending reservations can repopulate after deployed transport/worker drain; coalescing cancellation must synchronously release every producer in the same poll. None is established by the required deployed path or approved authority.

## Final deduplicated ledger

### FIND-007-1 — REVISED — INCORRECT: memory sources collapse the session partition count

Discovery: BEH-007-1, INV-007-1, DATA-001, CONC-001. Obligation: REQ-014's engine in-memory source uses the same partition count, TASK-007 requirement 3, AC-016.

Location: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:787` and `:849`, `ScribeTailResolver::live_leaf`.

Producer and consumer: authenticated `resolve` obtains one bounded snapshot from `FetchLiveTailService::open`, calls `live_leaf` with the follower session, and substitutes the returned plan in `PhysicalPlanFollower::decode`. At :783 the owner reads the target partition count. At :787 it reduces it to `min(target, snapshot batch count)`, while the empty branch supplies exactly one group. `MemorySourceConfig` advertises the number of supplied groups. For target four and one live batch, this reachable memory source advertises one; target four with no selected rows also advertises one. The staged sibling correctly uses `HotParquetExec::with_partitions(target)`. `execute_stream` consumes the substituted plan, rather than replanning to fix its memory source.

Consequence: sparse/empty memtable sources retain the one-partition shape explicitly retired by the task. The session-shape test does not inspect the resolved memory source.

Smallest correction: keep `ScribeTailResolver` as owner and use exactly the session-sized number of existing `MemorySourceConfig` groups, leaving unused groups empty, including empty snapshots. Preserve round-robin batch ownership, closure projection, compiled signed filtering, staged byte tiling and lease ownership. No new planner, source, dependency or session policy is necessary.

Scope resolution: a mixed `UnionExec` sums its children's partition counts (locked DataFusion `union.rs:245–250`). REQ-014 requires the memory and staged sources to use N and names the leader live leaf/follower session; it does not require every operator in the decoded physical fragment to advertise N. Aggregates/coalescers themselves legitimately advertise other counts. Once both input sources use N, union's ordinary 2N enumeration does not change either source's N-way data ownership. Requiring a new repartition above that union would add work without independently establishing a violated obligation. The retained finding and proof concern actual input source counts, not all operator roots. `InterleaveExec` is also not a drop-in replacement: the locked constructor requires consistent hash/range partitioning, which these sources lack.

Focused closure proof: resolve empty, one-batch and fewer-than-N memory snapshots at a target above one; assert the memory leaf's count is N and drain all partitions to prove exact-once filtered values. In a mixed snapshot inspect both input sources' counts and exact-once combined output. Retain the staged pruning, projection and session-shape checks. Do not serialize scans to obtain a simpler proof.

### FIND-007-2 — CONFIRMED — MISSING: native frames discard byte totals and counted completion

Discovery: BEH-007-2, INV-007-2, DATA-002, SYS-007-01. Obligation: REQ-014 and TASK-007 requirement 4 explicitly preserve row/byte counts and terminal footer rules; INV-004/006 retain terminal validation.

Locations: `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:598–603` (completion payload); `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:364–386` (producer); `crates/vala/vala-bifrost-redux/src/oracle/live.rs:754–760` (consumer).

Producer and consumers: the directory dispatches its own Scribe to `ScribeFragmentExecutor` in production as well as tests. That executor authenticates immutable request/assignment identity, runs the follower, forwards each batch and completes with only `WorkerScanStats`. It never accumulates output row/byte totals. The local decoder counts rows but never bytes; `Complete(_)` ignores its payload and accepts completion unconditionally. The remote sibling runs `AttemptEncoder` and retains its schema, counted rows/bytes, fingerprint and digest footer checks. Physical scan statistics cannot substitute for delivered-output totals. Local EOF and after-completion refusal remain and are not claimed broken.

Consequence: every nonempty native fragment records no delivered-byte count and has no producer-to-consumer row/byte/fingerprint terminal agreement. The current native test accepts default scan statistics after two rows and cannot represent contradictory completion totals.

Smallest correction: retain the existing executor/frame/decoder boundary. Accumulate checked output rows and native Arrow byte totals at the producer, carry those totals and the already authenticated fragment fingerprint with its native completion, and independently reconcile them at the decoder before closing the fragment. Select the existing batch Arrow memory-size measure (`RecordBatch::get_array_memory_size`) as the documented native-byte convention on both sides; this is transfer accounting, not a second memory ledger. Native bytes are not IPC bytes. Keep finalized scan evidence and explicit terminal ordering. The completion variant itself denotes successful completion; do not add a second terminal state machine. Identity comparison belongs at the decoder because it owns the fragment's successful-terminal acceptance boundary. No IPC/content hash, new public wire schema or retry is required. The gRPC adapter still produces its existing encoded/footer accounting through `AttemptEncoder`.

Focused closure proof: valid empty/nonempty native completions reconcile rows, nonzero native bytes and fingerprint; altered totals/fingerprint fail, and missing/repeated/late completion remains refused. Exercise the producer's emitted completion through the decoder, not only hand-built valid frames. Keep existing remote schema/footer cases. Inspect the local path to ensure no IPC/hash returns.

### FIND-007-3 — REVISED — INCORRECT: queued or retained staged Arrow outlives its memory charge

Discovery: CONC-002 and focused follow-up. Obligation: TASK-007 requirement 5, REQ-011 (held buffer counted once across ownership transfer), INV-005/006/008. The concurrency report's INV-007/008 shorthand does not replace the more precise retained-buffer authority above.

Changed entry: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:827–841`; defective inherited ownership: `oracle/exec.rs:3102–3105`; consuming direct transport: `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:375`, `oracle/dispatcher.rs:594`, `oracle/live.rs:754–756`.

Source proof: `HotParquetGovernance::reserve_decoded` creates a separate reservation. `hot_stream` yields a plain `RecordBatch` and drops that reservation on its next poll. It is not an owner of the Arrow allocation. `PhysicalPlanFollower::execute` calls `execute_stream` at :1473. Locked DataFusion 55.1.0 `execution_plan.rs:1772–1789` wraps a multi-partition root in `CoalescePartitionsExec`; `coalesce_partitions.rs:240–251` spawns a producer per input, and `stream.rs:355–377` sends its batch into the receiver before polling again. The queued batch can therefore be unconsumed when its charge drops; EOF can release every reservation while the receiver retains decoded buffers. A staged-only no-predicate snapshot with the normal CPU-derived follower count (at least two) suffices. No fault injection or unbounded queue is needed.

Downstream/siblings: `ScribeFragmentExecutor` forwards those same buffers as `LiveFrame::Batch`; the decoder and name projection return them without attaching retained ownership. A batch, projection, slice or array clone can outlive the producer/stream. Remote encoding is later than the coalescing receiver, so does not repair that queue. Hot published scans share the governance owner and must retain their existing mode and range behavior. The old Scribe source was single-partition and encoded locally before its next pull; its imperfect next-poll convention does not excuse introducing an independently buffered, parallel/native staged path under the explicit retained-memory requirement.

Consequence: the pool releases capacity while live staged allocations remain resident, permitting other queries to reuse that capacity and defeat the cooperative finite cap/query ceiling. Item-bounded buffering is not retained-byte accounting.

Smallest safe correction boundary: use the existing follower memory pool and `HotParquetGovernance` allocation ownership boundary to make decoded staged-buffer charges follow their actual Arrow owners through clones, slices, projection, asynchronous source queues and native handoff. `own_range`/`PooledRangeOwner` already demonstrate retained-owner charging for ranged bytes; Arrow's locked allocation-owner facility supports the same principle without a new dependency. Do not add a transport pool, process ledger, speculative reservation, stream-lifetime reservation list, or leader duplicate charge. A terminal/transport wrapper is too late to cover the receiver and cannot govern raw array clones.

Preserve the same ownership for newly allocated signed-predicate results before they can escape into buffering. The candidate's `FilterExec` calls `filter_record_batch` and retains new arrays in its batch coalescer (`filter.rs:1309–1310`); ownership of input buffers does not cover copied output buffers. Keep this correction within the existing live source's scan/filter ownership, reuse the already compiled signed predicate and engine filtering semantics, and account newly owned buffers once at their production/retention boundary. Do not audit or rewrite unrelated operators, change the query memory root, remove pruning, restore the staged decoder, reduce session parallelism or return to IPC. This selects ownership coupling at the producer rather than compensating guards at each downstream consumer. Private helper/fixture layout is intentionally unconstrained.

Focused closure proof: a governed follower staged plan with at least two partitions executes through actual `execute_stream`; queued output remains charged under slow consumption and a finite ceiling still refuses growth. Retain a received batch and projected/array clone, drain or drop the producer, and show that held decoded allocations remain charged until the final allocation owner drops. Repeat for a signed predicate producing copied arrays. Check cancellation and range-owner regressions. These assertions must distinguish decoded charges from metadata/range charges; the 18 current passes do not test this property.

### FIND-007-4 — CONFIRMED — VIOLATION: new dependencies bypass module imports

Discovery: REPO-007-1. Authority: `architecture/agent-rules.md` expressly puts imports at module top and requires bare imported type names in fields, parameters, returns and bounds; the tests' module exception does not permit test-function imports.

Verified changed locations: follower.rs:693, :695, :718, :745–746 and :3178–3179; exec.rs:2779 and :2844; tail_rpc.rs:351–353 and :375–376; server peer_service.rs:235; analytical_supervisor.rs:393. The new `scan_predicate_conjunction` and added test helpers likewise need their introduced/materially modified signature types covered by the owning module imports. Qualified constructors/value expressions are not part of the prohibition.

The pruning test introduces function-local type imports although its module already has an import block. New storage/FileIO/lease fields and split/stream/executor returns use qualified types. The attempt field adds qualified atomic types. These are actual changed declarations, not a request to clean the repository's older declarations.

Consequence: TASK-007's new dependency surface violates the mandatory module dependency manifest. No runtime defect is asserted.

Smallest correction: move only TASK-007's new function-local imports to their test-module import block and import the types used by owned new/materially changed declarations in the owning module. Preserve type identity, feature gates and behavior. Do not rewrite unrelated existing declarations or add abstractions.

Focused closure proof: static inspection of the owned declarations/import blocks, formatting and scoped Clippy; existing scan/frame regression checks. No additional runtime test is warranted for import placement.

### FIND-007-5 — REVISED — VIOLATION: task evidence omits exact focused recipes

Discovery: REPO-007-2. Authority: AGENTS.md §11 and spec-driven-development's Test command precision. Location: `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md:71–95`.

Direct inspection: Evidence names six concrete regression checks but supplies only test names, a module-regex fragment, wildcard server suite and aggregate lane. Diagnosis names three exact journey reruns but its only expression is the placeholder `test(=…)`. The packet supplies no complete package/target/features/selector/environment command for these claims. `verification.md` truthfully records a current 18-test module selection and scoped lint pass; it does not supply the packet's required per-named-test historical recipes.

Consequence: a maintainer cannot reproduce each claimed focused result directly from this task artifact. This is missing reproducibility evidence, not evidence that the tests were not run or failed.

Smallest correction: add complete current exact recipes for every specifically named check, identify the actual owning targets/features and setup wrapper, and separate recoverable historical execution evidence from newly executed or deferred checks. Recover authentic prior commands/results where available; run only permitted focused non-Postgres checks if new proof is needed. If Postgres historical command evidence is unavailable, record that limitation and defer execution to the authorized integrated caller, rather than fabricate a previous run. Recording a future reproducible Postgres recipe does not authorize running it here. No full lane, benchmark or replacement unit-only journey proof is required.

Focused closure proof: every named check has an exact reproducible recipe and truthful dated/outcome attribution; no literal selector placeholders or invented pass claims. Existing unit evidence and recorded journey claims remain distinguished. The current user's test/concurrency restrictions control execution.

### FIND-007-6 — CONFIRMED — VIOLATION: two changed tests lack panic contracts

Discovery: MNT-001. Authority: AGENTS.md §16 explicitly includes private tests and requires `# Panics` when a panic remains possible; agent-rules makes it a hard documentation criterion.

Locations: `crates/vala/vala-bifrost-redux/src/oracle/live.rs:867`, `live_frames_release_batches_incrementally_and_validate_the_footer`; `crates/vala/vala-bifrost-redux/src/resources.rs:5787`, `scribe_follower_execution_shape_contract`.

The actual diff materially expands native frame assertions and changes the resource shape test. Both function bodies contain construction/execution `expect` and result assertions. Both rustdoc blocks omit `# Panics`. The new follower staged regression tests supply substantive nearby examples. Standards' general documentation PASS does not rebut these precise items.

Consequence: the changed proof items fail the repository's mandatory documentation requirement; this is not a runtime finding or a stylistic preference.

Smallest correction: add substantive panic sections to these two changed tests describing failed fixture/execution construction and violated footer/partition/result assertions. Preserve test behavior and scope. Static body/doc comparison, formatting and whitespace checks close it; no new runtime suite is needed.

## Requested dispositions and diagnosis checks

**Attempt hook: acceptable.** `AnalyticalSupervisor::spawn_attempt` and `finish_attempt` retain existing `AnalyticalAttemptTelemetry::start/finish` at the actual admit/settle sites. The feature-gated pair observes those same outcomes per supervisor; `PeerCluster::attempt_counts` and `attempt_totals` attribute the coordinator's selected attempt rather than summing leader/followers in one registry process. The task's production-metric condition specifically governs staged pruning, which uses production `OracleQueryScanStats`, not this pair. Replacing it with a new node-identity production metric is neither necessary nor consistent with the bounded metric-label authority. Qualified field types remain covered by FIND-007-4.

**Exactly one published worker: acceptable.** `register_cut_providers` freezes one destination per table; `OracleRouteTasks` routes a source-bearing stage's task slots to that destination. `AnalyticalCutTaskCount` caps `LiveScribeExec` at one leader task; `LiveUnionBoundary` keeps published sibling execution remote. Physical N-way partitions and distributed stage task counts are distinct. The single-table journey still verifies exact aggregate rows, Analytical classification, private peer-plane polls and one fragment on each Scribe, now using per-worker graph leases. Requiring two leased workers would change the existing routing contract. The recorded independent diagnosis is corroborated by those producer/consumer paths, rather than merely by the passing assertion.

**Dependency edge: justified.** The base lockfile already resolves `iceberg-storage-opendal`/OpenDAL 0.58 alongside workspace 0.57; the diff adds redux's direct edge only. At the pinned revision, Iceberg `LocalFsFileRead::read` (`local_fs.rs:327`) locks a standard file and performs seek/read synchronously inside its async method. Existing warehouse `BifrostIcebergStorage` rejects absolute locations outside its configured warehouse (`catalog/iceberg_storage.rs:110–135`); local staged scratch can be separate from a cloud warehouse. Deleted staged-tail Tokio IO did not provide the shared scan's required FileIO adapter. The installed OpenDAL reader awaits ranged IO (`storage/opendal/src/lib.rs:1326`), fits that exact existing Iceberg boundary and is smaller than creating another local adapter/decoder or weakening warehouse path authority. No native Arrow/DataFusion duplicate cone is introduced.

**Shutdown diagnosis: supported.** Pending reservations hold unactivated capacity. Deployed server transport drain precedes Bifrost role shutdown, worker shutdown precedes `drain_pending`, and clearing those entries drops the owning envelopes before admission reporting. No reachable concurrent reinsertion after that lifecycle ordering was established. Do not add another shutdown protocol to solve an unproven race.

## Reconciled acceptance and evidence

| Obligation | Outcome | Evidence / limit |
|---|---|---|
| Shared staged Parquet reader, metadata/pruning; delete custom decoder and per-batch compilation | PASS | Resolver uses HotParquetExec/IcebergParquetReader; staged_tail.rs removed; production staged pruning test passed in the current 18-test set. Shared page selection remains installed. |
| Engine memory source and same signed projection/pushdown | PASS for values; FAIL for required count | Existing projection test passes; FIND-007-1 covers sparse/empty sources. |
| Leader live leaf and follower session CPU/session policy | PASS | LiveScribeExec receives session count; ScribeResources derives local CPU policy; current shape test passes. Actual memory source count remains FIND-007-1. |
| Native Arrow without IPC/hash; remote unchanged | PASS for transport | Directory/executor carry Arrow; encoding remains at gRPC adapter; current decoder wire/native cases pass. |
| Preserve native row/byte/terminal rules | FAIL | FIND-007-2; existing native test proves presence/order only. |
| Local staged authority and lease protection | PASS | Scribe authenticated resolver owns file opens; plan and streams retain lease; current lease tests pass. |
| Held live decoded allocations stay charged to query pool | FAIL | FIND-007-3, actual locked coalescing path; present tests omit retained ownership. |
| Three journey fixes explained at the source | PASS as static diagnosis | Existing production telemetry, reservation lifecycle and single-table routing corroborate each diagnosis. Recorded exact rerun passes are supplied evidence, not new runs here. |
| Import/rustdoc repository compliance | FAIL | FIND-007-4 and FIND-007-6, explicit rules and changed source. |
| Reproducible task test evidence | FAIL | FIND-007-5; truthful recipe repair needed. |
| Scoped verification | PASS within execution restrictions | Orchestrator recorded 18 focused nextest passes, scoped Clippy and diff whitespace pass; unavailable Postgres/full-lane/benchmark reproduction is explicitly excluded/deferred. |

The required independent reports and focused follow-up are present and complete. The memory finding resolves the earlier source-level memory PASS assumptions without broadening task ownership. The packet's recorded full-lane results are not a reason to rerun forbidden lanes; all current permitted checks passed. Passing them does not close the six uncovered acceptance/standards gaps. The retained corrections use existing owners, native engine/Arrow facilities and unchanged approved behavior, so this is bounded remediation, not SPEC_REVISION_REQUIRED or BLOCKED.

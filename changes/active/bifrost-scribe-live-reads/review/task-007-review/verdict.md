# TASK-007 verdict

**FIX_REQUIRED** — independently validated findings FIND-007-1 through FIND-007-6.

## Immutable subject and authority

- Repository: this detached review worktree.
- Base: `a7582db587c6170a290760f1741673125612b797`.
- Candidate: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.
- Cumulative diff: `git diff HEAD~1`.
- Approved authority: [spec.md](../../spec.md), revision 20, REQ-013, REQ-014, AC-016 and applicable invariants; [original TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md); repository rules and applicable architecture/references documented in the reports.

HEAD and tracked source remained unchanged. No implementation edits or commits were made. Although the snapshot bundles other task work, the user's explicit scope controls this audit: Oracle/Scribe live scan, partitions, in-process transport, staged decoder deletion and the three identified journey corrections. TASK-006 benchmark/harness migration, lifecycle timer and unrelated lint work are excluded except necessary caller evidence. Their presence does not independently fail this bounded review.

## Independent review results

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Analytical data correctness | [domain-review-data.md](domain-review-data.md) | FAIL |
| Concurrency/resource ownership | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Focused memory-lifetime follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

All required roles used separate fresh agents and all reports are present. [Claim comparison](claim-comparison.md) explains the follow-up: one reviewer uniquely established a decoded-buffer path through asynchronous coalescing. The follow-up resolved that path; final validation independently traced it again. The validator also reconciled standards' broad rustdoc PASS with two precisely identified omissions. No unresolved source/authority conflict remains.

## Reconciled acceptance matrix

| Obligation | Implementation and proof | Result |
|---|---|---|
| Shared staged Parquet scan, metadata cache, row-group/page pruning; delete custom staged decoder and per-batch compilation | Resolver uses HotParquetExec/IcebergParquetReader; staged_tail.rs deleted; existing production pruning evidence and current focused unit pass | PASS |
| Engine in-memory source with same signed projection/filter | Existing engine source/conjunction and nonzero-ordinal value test | PASS for values |
| Engine memory source uses session count, including sparse/empty snapshots | Batch-count minimum and empty one-partition branch contradict REQ-014; current session-shape proof does not inspect actual source | FAIL, FIND-007-1 |
| Leader live leaf and Scribe session use CPU/session count | LiveScribeExec receives session target; follower resource owner derives local CPU policy | PASS, subject to memory-source gap |
| Native Arrow without IPC/hash, remote unchanged | Local directory/executor carry Arrow; gRPC alone encodes through existing AttemptEncoder | PASS |
| Native row/byte totals and terminal agreement remain | Native producer completion contains scan stats only; decoder never counts bytes or reconciles completion totals/fingerprint | FAIL, FIND-007-2 |
| Scribe-local staged authority and source protection | Authenticated follower owns opens; HotParquet plan/streams retain staged lease; lease unit checks pass | PASS |
| Held live Arrow remains charged to query grant | Shared decoder releases reservation on next poll while parallel coalescing/local handoff retain its batch | FAIL, FIND-007-3 |
| Three journey fixes preserve diagnosed behavior | Node attribution, pending reservation shutdown and single-table frozen destination traced to source | PASS as static diagnosis; recorded journey reruns not reproduced |
| Mandatory module imports/signature declarations | Changed declarations/function-local imports violate explicit agent rules | FAIL, FIND-007-4 |
| Exact reproducible named-test recipes | Task packet supplies names/placeholders without full commands | FAIL, FIND-007-5 |
| Changed test panic documentation | Two materially changed proof items omit mandatory panic sections | FAIL, FIND-007-6 |
| Current permitted verification | 18 focused unit passes, scoped Clippy with warnings denied, whitespace pass | PASS within caller constraints |

Both memory and staged input sources must use the session count; the approved requirement does not require every composed operator root to advertise that count. The proposed extra mixed-union repartition was therefore rejected, not packaged as remediation.

## Validated finding ledger

Only independently confirmed or revised findings enter this verdict. Full source traces, exact sites, discovery IDs, corrections and closure proofs are in [findings-validation.md](findings-validation.md).

| ID | Status / class | Defect | Required correction boundary |
|---|---|---|---|
| FIND-007-1 | REVISED / INCORRECT | Sparse/empty memtable source collapses session partitions | Existing resolver's MemorySourceConfig group construction |
| FIND-007-2 | CONFIRMED / MISSING | Native bytes and counted/fingerprinted completion agreement absent | Existing Scribe executor, native completion and leader decoder |
| FIND-007-3 | REVISED / INCORRECT | Retained staged/filter Arrow buffers outlive their query charges | Existing live scan/allocation production ownership, before asynchronous buffering |
| FIND-007-4 | CONFIRMED / VIOLATION | New dependency declarations bypass module imports | Owned changed Rust declarations/import blocks |
| FIND-007-5 | REVISED / VIOLATION | Named-test task evidence lacks exact recipes | Original task evidence, with truthful historical/deferred attribution |
| FIND-007-6 | CONFIRMED / VIOLATION | Two changed tests lack panic contracts | The two existing rustdoc blocks |

No prior task review/FIND ledger was supplied for this candidate, so there are no prior stable findings to close. The three recorded journey diagnoses were assessed and accepted, but they do not close these uncovered gaps.

## Requested concerns

- **Attempt counter:** acceptable test attribution. Existing production attempt telemetry remains; the explicit production pruning-metric requirement is met by existing scan metrics. A new production node-label metric is unnecessary.
- **One leased worker:** correct for this single-table frozen-destination plan. The journey retains results, both Scribe execution checks and peer evidence. Physical scan partitions and distributed stage task counts differ.
- **OpenDAL dependency:** justified reuse. Both versions already existed in the base lockfile; the candidate adds one direct edge. Pinned Iceberg's native local reader blocks, while warehouse-bound storage cannot generally open Scribe scratch paths. The installed async filesystem backend fits the shared reader without adding another decoder.

## Verification limits and delivery

[verification.md](verification.md) records exact current commands and results: 18/18 permitted unit checks, scoped redux test-support Clippy, and `git diff --check`. Postgres wrappers, full mise lanes and benchmark were not run, honoring the concurrent TASK-008 constraint. Three journey rerun passes are recorded candidate evidence and were statically assessed, not independently reproduced. Benchmark qualification remains the caller's later integrated obligation.

One bounded task is ready for `$wyrd-implement`: [TASK-007-R1-close-live-scan-gaps.md](TASK-007-R1-close-live-scan-gaps.md). It preserves approved behavior and introduces no new public, architecture, concurrency or persistent-data decision. This review implements, merges and publishes nothing.

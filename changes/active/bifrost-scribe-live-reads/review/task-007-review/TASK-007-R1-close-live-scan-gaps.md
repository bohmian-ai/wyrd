---
id: TASK-007-R1
title: Preserve session partitions and governed native live output
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-013, REQ-014, REQ-011, INV-004, INV-005, INV-006, INV-008]
acceptance: [AC-016]
parent_task: TASK-007
remediates: [FIND-007-1, FIND-007-2, FIND-007-3, FIND-007-4, FIND-007-5, FIND-007-6]
depends_on: []
---

# TASK-007-R1: close validated live-read gaps

Route directly to `$wyrd-implement`. This task corrects the existing approved outcome, not a new design.

## Subject and authority

- Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20.
- Original task: `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md`.
- Reviewed base: `a7582db587c6170a290760f1741673125612b797`.
- Reviewed candidate: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.
- Verdict and independently validated diagnosis: this directory's `verdict.md` and `findings-validation.md`; memory-specific trace: `followup-review.md`.
- Ownership: Vala Oracle/Scribe live source, shared scan governance and native frame owners; server Scribe executor; original task evidence and narrowly changed Rust documentation/imports.

## Diagnosis and selected corrections

### FIND-007-1: actual memory source shape is not the session shape

REQ-014 requires the in-memory source to use the same session partition count. `ScribeTailResolver::live_leaf` in `oracle/follower.rs:787` uses `min(session target, snapshot batch count)`; its empty branch at :849 supplies one group. A valid four-partition follower with one memory batch therefore executes a one-partition memory source. `PhysicalPlanFollower` substitutes that source without replanning it. The passing session-shape test checks configuration, not the actual leaf.

Use the existing resolver and MemorySourceConfig with exactly the session target number of groups, retaining unused empty groups and exact-once batch assignment, including empty cuts. Preserve signed filtering/projection and staged byte tiling. Mixed UnionExec may enumerate both N-partition children as 2N partitions: the approved requirement governs each source, not every composite operator root. Do not add a union repartition, new source, planner or partition policy.

### FIND-007-2: native completion lost required output evidence

REQ-014 preserves row/byte counts and terminal rules while deleting IPC/hash work. `ScribeFragmentExecutor` (`peer_service.rs:364–386`) emits Arrow then only `WorkerScanStats`. `LiveFrame::Complete` (`dispatcher.rs:598–603`) carries no output totals/fingerprint. `LiveFrameDecoder` (`live.rs:754–760`) counts rows only and accepts that completion unconditionally. The remote sibling still constructs/validates its counted wire footer. Current native tests cannot express contradictory producer totals.

Keep those existing owners. The producer must accumulate checked delivered rows and native bytes and complete with those totals, finalized scan evidence and its already authenticated fragment fingerprint. Use `RecordBatch::get_array_memory_size` consistently as the native byte convention on producer and decoder; it is transfer accounting, not a new memory ledger or IPC-size claim. The decoder independently counts/reconciles those values before accepting successful completion because it owns terminal acceptance. The completion variant itself denotes success. Preserve missing/trailing/repeated-frame rules and failure classification. Keep the gRPC AttemptEncoder and wire/footer protocol unchanged. Do not restore IPC/content hashing or introduce a public schema, identity authority or second terminal state machine.

### FIND-007-3: asynchronous retained buffers outlive their charge

TASK-007 requirement 5 and REQ-011 require held memory to stay charged once through ownership transfer. `hot_stream` (`exec.rs:3102–3105`) yields a plain decoded RecordBatch and drops its separate decoded reservation on the next producer poll. The new staged entry at `follower.rs:827–841` has at least two normal session partitions. `PhysicalPlanFollower::execute` uses DataFusion `execute_stream`, whose locked coalescing producers send a batch into their receiver then poll the source again. The decoded reservation can drop while that unconsumed batch remains queued. Direct local Arrow transfer/projection/cloning extends those same buffers beyond producer completion. An item-bounded queue does not restore byte charging. The prior single-partition/IPC Scribe path did not have this independently buffered staged flow. Passing lease and pruning tests do not exercise it.

Correct retained ownership at the existing live scan/allocation production boundary before batches enter asynchronous coalescing. Use the existing follower pool and HotParquetGovernance ownership, following the existing `own_range`/`PooledRangeOwner` principle and Arrow's installed allocation-owner facilities. Decoded staged allocations must retain their charge through clones, slices, projection, buffering and native transfer, releasing it only with the final allocation owner. Preserve shared HotParquet leader/follower modes and range charges.

The signed predicate path also produces copied arrays through FilterExec/filter_record_batch and retains them in a coalescer. Its new output allocations need the same governed retained ownership at their production/retention boundary. Reuse the existing compiled signed predicate and engine filtering semantics within the live source's scan/filter ownership; do not restore per-batch compilation or another staged decoder. Governing only initial input arrays leaves copied output uncovered. This is a live output preservation requirement, not an audit/rewrite of all DataFusion operators.

A transport-only wrapper is too late for the source receiver and cannot cover raw array clones. A stream-held reservation list releases too early for retained clones and too late for freed batches. Do not add another transport pool, global ledger, predicted future-byte reservation, leader duplicate charge, serialization or reduced parallelism. Retain query ceilings, pruning, source leases, cancellation and remote transport behavior. These are existing approved resource semantics; no new budget or concurrency decision is needed.

### FIND-007-4: mandatory dependency declarations are bypassed

Agent rules require top-of-module imports and bare imported types in declarations. New/materially modified TASK-007 fields/returns use qualified paths, and the new pruning test imports types inside its function despite an existing module import block.

Validated sites include `follower.rs:693,695,718,745–746,3178–3179`, `exec.rs:2779,2844`, `tail_rpc.rs:351–353,375–376`, `peer_service.rs:235`, and `analytical_supervisor.rs:393`; include introduced signature types in the owned predicate helper/test helpers. Move these declarations' types into their owning module imports and test-function imports into the test-module import block. Preserve type identity, feature gates and behavior. Qualified value/constructor expressions remain allowed; do not clean unrelated old declarations or add abstractions.

### FIND-007-5: task evidence cannot reproduce its named checks

Original TASK-007 Evidence/Diagnosis (:71–95) names focused checks and three traced journey reruns but omits full package/target/features/setup commands, using names, fragments or `test(=…)`. This violates the exact-command artifact rule without proving historical execution failed or never happened.

Add complete reproducible current recipes for every named check and truthful outcome attribution to the original task evidence. Recover authentic historical commands/results where available. Distinguish historical claims, newly executed checks and deferred/unavailable execution. Do not invent past results. A recorded future Postgres recipe does not authorize executing it under this concurrent-review restriction. Existing current `verification.md` records 18 unit passes and scoped Clippy; it is corroboration rather than a substitute for exact per-named-test packet recipes.

### FIND-007-6: two changed proof items omit panic contracts

AGENTS §16 explicitly requires panic documentation for changed tests. `live.rs:867` native frame test and `resources.rs:5787` follower shape test contain construction/execution expects and assertions, but their materially changed rustdoc lacks `# Panics`. Add substantive sections describing those conditions; retain assertions and behavior. This static correction needs no new runtime test.

## Constraints and preserved behavior

- Preserve approved revision 20, server/Scribe-local authority, authenticated closure/schema/projection, query grant, no local IPC/hash, existing remote wire protocol, valid terminal acceptance, failure/degraded semantics and cancellation.
- Preserve shared staged Parquet pruning/cache, staged lease lifetime, write ACK/WAL/publication behavior and exact-once row-group ownership.
- Keep the accepted supervisor-local test attribution alongside existing production telemetry. Keep the corrected single-table one-worker journey assertion and pending reservation shutdown correction.
- Keep the justified installed async OpenDAL filesystem backend. No new dependency, public API, persisted state, scheduler, source selection or telemetry authority.
- Exclude TASK-006 benchmark/harness work, lifecycle timer, TASK-008 tenant-footer changes and unrelated lint/refactor debt. Do not revert concurrent work.
- No commits. While TASK-008 concurrency restrictions remain active, never run Postgres wrappers, full mise lanes or benchmark; use a worktree-local CARGO_TARGET_DIR. Do not weaken/ignore tests, suppress gates or reduce partitions to make proof pass.

## Acceptance criteria and ordered proof

For changed executable behavior, use the repository's scenario-by-scenario Red-Green-Refactor discipline and record exact selected tests and outcomes. Reuse existing owner fixtures; private helper/test layout is the implementer's choice.

1. **FIND-007-1:** empty, one-batch and fewer-than-N memory snapshots at N > 1 produce N memory partitions. Mixed snapshots show N partitions for each source. Drain all partitions and verify exact-once signed-filtered results; no extra union-root constraint.
2. **FIND-007-2:** actual producer completion passes through the decoder for empty/nonempty output and reconciles rows, nonzero native bytes and fingerprint. Altered row/byte totals/fingerprint fail; missing/repeated/trailing completions remain refused. Existing wire checks remain green; local source inspection shows no IPC/hash.
3. **FIND-007-3:** actual governed multi-partition staged execution through execute_stream keeps queued output charged under slow consumption and enforces a finite query ceiling. Retain received Arrow plus a projection/array clone, drain/drop the producer, and prove decoded charges persist to final allocation drop. Repeat for signed-predicate copied output. Distinguish output charges from metadata/range charges. Preserve cancellation and existing range-owner behavior.
4. **FIND-007-4:** owned new/materially changed declarations use top-level imported types, test imports are at module scope; formatting/scoped Clippy pass without unrelated cleanup.
5. **FIND-007-5:** every named proof in the packet has an exact complete recipe and truthful historical/new/deferred attribution; no selector placeholders or fabricated outcomes.
6. **FIND-007-6:** the two changed test rustdoc blocks explain actual panic conditions; body/doc inspection and formatting/whitespace pass.

Static-only import, documentation and evidence edits do not require manufactured RED failures. New behavior scenarios must fail for the diagnosed gap before correction, then pass with related regressions.

Focused existing regression recipes (through mise, with worktree-local CARGO_TARGET_DIR; set a writable MISE_STATE_DIR when necessary):

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::follower::tests::scribe_provider_projects_and_filters_a_nonzero_ordinal)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=resources::tests::scribe_follower_execution_shape_contract)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=oracle::live::tests::live_frames_release_batches_incrementally_and_validate_the_footer)'
```

Include and run exact commands for new selected proof tests once their names exist; confirm selectors select tests. Broaden only to affected non-Postgres owner-module checks, scoped Clippy with warnings denied, formatting and `git diff --check`. Server/real-runtime journey checks and full format/lint gates remain required integrated evidence when the caller permits their environment; record any currently blocked proof truthfully. Do not run the full Bifrost lane or capacity benchmark for this task; the caller owns those after TASK-008. Reassess the cumulative corrected candidate against the original task in a later independent review.

# TASK-004 focused follow-up: cross-revision generation capture

Result: **RESOLVED**.

Immutable subject: `3e7c59db7...c4dd61029a54c44017c63bd4ff469472c406bc13`; HEAD remained the candidate. This report investigates only the conflict between CONC-02 in `domain-review-concurrency.md` and the allocator assessment in `task-review-invariants.md`. It does not determine the task verdict or validate other proposed findings.

## Resolution

The collision is reachable. The allocator is monotonic across keys within one running memtable, but its counter is not restored globally. Its recovery high-water lookup examines only the requested full `SealKey`. Recovery can restore an old revision's staged authority without allocating any memtable generation. A subsequent first freeze under another revision of the same physical partition can therefore receive the old authority's ordinal. Capture then compares a lossy `(PhysicalPartition, GenerationOrdinal)` pair and incorrectly skips the old staged source.

The invariant report's observation describes an uninterrupted process correctly. It does not establish uniqueness between generations restored from a previous process and newly allocated generations under a different seal key. Existing global allocation does not prevent CONC-02's restart schedule.

## Authority and source coverage

Original task `tasks/TASK-004-shared-table-storage.md`, scenario 3, requires each accepted row once through active/immutable/staged/published/restart states and retained metadata. Approved spec revision 12 REQ-038 and AC-013 require evolution/recovery without disappearing records. `architecture/bifrost-design.md:395–411` requires every selected source row identity once and coherent captures; lines 561–575 establish staged durability before publication and revision-bearing assembly identity.

All source below is under `crates/vala/vala-bifrost-redux/src/`. I read the relevant cumulative diffs and complete relevant allocation, freeze, selection, registry capture and restoration function bodies, expanding the original map specifically to test whether startup replay secretly advances the counter:

| Boundary | Source inspected | Evidence |
| --- | --- | --- |
| Identity dimensions | `catalog/layout.rs::LayoutRevision`; `catalog/physical_partition.rs::PhysicalPartition`; `scribe/seal_key.rs::SealKey` | Revision contains schema/sort IDs; physical partition contains spec/time/typed tuple, with no schema/sort revision. Sort-only evolution can keep physical partition identical while changing `SealKey`. |
| Fresh lane and allocator | `scribe/shards.rs:1010–1033`; `scribe/memtable.rs::{new_with_config,ordinal,allocate_seal_id,register_hot_source,freeze,freeze_all_nonempty}` | Each fresh lane starts counter 1; allocation reads only `highest_generation(key, lane)`; restoring authorities does not update that atomic. All ordinary freeze routes use this allocator. |
| Registry identity and recovery | `scribe/hot_source.rs::{register_memtable,restore_durable,highest_generation}`; `scribe/staging_runtime.rs::{restore,restore_authorities}` | Registry keys include revision. `highest_generation` calls `state.get(key)` rather than scanning the lane. Recovery reconstructs full old `key.seal_key()` and durable ordinal, registering staged authority without invoking the memtable allocator. |
| Durability before publication | `scribe/persistence.rs::{persist_once,stage_member,advance_manifest}`; `scribe/staging_runtime.rs::{register_member,advance_authority}` | A member can be Ready/StagedRun and the manifest watermark advanced while no publication claim exists. Staging is sufficient to advance replay watermark. |
| Startup ordering and replay exclusion | `scribe/mod.rs::replay_wal_async`; `scribe/persistence.rs::resume_staging_claims`; `scribe/replay.rs::{sealed_lsn_map,ReplayAccumulator::commit}`; `scribe/shards.rs::{prepare_replay_chunk,prepare_replay_state,resolve_replay_fence}`; `scribe/memtable.rs::{restore_replayed,insert_replayed_frozen}` | Startup restores staged authorities, resumes outstanding claims, then replays WAL. A Ready member without an outstanding claim is not forcibly published by `resume_staging_claims`. Manifest-covered slices never reach generation reconstruction. Thus a valid staged-only restart need not increment `next_seal_id`. |
| Production reader | `scribe/tail_rpc.rs::capture`; `scribe/memtable.rs::{select_sources,FrozenMemtable::readable_source,ReadableSource::capture,LiveCapture::take,LiveCapture::read}`; `scribe/hot_source.rs::capture_staged` | Production tail capture selects non-Durable immutable memory, projects revision away, passes only `(partition, ordinal)` membership, and skips any matching staged generation without comparing revision. |

No CodeGraph directory exists. No source implementation was changed, and no Cargo, database, load or runtime command was run.

## Exact controlled schedule

Use one table, one shard lane 0, a currently open time partition P, unchanged partition spec/typed values, and a small member below publication target. Keep it below claim age/pressure triggers and crash the process rather than invoking a graceful drain.

1. In process A, accept rows A under revision R. Freeze the first generation in lane 0: `allocate_seal_id(K_R)` receives candidate 1 and the registry has no prior K_R authority, so `(K_R, (0,1))` is registered.
2. Allow `persist_once` to finish staging and `advance_manifest`. The staged record is Ready, the registry authority is StagedRun, and the durable replay watermark covers A. Do not run a publication claim for this member. This is an ordinary supported intermediate state, not a failed write.
3. Commit a sort-only revision R2 through the task-authorized catalog seam. Schema and physical partition P can remain unchanged. Crash/restart with the retained root before the Ready member becomes claim-owned/published. Alternatively commit R2 after startup; the capture schedule is unchanged.
4. In process B, the new lane's atomic counter starts at 1 (`memtable.rs:310`). `replay_wal_async` first restores `(K_R, (0,1))` as StagedRun (`staging_runtime.rs:709–763`). `resume_staging_claims` resumes existing claimed/publishing work, not every Ready member. The watermark suppresses A's slices in `ReplayAccumulator::commit` (`replay.rs:673–680`), so old A does not allocate a replacement memtable generation. Select a root with no other unsealed WAL work; no incidental replay advances the counter.
5. Accept fresh rows B under R2 in the same P and lane. Freeze before completing its persistence. `allocate_seal_id(K_R2)` obtains candidate 1. `highest_generation(K_R2,0)` returns 0 because only K_R exists. Registration succeeds: the full keys differ. The registry now has old `(K_R,(0,1)) -> StagedRun` and new `(K_R2,(0,1)) -> Memtable`.
6. Capture before B's staging completes and before any publication begins. `select_sources` includes B's pending immutable generation. `FrozenMemtable::readable_source` stores P and `(0,1)`, discarding R2. `LiveCapture::take` consequently creates `served = {(P,(0,1))}` (`memtable.rs:1746–1750`).
7. `capture_staged` iterates old K_R and finds that pair in `served` (`hot_source.rs:916–923`), so it continues before constructing a staged source for A. Iteration over new K_R2 also finds the pair and leaves B in memory. There is no unsettled publication, so capture succeeds. `LiveCapture::read` returns B's memory rows but no staged runs for A. A has no published object in the pinned cut, so those acknowledged rows are absent from the result.

This schedule depends on a short but controllable pending-persistence window for B. It requires no simultaneous allocator race, counter wraparound, corrupted record, synthetic load, or partition-spec collision. Once B becomes Durable, memory selection excludes it and old staged A may reappear on a later capture. Existing captures retain their already-selected incomplete source set.

The manifestation above is sufficient to resolve reachability. The report does not independently assert that the converse Published/superseded variant survives every cleanup schedule; it is unnecessary to establish this loss.

## Correction boundary and proof

Preserve the existing full `SealKey` in immutable readable-source identity and the served/superseded membership exchanged with `capture_staged`. Tenant/table remain capture scope; schema/sort revision must remain part of equality alongside physical partition and generation. Reuse the registry's existing key, locks, lease owner and publication barrier. No second allocator or durable global counter is required. Broadening the allocator's restart high-water scan would impose additional global ordinal uniqueness to compensate for a consumer that erased an already available identity; the current source model only promises an ordinal within its seal key/lane.

A focused in-module test can restore old-revision staged `(0,1)` via the existing registry API, use a fresh registry-attached memtable to freeze new-revision rows in the same P, assert its ordinal really is 1, then capture and assert both the memory rows and old staged run are selected exactly once. Exercise the full-key superseded comparison using the existing publication authority API without changing publication sequencing. A narrow integration restart proof can additionally pin the watermark/Ready-state preconditions; it is not necessary to invent another runtime or registry.

No new proposed finding is added. This evidence resolves CONC-02's disputed producer-to-consumer path for subsequent independent validation.

Skipped unrelated findings and runtime verification. The identified loss remains a static source-derived schedule; no numerical availability or performance claim is made.

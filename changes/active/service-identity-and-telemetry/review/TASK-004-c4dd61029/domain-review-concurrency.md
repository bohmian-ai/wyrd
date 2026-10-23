# TASK-004 concurrency domain review

Overall: **FAIL**.

Subject: complete cumulative range `3e7c59db7...c4dd61029a54c44017c63bd4ff469472c406bc13`. HEAD remained the candidate during this static review. No source changes or Cargo commands were made. Other review reports were not read.

## Boundary and authority coverage

| Boundary | Authority | Source and assessment |
| --- | --- | --- |
| Admission versus revision commit; accepted retry | Approved spec revision 12 REQ-038, AC-013; supplemental “Physical execution and evolution”; TASK-004 scenario 3 | `ingress.rs::resolve_logical_frame`, `admit_transport_frame`, `prepare_and_dispatch`; `preprocess.rs::{prepare_append,PreparedAppend::logical_identity,LogicalBatchDigest}`; `shards.rs::{committed_duplicate_rows,write_group,batch_commit_identity,commit_group}`; `vala-sql/queries/scribe_batch_commits.rs::{resolve,record}`. Accepted revisions reach downstream storage, but retries resolve latest metadata before accepted identity: CONC-01. |
| Current-revision shared catalog seam | TASK-004 boundary and scenario 3 (“New admission uses committed revision”; TASK-005 owns public edit orchestration); architecture Bifrost table identity | `bifrost_catalog.rs::{committed_revision,evict_committed_revision,registration,table_registration}`; test seam `wyrd-testing/src/bifrost/revisions.rs::commit_table_revision`. Cache publication can race invalidation: CONC-03. |
| Complete revision/partition authority through seal, staging, recovery, live read | Architecture Bifrost tables, Scribe, Oracle source authority; TASK-004 scenarios 2–3 | Full diffs and surrounding source in `seal_key.rs`, `physical_partition.rs`, `wal.rs`, `replay.rs`, `assembly.rs`, `member_stager.rs`, `hot_stage.rs`, `hot_source.rs`, `memtable.rs`, `staging_runtime.rs`, `persistence.rs`, `write_recipe.rs`, `claim_publication.rs`. Seal/assembly keys carry revision; coherent capture drops it: CONC-02. |
| Fenced publication and retry | Architecture Scribe publication, Forge scheduling/fences and promotion | `file_list_writer.rs::{insert_artifact_set_fenced,artifact_replay_matches}`, `staging.rs` recovery/validation, `claim_publication.rs` authority transitions, `forge/scribe_promotion.rs`, `forge/publication.rs`. Complete partition bytes now participate in file-list insertion/replay checks; changed key reconstruction uses `key.seal_key()` consistently. Existing transaction, epoch and authority transitions remain in place; no separate fence finding. |
| Compaction/publication versus metadata evolution | Architecture Forge managed compaction/publication; TASK-004 scenario 4 | Pinned Iceberg fork `bc01647...486273c` changes in transaction append/snapshot/apply-updates and scan; pinned compaction fork `8cc7bc5...efb39ce` (only dependency pins/lock changed). Fast append groups manifests per retained spec; metadata-update requirements retain CAS checks. No new compaction coordination owner or lease protocol introduced. |
| Pinned readers and cleanup | Architecture Oracle immutable planning, Forge retention/cleanup; operations reliability/recovery | `bifrost_catalog.rs::{pin_iceberg_snapshot,pinned_provider}`, `oracle/{exec,follower}.rs`, `memtable.rs` captures and `hot_source.rs` leases; pin-revision test; architecture retention window and query-deadline protection. Source-local Arc captures/staged leases and existing snapshot expiry roots remain; no new explicit query GC ledger required by authority. Live capture omission is CONC-02. |

Read root instructions, task-review skill, spec-driven workflow, maintainer guide, agent-doc index, authority map and relevant Bifrost/security/recovery sections. No nested AGENTS.md exists under the reviewed Redux or test-harness source. CodeGraph is absent. The wire, SDK and docsite changes were inspected as context, not independently audited for all language standards; that is outside this concurrency role.

## Material findings

### CONC-01 — INCORRECT: retries are repartitioned under current metadata before accepted identity resolves

**Obligation.** REQ-038/AC-013 and TASK-004 scenario 3 require retry binding to survive schema/layout alteration, restart and publication, and explicitly require resolving accepted batches before current-write rules.

**Locations.** `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:219–235,399–407,547–593`; `preprocess.rs:143–163,673–688,1022–1033`; durable consumer `crates/vala/vala-sql/src/queries/scribe_batch_commits.rs:215–243`.

**Producer-to-consumer evidence.** `resolve_logical_frame` always reads current `table_registration` and `committed_revision`. `prepare_and_dispatch` validates/stamps against that schema and preprocesses using that partition scheme before `committed_duplicate_rows`. The logical digest includes each slice's schema fingerprint, data digest/length and ordinal; the fence also compares slice count. Neither the accepted-batch cache nor durable fence resolves the accepted layout revision before that work. The changed WAL and staged metadata retain a revision for already accepted physical work, but that is not consulted for a resubmitted frame.

**Reachable consequence.** Accept a batch whose extra partition dimension splits rows into two slices, commit a supported spec that removes that dimension, and resend the identical original batch ID and bytes. The new planner produces one slice, so both fast duplicate recognition and durable logical fence matching fail. An honest retry becomes an identity contradiction. A rename or nullable addition can fail earlier while stamping/validating against the current schema. This affects live retries and retries after restart/publication, when the durable fence remains the identity authority. It does not require changing payload or weakening conflict checks.

**Existing proof limit.** `write_read::accepted_batch_revision_survives_alteration_and_restart` changes only the sort order at line 1456. Sorting does not change ingress schema or partition slice geometry, so that green proof cannot exercise the failure.

**Required correction.** Extend the existing accepted-batch/fence owner so accepted identity and retained revision resolve before current-schema validation/partitioning, then reuse catalog retained-layout resolution for that accepted attempt. Keep original payload/schema/order conflict checks and original accepted rows; do not add a parallel acceptance ledger or silently accept changed payloads. Prove identical retry success across a spec change that changes slice count and a compatible schema revision, including after restart/publication, while changed payload remains refused and rows remain exactly once.

### CONC-02 — INCORRECT: coherent live capture aliases generations from different revisions after restart

**Obligation.** TASK-004 scenario 3 and AC-013 require exact old/new revision visibility across restart, staged/immutable transitions and concurrent publication; complete accepted identity must not collapse in a reader.

**Locations.** `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:1746–1764,1912–1915`; `scribe/hot_source.rs:889–920`. Upstream allocation: `memtable.rs:250–264`; restored authority: `staging_runtime.rs:709–763`.

**Producer-to-consumer evidence.** Registry authority is correctly keyed by full `SealKey`, including `LayoutRevision`. Recovery restores each staged member under `key.seal_key()`. A fresh memtable counter starts at 1 (`memtable.rs:310`); `allocate_seal_id` skips restored ordinals only for the exact full key through `highest_generation(key, shard)`. Therefore old revision R's restored staged generation `(shard 0, generation 1)` and revised R+1's newly frozen generation `(0,1)` are both legal when the spec/physical partition stays unchanged, such as a sort-only revision. `ReadableSource` keeps only `PhysicalPartition`, and `LiveCapture` passes `(partition, generation)` to `capture_staged`. The registry consequently considers R's staged generation already served by R+1's memory generation and skips its runs.

**Reachable consequence.** Leave an old-revision staged member unpublished, restart, admit and freeze same-partition rows under a revised schema or sort order, and capture while the new member is still memory-authoritative. Old acknowledged rows disappear from a successful live query until the new generation transitions out of that state. Conversely, superseded matching can discard the wrong memory copy when the colliding old authority is published. Existing registry authority prevents actual duplicate full keys, so the defect is the capture's lossy projection, not missing registry validation.

**Required correction.** Preserve the full existing seal identity through `ReadableSource` and served/superseded capture membership, using the registry's existing revision-aware key. Keep current locking, publication barrier and lease lifecycle. Do not add a second allocator or global durable generation ledger. Add a focused capture test restoring old revision generation 1 and freezing new revision generation 1 in the same physical partition; both source row sets must be selected exactly once, with publication suppression applying only to the exact revision.

### CONC-03 — INCORRECT: a concurrent cold load can repopulate the committed-revision cache after invalidation

**Obligation.** TASK-004 owns the shared capability for new admission under a committed revision; scenario 3 requires committed revisions for new writes. TASK-005 owns public PATCH/CAS/refresh orchestration, but cannot rely on this local eviction seam's current guarantee.

**Locations.** `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1367–1395,1432–1441`; sibling producer `registration:1676–1703`; caller `scribe/ingress.rs:219–235`.

**Evidence and schedule.** A cold `committed_revision` call misses cache, awaits a catalog load and obtains R. An edit commits R+1 and calls `evict_committed_revision`, removing cached entries. The earlier cold call then acquires the write lock and inserts R. Every later admission now hits R indefinitely until another eviction/restart. The read's guard is dropped before IO and no generation or synchronization check detects invalidation. The registration cache has the same read-then-unconditional-insert race, and ingress reads registration and committed revision separately. Its read/insert shape predates this task, but previously its documented authority was immutable registration data; this candidate changes that premise, replaces immutable registered schemas with mutable committed revisions, and introduces the eviction API. The finding targets that newly required revision seam, not preexisting immutable-cache debt.

**Consequence.** Even after a successful commit and completed local eviction, fresh admissions can continue binding old schema/spec/order, or combine old registration fingerprint with newer metadata. This is stronger than allowing an already in-flight admission to finish under its originally resolved revision: it poisons subsequent fresh admissions.

**Required correction.** Make existing catalog cache publication respect completed invalidation for both related caches, preserving a coherent admission result. Use the existing catalog owner; do not require implementing public PATCH, cross-node notifications or TASK-005 edit orchestration here. A controlled overlap proof must pause a cold load before cache publication, commit/evict R+1, resume the old load, then show a fresh admission resolves R+1 and cannot repopulate stale registration/revision state.

## Verification assessment and limits

Supplied evidence records 919 Redux integration tests, 35 Scribe journeys, focused V8, format/lints/boundary/codegen passes. These were treated as supplied results and not rerun. V6 covers sequential sort-only alteration; V7 proves held-read schema/spec behavior; neither controls cache invalidation overlap nor cross-revision generation ordinal reuse after restart. V8 preserves compaction fences and inspects physical partition metadata, but does not close the ingress retry or live-capture gaps above.

No load generation, Cargo, or database mutation was performed. Findings are source-derived reachable schedules requiring independent validation; no numerical availability or latency promise is inferred. No speculative locking redesign, new retention policy, public edit API, or second engine is requested.

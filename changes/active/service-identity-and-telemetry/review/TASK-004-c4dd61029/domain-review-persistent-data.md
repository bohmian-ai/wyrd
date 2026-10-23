# Persistent-data domain review

Result: **FAIL**

Subject: complete `3e7c59db7...c4dd61029a54c44017c63bd4ff469472c406bc13` range. HEAD remained the candidate when checked. Review was independent; no other review report was read. No source or dependency files were modified.

## Boundary and authority

This review follows accepted row identity through physical partition calculation, revision selection, WAL and staged recovery, publication evidence, retained-spec planning and compaction. It does not assess public configuration orchestration assigned to TASK-005.

| Authority | Obligations applied |
| --- | --- |
| Root `AGENTS.md`; `changes/spec-driven-development.md`; `agent_docs/index.md` and `maintainer-style.md` | Actual source governs evidence; immutable cumulative review; durable behavior retains correct ownership and tests. |
| Approved revision-12 `spec.md`, REQ-035–038; AC-012, AC-013, AC-016 | Complete typed identity, whole-batch fanout limit, stable field IDs, accepted revision and exact reads across retained layouts. |
| `supplements/table-storage-and-correlation.md`, supported layout/type bounds (252–311), durable lifecycle (553–589), shared writer policy | 32 distinct tuples per accepted batch; null/type/spec distinctions; retained revisions; no compaction across physical partition boundaries. |
| Original `tasks/TASK-004-shared-table-storage.md`, scenarios 2–5 and acceptance table | Atomic fanout before WAL/fence; retained identity across restart/staging/publication; physical truth and partition-scoped compaction. |
| `architecture/README.md`; `architecture/bifrost-design.md` table identity, durability/visibility, Scribe staging/recovery, Oracle source authority, Forge promotion/managed compaction | One shared storage workflow; canonical identity in all tiers; staged authority survives restart; partition grouping includes spec; readers resolve stable field IDs. |

## Source coverage and observed chain

Paths below are under `crates/vala/vala-bifrost-redux/src/` unless qualified.

| Surface | Source inspected and assessment |
| --- | --- |
| Typed partition producer/codec | Full `catalog/physical_partition.rs`; `scribe/seal_key.rs` planning and canonical key. Null/string/int32/int64 have distinct tags, spec/time-position are encoded, and transform calculation reuses Iceberg. The pure planner enforces 32 for one RecordBatch. Finding DATA-1 concerns its enclosing stream owner. |
| Admission and retained recipe | `scribe/ingress.rs::resolve_logical_frame`, `prepare_admitted_rows`, `preprocess.rs::{NativeSliceProducer,prepare_rows,prepare_append,prepare_slice}`; `catalog/bifrost_catalog.rs::{committed_revision,retained_layout,evict_committed_revision}`; `catalog/layout.rs::{LayoutRevision,PhysicalLayout::of_iceberg}`; full `scribe/write_recipe.rs`. Schema/order and spec remain distinct, and the selected revision travels into each seal. Current Bloom policy is intentionally used when rebuilding retained layouts, but that conflicts with staged identity (DATA-3). |
| WAL and replay | Cumulative `scribe/wal.rs` and `replay.rs` diffs; partition decode, slice metadata, COMMIT digest/ordinal checks and canonical replay keys; `shards.rs` published replay suppression. WAL v7 carries complete partition bytes and schema/order IDs, with CRC and slice-set authentication preserved. Publication suppression compares dimension bytes alongside independent time fields. |
| Staged members/claims and durable publication | Cumulative changes in `assembly.rs`, `hot_stage.rs`, `member_stager.rs`, `hot_source.rs`, `claim_assembly.rs`, `claim_merge.rs`, `claim_publication.rs`, `persistence.rs`, `staging_runtime.rs`, `staging.rs`, `file_list_writer.rs`, `promotion.rs`. Examined `layout_fingerprint`, `restore_context`, claim key hashing, durable conversion and replay equality. Complete partition/revision is carried in assembly keys and claims; publication manifest v3 and SQL insert/replay comparisons carry dimension bytes. |
| SQL metadata | New `crates/vala/vala-sql/migrations/20261024000000_file_list_partition_dimensions.sql`; file-list writer insert and reconciliation SQL; shards replay query. Migration adds a required byte column with initial time-only encoding, without changing old migrations. |
| Reads | `scribe/memtable.rs::{LiveCapture::read,ReadableSource::project,projection_indices}`, `scribe/tail_rpc.rs` required-field transport, `oracle/follower.rs` leaf projection, `oracle/live.rs::{LiveTableRoutes::selected_by,fragment assignment}`, `oracle/exec.rs::{published_files,OracleTableProvider::try_new}`; catalog pin test. Live projection matches IDs and supplies null for an absent ID; time routing remains a conservative container over complete physical identities. Published planning explicitly binds the current schema rather than the snapshot schema. |
| Promotion | `forge/scribe_promotion.rs::validate_promoted_object`, `parquet/promoted_object.rs::PromotedObjectFooter::decode`, `scribe/promotion.rs`, `forge/publication.rs`. Retained specs/orders replace latest-only refusal; footer metrics resolve against retained schemas. |
| Compaction | `forge/managed/{policy,executor}.rs`, `forge/publication.rs`; pinned core grouping, request creation, DataFusion repartition and writer setup. Existing native partition grouping drops spec identity (DATA-2). Output request selects current schema/spec/order and computes truthful output partitions; no new Wyrd grouping layer was introduced. |
| Proof | TASK-004 implementation evidence/diagnoses, V3 pure identity/WAL test, V4 fanout journey, V6/V7 accepted/mixed-revision journeys, V8 mixed-spec promotion/compaction assertions and V9 physical sort/Bloom assertions. These provide meaningful positive-path evidence but omit the three counterexamples below. |

External forks were inspected locally at the exact pinned commits, rather than inferred from Cargo pins:

- `iceberg-rust` `bc01647...486273c`: complete changed-source diff in scan builder and transaction snapshot/append/apply-updates. Added files validate against a retained spec; manifests split by spec. `with_schema_id` binds planning to the requested retained schema.
- `datafusion` `b8ec4b1...1eab549`: complete diff, solely `physical-expr-adapter/src/schema_rewriter.rs`. Resolution uses stable field IDs when both schemas carry them; renamed physical slots are rebuilt with physical names, and absent IDs become null.
- `iceberg-compaction` `8cc7bc5...efb39ce`: complete diff changes dependency pins only. Expanded into `core/src/file_selection/strategy.rs`, `compaction/mod.rs::create_rewrite_request`, `executor/datafusion/{mod,iceberg_file_task_scan,datafusion_processor}.rs`, and partition-scope tests to verify the actual machinery used by Forge.

## Findings

### DATA-1 — INCORRECT: native IPC streams apply the 32-partition bound per RecordBatch

**Obligation:** REQ-036, TASK-004 scenario 2, supplemental per-write fanout: at most 32 complete tuples per accepted logical batch, independent of transport record-batch segmentation.

**Locations:** `scribe/preprocess.rs:336–340` (`NativeSliceProducer::next_slice`), `:696–710` (`prepare_rows`); `scribe/seal_key.rs:338–377` (`plan_partitions`).

**Evidence and reachability:** `gate/mod.rs:1044` forwards a public native frame as `IngressPayload::ArrowIpc`. `ingress.rs:335–349` retains that whole stream under one `batch_id`. `material_plan.rs::plan_native` only checks wire size. The decoder yields successive RecordBatches; `next_slice` constructs a fresh `plan_partitions` result for each batch, whose distinct-tuple vector is local to that call. `prepare_rows` collects every resulting slice, assigns final ordinals and submits them without a union of partitions. No stream-wide set exists in `NativeSliceProducer`.

**Observable consequence:** One small native frame containing two valid batches with 20 distinct customer partitions each (40 distinct complete tuples total) is accepted and fenced, although exactly the same rows in one RecordBatch are refused. Repeated tuples across record batches must count once, so a slice-count check alone is not a valid fix.

**Required correction:** Enforce the complete tuple union at the existing preparation owner across the entire logical native frame before yielding a PreparedAppend to WAL/shards. Keep the existing per-source planner and atomic refusal; do not add a second admission or durable ledger. A bounded union across the already materialized slices is sufficient.

**Focused proof:** Construct multi-RecordBatch IPC streams under one batch ID: union 32 succeeds, union 33 refuses with no WAL/fence/visible rows, and repeated tuples across batches do not consume additional slots. Retry a corrected payload after refusal. Existing V4 uses one RecordBatch and does not close this gap.

### DATA-2 — INCORRECT: pinned compaction grouping aliases equal tuples from different specs

**Obligation:** REQ-037, TASK-004 scenarios 2/4 and its explicit constraint that compaction cannot cross physical partition boundaries; architecture managed compaction explicitly includes spec identity.

**Locations:** `Cargo.toml:260` pins `iceberg-compaction-core` `efb39ce`; that immutable dependency's `core/src/file_selection/strategy.rs:1176–1203`, `PartitionKey` / `group_files_by_partition`. Wyrd caller: `forge/managed/policy.rs:289–300` selects partition-scoped grouping; `forge/managed/executor.rs:313` executes the resulting plan.

**Evidence and reachability:** Native `PartitionKey` contains only `iceberg::spec::Struct`; neither its constructor nor map key includes `FileScanTask.partition_spec.spec_id()`. All partition grouping strategies route through this owner. The core's cumulative fork diff only updates dependency pins, leaving this logic unchanged. Files promoted by this candidate now legitimately retain multiple specs, so the collision becomes reachable. Equal typed tuple values do not imply equal physical partitions across specs.

**Observable consequence:** Retained `hour(t), bucket(4, customer)` and new `hour(t), bucket(8, customer)` files for the same hour and bucket value 0 are placed into one compaction group. Two single files that should independently remain below the minimum partner count can qualify together; a rewrite can merge rows across those distinct spec-qualified partitions. The output may still contain exact rows and truthful current-spec metadata, but it violates the expressly required input partition isolation.

**Required correction:** Extend the existing native grouping key with the file's partition-spec identity and preserve its typed tuple. Fix the core owner and pin its immutable revision; do not duplicate grouping in Wyrd. Keep existing output partitioning, sorting, deletes and publication behavior.

**Focused proof:** At the native grouping owner, pass equal tuples under different retained spec IDs and require separate groups, including null tuples. Extend a focused Forge seam with same-valued tuples across specs and assert selected input groups do not mix specs; an output-only partition assertion cannot establish this invariant.

### DATA-3 — REGRESSION: a Bloom-only edit makes ready staged members unrestorable

**Obligation:** REQ-037–038, AC-013/016, TASK-004 scenario 3: accepted/staged work retains authority across compatible layout changes and restart. Bloom membership/order is optional pruning policy and must not make durable accepted rows unrestorable.

**Locations:** `catalog/layout.rs:1007–1014`, `scribe/assembly.rs:278–282`, `scribe/staging_runtime.rs:822–849` (recovery call at `:671`).

**Evidence and reachability:** `ScribeAssemblyKey` stores a layout fingerprint in its durable staged record. `assembly::layout_fingerprint` hashes the complete Bloom column vector, including order. `PhysicalLayout::of_iceberg` reconstructs a retained schema/spec/order using the table's *current* Bloom property, explicitly because Blooms are not file identity. During startup `restore_context` resolves this layout, reconstructs the assembly key and requires complete equality with the old key. Changing `[customer]` to `[]`, or simply reordering two valid Bloom columns, changes the reconstructed fingerprint even though retained schema/spec/sort remain identical. The existing catalog revision seam can commit the property change; no public PATCH implementation is needed to expose the storage bug.

**Observable consequence:** Accept rows, stage below publication target, commit a valid Bloom-only property change, then restart. Recovery returns `the registered write recipe no longer reproduces a recovered staged key` instead of restoring accepted rows. The same issue occurs when a Bloom source is renamed and the current property names the renamed field while the retained old schema uses the previous name.

**Required correction:** Reconcile durable assembly compatibility with the existing recipe owner's rule that optional current Bloom policy is not file identity. The smallest correction under that design is to exclude Bloom policy from the durable compatibility fingerprint while retaining schema, partition/spec, sort/order, tenant, stream and member checks; old staged filters may remain as encoded, and merged outputs use resolved policy. Do not weaken checksum/schema/footer validation. If preserving acceptance-time Bloom policy is instead required, retain it through the existing metadata owner rather than reconstructing it from mutable current properties.

**Focused proof:** Stage real members, replace/clear/reorder Blooms through the existing metadata seam, restart and publish, then verify exact rows once and valid physical Bloom output. Include a rename of a Bloomed custom field with a stable ID. Current V6 changes sort order and V7 starts from an empty Bloom set, so neither tests this transition.

## Verification limits

This was a static audit; no Cargo, Postgres, or journey command was rerun. Reported successful checks are supplied task evidence, not fresh execution. External changes were inspected from local immutable Cargo checkout source. No live data or cloud service was accessed. No unrelated security, public SDK, or deployment acceptance verdict is claimed here.

The preserved identity/field-ID paths have credible positive evidence, but the three reachable gaps above prevent a persistent-data PASS. Skipped broad reruns; the material risks are fanout-limit bypass, cross-spec compaction grouping, and staged recovery failure after Bloom changes.

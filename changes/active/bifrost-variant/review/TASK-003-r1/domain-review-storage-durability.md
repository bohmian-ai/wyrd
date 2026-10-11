# TASK-003 r1 — domain review: storage and durability

Reviewer: `domain-rev` (storage and durability), fresh and independent.
Subject: base `b145767dd` → candidate `861a0342f` (worktree HEAD confirmed
`861a0342f8230228b06323d28df43337c23dd5c0`). No source was modified.

## Reviewed boundary

The object-store and persistence paths the candidate changed:

- `vala-bifrost-redux/src/storage/{mod,error}.rs`. The deleted `storage/cache.rs`
  and `storage/telemetry.rs` were read at the base revision.
- `catalog/iceberg_storage.rs`, `catalog/bifrost_catalog.rs`, `catalog/storage.rs`
  (`warehouse_uri`).
- `parquet/object_uploader.rs` and its two callers in `scribe/persistence.rs`
  (`stage_and_upload_candidate`, `recover_publication`), plus
  `scribe/staging.rs` recovery verification.
- `forge/{compact,orphan_gc,path,mod,worker,expire,gc,live_reconcile,scribe_promotion,error}.rs`,
  and the Forge integration support and fixture changes.
- `wyrd-storage/src/{signer,local,telemetry,factory}` (the files live under
  `crates/wyrd/wyrd-storage`, not `crates/shared`).
- `wyrd-queue/src/{variant,batch_builder}.rs`.
- The storage seams Oracle reads through, in `oracle/exec.rs`:
  `TenantParquetReaderFactory`, `TenantWarehouseReads::{reader_factory,bind}`,
  `hot_parquet_scan`, `HotFileSource::statistics`, `cache_footers`, and
  `persisted_input`.
- Boot wiring in `wyrd-server/src/boot/mod.rs`: storage owner, footer cache,
  `ForgeObjects`, and the warehouse URI.
- The `object_store` 0.14.2 `LimitStore` permit semantics, read from the
  registry source.
- The `[patch.crates-io]` fork pins.

## Authority and source coverage

- Spec revision 26 (working-tree `spec.md`):
  - REQ-032 (one Bifrost object store)
  - REQ-033 (limits come from `object_store`; the governed request path and its
    metrics are deleted)
  - REQ-034 (uploads converge by read-back; "a published key is never uploaded
    again")
  - INV-010 (two storage implementations)
  - AC-014
- Plan `TASK-003-native-datafusion-read-path-plan.md`:
  - step 3 items 4 and 5;
  - "Item 5 follow-up", which caches footers for published files only;
  - the step-4 and step-5 evidence.
- `architecture/bifrost-design.md`:
  - object storage paragraph
  - Variant shredding
  - Scribe promotion
  - managed compaction and publication
  - convergence, retention and cleanup
  - telemetry
- AGENTS.md §15.

## Verification run by this reviewer (candidate tree)

```
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/parquet::object_uploader::tests::/) | test(=forge::compact::tests::forge_listing_pages_are_bounded_ascending_and_resumable) | test(/forge::path::tests::/) | test(/storage::tests::/)'
  -> 14 passed
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/forge::orphan_gc::tests::/)'
  -> 6 passed
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(/variant::tests::/)'
  -> 20 passed
```

## Verification limits

- The following were not rerun; I relied on the navigation map's recorded
  evidence for them:
  - `mise run test:bifrost`
  - the Postgres-backed Forge integration target
  - the emulated-cloud storage lanes
- I did not read the DataFusion or arrow-rs fork sources. Whether the fork's
  native read path can read Wyrd's shredded layout rests on the fork evidence
  (`verify6.log`) and on two Wyrd tests:
  `published::struct_and_variant_share_physical_pushdown` and
  `shredded_paths_read_only_their_leaves`.
- No real S3, GCS, or Azure backend was exercised.

## Answers to the domain questions

1. **Can a published object become unreadable or misread?**
   - **Layout.** No path found. The candidate leaves the writer layout
     unchanged (`BIFROST_VARIANT_SHREDDING`, `parquet::variant_residual`).
     `wyrd-queue` only adds residual validation to the JSON render gate.
   - **Pins.** The pins are immutable published revisions: DataFusion
     `a4ff145`, arrow-rs `0d0eb24`, iceberg `0b1a066`, compaction `7a54131`,
     and distributed `0b81b12`.
   - **Footer proof.** It runs after every metadata load, cached or not
     (`TenantParquetReader::get_metadata`). It is re-attached on distributed
     workers through `TenantWarehouseReads::bind`.
   - **Statistics.** Row counts are reported as `Inexact`, so `count(*)` cannot
     skip the proof.
   - **Ranged reads.** They keep the stale-object marker (`mark_stale` on
     `get_bytes`, `get_byte_ranges`, and `get_metadata`).
   - **Overwrite of a published key.** Uploads now overwrite unconditionally.
     The only re-upload of a published key that I found (STO-1) writes bytes
     that were locally re-verified against the manifest digest. Every backend
     replaces objects atomically, so readers never see torn bytes.
2. **Did removing the cache change read amplification, correctness, or tenant
   isolation?**
   - **Tenant isolation.** Preserved. DataFusion's `DefaultCache` is keyed by
     the warehouse-relative path, which always contains `tenants/{id}/…`. The
     tenant proof is applied on every load, including cache hits.
   - **Correctness.** Preserved:
     - Cache validity uses a manifest-fixed size and a constant `last_modified`.
     - Only published (immutable) objects are cached.
     - Staged runs use an uncached factory.
   - **Amplification.** Single-flight is lost, so concurrent cold queries can
     each decode the same footer. Separately, `persisted_input` →
     `cache_footers` now fetches the footer and page index of every
     Iceberg-kept published file at planning time on the leader. That
     behaviour is plan-approved ("Item 5 follow-up"), so it is not a finding.
     One note for the Oracle/admission reviewer: this is object IO before
     execution, while the rustdoc on `OracleTableProvider::scan` still says
     bytes are "first accessed only when DataFusion executes the already
     audited plan".
3. **Does Forge compaction and orphan GC still preserve objects referenced by
   live snapshots and active reads?**
   - Yes. The eligibility predicate, its protection roots, and the fence and
     lease checks are unchanged; only the `opendal` → `object_store` types
     changed.
   - `NotFile` and the "no last-modified" branches became unreachable, because
     `object_store` lists only objects and every `ObjectMeta` carries
     `last_modified`. Their removal is sound.
   - The listing cursor uses `list_with_offset`, which is exclusive. The local
     filesystem pages through an emulated ordered path, and a test proves
     bounded, ascending, resumable pages on both paths.
   - The listing stream is dropped before `apply_gc_deletions` runs, so no
     `LimitStore` list permit is held across deletes.
   - `validate_table_location` is now an exact `{warehouse}/{prefix}` match
     against the same `warehouse_uri` the catalog uses to create tables. That
     is stricter than before and no less safe.
4. **Do the deleted storage error variants or telemetry drop required
   contracts?**
   - `BifrostStorageError` is crate-internal and is not a `WyrdError` catalog
     entry.
   - REQ-033 explicitly deletes the governed request path and its
     `bifrost_storage_*` metrics, and the design doc's Storage bullet was
     updated to match.
   - One documented metric remains that the candidate removed: STO-2.

## Material proposed findings

### STO-1 — DRIFT: a published key is uploaded again during Scribe recovery

- **Violated obligation.** Spec REQ-034: "Only catalog compare-and-swap and
  lease fences publish an object, and a published key is never uploaded again."
- **Location.**
  - `crates/vala/vala-bifrost-redux/src/scribe/persistence.rs`:
    `recover_publication` uploads every claim unconditionally at about line
    1610, *before* `reconciler.publish` at line 1623.
  - `parquet/object_uploader.rs:229`: `converge` always calls `put_file` first,
    and `put_file` is an unconditional `BufWriter` PUT or multipart upload.
- **Evidence.**
  - Recovery runs for every uncleaned publication manifest. That includes
    publications whose fenced `file_list` commit already landed: a crash
    between COMMIT and `cleanup_published`, or an `UnknownCommitOutcome`.
  - `file_list_writer` inserts `ON CONFLICT … DO NOTHING` and reports "every
    row already existed and matched", so an already-committed publication is
    an expected recovery input.
  - At the base, the PUT was `if_not_exists(true)`, so this replay was a no-op
    against the published object. The candidate turns it into a full
    re-upload.
  - The object may also already have been promoted unchanged into an Iceberg
    snapshot under the same key by another node's Forge.
- **Observable consequence.**
  - A full re-upload, with a new `last_modified`, of an object that Oracle
    already reads and Iceberg may already reference.
  - On a versioned or replicated bucket, a new object version is created.
  - The bytes are re-verified locally (`staging.rs:650`) before the upload, so
    no reader sees different content. The finding is a contradiction of an
    approved invariant plus avoidable writes, not corruption.
- **Testable correction.** Pick one:
  - (a) In `converge`, attempt `verify_remote` first and skip `put_file` when
    the key already verifies. A first upload costs one extra HEAD that returns
    `NotFound`.
  - (b) If the re-upload is intended, amend REQ-034 to say so.

  For (a), add an uploader unit test with a PUT-counting store wrapper: a key
  that already holds the expected bytes converges with zero PUTs, and an absent
  key converges with one.

### STO-2 — DRIFT: the design doc documents a pruning metric the candidate removed

- **Violated obligation.** AGENTS.md §1(3) and plan step 3 item 11 require
  `architecture/bifrost-design.md` to be the active authority and to describe
  the result. The candidate edited the same Telemetry section.
- **Location.** `architecture/bifrost-design.md:1221-1223`:
  "`bifrost_oracle_file_pruning_total{outcome}` counts hot-object exclusions
  decided from declared bounds …".
- **Evidence.**
  - `git grep bifrost_oracle_file_pruning_total 861a0342f -- crates` returns
    nothing.
  - The plan records: "Wyrd's pruner and its metric are gone, replaced by
    DataFusion's `FilePruner`". `published.rs` no longer asserts the metric.
  - Pruned files now surface only through `files_ranges_pruned_statistics`,
    folded into scan stats (`PruningEvidence::files_pruned`,
    `RemoteScanMetrics::record_pruned_files`).
- **Observable consequence.** The design authority promises an operator-facing
  Prometheus series that is never emitted. Dashboards and alerts built from the
  doc go silent.
- **Testable correction.** Replace the sentence. Say that file exclusions are
  counted from DataFusion's `files_ranges_pruned_statistics` into the scan file
  counters, and that the dedicated counter no longer exists. Verify with
  `git grep -n bifrost_oracle_file_pruning_total architecture crates` returning
  no matches, plus `mise run docs:check`.

## Non-material observations (not findings)

- `BifrostStorageError::from_object_store` is only called from a unit test.
  It is a candidate for deletion.
- Some rustdoc is stale after the swap away from OpenDAL:
  - the `PARQUET_TRANSFER_BUFFER_BYTES` doc (`scribe` memory module) still says
    "while `OpenDAL` owns one payload copy";
  - `wyrd_queue::variant::mask_placeholders` says "every reader passes the
    column through here first", but Oracle readers now use the fork's
    validation, and only the JSON renderers use this gate.
- `HotObjects::Staged` reads staged runs through a separate
  `LocalFileSystem::new()`. This is consistent with REQ-032 only if
  "staged-run reads" means local WAL-disk reads, since a cloud store cannot
  reach them. Worth one clarifying line in REQ-032 or INV-010. It is not a
  defect.
- `BifrostIcebergStorage::delete_prefix` holds two `LimitStore` permits: one
  for the delete stream and one for the inner list. No production caller
  exists today, so this is not raised.

## Result

**FAIL**: two DRIFT findings (STO-1, STO-2), neither of which corrupts or
loses data. No MISSING, INCORRECT, VIOLATION, or REGRESSION was found in the
storage and durability boundary.

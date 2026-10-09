# TASK-003 r1 reuse review

Reviewer: `reuse-rev`.

- Subject: base `b145767dd` to candidate `861a0342f`.
- Forks checked:
  - DataFusion `a4ff14550`, at `/home/thorrester/Documents/GitHub/datafusion`.
  - arrow-rs `0d0eb24`, at `/home/thorrester/Documents/GitHub/arrow-rs`.
  - datafusion-distributed `0b81b12`.
  - iceberg-rust `0b1a066`.

Method:

- Read every added or materially changed item in the candidate diff.
- For each one, searched for an existing owner in three places:
  - the base tree (`git grep <term> b145767dd`, CodeGraph);
  - the installed crate APIs;
  - the pinned forks.
- Searched for duplicates inside the candidate itself.
- Most of the candidate (about 13.6k deleted lines in `oracle/`) removes Wyrd scan, pruning, Variant-SQL and cache code in favour of native DataFusion. Those deletions reduce duplication and are not findings.

## Confirmed duplicates

### REU-1: Oracle session construction re-implements `SessionStateBuilder::build`, and the candidate has two session paths

**New location:** `crates/vala/vala-bifrost-redux/src/oracle/session.rs:39-53` (`OracleSessionTemplate::session`).

It does three things:

1. Clones a `with_default_features` template.
2. Replaces the config with `*state.config_mut() = config`.
3. Builds a `MemoryCatalogProviderList` by hand, then registers `SessionStateDefaults::default_catalog(...)` under `options.catalog.default_catalog`.

**Second path inside the candidate:** `crates/vala/vala-bifrost-redux/src/resources.rs:4258-4262` (`OracleExecution` session state). It wraps the template output in `SessionStateBuilder::new_from_existing(..).with_runtime_env(..).build()`. That `build()` registers the default catalog and re-applies every UDF's `with_updated_config` again, so the template's work is repeated.

**Callers:**
- `oracle/mod.rs:2883`
- `oracle/bench_support.rs:445`
- `resources.rs:4262`

**Existing owner:** DataFusion fork `datafusion/core/src/execution/session_state.rs`, `SessionStateBuilder::build`:
- default catalog list and default catalog creation, about lines 1800-1810;
- the UDF `with_updated_config` pass, about lines 1726-1750.

`SessionStateBuilder::new_from_existing(state).with_config(..).with_catalog_list(..)` is the public API for "the same state with a fresh config and catalog list".

**Evidence they do the same job:**
- Both produce a `SessionState` with a fresh catalog list holding one default catalog built from `SessionStateDefaults::default_catalog`.
- The hand-written version also skips the config-dependent UDF refresh that `build()` performs after a config change. So it differs only where DataFusion is more correct.
- Wyrd now owns a copy of DataFusion session-construction logic, against the directive.

**Smallest consolidation:**
- Have `OracleSessionTemplate::session` return `SessionStateBuilder::new_from_existing(self.template.clone()).with_config(config).with_catalog_list(Arc::new(MemoryCatalogProviderList::new())).build()`.
  - This relies on `build()` adding the default catalog when `create_default_catalog_and_schema` is set. That is the default under `with_default_features`.
- Delete the hand-built catalog code (session.rs:42-51).
- Have `resources.rs:4258` pass `with_runtime_env` into the same single builder chain, instead of calling `build()` a second time on the template output.

### REU-2: Variant shredded-residual walk duplicates arrow-rs `unshred_variant`

**New location:** `crates/shared/wyrd-queue/src/variant.rs`. It walks the shredded Variant layout (`value` / `typed_value`, struct fields, list and large-list offsets) and validates each residual `value` cell against the metadata. Items:
- `ShreddedResiduals` and its methods `validate_typed` / `validate_shredded` / `validate_elements`, lines 577-683;
- `present_owners`, line 684;
- the call from `mask_placeholders`, lines 517-560.

**Existing owner:** arrow-rs fork, `parquet-variant-compute/src/unshred_variant.rs`. `unshred_variant` and `UnshredVariantRowBuilder` (line 61 and lines 248-351) walk the same shredded layout:
- typed_value struct fields;
- List, LargeList and ListView typed_value;
- per-row residual `value` binaries.

Each residual is validated through `Variant::try_new_with_metadata`. `VariantArray` / `ShreddedVariantFieldArray` own the typed access to those columns.

**Evidence they do the same job:**
- Both iterate the same `metadata` + `value` + `typed_value` tree with the same nullability and ownership rules (a residual is checked only where its parent row is present).
- Both reject a residual that does not decode under the row's metadata.
- The Wyrd side keeps that layout knowledge in a Wyrd crate, against the directive that Variant logic belongs in DataFusion / arrow-rs.
- In the candidate, `mask_placeholders` is reached only through `variant_cell_to_json` (variant.rs:383) and `WyrdJsonEncoderFactory` (variant.rs:434), which `wyrd-client` `bifrost/facade.rs` uses. The former production caller, `oracle/variant_sql.rs`, was deleted.

**Smallest consolidation:**
- Replace `ShreddedResiduals` and `present_owners` with `parquet_variant_compute::unshred_variant(&VariantArray::try_new(..)?)`, followed by the existing top-level `EncodedVariant::validate` row gate.
  - `unshred_variant` already returns an error for an undecodable residual.
- If the Bifrost depth bound has to run before arrow's recursion, put that bound into the arrow-rs fork's `unshred_variant` (or into a `VariantArray` validation entry point) rather than keeping a parallel walker in `wyrd-queue`.

### REU-3: Test BifrostStorage construction pasted in two `wyrd-server` sites

**New locations:**
- `crates/wyrd/wyrd-server/src/test_support.rs:60-75`
- `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs:3148-3163`

Both paste the same expression: `BifrostStorage::new(storage.backend_config(), BifrostStoragePolicy::resolve(BifrostStorageConfig::default(), u64::from(u32::MAX), false).expect(..)).expect(..)`.

**Existing owner:** `BifrostStorage::for_test(root, serves_oracle)` in `crates/vala/vala-bifrost-redux/src/storage/mod.rs` (around line 199). It wraps the same `resolve(..)` + `new(..)` pair. The candidate itself switched `catalog/bifrost_catalog.rs` production_pin_tests to `BifrostStorage::for_test(root, false)`, which shows the helper fits.

**Evidence they do the same job:** both build a Local-backend `BifrostStorage` with the default storage config, a fixed managed budget, and Oracle serving disabled.

**Smallest consolidation:**
- Have `pg_invocation_tests.rs` call the `test_support.rs` helper instead of its own paste.
- Or call `BifrostStorage::for_test` in both sites when the backend is Local.

### REU-4: Four `BackendSigner` methods now have two implementations each

**New location:** `crates/wyrd/wyrd-storage/src/signer.rs`. The candidate split each of these methods in two:
- `presign_single_put`
- `init_multipart`
- `presign_get`
- `remint_plan`

The split is:
- a `#[cfg(feature = "cloud")] async fn` that still contains a `Self::Local` arm;
- a `#[cfg(not(feature = "cloud"))]` sync fn returning `std::future::Ready`, built from `let Self::Local(signer) = self`.

**Existing owner:** the single base method per operation (base `signer.rs`). It already dispatched `Local` versus `Cloud` with one `match`.

**Evidence they do the same job:**
- In both variants the Local arm calls the same `LocalSigner` method (now synchronous).
- The non-cloud copy exists only to avoid an `async fn` with no await in that feature combination.
- The result is two bodies per operation, eight in total, with the Local behaviour stated twice.

**Smallest consolidation:** keep one method per operation that returns `impl Future<Output = Result<..>>` (or a single `async fn`) and matches inside it. Delete the four `#[cfg(not(feature = "cloud"))]` copies. Do not suppress the lint with `#[allow]`.

### REU-5 (low): Test-only `object_exists` and `injected` helpers duplicated across fixtures

**`object_exists`** appears in three places. Each does `head(path)` and maps `NotFound` to `false`:
- `PromotionIntegrationFixture` in `crates/vala/vala-bifrost-redux/tests/integration/`;
- `WyrdTestCluster::object_exists` in `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs`;
- production `BifrostIcebergStorage::exists` in `catalog/iceberg_storage.rs`.

The two fixtures could call `BifrostIcebergStorage::exists` (or `ObjectStore::head`), and the copies could be deleted.

**`injected(message) -> object_store::Error::Generic`** is pasted in two places:
- `crates/vala/vala-bifrost-redux/tests/integration/forge/support.rs`
- `crates/wyrd/wyrd-testing/src/bifrost/forge_harness.rs`

The vala integration tests already depend on `wyrd-testing`, so they could import that one.

## Checked, not duplicated

Each line names the surface, then the owner searched.

- `oracle/exec.rs` `TenantParquetReaderFactory` / `TenantParquetReader` / `mark_stale`: wraps DataFusion `CachedParquetFileReaderFactory` / `DefaultParquetFileReaderFactory` and adds only the tenant footer proof.
- `oracle/exec.rs` `hot_parquet_scan`: native `ParquetSource`, `FileScanConfigBuilder`, `FileGroup::split_files`, `compute_all_files_statistics`.
- `oracle/exec.rs` `published_files` / `PublishedAssignment`: iceberg `table.scan().plan_files()`, used for listing only.
- `oracle/exec.rs` `cache_footers` / `cache_footer`: calls DataFusion `DFParquetMetadata` into `FileMetadataCache`. Prewarm only, no reimplementation (directive risk noted for the system reviewer).
- `oracle/exec.rs` `distinct_files`: trivial dedup; no owner needed.
- `oracle/exec.rs` `parquet_pruning` / `PruningEvidence` / `sum_named_count`: DataFusion `MetricsSet::sum_by_name`; the evidence types pre-exist in base.
- `oracle/exec.rs` / `mod.rs` `query_class_for_root` and `record_distributed_scan_metrics`: datafusion-distributed `DistributedExec` / `rewrite_distributed_plan_with_metrics`; pre-exist.
- `oracle/exec.rs` `TenantWarehouseReads`: datafusion-distributed `WorkerPlanRewriteHandler` seam.
- `oracle/exec.rs` projection helpers (`scan_output_names`, `required_columns_closure`, `select_schema_by_name`, `OracleScanProjection`, `project_plan_by_name`, `project_batch`): pre-exist in base.
- `oracle/live.rs` `HotFileSource::statistics` / `set_event_time_bounds`: emits DataFusion `Statistics`.
- `oracle/live.rs` `selected_by`: DataFusion `PruningPredicateBuilder`.
- `oracle/live.rs` `handle_child_pushdown_result`: the DataFusion `ExecutionPlan` filter-pushdown trait method (`FilterPushdownPropagation`).
- `oracle/live.rs` `LiveUnionBoundary` / `LiveFrameDecoder`: pre-exist.
- `oracle/nested_pushdown.rs` `FileReadPlan`: `#[cfg(test)]`, wraps DataFusion `PerFileParquetReadPlanner` / `RowGroupAccessPlanFilter` / page pruning.
- `oracle/follower.rs`: mostly deletions; runs DataFusion's `FilterPushdown` rule after substitution.
- `oracle/codec.rs`: deletions of the private conversion and proto; remaining codec uses datafusion-distributed's codec.
- `oracle/analytical.rs` `AnalyticalSessionBuilder::build_session_state`: datafusion-distributed `WorkerSessionBuilder`.
- `oracle/analytical.rs` `planning_session`: maps `file_scan_bytes_per_task` onto upstream `set_distributed_file_scan_config_bytes_per_partition`.
- `oracle/analytical.rs` `tenant` / `record_fixture_tenant` / `fixture_objects`: thin accessors over the existing `recorded(graph)` entry and the catalog's `BifrostIcebergStorage`.
- `oracle/analytical_transport.rs`, `peer.rs`, `query_stream.rs`, `dispatcher.rs`, `bindings.rs` (`OracleExecutionBindingInputs::new`, `QueryClass` `for_test`): signature and plumbing edits on existing owners; no new mechanism.
- `resources.rs` `OracleSessionShape::for_test` / `config_options` and the new `use_row_number_estimates_to_optimize_partitioning` / `max_passes` settings: DataFusion `ConfigOptions` fields; the existing owner `OracleSessionShape` sets them.
- `wyrd-server` `config.rs:491` `file_scan_bytes_per_task` (validated at `:3128`): existing Bifrost config owner; maps to the upstream knob.
- `storage/mod.rs` `BifrostStorage` on object_store with `LimitStore`: object_store's own clients. It reuses `wyrd_storage::factory::pool_idle_timeout` (now pub) and the new `wyrd_storage::factory::gcs::service_account_json`. Running parallel to the opendal factory is mandated by spec REQ-032.
- `storage/error.rs` `from_object_store` / `error_chain_contains_not_found`: pre-existing chain-walk pattern (`is_tenant_invariant_error`, `is_stale_iceberg_object_error`).
- `catalog/iceberg_storage.rs` `BifrostIcebergStorage` / `BifrostIcebergStorageFactory` (now object_store + `BufWriter`): the iceberg fork ships only `crates/storage/opendal`, with no object_store `Storage`; `relativize` pre-exists.
- `catalog/bifrost_catalog.rs` `objects()` accessor and `warehouse_uri` re-export: `catalog/storage.rs:43` pre-exists; the test now reuses `BifrostStorage::for_test`.
- `tables/mod.rs` `holds_variant`: pre-existing, visibility change only (`wyrd-types::variant::is_variant` tests a single field; `holds_variant` recurses over structs).
- `forge/path.rs` `validate_table_location` / `catalog_path_to_object_key`: overlaps `relativize` only partly; enforces the stricter binding-scoped warehouse invariant.
- `forge/orphan_gc.rs`, `forge/compact.rs` `ForgeObjects` / `ForgeObjectStore`: opendal-to-object_store ports of existing owners.
- `parquet/object_uploader.rs`: object_store `BufWriter`.
- Scribe `operator: Arc<dyn ObjectStore>` and `ScribeStageMover::new`: type port of existing owners; tests use `object_store::memory::InMemory`.
- `ScribeError` variant removals: deletions.
- `wyrd-testing` `ObjectReadProbe`: object_store wrapper that replaces the deleted `StorageOperationBarrier`.
- `wyrd-testing` `ReadStall`: same Notify-flag pattern as `AnalyticalExecutePause` / `IngestStall` / `QueryStreamStall`, but there is no shared owner across these crates; not consolidated.
- `wyrd-testing` `CountingObjectStore` / `ForgeObjectStoreControl`: pre-existing fixtures, ported.
- `wyrd-testing` `server.rs` / `scribe_workload.rs` / `cluster.rs` edits: storage-type ports.
- `oracle/bench_support.rs` `OracleBenchTables::publish`: iceberg `MemoryCatalogBuilder` + `parquet_to_data_file_builder`, the same APIs production uses (`promoted_object.rs:82`, `scribe/parquet_writer.rs:881`); bench fixture.
- `oracle/bench_support.rs` `bench_session_config`: pre-exists.
- `benches/bifrost_reads/{main,events}.rs`: new bench driver replacing the deleted `nested_field_pushdown.rs`; uses `bench_support`.
- `benches/bifrost_reads/report.rs` `Latency::of` (nearest-rank p50/p95): the same logic exists in `wyrd-testing::capacity::Percentiles::raw` and in vala `production_routes.rs:2121`, but neither is reachable from vala-bifrost-redux benches because of dependency direction. Not consolidable.
- Analytical tests pasting `SessionContext::new_with_config(OracleSessionShape::for_test().session_config()).state()` (6 in `follower.rs`, 3 in `exec.rs`): minor in-module test repetition; below the finding bar.
- `wyrd-spec` `assignment_authority.rs` and `api.rs`: net deletions.
- `wyrd-queue` `batch_builder.rs`, `otlp_json` / `otlp_metrics_decode`: mechanical `as_chunks` rewrites.
- `wyrd-storage` `local.rs` methods made pub and sync, `telemetry.rs` `Ready` test futures: mechanical changes (their signer callers are covered by REU-4).
- `wyrd-storage` `factory/gcs.rs` `service_account_json`: single owner, shared by the opendal and object_store factories.
- Toolchain bump to Rust 1.98.1 and publisher `Box`: mechanical changes.
- Deleted modules (`variant_sql.rs`, `leaf_paths.rs`, `analytical_scan.rs`, `pruning.rs`, `storage/cache.rs`, `storage/telemetry.rs`, the old bench, `private_conversion.rs` / proto): removals; they reduce duplication.

## Verdict

FAIL. Confirmed duplicates REU-1 through REU-5 remain in the candidate. REU-1 (DataFusion session construction) and REU-2 (Variant layout walk in `wyrd-queue`) put DataFusion and arrow Variant logic into Wyrd, which the user directive forbids.

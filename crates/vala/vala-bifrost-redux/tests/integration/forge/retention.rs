//! Orphan GC and expiry protection: paging, run budget, fail-closed
//! protection loads, and TTL-only eligibility.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {
    use iceberg::Catalog;

    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use std::time::Duration;

    use iceberg::spec::StatisticsFile;

    use iceberg::transaction::{ApplyTransactionAction, Transaction};

    use opendal::Buffer;

    use vala_bifrost_redux::forge::{ForgeConfig, ForgeError, deterministic_output_path_for_test};

    use vala_sql::row_types::forge_operations::ForgeOperationFamily;

    use wyrd_spec::vala::api::ForgeCompactionPhase;

    use crate::forge::support::*;

    /// Proves terminal file-list history neither protects an expired object nor
    /// is required to reclaim it.
    ///
    /// Physical reclamation of a never-published Forge generation belongs to
    /// `orphan_gc` alone, gated on refreshed catalog, non-terminal `file_list`,
    /// and open-operation protection plus the TTL floor under the exact table
    /// lease. A terminal `file_list` row is none of those, so it must neither
    /// suppress the delete nor stand in for the protection set: one pass
    /// reclaims the aged, unreferenced generation without any terminal operation
    /// evidence naming it, which is what makes an attempt that died before its
    /// `Prepared` transition reclaimable at all.
    ///
    /// # Panics
    ///
    /// Panics when the real catalog roster, tenant row, object store, or Forge
    /// GC workflow violates exact orphan provenance.
    #[tokio::test]
    async fn terminal_file_list_history_does_not_protect_expired_object() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                min_files: 10,
                orphan_gc_ttl: Duration::from_millis(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let path = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            0,
        );
        fixture
            .staging
            .write(&path, Buffer::from(vec![1_u8]))
            .await
            .expect("orphan object");
        let now = chrono::Utc::now();
        let orphan_partition = vala_bifrost_redux::catalog::layout::TimeGranularity::Day
            .bucket(now)
            .expect("the current instant always buckets");
        sqlx::query(
            "INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path, file_size,
                row_count, min_event_time, max_event_time,
                partition_granularity, partition_start,
                node_id, writer_epoch, wal_lsn_min, wal_lsn_max,
                compacted, committed_snapshot_id
             ) VALUES ($1,$2,$3,$4,$5,1,1,$6,$6,$7,$8,$9,1,1,2,true,1)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .bind(&path)
        .bind(now)
        .bind(orphan_partition.granularity_str())
        .bind(orphan_partition.start_utc())
        .bind(uuid::Uuid::now_v7())
        .execute(
            &fixture
                .pg
                .superuser_pool()
                .await
                .expect("fixture owner pool"),
        )
        .await
        .expect("terminal staging history");
        tokio::time::sleep(Duration::from_millis(5)).await;

        let deleted = fixture
            .forge
            .run_orphan_gc_for_test(&fixture.binding)
            .await
            .expect("Forge GC pass");
        assert_eq!(
            deleted, 1,
            "terminal file-list history must not suppress orphan reclamation"
        );
        assert!(fixture.staging.stat(&path).await.is_err());
    }

    /// Writes one aged, Reset-evidenced orphan object and returns its object path.
    ///
    /// Mirrors the single-orphan seeding used by
    /// [`terminal_file_list_history_does_not_protect_expired_object`]: a fresh
    /// staging object plus a prepared/reset staging-fold operation pair under a
    /// distinct operation id, which is the positive never-published evidence the
    /// GC owner requires before deleting the object. `ordinal` keeps each seeded
    /// path distinct within one table.
    ///
    /// # Panics
    ///
    /// Panics when the staging write or either operation transition fails,
    /// because successful seeding is a fixture invariant.
    async fn seed_evidenced_orphan(fixture: &Fixture, ordinal: usize) -> String {
        let path = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            ordinal,
        );
        fixture
            .staging
            .write(&path, Buffer::from(vec![1_u8]))
            .await
            .expect("seed orphan object");
        let resource = format!(
            "bifrost://{}/{}/{}",
            fixture.tenant, fixture.binding.table_ref.namespace, fixture.binding.table_ref.name
        );
        let operation_id = uuid::Uuid::now_v7();
        let prepared = operation_event(
            "forge.file_compact.prepared",
            &resource,
            reset_detail_for_output(
                operation_id,
                &resource,
                ForgeCompactionPhase::Prepared,
                &path,
            ),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &prepared, true).await;
        let reset = operation_event(
            "forge.file_compact.reset",
            &resource,
            reset_detail_for_output(operation_id, &resource, ForgeCompactionPhase::Reset, &path),
        );
        append_operation(fixture, ForgeOperationFamily::StagingFold, &reset, false).await;
        path
    }

    /// A per-run page cap defers the unscanned tail as Partial and drains across runs.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or any bounded GC pass fails.
    #[tokio::test]
    async fn orphan_gc_page_cap_defers_remainder_and_drains_across_runs() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                orphan_gc_max_list_pages: 1,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        // One entry per page against a single-page cap means each run may only
        // scan the lexicographically first object; data-prefixed orphans sort
        // ahead of the table's metadata objects.
        fixture.reads.set_list_page_entries(1);
        let mut orphans = Vec::new();
        for ordinal in 0..3 {
            orphans.push(seed_evidenced_orphan(&fixture, ordinal).await);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        let first = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("first bounded orphan pass");
        assert!(first.partial, "page cap must defer the unscanned tail");
        assert_eq!(first.deleted, 1, "one page yields exactly one deletion");

        let mut deleted = first.deleted;
        for _ in 0..8 {
            if deleted == orphans.len() {
                break;
            }
            let report = fixture
                .forge
                .run_orphan_gc_report_for_test(&fixture.binding)
                .await
                .expect("successor bounded orphan pass");
            deleted += report.deleted;
        }
        assert_eq!(
            deleted,
            orphans.len(),
            "committed-deletion progress must drain the full set across runs"
        );
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            orphans.len(),
            "each orphan is deleted exactly once across the bounded runs"
        );
        for orphan in &orphans {
            assert!(
                fixture.staging.stat(orphan).await.is_err(),
                "every drained orphan is durably removed"
            );
        }
    }

    /// An exhausted per-run wall-clock budget defers cleanly without any deletion.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or either GC pass fails.
    #[tokio::test]
    async fn orphan_gc_run_budget_defers_without_deletion() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                orphan_gc_run_budget: Duration::from_nanos(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let orphan = seed_evidenced_orphan(&fixture, 0).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        let budgeted = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("budget-bounded orphan pass");
        assert!(
            budgeted.partial,
            "an exhausted budget marks the run Partial"
        );
        assert_eq!(budgeted.deleted, 0, "no deletion happens past the budget");
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            0,
            "no delete is even attempted once the budget is exhausted"
        );
        assert!(
            fixture.staging.stat(&orphan).await.is_ok(),
            "the deferred orphan survives the budgeted run intact for a successor"
        );
    }

    /// One batch loads maintenance protection a bounded number of times, not per candidate.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or the batched GC pass fails.
    #[tokio::test]
    async fn orphan_gc_loads_protection_once_per_batch() {
        const CANDIDATES: usize = 5;
        let load_table_calls = Arc::new(AtomicUsize::new(0));
        let fail_load = Arc::new(AtomicBool::new(false));
        let decorator_calls = Arc::clone(&load_table_calls);
        let decorator_fail = Arc::clone(&fail_load);
        let fixture = Fixture::build(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
            None,
            Some(Box::new(move |inner| {
                Arc::new(ProbeCatalog {
                    inner,
                    load_table_calls: decorator_calls,
                    fail_load: decorator_fail,
                }) as Arc<dyn Catalog>
            })),
        )
        .await;
        for ordinal in 0..CANDIDATES {
            seed_evidenced_orphan(&fixture, ordinal).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        load_table_calls.store(0, Ordering::Release);
        let report = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("batched orphan pass");
        assert_eq!(report.deleted, CANDIDATES, "the full batch is deleted");
        assert!(!report.partial, "the batch completes in one run");
        let loads = load_table_calls.load(Ordering::Acquire);
        assert!(
            loads < CANDIDATES,
            "protection loads ({loads}) must be independent of the candidate count ({CANDIDATES})"
        );
    }

    /// A protection-load failure fails the run closed and attempts no deletion.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup or seeding fails.
    #[tokio::test]
    async fn orphan_gc_fails_closed_when_protection_load_fails() {
        let load_table_calls = Arc::new(AtomicUsize::new(0));
        let fail_load = Arc::new(AtomicBool::new(false));
        let decorator_calls = Arc::clone(&load_table_calls);
        let decorator_fail = Arc::clone(&fail_load);
        let fixture = Fixture::build(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_millis(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
            None,
            Some(Box::new(move |inner| {
                Arc::new(ProbeCatalog {
                    inner,
                    load_table_calls: decorator_calls,
                    fail_load: decorator_fail,
                }) as Arc<dyn Catalog>
            })),
        )
        .await;
        let orphan = seed_evidenced_orphan(&fixture, 0).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        fail_load.store(true, Ordering::Release);
        let error = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect_err("protection load failure must fail the run closed");
        assert!(matches!(error, ForgeError::Catalog(_)), "error: {error:?}");
        assert_eq!(
            fixture.reads.total_delete_attempts(),
            0,
            "a fail-closed run attempts no deletion"
        );
        assert!(
            fixture.staging.stat(&orphan).await.is_ok(),
            "the orphan survives a fail-closed run"
        );
    }

    /// Journey: every unreferenced attempt generation aged past the TTL floor
    /// is deleted in one bounded orphan-GC run, whether or not it left terminal
    /// Reset evidence, while an object inside the floor survives.
    ///
    /// The eligibility predicate
    /// (`forge::orphan_gc::ProtectionSnapshot::gc_eligibility`) protects an
    /// object by the live set and the TTL floor alone. It deliberately does not
    /// require a terminal operation row: an output whose rewrite died before it
    /// could persist its `Prepared` transition leaves no evidence at all, and
    /// requiring evidence would strand that object forever. Reset evidence is
    /// therefore seeded on one of the two aged orphans purely to prove it makes
    /// no difference to the outcome.
    ///
    /// Gated out of the fast lane because it sleeps past a real TTL boundary.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup, seeding, or the GC pass fails.
    #[tokio::test]
    #[ignore = "journey: real-time TTL boundary, run in the gated lane"]
    async fn orphan_gc_deletes_aged_attempt_generations_and_preserves_young() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                orphan_gc_ttl: Duration::from_secs(1),
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        // Aged past the TTL and carrying terminal Reset evidence.
        let evidenced = seed_evidenced_orphan(&fixture, 0).await;
        // Aged past the TTL with no evidence of any kind: the abandoned-rewrite
        // shape the TTL floor exists to reclaim.
        let unevidenced = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            1,
        );
        fixture
            .staging
            .write(&unevidenced, Buffer::from(vec![2_u8]))
            .await
            .expect("aged unevidenced object");
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        // Written after the sleep: within the TTL floor and must survive.
        let young = deterministic_output_path_for_test(
            &fixture.binding.object_prefix,
            uuid::Uuid::now_v7(),
            2,
        );
        fixture
            .staging
            .write(&young, Buffer::from(vec![3_u8]))
            .await
            .expect("young object");

        let report = fixture
            .forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("journey orphan pass");
        assert_eq!(
            report.deleted, 2,
            "both aged attempt generations are reclaimed regardless of evidence"
        );
        assert!(
            fixture.staging.stat(&evidenced).await.is_err(),
            "the evidenced aged orphan is removed"
        );
        assert!(
            fixture.staging.stat(&unevidenced).await.is_err(),
            "the unevidenced aged orphan is removed by the TTL floor alone"
        );
        assert!(
            fixture.staging.stat(&young).await.is_ok(),
            "the young object survives the TTL floor"
        );
    }

    /// Proves retained-snapshot traversal fails closed above its configured cap.
    ///
    /// # Panics
    ///
    /// Panics when fixture setup or catalog commits fail, or traversal accepts
    /// more retained snapshots than configured.
    async fn assert_retained_snapshot_cap() {
        let capped = ForgeConfig {
            max_retained_snapshots_per_table: 1,
            ..ForgeConfig::default()
        };
        let fixture = Fixture::new_with_config(capped, true, 4, fixture_snapshot()).await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("first capped table");
        fixture.seed_files_at(99, 1, true).await;
        fixture.append_seed_manifest(&table, 99).await;
        assert!(
            fixture
                .forge
                .load_maintenance_protection_for_test(&fixture.binding)
                .await
                .is_err()
        );
    }

    /// Proves the production loader traverses real retained catalog history and statistics.
    ///
    /// # Panics
    ///
    /// Panics when fixture commits fail, a real retained object is omitted, or
    /// the retained-snapshot cap does not fail closed.
    #[tokio::test]
    async fn maintenance_protection_real_catalog_inventory_and_cap() {
        let fixture =
            Fixture::new_with_config(ForgeConfig::default(), true, 4, fixture_snapshot()).await;
        fixture.schedule_and_execute().await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table with data snapshot");
        let snapshot_id = table
            .metadata()
            .current_snapshot_id()
            .expect("data snapshot id");
        let statistics_key = format!("{}/metadata/stats.puffin", fixture.binding.object_prefix);
        fixture
            .staging
            .write(&statistics_key, Buffer::from(vec![1_u8]))
            .await
            .expect("statistics object");
        let statistics = StatisticsFile {
            snapshot_id,
            statistics_path: format!("{}/metadata/stats.puffin", table.metadata().location()),
            file_size_in_bytes: 1,
            file_footer_size_in_bytes: 0,
            key_metadata: None,
            blob_metadata: Vec::new(),
        };
        let update = Transaction::new(&table)
            .update_statistics()
            .set_statistics(statistics);
        ApplyTransactionAction::apply(update, Transaction::new(&table))
            .expect("statistics action")
            .commit(fixture.catalog.as_ref())
            .await
            .expect("statistics commit");

        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("retained table");
        let protection = fixture
            .forge
            .load_maintenance_protection_for_test(&fixture.binding)
            .await
            .expect("production protection");
        let paths = protection.paths_for_test();
        assert!(paths.contains(&statistics_key));
        assert!(
            table
                .metadata()
                .metadata_log()
                .iter()
                .all(|entry| paths.iter().any(|path| entry.metadata_file.ends_with(path)))
        );
        for snapshot in table.metadata().snapshots() {
            assert!(
                paths
                    .iter()
                    .any(|path| snapshot.manifest_list().ends_with(path))
            );
            let manifests = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .expect("manifest list");
            for manifest_file in manifests.entries() {
                assert!(
                    paths
                        .iter()
                        .any(|path| manifest_file.manifest_path.ends_with(path))
                );
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .expect("manifest");
                for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                    assert!(
                        paths
                            .iter()
                            .any(|path| entry.data_file().file_path().ends_with(path))
                    );
                }
            }
        }

        assert_retained_snapshot_cap().await;
    }
}

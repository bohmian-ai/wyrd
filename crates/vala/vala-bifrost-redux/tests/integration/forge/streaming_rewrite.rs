//! Streaming rewrite output: envelope limits, chunk-failure retry without
//! duplicate publication, and bounded upload buffers.
//!
//! Module of the `forge` group; shared fixtures live in `support.rs`.
//!
//! The tests stay wrapped in `mod pg_tests` so the fast lane's `--skip pg_tests`
//! filter still excludes them.

mod pg_tests {

    use std::sync::atomic::Ordering;

    use vala_bifrost_redux::forge::ForgeConfig;

    use crate::forge::support::*;

    /// SQL projection and audit parity columns for one staging operation.
    type StagingParityRow = (String, i64, Option<i64>, bool, Option<bool>);

    /// Validate the field metrics emitted for one committed Parquet file.
    fn assert_output_metrics(data_file: &iceberg::spec::DataFile) {
        assert_eq!(
            data_file.file_format(),
            iceberg::spec::DataFileFormat::Parquet
        );
        assert!(data_file.file_size_in_bytes() > 0);
        // One user column plus the nine managed columns appended by
        // `with_managed_columns`; every leaf column carries value, size, and
        // null-count metrics. Fields 2 and 3 (`run_id`, `card_uid`) are the two
        // nullable correlation columns and are entirely null in this fixture, so
        // they carry no lower/upper bounds while the eight non-null columns do.
        let expected = (1..=10).collect();
        let expected_bounds = [1, 4, 5, 6, 7, 8, 9, 10].into_iter().collect();
        assert_eq!(
            data_file
                .value_counts()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert_eq!(
            data_file
                .column_sizes()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert_eq!(
            data_file
                .lower_bounds()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected_bounds
        );
        assert_eq!(
            data_file
                .upper_bounds()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected_bounds
        );
        assert!(
            data_file
                .value_counts()
                .values()
                .all(|count| *count == data_file.record_count())
        );
        assert_eq!(
            data_file
                .null_value_counts()
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert_eq!(
            data_file.null_value_counts().values().copied().sum::<u64>(),
            data_file.record_count() * 2,
            "the production schema carries two nullable correlation columns"
        );
        assert!(data_file.nan_value_counts().is_empty());
        let offsets = data_file.split_offsets().expect("Parquet split offsets");
        assert!(!offsets.is_empty());
        assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// Read the live manifest and validate every committed output file.
    async fn committed_output_totals(fixture: &Fixture) -> (usize, u64) {
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("committed table");
        let snapshot = table.metadata().current_snapshot().expect("snapshot");
        let expected_sort_order_id = i32::try_from(table.metadata().default_sort_order_id())
            .expect("fixture sort order ID fits i32");
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("manifest list");
        let mut files = 0;
        let mut rows = 0;
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .expect("manifest");
            for entry in manifest.entries() {
                if entry.is_alive() {
                    files += 1;
                    let data_file = entry.data_file();
                    rows += data_file.record_count();
                    assert_output_metrics(data_file);
                    assert_eq!(data_file.sort_order_id(), Some(expected_sort_order_id));
                }
            }
        }
        (files, rows)
    }

    /// Assert the staging-fold projection references the exact prepared and terminal audits.
    ///
    /// # Panics
    ///
    /// Panics when the tenant query fails or the completed fixture operation
    /// does not have one parity-valid projection row.
    async fn assert_staging_transition_parity(fixture: &Fixture) {
        let mut conn = fixture
            .pg
            .vala_postgres()
            .tenant_conn(fixture.tenant)
            .await
            .expect("staging parity tenant connection");
        let rows: Vec<StagingParityRow> = sqlx::query_as(
            r"SELECT state.phase,
                     state.prepared_audit_seq,
                     state.terminal_audit_seq,
                     state.prepared_detail = prepared.detail::jsonb,
                     state.current_detail = terminal.detail::jsonb
                FROM vala.forge_operation_state AS state
                JOIN vala.audit_outbox AS prepared
                  ON prepared.data_tenant_id = state.data_tenant_id
                 AND prepared.seq = state.prepared_audit_seq
                LEFT JOIN vala.audit_outbox AS terminal
                  ON terminal.data_tenant_id = state.data_tenant_id
                 AND terminal.seq = state.terminal_audit_seq
               WHERE state.data_tenant_id = wyrd.current_tenant()
                 AND state.family = 'staging_fold'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("staging projection parity query");

        assert_eq!(rows.len(), 1, "one staging operation must be projected");
        let (phase, prepared_seq, terminal_seq, prepared_matches, terminal_matches) = &rows[0];
        assert_eq!(phase, "committed");
        assert!(*prepared_seq > 0);
        assert!(terminal_seq.is_some());
        assert!(*prepared_matches);
        assert_eq!(*terminal_matches, Some(true));
    }

    /// Real Parquet inputs spill, rotate, and conserve rows under one Forge operation.
    #[tokio::test]
    async fn constrained_streaming_rewrite_respects_complete_envelope() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_files_per_bin: 32,
                max_files_per_tick: 32,
                max_bins_per_tick: 32,
                max_concurrent_reads: 2,
                max_memory_bytes: 256 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("benchmark-shaped table");
        fixture.set_live_target_file_size(&table, 64 * 1024).await;
        // Thirty-two 100k-row files make the sort exceed the bounded 16 MiB
        // pool while exactly consuming the benchmark-shaped shared file budget.
        fixture.seed_files(32, true).await;
        vala_bifrost_redux::resources::reset_memory_peak_for_test();
        vala_bifrost_redux::forge::reset_scratch_peak_for_test();
        let outcome = fixture.schedule_and_execute().await;
        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        let envelope: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT decoded_batch_bytes,decoded_input_bytes,sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,upload_chunk_bytes,sort_spill_bytes,estimated_memory_bytes,footer_encoded_bytes FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(fixture.tenant.as_uuid())
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("persisted execution envelope");
        assert_eq!(fixture.reads.whole_reads.load(Ordering::Relaxed), 0);
        assert!(fixture.reads.ranged_reads.load(Ordering::Relaxed) > 0);
        assert!(
            fixture.reads.peak_reads.load(Ordering::Relaxed)
                <= usize::try_from(envelope.1 / envelope.0).expect("reader permits")
        );
        assert!(
            fixture.reads.output_chunks.load(Ordering::Acquire) > 1,
            "a multi-output rewrite must traverse the bounded chunk seam"
        );
        assert!(
            fixture.reads.largest_output_chunk.load(Ordering::Acquire)
                <= usize::try_from(envelope.5).expect("upload term"),
            "no upload write may exceed the persisted upload term"
        );
        let peak_memory = vala_bifrost_redux::resources::memory_peak_for_test();
        assert!(
            peak_memory <= usize::try_from(envelope.7).expect("resident total"),
            "leased-pool peak must remain independent of total output: {peak_memory}"
        );
        assert_eq!(envelope.2, 2 * envelope.0 + envelope.3);
        assert_eq!(
            envelope.7,
            envelope.1 + envelope.2 + envelope.4 + envelope.5 + envelope.8
        );
        assert!(
            vala_bifrost_redux::resources::memory_consumer_peak_for_test(
                "forge-rewrite-decoded-batch"
            ) <= usize::try_from(envelope.0).expect("decoded term")
        );
        assert!(
            vala_bifrost_redux::resources::memory_consumer_peak_for_test("forge-rewrite-output")
                <= usize::try_from(envelope.4 + envelope.5).expect("output resident terms")
        );
        let sort_peak = vala_bifrost_redux::resources::memory_consumer_peak_for_test("Sort");
        assert!(
            sort_peak <= usize::try_from(envelope.2).expect("sort working term"),
            "sort peak {sort_peak} exceeds persisted term {}",
            envelope.2
        );
        assert!(
            vala_bifrost_redux::forge::scratch_peak_for_test()
                <= u64::try_from(envelope.6).expect("sort spill term")
        );
        let released = fixture
            .forge
            .resources_for_test()
            .snapshot()
            .expect("released resource snapshot");
        assert_eq!(released.elastic_memory_used_bytes, 0);
        assert_eq!(released.scratch_used_bytes, 0);
        assert_eq!(released.forge_reader_permits_used, 0);
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files >= 2, "rotation must commit multiple outputs");
        assert_eq!(
            output_rows, 3_200_000,
            "rewrite must conserve every input row"
        );
        assert_staging_transition_parity(&fixture).await;
    }

    /// A failed output chunk aborts and retries the sealed file in one attempt.
    #[tokio::test]
    async fn streaming_output_chunk_failure_retries_without_duplicate_publication() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_files_per_bin: 4,
                max_files_per_tick: 4,
                max_bins_per_tick: 4,
                max_concurrent_reads: 1,
                max_memory_bytes: 256 * 1024 * 1024,
                spill_limit_bytes: 1024 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        fixture.seed_files(2, true).await;
        fixture.reads.fail_next_output_chunk();
        let snapshots_before = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table before retry")
            .metadata()
            .snapshots()
            .count();

        let outcome = fixture.schedule_and_execute().await;

        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert_eq!(
            fixture.reads.output_writer_aborts.load(Ordering::Acquire),
            1
        );
        assert_eq!(
            fixture.reads.output_writer_opens.load(Ordering::Acquire),
            fixture.reads.output_put_calls() + 1,
            "the one aborted writer is reopened without duplicating a successful output"
        );
        assert!(
            fixture.reads.output_chunks.load(Ordering::Acquire) >= 2,
            "the injected failure must be followed by a same-attempt retry"
        );
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files > 0, "the retry publishes its output set");
        assert_eq!(output_rows, 200_000, "the retry must conserve all rows");
        let snapshots_after = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("table after retry")
            .metadata()
            .snapshots()
            .count();
        assert_eq!(
            snapshots_after,
            snapshots_before + 1,
            "one task attempt must produce exactly one Iceberg publication"
        );
        assert_staging_transition_parity(&fixture).await;
    }

    /// One output larger than the upload bound reaches the store as many chunks.
    #[tokio::test]
    async fn scaled_upload_uses_exact_chunk_and_bounded_buffer() {
        let fixture = Fixture::new_with_config(
            ForgeConfig {
                max_files_per_bin: 64,
                max_files_per_tick: 64,
                max_bins_per_tick: 64,
                max_concurrent_reads: 2,
                max_memory_bytes: 512 * 1024 * 1024,
                spill_limit_bytes: 2 * 1024 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            false,
            0,
            fixture_snapshot(),
        )
        .await;
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("chunking table");
        fixture
            .set_live_target_file_size(&table, 768 * 1024 * 1024)
            .await;
        fixture.seed_files(64, true).await;
        vala_bifrost_redux::resources::reset_memory_peak_for_test();

        let outcome = fixture.schedule_and_execute().await;

        assert_eq!(outcome.tasks_enqueued, 1, "outcome: {outcome:?}");
        assert!(
            fixture.reads.output_put_calls() > 1,
            "a target above the physical cap must rotate under one operation"
        );
        assert!(
            fixture.reads.output_chunks.load(Ordering::Acquire) > 1,
            "one output above 8 MiB must use multiple writes"
        );
        assert!(fixture.reads.largest_output_chunk.load(Ordering::Acquire) <= 8 * 1024 * 1024);
        let peak_memory = vala_bifrost_redux::resources::memory_peak_for_test();
        assert!(
            peak_memory <= 512 * 1024 * 1024,
            "decoded, encoder, upload, and sort consumers must stay within the lease: {peak_memory}"
        );
        assert!(
            peak_memory < 768 * 1024 * 1024,
            "peak governed memory must remain below the configured output target"
        );
        let (output_files, output_rows) = committed_output_totals(&fixture).await;
        assert!(output_files > 1);
        assert_eq!(
            output_files,
            fixture.reads.output_put_calls(),
            "every capped physical output is committed under the one operation"
        );
        assert_eq!(output_rows, 6_400_000);
    }
}

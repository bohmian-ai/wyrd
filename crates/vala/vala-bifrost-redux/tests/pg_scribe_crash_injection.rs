mod pg_tests {
    //! Crash injection matrix for Scribe seal state machine.
    //!
    //! Tests verify:
    //! - Crash at each seal stage → no data loss, no duplicates after replay
    //!
    //! Skipped when WYRD_DATABASE_URL is unset (credential-free default suite).

    /// Fault injection points in the seal state machine.
    #[cfg(test)]
    #[allow(dead_code)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FaultPoint {
        /// Crash mid-WAL-record write (torn record, CRC will fail on replay).
        MidRecordWrite,
        /// Crash during segment roll (new segment partially written).
        MidSegmentRoll,
        /// Crash during manifest rename (atomic rename may or may not complete).
        MidManifestRename,
        /// Crash after Parquet PUT but before PG commit (object exists, no file_list row).
        PostParquetPutPreCommit,
        /// Crash after PG commit but before WAL retire (sealed LSN watermark not advanced).
        PostCommitPreWalRetire,
    }

    #[tokio::test]
    #[ignore = "requires fault-injection infrastructure + WAL replay"]
    async fn pg_scribe_crash_injection_matrix() {
        // This test would verify crash-recovery correctness at every seal stage:
        //
        // Test matrix (5 fault points):
        // 1. MidRecordWrite: Crash while writing a WAL record → torn tail, replay truncates
        // 2. MidSegmentRoll: Crash while rolling to new segment → incomplete segment
        // 3. MidManifestRename: Crash during atomic manifest rename → may or may not complete
        // 4. PostParquetPutPreCommit: Crash after object store PUT, before PG tx commit
        // 5. PostCommitPreWalRetire: Crash after file_list INSERT commit, before WAL retire
        //
        // For each fault point:
        // - Start ScribeImpl (writer_epoch=1)
        // - Append 100k rows across 3 seal-keys (3 tenants or 3 days)
        // - Configure fault injection to trigger at the target point
        // - Attempt seal → crash at fault point
        // - Restart ScribeImpl (writer_epoch=2, same WAL + object store)
        // - Replay reconstructs memtable from WAL
        // - Force seal (replay-driven)
        //
        // Verification (all fault points):
        // - Every acknowledged batch (batch_id) appears in file_list exactly once
        // - audit_outbox has correct per-append rows (no duplicates, no missing)
        // - wal_lsn_min/max ranges are correct and non-overlapping across seal-keys
        // - Parquet files are readable and contain exact expected row counts
        // - No orphaned Parquet files (PostParquetPutPreCommit case)
        //
        // Implementation requires:
        // - Fault injection seams in:
        //   - WalWriter::append_and_fsync (MidRecordWrite)
        //   - WalWriter::roll_segment (MidSegmentRoll)
        //   - Manifest::update (MidManifestRename)
        //   - SealDriver::pre_commit (PostParquetPutPreCommit)
        //   - SealDriver::post_commit (PostCommitPreWalRetire)
        // - Test harness that can:
        //   - Configure fault points via environment or test-only API
        //   - Restart ScribeImpl with shared WAL/object-store state
        //   - Verify file_list + audit_outbox correctness
        //
        // Deferred until fault-injection harness + WAL replay + manifest integration
        // are complete.
    }
}

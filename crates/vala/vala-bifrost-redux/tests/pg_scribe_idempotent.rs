mod pg_tests {
    //! Idempotency tests for Scribe seal.
    //!
    //! Tests verify:
    //! - Duplicate batch_id across appends is idempotent
    //! - Replay-driven re-seal hits unique index, no duplicate rows
    //!
    //! Skipped when WYRD_DATABASE_URL is unset (credential-free default suite).

    #[tokio::test]
    #[ignore = "requires WAL replay deduplication infrastructure"]
    async fn pg_scribe_duplicate_batch_id_is_idempotent() {
        // This test would verify that if the same batch_id is sent in two append
        // calls, the WAL deduplication logic (reading batch_id from the audit record
        // header) skips the duplicate and the memtable + file_list contain only one
        // set of rows.
        //
        // Implementation requires:
        // 1. WAL replay deduplication by batch_id (currently stubbed)
        // 2. Memtable restart-from-WAL integration
        // 3. Test harness that can restart ScribeImpl and verify memtable state
        //
        // Deferred until WAL replay lands end-to-end.
    }

    #[tokio::test]
    #[ignore = "requires WAL replay infrastructure"]
    async fn pg_scribe_replay_reseal_hits_unique_index() {
        // This test would verify idempotent seal behavior during replay:
        //
        // 1. Append + seal + commit successfully → file_list row written
        // 2. Simulate restart without manifest advance (manifest.sealed_lsn still 0)
        // 3. Replay reconstructs memtable from WAL
        // 4. Force seal again on same data
        // 5. Second seal's file_list INSERT hits unique index
        //    (node_id, writer_epoch, wal_lsn_min, wal_lsn_max)
        // 6. ON CONFLICT DO NOTHING succeeds, no duplicate audit rows
        //
        // Implementation requires:
        // - Full WAL replay infrastructure (extract_seal_key_from_path, real segment reading)
        // - Manifest read/write integration
        // - Test harness that can simulate restarts with controlled manifest state
        //
        // Deferred until WAL replay + manifest integration are complete.
    }
}

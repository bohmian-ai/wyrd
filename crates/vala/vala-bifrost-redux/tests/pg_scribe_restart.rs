mod pg_tests {
    //! Restart and replay tests for Scribe.
    //!
    //! Tests verify:
    //! - Interleaved tenant LSNs seal correctly after crash/replay
    //! - Replay-driven seal preserves per-append audit with correct `principal_id`
    //! - Restart reproduces `file_list` rows with prior `writer_epoch`
    //!
    //! Skipped when `WYRD_DATABASE_URL` is unset (credential-free default suite).

    #[tokio::test]
    #[ignore = "requires WAL replay + multi-tenant test harness"]
    async fn pg_scribe_two_tenants_interleaved_lsn_crash_replay() {
        // This test would verify correct tenant isolation during replay:
        //
        // 1. Start ScribeImpl with writer_epoch=1
        // 2. Append rows for tenant A (vala.events, 2026-07-14)
        // 3. Append rows for tenant B (vala.events, 2026-07-14) → LSNs interleave in WAL
        // 4. Force seal for tenant A only → file_list row for A, audit rows for A
        // 5. Simulate crash (drop ScribeImpl without sealing B)
        // 6. Restart ScribeImpl with writer_epoch=2, same WAL directory
        // 7. Replay reads WAL, skips A's sealed LSNs (via manifest), reconstructs B's memtable
        // 8. Force seal for B → file_list row for B
        //
        // Verification:
        // - file_list has exactly 2 rows (one per tenant)
        // - A's row has writer_epoch=1, B's row has writer_epoch=1 (replayed epoch)
        // - audit_outbox has correct row counts per tenant
        // - No duplicate rows
        //
        // Implementation requires:
        // - Full WAL replay (extract_seal_key_from_path with real tenant extraction)
        // - Manifest read/write integration
        // - Test harness that can restart ScribeImpl with shared WAL directory
        //
        // Deferred until WAL replay + manifest integration are complete.
    }

    #[tokio::test]
    #[ignore = "requires WAL replay with stream-identity preservation"]
    async fn pg_scribe_restart_replay_preserves_per_append_audit() {
        // This test would verify that WAL replay preserves per-append stream identity
        // (principal_id, request_id) across restarts:
        //
        // 1. Start ScribeImpl with writer_epoch=1
        // 2. Make 5 appends from 3 distinct principals (varying principal_id)
        // 3. Simulate crash before seal (drop ScribeImpl, leave WAL + memtable unsealed)
        // 4. Restart ScribeImpl with writer_epoch=2, same WAL directory
        // 5. Replay reconstructs memtable from WAL (including AuditEvent list)
        // 6. Force seal
        //
        // Verification:
        // - audit_outbox has exactly 5 rows
        // - Each row has correct principal_id (matches original append's Principal)
        // - Each row has correct request_id (matches original append's request_id)
        // - file_list row has writer_epoch=1 (the epoch from original WAL writes)
        //
        // Implementation requires:
        // - WAL replay that reconstructs AuditEvent list with full stream identity
        // - Test harness that can capture original principal_id values and verify post-replay
        //
        // Deferred until WAL replay with stream-identity preservation lands.
    }

    #[tokio::test]
    #[ignore = "requires WAL replay infrastructure"]
    async fn pg_scribe_restart_reproduces_rows_exactly() {
        // This test would verify that replay-driven seal produces identical file_list
        // metadata to a non-restart seal:
        //
        // 1. Start ScribeImpl with writer_epoch=1
        // 2. Append 10k rows (vala.events, 2026-07-14)
        // 3. Simulate crash before seal
        // 4. Restart ScribeImpl with writer_epoch=2, same WAL directory
        // 5. Replay reconstructs memtable from WAL
        // 6. Force seal
        //
        // Verification:
        // - file_list has exactly 1 row
        // - row_count = 10,000
        // - wal_lsn_min, wal_lsn_max match the LSN range from original WAL writes
        // - writer_epoch = 1 (original epoch, not restart epoch)
        // - partition_day = 2026-07-14
        // - Parquet file at file_path is readable and contains exact 10k rows
        //
        // Implementation requires:
        // - Full WAL replay
        // - Manifest integration (so restart knows sealed_lsn watermark)
        // - Test harness that can restart ScribeImpl and verify file_list stability
        //
        // Deferred until WAL replay integration is complete.
    }
}

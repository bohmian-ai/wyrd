mod pg_tests {
    //! Backpressure and error-code tests for Scribe.
    //!
    //! Tests verify:
    //! - WAL disk full returns `ScribeError::WalDiskFull`
    //! - Memtable full + blocked seal returns `ScribeError::IngestBusy`
    //!
    //! Skipped when `WYRD_DATABASE_URL` is unset (credential-free default suite).

    #[tokio::test]
    #[ignore = "requires WAL disk-full fault injection"]
    async fn pg_scribe_wal_disk_full_returns_507() {
        // This test would verify that when the WAL directory's filesystem is full
        // (ENOSPC on write or fsync), the append call returns ScribeError::WalDiskFull,
        // which maps to HTTP 507 Insufficient Storage via to_bifrost_error().
        //
        // Implementation requires:
        // 1. Fault injection seam in WalWriter that can simulate ENOSPC
        // 2. Test harness that configures a WAL with injected disk-full behavior
        // 3. Verification that append() returns Err(ScribeError::WalDiskFull)
        //
        // Current WAL implementation detects ENOSPC (io::ErrorKind::StorageFull
        // or raw_os_error == 28) and maps it to WalDiskFull. Test needs the
        // injection infrastructure to trigger that path.
        //
        // Deferred until fault injection harness lands.
    }

    #[tokio::test]
    #[ignore = "requires async seal + backpressure flow"]
    async fn pg_scribe_memtable_full_returns_429() {
        // This test would verify that when the memtable is full (row count or
        // byte size exceeds threshold) AND the background seal is blocked (e.g.,
        // object store slow, Postgres tx timeout), a new append call returns
        // ScribeError::IngestBusy { table }, which maps to HTTP 429 Too Many Requests.
        //
        // Implementation requires:
        // 1. Configurable memtable capacity thresholds (not just seal predicate)
        // 2. Background seal driver (currently seal is synchronous via force_seal)
        // 3. Test harness that can inject seal delays and fill memtable to capacity
        // 4. Verification that append() returns Err(ScribeError::IngestBusy { table })
        //
        // Current implementation has no capacity-based backpressure; seal predicate
        // triggers at 50k rows / 128 MiB / 1s / 5s inactivity, but seal is blocking.
        // Real async seal + bounded memtable capacity needed for 429 flow.
        //
        // Deferred until async seal lands.
    }
}

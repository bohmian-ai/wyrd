use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;

/// Parquet [`WriterProperties`] for every Bifrost data file.
///
/// ZSTD level 3 (writes are once-per-batch via 2PC; reads are scan-bound, so the
/// extra compression over SNAPPY is worth it). Row groups are capped by ROW COUNT
/// (~1M ≈ 128 MB) because parquet-58 has no byte-based row-group flush.
///
/// Bloom filters are intentionally OFF: Stage-1 pruning is time-range only
/// (`day(wyrd_event_time)` partition + row-group min/max). `data_tenant_id` is not
/// a partition column, so tenant scoping is a post-scan correctness filter, not
/// file pruning — a bloom buys nothing for the Stage-1 access pattern. Tenant
/// file-pruning is the Stage-5 tenant-bucket transform.
pub fn bifrost_writer_properties() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("zstd level 3 is valid"),
        ))
        .set_max_row_group_row_count(Some(1_000_000))
        .build()
}

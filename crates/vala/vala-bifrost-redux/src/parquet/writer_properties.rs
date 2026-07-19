//! Uniform Parquet writer properties for Bifrost data files.

use parquet::basic::{Compression, Encoding, ZstdLevel};
use parquet::file::properties::{EnabledStatistics, WriterProperties};
use parquet::schema::types::ColumnPath;

const MAX_ROW_GROUP_ROWS: usize = 131_072;
const BLOOM_FPP: f64 = 0.01;
const BLOOM_COLUMNS: [&str; 5] = [
    "data_tenant_id",
    "run_id",
    "card_uid",
    "trace_id",
    "span_id",
];

fn bloom_filter_ndv(row_count: usize) -> u64 {
    let ndv =
        u64::try_from(row_count.min(MAX_ROW_GROUP_ROWS)).expect("bounded row count fits in u64");
    if ndv > 1_000 {
        (ndv / 100).max(1_000)
    } else {
        ndv
    }
}

/// Parquet [`WriterProperties`] for every Bifrost data file.
///
/// ZSTD level 3 (writes are once-per-batch via 2PC; reads are scan-bound, so the
/// extra compression over SNAPPY is worth it). Row groups are capped at 131,072
/// rows because parquet-58 has no byte-based row-group flush.
///
/// Bloom filters are enabled only for the identity and correlation columns used
/// by tenant- and trace-scoped reads. Payload and message columns stay
/// unallowlisted so they cannot inflate every file's footer.
///
/// # Panics
/// Never panics — ZSTD level 3 is always valid.
pub fn bifrost_writer_properties(row_count: usize) -> WriterProperties {
    let bloom_ndv = bloom_filter_ndv(row_count);
    let mut builder = WriterProperties::builder()
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("zstd level 3 is valid"),
        ))
        .set_max_row_group_row_count(Some(MAX_ROW_GROUP_ROWS))
        .set_dictionary_enabled(false)
        .set_column_encoding(
            ColumnPath::from("wyrd_event_time"),
            Encoding::DELTA_BINARY_PACKED,
        )
        .set_statistics_enabled(EnabledStatistics::Page)
        .set_offset_index_disabled(false);

    for column in BLOOM_COLUMNS {
        let path = ColumnPath::from(column);
        builder = builder
            .set_column_bloom_filter_enabled(path.clone(), true)
            .set_column_bloom_filter_fpp(path.clone(), BLOOM_FPP)
            .set_column_bloom_filter_ndv(path, bloom_ndv);
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_recipe_metadata_is_deterministic() {
        let properties = bifrost_writer_properties(50_000);
        let timestamp = ColumnPath::from("wyrd_event_time");

        assert_eq!(
            properties.compression(&timestamp),
            Compression::ZSTD(ZstdLevel::try_new(3).expect("zstd level 3 is valid"))
        );
        assert_eq!(
            properties.max_row_group_row_count(),
            Some(MAX_ROW_GROUP_ROWS)
        );
        assert!(!properties.dictionary_enabled(&timestamp));
        assert_eq!(
            properties.encoding(&timestamp),
            Some(Encoding::DELTA_BINARY_PACKED)
        );
        assert_eq!(
            properties.statistics_enabled(&timestamp),
            EnabledStatistics::Page
        );
        assert!(!properties.offset_index_disabled());

        for column in BLOOM_COLUMNS {
            let properties_for_column = properties
                .bloom_filter_properties(&ColumnPath::from(column))
                .expect("allowlisted column has a bloom filter");
            assert!((properties_for_column.fpp - BLOOM_FPP).abs() < f64::EPSILON);
            assert_eq!(properties_for_column.ndv, 1_000);
        }

        for column in ["message", "payload", "value"] {
            assert!(
                properties
                    .bloom_filter_properties(&ColumnPath::from(column))
                    .is_none(),
                "unallowlisted column {column} must not have a bloom filter"
            );
        }
    }

    #[test]
    fn writer_recipe_bloom_ndv_is_bounded() {
        assert_eq!(bloom_filter_ndv(1_000), 1_000);
        assert_eq!(bloom_filter_ndv(50_000), 1_000);
        assert_eq!(bloom_filter_ndv(200_000), 1_310);
    }
}

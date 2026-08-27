//! Uniform Parquet writer properties for Bifrost data files.

use parquet::basic::{Compression, Encoding, ZstdLevel};
use parquet::file::metadata::KeyValue;
use parquet::file::properties::{EnabledStatistics, WriterProperties};
use parquet::schema::types::ColumnPath;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Number of rows handed to parquet-rs for one internal column write batch.
pub(crate) const PARQUET_WRITE_BATCH_ROWS: usize = 8_192;

use super::memory::MAX_ROW_GROUP_ROWS;
const BLOOM_FPP: f64 = 0.01;

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
/// ZSTD level 3 (writes are once-per-frame after bounded admission; reads are
/// scan-bound, so the extra compression over SNAPPY is worth it). Row groups are capped at 131,072
/// rows because parquet-58 has no byte-based row-group flush.
///
/// Dictionary encoding is on by default so low-cardinality string and
/// identifier columns encode as `RLE_DICTIONARY`; parquet-rs owns dictionary
/// page overflow and the fallback to `PLAIN`, so no sampler or cardinality
/// estimate is computed here. `wyrd_event_time` is the one column that opts
/// out: it is monotonic microsecond data that `DELTA_BINARY_PACKED` encodes
/// strictly better than a dictionary would.
///
/// Bloom filters are enabled only for `bloom_columns`, the canonical union the
/// registered physical layout resolved from the managed floor and the table's
/// declarations. Every other column stays unallowlisted so payload and message
/// columns cannot inflate a file's footer.
///
/// # Panics
/// Never panics — ZSTD level 3 is always valid.
pub fn bifrost_writer_properties(row_count: usize, bloom_columns: &[String]) -> WriterProperties {
    bifrost_writer_properties_with_metadata(row_count, Vec::new(), bloom_columns)
}

/// Parquet properties carrying the exact writer-v2 footer metadata.
///
/// Dictionary encoding is enabled globally and disabled for
/// [`WYRD_EVENT_TIME`], which keeps `DELTA_BINARY_PACKED` as its actual
/// encoding. Both calls are required: parquet-rs treats a per-column encoding
/// request as a *fallback* used only after a dictionary overflows, so
/// `set_column_encoding` alone would leave the column dictionary-encoded.
///
/// The caller must construct metadata through the common memory-contract
/// owner so producer paths cannot invent alternate field spellings, and must
/// pass the canonical Bloom column union resolved from the table's registered
/// physical layout so every producer writes one identical footer recipe.
///
/// # Panics
///
/// Panics only if the compile-time constant Zstandard level `3` becomes invalid.
#[must_use]
pub fn bifrost_writer_properties_with_metadata(
    row_count: usize,
    metadata: Vec<KeyValue>,
    bloom_columns: &[String],
) -> WriterProperties {
    let bloom_ndv = bloom_filter_ndv(row_count);
    let mut builder = WriterProperties::builder()
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("zstd level 3 is valid"),
        ))
        .set_write_batch_size(PARQUET_WRITE_BATCH_ROWS)
        .set_max_row_group_row_count(Some(MAX_ROW_GROUP_ROWS))
        .set_dictionary_enabled(true)
        .set_column_dictionary_enabled(ColumnPath::from(WYRD_EVENT_TIME), false)
        .set_column_encoding(
            ColumnPath::from(WYRD_EVENT_TIME),
            Encoding::DELTA_BINARY_PACKED,
        )
        .set_statistics_enabled(EnabledStatistics::Page)
        .set_offset_index_disabled(false);

    if !metadata.is_empty() {
        builder = builder.set_key_value_metadata(Some(metadata));
    }

    for column in bloom_columns {
        let path = ColumnPath::from(column.as_str());
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

    /// The canonical union a traces table resolves: managed floor plus the
    /// declaration-only correlation columns.
    fn declared_recipe() -> Vec<String> {
        [
            "data_tenant_id",
            "run_id",
            "card_uid",
            "trace_id",
            "span_id",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    /// The one writer recipe every Bifrost producer builds: dictionary
    /// encoding on by default so low-cardinality columns compress, off for
    /// `wyrd_event_time` so `DELTA_BINARY_PACKED` is its real encoding rather
    /// than a post-overflow fallback, and one deterministic footer recipe
    /// marker stamped by the memory-contract owner.
    #[test]
    fn writer_recipe_encoding_contract() {
        let bloom_columns = declared_recipe();
        let properties = bifrost_writer_properties(50_000, &bloom_columns);

        for column in ["service_name", "run_id", "message", "data_tenant_id"] {
            let path = ColumnPath::from(column);
            assert!(
                properties.dictionary_enabled(&path),
                "{column} must be dictionary-eligible by default"
            );
            assert_eq!(
                properties.encoding(&path),
                None,
                "{column} must leave encoding selection to parquet-rs"
            );
        }

        let event_time = ColumnPath::from(WYRD_EVENT_TIME);
        assert!(
            !properties.dictionary_enabled(&event_time),
            "wyrd_event_time must opt out of dictionary encoding"
        );
        assert_eq!(
            properties.encoding(&event_time),
            Some(Encoding::DELTA_BINARY_PACKED)
        );

        assert_eq!(super::super::memory::WRITER_RECIPE, "bifrost-writer-v2");
    }

    #[test]
    fn writer_recipe_metadata_is_deterministic() {
        let bloom_columns = declared_recipe();
        let properties = bifrost_writer_properties(50_000, &bloom_columns);
        let timestamp = ColumnPath::from("wyrd_event_time");

        assert_eq!(
            properties.compression(&timestamp),
            Compression::ZSTD(ZstdLevel::try_new(3).expect("zstd level 3 is valid"))
        );
        assert_eq!(
            properties.max_row_group_row_count(),
            Some(MAX_ROW_GROUP_ROWS)
        );
        assert_eq!(properties.write_batch_size(), PARQUET_WRITE_BATCH_ROWS);
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

        for column in &bloom_columns {
            let properties_for_column = properties
                .bloom_filter_properties(&ColumnPath::from(column.as_str()))
                .expect("allowlisted column has a bloom filter");
            assert!((properties_for_column.fpp() - BLOOM_FPP).abs() < f64::EPSILON);
            assert_eq!(properties_for_column.ndv(), 1_000);
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

    /// A column outside the resolved union never receives a Bloom filter, so a
    /// table that does not declare `trace_id` cannot inherit another table's
    /// footer recipe.
    #[test]
    fn writer_recipe_blooms_exactly_the_resolved_union() {
        let floor = ["data_tenant_id".to_owned(), "run_id".to_owned()];
        let properties = bifrost_writer_properties(50_000, &floor);
        for column in &floor {
            assert!(
                properties
                    .bloom_filter_properties(&ColumnPath::from(column.as_str()))
                    .is_some(),
                "resolved column {column} must have a bloom filter"
            );
        }
        for column in ["trace_id", "span_id", "card_uid"] {
            assert!(
                properties
                    .bloom_filter_properties(&ColumnPath::from(column))
                    .is_none(),
                "undeclared column {column} must not have a bloom filter"
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

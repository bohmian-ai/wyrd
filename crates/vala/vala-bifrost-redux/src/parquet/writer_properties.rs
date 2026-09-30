//! Uniform Parquet writer properties for Bifrost data files.

use parquet::basic::{Compression, Encoding, ZstdLevel};
use parquet::file::metadata::KeyValue;
use parquet::file::properties::{
    DEFAULT_MAX_ROW_GROUP_ROW_COUNT, EnabledStatistics, WriterProperties,
};
use parquet::schema::types::ColumnPath;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Estimated encoded bytes at which a Bifrost writer closes a row group.
///
/// A soft throughput target, not a size or memory limit: parquet-rs flushes a
/// group once its estimated encoded size passes this value, so a group may
/// overshoot by the batch that crossed it, and a single large accepted row
/// always fits a group of its own. Forge rewrites replace it with the table's
/// declared `write.parquet.row-group-size-bytes` when one is set.
pub const BIFROST_ROW_GROUP_TARGET_BYTES: usize = 128 * 1024 * 1024;

/// Encoded bytes at which a column chunk's dictionary page overflows to `PLAIN`.
///
/// A point lookup decompresses and decodes the whole dictionary page of every
/// column chunk it reads, even when its selected pages are `PLAIN`, so this
/// bound is a per-query CPU cost for high-cardinality columns. At parquet-rs's
/// 1 MiB default the benchmark's identifier column spent ~0.66 ms per lookup
/// on its dictionary; 256 KiB cuts that to ~0.17 ms and the file shrinks,
/// while low-cardinality columns still fit their dictionaries.
const BIFROST_DICTIONARY_PAGE_BYTES: usize = 256 * 1024;

/// Target false-positive probability of every Bifrost Bloom filter.
const BLOOM_FPP: f64 = 0.01;

/// Derives a row group's Bloom-filter distinct-value hint from its row count.
///
/// # Panics
/// Never panics: the row count is first bounded by parquet-rs's row-group row
/// maximum, which fits in `u64`.
fn bloom_filter_ndv(row_count: usize) -> u64 {
    let ndv = u64::try_from(row_count.min(DEFAULT_MAX_ROW_GROUP_ROW_COUNT))
        .expect("bounded row count fits in u64");
    if ndv > 1_000 {
        (ndv / 100).max(1_000)
    } else {
        ndv
    }
}

/// Parquet [`WriterProperties`] for every Bifrost data file.
///
/// ZSTD level 3 (writes are once-per-frame after bounded admission; reads are
/// scan-bound, so the extra compression over SNAPPY is worth it). Row groups
/// close at the soft [`BIFROST_ROW_GROUP_TARGET_BYTES`] encoded target or at
/// parquet-rs's default row maximum, whichever comes first; write batch and
/// page sizing keep parquet-rs defaults.
///
/// Dictionary encoding is on by default so low-cardinality string and
/// identifier columns encode as `RLE_DICTIONARY`; parquet-rs owns dictionary
/// page overflow at [`BIFROST_DICTIONARY_PAGE_BYTES`] and the fallback to
/// `PLAIN`, so no sampler or cardinality estimate is computed here. `wyrd_event_time` is the one column that opts
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

/// Parquet properties carrying caller-supplied footer metadata.
///
/// Dictionary encoding is enabled globally and disabled for
/// [`WYRD_EVENT_TIME`], which keeps `DELTA_BINARY_PACKED` as its actual
/// encoding. Both calls are required: parquet-rs treats a per-column encoding
/// request as a *fallback* used only after a dictionary overflows, so
/// `set_column_encoding` alone would leave the column dictionary-encoded.
///
/// The caller must construct footer identity metadata through
/// [`crate::parquet::footer`] so producer paths cannot invent alternate field
/// spellings, and must pass the canonical Bloom column union resolved from the table's registered
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
    let mut builder = recipe_builder(row_count, bloom_columns);
    if !metadata.is_empty() {
        builder = builder.set_key_value_metadata(Some(metadata));
    }
    builder.build()
}

/// Parquet properties for a Forge rewrite output at a table's row-group target.
///
/// Identical to every other Bifrost producer's recipe except that the soft
/// encoded row-group target is the table's resolved
/// `write.parquet.row-group-size-bytes` instead of
/// [`BIFROST_ROW_GROUP_TARGET_BYTES`]. The row-count default is kept as well;
/// whichever bound is reached first flushes the group.
///
/// # Panics
///
/// Panics only if the compile-time constant Zstandard level `3` becomes invalid.
#[must_use]
pub fn bifrost_rewrite_writer_properties(
    row_group_target_bytes: u64,
    bloom_columns: &[String],
) -> WriterProperties {
    recipe_builder(DEFAULT_MAX_ROW_GROUP_ROW_COUNT, bloom_columns)
        .set_max_row_group_bytes(Some(
            usize::try_from(row_group_target_bytes).unwrap_or(usize::MAX),
        ))
        .build()
}

/// Builds the one shared Bifrost writer recipe every producer starts from.
///
/// Extracted so the rewrite writer cannot drift from the ingest writer: a
/// producer that encoded differently would make an otherwise-identical file
/// obsolete on the next selection pass for no semantic reason.
fn recipe_builder(
    row_count: usize,
    bloom_columns: &[String],
) -> parquet::file::properties::WriterPropertiesBuilder {
    let bloom_ndv = bloom_filter_ndv(row_count);
    let mut builder = WriterProperties::builder()
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("zstd level 3 is valid"),
        ))
        .set_max_row_group_bytes(Some(BIFROST_ROW_GROUP_TARGET_BYTES))
        .set_dictionary_enabled(true)
        .set_dictionary_page_size_limit(BIFROST_DICTIONARY_PAGE_BYTES)
        .set_column_dictionary_enabled(ColumnPath::from(WYRD_EVENT_TIME), false)
        .set_column_encoding(
            ColumnPath::from(WYRD_EVENT_TIME),
            Encoding::DELTA_BINARY_PACKED,
        )
        .set_statistics_enabled(EnabledStatistics::Page)
        .set_offset_index_disabled(false);

    for column in bloom_columns {
        let path = ColumnPath::from(column.as_str());
        builder = builder
            .set_column_bloom_filter_enabled(path.clone(), true)
            .set_column_bloom_filter_fpp(path.clone(), BLOOM_FPP)
            .set_column_bloom_filter_max_ndv(path, bloom_ndv);
    }

    builder
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
    /// encoding on by default so low-cardinality columns compress, bounded at
    /// the per-lookup dictionary decode size, and off for `wyrd_event_time` so
    /// `DELTA_BINARY_PACKED` is its real encoding rather than a post-overflow
    /// fallback.
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

        assert_eq!(
            properties.dictionary_page_size_limit(),
            BIFROST_DICTIONARY_PAGE_BYTES,
            "dictionary pages overflow at the bounded per-lookup decode size"
        );

        let event_time = ColumnPath::from(WYRD_EVENT_TIME);
        assert!(
            !properties.dictionary_enabled(&event_time),
            "wyrd_event_time must opt out of dictionary encoding"
        );
        assert_eq!(
            properties.encoding(&event_time),
            Some(Encoding::DELTA_BINARY_PACKED)
        );
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
            Some(DEFAULT_MAX_ROW_GROUP_ROW_COUNT)
        );
        assert_eq!(
            properties.max_row_group_bytes(),
            Some(BIFROST_ROW_GROUP_TARGET_BYTES)
        );
        assert_eq!(BIFROST_ROW_GROUP_TARGET_BYTES, 128 * 1024 * 1024);
        assert_eq!(
            properties.write_batch_size(),
            parquet::file::properties::DEFAULT_WRITE_BATCH_SIZE
        );
        assert_eq!(
            properties.data_page_size_limit(),
            parquet::file::properties::DEFAULT_PAGE_SIZE
        );
        assert_eq!(
            properties.data_page_row_count_limit(),
            parquet::file::properties::DEFAULT_DATA_PAGE_ROW_COUNT_LIMIT
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

    /// The Bloom distinct-value hint scales with rows and is bounded by the
    /// row-group row maximum.
    #[test]
    fn writer_recipe_bloom_ndv_is_bounded() {
        assert_eq!(bloom_filter_ndv(1_000), 1_000);
        assert_eq!(bloom_filter_ndv(50_000), 1_000);
        assert_eq!(bloom_filter_ndv(200_000), 2_000);
        assert_eq!(bloom_filter_ndv(usize::MAX), 10_485);
    }

    /// The row-group target is soft: a row larger than the target is written
    /// into a group of its own rather than refused, and the group's encoded
    /// size overshoots the target.
    ///
    /// # Panics
    /// Panics when the oversized row is refused or no group overshoots.
    #[test]
    fn row_group_target_is_soft_for_an_oversized_row() {
        use std::sync::Arc;

        use arrow::array::{RecordBatch, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use parquet::file::reader::{FileReader, SerializedFileReader};

        let target = 1_024_u64;
        let schema = Arc::new(Schema::new(vec![Field::new(
            "payload",
            DataType::Utf8,
            false,
        )]));
        let mut state = 0x9E37_79B9_u32;
        let noisy = (0..256 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                char::from(b'a' + u8::try_from(state % 26).expect("letter"))
            })
            .collect::<String>();
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(StringArray::from(vec![noisy.as_str(), "small"]))],
        )
        .expect("batch");
        let mut bytes = Vec::new();
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            &mut bytes,
            schema,
            Some(bifrost_rewrite_writer_properties(target, &[])),
        )
        .expect("writer");
        writer.write(&batch).expect("an oversized row is written");
        writer.close().expect("footer");
        let reader = SerializedFileReader::new(bytes::Bytes::from(bytes)).expect("footer parses");
        let groups = reader.metadata().row_groups();
        assert_eq!(
            groups
                .iter()
                .map(parquet::file::metadata::RowGroupMetaData::num_rows)
                .sum::<i64>(),
            2
        );
        assert!(
            groups
                .iter()
                .any(|group| group.compressed_size().unsigned_abs() > target),
            "a group overshoots the soft target"
        );
    }
}

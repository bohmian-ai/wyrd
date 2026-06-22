use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;

pub fn bifrost_writer_properties() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(
            ZstdLevel::try_new(3).expect("zstd level 3 is valid"),
        ))
        .set_max_row_group_row_count(Some(1_000_000))
        .build()
}

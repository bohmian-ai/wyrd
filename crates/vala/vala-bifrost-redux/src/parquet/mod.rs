//! Shared bounded Parquet producer and consumer contract.

pub(crate) mod footer_preflight;
pub mod memory;
pub mod object_uploader;
pub mod writer_properties;

pub use memory::{
    BifrostArrowLogicalSizer, BifrostLeafSelection, BifrostLeafWidthProfile,
    BifrostParquetMemoryEnvelope, BoundedRowSlice, MAX_LOGICAL_ROW_GROUP_BYTES, MAX_ROW_GROUP_ROWS,
};
pub use writer_properties::{bifrost_writer_properties, bifrost_writer_properties_with_metadata};

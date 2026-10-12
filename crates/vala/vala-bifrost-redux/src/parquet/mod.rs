//! Shared Parquet producer and consumer contract.

pub mod footer;
pub mod object_uploader;
pub mod promoted_object;
pub mod variant_residual;
pub mod writer_properties;

pub use promoted_object::PromotedObjectFooter;
pub use writer_properties::{
    BIFROST_VARIANT_SHREDDING, bifrost_rewrite_writer_properties, bifrost_writer_properties,
    bifrost_writer_properties_with_metadata,
};

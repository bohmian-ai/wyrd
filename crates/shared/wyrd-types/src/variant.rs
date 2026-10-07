//! The Variant column type: its canonical storage and its extension marker.
//!
//! A Variant column is the unshredded `arrow.parquet.variant` storage struct
//! under the extension marker. Value encoding lives with the writers that
//! build Variant cells; this module owns only the type.

use arrow_schema::extension::ExtensionType;
use arrow_schema::{DataType, Field, Fields};
use parquet_variant_compute::VariantType;

/// Return the canonical unshredded Variant storage type.
///
/// Two non-null `Binary` children, `metadata` then `value`: the exact shape
/// the Iceberg schema converter produces for an Iceberg `variant`, so writers
/// and a catalog round trip agree.
#[must_use]
pub fn variant_storage_type() -> DataType {
    DataType::Struct(variant_storage_fields())
}

/// Return the two children of the canonical Variant storage struct.
///
/// Writers assemble a Variant column from these children directly.
#[must_use]
pub fn variant_storage_fields() -> Fields {
    Fields::from(vec![
        Field::new("metadata", DataType::Binary, false),
        Field::new("value", DataType::Binary, false),
    ])
}

/// Declare one Variant field: canonical storage under the
/// `arrow.parquet.variant` extension.
#[must_use]
pub fn variant_field(name: &str, nullable: bool) -> Field {
    Field::new(name, variant_storage_type(), nullable).with_extension_type(VariantType)
}

/// Report whether a field carries the `arrow.parquet.variant` extension.
///
/// The extension name, not the storage struct, is what makes a column a
/// Variant: a user Struct with `metadata`/`value` children stays a Struct.
#[must_use]
pub fn is_variant(field: &Field) -> bool {
    field.extension_type_name() == Some(VariantType::NAME)
}

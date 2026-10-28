//! Schema utilities — fingerprinting and logical Variant reconstruction.

pub mod fingerprint;

use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch};
use arrow::datatypes::{FieldRef, Schema};

pub use fingerprint::SchemaFingerprint;

/// Restores every shredded Variant column of `batch` to the storage `logical` declares.
///
/// A Bifrost table has one logical schema, but its rows may be held in a
/// physical layout that shreds Variant columns into `typed_value` leaves. Any
/// step that combines rows from different layouts — a claim merge, a union of
/// memory and staged rows, an authenticated remote response — first returns
/// each batch to the logical schema so values, not layouts, are combined.
///
/// `batch` may be a projection: each column is matched to the `logical` field
/// of the same name. A column whose type already equals its logical field is
/// kept as is; in any other column only Variant storage is rewritten — each
/// shredded Variant, at any Struct or List depth, is reconstructed by the
/// recursive `unshred_variants` and stored as the canonical Wyrd Variant
/// storage — and the result must be exactly the logical type. The returned
/// batch carries the logical fields (and their metadata) in `batch`'s column
/// order, so an unshredded input returns unchanged.
///
/// # Errors
///
/// Returns a refusal naming the column when `batch` holds a column `logical`
/// lacks, a column cannot be unshredded, or its reconstruction is not the
/// logical type — a different logical schema, never a different layout.
pub fn logical_batch(batch: &RecordBatch, logical: &Schema) -> Result<RecordBatch, String> {
    let schema = batch.schema_ref();
    let mut fields: Vec<FieldRef> = Vec::with_capacity(schema.fields().len());
    let mut columns: Vec<ArrayRef> = Vec::with_capacity(fields.capacity());
    let mut changed = false;
    let canonical = wyrd_types::variant::variant_storage_type();
    for (field, column) in schema.fields().iter().zip(batch.columns()) {
        let target = logical
            .field_with_name(field.name())
            .map_err(|_| format!("column `{}` is not in the logical schema", field.name()))?;
        if field.data_type() == target.data_type() {
            fields.push(Arc::clone(field));
            columns.push(Arc::clone(column));
            continue;
        }
        let (_, restored) = iceberg::arrow::unshred_variants(field, column)
            .and_then(|(field, column)| {
                iceberg::arrow::map_variant_fields(&field, &column, &mut |variant| {
                    if variant.data_type() == &canonical {
                        return Ok(Arc::clone(variant));
                    }
                    Ok(arrow::compute::cast(variant, &canonical)?)
                })
            })
            .map_err(|error| format!("unshred column `{}`: {error}", field.name()))?;
        if restored.data_type() != target.data_type() {
            return Err(format!(
                "column `{}` is {} but the logical schema declares {}",
                field.name(),
                restored.data_type(),
                target.data_type()
            ));
        }
        fields.push(Arc::new(target.clone()));
        columns.push(restored);
        changed = true;
    }
    if !changed {
        return Ok(batch.clone());
    }
    RecordBatch::try_new(
        Arc::new(Schema::new_with_metadata(fields, schema.metadata().clone())),
        columns,
    )
    .map_err(|error| format!("assemble the logical batch: {error}"))
}

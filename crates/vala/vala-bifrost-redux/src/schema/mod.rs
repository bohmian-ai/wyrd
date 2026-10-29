//! Schema utilities — fingerprinting and logical Variant reconstruction.

pub mod fingerprint;

use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch};
use arrow::datatypes::{Field, FieldRef, Schema};

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
    for (field, column) in schema.fields().iter().zip(batch.columns()) {
        let target = logical
            .field_with_name(field.name())
            .map_err(|_| format!("column `{}` is not in the logical schema", field.name()))?;
        if field.data_type() == target.data_type() {
            fields.push(Arc::clone(field));
            columns.push(Arc::clone(column));
            continue;
        }
        columns.push(logical_column(field, column, target)?);
        fields.push(Arc::new(target.clone()));
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

/// Returns the logical schema of rows held in `physical`, a possibly shredded layout.
///
/// Recovery reads a staged run's schema from its file, which is the physical
/// layout its generation pinned; the claim and its identity need the logical
/// schema. Each field holding a shredded Variant, at any Struct or List depth,
/// takes the type the recursive `unshred_variants` reconstructs, stored as the
/// canonical Wyrd Variant storage; every name, nullability, and metadata entry
/// is kept, so an unshredded schema returns unchanged.
///
/// # Errors
///
/// Returns a refusal naming the column when its Variant storage cannot be
/// unshredded.
pub fn logical_schema(physical: &Schema) -> Result<Schema, String> {
    let canonical = wyrd_types::variant::variant_storage_type();
    let fields = physical
        .fields()
        .iter()
        .map(|field| {
            let empty = arrow::array::new_empty_array(field.data_type());
            let (_, restored) = iceberg::arrow::unshred_variants(field, &empty)
                .and_then(|(field, column)| {
                    iceberg::arrow::map_variant_fields(&field, &column, &mut |variant| {
                        if variant.data_type() == &canonical {
                            return Ok(Arc::clone(variant));
                        }
                        Ok(arrow::compute::cast(variant, &canonical)?)
                    })
                })
                .map_err(|error| format!("unshred column `{}`: {error}", field.name()))?;
            Ok(field
                .as_ref()
                .clone()
                .with_data_type(restored.data_type().clone()))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Schema::new_with_metadata(
        fields,
        physical.metadata().clone(),
    ))
}

/// Shredded columns [`logical_column`] reconstructed in this test process.
///
/// Lets a test prove a scalar read evaluated over physical leaves without
/// rebuilding a whole Variant.
#[cfg(test)]
pub(crate) static VARIANT_RECONSTRUCTIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Restores one physical column to the type of its `logical` field.
///
/// Only Variant storage is rewritten: each shredded Variant in `column`, at
/// any Struct or List depth, is reconstructed from its typed leaves and
/// residual by the recursive `unshred_variants` and stored as the canonical
/// Wyrd Variant storage. A column already of the logical type is returned as
/// is. `field` is the column's physical field, which still carries every
/// Variant marker.
///
/// # Errors
///
/// Returns a refusal naming the column when it cannot be unshredded or its
/// reconstruction is not exactly the logical type.
pub fn logical_column(
    field: &FieldRef,
    column: &ArrayRef,
    logical: &Field,
) -> Result<ArrayRef, String> {
    if column.data_type() == logical.data_type() {
        return Ok(Arc::clone(column));
    }
    #[cfg(test)]
    VARIANT_RECONSTRUCTIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let canonical = wyrd_types::variant::variant_storage_type();
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
    if restored.data_type() != logical.data_type() {
        return Err(format!(
            "column `{}` is {} but the logical schema declares {}",
            field.name(),
            restored.data_type(),
            logical.data_type()
        ));
    }
    Ok(restored)
}

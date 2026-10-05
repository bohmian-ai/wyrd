//! Bounded footer proof for an immutable object Forge is asked to promote.
//!
//! Promotion adds a `DataFile` to a table without writing a byte, so the value
//! that lands in the manifest is evidence Scribe recorded, not evidence the
//! promoter computed. Size and checksum prove only that the object is the one
//! the row names — they say nothing about whether the statistics inside that
//! evidence describe the object's actual contents. This module re-derives the
//! `DataFile` projection from the object's own footer under the same bounds the
//! writer honored, so the promoter can compare the two and refuse a
//! disagreement before any catalog work begins.
//!
//! The footer is parsed by the standard Parquet reader; structural validity is
//! its job, not a Wyrd size policy.

use std::sync::Arc;

use bytes::Bytes;
use iceberg::spec::{DataFile, NestedField, Schema, Type};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::schema::types;

use crate::parquet::footer::footer_schema_fingerprint;
use crate::scribe::promotion::ScribeDataFileV1;

/// What re-reading one promoted object's footer proved about the object.
///
/// The projection is deliberately taken from the *table's* current schema
/// rather than from a schema carried alongside the evidence: an object whose
/// columns no longer project onto the table cannot be promoted into it, and
/// discovering that here keeps it out of the manifest.
pub struct PromotedObjectFooter {
    /// Iceberg projection of the object's own footer.
    data_file: DataFile,
    /// Lowercase hex Wyrd schema fingerprint the footer carries.
    schema_fingerprint: String,
}

impl PromotedObjectFooter {
    /// Decodes one promoted object's footer and projects it onto the table.
    ///
    /// The trailer must be present and well formed and the footer must parse;
    /// the object must carry the Bifrost schema fingerprint, and every Parquet
    /// field must carry the id the table assigns the field at the same path.
    /// Only then is the footer projected onto the table schema. The id check
    /// is explicit because the projection itself resolves columns by name and
    /// would accept an object numbered differently from its table. The object body is never
    /// decoded, so the cost is one footer regardless of object size.
    ///
    /// # Errors
    ///
    /// Returns a description of the refusal when the trailer magic is absent,
    /// the Parquet metadata cannot be parsed, the Bifrost schema fingerprint is
    /// missing or malformed, a field's id is absent or differs from the
    /// table's, or the footer has no Iceberg projection onto the table schema.
    pub fn decode(
        bytes: &Bytes,
        object_key: &str,
        table_schema: Arc<Schema>,
        file_size: u64,
    ) -> Result<Self, String> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| "promoted object size exceeds u64".to_owned())?;
        if length != file_size {
            return Err("promoted object read a different length than it reports".to_owned());
        }
        if bytes.len() < 8 {
            return Err("promoted object is shorter than its Parquet trailer".to_owned());
        }
        let trailer = &bytes[bytes.len() - 8..];
        if &trailer[4..] != b"PAR1" {
            return Err("promoted object trailer magic is invalid".to_owned());
        }
        let reader = SerializedFileReader::new(bytes.clone())
            .map_err(|error| format!("promoted object footer does not parse: {error}"))?;
        let metadata = reader.metadata();
        let schema_fingerprint = footer_schema_fingerprint(metadata.file_metadata())
            .map_err(|refusal| format!("promoted object was not sealed by Bifrost: {refusal}"))?
            .to_owned();
        verify_registered_field_ids(metadata.file_metadata().schema(), &table_schema)?;
        let written = usize::try_from(file_size)
            .map_err(|_| "promoted object size exceeds address space".to_owned())?;
        let data_file = iceberg::writer::file_writer::ParquetWriter::parquet_to_data_file_builder(
            table_schema,
            Arc::new(metadata.clone()),
            written,
            object_key.to_owned(),
            std::collections::HashMap::new(),
        )
        .map_err(|error| format!("promoted object footer has no Iceberg projection: {error}"))?
        .build()
        .map_err(|error| {
            format!("promoted object footer does not assemble a data file: {error}")
        })?;
        Ok(Self {
            data_file,
            schema_fingerprint,
        })
    }

    /// Projects the decoded footer into the persisted evidence shape.
    ///
    /// # Errors
    ///
    /// Returns a description of the refusal when a bound the footer produced has
    /// no binary single-value serialization, which would make the physical file
    /// and the persisted evidence incomparable rather than merely unequal.
    pub fn metrics(&self) -> Result<ScribeDataFileV1, String> {
        ScribeDataFileV1::from_data_file(&self.data_file)
            .map_err(|error| format!("promoted object footer has no persisted projection: {error}"))
    }

    /// Borrows the lowercase hex schema fingerprint the footer carries.
    #[must_use]
    pub fn schema_fingerprint(&self) -> &str {
        &self.schema_fingerprint
    }

    /// Borrows the Iceberg projection of the object's own footer.
    #[must_use]
    pub fn data_file(&self) -> &DataFile {
        &self.data_file
    }
}

/// Refuses an object whose Parquet field ids differ from its table's.
///
/// Every top-level column is matched to the table field of the same name and
/// checked by [`verify_field_id`], so a column the table lacks, a missing id,
/// or a different id anywhere in the tree refuses the object.
///
/// # Errors
///
/// Returns a description naming the first column whose id disagrees.
fn verify_registered_field_ids(root: &types::Type, table_schema: &Schema) -> Result<(), String> {
    for column in root.get_fields() {
        let registered = table_schema.field_by_name(column.name()).ok_or_else(|| {
            format!(
                "promoted object column {} is not in its table",
                column.name()
            )
        })?;
        verify_field_id(column, registered)?;
    }
    Ok(())
}

/// Checks one Parquet node and its descendants against one table field.
///
/// The node must carry the table field's id. A struct's children are matched
/// by name; a list's element and a map's key and value sit under the
/// unnumbered repeated group the Parquet list and map encodings insert, and
/// are matched by position there.
///
/// # Errors
///
/// Returns a description naming the node whose id is missing or differs, or
/// whose nesting does not match the table field's type.
fn verify_field_id(node: &types::Type, registered: &NestedField) -> Result<(), String> {
    let info = node.get_basic_info();
    if !info.has_id() || info.id() != registered.id {
        return Err(format!(
            "promoted object column {} carries field id {} but its table assigns {}",
            node.name(),
            if info.has_id() {
                info.id().to_string()
            } else {
                "none".to_owned()
            },
            registered.id
        ));
    }
    let nesting = || {
        format!(
            "promoted object column {} does not match its table's nesting",
            node.name()
        )
    };
    let repeated = || match node.get_fields() {
        [repeated] if repeated.is_group() => Ok(repeated.get_fields()),
        _ => Err(nesting()),
    };
    match registered.field_type.as_ref() {
        Type::Primitive(_) if node.is_primitive() => Ok(()),
        Type::Struct(children) if node.is_group() => {
            node.get_fields().iter().try_for_each(|child| {
                let field = children.field_by_name(child.name()).ok_or_else(nesting)?;
                verify_field_id(child, field)
            })
        }
        Type::List(list) if node.is_group() => match repeated()? {
            [element] => verify_field_id(element, &list.element_field),
            _ => Err(nesting()),
        },
        Type::Map(map) if node.is_group() => match repeated()? {
            [key, value] => {
                verify_field_id(key, &map.key_field)?;
                verify_field_id(value, &map.value_field)
            }
            _ => Err(nesting()),
        },
        _ => Err(nesting()),
    }
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::Schema;
    use parquet::schema::types::TypePtr;

    use super::*;

    /// Convert one Arrow schema to the Parquet schema the Arrow writer emits.
    ///
    /// # Panics
    ///
    /// Panics when the Arrow schema has no Parquet projection.
    fn parquet_root(schema: &Schema) -> TypePtr {
        parquet::arrow::ArrowSchemaConverter::new()
            .convert(schema)
            .expect("the schema has a Parquet projection")
            .root_schema_ptr()
    }

    /// Forge accepts the table's own ids and refuses any other numbering.
    ///
    /// # Panics
    ///
    /// Panics when a file stamped with the registered ids is refused, or when
    /// a file whose ids are absent or shifted, at the top level or inside a
    /// nested list element, is accepted.
    #[test]
    fn field_ids_must_match_the_registered_table() {
        let definition = crate::tables::builtin_table("traces", "spans").expect("spans definition");
        let physical = (definition.schema)();
        let registered = iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&physical)
            .expect("the physical schema has an Iceberg projection");
        let stamped = crate::tables::stamp_registered_field_ids(
            &arrow::record_batch::RecordBatch::new_empty(Arc::clone(&physical)),
            &registered,
        )
        .expect("every physical field is registered")
        .schema();
        verify_registered_field_ids(&parquet_root(&stamped), &registered)
            .expect("the registered ids are accepted");

        assert!(
            verify_registered_field_ids(&parquet_root(&physical), &registered).is_err(),
            "a file without ids is refused"
        );

        let shifted = iceberg::spec::Schema::builder()
            .with_fields(registered.as_struct().fields().iter().map(|field| {
                let mut field = field.as_ref().clone();
                if field.name == "events"
                    && let iceberg::spec::Type::List(list) = field.field_type.as_mut()
                {
                    let mut element = list.element_field.as_ref().clone();
                    element.id += 10_000;
                    list.element_field = Arc::new(element);
                }
                Arc::new(field)
            }))
            .build()
            .expect("the shifted schema builds");
        assert!(
            verify_registered_field_ids(&parquet_root(&stamped), &shifted).is_err(),
            "a nested element numbered differently from its table is refused"
        );
    }
}

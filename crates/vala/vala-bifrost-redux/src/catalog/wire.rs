//! Redux catalog row and stored-schema projection onto public wire types.

use arrow::datatypes::{Field, Schema};
use vala_sql::row_types::olap_catalog::BifrostTableRow;
use wyrd_spec::vala::api::{BifrostTableEntry, FieldSpec, PhysicalLayoutWire, TableStatus};
use wyrd_spec::vala::{RUN_ID, SUBJECT_CARD_UID};

use wyrd_types::field_to_spec;

use crate::catalog::BifrostCatalogError;
use crate::tables::managed_columns::{is_managed_column, is_reserved_column};

/// Reject user fields that collide with server-owned physical columns.
///
/// Registration calls this before any schema is resolved, so a caller can
/// never declare a column the server stamps or derives itself, nor any
/// `wyrd_*` name.
///
/// # Errors
/// Returns [`BifrostCatalogError::ReservedColumn`] naming the first user field
/// whose name is reserved.
pub fn reject_reserved_field_names(user_fields: &[Field]) -> Result<(), BifrostCatalogError> {
    for field in user_fields {
        let name = field.name();
        if is_reserved_column(name) {
            return Err(BifrostCatalogError::ReservedColumn(name.clone()));
        }
    }
    Ok(())
}

/// Map one tenant-scoped catalog row to its public list entry.
pub fn entry_from_row(row: &BifrostTableRow) -> Result<BifrostTableEntry, BifrostCatalogError> {
    let (namespace, name) = row.fqn.rsplit_once('.').ok_or_else(|| {
        BifrostCatalogError::MetadataMismatch(format!(
            "fqn missing namespace separator: {}",
            row.fqn
        ))
    })?;
    Ok(BifrostTableEntry {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        table_uid: to_hex(&row.table_uid),
        status: status_from_db(&row.status)?,
        fingerprint: to_hex(&row.fingerprint),
        registered_at: row.registered_at,
        updated_at: row.updated_at,
    })
}

/// Decode the persisted canonical physical layout from one catalog row.
///
/// The `physical_layout` column is written only by the catalog from
/// [`crate::catalog::PhysicalLayout::to_wire`], so anything that fails to
/// deserialize is control-plane corruption, not caller input.
///
/// # Errors
/// Returns [`BifrostCatalogError::MetadataMismatch`] when the stored JSON is
/// not a physical-layout declaration.
pub fn layout_wire_from_row(
    row: &BifrostTableRow,
) -> Result<PhysicalLayoutWire, BifrostCatalogError> {
    serde_json::from_value(row.physical_layout.clone()).map_err(|error| {
        BifrostCatalogError::MetadataMismatch(format!(
            "stored physical_layout for {} is not decodable: {error}",
            row.fqn
        ))
    })
}

/// The two describe field classes projected from one stored physical schema.
///
/// Splitting them is what lets a writer tell what it declares (`user_fields`)
/// from the correlation inputs it may attach (`correlation_fields`). No other
/// managed column is writer-supplied, so none is described.
#[derive(Debug, Clone, PartialEq)]
pub struct DescribedFields {
    /// The table's own declared columns, in stored order.
    pub user_fields: Vec<FieldSpec>,
    /// `subject_card_uid` then `run_id`, the correlation inputs a writer
    /// supplies.
    pub correlation_fields: Vec<FieldSpec>,
}

/// Project a stored physical schema onto the two describe field classes.
///
/// Every declaration is taken from the stored schema, so its type,
/// nullability, and registered field id are the table's actual ones rather
/// than a restatement. Every table stores the managed envelope, so the
/// correlation inputs are its `subject_card_uid` and `run_id` columns; the
/// rest of the envelope is server-stamped and described in neither class.
/// Card correlation is optional: a row without one is stored with its
/// authenticated `sub_principal_id`/`act_principal_id` pair and a null
/// `subject_card_uid`.
///
/// # Errors
///
/// Returns [`BifrostCatalogError::MetadataMismatch`] when a stored column type
/// is not representable on the wire.
pub fn described_fields_from_stored_schema(
    schema: &Schema,
) -> Result<DescribedFields, BifrostCatalogError> {
    let mut described = DescribedFields {
        user_fields: Vec::new(),
        correlation_fields: Vec::new(),
    };
    for field in schema.fields() {
        let name = field.name().as_str();
        let correlation = name == SUBJECT_CARD_UID || name == RUN_ID;
        if !correlation && is_managed_column(name) {
            continue;
        }
        let spec = field_to_spec(field)
            .map_err(|error| BifrostCatalogError::MetadataMismatch(error.to_string()))?;
        if correlation {
            described.correlation_fields.push(spec);
        } else {
            described.user_fields.push(spec);
        }
    }
    Ok(described)
}

fn status_from_db(status: &str) -> Result<TableStatus, BifrostCatalogError> {
    match status {
        "active" => Ok(TableStatus::Active),
        "deprecated" => Ok(TableStatus::Deprecated),
        "quarantined" => Ok(TableStatus::Quarantined),
        other => Err(BifrostCatalogError::MetadataMismatch(format!(
            "unknown table status: {other}"
        ))),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::DataType;

    use crate::tables::managed_columns::MANAGED_COLUMNS;

    use super::*;

    /// Every managed column name, and any other `wyrd_*` name such as the
    /// unmanaged `wyrd_custom`, is refused as a user field.
    ///
    /// The managed names are iterated from the managed-column declaration
    /// rather than spelled out here. A hand-written list silently stops
    /// covering a name the moment one is added to the declaration.
    ///
    /// # Panics
    ///
    /// Panics when a managed name is accepted or refused with any error other
    /// than [`BifrostCatalogError::ReservedColumn`] for that name.
    #[test]
    fn every_managed_field_name_is_rejected() {
        let managed = MANAGED_COLUMNS.iter().map(|column| column.field.name);
        for name in managed.chain(["wyrd_custom"]) {
            let error = reject_reserved_field_names(&[Field::new(name, DataType::Int64, true)])
                .expect_err("every reserved field must fail");
            assert!(
                matches!(error, BifrostCatalogError::ReservedColumn(ref column) if column == name),
                "reserved column {name} was refused as {error:?}"
            );
        }
    }

    /// Projection splits every stored column into its describe class.
    ///
    /// The split is what tells a client which columns it declares, which it
    /// supplies as correlation inputs, and which the server stamps. Leaking a
    /// server-stamped column such as `sub_principal_id` into `user_fields` would
    /// present it as an ordinary declarable column.
    ///
    /// # Panics
    ///
    /// Panics when a class gains or loses a column.
    #[test]
    fn stored_schema_projection_classifies_every_correlation_column() {
        let mut fields: Vec<Field> =
            crate::tables::managed_columns::ensure_managed_columns(Vec::new());
        fields.push(Field::new("value", DataType::Int64, true));
        let schema = Schema::new(fields);

        let described = described_fields_from_stored_schema(&schema)
            .expect("the stored schema projects onto the wire");

        assert_eq!(
            described
                .user_fields
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            vec!["value"],
            "only declarable columns are user fields: {:?}",
            described.user_fields
        );
        assert_eq!(
            described
                .correlation_fields
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            vec![SUBJECT_CARD_UID, RUN_ID],
            "correlation is exactly the card uid and the run id"
        );
    }
}

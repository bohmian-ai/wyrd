//! Planning one explicit table-definition edit.
//!
//! [`BifrostCatalog::alter_table`](crate::catalog::BifrostCatalog::alter_table)
//! plans an edit here from the table's current definition, resolves the
//! edited layout through the one layout validator, and commits the Iceberg
//! updates [`definition_updates`] derives. Nothing here performs IO, so every
//! refusal is decided before the catalog commits anything.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::datatypes::Field;
use iceberg::TableUpdate;
use iceberg::spec::{NestedField, NestedFieldRef, Schema as IcebergSchema, TableMetadata, Type};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::TableEditViolation;
use wyrd_spec::vala::api::{ColumnRename, PhysicalLayoutEdit, PhysicalLayoutWire};

use crate::catalog::TableDefinition;
use crate::catalog::layout::{BLOOM_COLUMNS_PROPERTY, PhysicalLayout};
use crate::tables::managed_columns::{is_managed_column, is_reserved_column};

/// One explicit edit of a table's definition, as the alteration route
/// received it.
pub struct TableAlteration {
    /// Version the caller built the edit from; any other current version
    /// refuses the edit.
    pub expected_table_version: wyrd_spec::vala::ids::TableVersion,
    /// Layout lists to replace; an omitted list keeps the current one.
    pub physical_layout: PhysicalLayoutEdit,
    /// Nullable top-level custom columns to append, already converted from
    /// their wire declarations.
    pub add_columns: Vec<Field>,
    /// Top-level custom columns to rename under their field ids.
    pub rename_columns: Vec<ColumnRename>,
}

/// The edited schema and layout declaration one alteration produces, before
/// the layout validator resolves it.
pub(crate) struct EditedDefinition {
    /// The edited Iceberg schema, or `None` when no column is added or
    /// renamed.
    pub(crate) schema: Option<IcebergSchema>,
    /// The complete edited layout declaration, every reference renamed.
    pub(crate) layout: PhysicalLayoutWire,
}

/// Plans the schema and layout declaration `alteration` leaves `current` with.
///
/// Renames keep each column's field id, type, nullability, and values, and
/// every layout reference to a renamed column follows it. Added columns get
/// fresh ids above `metadata`'s last column id and sit after the existing
/// custom columns, where a registration declaring them would put them. A
/// supplied layout list replaces the current list; an omitted one is kept.
/// The time column is never replaced, only renamed with its column.
///
/// `builtin` refuses every schema edit: a built-in's schema is the canonical
/// one, though its layout may still change.
///
/// # Errors
///
/// Returns [`BifrostError::InvalidTableEdit`] for a protected, unknown,
/// colliding, ambiguous, no-op, or required column edit or an empty partition
/// list, and [`BifrostError::ReservedColumn`] for a new name a writer may not
/// declare.
pub(crate) fn edited_definition(
    fqn: &str,
    current: &TableDefinition,
    metadata: &TableMetadata,
    builtin: bool,
    alteration: &TableAlteration,
) -> Result<EditedDefinition, BifrostError> {
    let refuse = |violation, column: &str| refusal(fqn, violation, column);
    let schema = &current.schema;
    let renames = renames(fqn, schema, builtin, &alteration.rename_columns)?;
    let mut names = schema
        .as_struct()
        .fields()
        .iter()
        .map(|field| renames.get(&field.name).unwrap_or(&field.name).clone())
        .collect::<BTreeSet<_>>();
    let partition_names = metadata
        .partition_specs_iter()
        .flat_map(|spec| spec.fields().iter().map(|field| field.name.clone()))
        .collect::<BTreeSet<_>>();
    for field in &alteration.add_columns {
        let name = field.name();
        if builtin {
            return Err(refuse(TableEditViolation::ProtectedColumn, name));
        }
        if is_reserved_column(name) {
            return Err(BifrostError::ReservedColumn {
                column: name.clone(),
            });
        }
        if !field.is_nullable() {
            return Err(refuse(TableEditViolation::RequiredColumn, name));
        }
        if partition_names.contains(name) || !names.insert(name.clone()) {
            return Err(refuse(TableEditViolation::NameCollision, name));
        }
    }

    let edited_schema = if renames.is_empty() && alteration.add_columns.is_empty() {
        None
    } else {
        Some(edited_schema(fqn, schema, metadata, &renames, &alteration.add_columns)?)
    };
    let renamed = |column: &str| renames.get(column).map_or(column, String::as_str).to_owned();
    let mut layout = current.layout.to_wire();
    layout.time_column = layout.time_column.as_deref().map(renamed);
    for field in &mut layout.partition_by {
        field.column = renamed(&field.column);
    }
    for key in &mut layout.sort_keys {
        key.column = renamed(&key.column);
    }
    for column in &mut layout.bloom_columns {
        *column = renamed(column);
    }
    let edit = &alteration.physical_layout;
    if let Some(partition_by) = &edit.partition_by {
        if partition_by.is_empty() {
            return Err(BifrostError::InvalidPhysicalLayout {
                table: fqn.to_owned(),
                field: wyrd_spec::vala::PhysicalLayoutField::Partition,
                violation: wyrd_spec::vala::PhysicalLayoutViolation::Missing,
                column: layout.time_column.clone(),
            });
        }
        layout.partition_by.clone_from(partition_by);
    }
    if let Some(sort_keys) = &edit.sort_keys {
        layout.sort_keys.clone_from(sort_keys);
    }
    if let Some(bloom_columns) = &edit.bloom_columns {
        layout.bloom_columns.clone_from(bloom_columns);
    }
    Ok(EditedDefinition {
        schema: edited_schema,
        layout,
    })
}

/// Builds the refusal of one edit naming `column`.
fn refusal(fqn: &str, violation: TableEditViolation, column: &str) -> BifrostError {
    BifrostError::InvalidTableEdit {
        table: fqn.to_owned(),
        violation,
        column: Some(column.to_owned()),
    }
}

/// Validates `renames` against the current top-level columns and returns them
/// as an old-to-new name map.
///
/// # Errors
///
/// Returns [`BifrostError::InvalidTableEdit`] for an unknown, protected,
/// repeated, chained, colliding, or unchanged rename, and
/// [`BifrostError::ReservedColumn`] for a reserved new name.
fn renames(
    fqn: &str,
    schema: &IcebergSchema,
    builtin: bool,
    renames: &[ColumnRename],
) -> Result<BTreeMap<String, String>, BifrostError> {
    let refuse = |violation, column: &str| refusal(fqn, violation, column);
    let top_level = |name: &str| {
        schema
            .as_struct()
            .fields()
            .iter()
            .any(|field| field.name == name)
    };
    let mut map = BTreeMap::new();
    let mut targets = BTreeSet::new();
    for rename in renames {
        let (from, to) = (rename.from.as_str(), rename.to.as_str());
        if !top_level(from) {
            return Err(refuse(TableEditViolation::UnknownColumn, from));
        }
        if builtin || is_managed_column(from) || is_reserved_column(from) {
            return Err(refuse(TableEditViolation::ProtectedColumn, from));
        }
        if is_reserved_column(to) {
            return Err(BifrostError::ReservedColumn {
                column: to.to_owned(),
            });
        }
        if from == to {
            return Err(refuse(TableEditViolation::NoEffectiveChange, from));
        }
        if map.contains_key(from) || !targets.insert(to) {
            return Err(refuse(TableEditViolation::AmbiguousRename, from));
        }
        map.insert(from.to_owned(), to.to_owned());
    }
    for (from, to) in &map {
        if map.contains_key(to.as_str()) {
            return Err(refuse(TableEditViolation::AmbiguousRename, from));
        }
        if top_level(to) {
            return Err(refuse(TableEditViolation::NameCollision, to));
        }
    }
    Ok(map)
}

/// Builds the edited Iceberg schema: current columns under their ids with
/// `renames` applied, and `added` inserted before the first managed column
/// under fresh ids.
///
/// # Errors
///
/// Returns [`BifrostError::UnsupportedType`] when an added column has no
/// Iceberg type, and [`BifrostError::SchemaParse`] when the edited schema
/// does not build.
fn edited_schema(
    fqn: &str,
    schema: &IcebergSchema,
    metadata: &TableMetadata,
    renames: &BTreeMap<String, String>,
    added: &[Field],
) -> Result<IcebergSchema, BifrostError> {
    let unsupported = |field: &Field| BifrostError::UnsupportedType {
        field: field.name().clone(),
        data_type: field.data_type().to_string(),
    };
    let mut next_id = metadata.last_column_id();
    let mut new_fields = Vec::with_capacity(added.len());
    for field in added {
        let stored = crate::tables::iceberg_form(field).map_err(|_| unsupported(field))?;
        let converted = iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(
            &arrow::datatypes::Schema::new(vec![stored]),
        )
        .map_err(|_| unsupported(field))?;
        let column = converted
            .as_struct()
            .fields()
            .first()
            .cloned()
            .ok_or_else(|| unsupported(field))?;
        new_fields.push(Arc::new(reassign_ids(&column, &mut next_id)));
    }
    let mut fields = Vec::with_capacity(schema.as_struct().fields().len() + new_fields.len());
    let mut pending = Some(new_fields);
    for field in schema.as_struct().fields() {
        if is_managed_column(&field.name) {
            fields.extend(pending.take().into_iter().flatten());
        }
        fields.push(renames.get(&field.name).map_or_else(
            || Arc::clone(field),
            |to| {
                let mut renamed = field.as_ref().clone();
                renamed.name.clone_from(to);
                Arc::new(renamed)
            },
        ));
    }
    fields.extend(pending.into_iter().flatten());
    IcebergSchema::builder()
        .with_schema_id(schema.schema_id() + 1)
        .with_identifier_field_ids(schema.identifier_field_ids())
        .with_fields(fields)
        .build()
        .map_err(|error| BifrostError::SchemaParse {
            detail: format!("{fqn} edited schema does not build: {error}"),
        })
}

/// Returns `field` with its id and every nested id replaced by the ids after
/// `next_id`, advancing `next_id` past them.
fn reassign_ids(field: &NestedField, next_id: &mut i32) -> NestedField {
    *next_id += 1;
    let mut field = field.clone();
    field.id = *next_id;
    field.field_type = Box::new(reassign_type_ids(&field.field_type, next_id));
    field
}

/// Returns `field_type` with every nested field id reassigned after `next_id`.
fn reassign_type_ids(field_type: &Type, next_id: &mut i32) -> Type {
    let child = |field: &NestedFieldRef, next_id: &mut i32| -> NestedFieldRef {
        Arc::new(reassign_ids(field, next_id))
    };
    match field_type {
        Type::Primitive(_) | Type::Variant(_) => field_type.clone(),
        Type::Struct(fields) => Type::Struct(iceberg::spec::StructType::new(
            fields
                .fields()
                .iter()
                .map(|field| child(field, next_id))
                .collect(),
        )),
        Type::List(list) => Type::List(iceberg::spec::ListType::new(child(
            &list.element_field,
            next_id,
        ))),
        Type::Map(map) => {
            let key = child(&map.key_field, next_id);
            Type::Map(iceberg::spec::MapType::new(
                key,
                child(&map.value_field, next_id),
            ))
        }
    }
}

/// Derives the Iceberg updates that move `metadata`'s current definition to
/// `schema` and `layout`, empty when they declare nothing new.
///
/// Partition spec and sort order compare by source field id and transform,
/// so a rename alone adds neither. The table-version property is not among
/// them: the caller sets it once it knows the edit is effective.
///
/// # Errors
///
/// Returns [`BifrostError::PhysicalDrift`] when the layout cannot be bound to
/// the edited schema.
pub(crate) fn definition_updates(
    fqn: &str,
    metadata: &TableMetadata,
    current: &TableDefinition,
    schema: Option<IcebergSchema>,
    layout: &PhysicalLayout,
) -> Result<Vec<TableUpdate>, BifrostError> {
    let drift = |detail: String| BifrostError::PhysicalDrift {
        detail: format!("{fqn}: {detail}"),
    };
    let bound = schema.as_ref().unwrap_or(&current.schema);
    let mut updates = Vec::new();
    let spec = layout.iceberg_partition_spec(bound).map_err(drift)?;
    let spec_changed = {
        let current_fields = metadata.default_partition_spec().fields();
        current_fields.len() != spec.fields().len()
            || current_fields
                .iter()
                .zip(spec.fields())
                .any(|(current, next)| {
                    current.source_id != next.source_id || current.transform != next.transform
                })
    };
    let mut order = layout.iceberg_sort_order(bound).map_err(drift)?;
    let order_changed = {
        let current_fields = &metadata.default_sort_order().fields;
        current_fields.len() != order.fields.len()
            || current_fields.iter().zip(&order.fields).any(|(current, next)| {
                current.source_id != next.source_id
                    || current.direction != next.direction
                    || current.null_order != next.null_order
            })
    };
    if let Some(schema) = schema {
        updates.push(TableUpdate::AddSchema { schema });
        updates.push(TableUpdate::SetCurrentSchema { schema_id: -1 });
    }
    if spec_changed {
        updates.push(TableUpdate::AddSpec { spec });
        updates.push(TableUpdate::SetDefaultSpec { spec_id: -1 });
    }
    if order_changed {
        order.order_id = metadata
            .sort_orders_iter()
            .map(|order| order.order_id)
            .max()
            .unwrap_or_default()
            + 1;
        updates.push(TableUpdate::AddSortOrder { sort_order: order });
        updates.push(TableUpdate::SetDefaultSortOrder { sort_order_id: -1 });
    }
    if layout.bloom_columns() != current.layout.bloom_columns() {
        updates.push(TableUpdate::SetProperties {
            updates: [(
                BLOOM_COLUMNS_PROPERTY.to_owned(),
                layout.bloom_columns_property(),
            )]
            .into(),
        });
    }
    Ok(updates)
}

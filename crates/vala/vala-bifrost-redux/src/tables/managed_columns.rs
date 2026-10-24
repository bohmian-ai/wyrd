//! The one declaration of every server-managed column Bifrost appends.
//!
//! [`MANAGED_COLUMNS`] is the only place a managed column's name, physical
//! type, nullability, and physical order are declared. Every
//! physical schema — dynamic tables, pre-declared domain tables, and the
//! canonical signal tables — and every reserved-name check derives from it, so
//! adding, retyping, or retiring a managed column is one edit here.

use arrow::datatypes::{Field, TimeUnit};

use crate::tables::fields::{CanonicalField, CanonicalType, canonical_arrow_fields};
use wyrd_spec::vala::{
    ACT_PRINCIPAL_ID, RUN_ID, SUB_PRINCIPAL_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};

/// How a managed column's value relates to one accepted batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedScope {
    /// Attribution that is the same on every retry of a batch.
    Correlation,
    /// The request that carried or produced the rows.
    Request,
    /// The admission instant.
    Time,
}

/// One server-managed column: its physical declaration and its scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedColumn {
    /// Name, physical type, and nullability of the column.
    pub field: CanonicalField,
    /// How the column's value relates to one accepted batch.
    pub scope: ManagedScope,
}

impl ManagedColumn {
    /// Declare one UTF-8 identity column in `scope`.
    const fn identity(name: &'static str, nullable: bool, scope: ManagedScope) -> Self {
        Self {
            field: CanonicalField::meta(name, CanonicalType::Utf8, nullable),
            scope,
        }
    }

    /// Declare one non-null UTC microsecond time column.
    const fn time(name: &'static str) -> Self {
        Self {
            field: CanonicalField::meta(
                name,
                CanonicalType::Timestamp(
                    TimeUnit::Microsecond,
                    Some(iceberg::arrow::UTC_TIME_ZONE),
                ),
                false,
            ),
            scope: ManagedScope::Time,
        }
    }

    /// Project this column into an Arrow field without field metadata.
    ///
    /// Dynamic and pre-declared tables carry no sensitivity ledger, so their
    /// managed fields carry no metadata either.
    fn untagged_arrow(&self) -> Field {
        Field::new(
            self.field.name,
            self.field.ty.to_arrow(),
            self.field.nullable,
        )
    }
}

/// Every server-managed column, in physical order; every table appends all
/// of them after its own fields.
///
/// Correlation columns come first, then the request and ingestion time:
///
/// - `workload_card_uid`: the validated exact Service release the work ran
///   under, null without proof;
/// - `subject_card_uid`: the exact Card observed or judged;
/// - `run_id`: optional application Run correlation;
/// - `sub_principal_id`: the principal the row's work was for;
/// - `act_principal_id`: the principal that performed it;
/// - `wyrd_request_id`: the request that wrote the row, the join key to audit;
/// - `wyrd_ingested_at`: when Wyrd accepted the row, read once per batch from
///   `PostgreSQL` and never caller-supplied.
///
/// Tenant ownership is not a column: every Parquet file proves it in footer
/// metadata. Field ids are assigned by each registered Iceberg table, never
/// declared here.
pub static MANAGED_COLUMNS: &[ManagedColumn] = &[
    ManagedColumn::identity(WORKLOAD_CARD_UID, true, ManagedScope::Correlation),
    ManagedColumn::identity(SUBJECT_CARD_UID, true, ManagedScope::Correlation),
    ManagedColumn::identity(RUN_ID, true, ManagedScope::Correlation),
    ManagedColumn::identity(SUB_PRINCIPAL_ID, false, ManagedScope::Correlation),
    ManagedColumn::identity(ACT_PRINCIPAL_ID, false, ManagedScope::Correlation),
    ManagedColumn::identity(WYRD_REQUEST_ID, false, ManagedScope::Request),
    ManagedColumn::time(WYRD_INGESTED_AT),
];

/// Report whether `name` is a server-managed column.
///
/// Writers may not declare a column with a managed name; the server stamps or
/// resolves every one of them.
#[must_use]
pub fn is_managed_column(name: &str) -> bool {
    MANAGED_COLUMNS
        .iter()
        .any(|column| column.field.name == name)
}

/// The name prefix every server-owned column carries, managed or not.
pub const RESERVED_PREFIX: &str = "wyrd_";

/// Report whether a writer may not supply or declare a column named `name`.
///
/// Reserved is every managed column plus every [`RESERVED_PREFIX`] name, so
/// a retired or future server column stays refused rather than silently
/// becoming user content. Registration and native ingest both
/// refuse through this one rule.
#[must_use]
pub fn is_reserved_column(name: &str) -> bool {
    name.starts_with(RESERVED_PREFIX) || is_managed_column(name)
}

/// Report whether `name` is a managed correlation column.
///
/// Correlation columns attribute rows to a workload, a subject Card, a Run, and
/// an account pair.
/// They are identical on every retry of one batch, unlike the request and time
/// columns, which is what lets a retry be recognised as the same write.
#[must_use]
pub fn is_correlation_column(name: &str) -> bool {
    MANAGED_COLUMNS
        .iter()
        .any(|column| column.scope == ManagedScope::Correlation && column.field.name == name)
}

/// Append every managed column to a table's user fields.
///
/// User fields stay first, in their declared order, followed by the managed
/// columns in [`MANAGED_COLUMNS`] order. The appended fields carry no field
/// metadata. The user-schema fingerprint hashes the user fields alone, so
/// appending them never perturbs it.
#[must_use]
pub fn ensure_managed_columns(mut user_fields: Vec<Field>) -> Vec<Field> {
    user_fields.extend(MANAGED_COLUMNS.iter().map(ManagedColumn::untagged_arrow));
    user_fields
}

/// Build one canonical signal table's complete physical Arrow fields.
///
/// The declared signal ledger comes first in declaration order, then the
/// managed columns. Every field — ledger and managed, at every
/// nesting depth — carries its sensitivity metadata, which is what lets the
/// canonical physical fingerprint cover the whole schema.
#[must_use]
pub fn canonical_physical_fields(declared: &[CanonicalField]) -> Vec<Field> {
    let mut physical = canonical_arrow_fields(declared);
    physical.extend(MANAGED_COLUMNS.iter().map(|column| column.field.to_arrow()));
    physical
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table appends the seven-field envelope in declared order.
    ///
    /// # Panics
    ///
    /// Panics when the order changes, a retired column reappears, or an
    /// envelope field's nullability differs from the contract.
    #[test]
    fn observation_policy_appends_every_managed_column_in_order() {
        let fields = ensure_managed_columns(vec![]);
        let shape: Vec<(&str, bool)> = fields
            .iter()
            .map(|field| (field.name().as_str(), field.is_nullable()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (WORKLOAD_CARD_UID, true),
                (SUBJECT_CARD_UID, true),
                (RUN_ID, true),
                (SUB_PRINCIPAL_ID, false),
                (ACT_PRINCIPAL_ID, false),
                (WYRD_REQUEST_ID, false),
                (WYRD_INGESTED_AT, false),
            ]
        );
        assert!(
            !is_managed_column("card_uid"),
            "the retired name is ordinary"
        );
        assert!(!is_managed_column("owner_card_uid"));
    }

    /// Caller-declared fields remain ahead of every server-managed field.
    ///
    /// # Panics
    ///
    /// Panics when the caller's field is not first in the managed schema.
    #[test]
    fn user_fields_come_before_managed_columns() {
        let user = vec![Field::new("my_col", arrow::datatypes::DataType::Utf8, true)];
        let fields = ensure_managed_columns(user);
        assert_eq!(fields[0].name(), "my_col");
    }

    /// The managed-name check covers exactly the declaration, and the
    /// reserved-name check adds every `wyrd_*` name to it.
    ///
    /// # Panics
    ///
    /// Panics when a declared column is not reported as managed, when the
    /// retired batch column or a user name is, or when a `wyrd_*` name is not
    /// reported as reserved.
    #[test]
    fn is_managed_column_covers_exactly_the_declaration() {
        for column in MANAGED_COLUMNS {
            assert!(
                is_managed_column(column.field.name),
                "{}",
                column.field.name
            );
        }
        assert!(!is_managed_column("wyrd_batch_id"));
        assert!(!is_managed_column("value"));
        assert!(is_reserved_column("wyrd_batch_id"));
        assert!(is_reserved_column(SUB_PRINCIPAL_ID));
        assert!(is_reserved_column(ACT_PRINCIPAL_ID));
        assert!(!is_reserved_column("value"));
        assert!(is_correlation_column(WORKLOAD_CARD_UID));
        assert!(is_correlation_column(RUN_ID));
        assert!(is_correlation_column(SUB_PRINCIPAL_ID));
        assert!(is_correlation_column(ACT_PRINCIPAL_ID));
        assert!(!is_correlation_column(WYRD_REQUEST_ID));
        assert!(!is_correlation_column(WYRD_INGESTED_AT));
    }

    /// Canonical tables tag the managed columns with sensitivity and no id.
    ///
    /// # Panics
    ///
    /// Panics when a canonical managed field declares a field id or loses its
    /// sensitivity marker.
    #[test]
    fn canonical_physical_fields_declare_no_managed_field_ids() {
        for field in canonical_physical_fields(&[]) {
            assert_eq!(
                field
                    .metadata()
                    .get(crate::tables::fields::PARQUET_FIELD_ID),
                None,
                "{}",
                field.name()
            );
            assert_eq!(
                field
                    .metadata()
                    .get(crate::tables::fields::WYRD_SENSITIVE)
                    .map(String::as_str),
                Some("false"),
                "{}",
                field.name()
            );
        }
    }
}

//! The one declaration of every server-managed column Bifrost appends.
//!
//! [`MANAGED_COLUMNS`] is the only place a managed column's name, physical
//! type, nullability, stable id, and physical order are declared. Every
//! physical schema — dynamic tables, pre-declared domain tables, and the
//! canonical signal tables — and every reserved-name check derives from it, so
//! adding, retyping, or retiring a managed column is one edit here.

use arrow::datatypes::{Field, TimeUnit};

use crate::tables::CorrelationPolicy;
use crate::tables::fields::{CanonicalField, CanonicalType, canonical_arrow_fields};
use wyrd_spec::vala::{
    CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
};

/// Which tables a managed column is appended to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedScope {
    /// Attribution that is the same on every retry of a batch, appended only
    /// by a table's correlation policy.
    Correlation,
    /// The request that carried the rows, fresh on every attempt and appended
    /// alongside the correlation columns.
    Request,
    /// Time axis appended to every table, whatever its correlation policy.
    Time,
}

/// One server-managed column: its physical declaration and where it applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedColumn {
    /// Name, physical type, nullability, and stable id of the column.
    pub field: CanonicalField,
    /// Which tables append the column.
    pub scope: ManagedScope,
}

impl ManagedColumn {
    /// Declare one UTF-8 identity column in `scope`.
    const fn identity(id: i32, name: &'static str, nullable: bool, scope: ManagedScope) -> Self {
        Self {
            field: CanonicalField::meta(id, name, CanonicalType::Utf8, nullable),
            scope,
        }
    }

    /// Declare one non-null UTC microsecond time column every table appends.
    const fn time(id: i32, name: &'static str) -> Self {
        Self {
            field: CanonicalField::meta(
                id,
                name,
                CanonicalType::Timestamp(TimeUnit::Microsecond, Some("UTC")),
                false,
            ),
            scope: ManagedScope::Time,
        }
    }

    /// Project this column into an Arrow field without field metadata.
    ///
    /// Dynamic and pre-declared tables carry no stable ids, so their managed
    /// fields carry none either and keep automatic Iceberg id assignment.
    fn untagged_arrow(&self) -> Field {
        Field::new(
            self.field.name,
            self.field.ty.to_arrow(),
            self.field.nullable,
        )
    }
}

/// Every server-managed column, in physical order.
///
/// Correlation columns come first, then the two time columns:
///
/// - `run_id`, `card_uid`: optional correlation to a run and a Card version;
/// - `principal_id`: the authenticated writer;
/// - `wyrd_request_id`: the request that wrote the row, the join key to audit;
/// - `wyrd_event_time`: when the observed thing happened — the caller's value
///   when supplied, otherwise equal to `wyrd_ingested_at`;
/// - `wyrd_ingested_at`: when Wyrd accepted the row, read once per batch from
///   `PostgreSQL` and never caller-supplied.
///
/// Tenant ownership is not a column: every Parquet file proves it in footer
/// metadata. The stable ids sit far above any signal ledger so a canonical
/// table can add fields without colliding with them. Ids 1006, 1007, and 1008
/// are retired — the removed per-row batch id, batch-local row ordinal, and
/// tenant column — and an id is never reused for a different column.
pub static MANAGED_COLUMNS: &[ManagedColumn] = &[
    ManagedColumn::identity(1000, RUN_ID, true, ManagedScope::Correlation),
    ManagedColumn::identity(1001, CARD_UID, true, ManagedScope::Correlation),
    ManagedColumn::identity(1002, PRINCIPAL_ID, false, ManagedScope::Correlation),
    ManagedColumn::identity(1003, WYRD_REQUEST_ID, false, ManagedScope::Request),
    ManagedColumn::time(1004, WYRD_EVENT_TIME),
    ManagedColumn::time(1005, WYRD_INGESTED_AT),
];

impl CorrelationPolicy {
    /// Report whether a table under this policy appends `column`.
    ///
    /// Time columns are always appended. Correlation and request columns are
    /// appended by `Observation` tables; `CodeAxis` tables declare their own
    /// `run_id` content column and append the rest; `None` tables carry their
    /// own identity columns and append neither.
    #[must_use]
    pub fn appends(self, column: &ManagedColumn) -> bool {
        match (self, column.scope) {
            (_, ManagedScope::Time) | (Self::Observation, _) => true,
            (Self::CodeAxis, _) => column.field.name != RUN_ID,
            (Self::None, _) => false,
        }
    }

    /// The managed columns a table under this policy appends, in physical order.
    pub fn managed_columns(self) -> impl Iterator<Item = &'static ManagedColumn> {
        MANAGED_COLUMNS
            .iter()
            .filter(move |column| self.appends(column))
    }
}

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

/// Report whether `name` is a managed correlation column.
///
/// Correlation columns attribute rows to a run, a Card version, and a writer.
/// They are identical on every retry of one batch, unlike the request and time
/// columns, which is what lets a retry be recognised as the same write.
#[must_use]
pub fn is_correlation_column(name: &str) -> bool {
    MANAGED_COLUMNS
        .iter()
        .any(|column| column.scope == ManagedScope::Correlation && column.field.name == name)
}

/// Append the managed columns a policy declares to a table's user fields.
///
/// User fields stay first, in their declared order, followed by the policy's
/// managed columns in [`MANAGED_COLUMNS`] order. The appended fields carry no
/// field metadata. The user-schema fingerprint hashes the user fields alone,
/// so appending them never perturbs it.
#[must_use]
pub fn ensure_managed_columns(
    mut user_fields: Vec<Field>,
    policy: CorrelationPolicy,
) -> Vec<Field> {
    user_fields.extend(policy.managed_columns().map(ManagedColumn::untagged_arrow));
    user_fields
}

/// Build one canonical signal table's complete physical Arrow fields.
///
/// The declared signal ledger comes first in declaration order, then the
/// `Observation` managed columns. Every field — ledger and managed, at every
/// nesting depth — carries its stable id and sensitivity metadata, which is
/// what lets the canonical physical fingerprint cover the whole schema.
#[must_use]
pub fn canonical_physical_fields(declared: &[CanonicalField]) -> Vec<Field> {
    let mut physical = canonical_arrow_fields(declared);
    physical.extend(
        CorrelationPolicy::Observation
            .managed_columns()
            .map(|column| column.field.to_arrow()),
    );
    physical
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names of `fields` in order.
    fn field_names(fields: &[Field]) -> Vec<&str> {
        fields.iter().map(|f| f.name().as_str()).collect()
    }

    /// Observation tables append every managed column in declared order.
    ///
    /// # Panics
    ///
    /// Panics when the order changes, a retired column reappears, or a
    /// required identity becomes nullable.
    #[test]
    fn observation_policy_appends_every_managed_column_in_order() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::Observation);
        assert_eq!(
            field_names(&fields),
            vec![
                RUN_ID,
                CARD_UID,
                PRINCIPAL_ID,
                WYRD_REQUEST_ID,
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
            ]
        );
        assert!(!fields[2].is_nullable());
        assert!(!fields[3].is_nullable());
    }

    /// Code-axis tables omit only the `run_id` they declare themselves.
    #[test]
    fn code_axis_policy_omits_run_id() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::CodeAxis);
        assert_eq!(
            field_names(&fields),
            vec![
                CARD_UID,
                PRINCIPAL_ID,
                WYRD_REQUEST_ID,
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
            ]
        );
    }

    /// Tables without a correlation policy append only the two time columns.
    #[test]
    fn none_policy_appends_only_the_time_columns() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::None);
        assert_eq!(
            field_names(&fields),
            vec![WYRD_EVENT_TIME, WYRD_INGESTED_AT]
        );
    }

    /// Caller-declared fields remain ahead of every server-managed field.
    #[test]
    fn user_fields_come_before_managed_columns() {
        let user = vec![Field::new("my_col", arrow::datatypes::DataType::Utf8, true)];
        let fields = ensure_managed_columns(user, CorrelationPolicy::Observation);
        assert_eq!(fields[0].name(), "my_col");
    }

    /// The managed-name check covers exactly the declaration.
    ///
    /// # Panics
    ///
    /// Panics when a declared column is not reported as managed, or when the
    /// retired batch column or a user name is.
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
        assert!(is_correlation_column(RUN_ID));
        assert!(is_correlation_column(PRINCIPAL_ID));
        assert!(!is_correlation_column(WYRD_REQUEST_ID));
        assert!(!is_correlation_column(WYRD_INGESTED_AT));
    }

    /// Canonical tables tag the managed columns with their stable ids.
    ///
    /// # Panics
    ///
    /// Panics when a canonical managed field's id differs from its declaration.
    #[test]
    fn canonical_physical_fields_tag_managed_columns_with_stable_ids() {
        let physical = canonical_physical_fields(&[]);
        let ids = physical
            .iter()
            .map(|field| {
                field
                    .metadata()
                    .get(crate::tables::fields::PARQUET_FIELD_ID)
                    .map(String::as_str)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                Some("1000"),
                Some("1001"),
                Some("1002"),
                Some("1003"),
                Some("1004"),
                Some("1005"),
            ]
        );
    }
}

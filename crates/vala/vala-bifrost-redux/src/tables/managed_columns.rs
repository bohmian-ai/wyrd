use arrow::datatypes::{DataType, Field, TimeUnit};

use crate::tables::CorrelationPolicy;
use crate::tables::fields::{CanonicalField, CanonicalType, canonical_arrow_fields};
use wyrd_spec::vala::{
    CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT, WYRD_REQUEST_ID, WYRD_ROW_ORDINAL,
};

/// Append the Bifrost system columns (and policy-gated correlation columns)
/// to the user fields of a pre-declared domain table.
///
/// Column order: policy correlation columns first, then system timestamp
/// columns, then `wyrd_batch_id`, `wyrd_row_ordinal`, and `data_tenant_id`.
///
/// This is the single source of truth for what gets appended per policy.
/// The appended columns here are excluded from `schema_fingerprint()`, which
/// hashes `arrow_fields()` only.
pub fn ensure_managed_columns(
    mut user_fields: Vec<Field>,
    policy: CorrelationPolicy,
) -> Vec<Field> {
    // Universal correlation columns per policy (C-01).
    match policy {
        CorrelationPolicy::Observation => {
            user_fields.push(Field::new(RUN_ID, DataType::Utf8, true));
            user_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
            user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, false));
            user_fields.push(Field::new(WYRD_REQUEST_ID, DataType::Utf8, false));
        }
        CorrelationPolicy::CodeAxis => {
            // run_id is declared as a code-axis user field; do not append the universal one.
            user_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
            user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, false));
            user_fields.push(Field::new(WYRD_REQUEST_ID, DataType::Utf8, false));
        }
    }

    // Bifrost system columns (always present on domain tables).
    user_fields.push(Field::new(
        WYRD_EVENT_TIME,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    user_fields.push(Field::new(
        WYRD_INGESTED_AT,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    user_fields.push(Field::new(
        WYRD_BATCH_ID,
        DataType::FixedSizeBinary(16),
        false,
    ));
    user_fields.push(Field::new(WYRD_ROW_ORDINAL, DataType::Int32, false));
    // Every physical table carries the tenant isolation key.
    user_fields.push(Field::new(DATA_TENANT_ID, DataType::Utf8, false));

    user_fields
}

/// The Observation envelope appended to every canonical signal table.
///
/// These nine fields are physical schema authority: they carry stable ids
/// 1000-1008 and the same names, types, order, and nullability
/// [`ensure_managed_columns`] appends for [`CorrelationPolicy::Observation`],
/// so a canonical table's envelope and a pre-declared table's envelope remain
/// one contract. They are stripped from the canonical user batch and stamped
/// from trusted Gate context, never supplied by a client.
///
/// The ids live in a range far above any signal ledger so a signal can add
/// fields indefinitely without ever colliding with the envelope.
pub static CANONICAL_ENVELOPE_FIELDS: &[CanonicalField] = &[
    CanonicalField::meta(1000, RUN_ID, CanonicalType::Utf8, true),
    CanonicalField::meta(1001, CARD_UID, CanonicalType::Utf8, true),
    CanonicalField::meta(1002, PRINCIPAL_ID, CanonicalType::Utf8, false),
    CanonicalField::meta(1003, WYRD_REQUEST_ID, CanonicalType::Utf8, false),
    CanonicalField::meta(
        1004,
        WYRD_EVENT_TIME,
        CanonicalType::Timestamp(TimeUnit::Microsecond, Some("UTC")),
        false,
    ),
    CanonicalField::meta(
        1005,
        WYRD_INGESTED_AT,
        CanonicalType::Timestamp(TimeUnit::Microsecond, Some("UTC")),
        false,
    ),
    CanonicalField::meta(
        1006,
        WYRD_BATCH_ID,
        CanonicalType::FixedSizeBinary(16),
        false,
    ),
    CanonicalField::meta(1007, WYRD_ROW_ORDINAL, CanonicalType::Int32, false),
    CanonicalField::meta(1008, DATA_TENANT_ID, CanonicalType::Utf8, false),
];

/// Build one canonical signal table's complete physical Arrow fields.
///
/// The declared signal ledger comes first in declaration order, then the
/// Observation envelope. Every field — signal and envelope, at every nesting
/// depth — carries its stable id and sensitivity metadata, which is what lets
/// Iceberg adopt the table's own ids and lets the canonical physical
/// fingerprint cover the whole schema.
#[must_use]
pub fn canonical_physical_fields(declared: &[CanonicalField]) -> Vec<Field> {
    let mut physical = canonical_arrow_fields(declared);
    physical.extend(canonical_arrow_fields(CANONICAL_ENVELOPE_FIELDS));
    physical
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_names(fields: &[Field]) -> Vec<&str> {
        fields.iter().map(|f| f.name().as_str()).collect()
    }

    #[test]
    /// Observation schemas match the non-null identities stamped by Scribe.
    fn observation_policy_appends_all_three_then_system() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::Observation);
        let names = field_names(&fields);
        assert_eq!(
            names,
            vec![
                RUN_ID,
                CARD_UID,
                PRINCIPAL_ID,
                WYRD_REQUEST_ID,
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
                WYRD_BATCH_ID,
                WYRD_ROW_ORDINAL,
                DATA_TENANT_ID
            ]
        );
        assert!(!fields[2].is_nullable());
        assert!(!fields[3].is_nullable());
    }

    #[test]
    /// Code-axis schemas omit only the already-declared run identifier.
    fn code_axis_policy_omits_run_id() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::CodeAxis);
        let names = field_names(&fields);
        assert!(
            !names.contains(&RUN_ID),
            "CodeAxis must not append universal run_id"
        );
        assert!(names.contains(&CARD_UID));
        assert!(names.contains(&PRINCIPAL_ID));
        assert!(names.contains(&WYRD_REQUEST_ID));
    }

    #[test]
    /// Caller-declared fields remain ahead of every server-managed field.
    fn user_fields_come_before_system() {
        let user = vec![Field::new("my_col", DataType::Utf8, true)];
        let fields = ensure_managed_columns(user, CorrelationPolicy::Observation);
        assert_eq!(fields[0].name(), "my_col");
    }

    /// Pins the Redux managed-column contract after removal of the duplicate catalog.
    ///
    /// # Panics
    ///
    /// Panics when any managed field differs in name, type, order, or nullability.
    #[test]
    fn redux_managed_columns_remain_stable() {
        let user_fields = vec![Field::new("value", DataType::UInt64, false)];
        let redux_dynamic = crate::schema::with_managed_columns(user_fields);
        assert_eq!(
            redux_dynamic
                .iter()
                .map(|field| (
                    field.name().as_str(),
                    field.data_type(),
                    field.is_nullable()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("value", &DataType::UInt64, false),
                ("run_id", &DataType::Utf8, true),
                ("card_uid", &DataType::Utf8, true),
                ("principal_id", &DataType::Utf8, false),
                ("wyrd_request_id", &DataType::Utf8, false),
                (
                    "wyrd_event_time",
                    &DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        Some("UTC".into())
                    ),
                    false,
                ),
                (
                    "wyrd_ingested_at",
                    &DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        Some("UTC".into())
                    ),
                    false,
                ),
                ("wyrd_batch_id", &DataType::FixedSizeBinary(16), false),
                ("wyrd_row_ordinal", &DataType::Int32, false),
                ("data_tenant_id", &DataType::Utf8, false),
            ]
        );

        for managed in [
            ensure_managed_columns(Vec::new(), CorrelationPolicy::Observation),
            ensure_managed_columns(Vec::new(), CorrelationPolicy::CodeAxis),
        ] {
            assert_eq!(
                managed.last().map(|field| field.name().as_str()),
                Some("data_tenant_id")
            );
            assert_eq!(
                managed
                    .iter()
                    .find(|field| field.name() == "wyrd_row_ordinal")
                    .map(|field| (field.data_type(), field.is_nullable())),
                Some((&DataType::Int32, false))
            );
        }

        for definition in crate::tables::builtin_tables() {
            let schema = (definition.schema)();
            let ordinal = schema
                .field_with_name("wyrd_row_ordinal")
                .expect("every initial built-in recipe carries row identity");
            assert_eq!(
                (ordinal.data_type(), ordinal.is_nullable()),
                (&DataType::Int32, false),
                "{}.{},",
                definition.namespace,
                definition.name,
            );
        }
    }
}

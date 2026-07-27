use arrow::datatypes::{DataType, Field, TimeUnit};

use crate::tables::CorrelationPolicy;
use wyrd_spec::vala::{
    CARD_UID, DATA_TENANT_ID, PRINCIPAL_ID, RUN_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME,
    WYRD_INGESTED_AT,
};

/// Append the Bifrost system columns (and policy-gated correlation columns)
/// to the user fields of a pre-declared domain table.
///
/// Column order: policy correlation columns first, then system timestamp
/// columns, then `wyrd_batch_id`, then `data_tenant_id`.
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
            user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, true));
        }
        CorrelationPolicy::CodeAxis => {
            // run_id is declared as a code-axis user field; do not append the universal one.
            user_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
            user_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, true));
        }
        CorrelationPolicy::None => {
            // audit_log: no universal correlation columns appended.
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
    // Every physical table carries the tenant isolation key.
    user_fields.push(Field::new(DATA_TENANT_ID, DataType::Utf8, false));

    user_fields
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_names(fields: &[Field]) -> Vec<&str> {
        fields.iter().map(|f| f.name().as_str()).collect()
    }

    #[test]
    fn observation_policy_appends_all_three_then_system() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::Observation);
        let names = field_names(&fields);
        assert_eq!(
            names,
            vec![
                RUN_ID,
                CARD_UID,
                PRINCIPAL_ID,
                WYRD_EVENT_TIME,
                WYRD_INGESTED_AT,
                WYRD_BATCH_ID,
                DATA_TENANT_ID
            ]
        );
    }

    #[test]
    fn code_axis_policy_omits_run_id() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::CodeAxis);
        let names = field_names(&fields);
        assert!(
            !names.contains(&RUN_ID),
            "CodeAxis must not append universal run_id"
        );
        assert!(names.contains(&CARD_UID));
        assert!(names.contains(&PRINCIPAL_ID));
    }

    #[test]
    fn none_policy_only_managed_columns() {
        let fields = ensure_managed_columns(vec![], CorrelationPolicy::None);
        let names = field_names(&fields);
        assert!(!names.contains(&RUN_ID));
        assert!(!names.contains(&CARD_UID));
        assert!(!names.contains(&PRINCIPAL_ID));
        assert!(names.contains(&WYRD_EVENT_TIME));
        assert!(names.contains(&DATA_TENANT_ID));
    }

    #[test]
    fn user_fields_come_before_system() {
        let user = vec![Field::new("my_col", DataType::Utf8, true)];
        let fields = ensure_managed_columns(user, CorrelationPolicy::Observation);
        assert_eq!(fields[0].name(), "my_col");
    }
}

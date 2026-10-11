//! Pure projection of one staged audit decision into the canonical audit
//! content schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::Schema;
use arrow::error::ArrowError;
use chrono::{DateTime, Utc};
use thiserror::Error;
use wyrd_queue::variant::{EncodedVariant, VariantColumnBuilder};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_spec::vala::audit_detail_canonical_json;

use super::{AuditLogTable, DETAIL};
use crate::tables::DomainTable;

/// Why one audit decision could not be projected into an `audit_log` row.
#[derive(Debug, Error)]
pub enum AuditProjectionError {
    /// The decision's canonical detail exceeds a Variant limit, so it cannot
    /// be stored in the Variant `detail` column.
    #[error("audit detail cannot be stored as a Variant: {0}")]
    InvalidDetail(BifrostError),
    /// The Arrow batch could not be assembled, which only a schema drift
    /// between this projection and the table declaration causes.
    #[error("audit content batch construction failed: {0}")]
    Arrow(#[from] ArrowError),
}

/// Projects `event`, decided at `decided_at`, into one `audit_log` row.
///
/// Column order follows [`AuditLogTable::arrow_fields`]. The detail is its
/// canonical JSON encoded as a Variant, the outcome and principal kind their
/// stable snake-case tags, and the decision instant is the `created_at` time
/// column, so a retained row partitions by when the boundary decided rather
/// than when it was written.
///
/// # Errors
///
/// Returns [`AuditProjectionError::InvalidDetail`] when the canonical detail
/// exceeds a Variant limit, and [`AuditProjectionError::Arrow`] when the batch
/// cannot be assembled.
pub fn project_audit_event(
    event: &AuditEvent,
    decided_at: DateTime<Utc>,
) -> Result<RecordBatch, AuditProjectionError> {
    let text = |value: Option<String>| Arc::new(StringArray::from(vec![value])) as ArrayRef;
    let detail = event.detail.as_ref().map(audit_detail_canonical_json);
    let detail =
        VariantColumnBuilder::encode(DETAIL, [detail.as_deref()], EncodedVariant::from_json_text)
            .map_err(AuditProjectionError::InvalidDetail)?;
    let columns = vec![
        text(Some(event.request_id.as_str().to_owned())),
        text(event.trace_id.clone()),
        text(Some(event.operation.clone())),
        text(Some(event.resource.clone())),
        text(event.card_ref.as_ref().map(ToString::to_string)),
        text(Some(event.principal_id.to_string())),
        text(Some(event.principal_kind.as_str().to_owned())),
        text(Some(event.permission.clone())),
        text(Some(
            match event.outcome {
                AuditOutcome::Allowed => "allowed",
                AuditOutcome::Denied => "denied",
            }
            .to_owned(),
        )),
        detail,
        text(event.credential_id.map(|id| id.to_string())),
        Arc::new(
            TimestampMicrosecondArray::from(vec![decided_at.timestamp_micros()])
                .with_timezone(iceberg::arrow::UTC_TIME_ZONE),
        ),
    ];
    let schema = Schema::new(AuditLogTable::arrow_fields());
    Ok(RecordBatch::try_new(Arc::new(schema), columns)?)
}

#[cfg(test)]
mod tests {
    use arrow::array::Array;
    use chrono::TimeZone;
    use uuid::Uuid;
    use wyrd_queue::variant::variant_cell_to_json;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

    use super::*;
    use crate::tables::audit::{CREATED_AT, CREDENTIAL_ID};

    /// Builds one denied system decision for the projection tests.
    fn event() -> AuditEvent {
        AuditEvent::new(
            RequestId::now_v7(),
            None,
            "auth.token.exchange".to_owned(),
            "principal:x".to_owned(),
            None,
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKindTag::System,
            "auth.token.exchange".to_owned(),
            AuditOutcome::Denied,
        )
    }

    /// One decision projects every content column in canonical order, the
    /// credential before the `created_at` time column, which carries the
    /// decision instant.
    ///
    /// # Panics
    ///
    /// Panics when the projection is refused or a column is misplaced.
    #[test]
    fn projects_canonical_content_with_the_decision_time() {
        let credential = Uuid::now_v7();
        let mut event = event();
        event.credential_id = Some(credential);
        let decided_at = Utc
            .timestamp_micros(1_700_000_000_000_007)
            .single()
            .expect("valid test timestamp");

        let row = project_audit_event(&event, decided_at).expect("valid projection");

        assert_eq!(row.num_rows(), 1);
        assert_eq!(row.num_columns(), 12);
        let text = |index: usize| {
            row.column(index)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("text column")
        };
        assert_eq!(row.schema().field(10).name(), CREDENTIAL_ID);
        assert_eq!(text(10).value(0), credential.to_string());
        assert_eq!(text(6).value(0), "system");
        assert_eq!(text(8).value(0), "denied");
        assert!(text(1).is_null(0), "no trace id projects null");
        assert!(row.column(9).is_null(0), "no detail projects null");
        let time = row
            .column(11)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("created_at column");
        assert_eq!(row.schema().field(11).name(), CREATED_AT);
        assert_eq!(time.value(0), decided_at.timestamp_micros());
    }

    /// A present detail projects as the Variant of its canonical JSON, so the
    /// stored value decodes back to exactly that JSON.
    ///
    /// # Panics
    ///
    /// Panics when the projection is refused or the Variant does not decode
    /// to the canonical detail.
    #[test]
    fn detail_projects_as_variant_of_the_canonical_json() {
        let mut event = event();
        event.detail = Some(wyrd_spec::vala::AuditDetail::OracleAdmissionRecovery {
            expired_lease_count: 1,
            active_lease_count: 2,
            interactive_slots: 3,
            analytical_slots: 4,
            total_slots: 7,
        });
        let canonical = audit_detail_canonical_json(event.detail.as_ref().expect("detail"));

        let row = project_audit_event(&event, Utc::now()).expect("valid projection");

        assert_eq!(
            variant_cell_to_json(row.column(9).as_ref(), 0).expect("decodes"),
            serde_json::from_str::<serde_json::Value>(&canonical).expect("canonical JSON")
        );
    }
}

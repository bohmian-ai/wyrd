//! Pure projection of one staged audit decision into the canonical audit
//! content schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::error::ArrowError;
use chrono::{DateTime, Utc};
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_spec::vala::audit_detail_canonical_json;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use super::AuditLogTable;
use crate::tables::DomainTable;

/// Projects `event`, decided at `decided_at`, into one `audit_log` row.
///
/// Column order follows [`AuditLogTable::arrow_fields`], followed by the
/// decision instant as the managed `wyrd_event_time`, so a retained row
/// partitions by when the boundary decided rather than when it was written.
/// The detail is its canonical JSON, the outcome and principal kind their
/// stable snake-case tags.
///
/// # Errors
///
/// Returns the Arrow error when the batch cannot be assembled, which only a
/// schema drift between this projection and the table declaration causes.
pub fn project_audit_event(
    event: &AuditEvent,
    decided_at: DateTime<Utc>,
) -> Result<RecordBatch, ArrowError> {
    let mut fields = AuditLogTable::arrow_fields();
    fields.push(Field::new(
        WYRD_EVENT_TIME,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    let text = |value: Option<String>| Arc::new(StringArray::from(vec![value])) as ArrayRef;
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
        text(event.detail.as_ref().map(audit_detail_canonical_json)),
        text(event.credential_id.map(|id| id.to_string())),
        Arc::new(
            TimestampMicrosecondArray::from(vec![decided_at.timestamp_micros()])
                .with_timezone(Arc::from("UTC")),
        ),
    ];
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)
}

#[cfg(test)]
mod tests {
    use arrow::array::Array;
    use chrono::TimeZone;
    use uuid::Uuid;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

    use super::*;

    /// One decision projects every content column in canonical order, the
    /// credential last before the managed event time, which carries the
    /// decision instant.
    ///
    /// # Panics
    ///
    /// Panics when the projection is refused or a column is misplaced.
    #[test]
    fn projects_canonical_content_with_the_decision_time() {
        let credential = Uuid::now_v7();
        let mut event = AuditEvent::new(
            RequestId::now_v7(),
            None,
            "auth.token.exchange".to_owned(),
            "principal:x".to_owned(),
            None,
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKindTag::System,
            "auth.token.exchange".to_owned(),
            AuditOutcome::Denied,
        );
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
        assert_eq!(row.schema().field(10).name(), super::super::CREDENTIAL_ID);
        assert_eq!(text(10).value(0), credential.to_string());
        assert_eq!(text(6).value(0), "system");
        assert_eq!(text(8).value(0), "denied");
        assert!(text(1).is_null(0), "no trace id projects null");
        assert!(text(9).is_null(0), "no detail projects null");
        let time = row
            .column(11)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("event time column");
        assert_eq!(row.schema().field(11).name(), WYRD_EVENT_TIME);
        assert_eq!(time.value(0), decided_at.timestamp_micros());
    }
}

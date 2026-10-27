//! Pure projection of one staged audit decision into one owned
//! row-attributed audit row.

use std::sync::{Arc, LazyLock};

use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::error::ArrowError;
use chrono::{DateTime, Utc};
use thiserror::Error;
use wyrd_queue::variant::EncodedVariant;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_spec::vala::managed_columns::{ACT_PRINCIPAL_ID, SUB_PRINCIPAL_ID, WYRD_REQUEST_ID};

use super::{AuditLogTable, DETAIL};
use crate::tables::staged::{Cell, StagedRow};
use crate::tables::{DomainTable, SHARED_DIMENSIONS};

/// The `audit_log` content schema followed by the decision's carried pair
/// and request, built once and shared by every projected row, so rows
/// compare as one schema by pointer instead of field by field.
static AUDIT_SCHEMA: LazyLock<SchemaRef> = LazyLock::new(|| {
    let mut fields = AuditLogTable::arrow_fields();
    fields.extend(
        [SUB_PRINCIPAL_ID, ACT_PRINCIPAL_ID, WYRD_REQUEST_ID]
            .map(|name| Field::new(name, DataType::Utf8, false)),
    );
    Arc::new(Schema::new(fields))
});

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

/// Stages `event`, decided at `decided_at`, as one `audit_log` row.
///
/// Cell order follows [`AuditLogTable::arrow_fields`], whose shared
/// dimensions are null, then the decision's `sub_principal_id`,
/// `act_principal_id`, and `wyrd_request_id`, which Scribe carries into the
/// managed envelope of a row-attributed frame. The detail is encoded
/// directly as a Variant when staged, the outcome and subject kind their
/// stable snake-case tags, and the decision instant is the `created_at` time
/// column, so a retained row partitions by when the boundary decided rather
/// than when it was written. The Scribe outbox builds a slice's staged rows
/// into one batch with [`StagedRow::project`].
///
/// # Errors
///
/// Returns [`AuditProjectionError::InvalidDetail`] when the detail exceeds a
/// Variant limit, and [`AuditProjectionError::Arrow`] when the cells do not
/// fill the audit schema.
pub fn stage_audit_event(
    event: &AuditEvent,
    decided_at: DateTime<Utc>,
) -> Result<StagedRow, AuditProjectionError> {
    let detail = event
        .detail
        .as_ref()
        .map(EncodedVariant::from_serialize)
        .transpose()
        .map_err(|violation| {
            AuditProjectionError::InvalidDetail(violation.into_error(DETAIL, 0))
        })?;
    let outcome = match event.outcome {
        AuditOutcome::Allowed => "allowed",
        AuditOutcome::Denied => "denied",
    };
    let mut cells = vec![
        Cell::text(event.trace_id.as_deref()),
        Cell::text(Some(event.operation.as_str())),
        Cell::text(Some(event.resource.as_str())),
        Cell::text(event.card_ref.as_ref().map(ToString::to_string)),
        Cell::text(Some(event.sub_principal_kind.as_str())),
        Cell::text(Some(event.permission.as_str())),
        Cell::text(Some(outcome)),
        Cell::Variant(detail),
        Cell::text(event.credential_id.map(|id| id.to_string())),
        Cell::Micros(Some(decided_at.timestamp_micros())),
    ];
    cells.extend(SHARED_DIMENSIONS.map(|_| Cell::Text(None)));
    cells.extend([
        Cell::text(Some(event.sub_principal_id.to_string())),
        Cell::text(Some(event.act_principal_id.to_string())),
        Cell::text(Some(event.request_id.as_str())),
    ]);
    Ok(StagedRow::new(Arc::clone(&AUDIT_SCHEMA), cells)?)
}

#[cfg(test)]
mod tests {
    use arrow::array::{Array, RecordBatch, StringArray, TimestampMicrosecondArray};
    use chrono::TimeZone;
    use uuid::Uuid;
    use wyrd_queue::variant::variant_cell_to_json;
    use wyrd_spec::auth::{Attribution, PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

    use super::*;
    use crate::tables::audit::{CREATED_AT, CREDENTIAL_ID};

    /// `event` staged at `decided_at` and built as a one-row batch.
    ///
    /// # Panics
    ///
    /// Panics when the decision cannot be staged or built.
    fn project(event: &AuditEvent, decided_at: DateTime<Utc>) -> RecordBatch {
        let row = stage_audit_event(event, decided_at).expect("valid projection");
        StagedRow::project(&[row]).expect("one row builds")
    }

    /// Builds one denied system decision for the projection tests.
    fn event() -> AuditEvent {
        AuditEvent::new(
            RequestId::now_v7(),
            None,
            "auth.token.exchange".to_owned(),
            "principal:x".to_owned(),
            None,
            Attribution::direct(PrincipalId::new(Uuid::now_v7())),
            PrincipalKindTag::System,
            "auth.token.exchange".to_owned(),
            AuditOutcome::Denied,
        )
    }

    /// One decision projects every content column in canonical order, the
    /// credential before the `created_at` time column, which carries the
    /// decision instant, then null shared dimensions and the carried pair and
    /// request.
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

        let row = project(&event, decided_at);

        assert_eq!(row.num_rows(), 1);
        assert_eq!(row.num_columns(), 19);
        let text = |name: &str| {
            row.column_by_name(name)
                .expect("projected column")
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("text column")
        };
        assert_eq!(row.schema().field(8).name(), CREDENTIAL_ID);
        assert_eq!(text(CREDENTIAL_ID).value(0), credential.to_string());
        assert_eq!(
            text(SUB_PRINCIPAL_ID).value(0),
            event.sub_principal_id.to_string()
        );
        assert_eq!(text("sub_principal_kind").value(0), "system");
        assert_eq!(
            text(ACT_PRINCIPAL_ID).value(0),
            event.act_principal_id.to_string()
        );
        assert_eq!(text(WYRD_REQUEST_ID).value(0), event.request_id.as_str());
        assert_eq!(text("outcome").value(0), "denied");
        assert!(text("trace_id").is_null(0), "no trace id projects null");
        assert!(row.column(7).is_null(0), "no detail projects null");
        for dimension in SHARED_DIMENSIONS {
            assert!(text(dimension).is_null(0), "{dimension} projects null");
        }
        let time = row
            .column(9)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("created_at column");
        assert_eq!(row.schema().field(9).name(), CREATED_AT);
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
        let canonical =
            wyrd_spec::vala::audit_detail_canonical_json(event.detail.as_ref().expect("detail"));

        let row = project(&event, Utc::now());

        assert_eq!(
            variant_cell_to_json(row.column(7).as_ref(), 0).expect("decodes"),
            serde_json::from_str::<serde_json::Value>(&canonical).expect("canonical JSON")
        );
    }
}

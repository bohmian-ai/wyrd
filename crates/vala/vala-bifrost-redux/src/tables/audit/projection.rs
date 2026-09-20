//! Pure projection of SQL audit rows into the canonical audit content schema.

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use vala_sql::row_types::audit_staging::AuditStagingRow;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use super::AuditLogTable;
use crate::tables::DomainTable;

const AUDIT_PROJECTION_DOMAIN: &[u8] = b"wyrd.vala.audit_projection.v1";

/// Canonical audit content projection ready for Scribe physical stamping.
#[derive(Debug)]
pub struct AuditProjection {
    /// Tenant that owns every projected row.
    pub tenant: DataTenantId,
    /// First sequence number in the contiguous range.
    pub seq_lo: i64,
    /// Last sequence number in the contiguous range.
    pub seq_hi: i64,
    /// Stable idempotency key for the projected shipment.
    ///
    /// Derived from the tenant and the frozen inclusive sequence range alone,
    /// so every replica and every crash replay of the same frozen range
    /// presents Scribe the same identity and lands on its durable batch fence.
    pub batch_id: Uuid,
    /// The 14 content fields declared by [`AuditLogTable`], plus the
    /// `wyrd_event_time` the decision was stamped with in Postgres.
    pub rows: RecordBatch,
}

/// Errors raised while projecting a SQL audit range.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditProjectionError {
    #[error("audit projection requires at least one row")]
    Empty,
    #[error(
        "audit row {index} belongs to tenant {row_tenant}, not authenticated tenant {authenticated_tenant}"
    )]
    TenantMismatch {
        index: usize,
        row_tenant: Uuid,
        authenticated_tenant: Uuid,
    },
    #[error("audit sequence is not contiguous at row {index}: expected {expected}, got {actual}")]
    NonContiguousSequence {
        index: usize,
        expected: i64,
        actual: i64,
    },
    #[error("audit row {index} has {field} hash length {length}, expected 32 bytes")]
    InvalidHashLength {
        index: usize,
        field: &'static str,
        length: usize,
    },
    #[error("audit row {index} has invalid {field} value {value:?}")]
    InvalidEnum {
        index: usize,
        field: &'static str,
        value: String,
    },
    #[error("audit range sequence overflow")]
    SequenceOverflow,
    #[error("audit content schema construction failed: {0}")]
    Schema(String),
}

/// Project one authenticated tenant's ordered audit rows without IO.
pub fn project_audit_rows(
    authenticated_tenant: DataTenantId,
    rows: &[AuditStagingRow],
) -> Result<AuditProjection, AuditProjectionError> {
    let range = validate_range(authenticated_tenant, rows)?;
    Ok(AuditProjection {
        tenant: authenticated_tenant,
        seq_lo: range.seq_lo,
        seq_hi: range.seq_hi,
        batch_id: derive_batch_id(authenticated_tenant, range.seq_lo, range.seq_hi),
        rows: project_record_batch(rows)?,
    })
}

struct ValidatedAuditRange {
    seq_lo: i64,
    seq_hi: i64,
}

fn validate_range(
    authenticated_tenant: DataTenantId,
    rows: &[AuditStagingRow],
) -> Result<ValidatedAuditRange, AuditProjectionError> {
    if rows.is_empty() {
        return Err(AuditProjectionError::Empty);
    }

    let seq_lo = rows[0].seq;
    let mut expected_seq = seq_lo;
    for (index, row) in rows.iter().enumerate() {
        if row.data_tenant_id != authenticated_tenant.as_uuid() {
            return Err(AuditProjectionError::TenantMismatch {
                index,
                row_tenant: row.data_tenant_id,
                authenticated_tenant: authenticated_tenant.as_uuid(),
            });
        }
        if row.seq != expected_seq {
            return Err(AuditProjectionError::NonContiguousSequence {
                index,
                expected: expected_seq,
                actual: row.seq,
            });
        }
        expected_seq = expected_seq
            .checked_add(1)
            .ok_or(AuditProjectionError::SequenceOverflow)?;

        let entry_hash = hash_hex(index, "entry_hash", &row.entry_hash)?;
        let prev_hash = hash_hex(index, "prev_hash", &row.prev_hash)?;
        validate_enum(
            index,
            "principal_kind",
            &row.principal_kind,
            ["global_admin", "tenant_admin", "user", "service", "agent"],
        )?;
        validate_enum(index, "outcome", &row.outcome, ["allowed", "denied"])?;

        let _ = (entry_hash, prev_hash);
    }

    let seq_hi = rows
        .last()
        .map(|row| row.seq)
        .ok_or(AuditProjectionError::Empty)?;
    Ok(ValidatedAuditRange { seq_lo, seq_hi })
}

fn project_record_batch(rows: &[AuditStagingRow]) -> Result<RecordBatch, AuditProjectionError> {
    let seq_values = rows.iter().map(|row| row.seq).collect::<Vec<_>>();
    let entry_hash_values = rows
        .iter()
        .enumerate()
        .map(|(index, row)| hash_hex(index, "entry_hash", &row.entry_hash))
        .collect::<Result<Vec<_>, _>>()?;
    let prev_hash_values = rows
        .iter()
        .enumerate()
        .map(|(index, row)| hash_hex(index, "prev_hash", &row.prev_hash))
        .collect::<Result<Vec<_>, _>>()?;
    let request_id_values = rows
        .iter()
        .map(|row| row.request_id.clone())
        .collect::<Vec<_>>();
    let trace_id_values = rows
        .iter()
        .map(|row| row.trace_id.clone())
        .collect::<Vec<_>>();
    let operation_values = rows
        .iter()
        .map(|row| row.operation.clone())
        .collect::<Vec<_>>();
    let resource_values = rows
        .iter()
        .map(|row| row.resource.clone())
        .collect::<Vec<_>>();
    let card_ref_values = rows
        .iter()
        .map(|row| row.card_ref.clone())
        .collect::<Vec<_>>();
    let principal_id_values = rows
        .iter()
        .map(|row| row.principal_id.to_string())
        .collect::<Vec<_>>();
    let principal_kind_values = rows
        .iter()
        .map(|row| row.principal_kind.clone())
        .collect::<Vec<_>>();
    let credential_id_values = rows
        .iter()
        .map(|row| row.credential_id.map(|id| id.to_string()))
        .collect::<Vec<_>>();
    let outcome_values = rows
        .iter()
        .map(|row| row.outcome.clone())
        .collect::<Vec<_>>();
    let permission_values = rows
        .iter()
        .map(|row| row.permission.clone())
        .collect::<Vec<_>>();
    let detail_values = rows
        .iter()
        .map(|row| row.detail.clone())
        .collect::<Vec<_>>();
    // The decision instant travels as the event time rather than as content, so
    // a retained row partitions by when the boundary decided, not by when the
    // publisher happened to ship it.
    let event_time_values = rows
        .iter()
        .map(|row| row.created_at.timestamp_micros())
        .collect::<Vec<_>>();
    let mut fields = AuditLogTable::arrow_fields();
    fields.push(Field::new(
        WYRD_EVENT_TIME,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        false,
    ));
    let schema = Arc::new(Schema::new(fields));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(seq_values)),
        Arc::new(StringArray::from(entry_hash_values)),
        Arc::new(StringArray::from(prev_hash_values)),
        Arc::new(StringArray::from(request_id_values)),
        Arc::new(StringArray::from(trace_id_values)),
        Arc::new(StringArray::from(operation_values)),
        Arc::new(StringArray::from(resource_values)),
        Arc::new(StringArray::from(card_ref_values)),
        Arc::new(StringArray::from(principal_id_values)),
        Arc::new(StringArray::from(principal_kind_values)),
        Arc::new(StringArray::from(permission_values)),
        Arc::new(StringArray::from(outcome_values)),
        Arc::new(StringArray::from(detail_values)),
        Arc::new(StringArray::from(credential_id_values)),
        Arc::new(
            TimestampMicrosecondArray::from(event_time_values).with_timezone(Arc::from("UTC")),
        ),
    ];
    let rows = RecordBatch::try_new(schema, columns)
        .map_err(|error| AuditProjectionError::Schema(error.to_string()))?;

    Ok(rows)
}

fn hash_hex(
    index: usize,
    field: &'static str,
    bytes: &[u8],
) -> Result<String, AuditProjectionError> {
    if bytes.len() != 32 {
        return Err(AuditProjectionError::InvalidHashLength {
            index,
            field,
            length: bytes.len(),
        });
    }
    Ok(hex::encode(bytes))
}

fn validate_enum<const N: usize>(
    index: usize,
    field: &'static str,
    value: &str,
    allowed: [&str; N],
) -> Result<(), AuditProjectionError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(AuditProjectionError::InvalidEnum {
            index,
            field,
            value: value.to_owned(),
        })
    }
}

/// Derive the deterministic shipment key for one tenant audit range.
///
/// The digest covers only the tenant and the inclusive sequence bounds, which
/// is exactly the state a restarted or competing publisher can reconstruct from
/// the frozen in-flight bound. Scribe's fence requires the sortable-random
/// version and variant bits, so only those are stamped over the digest prefix.
///
/// # Panics
/// Never panics: a SHA-256 digest always yields the sixteen-byte prefix the
/// `expect` names.
fn derive_batch_id(tenant: DataTenantId, seq_lo: i64, seq_hi: i64) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(AUDIT_PROJECTION_DOMAIN);
    hasher.update(tenant.as_uuid().as_bytes());
    hasher.update(seq_lo.to_be_bytes());
    hasher.update(seq_hi.to_be_bytes());
    let digest = hasher.finalize();
    let mut bytes: [u8; 16] = digest[..16]
        .try_into()
        .expect("SHA-256 prefix has 16 bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use arrow::array::{Array, StringArray};
    use chrono::{TimeZone, Utc};

    use super::*;

    fn tenant(byte: u8) -> DataTenantId {
        let mut bytes = *Uuid::now_v7().as_bytes();
        bytes[0] = byte;
        DataTenantId::new(Uuid::from_bytes(bytes)).expect("UUIDv7 test tenant")
    }

    fn row(tenant: DataTenantId, seq: i64) -> AuditStagingRow {
        AuditStagingRow {
            data_tenant_id: tenant.as_uuid(),
            seq,
            entry_hash: vec![0xab; 32],
            prev_hash: vec![0xcd; 32],
            request_id: format!("request-{seq}"),
            trace_id: None,
            operation: "audit.write".to_owned(),
            resource: "vala.system.audit_log".to_owned(),
            card_ref: None,
            principal_id: Uuid::now_v7(),
            principal_kind: "user".to_owned(),
            credential_id: None,
            permission: "audit:write".to_owned(),
            outcome: "allowed".to_owned(),
            detail: None,
            created_at: Utc
                .timestamp_micros(1_700_000_000_000_000 + seq)
                .single()
                .expect("valid test timestamp"),
        }
    }

    #[test]
    fn projects_canonical_content_and_derives_missing_batch_id() {
        let authenticated = tenant(1);
        let rows = vec![row(authenticated, 7), row(authenticated, 8)];
        let projection = project_audit_rows(authenticated, &rows).expect("valid projection");

        assert_eq!(projection.tenant, authenticated);
        assert_eq!((projection.seq_lo, projection.seq_hi), (7, 8));
        assert_eq!(projection.batch_id, derive_batch_id(authenticated, 7, 8));
        assert_eq!(projection.rows.num_columns(), 14);
        assert_eq!(
            projection.rows.schema().field(13).name(),
            WYRD_EVENT_TIME,
            "the decision instant travels as the managed event time"
        );

        let hashes = projection
            .rows
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("string hash column");
        assert_eq!(hashes.value(0), "ab".repeat(32));
        assert!(projection.rows.column(4).is_null(0));
        assert!(projection.rows.column(7).is_null(0));
        assert!(projection.rows.column(12).is_null(0));
    }

    #[test]
    fn rejects_empty_rows() {
        assert!(matches!(
            project_audit_rows(tenant(1), &[]),
            Err(AuditProjectionError::Empty)
        ));
    }

    /// The system owner's staged security decisions project like any tenant's,
    /// so they can reach retained history.
    ///
    /// # Panics
    ///
    /// Panics when the system-owner range is refused.
    #[test]
    fn projects_system_owner_rows() {
        let system = DataTenantId::SYSTEM_OWNER;
        let projection = project_audit_rows(system, &[row(system, 1)])
            .expect("system-owner decisions project into retained history");
        assert_eq!(projection.tenant, system);
    }

    #[test]
    fn rejects_cross_tenant_rows() {
        let authenticated = tenant(1);
        let other = tenant(2);
        assert!(matches!(
            project_audit_rows(authenticated, &[row(other, 1)]),
            Err(AuditProjectionError::TenantMismatch { index: 0, .. })
        ));
    }

    #[test]
    fn rejects_non_contiguous_sequences() {
        let authenticated = tenant(1);
        assert!(matches!(
            project_audit_rows(
                authenticated,
                &[row(authenticated, 1), row(authenticated, 3)]
            ),
            Err(AuditProjectionError::NonContiguousSequence {
                index: 1,
                expected: 2,
                actual: 3
            })
        ));
    }

    #[test]
    fn rejects_invalid_hash_and_enum_values() {
        let authenticated = tenant(1);
        let mut bad_hash = row(authenticated, 1);
        bad_hash.entry_hash.pop();
        assert!(matches!(
            project_audit_rows(authenticated, &[bad_hash]),
            Err(AuditProjectionError::InvalidHashLength {
                field: "entry_hash",
                ..
            })
        ));

        let mut bad_enum = row(authenticated, 1);
        bad_enum.outcome = "maybe".to_owned();
        assert!(matches!(
            project_audit_rows(authenticated, &[bad_enum]),
            Err(AuditProjectionError::InvalidEnum {
                field: "outcome",
                ..
            })
        ));
    }
}

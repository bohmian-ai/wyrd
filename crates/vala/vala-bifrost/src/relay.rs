//! S3.C5 background audit relay.
//!
//! Ships hash-chained rows from the transactional `vala.audit_outbox` into the
//! queryable `vala.system.audit_log` warehouse table. One tick claims unshipped
//! rows across tenants (SECURITY DEFINER, `FOR UPDATE SKIP LOCKED`), groups them
//! by tenant, flushes one `RecordBatch` per tenant through the ordinary writer
//! path under `origin = "audit-relay"` (so the relay's own write never
//! self-feeds the spine, M-11), then marks the shipped `seq` range.
//!
//! Re-ship after a crash between flush and mark is safe: the deterministic
//! `batch_id = derive(tenant, seq_lo, seq_hi)` replays the prior `vala.olap_commits`
//! commit (no duplicate warehouse rows), and `mark_audit_shipped` is idempotent
//! via its `AND NOT shipped` guard.

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::system_columns::{CARD_REF, RUN_ID};

use crate::catalog::WyrdCatalog;
use crate::catalog::namespaces::BifrostNamespace;
use crate::error::BifrostError;
use crate::types::TableScope;
use crate::writer::BifrostWriteContext;
use crate::writer::coordinator::AUDIT_RELAY_ORIGIN;

use vala_sql::TenantConn;
use vala_sql::row_types::audit_outbox::AuditOutboxRow;

/// Bifrost table name of the audit warehouse (`vala.system.audit_log`).
pub const AUDIT_LOG_TABLE: &str = "audit_log";

/// Background relay from `vala.audit_outbox` to `vala.system.audit_log`.
pub struct AuditRelay {
    catalog: Arc<WyrdCatalog>,
    pool: Arc<PgPool>,
}

/// One tenant's contiguous unshipped `seq` range, ready to flush and mark.
pub struct RelayShipment {
    /// The tenant whose rows this shipment carries.
    pub tenant: DataTenantId,
    /// Lowest `seq` in the shipment.
    pub seq_lo: i64,
    /// Highest `seq` in the shipment.
    pub seq_hi: i64,
    /// Deterministic idempotency key for the warehouse flush.
    pub batch_id: [u8; 16],
    rows: Vec<AuditOutboxRow>,
}

impl AuditRelay {
    /// Build a relay over `catalog` and the `wyrd_app` `pool`.
    #[must_use]
    pub fn new(catalog: Arc<WyrdCatalog>, pool: Arc<PgPool>) -> Self {
        Self { catalog, pool }
    }

    /// Idempotently ensure `vala.system.audit_log` exists. Call once at boot
    /// before the first tick.
    ///
    /// # Errors
    /// Returns [`BifrostError`] when the catalog create/register fails.
    pub async fn ensure_audit_log_table(&self) -> Result<(), BifrostError> {
        self.catalog
            .ensure_system_table(BifrostNamespace::System, AUDIT_LOG_TABLE, audit_log_fields(), &[])
            .await
    }

    /// Claim up to `limit` unshipped rows and group them into per-tenant
    /// shipments. Rows are claimed under a short transaction that commits
    /// immediately (releasing the `FOR UPDATE` locks); idempotency across a
    /// crash is provided by the deterministic `batch_id` and the `NOT shipped`
    /// mark guard, not by holding the lock across the flush.
    ///
    /// # Errors
    /// Returns [`BifrostError`] when the claim query fails.
    pub async fn claim(&self, limit: i32) -> Result<Vec<RelayShipment>, BifrostError> {
        let mut conn = TenantConn::acquire(&self.pool, DataTenantId::SYSTEM_OWNER)
            .await
            .map_err(BifrostError::Sql)?;
        let rows = vala_sql::queries::audit_outbox::claim_unshipped_audit(&mut conn, limit)
            .await
            .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;

        group_by_tenant(rows)
    }

    /// Flush one tenant's shipment into `vala.system.audit_log` under
    /// `origin = "audit-relay"` (skips the audit self-append, M-11). Idempotent:
    /// a re-flush of the same `batch_id` replays the prior commit's snapshot.
    ///
    /// # Errors
    /// Returns [`BifrostError`] when the warehouse write fails.
    pub async fn ship(&self, shipment: &RelayShipment) -> Result<(), BifrostError> {
        let batch = build_audit_log_batch(&shipment.rows)?;
        let writer = self
            .catalog
            .writer(
                BifrostNamespace::System,
                AUDIT_LOG_TABLE,
                TableScope::SystemShared,
                shipment.tenant,
            )
            .await?;
        writer.write(batch).await?;
        let ctx = BifrostWriteContext {
            batch_id: shipment.batch_id,
            origin: AUDIT_RELAY_ORIGIN.to_owned(),
            actor: AUDIT_RELAY_ORIGIN.to_owned(),
            request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("UUIDv7 is a valid request id"),
            card_ref: None,
        };
        writer.flush(ctx).await?;
        Ok(())
    }

    /// Mark a shipped `seq` range shipped under the tenant's bind. Idempotent via
    /// the `NOT shipped` guard. Returns the number of rows transitioned.
    ///
    /// # Errors
    /// Returns [`BifrostError`] when the update fails.
    pub async fn mark_shipped(&self, shipment: &RelayShipment) -> Result<u64, BifrostError> {
        let mut conn = TenantConn::acquire(&self.pool, shipment.tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let n = vala_sql::queries::audit_outbox::mark_audit_shipped(
            &mut conn,
            shipment.seq_lo,
            shipment.seq_hi,
            &shipment.batch_id,
        )
        .await
        .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        Ok(n)
    }

    /// Run one full relay cycle: claim, then ship-and-mark each tenant shipment.
    /// Returns the number of rows marked shipped.
    ///
    /// # Errors
    /// Returns [`BifrostError`] on the first claim/ship/mark failure.
    pub async fn tick(&self, limit: i32) -> Result<u64, BifrostError> {
        let shipments = self.claim(limit).await?;
        let mut shipped = 0;
        for shipment in &shipments {
            self.ship(shipment).await?;
            shipped += self.mark_shipped(shipment).await?;
        }
        Ok(shipped)
    }
}

/// Group claim rows (ordered by `(data_tenant_id, seq)`) into contiguous
/// per-tenant shipments with a deterministic `batch_id`.
fn group_by_tenant(rows: Vec<AuditOutboxRow>) -> Result<Vec<RelayShipment>, BifrostError> {
    let mut groups: Vec<Vec<AuditOutboxRow>> = Vec::new();
    for row in rows {
        match groups.last_mut() {
            Some(last) if last[0].data_tenant_id == row.data_tenant_id => last.push(row),
            _ => groups.push(vec![row]),
        }
    }

    groups
        .into_iter()
        .map(|rows| {
            let tenant_uuid = rows[0].data_tenant_id;
            let tenant = if tenant_uuid.is_nil() {
                DataTenantId::SYSTEM_OWNER
            } else {
                DataTenantId::new(tenant_uuid)
                    .map_err(|e| BifrostError::Internal(format!("relay: bad tenant id: {e}")))?
            };
            let seq_lo = rows.first().expect("group is non-empty").seq;
            let seq_hi = rows.last().expect("group is non-empty").seq;
            let batch_id = derive_batch_id(tenant, seq_lo, seq_hi);
            Ok(RelayShipment {
                tenant,
                seq_lo,
                seq_hi,
                batch_id,
                rows,
            })
        })
        .collect()
}

/// Deterministic 16-byte idempotency key for a tenant's shipped `seq` range.
///
/// Stable across crash/retry so a re-ship replays the prior `vala.olap_commits`
/// commit rather than writing duplicate warehouse rows.
fn derive_batch_id(tenant: DataTenantId, seq_lo: i64, seq_hi: i64) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(AUDIT_RELAY_ORIGIN.as_bytes());
    hasher.update(tenant.as_uuid().as_bytes());
    hasher.update(seq_lo.to_be_bytes());
    hasher.update(seq_hi.to_be_bytes());
    let digest = hasher.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

/// User (content) fields of `vala.system.audit_log`. The correlation
/// (`run_id`/`card_ref`) and system (`wyrd_*`/`data_tenant_id`) columns are
/// appended by `create_table`; the writer-identity card is `audit_card_ref` so
/// it never collides with the reserved `card_ref` correlation column.
fn audit_log_fields() -> Vec<Field> {
    vec![
        Field::new("seq", DataType::Int64, false),
        Field::new("entry_hash", DataType::Utf8, false),
        Field::new("prev_hash", DataType::Utf8, false),
        Field::new("request_id", DataType::Utf8, false),
        Field::new("trace_id", DataType::Utf8, true),
        Field::new("operation", DataType::Utf8, false),
        Field::new("resource", DataType::Utf8, false),
        Field::new("audit_card_ref", DataType::Utf8, true),
        Field::new("principal_id", DataType::Utf8, false),
        Field::new("principal_kind", DataType::Utf8, false),
        Field::new("auth_method", DataType::Utf8, false),
        Field::new("permission", DataType::Utf8, false),
        Field::new("decision", DataType::Utf8, false),
        Field::new("result", DataType::Utf8, false),
        Field::new("payload_summary", DataType::Utf8, false),
        Field::new("created_at_us", DataType::Int64, false),
    ]
}

/// Build the Arrow batch of audit content columns for one tenant's shipment.
///
/// The batch carries the content fields plus the nullable `run_id`/`card_ref`
/// correlation columns (both NULL for relay writes) so the stamped batch aligns
/// with the stored physical schema; the server stamps the remaining system
/// columns (`wyrd_*`, `data_tenant_id`) at flush.
fn build_audit_log_batch(rows: &[AuditOutboxRow]) -> Result<RecordBatch, BifrostError> {
    let seq = Int64Array::from(rows.iter().map(|r| r.seq).collect::<Vec<_>>());
    let entry_hash = StringArray::from(rows.iter().map(|r| hex(&r.entry_hash)).collect::<Vec<_>>());
    let prev_hash = StringArray::from(rows.iter().map(|r| hex(&r.prev_hash)).collect::<Vec<_>>());
    let request_id = StringArray::from(rows.iter().map(|r| r.request_id.clone()).collect::<Vec<_>>());
    let trace_id = StringArray::from(rows.iter().map(|r| r.trace_id.clone()).collect::<Vec<_>>());
    let operation = StringArray::from(rows.iter().map(|r| r.operation.clone()).collect::<Vec<_>>());
    let resource = StringArray::from(rows.iter().map(|r| r.resource.clone()).collect::<Vec<_>>());
    let audit_card_ref =
        StringArray::from(rows.iter().map(|r| r.card_ref.clone()).collect::<Vec<_>>());
    let principal_id =
        StringArray::from(rows.iter().map(|r| r.principal_id.to_string()).collect::<Vec<_>>());
    let principal_kind =
        StringArray::from(rows.iter().map(|r| r.principal_kind.clone()).collect::<Vec<_>>());
    let auth_method =
        StringArray::from(rows.iter().map(|r| r.auth_method.clone()).collect::<Vec<_>>());
    let permission = StringArray::from(rows.iter().map(|r| r.permission.clone()).collect::<Vec<_>>());
    let decision = StringArray::from(rows.iter().map(|r| r.decision.clone()).collect::<Vec<_>>());
    let result = StringArray::from(rows.iter().map(|r| r.result.clone()).collect::<Vec<_>>());
    let payload_summary =
        StringArray::from(rows.iter().map(|r| r.payload_summary.clone()).collect::<Vec<_>>());
    let created_at_us = Int64Array::from(
        rows.iter().map(|r| r.created_at.timestamp_micros()).collect::<Vec<_>>(),
    );
    let null_corr = StringArray::from(vec![None::<String>; rows.len()]);

    let mut fields = audit_log_fields();
    fields.push(Field::new(RUN_ID, DataType::Utf8, true));
    fields.push(Field::new(CARD_REF, DataType::Utf8, true));
    let schema = Arc::new(Schema::new(fields));

    let columns: Vec<ArrayRef> = vec![
        Arc::new(seq),
        Arc::new(entry_hash),
        Arc::new(prev_hash),
        Arc::new(request_id),
        Arc::new(trace_id),
        Arc::new(operation),
        Arc::new(resource),
        Arc::new(audit_card_ref),
        Arc::new(principal_id),
        Arc::new(principal_kind),
        Arc::new(auth_method),
        Arc::new(permission),
        Arc::new(decision),
        Arc::new(result),
        Arc::new(payload_summary),
        Arc::new(created_at_us),
        Arc::new(null_corr.clone()),
        Arc::new(null_corr),
    ];

    RecordBatch::try_new(schema, columns).map_err(BifrostError::Arrow)
}

/// Lowercase hex-encode a byte slice (used for the chain-hash columns).
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

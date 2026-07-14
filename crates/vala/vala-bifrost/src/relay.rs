//! background audit relay.
//!
//! Ships hash-chained rows from the transactional `vala.audit_outbox` into the
//! queryable `vala.system.audit_log` warehouse table. One tick claims unshipped
//! rows across tenants (SECURITY DEFINER, `FOR UPDATE SKIP LOCKED`), groups them
//! by tenant, flushes one `RecordBatch` per tenant through the ordinary writer
//! path under `origin = "audit-relay"` (so the relay's own write never
//! self-feeds the spine, M-11), then marks the shipped `seq` range.
//!
//! ## Crash-safety idempotency
//!
//! `ship_batch_id` is stamped on claimed rows **inside the claim transaction**
//! before commit. On a crash between `ship` and `mark_shipped`, the next `claim`
//! returns those rows with `ship_batch_id` already set; the relay reuses that
//! stored key instead of re-deriving from the live seq range. Because new rows
//! may have been appended for the same tenant in the interim (shifting the range),
//! re-deriving would mint a different `batch_id` and re-ship already-committed rows
//! into the append-only `vala.system.audit_log`. The persisted key prevents that.

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;

use crate::catalog::WyrdCatalog;
use crate::catalog::namespaces::BifrostNamespace;
use crate::error::BifrostError;
use crate::tables::{self, DomainTable};
use crate::types::TableScope;
use crate::writer::BifrostWriteContext;
use crate::writer::coordinator::AUDIT_RELAY_ORIGIN;

use vala_sql::TenantConn;
use vala_sql::queries::audit_outbox::stamp_audit_ship_batch_id;
use vala_sql::row_types::audit_outbox::AuditOutboxRow;

/// Bifrost table name of the audit warehouse (`vala.system.audit_log`).
pub const AUDIT_LOG_TABLE: &str = tables::system::AuditLogTable::NAME;

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

    /// Idempotently ensure `vala.system.audit_log` exists and is registered with
    /// fingerprint pinning. Delegates to the `DomainTable` registration path
    /// (`register_all` equivalent) so the table goes through the same steady-state
    /// and split-brain checks as all other pre-declared domain tables.
    ///
    /// # Errors
    /// Returns [`BifrostError`] when the catalog create/register fails.
    pub async fn ensure_audit_log_table(&self) -> Result<(), BifrostError> {
        tables::register::<tables::system::AuditLogTable>(&self.catalog).await
    }

    /// Claim up to `limit` unshipped rows, stamp their `ship_batch_id` inside the
    /// same transaction, then commit. The stamp makes crash recovery idempotent: if
    /// the process crashes between `ship` and `mark_shipped`, the next `claim`
    /// returns the same rows with `ship_batch_id` already set and `group_by_tenant`
    /// reuses that key — no re-derivation from the (now-shifted) live seq range.
    ///
    /// # Errors
    /// Returns [`BifrostError`] when the claim query or batch-id stamp fails.
    pub async fn claim(&self, limit: i32) -> Result<Vec<RelayShipment>, BifrostError> {
        let mut conn = TenantConn::acquire(&self.pool, DataTenantId::SYSTEM_OWNER)
            .await
            .map_err(BifrostError::Sql)?;
        let rows = vala_sql::queries::audit_outbox::claim_unshipped_audit(&mut conn, limit)
            .await
            .map_err(BifrostError::Sql)?;
        let shipments = group_by_tenant(rows)?;
        for shipment in &shipments {
            stamp_audit_ship_batch_id(
                &mut conn,
                shipment.tenant.as_uuid(),
                shipment.seq_lo,
                shipment.seq_hi,
                &shipment.batch_id,
            )
            .await
            .map_err(BifrostError::Sql)?;
        }
        conn.commit().await.map_err(BifrostError::Sql)?;
        Ok(shipments)
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
        let ctx = BifrostWriteContext {
            batch_id: shipment.batch_id,
            origin: AUDIT_RELAY_ORIGIN.to_owned(),
            actor: AUDIT_RELAY_ORIGIN.to_owned(),
            request_id: RequestId::now_v7(),
            card_ref: None,
        };
        // One shipment = one commit unit under `shipment.batch_id`; `commit_one`
        // closes the writer so the coordinator drains and commits immediately.
        writer.commit_one(shipment.tenant, vec![batch], ctx).await?;
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
/// per-tenant shipments.
///
/// If the first row of a group already has `ship_batch_id` set (crash-recovery
/// path), that persisted key is used as-is. Otherwise a fresh deterministic key
/// is derived from the tenant + seq range; the caller stamps it back to the DB
/// inside the claim transaction before commit.
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
            let batch_id = match rows[0].ship_batch_id.as_deref() {
                Some(existing) => existing.try_into().map_err(|_| {
                    BifrostError::Internal(format!(
                        "relay: persisted ship_batch_id for tenant {tenant} has wrong length"
                    ))
                })?,
                None => derive_batch_id(tenant, seq_lo, seq_hi),
            };
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

/// Derive a 16-byte idempotency key for a fresh (not yet stamped) shipment.
///
/// Used only when the claimed rows carry no `ship_batch_id` yet. The derived key
/// is immediately stamped back to the DB inside the claim transaction so it is
/// durable before the relay begins shipping.
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
        Field::new("detail", DataType::Utf8, true),
        Field::new("created_at_us", DataType::Int64, false),
    ]
}

/// Build the Arrow batch of the 16 audit content columns for one tenant's
/// shipment. `CorrelationPolicy::None` — no `run_id`/`card_ref` appended (C-01).
/// The writer stamps the 4 Bifrost system columns at flush.
fn build_audit_log_batch(rows: &[AuditOutboxRow]) -> Result<RecordBatch, BifrostError> {
    let seq = Int64Array::from(rows.iter().map(|r| r.seq).collect::<Vec<_>>());
    let entry_hash = StringArray::from(rows.iter().map(|r| hex(&r.entry_hash)).collect::<Vec<_>>());
    let prev_hash = StringArray::from(rows.iter().map(|r| hex(&r.prev_hash)).collect::<Vec<_>>());
    let request_id = StringArray::from(
        rows.iter()
            .map(|r| r.request_id.clone())
            .collect::<Vec<_>>(),
    );
    let trace_id = StringArray::from(rows.iter().map(|r| r.trace_id.clone()).collect::<Vec<_>>());
    let operation = StringArray::from(rows.iter().map(|r| r.operation.clone()).collect::<Vec<_>>());
    let resource = StringArray::from(rows.iter().map(|r| r.resource.clone()).collect::<Vec<_>>());
    let audit_card_ref =
        StringArray::from(rows.iter().map(|r| r.card_ref.clone()).collect::<Vec<_>>());
    let principal_id = StringArray::from(
        rows.iter()
            .map(|r| r.principal_id.to_string())
            .collect::<Vec<_>>(),
    );
    let principal_kind = StringArray::from(
        rows.iter()
            .map(|r| r.principal_kind.clone())
            .collect::<Vec<_>>(),
    );
    let auth_method = StringArray::from(
        rows.iter()
            .map(|r| r.auth_method.clone())
            .collect::<Vec<_>>(),
    );
    let permission = StringArray::from(
        rows.iter()
            .map(|r| r.permission.clone())
            .collect::<Vec<_>>(),
    );
    let decision = StringArray::from(rows.iter().map(|r| r.decision.clone()).collect::<Vec<_>>());
    let result = StringArray::from(rows.iter().map(|r| r.result.clone()).collect::<Vec<_>>());
    let payload_summary = StringArray::from(
        rows.iter()
            .map(|r| r.payload_summary.clone())
            .collect::<Vec<_>>(),
    );
    let detail = StringArray::from(rows.iter().map(|r| r.detail.clone()).collect::<Vec<_>>());
    let created_at_us = Int64Array::from(
        rows.iter()
            .map(|r| r.created_at.timestamp_micros())
            .collect::<Vec<_>>(),
    );

    let schema = Arc::new(Schema::new(audit_log_fields()));

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
        Arc::new(detail),
        Arc::new(created_at_us),
    ];

    RecordBatch::try_new(schema, columns).map_err(BifrostError::Arrow)
}

/// Lowercase hex-encode a byte slice (used for the chain-hash columns).
fn hex(bytes: &[u8]) -> String {
    ::hex::encode(bytes)
}

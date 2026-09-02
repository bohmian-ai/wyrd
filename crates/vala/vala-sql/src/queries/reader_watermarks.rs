//! Durable snapshots that live Bifrost readers still depend on.
//!
//! Forge maintenance and Bifrost readers never share a process, so a pinned
//! Oracle cut has to leave a durable trace before
//! destructive maintenance can honour it. Each reader node republishes the
//! oldest snapshot it still needs per table on its ordinary heartbeat, with an
//! expiry derived from that same heartbeat, so a reader that stops answering
//! stops protecting snapshots on exactly the schedule its role lease already
//! uses. Nothing here interprets the watermarks: corroborating one against
//! Iceberg ancestry belongs to the maintenance owner that also holds the table.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::row_types::forge_tasks::SnapshotWatermark;
use crate::row_types::reader_watermarks::ReaderWatermarkPublication;
use crate::{SqlError, TenantConn};

/// Tenant-scoped durable owner of live reader snapshot dependencies.
pub struct BifrostReaderWatermarks<'conn, 'tx> {
    /// Tenant-bound connection every statement runs inside.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> BifrostReaderWatermarks<'conn, 'tx> {
    /// Binds the durable owner to one tenant transaction.
    pub fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Republishes one reader node's complete set of pinned snapshots.
    ///
    /// The publication is the node's whole current dependency for the tables it
    /// names: a table absent from `publication` has its previous row for this
    /// node removed rather than left to expire, so releasing a cut takes effect
    /// at the next heartbeat instead of one lease TTL later. Rows for other
    /// nodes are never touched.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when a watermark is malformed or its
    /// expiry does not follow its publication time, and [`SqlError`] for any
    /// statement failure. The whole publication shares the caller's
    /// transaction, so a failure leaves the previous set intact.
    pub async fn publish(
        &mut self,
        node_id: Uuid,
        publication: &ReaderWatermarkPublication,
    ) -> Result<(), SqlError> {
        publication.validate()?;
        let (namespaces, tables, snapshot_ids, timestamps) = publication.columns();
        sqlx::query(
            r#"
            DELETE FROM vala.bifrost_reader_watermarks
             WHERE data_tenant_id = wyrd.current_tenant()
               AND node_id = $1
               AND (namespace, table_name) <> ALL (
                     SELECT unnest($2::text[]), unnest($3::text[])
                   )
            "#,
        )
        .bind(node_id)
        .bind(&namespaces)
        .bind(&tables)
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        sqlx::query(
            r#"
            INSERT INTO vala.bifrost_reader_watermarks (
                data_tenant_id, node_id, namespace, table_name,
                snapshot_id, snapshot_timestamp_ms, published_at, expires_at
            )
            SELECT wyrd.current_tenant(), $1, ns, tbl, sid, ts, $6, $7
              FROM unnest($2::text[], $3::text[], $4::bigint[], $5::bigint[])
                AS source(ns, tbl, sid, ts)
            ON CONFLICT (data_tenant_id, node_id, namespace, table_name)
            DO UPDATE SET
                snapshot_id = EXCLUDED.snapshot_id,
                snapshot_timestamp_ms = EXCLUDED.snapshot_timestamp_ms,
                published_at = EXCLUDED.published_at,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(node_id)
        .bind(&namespaces)
        .bind(&tables)
        .bind(&snapshot_ids)
        .bind(&timestamps)
        .bind(publication.published_at)
        .bind(publication.expires_at)
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Drops every watermark one reader node published.
    ///
    /// Used by a node that is shutting down cleanly, so its snapshots stop
    /// being protected immediately rather than for one more lease TTL.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the statement fails.
    pub async fn retire_node(&mut self, node_id: Uuid) -> Result<(), SqlError> {
        sqlx::query(
            r#"
            DELETE FROM vala.bifrost_reader_watermarks
             WHERE data_tenant_id = wyrd.current_tenant()
               AND node_id = $1
            "#,
        )
        .bind(node_id)
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Lists the unexpired snapshots live readers still need for one table.
    ///
    /// The bound is reported rather than silently applied: a truncated
    /// protected set is indistinguishable from a smaller one at the call site,
    /// and maintenance must fail closed on the difference.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when `cap` is zero,
    /// [`SqlError::InvariantViolation`] when a persisted watermark is
    /// malformed, and [`SqlError`] for any statement failure.
    pub async fn list_active(
        &mut self,
        namespace: &str,
        table_name: &str,
        now: DateTime<Utc>,
        cap: u32,
    ) -> Result<(Vec<SnapshotWatermark>, bool), SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "reader watermark cap must be positive".to_owned(),
            });
        }
        let rows = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT snapshot_id, snapshot_timestamp_ms
              FROM vala.bifrost_reader_watermarks
             WHERE data_tenant_id = wyrd.current_tenant()
               AND namespace = $1
               AND table_name = $2
               AND expires_at > $3
             ORDER BY snapshot_timestamp_ms, snapshot_id, node_id
             LIMIT $4
            "#,
        )
        .bind(namespace)
        .bind(table_name)
        .bind(now)
        .bind(i64::from(cap) + 1)
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let limit = usize::try_from(cap).map_err(|_| SqlError::Conflict {
            detail: "reader watermark cap overflow".to_owned(),
        })?;
        let overflowed = rows.len() > limit;
        let mut watermarks = Vec::with_capacity(rows.len().min(limit));
        for (snapshot_id, timestamp_ms) in rows.into_iter().take(limit) {
            let watermark = SnapshotWatermark {
                snapshot_id,
                timestamp_ms,
            };
            watermark
                .validate()
                .map_err(|_| SqlError::InvariantViolation {
                    detail: "invalid persisted reader watermark timestamp".to_owned(),
                })?;
            watermarks.push(watermark);
        }
        Ok((watermarks, overflowed))
    }
}

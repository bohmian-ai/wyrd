//! Tenant-scoped reads of the sealed `vala.file_list` manifest.

use std::collections::BTreeSet;
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::file_list::HotFileRow;

/// Owner for ordered hot-file manifest reads.
pub struct HotFileCatalog {
    /// Logical namespace whose sealed files this reader projects.
    namespace: String,
    /// Logical table name whose sealed files this reader projects.
    table_name: String,
}

/// Cut-aware scan membership and complete sealed lineage for one table.
pub struct HotFileCut {
    /// Rows that remain unresolved and must be scanned as hot Parquet.
    pub hot_files: Vec<HotFileRow>,
    /// Every sealed row retained for live-tail watermark derivation.
    pub sealed_manifest: Vec<HotFileRow>,
    /// Whether a legacy prepared row lacked safe publication evidence.
    pub ambiguous_publication: bool,
}

/// One hot object that is eligible for unchanged Iceberg promotion.
///
/// The projection is deliberately narrow: promotion appends the writer's own
/// `DataFile` unchanged, so it needs the durable identity, the canonical object
/// path, the checksum it revalidates against, and the writer's promotion
/// evidence — and nothing that would tempt a promoter to re-derive geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotableHotFileRow {
    /// Durable file-list identity carried into the promoted-file-set digest.
    pub id: uuid::Uuid,
    /// Canonical object-store path of the already-published hot object.
    pub file_path: String,
    /// Lowercase object checksum the promoter revalidates before appending.
    pub file_checksum: String,
    /// Encoded object size, carried so planning can report promoted volume.
    pub file_size: i64,
    /// The writer's `ScribePublishedHotFileV1` evidence for this exact object.
    pub promotion_record: serde_json::Value,
}

/// Durable settlement state of one exact planned hot object.
///
/// Returned only by [`HotFileCatalog::planned_settlement`], whose caller asks
/// about identities it planned rather than about the current promotable set.
/// `committed_snapshot_id` and `forge_publication_operation_id` are the two
/// halves of the catalog-to-SQL settlement window: both present means the
/// group provably landed in that snapshot under that operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedHotFileRow {
    /// Durable file-list identity this state describes.
    pub id: uuid::Uuid,
    /// Canonical object-store path of the planned hot object.
    pub file_path: String,
    /// Snapshot that settled this row, when a promotion already committed it.
    pub committed_snapshot_id: Option<i64>,
    /// Forge publication operation that settled this row, when one did.
    pub forge_publication_operation_id: Option<uuid::Uuid>,
    /// Whether the row has left the hot scan set.
    pub compacted: bool,
}

/// Classifies one row against the pinned publication cut.
fn is_unresolved_hot(
    row: &HotFileRow,
    pinned_paths: &BTreeSet<String>,
    pinned_operation: Option<uuid::Uuid>,
) -> (bool, bool) {
    let represented_by_path = pinned_paths.contains(&row.file_path);
    let represented_by_operation = row.compacted
        && pinned_operation.is_some()
        && row.forge_publication_operation_id == pinned_operation;
    let ambiguous = row.compacted
        && row.committed_snapshot_id.is_none()
        && row.forge_publication_operation_id.is_none()
        && !represented_by_path;
    (
        row.committed_snapshot_id.is_none() && !represented_by_path && !represented_by_operation,
        ambiguous,
    )
}

impl HotFileCatalog {
    /// Creates a manifest reader for one logical table identity.
    #[must_use]
    pub fn new(namespace: &str, table_name: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
            table_name: table_name.to_owned(),
        }
    }

    /// Reads tenant-scoped sealed files for exact pinned-snapshot subtraction.
    ///
    /// Compacted transition rows remain visible because their committed
    /// snapshot may be newer than Oracle's independently pinned snapshot.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the RLS-bound transaction or manifest query fails.
    pub async fn unresolved_for_cut(
        &self,
        conn: &mut TenantConn<'_>,
        pinned_paths: &BTreeSet<String>,
        pinned_operation: Option<uuid::Uuid>,
    ) -> Result<HotFileCut, SqlError> {
        let rows = sqlx::query_as::<_, HotFileRow>(
            "SELECT id, data_tenant_id, namespace, table_name, file_path, file_ordinal, file_checksum, file_size, row_count, min_event_time, max_event_time, partition_granularity, partition_start, compacted, committed_snapshot_id, forge_publication_operation_id, node_id, writer_epoch, wal_lsn_min, wal_lsn_max, created_at FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 ORDER BY partition_granularity, partition_start, created_at, file_ordinal, id",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(&self.namespace)
        .bind(&self.table_name)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let mut hot_files = Vec::new();
        let mut ambiguous_publication = false;
        for row in &rows {
            let (unresolved, ambiguous) = is_unresolved_hot(row, pinned_paths, pinned_operation);
            ambiguous_publication |= ambiguous;
            if unresolved {
                hot_files.push(row.clone());
            }
        }
        Ok(HotFileCut {
            hot_files,
            sealed_manifest: rows,
            ambiguous_publication,
        })
    }

    /// Reads the exact hot rows this table owes an unchanged Iceberg promotion.
    ///
    /// Eligibility is the never-published state: not yet compacted, no
    /// committed snapshot, and no Forge publication operation. Ordering is the
    /// durable production order (`created_at`, then the writer's file ordinal,
    /// then identity), so two schedulers observing the same manifest derive the
    /// same ordered group and therefore the same promoted-file-set digest.
    ///
    /// The read fails closed rather than dropping a row: a hot object with no
    /// durable checksum cannot be revalidated before it is appended unchanged,
    /// so its presence invalidates the whole group instead of silently
    /// shrinking it.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Query`] when the RLS-bound read fails and
    /// [`SqlError::InvariantViolation`] when an eligible row carries no
    /// durable object checksum.
    pub async fn list_promotable(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<Vec<PromotableHotFileRow>, SqlError> {
        let rows: Vec<(uuid::Uuid, String, Option<String>, i64, serde_json::Value)> =
            sqlx::query_as(
                r#"
            SELECT id, file_path, file_checksum, file_size, promotion_record
              FROM vala.file_list
             WHERE data_tenant_id = wyrd.current_tenant()
               AND namespace = $1
               AND table_name = $2
               AND NOT compacted
               AND committed_snapshot_id IS NULL
               AND forge_publication_operation_id IS NULL
             ORDER BY created_at, file_ordinal, id
            "#,
            )
            .bind(&self.namespace)
            .bind(&self.table_name)
            .fetch_all(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        rows.into_iter()
            .map(
                |(id, file_path, file_checksum, file_size, promotion_record)| {
                    let file_checksum =
                        file_checksum.ok_or_else(|| SqlError::InvariantViolation {
                            detail: format!(
                                "promotable hot object {file_path} carries no durable checksum"
                            ),
                        })?;
                    Ok(PromotableHotFileRow {
                        id,
                        file_path,
                        file_checksum,
                        file_size,
                        promotion_record,
                    })
                },
            )
            .collect()
    }

    /// Reads the durable settlement state of an exact set of planned hot rows.
    ///
    /// Promotion planning records the precise `file_list` identities one task
    /// will promote. Between planning and execution another task can commit the
    /// same group, which settles those rows and removes them from the
    /// promotable projection. A worker therefore needs the state of *its own*
    /// planned identities rather than whatever is promotable now: a row that is
    /// settled is evidence the group already landed, while a row that vanished
    /// without settlement is evidence of loss. Rows absent from the result no
    /// longer exist and are the caller's signal for the latter.
    ///
    /// The projection is intentionally the settlement triple plus identity; it
    /// carries no evidence a caller could mistake for a promotable group.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Query`] when the RLS-bound read fails.
    pub async fn planned_settlement(
        &self,
        conn: &mut TenantConn<'_>,
        file_ids: &[uuid::Uuid],
    ) -> Result<Vec<PlannedHotFileRow>, SqlError> {
        // raw-query grep allowlist: this tenant-scoped file-list read post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote it to a macro. It remains bound to `TenantConn` and `wyrd.current_tenant()` and introduces no tenant-boundary exception.
        let rows: Vec<(uuid::Uuid, String, Option<i64>, Option<uuid::Uuid>, bool)> =
            sqlx::query_as(
                r#"
            SELECT id, file_path, committed_snapshot_id, forge_publication_operation_id, compacted
              FROM vala.file_list
             WHERE data_tenant_id = wyrd.current_tenant()
               AND namespace = $1
               AND table_name = $2
               AND id = ANY($3)
             ORDER BY created_at, file_ordinal, id
            "#,
            )
            .bind(&self.namespace)
            .bind(&self.table_name)
            .bind(file_ids)
            .fetch_all(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    file_path,
                    committed_snapshot_id,
                    forge_publication_operation_id,
                    compacted,
                )| {
                    PlannedHotFileRow {
                        id,
                        file_path,
                        committed_snapshot_id,
                        forge_publication_operation_id,
                        compacted,
                    }
                },
            )
            .collect())
    }

    /// Records that one committed promotion snapshot now represents these rows.
    ///
    /// The update is the SQL half of the catalog-to-SQL window: Oracle stops
    /// scanning a row as hot only once it carries both the committed snapshot
    /// and the Forge publication operation that placed it there. Replaying the
    /// same settlement matches the already-settled rows and reports the same
    /// count, so a takeover that repeats a settled promotion neither
    /// double-writes nor reports a gap. Rows already settled by a *different*
    /// operation are not matched and are therefore excluded from the count.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Query`] when the RLS-bound update fails and
    /// [`SqlError::InvariantViolation`] when the affected-row count exceeds the
    /// durable identifier domain.
    pub async fn settle_promoted(
        &self,
        conn: &mut TenantConn<'_>,
        file_ids: &[uuid::Uuid],
        committed_snapshot_id: i64,
        operation_id: uuid::Uuid,
    ) -> Result<u64, SqlError> {
        sqlx::query(
            r#"
            UPDATE vala.file_list
               SET compacted = true,
                   committed_snapshot_id = $4,
                   forge_publication_operation_id = $5
             WHERE data_tenant_id = wyrd.current_tenant()
               AND namespace = $1
               AND table_name = $2
               AND id = ANY($3)
               AND (
                     (committed_snapshot_id IS NULL AND forge_publication_operation_id IS NULL)
                  OR (committed_snapshot_id = $4 AND forge_publication_operation_id = $5)
               )
            "#,
        )
        .bind(&self.namespace)
        .bind(&self.table_name)
        .bind(file_ids)
        .bind(committed_snapshot_id)
        .bind(operation_id)
        .execute(&mut **conn.transaction())
        .await
        .map(|done| done.rows_affected())
        .map_err(SqlError::from)
    }
}

// Additional Forge lifecycle reads share the same tenant-bound connection.

// raw-query grep allowlist: this tenant-scoped file-list query post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote it to a macro. It remains bound to `TenantConn` and `wyrd.current_tenant()` and introduces no tenant-boundary exception.

/// Lists file paths whose staging lifecycle is not terminal for one physical table.
///
/// The optional `path` narrows the same nonterminal predicate for focused
/// reference proofs; it never broadens tenant or table scope.
///
/// # Errors
///
/// Returns [`SqlError::Query`] when PostgreSQL cannot execute the tenant-scoped
/// read.
pub async fn list_nonterminal_file_paths(
    conn: &mut TenantConn<'_>,
    namespace: &str,
    table_name: &str,
    path: Option<&str>,
) -> Result<Vec<String>, SqlError> {
    sqlx::query_scalar(
        r#"
        SELECT file_path
          FROM vala.file_list
         WHERE data_tenant_id = wyrd.current_tenant()
           AND namespace = $1
           AND table_name = $2
           AND (NOT compacted OR committed_snapshot_id IS NULL)
           AND ($3::text IS NULL OR file_path = $3)
         ORDER BY file_path
        "#,
    )
    .bind(namespace)
    .bind(table_name)
    .bind(path)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Lists exact path/size inputs awaiting Forge planning for one demanded table.
///
/// The table-scoped read is not roster discovery: callers already hold one
/// durable planning demand and use this result only to construct its exact plan.
///
/// # Errors
/// Returns SQL errors or an invariant violation for a negative persisted size.
pub async fn list_nonterminal_files(
    conn: &mut TenantConn<'_>,
    namespace: &str,
    table_name: &str,
) -> Result<Vec<(String, u64)>, SqlError> {
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT file_path,file_size FROM vala.file_list WHERE data_tenant_id=wyrd.current_tenant() AND namespace=$1 AND table_name=$2 AND (NOT compacted OR committed_snapshot_id IS NULL) ORDER BY file_path")
        .bind(namespace).bind(table_name).fetch_all(&mut **conn.transaction()).await.map_err(SqlError::from)?;
    rows.into_iter()
        .map(|(path, size)| {
            u64::try_from(size).map(|value| (path, value)).map_err(|_| {
                SqlError::InvariantViolation {
                    detail: "Forge staging file size is negative".to_owned(),
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one valid manifest row for cut-membership policy tests.
    fn row(compacted: bool, committed: Option<i64>, operation: Option<uuid::Uuid>) -> HotFileRow {
        HotFileRow {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: uuid::Uuid::now_v7(),
            namespace: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
            file_path: "events/a.parquet".to_owned(),
            file_ordinal: 0,
            file_checksum: None,
            file_size: 1,
            row_count: 1,
            min_event_time: None,
            max_event_time: None,
            partition_granularity: "hour".to_owned(),
            partition_start: chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
                .expect("fixture partition start"),
            compacted,
            committed_snapshot_id: committed,
            forge_publication_operation_id: operation,
            node_id: uuid::Uuid::now_v7(),
            writer_epoch: 1,
            wal_lsn_min: 1,
            wal_lsn_max: 2,
            created_at: chrono::Utc::now(),
        }
    }

    /// Covers hot, path, operation, committed, reset, unrelated, and ambiguous states.
    #[test]
    fn hot_cut_excludes_only_snapshot_represented_rows() {
        let pinned = uuid::Uuid::from_u128(7);
        let unrelated = uuid::Uuid::from_u128(8);
        let empty = BTreeSet::new();
        assert_eq!(
            is_unresolved_hot(&row(false, None, None), &empty, Some(pinned)),
            (true, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, None, Some(unrelated)), &empty, Some(pinned)),
            (true, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, None, Some(pinned)), &empty, Some(pinned)),
            (false, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, Some(9), Some(pinned)), &empty, Some(pinned)),
            (false, false)
        );
        assert_eq!(
            is_unresolved_hot(&row(true, None, None), &empty, Some(pinned)),
            (true, true)
        );
        let mut path = BTreeSet::new();
        path.insert("events/a.parquet".to_owned());
        assert_eq!(
            is_unresolved_hot(&row(true, None, None), &path, Some(pinned)),
            (false, false)
        );
    }
}

#[cfg(test)]
mod pg_tests {
    //! Database projection proof for the already-durable hot event-time bounds.

    use std::collections::BTreeSet;

    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::TenantConn;

    use super::HotFileCatalog;

    /// The tenant-scoped unresolved-hot read projects the durable
    /// `min_event_time`/`max_event_time` interval and row count alongside the
    /// identity columns, so Oracle can exclude a non-overlapping hot file
    /// before it opens the object's footer.
    ///
    /// No migration is involved: both columns already exist on
    /// `vala.file_list`; only the query projection and row mapping change.
    ///
    /// # Panics
    ///
    /// Panics when the PostgreSQL fixture, insert, or tenant-scoped read fails.
    #[tokio::test]
    async fn unresolved_hot_cut_projects_bounds_without_migration() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let tenant = fixture.data_tenant_id();
        let lower = chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
            .expect("fixture lower bound is representable");
        let upper = chrono::DateTime::from_timestamp_micros(1_787_497_199_000_000)
            .expect("fixture upper bound is representable");
        sqlx::query(
            r#"
            INSERT INTO vala.file_list (
                id, data_tenant_id, namespace, table_name, file_path,
                file_size, row_count, min_event_time, max_event_time,
                partition_granularity, partition_start, node_id, writer_epoch,
                wal_lsn_min, wal_lsn_max, promotion_record
            ) VALUES (
                $1, $2, 'vala.traces', 'spans', 'spans/a.parquet',
                4096, 128, $3, $4,
                'hour', $5,
                $6, 1, 100, 200, '{"fixture": "hot-bounds"}'::jsonb
            )
            "#,
        )
        .bind(uuid::Uuid::now_v7())
        .bind(tenant.as_uuid())
        .bind(lower)
        .bind(upper)
        .bind(lower)
        .bind(uuid::Uuid::now_v7())
        .execute(&pool)
        .await
        .expect("insert unresolved hot row");

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let cut = HotFileCatalog::new("vala.traces", "spans")
            .unresolved_for_cut(&mut conn, &BTreeSet::new(), None)
            .await
            .expect("unresolved hot cut reads");

        assert_eq!(cut.hot_files.len(), 1, "one unresolved hot row is returned");
        let row = &cut.hot_files[0];
        assert_eq!(row.min_event_time, Some(lower));
        assert_eq!(row.max_event_time, Some(upper));
        assert_eq!(row.row_count, 128);
        assert!(row.row_count >= 0, "durable row count is nonnegative");
    }

    /// Promotion demand is exactly the hot rows that carry complete evidence and
    /// have never been published, in deterministic promotion order, and the
    /// settlement that follows a committed snapshot is idempotent.
    ///
    /// A row missing its object checksum cannot be promoted unchanged — the
    /// promoter would have no durable value to revalidate the object against —
    /// so the read fails closed rather than returning a partial group.
    ///
    /// # Panics
    ///
    /// Panics when the PostgreSQL fixture, inserts, reads, or settlement fail.
    #[tokio::test]
    async fn promotable_hot_files_are_exact_and_settlement_is_idempotent() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let tenant = fixture.data_tenant_id();
        let start = chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
            .expect("fixture partition start is representable");
        let mut ids = Vec::new();
        for (ordinal, checksum, compacted, snapshot, operation) in [
            (0_i16, "a".repeat(64), false, None, None),
            (1, "b".repeat(64), false, None, None),
            (
                2,
                "c".repeat(64),
                true,
                Some(41_i64),
                Some(uuid::Uuid::now_v7()),
            ),
            (3, "d".repeat(64), false, Some(42), None),
            (4, "e".repeat(64), false, None, Some(uuid::Uuid::now_v7())),
        ] {
            let id = uuid::Uuid::now_v7();
            ids.push(id);
            sqlx::query(
                r#"
                INSERT INTO vala.file_list (
                    id, data_tenant_id, namespace, table_name, file_path, file_ordinal,
                    file_checksum, file_size, row_count, min_event_time, max_event_time,
                    partition_granularity, partition_start, compacted, committed_snapshot_id,
                    forge_publication_operation_id, node_id, writer_epoch, wal_lsn_min,
                    wal_lsn_max, promotion_record
                ) VALUES (
                    $1, $2, 'vala.bifrost', 'events', $3, $4,
                    $5, 16, 4, $6, $6,
                    'hour', $6, $7, $8,
                    $9, $10, 1, $4, $4,
                    '{"version": 1}'::jsonb
                )
                "#,
            )
            .bind(id)
            .bind(tenant.as_uuid())
            .bind(format!("events/{ordinal}.parquet"))
            .bind(i64::from(ordinal))
            .bind(&checksum)
            .bind(start)
            .bind(compacted)
            .bind(snapshot)
            .bind(operation)
            .bind(uuid::Uuid::now_v7())
            .execute(&pool)
            .await
            .expect("insert manifest row");
        }

        let catalog = HotFileCatalog::new("vala.bifrost", "events");
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let promotable = catalog
            .list_promotable(&mut conn)
            .await
            .expect("promotion demand reads");
        assert_eq!(
            promotable.iter().map(|row| row.id).collect::<Vec<_>>(),
            ids[..2].to_vec(),
            "only never-published rows with complete evidence are promotable"
        );
        assert_eq!(promotable[0].file_path, "events/0.parquet");
        assert_eq!(promotable[0].file_checksum, "a".repeat(64));
        assert_eq!(
            promotable[0].promotion_record,
            serde_json::json!({"version": 1})
        );

        let operation = uuid::Uuid::now_v7();
        let settled = catalog
            .settle_promoted(&mut conn, &ids[..2], 77, operation)
            .await
            .expect("first settlement applies");
        assert_eq!(settled, 2, "both promoted rows settle exactly once");
        let replayed = catalog
            .settle_promoted(&mut conn, &ids[..2], 77, operation)
            .await
            .expect("replayed settlement is accepted");
        assert_eq!(replayed, 2, "replaying the same settlement is idempotent");
        assert!(
            catalog
                .list_promotable(&mut conn)
                .await
                .expect("post-settlement demand reads")
                .is_empty(),
            "settled rows leave no promotion demand"
        );
        // The promotion read claims its rows for the caller, so the settlement
        // transaction still holds them here. Committing before the fail-closed
        // setup below is what the production promoter does too, and without it
        // the out-of-band update would wait on this very transaction.
        conn.commit().await.expect("settlement commits");

        sqlx::query("UPDATE vala.file_list SET file_checksum = NULL WHERE id = $1")
            .bind(ids[0])
            .execute(&pool)
            .await
            .expect("clear the durable checksum");
        sqlx::query(
            "UPDATE vala.file_list SET compacted = false, committed_snapshot_id = NULL, forge_publication_operation_id = NULL WHERE id = $1",
        )
        .bind(ids[0])
        .execute(&pool)
        .await
        .expect("restore promotion eligibility");
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("second tenant connection");
        assert!(
            matches!(
                catalog.list_promotable(&mut conn).await,
                Err(crate::SqlError::InvariantViolation { .. })
            ),
            "a hot row without its durable checksum fails the read closed"
        );
    }
}

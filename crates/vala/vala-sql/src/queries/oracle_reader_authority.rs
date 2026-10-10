//! Durable statements behind Oracle cuts and Forge table maintenance.
//!
//! Oracle reads a query's cut with one read-only statement and records
//! nothing: readers are protected by Forge's snapshot retention window, not
//! tracked. `vala.bifrost_table_maintenance_authority` is the per-table row
//! destructive Forge work locks `FOR NO KEY UPDATE` as an
//! [`ExclusiveTableAuthority`], serializing Forge's own protocols on one table.
//! `vala.forge_snapshot_supersessions` records when Forge first observed each
//! retained snapshot as no longer current, which is what the retention window
//! is measured from.
//!
//! Every read fails closed. A missing row, an identity mismatch, or a
//! malformed result is contradictory evidence.

// raw-query grep allowlist: the Oracle cut and Forge supersession relations post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::DataTenantId;

use crate::row_types::file_list::HotFileRow;
use crate::row_types::oracle_reader_authority::TableAuthorityIdentity;
use crate::{SqlError, TenantConn};

/// Exact catalog name every reader-protection row must carry.
///
/// Duplicated as a durable expectation rather than imported from the Redux
/// crate because `vala-sql` sits below it; the schema check constraint, this
/// constant, and `catalog::BIFROST_CATALOG_NAME` are asserted equal by the
/// schema-contract test.
pub const BIFROST_CATALOG_NAME: &str = "wyrd-redux";

/// Constructs a fail-closed invariant error without leaking row payloads.
pub(crate) fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}

/// Decodes one stored 16-byte table UID.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] when the stored value is not
/// exactly 16 bytes, which the schema forbids and therefore indicates
/// corruption rather than a shorter identity.
fn table_uid(bytes: Vec<u8>) -> Result<[u8; 16], SqlError> {
    <[u8; 16]>::try_from(bytes.as_slice())
        .map_err(|_| invariant("stored Bifrost table UID is not 16 bytes"))
}

/// Live, exclusive maintenance authority over one tenant-qualified table.
///
/// Obtained only from [`BifrostTableMaintenanceAuthority::exclusive`] after
/// the table's authority row was locked. It mutably borrows the transaction
/// holding that lock, so the lock cannot be released while this value exists,
/// and every destructive Forge operation takes `&ExclusiveTableAuthority` as
/// its proof that no other Forge protocol is destroying on the same table.
/// Ending the transaction first ends this borrow.
///
/// The owner holds no destructive SQL of its own. Forge's preparation and
/// settlement transactions run on separate connections, so the caller drops
/// this value and ends the borrowed transaction once the external effect's
/// outcome is known and before settlement, which takes the row exclusively.
pub struct ExclusiveTableAuthority<'conn, 'tx> {
    /// Transaction holding the authority row `FOR NO KEY UPDATE`.
    conn: &'conn mut TenantConn<'tx>,
    /// Exact table the held row serializes.
    identity: TableAuthorityIdentity,
}

impl ExclusiveTableAuthority<'_, '_> {
    /// Returns the exact table identity the held authority covers.
    #[must_use]
    pub fn identity(&self) -> &TableAuthorityIdentity {
        &self.identity
    }

    /// Returns the tenant the held authority transaction is bound to.
    #[must_use]
    pub fn tenant(&self) -> DataTenantId {
        self.conn.data_tenant_id()
    }

    /// Fails closed unless this authority covers exactly `tenant` and the
    /// table named by `table_uid`, `catalog_name`, `namespace_name`, and
    /// `table_name`.
    ///
    /// Destructive preparation calls this with its request's identity, so a
    /// capability taken for one table can never authorize work on another.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when any component differs.
    pub fn require_covers(
        &self,
        tenant: DataTenantId,
        table_uid: &[u8; 16],
        catalog_name: &str,
        namespace_name: &str,
        table_name: &str,
    ) -> Result<(), SqlError> {
        if self.tenant() == tenant
            && self.identity.tenant == tenant
            && &self.identity.table_uid == table_uid
            && self.identity.catalog_name == catalog_name
            && self.identity.namespace_name == namespace_name
            && self.identity.table_name == table_name
        {
            return Ok(());
        }
        Err(SqlError::Conflict {
            detail: "Forge maintenance names a table its exclusive authority does not cover"
                .to_owned(),
        })
    }
}

/// The single SQL serialization boundary for one tenant-qualified table.
///
/// Destructive Forge preparation locks this one row exclusively, so Forge's
/// destructive protocols on one table run one at a time without a global
/// physical lock. Oracle never takes it.
pub struct BifrostTableMaintenanceAuthority<'conn, 'tx> {
    /// Tenant-bound connection every statement runs inside.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> BifrostTableMaintenanceAuthority<'conn, 'tx> {
    /// Binds the serialization owner to one tenant transaction.
    pub fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Inserts the authority row for one newly registered Bifrost table.
    ///
    /// Registration writes this row inside its own transaction so the row
    /// exists before any cut or maintenance owner can want it. That is what
    /// lets every consumer treat a missing row as an identity failure instead
    /// of falling back to an advisory lock.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity names a
    /// foreign catalog or a blank name segment, and [`SqlError`] when the
    /// statement fails, including the foreign key to `vala.bifrost_tables`.
    pub async fn register(&mut self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        sqlx::query(
            r"
            INSERT INTO vala.bifrost_table_maintenance_authority
                (data_tenant_id, catalog_name, namespace_name, table_name, table_uid)
            VALUES (wyrd.current_tenant(), $1, $2, $3, $4)
            ON CONFLICT (data_tenant_id, catalog_name, namespace_name, table_name)
            DO UPDATE SET table_uid = EXCLUDED.table_uid
            ",
        )
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .bind(identity.table_uid.as_slice())
        .execute(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Takes the table's exclusive maintenance authority and turns this owner
    /// into the capability destructive Forge work requires.
    ///
    /// The row is locked `FOR NO KEY UPDATE`, the mode that conflicts with
    /// another Forge holder but not with the `FOR KEY SHARE` a claim row's
    /// foreign key takes, so a separate Forge preparation transaction can
    /// still record claims while this one holds the row. No Oracle read is
    /// consulted: snapshot retention protects readers.
    ///
    /// The lock lives for the rest of the borrowed transaction, which the
    /// returned capability keeps mutably borrowed: the caller cannot commit or
    /// roll it back — and so cannot surrender the authority — while the
    /// capability, and therefore any destructive call taking it, is alive.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed,
    /// names a table with no authority row, names a different table UID, or
    /// names a tenant other than the borrowed connection's; and [`SqlError`]
    /// when the lock statement fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation leaves the lock with the borrowed transaction, which
    /// releases it when it commits, rolls back, or is dropped.
    pub async fn exclusive(
        self,
        identity: TableAuthorityIdentity,
    ) -> Result<ExclusiveTableAuthority<'conn, 'tx>, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        if identity.tenant != self.conn.data_tenant_id() {
            return Err(invariant(
                "exclusive table authority names a tenant other than its connection's",
            ));
        }
        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            r"
            SELECT table_uid
              FROM vala.bifrost_table_maintenance_authority
             WHERE data_tenant_id = wyrd.current_tenant()
               AND catalog_name = $1
               AND namespace_name = $2
               AND table_name = $3
               FOR NO KEY UPDATE
            ",
        )
        .bind(&identity.catalog_name)
        .bind(&identity.namespace_name)
        .bind(&identity.table_name)
        .fetch_optional(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let stored = stored.ok_or_else(|| {
            invariant("Bifrost table has no maintenance authority row to serialize on")
        })?;
        if table_uid(stored)? != identity.table_uid {
            return Err(invariant(
                "Bifrost table maintenance authority names a different registered table UID",
            ));
        }
        Ok(ExclusiveTableAuthority {
            conn: self.conn,
            identity,
        })
    }

    /// Returns every snapshot of this table an unresolved Forge expiration has
    /// already claimed, in ascending order.
    ///
    /// A snapshot another prepared expiration owns is removed from a new
    /// selection rather than prepared twice; preparation re-checks the claim
    /// index under [`Self::exclusive`]. The owner never mutates a claim; only the
    /// fenced Forge lifecycle does.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed
    /// and [`SqlError`] when the claim-index read fails.
    pub async fn claimed_snapshots(
        &mut self,
        identity: &TableAuthorityIdentity,
    ) -> Result<Vec<i64>, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        sqlx::query_scalar(
            r"
            SELECT snapshot_id
              FROM vala.forge_snapshot_expiration_claims
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
             ORDER BY snapshot_id
            ",
        )
        .bind(identity.table_uid.as_slice())
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)
    }

    /// Records this table's replaced snapshots and returns those still inside
    /// the retention window, in ascending order.
    ///
    /// `replaced` is every snapshot the table retains that is not current.
    /// One statement records the PostgreSQL time of each first observation,
    /// forgets observations of snapshots no longer retained, and returns every
    /// replaced snapshot first observed less than `retention` ago, including
    /// the ones observed now unless `retention` is zero. Snapshot expiry must
    /// not select any of them.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed
    /// or `retention` exceeds `i64::MAX` milliseconds, and [`SqlError`] when
    /// the statement fails.
    pub async fn retained_supersessions(
        &mut self,
        identity: &TableAuthorityIdentity,
        replaced: &[i64],
        retention: Duration,
    ) -> Result<Vec<i64>, SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        let retention_ms = i64::try_from(retention.as_millis())
            .map_err(|_| invariant("snapshot retention exceeds i64 milliseconds"))?;
        sqlx::query_scalar(
            r"
            WITH forgotten AS (
                DELETE FROM vala.forge_snapshot_supersessions
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND table_uid = $1
                   AND snapshot_id <> ALL ($2)
            ), observed AS (
                INSERT INTO vala.forge_snapshot_supersessions
                    (data_tenant_id, table_uid, snapshot_id, observed_at)
                SELECT wyrd.current_tenant(), $1, replaced, statement_timestamp()
                  FROM unnest($2::bigint[]) AS replaced
                ON CONFLICT DO NOTHING
                RETURNING snapshot_id, observed_at
            )
            SELECT snapshot_id
              FROM observed
             WHERE observed_at > statement_timestamp() - $3 * interval '1 millisecond'
            UNION
            SELECT snapshot_id
              FROM vala.forge_snapshot_supersessions
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
               AND snapshot_id = ANY ($2)
               AND observed_at > statement_timestamp() - $3 * interval '1 millisecond'
             ORDER BY snapshot_id
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(replaced)
        .bind(retention_ms)
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(SqlError::from)
    }
}

/// One logical table a query asks to read, as the planner canonicalized it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutTableRef<'a> {
    /// Canonical logical namespace, `vala.<segment>`.
    pub namespace_name: &'a str,
    /// Local table name inside that namespace.
    pub table_name: &'a str,
}

/// One table of a read cut: its registered identity, its catalog pointer, and
/// its unresolved hot candidates.
#[derive(Debug, Clone)]
pub struct TableCut {
    /// Registered table identity the cut resolved.
    pub identity: TableAuthorityIdentity,
    /// Current Iceberg metadata document location for the table.
    pub metadata_location: String,
    /// Unresolved `file_list` rows in durable file-list order. These are
    /// candidates; reconciliation against the selected snapshot happens after
    /// the metadata document is read.
    pub hot_files: Vec<HotFileRow>,
}

/// Owner of one tenant's Oracle cut reads.
///
/// A cut is one read-only SQL statement that resolves every requested table,
/// reads each catalog pointer, and returns the unresolved hot candidates. It
/// takes no lock and writes no row, so nothing is released afterwards. The
/// statement binds its tenant for RLS itself, so it runs as one autocommit
/// round trip on a plain connection, with no transaction around it.
pub struct OracleTableCuts<'conn> {
    /// Connection the cut statement runs on.
    conn: &'conn mut sqlx::PgConnection,
    /// Tenant the statement binds, scoping every row it reads.
    data_tenant_id: DataTenantId,
}

impl<'conn> OracleTableCuts<'conn> {
    /// Binds the owner to one connection and the tenant it reads for.
    ///
    /// `conn` must not be inside a transaction bound to another tenant; the
    /// statement refuses that rather than rebinding.
    pub fn new(conn: &'conn mut sqlx::PgConnection, data_tenant_id: DataTenantId) -> Self {
        Self {
            conn,
            data_tenant_id,
        }
    }

    /// Reads a complete cut in one statement.
    ///
    /// `tables` is the planner's ordered, canonical table set; a repeated table
    /// keeps its first position and yields one result. The result has exactly
    /// one entry per distinct table, in input order.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::NoRows`] when any table has no registration visible
    /// to this tenant or no catalog pointer. Returns
    /// [`SqlError::InvariantViolation`] for an empty request or a result that
    /// does not contain exactly one well-formed identity and pointer per
    /// requested table. Returns [`SqlError`] when the statement fails.
    pub async fn read(&mut self, tables: &[CutTableRef<'_>]) -> Result<Vec<TableCut>, SqlError> {
        if tables.is_empty() {
            return Err(invariant("an Oracle cut needs at least one table"));
        }
        let request = serde_json::Value::Array(
            tables
                .iter()
                .map(|table| {
                    serde_json::json!({
                        "namespace": table.namespace_name,
                        "table": table.table_name,
                    })
                })
                .collect(),
        );
        let rows: Vec<CutDbRow> =
            sqlx::query_as("SELECT * FROM vala.oracle_read_table_cut($1::uuid, $2::jsonb)")
                .bind(self.data_tenant_id.as_uuid())
                .bind(request)
                .fetch_all(&mut *self.conn)
                .await
                .map_err(cut_error)?;
        group_table_cut(self.data_tenant_id, tables, rows)
    }
}

/// Maps the cut function's not-found signal onto the typed absence.
///
/// The function raises `no_data_found` (SQLSTATE `P0002`) for a missing
/// registration or catalog pointer; callers already treat
/// [`SqlError::NoRows`] as the catalog's table-not-found.
fn cut_error(error: sqlx::Error) -> SqlError {
    match &error {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("P0002") => SqlError::NoRows,
        _ => SqlError::from(error),
    }
}

/// One flat result row of `vala.oracle_read_table_cut`.
///
/// Every hot column comes from a left join, so each is nullable here; an
/// all-null hot side means the table has no unresolved candidates.
#[derive(sqlx::FromRow)]
struct CutDbRow {
    /// First input position of the table this row belongs to.
    ordinal: i32,
    /// Registered table UID.
    table_uid: Vec<u8>,
    /// Registered logical namespace.
    namespace_name: String,
    /// Registered table name.
    table_name: String,
    /// Catalog pointer of the table's current metadata.
    metadata_location: String,
    /// Hot row identity; `None` exactly when the hot side is empty.
    id: Option<Uuid>,
    /// Hot row tenant.
    data_tenant_id: Option<Uuid>,
    /// Hot object path.
    file_path: Option<String>,
    /// Hot object ordinal within its generation.
    file_ordinal: Option<i16>,
    /// Hot object checksum.
    file_checksum: Option<String>,
    /// Hot object size.
    file_size: Option<i64>,
    /// Hot object row count.
    row_count: Option<i64>,
    /// Hot object lower event-time bound.
    min_event_time: Option<DateTime<Utc>>,
    /// Hot object upper event-time bound.
    max_event_time: Option<DateTime<Utc>>,
    /// Hot object partition granularity.
    partition_granularity: Option<String>,
    /// Hot object partition start.
    partition_start: Option<DateTime<Utc>>,
    /// Whether Forge already published the hot object.
    compacted: Option<bool>,
    /// Snapshot that committed the hot object; always `None` for a candidate.
    committed_snapshot_id: Option<i64>,
    /// Forge operation that published the hot object.
    forge_publication_operation_id: Option<Uuid>,
    /// Producing Scribe node.
    node_id: Option<Uuid>,
    /// Producing writer epoch.
    writer_epoch: Option<i64>,
    /// Inclusive WAL lower bound.
    wal_lsn_min: Option<i64>,
    /// Inclusive WAL upper bound.
    wal_lsn_max: Option<i64>,
    /// Manifest insertion time.
    created_at: Option<DateTime<Utc>>,
}

impl CutDbRow {
    /// Splits the hot side off one flat row.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the hot side is partially
    /// null, carries a negative size, or is not an unresolved candidate.
    fn hot_file(&mut self) -> Result<Option<HotFileRow>, SqlError> {
        let Some(id) = self.id else {
            let empty = self.data_tenant_id.is_none()
                && self.file_path.is_none()
                && self.file_ordinal.is_none()
                && self.file_checksum.is_none()
                && self.file_size.is_none()
                && self.row_count.is_none()
                && self.min_event_time.is_none()
                && self.max_event_time.is_none()
                && self.partition_granularity.is_none()
                && self.partition_start.is_none()
                && self.compacted.is_none()
                && self.committed_snapshot_id.is_none()
                && self.forge_publication_operation_id.is_none()
                && self.node_id.is_none()
                && self.writer_epoch.is_none()
                && self.wal_lsn_min.is_none()
                && self.wal_lsn_max.is_none()
                && self.created_at.is_none();
            return if empty {
                Ok(None)
            } else {
                Err(invariant("table cut returned a partially null hot row"))
            };
        };
        let partial = || invariant("table cut returned a partially null hot row");
        let row = HotFileRow {
            id,
            data_tenant_id: self.data_tenant_id.ok_or_else(partial)?,
            namespace: self.namespace_name.clone(),
            table_name: self.table_name.clone(),
            file_path: self.file_path.take().ok_or_else(partial)?,
            file_ordinal: self.file_ordinal.ok_or_else(partial)?,
            file_checksum: self.file_checksum.take(),
            file_size: self.file_size.ok_or_else(partial)?,
            row_count: self.row_count.ok_or_else(partial)?,
            min_event_time: self.min_event_time,
            max_event_time: self.max_event_time,
            partition_granularity: self.partition_granularity.take().ok_or_else(partial)?,
            partition_start: self.partition_start.ok_or_else(partial)?,
            compacted: self.compacted.ok_or_else(partial)?,
            committed_snapshot_id: self.committed_snapshot_id,
            forge_publication_operation_id: self.forge_publication_operation_id,
            node_id: self.node_id.ok_or_else(partial)?,
            writer_epoch: self.writer_epoch.ok_or_else(partial)?,
            wal_lsn_min: self.wal_lsn_min.ok_or_else(partial)?,
            wal_lsn_max: self.wal_lsn_max.ok_or_else(partial)?,
            created_at: self.created_at.ok_or_else(partial)?,
        };
        if row.file_size < 0 || row.row_count < 0 || row.committed_snapshot_id.is_some() {
            return Err(invariant(
                "table cut returned a hot candidate with impossible metadata",
            ));
        }
        Ok(Some(row))
    }
}

/// Groups the flat cut result into one validated cut per table.
///
/// The grouping is complete or fails: every distinct requested table must
/// appear exactly once with one stable identity and pointer, and no
/// unrequested table may appear.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for a missing, duplicate,
/// conflicting, unrequested, or malformed table result or hot row.
fn group_table_cut(
    tenant: DataTenantId,
    tables: &[CutTableRef<'_>],
    rows: Vec<CutDbRow>,
) -> Result<Vec<TableCut>, SqlError> {
    let mut expected: Vec<(i32, CutTableRef<'_>)> = Vec::with_capacity(tables.len());
    for (position, table) in tables.iter().enumerate() {
        if !expected.iter().any(|(_, seen)| seen == table) {
            let ordinal = i32::try_from(position + 1)
                .map_err(|_| invariant("Oracle cut request is too large"))?;
            expected.push((ordinal, *table));
        }
    }
    let mut cuts: Vec<(i32, TableCut)> = Vec::with_capacity(expected.len());
    for mut row in rows {
        let hot = row.hot_file()?;
        if let Some((ordinal, cut)) = cuts.last_mut()
            && *ordinal == row.ordinal
        {
            if cut.identity.table_uid.as_slice() != row.table_uid.as_slice()
                || cut.identity.namespace_name != row.namespace_name
                || cut.identity.table_name != row.table_name
                || cut.metadata_location != row.metadata_location
            {
                return Err(invariant("table cut returned conflicting table identity"));
            }
            match hot {
                Some(hot) if !cut.hot_files.is_empty() => cut.hot_files.push(hot),
                _ => return Err(invariant("table cut mixed empty and hot rows")),
            }
            continue;
        }
        let Some((_, requested)) = expected.get(cuts.len()) else {
            return Err(invariant("table cut returned an unrequested table"));
        };
        if expected[cuts.len()].0 != row.ordinal
            || requested.namespace_name != row.namespace_name
            || requested.table_name != row.table_name
        {
            return Err(invariant("table cut returned tables out of request order"));
        }
        let identity = TableAuthorityIdentity {
            tenant,
            table_uid: table_uid(std::mem::take(&mut row.table_uid))?,
            catalog_name: BIFROST_CATALOG_NAME.to_owned(),
            namespace_name: row.namespace_name,
            table_name: row.table_name,
        };
        identity.validate(BIFROST_CATALOG_NAME)?;
        cuts.push((
            row.ordinal,
            TableCut {
                identity,
                metadata_location: row.metadata_location,
                hot_files: hot.into_iter().collect(),
            },
        ));
    }
    if cuts.len() != expected.len() {
        return Err(invariant("table cut is missing a requested table"));
    }
    Ok(cuts.into_iter().map(|(_, cut)| cut).collect())
}

//! Durable statements behind Oracle reader authority.
//!
//! Two owners share one serialization row per tenant-qualified table,
//! `vala.bifrost_table_maintenance_authority`. Oracle cut acquisition takes it
//! `FOR SHARE` inside the one statement that also records the query's active
//! table reads; destructive Forge preparation takes it `FOR UPDATE` and then
//! refuses while any active read exists. That gives the read-versus-destroy
//! race exactly one durable winner per table.
//!
//! Every read fails closed. A missing row, an identity mismatch, or a
//! malformed result is contradictory evidence and never degrades into
//! "nothing is being read".

// raw-query grep allowlist: the Oracle reader-authority relations post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

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

/// The single SQL serialization boundary for one tenant-qualified table.
///
/// Cut acquisition share-locks this one row and destructive Forge preparation
/// locks it exclusively, which is what gives the admission-versus-destruction
/// race exactly one durable winner per table without taking a global physical
/// lock.
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
    /// exists before any reader or maintenance owner can want it. That is what
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

    /// Takes the exact table's serialization row `FOR UPDATE`.
    ///
    /// The lock is held for the remainder of the caller's transaction, so every
    /// competing cut acquisition or destructive decision for this table waits
    /// here rather than racing on the active-read rows themselves.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the identity is malformed,
    /// when no row exists for the exact catalog/namespace/table, or when the
    /// stored table UID differs from the caller's — all of which are identity
    /// failures that must fail closed rather than proceed unserialized.
    pub async fn lock(&mut self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
        identity.validate(BIFROST_CATALOG_NAME)?;
        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            r"
            SELECT table_uid
              FROM vala.bifrost_table_maintenance_authority
             WHERE data_tenant_id = wyrd.current_tenant()
               AND catalog_name = $1
               AND namespace_name = $2
               AND table_name = $3
               FOR UPDATE
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
        Ok(())
    }
}

/// One logical table a query asks to read, as the planner canonicalized it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveTableRef<'a> {
    /// Canonical logical namespace, `vala.<segment>`.
    pub namespace_name: &'a str,
    /// Local table name inside that namespace.
    pub table_name: &'a str,
}

/// The exact Oracle role fence that owns one query's active table reads.
///
/// Forge treats a row as abandonable only once this exact pair is no longer a
/// live Oracle membership row, so a node ID without its token is not enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveReadOwner {
    /// Durable query identity Oracle already uses for the request.
    pub query_id: Uuid,
    /// Oracle node that admitted the query.
    pub node_id: Uuid,
    /// That node's current Oracle role fencing token.
    pub fencing_token: i64,
}

/// One table of an acquired cut: its registered identity, the catalog pointer
/// read under the table's authority lock, and its unresolved hot candidates.
#[derive(Debug, Clone)]
pub struct AcquiredTableCut {
    /// Registered table identity the active read was recorded against.
    pub identity: TableAuthorityIdentity,
    /// Current Iceberg metadata document location for the table.
    pub metadata_location: String,
    /// Unresolved `file_list` rows in durable file-list order. These are
    /// candidates; reconciliation against the selected snapshot happens after
    /// the metadata document is read.
    pub hot_files: Vec<HotFileRow>,
}

/// Owner of one tenant's active Oracle table reads.
///
/// Acquisition is one SQL statement that locks every requested table's
/// maintenance authority, reads each catalog pointer and the unresolved hot
/// candidates, and records one active read per query/table before returning
/// any of it. The caller owns the transaction and must commit it before any
/// metadata, manifest, or data-file IO.
pub struct OracleActiveTableReads<'conn, 'tx> {
    /// Tenant-bound connection whose RLS scopes every statement.
    conn: &'conn mut TenantConn<'tx>,
}

impl<'conn, 'tx> OracleActiveTableReads<'conn, 'tx> {
    /// Binds the owner to one tenant transaction.
    pub fn new(conn: &'conn mut TenantConn<'tx>) -> Self {
        Self { conn }
    }

    /// Acquires a complete cut and its active reads in one statement.
    ///
    /// `tables` is the planner's ordered, canonical table set; a repeated table
    /// keeps its first position and yields one result and one active read. The
    /// result has exactly one entry per distinct table, in input order. A
    /// replayed acquisition for the same query refreshes the existing rows to
    /// the caller's fence and a new PostgreSQL abandonment time.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::NoRows`] when any table has no registration visible
    /// to this tenant or no catalog pointer; no active read commits in that
    /// case. Returns [`SqlError::InvariantViolation`] for an empty request, a
    /// non-positive fence, or a result that does not contain exactly one
    /// well-formed identity and pointer per requested table. Returns
    /// [`SqlError`] when the statement fails.
    ///
    /// # Cancellation
    ///
    /// Dropping the future leaves the statement's effects in the caller's
    /// uncommitted transaction, which rolls back with it.
    pub async fn acquire(
        &mut self,
        owner: ActiveReadOwner,
        tables: &[ActiveTableRef<'_>],
    ) -> Result<Vec<AcquiredTableCut>, SqlError> {
        if tables.is_empty() || owner.fencing_token <= 0 {
            return Err(invariant(
                "active table read acquisition needs tables and a positive fence",
            ));
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
        let rows: Vec<AcquiredCutDbRow> = sqlx::query_as(
            "SELECT * FROM vala.oracle_acquire_table_cut($1, $2, $3, $4::jsonb)",
        )
        .bind(owner.query_id)
        .bind(owner.node_id)
        .bind(owner.fencing_token)
        .bind(request)
        .fetch_all(&mut **self.conn.transaction())
        .await
        .map_err(acquisition_error)?;
        group_acquired_cut(self.conn.data_tenant_id(), tables, rows)
    }

    /// Deletes every active read this query holds and returns how many it
    /// removed.
    ///
    /// Release is idempotent: a second call, or a call after Forge discarded
    /// an abandoned row, removes nothing and succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the statement fails.
    pub async fn release(&mut self, query_id: Uuid) -> Result<u64, SqlError> {
        sqlx::query("DELETE FROM vala.oracle_active_table_reads WHERE query_id = $1")
            .bind(query_id)
            .execute(&mut **self.conn.transaction())
            .await
            .map(|done| done.rows_affected())
            .map_err(SqlError::from)
    }
}

/// Maps the acquisition function's not-found signal onto the typed absence.
///
/// The function raises `no_data_found` (SQLSTATE `P0002`) for a missing
/// registration or catalog pointer so no partial claim set can commit; callers
/// already treat [`SqlError::NoRows`] as the catalog's table-not-found.
fn acquisition_error(error: sqlx::Error) -> SqlError {
    match &error {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("P0002") => SqlError::NoRows,
        _ => SqlError::from(error),
    }
}

/// One flat result row of `vala.oracle_acquire_table_cut`.
///
/// Every hot column comes from a left join, so each is nullable here; an
/// all-null hot side means the table has no unresolved candidates.
#[derive(sqlx::FromRow)]
struct AcquiredCutDbRow {
    /// First input position of the table this row belongs to.
    ordinal: i32,
    /// Registered table UID.
    table_uid: Vec<u8>,
    /// Registered logical namespace.
    namespace_name: String,
    /// Registered table name.
    table_name: String,
    /// Catalog pointer read under the authority lock.
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

impl AcquiredCutDbRow {
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
                Err(invariant("acquired cut returned a partially null hot row"))
            };
        };
        let partial = || invariant("acquired cut returned a partially null hot row");
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
                "acquired cut returned a hot candidate with impossible metadata",
            ));
        }
        Ok(Some(row))
    }
}

/// Groups the flat acquisition result into one validated cut per table.
///
/// The grouping is complete or fails: every distinct requested table must
/// appear exactly once with one stable identity and pointer, and no
/// unrequested table may appear.
///
/// # Errors
///
/// Returns [`SqlError::InvariantViolation`] for a missing, duplicate,
/// conflicting, unrequested, or malformed table result or hot row.
fn group_acquired_cut(
    tenant: DataTenantId,
    tables: &[ActiveTableRef<'_>],
    rows: Vec<AcquiredCutDbRow>,
) -> Result<Vec<AcquiredTableCut>, SqlError> {
    let mut expected: Vec<(i32, ActiveTableRef<'_>)> = Vec::with_capacity(tables.len());
    for (position, table) in tables.iter().enumerate() {
        if !expected.iter().any(|(_, seen)| seen == table) {
            let ordinal = i32::try_from(position + 1)
                .map_err(|_| invariant("active table read request is too large"))?;
            expected.push((ordinal, *table));
        }
    }
    let mut cuts: Vec<(i32, AcquiredTableCut)> = Vec::with_capacity(expected.len());
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
                return Err(invariant("acquired cut returned conflicting table identity"));
            }
            match hot {
                Some(hot) if !cut.hot_files.is_empty() => cut.hot_files.push(hot),
                _ => return Err(invariant("acquired cut mixed empty and hot rows")),
            }
            continue;
        }
        let Some((_, requested)) = expected.get(cuts.len()) else {
            return Err(invariant("acquired cut returned an unrequested table"));
        };
        if expected[cuts.len()].0 != row.ordinal
            || requested.namespace_name != row.namespace_name
            || requested.table_name != row.table_name
        {
            return Err(invariant("acquired cut returned tables out of request order"));
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
            AcquiredTableCut {
                identity,
                metadata_location: row.metadata_location,
                hot_files: hot.into_iter().collect(),
            },
        ));
    }
    if cuts.len() != expected.len() {
        return Err(invariant("acquired cut is missing a requested table"));
    }
    Ok(cuts.into_iter().map(|(_, cut)| cut).collect())
}

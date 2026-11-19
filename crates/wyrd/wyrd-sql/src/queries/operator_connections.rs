//! Operator connection persistence: redacted authority plus envelope ciphertext.
//!
//! `wyrd.operator_connections` stores one row per tenant (provider, name). The
//! nonsecret authority is the redacted read shape; the secret is stored only as
//! the [`SealedSecret`] stored-secret envelope the server's facade produced.
//! Every tenant function runs on the caller's [`TenantConn`] under forced RLS
//! and never commits. Writes are fenced on the exact row they were prepared
//! from, so the server seals with no row lock held. Rewrap discovery is the
//! one explicit [`OperatorPool`] read, and it returns only tenant ids.
// raw-query grep allowlist: operator connection tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};

use chrono::{DateTime, Utc};
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::{
    OperatorConnectionConfig, OperatorConnectionStatus, OperatorConnectionView, OperatorProvider,
};

use crate::{OperatorPool, TenantConn};

/// Every column a connection read returns, as a literal for `concat!`.
macro_rules! columns {
    () => {
        "connection_id, provider, name, config, status, secret_ciphertext, secret_version, \
         created_at, updated_at"
    };
}

/// Insert one connection unless its (tenant, provider, name) exists.
const INSERT_SQL: &str = concat!(
    "INSERT INTO wyrd.operator_connections (
         connection_id, data_tenant_id, provider, name, config, status,
         secret_ciphertext, secret_version, sub_principal_id, act_principal_id,
         updated_by, created_at, updated_at
     ) VALUES ($1, wyrd.current_tenant(), $2, $3, $4, 'active', $5, $6, $7, $8, $7,
               statement_timestamp(), statement_timestamp())
     ON CONFLICT (data_tenant_id, provider, name) DO NOTHING
     RETURNING ",
    columns!()
);

/// List the tenant's connections.
const LIST_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.operator_connections ORDER BY provider, name, connection_id"
);

/// Read one connection by identity.
const GET_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.operator_connections WHERE connection_id = $1"
);

/// Read one connection by provider and name.
const FIND_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.operator_connections WHERE provider = $1 AND name = $2"
);

/// Apply one update to the exact row it was prepared from; `NULL` secret
/// parameters keep the stored secret. No row is returned when the stored
/// authority, status, secret version, or envelope changed since that read.
const UPDATE_SQL: &str = concat!(
    "UPDATE wyrd.operator_connections
        SET config = $2, status = $3,
            secret_ciphertext = COALESCE($4, secret_ciphertext),
            secret_version = COALESCE($5, secret_version),
            updated_by = $6, updated_at = statement_timestamp()
      WHERE connection_id = $1
        AND config = $7 AND status = $8
        AND secret_version = $9 AND secret_ciphertext = $10
      RETURNING ",
    columns!()
);

/// The stored-secret envelope of one secret version; never plaintext.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret {
    /// The envelope bytes exactly as stored.
    pub envelope: Vec<u8>,
}

impl Debug for SealedSecret {
    /// Redacting debug: prints only the envelope length.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedSecret")
            .field("len", &self.envelope.len())
            .finish()
    }
}

/// One stored connection: its redacted view and its sealed secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConnection {
    /// The redacted read shape.
    pub view: OperatorConnectionView,
    /// The sealed current secret version.
    pub sealed: SealedSecret,
    /// Current secret version; part of the authenticated context.
    pub secret_version: i32,
}

/// A connection to insert.
#[derive(Debug)]
pub struct NewConnection<'a> {
    /// Server-minted identity.
    pub connection_id: OperatorConnectionId,
    /// Immutable name.
    pub name: &'a ConnectionName,
    /// Provider and nonsecret authority.
    pub config: &'a OperatorConnectionConfig,
    /// Sealed first secret version.
    pub sealed: &'a SealedSecret,
    /// First secret version; the sealed context used it.
    pub secret_version: i32,
    /// Verified subject and actor creating the connection; later updates
    /// keep it and record only `updated_by`.
    pub attribution: Attribution,
}

/// A connection update to apply.
#[derive(Debug)]
pub struct ConnectionChange<'a> {
    /// Authority to store.
    pub config: &'a OperatorConnectionConfig,
    /// Status to store.
    pub status: OperatorConnectionStatus,
    /// Replacement secret and its new secret version, when rotated.
    pub secret: Option<(&'a SealedSecret, i32)>,
    /// Updating principal.
    pub principal: PrincipalId,
}

/// Insert a connection; `Ok(None)` when (tenant, provider, name) already exists.
///
/// # Errors
/// Returns the database error when the insert fails, or a decode error when
/// the returned row is malformed.
pub async fn insert_connection(
    conn: &mut TenantConn<'_>,
    new: &NewConnection<'_>,
) -> Result<Option<StoredConnection>, SqlxError> {
    let row: Option<ConnectionRow> = sqlx::query_as(INSERT_SQL)
        .bind(new.connection_id.as_uuid())
        .bind(<&'static str>::from(new.config.provider()))
        .bind(new.name.as_str())
        .bind(Json(new.config))
        .bind(&new.sealed.envelope)
        .bind(new.secret_version)
        .bind(new.attribution.sub_principal_id.as_uuid())
        .bind(new.attribution.act_principal_id.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(ConnectionRow::into_stored).transpose()
}

/// List the tenant's connections in name order.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error.
pub async fn list_connections(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<StoredConnection>, SqlxError> {
    let rows: Vec<ConnectionRow> = sqlx::query_as(LIST_SQL)
        .fetch_all(&mut **conn.transaction())
        .await?;
    rows.into_iter().map(ConnectionRow::into_stored).collect()
}

/// Read one connection by identity.
///
/// An unknown or other-tenant identity is `Ok(None)`.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error.
pub async fn get_connection(
    conn: &mut TenantConn<'_>,
    connection_id: OperatorConnectionId,
) -> Result<Option<StoredConnection>, SqlxError> {
    let row: Option<ConnectionRow> = sqlx::query_as(GET_SQL)
        .bind(connection_id.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(ConnectionRow::into_stored).transpose()
}

/// Read one connection by its tenant-unique (provider, name).
///
/// Registration uses only the view; delivery also opens the sealed secret.
/// Missing and other-tenant connections are both `Ok(None)`.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error.
pub async fn find_connection(
    conn: &mut TenantConn<'_>,
    provider: OperatorProvider,
    name: &ConnectionName,
) -> Result<Option<StoredConnection>, SqlxError> {
    let row: Option<ConnectionRow> = sqlx::query_as(FIND_SQL)
        .bind(<&'static str>::from(provider))
        .bind(name.as_str())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(ConnectionRow::into_stored).transpose()
}

/// Apply one update to the connection exactly as `observed` was read.
///
/// A rotated secret replaces the envelope and secret version together;
/// otherwise they are untouched. `Ok(None)` means the row's authority,
/// status, secret version, or envelope changed after `observed` was read: a
/// concurrent write won and this one changed nothing.
///
/// # Errors
/// Returns the database error when the update fails, or a decode error.
pub async fn update_connection(
    conn: &mut TenantConn<'_>,
    observed: &StoredConnection,
    change: &ConnectionChange<'_>,
) -> Result<Option<StoredConnection>, SqlxError> {
    let row: Option<ConnectionRow> = sqlx::query_as(UPDATE_SQL)
        .bind(observed.view.connection_id.as_uuid())
        .bind(Json(change.config))
        .bind(<&'static str>::from(change.status))
        .bind(change.secret.map(|(sealed, _)| sealed.envelope.as_slice()))
        .bind(change.secret.map(|(_, version)| version))
        .bind(change.principal.as_uuid())
        .bind(Json(&observed.view.config))
        .bind(<&'static str>::from(observed.view.status))
        .bind(observed.secret_version)
        .bind(&observed.sealed.envelope)
        .fetch_optional(&mut **conn.transaction())
        .await?;
    row.map(ConnectionRow::into_stored).transpose()
}

/// Replace one connection's envelope with its rewrap.
///
/// Fenced on the secret version and exact envelope it was read with, so a
/// concurrent secret rotation or another replica's rewrap is never
/// overwritten. Returns whether this write won.
///
/// # Errors
/// Returns the database error when the update fails.
pub async fn rewrap_connection(
    conn: &mut TenantConn<'_>,
    observed: &StoredConnection,
    envelope: &[u8],
) -> Result<bool, SqlxError> {
    let updated = sqlx::query(
        "UPDATE wyrd.operator_connections
            SET secret_ciphertext = $4
          WHERE connection_id = $1 AND secret_version = $2 AND secret_ciphertext = $3",
    )
    .bind(observed.view.connection_id.as_uuid())
    .bind(observed.secret_version)
    .bind(&observed.sealed.envelope)
    .bind(envelope)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Every tenant holding at least one connection, across tenants.
///
/// Rewrap visits each of them through its own tenant session.
///
/// # Errors
/// Returns the database error when the read fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn tenants_with_connections(
    operator: &OperatorPool,
) -> Result<Vec<DataTenantId>, SqlxError> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT DISTINCT data_tenant_id FROM wyrd.operator_connections ORDER BY data_tenant_id",
    )
    .fetch_all(operator.pool())
    .await?;
    rows.into_iter()
        .map(|(tenant,)| stored(DataTenantId::new(tenant)))
        .collect()
}

/// One `wyrd.operator_connections` row.
#[derive(sqlx::FromRow)]
struct ConnectionRow {
    /// Identity.
    connection_id: Uuid,
    /// Stored provider name.
    provider: String,
    /// Stored name.
    name: String,
    /// Stored authority.
    config: Json<OperatorConnectionConfig>,
    /// Stored status name.
    status: String,
    /// Secret envelope.
    secret_ciphertext: Vec<u8>,
    /// Secret version.
    secret_version: i32,
    /// Creation time.
    created_at: DateTime<Utc>,
    /// Update time.
    updated_at: DateTime<Utc>,
}

impl ConnectionRow {
    /// Decode the row into typed values.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] for a malformed identity, name, status,
    /// or a provider that disagrees with the stored authority.
    fn into_stored(self) -> Result<StoredConnection, SqlxError> {
        let provider: OperatorProvider = stored(self.provider.parse())?;
        let Json(config) = self.config;
        if config.provider() != provider {
            return Err(SqlxError::Decode(
                "operator connection provider disagrees with its config".into(),
            ));
        }
        Ok(StoredConnection {
            view: OperatorConnectionView {
                connection_id: stored(OperatorConnectionId::new(self.connection_id))?,
                name: stored(ConnectionName::new(self.name))?,
                config,
                status: stored(self.status.parse())?,
                created_at: self.created_at,
                updated_at: self.updated_at,
            },
            sealed: SealedSecret {
                envelope: self.secret_ciphertext,
            },
            secret_version: self.secret_version,
        })
    }
}

/// Map a stored-value parse failure to a decode error.
fn stored<T, E>(value: Result<T, E>) -> Result<T, SqlxError>
where
    E: StdError + Send + Sync + 'static,
{
    value.map_err(|error| SqlxError::Decode(Box::new(error)))
}

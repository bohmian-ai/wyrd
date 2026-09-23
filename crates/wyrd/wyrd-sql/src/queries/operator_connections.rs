//! Operator connection persistence: redacted authority plus envelope ciphertext.
//!
//! `wyrd.operator_connections` stores one row per tenant (provider, name). The
//! nonsecret authority is the redacted read shape; the secret is stored only as
//! [`SealedSecret`] bytes produced by `wyrd-crypt` in the server. Every tenant
//! function runs on the caller's [`TenantConn`] under forced RLS and never
//! commits, so a handler composes the write with its audit append. Key-rotation
//! discovery is the one explicit [`OperatorPool`] read, limited by column
//! grants to tenant and key version.
// raw-query grep allowlist: operator connection tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::error::Error as StdError;

use chrono::{DateTime, Utc};
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::{
    OperatorConnectionConfig, OperatorConnectionStatus, OperatorConnectionView, OperatorProvider,
};

use crate::{OperatorPool, TenantConn};

/// Every column a connection read returns, as a literal for `concat!`.
macro_rules! columns {
    () => {
        "connection_id, provider, name, config, status, secret_ciphertext, secret_nonce, \
         wrapped_dek, dek_nonce, key_version, secret_version, created_at, updated_at"
    };
}

/// Insert one connection unless its (tenant, provider, name) exists.
const INSERT_SQL: &str = concat!(
    "INSERT INTO wyrd.operator_connections (
         connection_id, data_tenant_id, provider, name, config, status,
         secret_ciphertext, secret_nonce, wrapped_dek, dek_nonce, key_version,
         secret_version, created_by, updated_by, created_at, updated_at
     ) VALUES ($1, wyrd.current_tenant(), $2, $3, $4, 'active', $5, $6, $7, $8, $9,
               $10, $11, $11, statement_timestamp(), statement_timestamp())
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

/// Read and lock one connection by identity.
const GET_FOR_UPDATE_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.operator_connections WHERE connection_id = $1 FOR UPDATE"
);

/// Read one connection by provider and name.
const FIND_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.operator_connections WHERE provider = $1 AND name = $2"
);

/// Apply one update; `NULL` secret parameters keep the stored secret.
const UPDATE_SQL: &str = concat!(
    "UPDATE wyrd.operator_connections
        SET config = $2, status = $3,
            secret_ciphertext = COALESCE($4, secret_ciphertext),
            secret_nonce = COALESCE($5, secret_nonce),
            wrapped_dek = COALESCE($6, wrapped_dek),
            dek_nonce = COALESCE($7, dek_nonce),
            key_version = COALESCE($8, key_version),
            secret_version = COALESCE($9, secret_version),
            updated_by = $10, updated_at = statement_timestamp()
      WHERE connection_id = $1
      RETURNING ",
    columns!()
);

/// Lock rows wrapped under a non-active key version.
const STALE_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.operator_connections
      WHERE key_version <> $1
      ORDER BY connection_id
      LIMIT $2
        FOR UPDATE SKIP LOCKED"
);

/// Envelope ciphertext of one secret version; never plaintext.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret {
    /// The secret encrypted under its data key.
    pub ciphertext: Vec<u8>,
    /// Nonce of `ciphertext`.
    pub nonce: [u8; 12],
    /// The data key encrypted under the tenant key-encryption key.
    pub wrapped_dek: Vec<u8>,
    /// Nonce of `wrapped_dek`.
    pub dek_nonce: [u8; 12],
    /// Version of the tenant key-encryption key that wrapped the data key.
    pub key_version: i32,
}

impl std::fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedSecret")
            .field("key_version", &self.key_version)
            .finish_non_exhaustive()
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
    /// Creating principal.
    pub principal: PrincipalId,
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
        .bind(&new.sealed.ciphertext)
        .bind(new.sealed.nonce.as_slice())
        .bind(&new.sealed.wrapped_dek)
        .bind(new.sealed.dek_nonce.as_slice())
        .bind(new.sealed.key_version)
        .bind(new.secret_version)
        .bind(new.principal.as_uuid())
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

/// Read one connection by identity, locking it when `for_update`.
///
/// An unknown or other-tenant identity is `Ok(None)`.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error.
pub async fn get_connection(
    conn: &mut TenantConn<'_>,
    connection_id: OperatorConnectionId,
    for_update: bool,
) -> Result<Option<StoredConnection>, SqlxError> {
    let sql = if for_update {
        GET_FOR_UPDATE_SQL
    } else {
        GET_SQL
    };
    let row: Option<ConnectionRow> = sqlx::query_as(sql)
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

/// Apply one update to a connection previously read `for_update`.
///
/// A rotated secret replaces the ciphertext, wrapped key, key version, and
/// secret version together; otherwise they are untouched.
///
/// # Errors
/// Returns the database error when the update fails or matches no row, or a
/// decode error.
pub async fn update_connection(
    conn: &mut TenantConn<'_>,
    connection_id: OperatorConnectionId,
    change: &ConnectionChange<'_>,
) -> Result<StoredConnection, SqlxError> {
    let sealed = change.secret.map(|(sealed, _)| sealed);
    let row: ConnectionRow = sqlx::query_as(UPDATE_SQL)
        .bind(connection_id.as_uuid())
        .bind(Json(change.config))
        .bind(<&'static str>::from(change.status))
        .bind(sealed.map(|s| s.ciphertext.as_slice()))
        .bind(sealed.map(|s| s.nonce.as_slice()))
        .bind(sealed.map(|s| s.wrapped_dek.as_slice()))
        .bind(sealed.map(|s| s.dek_nonce.as_slice()))
        .bind(sealed.map(|s| s.key_version))
        .bind(change.secret.map(|(_, version)| version))
        .bind(change.principal.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await?;
    row.into_stored()
}

/// Lock up to `limit` of the tenant's connections not wrapped under `active`.
///
/// Skips rows another rewrap holds, so concurrent replicas share the work.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error.
pub async fn stale_key_connections(
    conn: &mut TenantConn<'_>,
    active: i32,
    limit: i64,
) -> Result<Vec<StoredConnection>, SqlxError> {
    let rows: Vec<ConnectionRow> = sqlx::query_as(STALE_SQL)
        .bind(active)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await?;
    rows.into_iter().map(ConnectionRow::into_stored).collect()
}

/// Replace one locked row's wrapped data key and key version.
///
/// Fenced on the secret version it was read with, so a concurrent secret
/// rotation is never overwritten with a stale wrapped key.
///
/// # Errors
/// Returns the database error when the update fails.
pub async fn rewrap_connection(
    conn: &mut TenantConn<'_>,
    connection_id: OperatorConnectionId,
    secret_version: i32,
    wrapped_dek: &[u8],
    dek_nonce: &[u8; 12],
    key_version: i32,
) -> Result<bool, SqlxError> {
    let updated = sqlx::query(
        "UPDATE wyrd.operator_connections
            SET wrapped_dek = $3, dek_nonce = $4, key_version = $5
          WHERE connection_id = $1 AND secret_version = $2",
    )
    .bind(connection_id.as_uuid())
    .bind(secret_version)
    .bind(wrapped_dek)
    .bind(dek_nonce.as_slice())
    .bind(key_version)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Every (tenant, key version) pair still referenced, across tenants.
///
/// Rotation rewraps tenants whose rows reference a non-active version, and an
/// operator retires an external key version only once it is absent here.
///
/// # Errors
/// Returns the database error when the read fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn referenced_key_versions(
    operator: &OperatorPool,
) -> Result<Vec<(DataTenantId, i32)>, SqlxError> {
    let rows: Vec<(Uuid, i32)> = sqlx::query_as(
        "SELECT DISTINCT data_tenant_id, key_version
           FROM wyrd.operator_connections
          ORDER BY data_tenant_id, key_version",
    )
    .fetch_all(operator.pool())
    .await?;
    rows.into_iter()
        .map(|(tenant, version)| Ok((stored(DataTenantId::new(tenant))?, version)))
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
    /// Secret ciphertext.
    secret_ciphertext: Vec<u8>,
    /// Secret nonce.
    secret_nonce: Vec<u8>,
    /// Wrapped data key.
    wrapped_dek: Vec<u8>,
    /// Wrapped data key nonce.
    dek_nonce: Vec<u8>,
    /// Key-encryption key version.
    key_version: i32,
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
    /// nonce, or a provider that disagrees with the stored authority.
    fn into_stored(self) -> Result<StoredConnection, SqlxError> {
        let provider: OperatorProvider = stored(self.provider.parse())?;
        let Json(config) = self.config;
        if config.provider() != provider {
            return Err(SqlxError::Decode(
                "operator connection provider disagrees with its config".into(),
            ));
        }
        let nonce = |bytes: Vec<u8>| -> Result<[u8; 12], SqlxError> {
            bytes
                .try_into()
                .map_err(|_| SqlxError::Decode("operator connection nonce is not 12 bytes".into()))
        };
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
                ciphertext: self.secret_ciphertext,
                nonce: nonce(self.secret_nonce)?,
                wrapped_dek: self.wrapped_dek,
                dek_nonce: nonce(self.dek_nonce)?,
                key_version: self.key_version,
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

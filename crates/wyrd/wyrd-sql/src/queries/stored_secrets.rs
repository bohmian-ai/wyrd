//! Cross-store slots for deployment wrapping-key maintenance.
//!
//! Every recoverable stored secret lives in one owner's column as the common
//! envelope. Wrapping-key maintenance is a deployment operation over every
//! tenant's and the platform's envelopes, so these are the only slots that
//! read and replace those columns across tenants, on the cross-tenant
//! [`OperatorPool`]. Each replacement is a compare-and-swap on the exact
//! bytes read, so a concurrent replacement, revocation, or renewal is never
//! overwritten and no other column is touched.
// raw-query grep allowlist: these statements post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError};

/// One stored envelope with the trusted owner metadata its context binds.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredSecretRow {
    /// Owning tenant; `None` for the platform connection.
    pub data_tenant_id: Option<Uuid>,
    /// Row key as text: the connection id, the trusted issuer URL, the
    /// gateway credential name, or the platform issuer URL.
    pub row_key: String,
    /// Owner identity the envelope context binds: an issuer URL, a gateway
    /// credential name, or an Operator connection id.
    pub owner: String,
    /// Authority the envelope context binds, in order: the client id,
    /// client-authentication method, and (human connections) connection id;
    /// the gateway provider; or the Operator provider and name.
    pub authority: Vec<String>,
    /// Secret version the context binds, for owners that version secrets.
    pub version: Option<i64>,
    /// The envelope exactly as stored.
    pub envelope: Vec<u8>,
}

/// A column that holds stored-secret envelopes, as maintenance walks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredSecretTable {
    /// `wyrd.auth_human_connections.client_secret_enc`, keyed by connection id.
    HumanConnections,
    /// `wyrd.auth_trusted_issuers.client_secret_enc`, keyed by issuer URL.
    TrustedIssuers,
    /// `platform.oidc_connection.client_secret_enc`, keyed by issuer URL.
    PlatformOidc,
    /// `wyrd.gateway_provider_credentials.secret_ciphertext`, the managed
    /// provider keys, keyed by credential name.
    GatewayProviderCredentials,
    /// `wyrd.operator_connections.secret_ciphertext`, keyed by connection id.
    OperatorConnections,
}

impl StoredSecretTable {
    /// Every column maintenance walks, in a stable order.
    pub const ALL: [Self; 5] = [
        Self::HumanConnections,
        Self::TrustedIssuers,
        Self::PlatformOidc,
        Self::GatewayProviderCredentials,
        Self::OperatorConnections,
    ];

    /// The operator statement listing every stored (non-null) envelope in
    /// this column with no lifecycle or expiry filter, so a pass and the
    /// keyless boot check cover all ciphertext the column holds, including
    /// disabled and revoked owners'.
    const fn select_sql(self) -> &'static str {
        match self {
            Self::HumanConnections => {
                "SELECT data_tenant_id, connection_id::text AS row_key, issuer_url AS owner,
                        ARRAY[client_id, client_auth, connection_id::text] AS authority,
                        NULL::bigint AS version, client_secret_enc AS envelope
                   FROM wyrd.auth_human_connections WHERE client_secret_enc IS NOT NULL
                  ORDER BY data_tenant_id, connection_id"
            }
            Self::TrustedIssuers => {
                "SELECT data_tenant_id, issuer_url AS row_key, issuer_url AS owner,
                        ARRAY[client_id, client_auth] AS authority,
                        NULL::bigint AS version, client_secret_enc AS envelope
                   FROM wyrd.auth_trusted_issuers WHERE client_secret_enc IS NOT NULL
                  ORDER BY data_tenant_id, issuer_url"
            }
            Self::PlatformOidc => {
                "SELECT NULL::uuid AS data_tenant_id, issuer_url AS row_key, issuer_url AS owner,
                        ARRAY[client_id, client_auth] AS authority,
                        NULL::bigint AS version, client_secret_enc AS envelope
                   FROM platform.oidc_connection WHERE client_secret_enc IS NOT NULL"
            }
            Self::GatewayProviderCredentials => {
                "SELECT data_tenant_id, name AS row_key, name AS owner,
                        ARRAY[provider] AS authority,
                        NULL::bigint AS version, secret_ciphertext AS envelope
                   FROM wyrd.gateway_provider_credentials WHERE secret_ciphertext IS NOT NULL
                  ORDER BY data_tenant_id, name"
            }
            Self::OperatorConnections => {
                "SELECT data_tenant_id, connection_id::text AS row_key,
                        connection_id::text AS owner, ARRAY[provider, name] AS authority,
                        secret_version::bigint AS version, secret_ciphertext AS envelope
                   FROM wyrd.operator_connections
                  ORDER BY data_tenant_id, connection_id"
            }
        }
    }

    /// The operator compare-and-swap statement replacing one envelope; it
    /// matches only while the column still holds exactly the bytes read.
    const fn swap_sql(self) -> &'static str {
        match self {
            Self::HumanConnections => {
                "UPDATE wyrd.auth_human_connections SET client_secret_enc = $4
                  WHERE data_tenant_id = $1 AND connection_id::text = $2
                    AND client_secret_enc = $3"
            }
            Self::TrustedIssuers => {
                "UPDATE wyrd.auth_trusted_issuers SET client_secret_enc = $4
                  WHERE data_tenant_id = $1 AND issuer_url = $2 AND client_secret_enc = $3"
            }
            Self::PlatformOidc => {
                "UPDATE platform.oidc_connection SET client_secret_enc = $4
                  WHERE $1::uuid IS NULL AND issuer_url = $2 AND client_secret_enc = $3"
            }
            Self::GatewayProviderCredentials => {
                "UPDATE wyrd.gateway_provider_credentials SET secret_ciphertext = $4
                  WHERE data_tenant_id = $1 AND name = $2 AND secret_ciphertext = $3"
            }
            Self::OperatorConnections => {
                "UPDATE wyrd.operator_connections SET secret_ciphertext = $4
                  WHERE data_tenant_id = $1 AND connection_id::text = $2
                    AND secret_ciphertext = $3"
            }
        }
    }
}

/// List every stored envelope of `table` across all tenants, with the owner
/// metadata its context binds.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn stored_secrets(
    operator: &OperatorPool,
    table: StoredSecretTable,
) -> Result<Vec<StoredSecretRow>, SqlError> {
    sqlx::query_as::<_, StoredSecretRow>(table.select_sql())
        .fetch_all(operator.pool())
        .await
        .map_err(SqlError::from)
}

/// Replace `row`'s envelope with `replacement` if the column still holds
/// exactly the bytes `row` was read with.
///
/// Returns `false` when the row changed or disappeared since it was read; the
/// caller leaves it for the next pass and the concurrent write stands.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
// tenant-isolation: cross-tenant OperatorPool
pub async fn swap_stored_secret(
    operator: &OperatorPool,
    table: StoredSecretTable,
    row: &StoredSecretRow,
    replacement: &[u8],
) -> Result<bool, SqlError> {
    sqlx::query(table.swap_sql())
        .bind(row.data_tenant_id)
        .bind(&row.row_key)
        .bind(&row.envelope)
        .bind(replacement)
        .execute(operator.pool())
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(SqlError::from)
}

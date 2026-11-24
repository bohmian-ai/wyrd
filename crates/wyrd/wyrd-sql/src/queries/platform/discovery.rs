//! Workspace discovery: the HMAC directory of verified contacts and the spent
//! discovery proofs.
//!
//! The directory maps the keyed pseudonym of a canonical verified address to
//! the tenant and User whose contact it is, at the contact revision it was
//! projected from. It is a projection, never authority: every read joins only
//! live tenants, and the caller revalidates each candidate through the
//! tenant's own connection before disclosing it. Every timestamp is
//! PostgreSQL's.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use uuid::Uuid;

use crate::{OperatorPool, SqlError};

/// One directory candidate for an address, in a live tenant.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DirectoryCandidate {
    /// The tenant.
    pub data_tenant_id: Uuid,
    /// The tenant's route key.
    pub slug: String,
    /// The tenant's display name.
    pub display_name: String,
    /// The User whose contact the entry projects.
    pub user_id: Uuid,
    /// The contact revision it was projected from.
    pub contact_revision: i64,
}

/// Project `user_id`'s contact in `data_tenant_id` as `address_key` at
/// `revision`, unless an entry from a later revision is already held.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails, including a foreign-key
/// violation for an unknown tenant.
pub async fn project_contact(
    pool: &OperatorPool,
    data_tenant_id: Uuid,
    user_id: Uuid,
    address_key: &[u8; 32],
    revision: i64,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.workspace_directory
                (data_tenant_id, user_id, address_key, contact_revision)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (data_tenant_id, user_id) DO UPDATE
            SET address_key = EXCLUDED.address_key,
                contact_revision = EXCLUDED.contact_revision
          WHERE platform.workspace_directory.contact_revision < EXCLUDED.contact_revision",
    )
    .bind(data_tenant_id)
    .bind(user_id)
    .bind(address_key.as_slice())
    .bind(revision)
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// Remove `user_id`'s entry in `data_tenant_id`, whose contact is gone.
///
/// # Errors
/// Returns [`SqlError::Query`] when the delete fails.
pub async fn unproject_contact(
    pool: &OperatorPool,
    data_tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), SqlError> {
    sqlx::query(
        "DELETE FROM platform.workspace_directory WHERE data_tenant_id = $1 AND user_id = $2",
    )
    .bind(data_tenant_id)
    .bind(user_id)
    .execute(pool.pool())
    .await
    .map(|_| ())
    .map_err(SqlError::from)
}

/// The Users `data_tenant_id` holds directory entries for.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn projected_users(
    pool: &OperatorPool,
    data_tenant_id: Uuid,
) -> Result<Vec<Uuid>, SqlError> {
    sqlx::query_scalar("SELECT user_id FROM platform.workspace_directory WHERE data_tenant_id = $1")
        .bind(data_tenant_id)
        .fetch_all(pool.pool())
        .await
        .map_err(SqlError::from)
}

/// At most `limit` entries for `address_key` in active tenants whose route
/// key sorts after `after`, ordered by route key.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn directory_candidates(
    pool: &OperatorPool,
    address_key: &[u8; 32],
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<DirectoryCandidate>, SqlError> {
    sqlx::query_as(
        "SELECT d.data_tenant_id, t.slug, t.display_name, d.user_id, d.contact_revision
           FROM platform.workspace_directory d
           JOIN platform.tenants t ON t.data_tenant_id = d.data_tenant_id
          WHERE d.address_key = $1
            AND t.status = 'active' AND t.deleted_at IS NULL
            AND ($2::text IS NULL OR t.slug > $2)
          ORDER BY t.slug, d.user_id
          LIMIT $3",
    )
    .bind(address_key.as_slice())
    .bind(after)
    .bind(limit)
    .fetch_all(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Spend the discovery proof `proof_hash` for the browser holding
/// `binding_hash`, retaining it until `expires_in` from now, and return the
/// Unix-second deadline of the read it grants: `read_for` from the first
/// spend. The same browser repeating the spend gets the original deadline
/// while it is still ahead; any other repeat gets `None`.
///
/// # Errors
/// Returns [`SqlError::Query`] when the write fails.
pub async fn spend_discovery_proof(
    pool: &OperatorPool,
    proof_hash: &[u8; 32],
    binding_hash: &[u8; 32],
    read_for: Duration,
    expires_in: Duration,
) -> Result<Option<i64>, SqlError> {
    sqlx::query_scalar(
        "INSERT INTO platform.discovery_proofs
                (proof_hash, browser_binding_hash, read_until, expires_at)
         VALUES ($1, $2,
                 statement_timestamp() + ($3 * interval '1 second'),
                 statement_timestamp() + ($4 * interval '1 second'))
         ON CONFLICT (proof_hash) DO UPDATE
            SET proof_hash = EXCLUDED.proof_hash
          WHERE platform.discovery_proofs.browser_binding_hash = EXCLUDED.browser_binding_hash
            AND platform.discovery_proofs.read_until > statement_timestamp()
         RETURNING extract(epoch FROM read_until)::bigint",
    )
    .bind(proof_hash.as_slice())
    .bind(binding_hash.as_slice())
    .bind(read_for.as_secs_f64())
    .bind(expires_in.as_secs_f64())
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

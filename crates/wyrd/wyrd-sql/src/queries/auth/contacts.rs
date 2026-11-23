//! Tenant-local verified recovery contacts: one verified address per User,
//! distinct from its unverified profile or invitation email.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use uuid::Uuid;

use crate::TenantConn;

/// A User's verified recovery contact.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VerifiedContact {
    /// The address as it was typed, which mail is delivered to.
    pub address: String,
    /// Advances with every replacement; one for the first contact.
    pub revision: i64,
}

/// `user_id`'s verified contact, locked until the transaction ends; `None`
/// when it has none.
///
/// # Errors
/// Returns a SQLx error when the read fails.
pub async fn verified_contact_for_update(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
) -> Result<Option<VerifiedContact>, sqlx::Error> {
    sqlx::query_as(
        "SELECT address, revision FROM wyrd.auth_user_contacts
          WHERE user_id = $1
          FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Make `address`, just proven, `user_id`'s verified contact, only while
/// its revision is still `expected` (zero for none). Returns the new
/// revision, or `None` when another verification moved it first: two
/// first contacts racing insert one, and the other updates nothing.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the write.
pub async fn replace_verified_contact(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    address: &str,
    expected: i64,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO wyrd.auth_user_contacts (data_tenant_id, user_id, address)
         VALUES ($1, $2, $3)
         ON CONFLICT (data_tenant_id, user_id) DO UPDATE
            SET address = EXCLUDED.address,
                verified_at = statement_timestamp(),
                revision = wyrd.auth_user_contacts.revision + 1
          WHERE wyrd.auth_user_contacts.revision = $4         RETURNING revision",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(user_id)
    .bind(address)
    .bind(expected)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Record `address`, already proven, as `user_id`'s verified contact at
/// revision one.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert, including a unique
/// violation when the User already has a contact.
pub async fn insert_verified_contact(
    conn: &mut TenantConn<'_>,
    user_id: Uuid,
    address: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO wyrd.auth_user_contacts (data_tenant_id, user_id, address)
         VALUES ($1, $2, $3)",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(user_id)
    .bind(address)
    .execute(&mut **conn.transaction())
    .await
    .map(|_| ())
}

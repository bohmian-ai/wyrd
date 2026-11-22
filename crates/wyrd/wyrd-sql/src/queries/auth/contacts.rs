//! Tenant-local verified recovery contacts: one verified address per User,
//! distinct from its unverified profile or invitation email.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use uuid::Uuid;

use crate::TenantConn;

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

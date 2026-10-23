//! Tenant-scoped non-human principal and API-key queries.
// raw-query grep allowlist: auth tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::Error as SqlxError;
use sqlx::types::Uuid;
use wyrd_spec::auth::{Attribution, WorkloadFamily, WorkloadKind};
use wyrd_spec::ids::CardName;

use crate::TenantConn;

/// The one active account restricted to a family: the family's scheduled
/// runtime account.
const FAMILY_ACCOUNT_SQL: &str = r"
        SELECT id
          FROM wyrd.auth_service_accounts
         WHERE restriction_kind = $1
           AND restriction_name = $2
           AND status = 'active'
        ";

/// Replace one Service account's restriction; the partial unique index
/// refuses a second account for the same family.
const SET_RESTRICTION_SQL: &str = r"
        UPDATE wyrd.auth_service_accounts
           SET restriction_kind = $2, restriction_name = $3, updated_at = now()
         WHERE id = $1
           AND principal_kind = 'service'
           AND status <> 'deleted'
        ";

const INSERT_REFRESH_TOKEN_SQL: &str = r"
        INSERT INTO wyrd.auth_refresh_tokens (
            id, data_tenant_id, principal_kind, principal_id, token_hash, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6)
        ";

/// Active tenant-scope machine principal row.
///
/// Covers every kind `wyrd.auth_service_accounts` holds: a tenant
/// administrator, a stable Service account, and the internal SYSTEM writer.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceAccountPrincipalRow {
    /// Principal id.
    pub id: Uuid,
    /// Principal kind label: `tenant_admin`, `service`, or the internal
    /// `system` writer.
    pub principal_kind: String,
    /// Restricting family kind, for a family-restricted Service account.
    pub restriction_kind: Option<String>,
    /// Restricting family name, for a family-restricted Service account.
    pub restriction_name: Option<String>,
    /// Status.
    pub status: String,
}

impl ServiceAccountPrincipalRow {
    /// The account's restricting family, when it has one.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Decode`] when the stored pair is malformed.
    pub fn workload_family(&self) -> Result<Option<WorkloadFamily>, sqlx::Error> {
        workload_family(
            self.restriction_kind.as_deref(),
            self.restriction_name.as_deref(),
        )
    }
}

/// API-key lookup row joined to its principal.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyLookupRow {
    /// API key id.
    pub api_key_id: Uuid,
    /// Stored SHA-256 hex verifier.
    pub key_hash: String,
    /// Service account id.
    pub principal_id: Uuid,
    /// Principal kind label.
    pub principal_kind: String,
    /// Restricting family kind, for a family-restricted Service account.
    pub restriction_kind: Option<String>,
    /// Restricting family name, for a family-restricted Service account.
    pub restriction_name: Option<String>,
    /// Principal status.
    pub status: String,
}

impl ApiKeyLookupRow {
    /// The owning account's restricting family, when it has one.
    ///
    /// # Errors
    /// Returns [`sqlx::Error::Decode`] when the stored pair is malformed.
    pub fn workload_family(&self) -> Result<Option<WorkloadFamily>, sqlx::Error> {
        workload_family(
            self.restriction_kind.as_deref(),
            self.restriction_name.as_deref(),
        )
    }
}

/// Decode a stored `(restriction_kind, restriction_name)` pair.
///
/// # Errors
/// Returns [`sqlx::Error::Decode`] when exactly one half is present, the kind
/// is not `Service`, or the name is not a Card name; the table's
/// check constraints rule all of these out.
pub fn workload_family(
    kind: Option<&str>,
    name: Option<&str>,
) -> Result<Option<WorkloadFamily>, sqlx::Error> {
    let decode = |message: String| sqlx::Error::Decode(message.into());
    match (kind, name) {
        (None, None) => Ok(None),
        (Some(kind), Some(name)) => {
            let kind = match kind {
                "Service" => WorkloadKind::Service,
                other => return Err(decode(format!("unknown restriction kind {other:?}"))),
            };
            let name = CardName::new(name).map_err(|error| decode(error.to_string()))?;
            Ok(Some(WorkloadFamily { kind, name }))
        }
        _ => Err(decode("half-stored workload restriction".to_owned())),
    }
}

/// API-key status derived from row presence and lifecycle timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStatus {
    /// Row exists, `revoked_at IS NULL`, and the credential is unexpired —
    /// either `expires_at IS NULL` or `expires_at > statement_timestamp()`.
    Active,
    /// Row exists and `revoked_at IS NOT NULL`.
    Revoked,
    /// Row exists, is not revoked, and `expires_at <= statement_timestamp()`.
    Expired,
    /// No row matches the prefix.
    Missing,
}

/// Insert a tenant-scope machine principal row.
///
/// `family` is the durable workload restriction of a Service account, `None`
/// for an unrestricted Service account or a tenant administrator. The partial
/// unique index refuses a second live account for one family (`23505`).
/// `attribution` is the verified subject and actor creating the principal.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn insert_service_account(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_kind: &str,
    family: Option<&WorkloadFamily>,
    name: &str,
    description: Option<&str>,
    attribution: Attribution,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO wyrd.auth_service_accounts (
            id, data_tenant_id, principal_kind, restriction_kind, restriction_name,
            name, description, status, sub_principal_id, act_principal_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', $8, $9)
        ",
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(principal_kind)
    .bind(family.map(|family| family.kind.as_str()))
    .bind(family.map(|family| family.name.as_str()))
    .bind(name)
    .bind(description)
    .bind(attribution.sub_principal_id.as_uuid())
    .bind(attribution.act_principal_id.as_uuid())
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Replace one live Service account's workload restriction.
///
/// `None` makes the account unrestricted. Setting the stored value again is a
/// no-op success. Returns `Ok(false)` when no live Service account has `id`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update, including a unique
/// violation (`23505`) when another live account already holds `family`.
pub async fn set_workload_restriction(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    family: Option<&WorkloadFamily>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(SET_RESTRICTION_SQL)
        .bind(id)
        .bind(family.map(|family| family.kind.as_str()))
        .bind(family.map(|family| family.name.as_str()))
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() > 0)
}

/// The active account restricted to `family`, when one exists.
///
/// The family's unique scheduled runtime account. Tenant scope comes solely
/// from the forced RLS policy `conn` activates.
///
/// # Errors
/// Returns the database error when the read fails.
pub async fn family_account_id(
    conn: &mut TenantConn<'_>,
    family: &WorkloadFamily,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(FAMILY_ACCOUNT_SQL)
        .bind(family.kind.as_str())
        .bind(family.name.as_str())
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Soft-delete a service account by marking `status = 'deleted'`.
///
/// Returns `Ok(true)` when a row was updated, `Ok(false)` when no row matched.
/// The internal SYSTEM writer has no lifecycle and never matches.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn delete_service_account(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE wyrd.auth_service_accounts
            SET status = 'deleted', updated_at = now()
          WHERE data_tenant_id = wyrd.current_tenant()
            AND id = $1
            AND principal_kind <> 'system'",
    )
    .bind(id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Find an active public tenant machine principal by id.
///
/// Every public surface that names a principal by id — credential issuance,
/// listing, and revocation, principal revocation, and token exchange — resolves
/// it here, so the internal SYSTEM writer is excluded at this one owner and is
/// indistinguishable from an absent principal on all of them.
///
/// # Errors
/// Returns the database error when the read fails.
pub async fn service_account_by_id(
    conn: &mut TenantConn<'_>,
    id: Uuid,
) -> Result<Option<ServiceAccountPrincipalRow>, sqlx::Error> {
    sqlx::query_as::<_, ServiceAccountPrincipalRow>(
        r"
        SELECT id, principal_kind, restriction_kind, restriction_name, status
          FROM wyrd.auth_service_accounts
         WHERE data_tenant_id = $1
           AND id = $2
           AND status = 'active'
           AND principal_kind <> 'system'
        ",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(id)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Create the tenant's internal SYSTEM writer when absent and return its id.
///
/// Delegates to `wyrd.provision_system_principal(creator_sub, creator_act)`,
/// the single owner the upgrade migration's backfill also uses, so
/// provisioning and backfill cannot diverge. `creator` is the verified pair
/// creating the row: the provisioning platform operator, or
/// [`Attribution::unresolved`] when no caller exists. Idempotent: every call in
/// a tenant returns the same stable UUIDv7 and keeps the first creator pair.
/// The row joins the caller's transaction and commits with it.
///
/// # Errors
/// Returns the database error when the insert or read fails.
pub async fn provision_system_principal(
    conn: &mut TenantConn<'_>,
    creator: Attribution,
) -> Result<Uuid, SqlxError> {
    sqlx::query_scalar("SELECT wyrd.provision_system_principal($1, $2)")
        .bind(creator.sub_principal_id.as_uuid())
        .bind(creator.act_principal_id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Load the id of the tenant's internal SYSTEM writer, when provisioned.
///
/// Tenant scope comes solely from the forced RLS policy `conn` activates.
///
/// # Errors
/// Returns the database error when the read fails.
pub async fn system_principal_id(conn: &mut TenantConn<'_>) -> Result<Option<Uuid>, SqlxError> {
    sqlx::query_scalar(
        r"
        SELECT id
          FROM wyrd.auth_service_accounts
         WHERE principal_kind = 'system'
           AND status = 'active'
        ",
    )
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Find the tenant's administrative principal.
///
/// A tenant has exactly one, created during provisioning. Recovery needs it by
/// identity rather than by Card or name, because the whole point is that the
/// principal outlives the credentials that used to reach it. The oldest active
/// one wins so a tenant that somehow acquired two resolves deterministically.
///
/// # Errors
/// Returns the database error when the read fails.
pub async fn tenant_admin_principal_id(
    conn: &mut TenantConn<'_>,
) -> Result<Option<Uuid>, SqlxError> {
    sqlx::query_scalar(
        r"
        SELECT id
          FROM wyrd.auth_service_accounts
         WHERE principal_kind = 'tenant_admin'
           AND status = 'active'
         ORDER BY created_at
         LIMIT 1
        ",
    )
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Insert a hashed credential row for a tenant-scope principal.
///
/// Stores only the SHA-256 verifier and non-secret lookup metadata; the
/// plaintext is returned once by the issuing caller and never persisted.
/// `attribution` is the verified subject and actor issuing the credential.
/// `lifetime` is optional: an administrative credential issued during tenant
/// provisioning or recovery has no natural lifetime, while a workload key
/// supplies the bounded lifetime its issuance path requested. `PostgreSQL` turns
/// that lifetime into the stored expiry, and the insert returns the exact
/// `created_at` and `expires_at` it wrote so no caller reconstructs them from
/// the host clock.
///
/// # Errors
/// Returns the database error when the insert fails, including when `prefix`
/// collides within the tenant or `principal_id` names no principal.
pub async fn insert_api_key(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_id: Uuid,
    prefix: &str,
    key_hash: &str,
    attribution: Attribution,
    lifetime: Option<Duration>,
) -> Result<(DateTime<Utc>, Option<DateTime<Utc>>), sqlx::Error> {
    sqlx::query_as(
        r"
        INSERT INTO wyrd.auth_api_keys (
            id, data_tenant_id, principal_id, prefix, key_hash,
            sub_principal_id, act_principal_id, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7,
                  CASE WHEN $8::double precision IS NULL THEN NULL
                       ELSE statement_timestamp() + ($8 * interval '1 second')
                  END)
        RETURNING created_at, expires_at
        ",
    )
    .bind(id)
    .bind(conn.data_tenant_id().as_uuid())
    .bind(principal_id)
    .bind(prefix)
    .bind(key_hash)
    .bind(attribution.sub_principal_id.as_uuid())
    .bind(attribution.act_principal_id.as_uuid())
    .bind(lifetime.map(|lifetime| lifetime.as_secs_f64()))
    .fetch_one(&mut **conn.transaction())
    .await
}

/// Lookup an unexpired, unrevoked API key by prefix.
///
/// # Errors
/// Returns the [`sqlx::Error`] of the failing read. A key that does not exist,
/// is expired, or is revoked is `Ok(None)`, so every public invalid-key case
/// costs the caller the same one verification.
pub async fn api_key_by_prefix(
    conn: &mut TenantConn<'_>,
    prefix: &str,
) -> Result<Option<ApiKeyLookupRow>, sqlx::Error> {
    sqlx::query_as::<_, ApiKeyLookupRow>(
        r"
        SELECT k.id AS api_key_id,
               k.key_hash,
               sa.id AS principal_id,
               sa.principal_kind,
               sa.restriction_kind,
               sa.restriction_name,
               sa.status
          FROM wyrd.auth_api_keys k
          JOIN wyrd.auth_service_accounts sa
            ON sa.data_tenant_id = k.data_tenant_id
           AND sa.id = k.principal_id
         WHERE k.data_tenant_id = $1
           AND k.prefix = $2
           AND k.revoked_at IS NULL
           AND (k.expires_at IS NULL OR k.expires_at > statement_timestamp())
         LIMIT 1
        ",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(prefix)
    .fetch_optional(&mut **conn.transaction())
    .await
}

/// Resolve an API key status by prefix without filtering invalid states.
///
/// The revoked/expired verdict is computed by the same statement that owns the
/// row, so status and the active-key lookup cannot disagree over clock skew.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query or row decoding fails.
pub async fn api_key_status_by_prefix(
    conn: &mut TenantConn<'_>,
    prefix: &str,
) -> Result<ApiKeyStatus, sqlx::Error> {
    let row: Option<(bool, bool)> = sqlx::query_as(
        r"
        SELECT revoked_at IS NOT NULL,
               COALESCE(expires_at <= statement_timestamp(), false)
          FROM wyrd.auth_api_keys
         WHERE data_tenant_id = $1
           AND prefix = $2
         LIMIT 1
        ",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(prefix)
    .fetch_optional(&mut **conn.transaction())
    .await?;

    Ok(match row {
        None => ApiKeyStatus::Missing,
        Some((true, _)) => ApiKeyStatus::Revoked,
        Some((false, true)) => ApiKeyStatus::Expired,
        Some((false, false)) => ApiKeyStatus::Active,
    })
}

/// Mark API key usage.
pub async fn touch_api_key_last_used(
    conn: &mut TenantConn<'_>,
    api_key_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        UPDATE wyrd.auth_api_keys
           SET last_used_at = now()
         WHERE data_tenant_id = $1
           AND id = $2
        ",
    )
    .bind(conn.data_tenant_id().as_uuid())
    .bind(api_key_id)
    .execute(&mut **conn.transaction())
    .await?;
    Ok(())
}

/// Insert a principal-generic refresh token row.
pub async fn insert_refresh_token(
    conn: &mut TenantConn<'_>,
    id: Uuid,
    principal_kind: &str,
    principal_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(INSERT_REFRESH_TOKEN_SQL)
        .bind(id)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(principal_kind)
        .bind(principal_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use wyrd_spec::auth::WorkloadKind;

    use super::{
        ApiKeyStatus, FAMILY_ACCOUNT_SQL, INSERT_REFRESH_TOKEN_SQL, SET_RESTRICTION_SQL,
        workload_family,
    };

    /// The stored restriction pair decodes only as a complete, valid family.
    ///
    /// # Panics
    /// Panics when a malformed pair decodes or a valid one does not.
    #[test]
    fn stored_restriction_decodes_only_complete_families() {
        assert!(workload_family(None, None).expect("unrestricted").is_none());
        let family = workload_family(Some("Service"), Some("planner"))
            .expect("valid pair")
            .expect("restricted");
        assert_eq!(family.kind, WorkloadKind::Service);
        assert_eq!(family.name.as_str(), "planner");
        for (kind, name) in [
            (Some("Model"), Some("m")),
            (Some("Agent"), Some("planner")),
            (Some("Service"), None),
            (None, Some("checkout")),
            (Some("Service"), Some("Not A Name")),
        ] {
            assert!(workload_family(kind, name).is_err(), "{kind:?}/{name:?}");
        }
    }

    /// Family lookups and replacement rely on forced RLS, never a manual
    /// tenant predicate, and replacement never revives a deleted account.
    #[test]
    fn restriction_statements_stay_tenant_implicit() {
        for sql in [FAMILY_ACCOUNT_SQL, SET_RESTRICTION_SQL] {
            assert!(!sql.contains("data_tenant_id"));
        }
        assert!(FAMILY_ACCOUNT_SQL.contains("status = 'active'"));
        assert!(SET_RESTRICTION_SQL.contains("status <> 'deleted'"));
        assert!(SET_RESTRICTION_SQL.contains("principal_kind = 'service'"));
    }

    #[test]
    fn refresh_token_insert_is_principal_generic() {
        assert!(INSERT_REFRESH_TOKEN_SQL.contains("principal_kind, principal_id"));
        assert!(INSERT_REFRESH_TOKEN_SQL.contains("token_hash"));
        assert!(!INSERT_REFRESH_TOKEN_SQL.contains("user_id"));
        assert!(!INSERT_REFRESH_TOKEN_SQL.contains("service_account_id"));
    }

    #[test]
    fn api_key_status_enum_covers_invalid_states() {
        let statuses = [
            ApiKeyStatus::Active,
            ApiKeyStatus::Revoked,
            ApiKeyStatus::Expired,
            ApiKeyStatus::Missing,
        ];
        assert_eq!(statuses.len(), 4);
        assert_eq!(ApiKeyStatus::Active, ApiKeyStatus::Active);
    }
}

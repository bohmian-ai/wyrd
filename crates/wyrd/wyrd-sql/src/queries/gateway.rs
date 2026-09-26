//! Query slots for the tenant gateway administration and Batches tables.
//!
//! All functions take `&mut TenantConn<'_>`; FORCE RLS is the tenant boundary,
//! so statements carry no tenant-only predicate. Writes bind `data_tenant_id`
//! from the [`TenantConn`], and correlated subqueries keep the relational
//! tenant equality their composite keys require.
//! Referential rules (credential references, pricing history) live in the
//! migration's composite foreign keys, so callers map `23503` to a conflict.
// raw-query grep allowlist: gateway tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Error;
use uuid::Uuid;

use crate::TenantConn;
use crate::row_types::gateway::{
    GatewayBatchFileRow, GatewayBatchRow, GatewayCredentialRow, GatewayPolicyRow,
    GatewayPricingRow, GatewaySnapshotRow,
};

/// Creates an active credential or replaces the provider and source of an active
/// one, stamping `rotated_at`. Returns no row for a revoked name so the caller
/// reports a conflict; a provider change that would orphan a deployment fails
/// the composite foreign key with `23503`.
const UPSERT_CREDENTIAL_SQL: &str = r#"
    INSERT INTO wyrd.gateway_provider_credentials AS c (
        data_tenant_id, name, provider, source, secret_key_version,
        secret_nonce, secret_ciphertext, state
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')
    ON CONFLICT (data_tenant_id, name) DO UPDATE SET
        provider = EXCLUDED.provider,
        source = EXCLUDED.source,
        secret_key_version = EXCLUDED.secret_key_version,
        secret_nonce = EXCLUDED.secret_nonce,
        secret_ciphertext = EXCLUDED.secret_ciphertext,
        updated_at = now(),
        rotated_at = now()
     WHERE c.state = 'active'
    RETURNING name, provider, source, state, created_at, updated_at, rotated_at, revoked_at
"#;

/// Reads one credential row by name within the tenant RLS scope.
const CREDENTIAL_SQL: &str = r#"
    SELECT name, provider, source, state, created_at, updated_at, rotated_at, revoked_at
      FROM wyrd.gateway_provider_credentials
     WHERE name = $1
"#;

/// Reads one credential row under a `FOR SHARE` lock held until commit, so a
/// concurrent revoke or delete serializes behind the deployment write that
/// references it.
const CREDENTIAL_FOR_SHARE_SQL: &str = r#"
    SELECT name, provider, source, state, created_at, updated_at, rotated_at, revoked_at
      FROM wyrd.gateway_provider_credentials
     WHERE name = $1
       FOR SHARE
"#;

/// Lists the tenant's credential rows ordered by name.
const CREDENTIALS_SQL: &str = r#"
    SELECT name, provider, source, state, created_at, updated_at, rotated_at, revoked_at
      FROM wyrd.gateway_provider_credentials
     ORDER BY name
"#;

/// Terminally revokes a credential; repeating keeps the first `revoked_at` and
/// leaves `updated_at` unchanged.
const REVOKE_CREDENTIAL_SQL: &str = r#"
    UPDATE wyrd.gateway_provider_credentials
       SET state = 'revoked',
           revoked_at = COALESCE(revoked_at, now()),
           updated_at = CASE WHEN state = 'active' THEN now() ELSE updated_at END
     WHERE name = $1
    RETURNING name, provider, source, state, created_at, updated_at, rotated_at, revoked_at
"#;

/// Deletes one credential; a referencing deployment fails the composite foreign
/// key with `23503`.
const DELETE_CREDENTIAL_SQL: &str = r#"
    DELETE FROM wyrd.gateway_provider_credentials
     WHERE name = $1
"#;

/// Creates or replaces one deployment document together with its provider,
/// model, and credential-reference columns that the foreign keys check.
const UPSERT_DEPLOYMENT_SQL: &str = r#"
    INSERT INTO wyrd.gateway_provider_deployments (
        data_tenant_id, name, provider, model, credential_name, deployment
    ) VALUES ($1, $2, $3, $4, $5, $6)
    ON CONFLICT (data_tenant_id, name) DO UPDATE SET
        provider = EXCLUDED.provider,
        model = EXCLUDED.model,
        credential_name = EXCLUDED.credential_name,
        deployment = EXCLUDED.deployment,
        updated_at = now()
"#;

/// Reads one deployment document by name.
const DEPLOYMENT_SQL: &str = r#"
    SELECT deployment
      FROM wyrd.gateway_provider_deployments
     WHERE name = $1
"#;

/// Lists the tenant's deployment documents ordered by name.
const DEPLOYMENTS_SQL: &str = r#"
    SELECT deployment
      FROM wyrd.gateway_provider_deployments
     ORDER BY name
"#;

/// Deletes one deployment; an absent name deletes nothing.
const DELETE_DEPLOYMENT_SQL: &str = r#"
    DELETE FROM wyrd.gateway_provider_deployments
     WHERE name = $1
"#;

/// Creates the tenant's single policy row when absent so it can be locked.
const ENSURE_POLICY_ROW_SQL: &str = r#"
    INSERT INTO wyrd.gateway_policies (data_tenant_id)
    VALUES ($1)
    ON CONFLICT (data_tenant_id) DO NOTHING
"#;

/// Reads the tenant's policy documents under a `FOR UPDATE` row lock that
/// serializes concurrent policy replacements until commit.
const LOCK_POLICY_SQL: &str = r#"
    SELECT fallback, governance, capture
      FROM wyrd.gateway_policies
       FOR UPDATE
"#;

/// Reads the tenant's fallback, governance, and capture documents without a lock.
const POLICY_SQL: &str = r#"
    SELECT fallback, governance, capture
      FROM wyrd.gateway_policies
"#;

/// Overwrites all three policy documents on the tenant's locked policy row.
const UPDATE_POLICY_SQL: &str = r#"
    UPDATE wyrd.gateway_policies
       SET fallback = $1,
           governance = $2,
           capture = $3,
           updated_at = now()
"#;

/// Lists every retained pricing version with its authoritative `active` flag.
const PRICING_SQL: &str = r#"
    SELECT active, entry
      FROM wyrd.gateway_model_pricing
     ORDER BY provider, model, effective_at
"#;

/// Inserts a pricing version, or updates only the `active` flag and entry of an
/// existing `(provider, model, version)`; callers reject changed version content.
const UPSERT_PRICING_SQL: &str = r#"
    INSERT INTO wyrd.gateway_model_pricing (
        data_tenant_id, provider, model, version, effective_at, active, entry
    ) VALUES ($1, $2, $3, $4, $5, $6, $7)
    ON CONFLICT (data_tenant_id, provider, model, version) DO UPDATE SET
        active = EXCLUDED.active,
        entry = EXCLUDED.entry
"#;

/// Appends one accounting entry; `ON CONFLICT DO NOTHING` fences replays so an
/// entry is written at most once.
const APPEND_ACCOUNTING_ENTRY_SQL: &str = r#"
    INSERT INTO wyrd.gateway_accounting_entries (
        data_tenant_id, entry_id, call_id, kind, provider, model, pricing_version, entry
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    ON CONFLICT DO NOTHING
"#;

/// Takes the tenant's transaction-scoped admission advisory lock, serializing
/// admission across every replica until the transaction ends.
const LOCK_ADMISSION_SQL: &str = r#"
    SELECT pg_advisory_xact_lock(
        hashtextextended('wyrd.gateway_admission:' || wyrd.current_tenant()::text, 0)
    )
"#;

/// Deletes limit windows that started before the cutoff minute.
const PRUNE_WINDOWS_SQL: &str = r#"
    DELETE FROM wyrd.gateway_limit_windows
     WHERE window_start < $1
"#;

/// Deletes concurrency leases that expired at or before `now`.
const PRUNE_LEASES_SQL: &str = r#"
    DELETE FROM wyrd.gateway_call_leases
     WHERE expires_at <= $1
"#;

/// Reads one limit's requests and tokens in a window and its unexpired lease count.
const LIMIT_USAGE_SQL: &str = r#"
    SELECT
        COALESCE((SELECT w.requests FROM wyrd.gateway_limit_windows w
                   WHERE w.limit_key = $1 AND w.window_start = $2), 0) AS requests,
        COALESCE((SELECT w.tokens FROM wyrd.gateway_limit_windows w
                   WHERE w.limit_key = $1 AND w.window_start = $2), 0) AS tokens,
        (SELECT count(*) FROM wyrd.gateway_call_leases l
          WHERE l.limit_key = $1 AND l.expires_at > $3) AS leases
"#;

/// Adds request and signed token counts to one limit window, creating it on
/// first charge; a token release never takes the window below zero.
const CHARGE_WINDOW_SQL: &str = r#"
    INSERT INTO wyrd.gateway_limit_windows AS w (
        data_tenant_id, limit_key, window_start, requests, tokens
    ) VALUES ($1, $2, $3, $4, GREATEST($5, 0))
    ON CONFLICT (data_tenant_id, limit_key, window_start) DO UPDATE SET
        requests = w.requests + EXCLUDED.requests,
        tokens = GREATEST(w.tokens + $5, 0)
"#;

/// Records one call's concurrency lease on a limit; a replay inserts nothing.
const INSERT_LEASE_SQL: &str = r#"
    INSERT INTO wyrd.gateway_call_leases (data_tenant_id, limit_key, call_id, expires_at)
    VALUES ($1, $2, $3, $4)
    ON CONFLICT DO NOTHING
"#;

/// Deletes every concurrency lease a call holds.
const RELEASE_LEASES_SQL: &str = r#"
    DELETE FROM wyrd.gateway_call_leases
     WHERE call_id = $1
"#;

/// Sums a budget subject's period spend: settled actual cost, or the reserved
/// cost while a reservation is unsettled.
const BUDGET_SPEND_SQL: &str = r#"
    SELECT COALESCE(SUM(COALESCE(
               (s.entry -> 'budget_reservation_settled' ->> 'actual_cost')::numeric,
               (c.entry -> 'budget_reservation_created' ->> 'reserved_cost')::numeric
           )), 0)::text
      FROM wyrd.gateway_accounting_entries c
      LEFT JOIN wyrd.gateway_accounting_entries s
        ON s.data_tenant_id = c.data_tenant_id
       AND s.kind = 'budget_reservation_settled'
       AND s.entry -> 'budget_reservation_settled' ->> 'reservation_id'
         = c.entry -> 'budget_reservation_created' ->> 'reservation_id'
     WHERE c.kind = 'budget_reservation_created'
       AND c.entry -> 'budget_reservation_created' ->> 'period_start' = $2
       AND c.entry -> 'budget_reservation_created' ->> 'period_end' = $3
       AND c.entry -> 'budget_reservation_created' -> 'subject' = $1
"#;

/// Reads a bounded, oldest-first batch of expired reservations that have no
/// settlement entry.
const EXPIRED_RESERVATIONS_SQL: &str = r#"
    SELECT c.entry
      FROM wyrd.gateway_accounting_entries c
     WHERE c.kind = 'budget_reservation_created'
       AND (c.entry -> 'budget_reservation_created' ->> 'expires_at')::timestamptz < $1
       AND NOT EXISTS (
           SELECT 1
             FROM wyrd.gateway_accounting_entries s
            WHERE s.data_tenant_id = c.data_tenant_id
              AND s.kind = 'budget_reservation_settled'
              AND s.entry -> 'budget_reservation_settled' ->> 'reservation_id'
                = c.entry -> 'budget_reservation_created' ->> 'reservation_id'
       )
     ORDER BY c.recorded_at
     LIMIT $2
"#;

/// Reads every accounting entry of one call in recording order.
const CALL_ENTRIES_SQL: &str = r#"
    SELECT entry
      FROM wyrd.gateway_accounting_entries
     WHERE call_id = $1
     ORDER BY recorded_at, entry_id
"#;

/// Reads deployments, credentials, policies, and pricing in one statement so
/// admission observes one consistent tenant configuration.
const SNAPSHOT_SQL: &str = r#"
    SELECT
        COALESCE((
            SELECT jsonb_agg(d.deployment ORDER BY d.name)
              FROM wyrd.gateway_provider_deployments d
        ), '[]'::jsonb) AS deployments,
        COALESCE((
            SELECT jsonb_agg(jsonb_build_object(
                       'name', c.name,
                       'provider', c.provider,
                       'source', c.source,
                       'state', c.state,
                       'secret_key_version', c.secret_key_version,
                       -- encode() wraps base64 every 76 characters; the
                       -- reader decodes one unbroken string.
                       'secret_nonce', replace(encode(c.secret_nonce, 'base64'), E'\n', ''),
                       'secret_ciphertext',
                           replace(encode(c.secret_ciphertext, 'base64'), E'\n', '')
                   ) ORDER BY c.name)
              FROM wyrd.gateway_provider_credentials c
        ), '[]'::jsonb) AS credentials,
        COALESCE((
            SELECT jsonb_agg(
                       p.entry || jsonb_build_object('active', p.active)
                       ORDER BY p.provider, p.model, p.effective_at
                   )
              FROM wyrd.gateway_model_pricing p
        ), '[]'::jsonb) AS pricing,
        (SELECT g.fallback FROM wyrd.gateway_policies g) AS fallback,
        (SELECT g.governance FROM wyrd.gateway_policies g) AS governance,
        (SELECT g.capture FROM wyrd.gateway_policies g) AS capture
"#;

/// Column values for one credential create-or-replace.
#[derive(Debug, Clone, Copy)]
pub struct GatewayCredentialWrite<'a> {
    /// Tenant-unique credential name.
    pub name: &'a str,
    /// Provider identity.
    pub provider: &'a str,
    /// Redacted source view JSON.
    pub source: &'a Value,
    /// Sealed managed-secret envelope, or `None` for an operator-owned source
    /// whose value Wyrd never holds.
    pub secret: Option<GatewayCredentialEnvelope<'a>>,
}

/// Sealed managed-secret columns of one credential row.
///
/// The server seals the value before this write, so the query layer stores
/// opaque bytes and never sees plaintext or key material.
#[derive(Debug, Clone, Copy)]
pub struct GatewayCredentialEnvelope<'a> {
    /// Tenant keyring version that sealed the payload.
    pub key_version: &'a str,
    /// AES-GCM nonce.
    pub nonce: &'a [u8],
    /// Sealed payload bytes.
    pub ciphertext: &'a [u8],
}

/// Column values for one deployment create-or-replace.
#[derive(Debug, Clone, Copy)]
pub struct GatewayDeploymentWrite<'a> {
    /// Tenant-unique deployment name.
    pub name: &'a str,
    /// Provider identity of the served model.
    pub provider: &'a str,
    /// Provider-native model identifier.
    pub model: &'a str,
    /// Referenced credential, if the deployment authenticates upstream.
    pub credential_name: Option<&'a str>,
    /// Complete deployment JSON.
    pub deployment: &'a Value,
}

/// Column values for one pricing entry create-or-retire.
#[derive(Debug, Clone, Copy)]
pub struct GatewayPricingWrite<'a> {
    /// Provider identity.
    pub provider: &'a str,
    /// Provider-native model identifier.
    pub model: &'a str,
    /// Immutable version label.
    pub version: &'a str,
    /// Admission time from which the entry applies.
    pub effective_at: DateTime<Utc>,
    /// Authoritative eligibility flag.
    pub active: bool,
    /// Complete pricing entry JSON.
    pub entry: &'a Value,
}

/// Column values for one append-only ledger entry.
#[derive(Debug, Clone, Copy)]
pub struct GatewayAccountingEntryWrite<'a> {
    /// Entry identity and idempotency fence.
    pub entry_id: Uuid,
    /// Logical call.
    pub call_id: Uuid,
    /// Entry variant tag.
    pub kind: &'a str,
    /// Priced model provider, for a priced attempt entry.
    pub provider: Option<&'a str>,
    /// Priced model identifier, for a priced attempt entry.
    pub model: Option<&'a str>,
    /// Pricing version used, for a priced attempt entry.
    pub pricing_version: Option<&'a str>,
    /// Complete entry JSON.
    pub entry: &'a Value,
}

/// Creates an active credential or replaces an active one as a rotation.
///
/// One atomic `INSERT ... ON CONFLICT DO UPDATE ... WHERE state = 'active'`
/// makes repeats and concurrent writers resolve by commit order without a
/// duplicate. Returns `None` when the name exists and is revoked.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the write, including an FK
/// violation when a provider change would orphan a referencing deployment.
pub async fn upsert_gateway_credential(
    conn: &mut TenantConn<'_>,
    write: GatewayCredentialWrite<'_>,
) -> Result<Option<GatewayCredentialRow>, Error> {
    sqlx::query_as::<_, GatewayCredentialRow>(UPSERT_CREDENTIAL_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(write.name)
        .bind(write.provider)
        .bind(write.source)
        .bind(write.secret.map(|secret| secret.key_version))
        .bind(write.secret.map(|secret| secret.nonce))
        .bind(write.secret.map(|secret| secret.ciphertext))
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Reads one credential by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_credential(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<Option<GatewayCredentialRow>, Error> {
    sqlx::query_as::<_, GatewayCredentialRow>(CREDENTIAL_SQL)
        .bind(name)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Reads one credential by name and share-locks it until commit.
///
/// A deployment write holds this lock so a concurrent revoke or delete of the
/// referenced credential serializes behind it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_credential_for_share(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<Option<GatewayCredentialRow>, Error> {
    sqlx::query_as::<_, GatewayCredentialRow>(CREDENTIAL_FOR_SHARE_SQL)
        .bind(name)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Lists the tenant's credentials ordered by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_credentials(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<GatewayCredentialRow>, Error> {
    sqlx::query_as::<_, GatewayCredentialRow>(CREDENTIALS_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Terminally revokes a credential; repeating keeps the first revocation time.
///
/// Returns `None` when no credential has the name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn revoke_gateway_credential(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<Option<GatewayCredentialRow>, Error> {
    sqlx::query_as::<_, GatewayCredentialRow>(REVOKE_CREDENTIAL_SQL)
        .bind(name)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Deletes a credential and returns the number of rows removed.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete, including an FK
/// violation while a deployment references the credential.
pub async fn delete_gateway_credential(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<u64, Error> {
    sqlx::query(DELETE_CREDENTIAL_SQL)
        .bind(name)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected())
}

/// Creates or replaces one deployment.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the write, including an FK
/// violation when the credential reference does not match by name and provider.
pub async fn upsert_gateway_deployment(
    conn: &mut TenantConn<'_>,
    write: GatewayDeploymentWrite<'_>,
) -> Result<(), Error> {
    sqlx::query(UPSERT_DEPLOYMENT_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(write.name)
        .bind(write.provider)
        .bind(write.model)
        .bind(write.credential_name)
        .bind(write.deployment)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Reads one deployment document by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_deployment(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<Option<Value>, Error> {
    sqlx::query_scalar::<_, Value>(DEPLOYMENT_SQL)
        .bind(name)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Lists the tenant's deployment documents ordered by name.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_deployments(conn: &mut TenantConn<'_>) -> Result<Vec<Value>, Error> {
    sqlx::query_scalar::<_, Value>(DEPLOYMENTS_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Deletes a deployment and returns the number of rows removed.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn delete_gateway_deployment(
    conn: &mut TenantConn<'_>,
    name: &str,
) -> Result<u64, Error> {
    sqlx::query(DELETE_DEPLOYMENT_SQL)
        .bind(name)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected())
}

/// Ensures the tenant policy row exists and locks it until commit.
///
/// Every policy write takes this lock first, so fallback, governance (with its
/// pricing rows), and capture replacements serialize per tenant and the last
/// committed writer wins.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects either statement.
pub async fn lock_gateway_policies(conn: &mut TenantConn<'_>) -> Result<GatewayPolicyRow, Error> {
    sqlx::query(ENSURE_POLICY_ROW_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query_as::<_, GatewayPolicyRow>(LOCK_POLICY_SQL)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Reads the tenant policy documents; a missing row is all defaults.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_policies(conn: &mut TenantConn<'_>) -> Result<GatewayPolicyRow, Error> {
    sqlx::query_as::<_, GatewayPolicyRow>(POLICY_SQL)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map(Option::unwrap_or_default)
}

/// Writes all three policy documents on the locked tenant row.
///
/// Call only after [`lock_gateway_policies`] in the same transaction.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn update_gateway_policies(
    conn: &mut TenantConn<'_>,
    policies: &GatewayPolicyRow,
) -> Result<(), Error> {
    sqlx::query(UPDATE_POLICY_SQL)
        .bind(&policies.fallback)
        .bind(&policies.governance)
        .bind(&policies.capture)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Lists every retained pricing entry.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_pricing(conn: &mut TenantConn<'_>) -> Result<Vec<GatewayPricingRow>, Error> {
    sqlx::query_as::<_, GatewayPricingRow>(PRICING_SQL)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Creates a pricing version or updates a retained version's eligibility.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the write.
pub async fn upsert_gateway_pricing(
    conn: &mut TenantConn<'_>,
    write: GatewayPricingWrite<'_>,
) -> Result<(), Error> {
    sqlx::query(UPSERT_PRICING_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(write.provider)
        .bind(write.model)
        .bind(write.version)
        .bind(write.effective_at)
        .bind(write.active)
        .bind(write.entry)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Appends one accounting ledger entry unless an idempotency fence already
/// holds it, and returns whether the entry was inserted.
///
/// The fences are the entry id, the reservation id of a creation or
/// settlement, `(call_id, attempt_ordinal)`, and one call entry per call, so a
/// replayed or concurrently reconciled write is a no-op.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert, including an FK
/// violation for unknown pricing.
pub async fn append_gateway_accounting_entry(
    conn: &mut TenantConn<'_>,
    write: GatewayAccountingEntryWrite<'_>,
) -> Result<bool, Error> {
    sqlx::query(APPEND_ACCOUNTING_ENTRY_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(write.entry_id)
        .bind(write.call_id)
        .bind(write.kind)
        .bind(write.provider)
        .bind(write.model)
        .bind(write.pricing_version)
        .bind(write.entry)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected() == 1)
}

/// Serializes gateway admission for the connection's tenant until commit.
///
/// Every replica takes this transaction advisory lock before reading and
/// charging limits or budgets, so concurrent admissions observe each other's
/// committed charges and cannot oversubscribe.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the lock.
pub async fn lock_gateway_admission(conn: &mut TenantConn<'_>) -> Result<(), Error> {
    sqlx::query(LOCK_ADMISSION_SQL)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Deletes limit windows older than `window_floor` and leases lapsed at `now`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects either delete.
pub async fn prune_gateway_admission(
    conn: &mut TenantConn<'_>,
    window_floor: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(), Error> {
    sqlx::query(PRUNE_WINDOWS_SQL)
        .bind(window_floor)
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query(PRUNE_LEASES_SQL)
        .bind(now)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Reads one limit's `(requests, tokens)` in `window_start` and its leases
/// unexpired at `now`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_limit_usage(
    conn: &mut TenantConn<'_>,
    limit_key: &str,
    window_start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(i64, i64, i64), Error> {
    sqlx::query_as::<_, (i64, i64, i64)>(LIMIT_USAGE_SQL)
        .bind(limit_key)
        .bind(window_start)
        .bind(now)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Adds `requests` and `tokens` to one limit's window; negative `tokens`
/// release a hold, clamped at zero.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the upsert.
pub async fn charge_gateway_limit_window(
    conn: &mut TenantConn<'_>,
    limit_key: &str,
    window_start: DateTime<Utc>,
    requests: i64,
    tokens: i64,
) -> Result<(), Error> {
    sqlx::query(CHARGE_WINDOW_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(limit_key)
        .bind(window_start)
        .bind(requests)
        .bind(tokens)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Holds one concurrency slot of `limit_key` for `call_id` until `expires_at`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn insert_gateway_call_lease(
    conn: &mut TenantConn<'_>,
    limit_key: &str,
    call_id: Uuid,
    expires_at: DateTime<Utc>,
) -> Result<(), Error> {
    sqlx::query(INSERT_LEASE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(limit_key)
        .bind(call_id)
        .bind(expires_at)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Releases every concurrency slot held by `call_id`.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn release_gateway_call_leases(
    conn: &mut TenantConn<'_>,
    call_id: Uuid,
) -> Result<(), Error> {
    sqlx::query(RELEASE_LEASES_SQL)
        .bind(call_id)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

/// Sums one budget subject's spend in the exact period `[period_start,
/// period_end)` (the RFC 3339 text stored on its reservations), so budgets of
/// different lengths sharing a start never share spend: settled actual cost,
/// or the reserved cost while a reservation is unsettled. Returns decimal
/// text.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_budget_spend(
    conn: &mut TenantConn<'_>,
    subject: &Value,
    period_start: &str,
    period_end: &str,
) -> Result<String, Error> {
    sqlx::query_scalar::<_, String>(BUDGET_SPEND_SQL)
        .bind(subject)
        .bind(period_start)
        .bind(period_end)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Lists at most `limit` unsettled reservation-creation entries that expired
/// before `now`, oldest first.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn expired_gateway_reservations(
    conn: &mut TenantConn<'_>,
    now: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<Value>, Error> {
    sqlx::query_scalar::<_, Value>(EXPIRED_RESERVATIONS_SQL)
        .bind(now)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Lists every ledger entry of one call in recording order.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_call_accounting_entries(
    conn: &mut TenantConn<'_>,
    call_id: Uuid,
) -> Result<Vec<Value>, Error> {
    sqlx::query_scalar::<_, Value>(CALL_ENTRIES_SQL)
        .bind(call_id)
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Reads every gateway configuration table in one SQL statement.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_snapshot(conn: &mut TenantConn<'_>) -> Result<GatewaySnapshotRow, Error> {
    sqlx::query_as::<_, GatewaySnapshotRow>(SNAPSHOT_SQL)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Records one uploaded batch input file.
const INSERT_BATCH_FILE_SQL: &str = r#"
    INSERT INTO wyrd.gateway_batch_files (
        data_tenant_id, file_id, filename, size_bytes, sha256, content_type,
        endpoint, model, deployment, upstream_file_id
    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
    RETURNING file_id, filename, size_bytes, sha256, content_type, endpoint,
              model, deployment, upstream_file_id, created_at
"#;

/// Reads one batch input file.
const BATCH_FILE_SQL: &str = r#"
    SELECT file_id, filename, size_bytes, sha256, content_type, endpoint,
           model, deployment, upstream_file_id, created_at
      FROM wyrd.gateway_batch_files
     WHERE file_id = $1
"#;

/// Deletes one batch input file.
const DELETE_BATCH_FILE_SQL: &str = r#"
    DELETE FROM wyrd.gateway_batch_files
     WHERE file_id = $1
"#;

/// Claims the creation fence of one canonical create request; returns no row
/// when the request was already claimed.
const CLAIM_BATCH_SQL: &str = r#"
    INSERT INTO wyrd.gateway_batches (
        data_tenant_id, batch_id, request_sha256, file_id, model, deployment
    ) VALUES ($1, $2, $3, $4, $5, $6)
    ON CONFLICT (data_tenant_id, request_sha256) DO NOTHING
    RETURNING batch_id
"#;

/// Reads the batch claimed under a create request digest.
const BATCH_BY_REQUEST_SQL: &str = r#"
    SELECT batch_id, file_id, model, deployment, upstream_batch_id, batch
      FROM wyrd.gateway_batches
     WHERE request_sha256 = $1
"#;

/// Reads one batch.
const BATCH_SQL: &str = r#"
    SELECT batch_id, file_id, model, deployment, upstream_batch_id, batch
      FROM wyrd.gateway_batches
     WHERE batch_id = $1
"#;

/// Lists created batches newest first below an optional cursor, restricted,
/// unless `$3` is NULL, to models of providers `$3` or exact models `$4`.
const BATCHES_SQL: &str = r#"
    SELECT batch_id, file_id, model, deployment, upstream_batch_id, batch
      FROM wyrd.gateway_batches
     WHERE upstream_batch_id IS NOT NULL AND ($1::uuid IS NULL OR batch_id < $1)
       AND ($3::text[] IS NULL OR split_part(model, '/', 1) = ANY($3) OR model = ANY($4::text[]))
     ORDER BY batch_id DESC
     LIMIT $2
"#;

/// Records the provider batch id, when first known, and the last observed
/// provider batch object.
const RECORD_BATCH_SQL: &str = r#"
    UPDATE wyrd.gateway_batches
       SET upstream_batch_id = $2, batch = $3
     WHERE batch_id = $1
       AND (upstream_batch_id IS NULL OR upstream_batch_id = $2)
"#;

/// Releases a still-pending creation fence.
const RELEASE_BATCH_SQL: &str = r#"
    DELETE FROM wyrd.gateway_batches
     WHERE batch_id = $1 AND upstream_batch_id IS NULL
"#;

/// Records one uploaded batch input file and returns its stored row.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert, including a
/// duplicate file id.
pub async fn insert_gateway_batch_file(
    conn: &mut TenantConn<'_>,
    file: &GatewayBatchFileRow,
) -> Result<GatewayBatchFileRow, Error> {
    sqlx::query_as::<_, GatewayBatchFileRow>(INSERT_BATCH_FILE_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(file.file_id)
        .bind(&file.filename)
        .bind(file.size_bytes)
        .bind(&file.sha256)
        .bind(&file.content_type)
        .bind(&file.endpoint)
        .bind(&file.model)
        .bind(&file.deployment)
        .bind(&file.upstream_file_id)
        .fetch_one(&mut **conn.transaction())
        .await
}

/// Reads one batch input file of the tenant.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_batch_file(
    conn: &mut TenantConn<'_>,
    file_id: Uuid,
) -> Result<Option<GatewayBatchFileRow>, Error> {
    sqlx::query_as::<_, GatewayBatchFileRow>(BATCH_FILE_SQL)
        .bind(file_id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Deletes one batch input file and returns the number of deleted rows.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn delete_gateway_batch_file(
    conn: &mut TenantConn<'_>,
    file_id: Uuid,
) -> Result<u64, Error> {
    sqlx::query(DELETE_BATCH_FILE_SQL)
        .bind(file_id)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected())
}

/// Claims the creation fence of a create request digest for a new pending
/// batch.
///
/// Returns `true` when this call claimed it and `false` when an earlier
/// creation of the same request holds it.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the insert.
pub async fn claim_gateway_batch(
    conn: &mut TenantConn<'_>,
    batch: &GatewayBatchRow,
    request_sha256: &str,
) -> Result<bool, Error> {
    sqlx::query_scalar::<_, Uuid>(CLAIM_BATCH_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(batch.batch_id)
        .bind(request_sha256)
        .bind(batch.file_id)
        .bind(&batch.model)
        .bind(&batch.deployment)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map(|claimed| claimed.is_some())
}

/// Reads the batch claimed under a create request digest.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_batch_by_request(
    conn: &mut TenantConn<'_>,
    request_sha256: &str,
) -> Result<Option<GatewayBatchRow>, Error> {
    sqlx::query_as::<_, GatewayBatchRow>(BATCH_BY_REQUEST_SQL)
        .bind(request_sha256)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Reads one batch of the tenant, pending or created.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_batch(
    conn: &mut TenantConn<'_>,
    batch_id: Uuid,
) -> Result<Option<GatewayBatchRow>, Error> {
    sqlx::query_as::<_, GatewayBatchRow>(BATCH_SQL)
        .bind(batch_id)
        .fetch_optional(&mut **conn.transaction())
        .await
}

/// Lists at most `limit` created batches, newest first, strictly older than
/// the `after` cursor when given.
///
/// `eligible`, when given, is the `(providers, models)` pair of provider ids
/// and exact `<provider>/<model>` projections a batch's model must match; it
/// lets a caller prune batches it can never see inside this one bounded read.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the query.
pub async fn gateway_batches(
    conn: &mut TenantConn<'_>,
    after: Option<Uuid>,
    limit: i64,
    eligible: Option<(&[String], &[String])>,
) -> Result<Vec<GatewayBatchRow>, Error> {
    sqlx::query_as::<_, GatewayBatchRow>(BATCHES_SQL)
        .bind(after)
        .bind(limit)
        .bind(eligible.map(|(providers, _)| providers))
        .bind(eligible.map_or(&[][..], |(_, models)| models))
        .fetch_all(&mut **conn.transaction())
        .await
}

/// Records a batch's provider id and last observed provider object.
///
/// Returns `false`, changing nothing, when the batch is absent or already
/// bound to a different provider id.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the update.
pub async fn record_gateway_batch(
    conn: &mut TenantConn<'_>,
    batch_id: Uuid,
    upstream_batch_id: &str,
    batch: &Value,
) -> Result<bool, Error> {
    sqlx::query(RECORD_BATCH_SQL)
        .bind(batch_id)
        .bind(upstream_batch_id)
        .bind(batch)
        .execute(&mut **conn.transaction())
        .await
        .map(|result| result.rows_affected() == 1)
}

/// Releases a pending creation fence so the same request may be created
/// again; a created batch is never released.
///
/// # Errors
/// Returns a SQLx error when Postgres rejects the delete.
pub async fn release_gateway_batch(conn: &mut TenantConn<'_>, batch_id: Uuid) -> Result<(), Error> {
    sqlx::query(RELEASE_BATCH_SQL)
        .bind(batch_id)
        .execute(&mut **conn.transaction())
        .await
        .map(|_| ())
}

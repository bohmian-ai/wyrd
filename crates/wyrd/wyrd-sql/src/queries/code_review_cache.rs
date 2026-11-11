//! Code-review pipeline cache metadata in `wyrd.code_review_snapshot_pointers`,
//! `wyrd.code_review_snapshot_records` and `wyrd.code_review_contexts`.
//!
//! These are the statements behind the pipeline's snapshot and context store
//! protocols. Each is one statement on the caller's RLS [`TenantConn`] and
//! never commits; the caller opens one connection per protocol call and
//! commits it before any GitHub or object-storage IO. Rows are keyed by
//! installation and stable numeric repository id within the tenant, so a
//! repository rename keeps its rows. Lifecycle timestamps come from
//! `statement_timestamp()` with bound durations. The one cross-tenant read,
//! [`expired_code_review_contexts`], runs on the [`OperatorPool`].
// raw-query grep allowlist: code-review cache tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::{OperatorPool, SqlError, TenantConn};

/// Read the pointer of one compatibility scope.
const POINTER_SQL: &str = "SELECT pointer FROM wyrd.code_review_snapshot_pointers
     WHERE installation_id = $1 AND repository_id = $2
       AND extraction_version = $3 AND format_version = $4 AND max_file_bytes = $5";

/// Publish the first pointer of a scope; a concurrent first publication wins.
const INSERT_POINTER_SQL: &str = "INSERT INTO wyrd.code_review_snapshot_pointers (
         data_tenant_id, installation_id, repository_id, extraction_version, format_version,
         max_file_bytes, pointer, tree
     ) VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7)
     ON CONFLICT DO NOTHING";

/// Move a scope's pointer only when it still names the expected tree.
const SWAP_POINTER_SQL: &str = "UPDATE wyrd.code_review_snapshot_pointers
        SET pointer = $6, tree = $7
      WHERE installation_id = $1 AND repository_id = $2
        AND extraction_version = $3 AND format_version = $4 AND max_file_bytes = $5
        AND tree = $8";

/// Insert or re-touch an unclaimed snapshot record.
const TOUCH_RECORD_SQL: &str = "INSERT INTO wyrd.code_review_snapshot_records (
         data_tenant_id, installation_id, repository_id, object_key, touched_at
     ) VALUES (wyrd.current_tenant(), $1, $2, $3, statement_timestamp())
     ON CONFLICT (data_tenant_id, installation_id, repository_id, object_key)
     DO UPDATE SET touched_at = statement_timestamp()
      WHERE NOT wyrd.code_review_snapshot_records.claimed";

/// Claim every record no pointer names that was not touched within the grace.
const CLAIM_RECORDS_SQL: &str = "UPDATE wyrd.code_review_snapshot_records record
        SET claimed = true
      WHERE record.installation_id = $1 AND record.repository_id = $2
        AND record.touched_at <= statement_timestamp() - make_interval(secs => $3)
        AND NOT EXISTS (
            SELECT 1 FROM wyrd.code_review_snapshot_pointers pointer
             WHERE pointer.installation_id = record.installation_id
               AND pointer.repository_id = record.repository_id
               AND pointer.pointer->>'key' = record.object_key)
      RETURNING record.object_key";

/// Delete one snapshot record.
const DELETE_RECORD_SQL: &str = "DELETE FROM wyrd.code_review_snapshot_records
      WHERE installation_id = $1 AND repository_id = $2 AND object_key = $3";

/// Delete every pointer of one repository.
const DELETE_POINTERS_SQL: &str = "DELETE FROM wyrd.code_review_snapshot_pointers
      WHERE installation_id = $1 AND repository_id = $2";

/// Insert a context, or return an existing one's original stamps unchanged.
const UPSERT_CONTEXT_SQL: &str = "INSERT INTO wyrd.code_review_contexts (
         data_tenant_id, installation_id, repository_id, context_id, identity, record,
         published_at, expires_at
     ) VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, statement_timestamp(),
               statement_timestamp() + make_interval(secs => $6))
     ON CONFLICT (data_tenant_id, installation_id, repository_id, context_id)
     DO UPDATE SET context_id = EXCLUDED.context_id
     RETURNING published_at, expires_at";

/// Read one context and whether it has expired.
const CONTEXT_SQL: &str = "SELECT record, expires_at <= statement_timestamp() AS expired
       FROM wyrd.code_review_contexts
      WHERE installation_id = $1 AND repository_id = $2 AND context_id = $3";

/// Read expired contexts across tenants.
const EXPIRED_CONTEXTS_SQL: &str = "SELECT data_tenant_id, identity, record
       FROM wyrd.code_review_contexts
      WHERE expires_at <= statement_timestamp()
      ORDER BY expires_at
      LIMIT $1";

/// Delete one context.
const DELETE_CONTEXT_SQL: &str = "DELETE FROM wyrd.code_review_contexts
      WHERE installation_id = $1 AND repository_id = $2 AND context_id = $3";

/// Delete every context of one repository, returning their object keys.
const DELETE_CONTEXTS_SQL: &str = "DELETE FROM wyrd.code_review_contexts
      WHERE installation_id = $1 AND repository_id = $2
      RETURNING record->>'key' AS object_key";

/// One repository of one installation within the connection's tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeReviewRepository {
    /// GitHub App installation id.
    pub installation_id: i64,
    /// Stable numeric GitHub repository id.
    pub repository_id: i64,
}

/// One snapshot compatibility scope of a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeReviewSnapshotScope<'a> {
    /// Repository the snapshot indexes.
    pub repository: CodeReviewRepository,
    /// Extraction version the facts were produced under.
    pub extraction_version: &'a str,
    /// Snapshot object format version.
    pub format_version: i64,
    /// Largest eligible file the facts were built under.
    pub max_file_bytes: i64,
}

/// A context's publication and expiry stamps, on the database clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct CodeReviewContextStamps {
    /// When the context was first recorded.
    pub published_at: DateTime<Utc>,
    /// When reads start being refused.
    pub expires_at: DateTime<Utc>,
}

/// One stored context record and whether it has expired.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CodeReviewContextRow {
    /// The pipeline's serialized context record.
    pub record: Value,
    /// Whether `expires_at` is at or before the statement time.
    pub expired: bool,
}

/// One expired context found by the cross-tenant sweep.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ExpiredCodeReviewContext {
    /// Tenant that owns the row.
    pub data_tenant_id: Uuid,
    /// The pipeline's serialized caller identity.
    pub identity: Value,
    /// The pipeline's serialized context record.
    pub record: Value,
}

/// Read the pointer of `scope`, if one was published.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the read.
pub async fn code_review_snapshot_pointer(
    conn: &mut TenantConn<'_>,
    scope: CodeReviewSnapshotScope<'_>,
) -> Result<Option<Value>, SqlError> {
    Ok(sqlx::query_scalar::<_, Value>(POINTER_SQL)
        .bind(scope.repository.installation_id)
        .bind(scope.repository.repository_id)
        .bind(scope.extraction_version)
        .bind(scope.format_version)
        .bind(scope.max_file_bytes)
        .fetch_optional(&mut **conn.transaction())
        .await?)
}

/// Publish `pointer` for `scope` when the current pointer names
/// `expected_tree`, or when there is no pointer and `expected_tree` is `None`.
///
/// Returns whether the pointer moved; `false` means another publication won.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the write.
pub async fn publish_code_review_snapshot_pointer(
    conn: &mut TenantConn<'_>,
    scope: CodeReviewSnapshotScope<'_>,
    expected_tree: Option<&str>,
    pointer: &Value,
    tree: &str,
) -> Result<bool, SqlError> {
    let query = sqlx::query(if expected_tree.is_some() {
        SWAP_POINTER_SQL
    } else {
        INSERT_POINTER_SQL
    })
    .bind(scope.repository.installation_id)
    .bind(scope.repository.repository_id)
    .bind(scope.extraction_version)
    .bind(scope.format_version)
    .bind(scope.max_file_bytes)
    .bind(pointer)
    .bind(tree);
    let query = match expected_tree {
        Some(expected) => query.bind(expected),
        None => query,
    };
    let result = query.execute(&mut **conn.transaction()).await?;
    Ok(result.rows_affected() == 1)
}

/// Insert the snapshot record of `object_key`, or re-touch it at the
/// statement time.
///
/// Returns `false`, changing nothing, when a collection has claimed it.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the write.
pub async fn touch_code_review_snapshot_record(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
    object_key: &str,
) -> Result<bool, SqlError> {
    let result = sqlx::query(TOUCH_RECORD_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .bind(object_key)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Claim and return the object keys of `repository`'s records that no
/// pointer names and that were last touched at or before the statement time
/// minus `grace`.
///
/// Selection and claim are one statement, so a concurrent touch or pointer
/// swap either lands first and protects its record or finds it claimed.
/// Already-claimed records match again, so an interrupted collection retries.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the write.
pub async fn claim_unreferenced_code_review_snapshots(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
    grace: Duration,
) -> Result<Vec<String>, SqlError> {
    Ok(sqlx::query_scalar::<_, String>(CLAIM_RECORDS_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .bind(grace.as_secs_f64())
        .fetch_all(&mut **conn.transaction())
        .await?)
}

/// Delete the snapshot record of `object_key`; an absent record succeeds.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the delete.
pub async fn delete_code_review_snapshot_record(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
    object_key: &str,
) -> Result<(), SqlError> {
    sqlx::query(DELETE_RECORD_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .bind(object_key)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Delete every snapshot pointer of `repository`, leaving its records
/// unreferenced for collection.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the delete.
pub async fn delete_code_review_snapshot_pointers(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
) -> Result<(), SqlError> {
    sqlx::query(DELETE_POINTERS_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Record context `context_id`, stamping publication at the statement time
/// and expiry `lifetime` later, and return its stamps.
///
/// A context already recorded under the same id keeps its original stamps
/// and record, so a repeated publication never extends its lifetime.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the write.
pub async fn upsert_code_review_context(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
    context_id: &str,
    identity: &Value,
    record: &Value,
    lifetime: Duration,
) -> Result<CodeReviewContextStamps, SqlError> {
    Ok(
        sqlx::query_as::<_, CodeReviewContextStamps>(UPSERT_CONTEXT_SQL)
            .bind(repository.installation_id)
            .bind(repository.repository_id)
            .bind(context_id)
            .bind(identity)
            .bind(record)
            .bind(lifetime.as_secs_f64())
            .fetch_one(&mut **conn.transaction())
            .await?,
    )
}

/// Read context `context_id` and whether it has expired at the statement time.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the read.
pub async fn code_review_context(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
    context_id: &str,
) -> Result<Option<CodeReviewContextRow>, SqlError> {
    Ok(sqlx::query_as::<_, CodeReviewContextRow>(CONTEXT_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .bind(context_id)
        .fetch_optional(&mut **conn.transaction())
        .await?)
}

/// Read up to `limit` contexts of every tenant that have expired at the
/// statement time, oldest expiry first.
///
/// The sweep only enumerates: each returned row names its tenant, and the
/// caller deletes it through that tenant's [`TenantConn`].
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the read.
// tenant-isolation: cross-tenant OperatorPool
pub async fn expired_code_review_contexts(
    operator: &OperatorPool,
    limit: i64,
) -> Result<Vec<ExpiredCodeReviewContext>, SqlError> {
    Ok(
        sqlx::query_as::<_, ExpiredCodeReviewContext>(EXPIRED_CONTEXTS_SQL)
            .bind(limit)
            .fetch_all(operator.pool())
            .await?,
    )
}

/// Delete context `context_id`; an absent context succeeds.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the delete.
pub async fn delete_code_review_context(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
    context_id: &str,
) -> Result<(), SqlError> {
    sqlx::query(DELETE_CONTEXT_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .bind(context_id)
        .execute(&mut **conn.transaction())
        .await?;
    Ok(())
}

/// Delete every context of `repository`, returning the object keys their
/// records name.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the delete.
pub async fn delete_code_review_contexts(
    conn: &mut TenantConn<'_>,
    repository: CodeReviewRepository,
) -> Result<Vec<String>, SqlError> {
    Ok(sqlx::query_scalar::<_, String>(DELETE_CONTEXTS_SQL)
        .bind(repository.installation_id)
        .bind(repository.repository_id)
        .fetch_all(&mut **conn.transaction())
        .await?)
}

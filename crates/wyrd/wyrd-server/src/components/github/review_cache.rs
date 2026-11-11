//! Wyrd custody of the code-review pipeline cache.
//!
//! The pipeline's `ReviewPipeline` takes an object store and one metadata
//! owner. [`CodeReviewObjects`] keeps its immutable snapshot and context
//! objects in the configured object store under `<tenant>/code-review/`;
//! [`CodeReviewMetadata`] keeps its snapshot pointers, snapshot records and
//! context records in PostgreSQL. Every call maps the pipeline identity's
//! tenant to a Wyrd [`DataTenantId`], opens one RLS [`TenantConn`] for one
//! statement and commits it before returning, so no transaction spans GitHub
//! or object-storage IO. The cache is never accepted execution evidence:
//! expiring it never touches what Bifrost retains.

use std::sync::Arc;
use std::time::SystemTime;

use bohmian_code_review_pipeline::persistence::{ContextLifetime, ContextLookup};
use bohmian_code_review_pipeline::{
    CONTEXT_LIFETIME, ContextId, ContextRecord, ContextStore, Identity, ObjectKey, ObjectSha,
    ObjectStorage, PersistenceError, SnapshotPointer, SnapshotScope, SnapshotStore,
    StorageError as ObjectError,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::code_review_cache::{
    CodeReviewRepository, CodeReviewSnapshotScope, claim_unreferenced_code_review_snapshots,
    code_review_context, code_review_snapshot_pointer, delete_code_review_context,
    delete_code_review_contexts, delete_code_review_snapshot_pointers,
    delete_code_review_snapshot_record, expired_code_review_contexts,
    publish_code_review_snapshot_pointer, touch_code_review_snapshot_record,
    upsert_code_review_context,
};
use wyrd_sql::{SqlError, TenantConn};
use wyrd_storage::error::StorageError;
use wyrd_storage::{StorageHandle, ValidatedPath, tenant_path};

use crate::postgres::ServerPostgres;

/// The pipeline's code-review objects in Wyrd's configured object store.
///
/// A pipeline key `tenants/<tenant>/<rest>` is stored at
/// `<tenant>/code-review/<rest>`, so it sits under its tenant's prefix and
/// its scope stays the pipeline's own. A key whose tenant is not a Wyrd data
/// tenant is refused as a backend failure.
#[derive(Clone)]
pub struct CodeReviewObjects {
    /// Process storage handle.
    storage: Arc<StorageHandle>,
}

impl CodeReviewObjects {
    /// Code-review objects over `storage`.
    #[must_use]
    pub fn new(storage: Arc<StorageHandle>) -> Self {
        Self { storage }
    }

    /// The tenant-qualified object path of `key`.
    ///
    /// # Errors
    /// Returns [`ObjectError::Backend`] when the key's tenant is not a Wyrd
    /// data tenant or its scope is not a valid object path.
    fn path(key: &ObjectKey) -> Result<ValidatedPath, ObjectError> {
        let refused = || ObjectError::Backend {
            detail: format!("code-review key is outside a Wyrd tenant: {key}"),
        };
        let (tenant, rest) = key
            .as_str()
            .strip_prefix("tenants/")
            .and_then(|scoped| scoped.split_once('/'))
            .ok_or_else(refused)?;
        let tenant = tenant.parse::<DataTenantId>().map_err(|_| refused())?;
        tenant_path::code_review(tenant, rest).map_err(|_| refused())
    }
}

/// A storage failure as the pipeline's backend error. The detail names the
/// operation and path, never object bytes or credentials.
fn object_backend(error: &StorageError) -> ObjectError {
    ObjectError::Backend {
        detail: error.to_string(),
    }
}

impl ObjectStorage for CodeReviewObjects {
    /// Create `key` only when it is free.
    ///
    /// # Errors
    /// Returns [`ObjectError::AlreadyExists`] when `key` is taken, leaving it
    /// unchanged, and [`ObjectError::Backend`] for a refused key or a failed
    /// write.
    #[tracing::instrument(skip_all)]
    async fn create(&self, key: &ObjectKey, bytes: &[u8]) -> Result<(), ObjectError> {
        let path = Self::path(key)?;
        match self.storage.create_object(&path, bytes.to_vec()).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(ObjectError::AlreadyExists { key: key.clone() }),
            Err(error) => Err(object_backend(&error)),
        }
    }

    /// Read exactly `key`.
    ///
    /// # Errors
    /// Returns [`ObjectError::NotFound`] when `key` is absent and
    /// [`ObjectError::Backend`] for a refused key or any other failure.
    #[tracing::instrument(skip_all)]
    async fn read(&self, key: &ObjectKey) -> Result<Vec<u8>, ObjectError> {
        let path = Self::path(key)?;
        match self.storage.get_object(&path).await {
            Ok(bytes) => Ok(bytes),
            Err(StorageError::ObjectNotFound { .. }) => {
                Err(ObjectError::NotFound { key: key.clone() })
            }
            Err(error) => Err(object_backend(&error)),
        }
    }

    /// Delete `key`; an absent key succeeds.
    ///
    /// # Errors
    /// Returns [`ObjectError::Backend`] for a refused key or a failed delete.
    #[tracing::instrument(skip_all)]
    async fn delete(&self, key: &ObjectKey) -> Result<(), ObjectError> {
        let path = Self::path(key)?;
        self.storage
            .delete_object(&path)
            .await
            .map_err(|error| object_backend(&error))
    }
}

/// The pipeline's snapshot and context metadata in Wyrd's PostgreSQL.
///
/// Rows are scoped by the identity's tenant through RLS, and by its
/// installation and numeric repository id; owner and name are labels, so a
/// rename keeps its rows. The expiry sweep alone reads across tenants, on
/// the operator pool, and refuses a row whose stored identity names another
/// tenant than the row's.
#[derive(Clone)]
pub struct CodeReviewMetadata {
    /// Composed server Postgres handle.
    postgres: Arc<ServerPostgres>,
}

/// A metadata failure as the pipeline's backend error.
fn backend(detail: impl ToString) -> PersistenceError {
    PersistenceError::Backend {
        detail: detail.to_string(),
    }
}

/// Serialize a pipeline value for a JSONB column.
///
/// # Errors
/// Returns [`PersistenceError::Backend`] when serialization fails.
fn to_json(value: &impl Serialize) -> Result<serde_json::Value, PersistenceError> {
    serde_json::to_value(value).map_err(backend)
}

/// Deserialize a stored pipeline value, re-validating it.
///
/// # Errors
/// Returns [`PersistenceError::Backend`] when the stored value is not valid.
fn from_json<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, PersistenceError> {
    serde_json::from_value(value).map_err(backend)
}

/// A stored object key, re-validated against the pipeline's layouts.
///
/// # Errors
/// Returns [`PersistenceError::Backend`] when the text is not a pipeline key.
fn object_key(text: String) -> Result<ObjectKey, PersistenceError> {
    from_json(serde_json::Value::String(text))
}

/// The Wyrd tenant of `identity`.
///
/// # Errors
/// Returns [`PersistenceError::Backend`] when the tenant is not a Wyrd data
/// tenant id.
fn tenant(identity: &Identity) -> Result<DataTenantId, PersistenceError> {
    identity
        .tenant
        .as_str()
        .parse()
        .map_err(|_| backend("code-review identity tenant is not a Wyrd data tenant"))
}

/// The installation and numeric repository id of `identity`.
///
/// # Errors
/// Returns [`PersistenceError::Backend`] when an id exceeds `BIGINT`.
fn repository(identity: &Identity) -> Result<CodeReviewRepository, PersistenceError> {
    let bigint = |id: u64| i64::try_from(id).map_err(|_| backend(format!("{id} exceeds bigint")));
    Ok(CodeReviewRepository {
        installation_id: bigint(identity.installation.get())?,
        repository_id: bigint(identity.repository.id())?,
    })
}

impl CodeReviewMetadata {
    /// Code-review metadata over `postgres`.
    #[must_use]
    pub fn new(postgres: Arc<ServerPostgres>) -> Self {
        Self { postgres }
    }

    /// Run one statement on `identity`'s tenant connection and commit it.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] when the tenant is not a Wyrd
    /// tenant, an id exceeds `BIGINT`, or Postgres fails.
    async fn tenant<T>(
        &self,
        identity: &Identity,
        statement: impl AsyncFnOnce(&mut TenantConn<'_>, CodeReviewRepository) -> Result<T, SqlError>,
    ) -> Result<T, PersistenceError> {
        let repository = repository(identity)?;
        let mut conn = self
            .postgres
            .tenant_conn(tenant(identity)?)
            .await
            .map_err(backend)?;
        let value = statement(&mut conn, repository).await.map_err(backend)?;
        conn.commit().await.map_err(backend)?;
        Ok(value)
    }
}

/// The SQL scope of `scope`.
///
/// # Errors
/// Returns [`PersistenceError::Backend`] when an id or the file ceiling
/// exceeds `BIGINT`.
fn sql_scope(
    scope: &SnapshotScope,
    repository: CodeReviewRepository,
) -> Result<CodeReviewSnapshotScope<'_>, PersistenceError> {
    Ok(CodeReviewSnapshotScope {
        repository,
        extraction_version: &scope.extraction_version,
        format_version: i64::from(scope.format_version),
        max_file_bytes: i64::try_from(scope.max_file_bytes)
            .map_err(|_| backend("file ceiling exceeds bigint"))?,
    })
}

impl SnapshotStore for CodeReviewMetadata {
    /// The current pointer of `scope`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity, a
    /// Postgres failure or an invalid stored pointer.
    #[tracing::instrument(skip_all)]
    async fn snapshot_pointer(
        &self,
        scope: &SnapshotScope,
    ) -> Result<Option<SnapshotPointer>, PersistenceError> {
        let repository = repository(&scope.identity)?;
        let sql = sql_scope(scope, repository)?;
        self.tenant(&scope.identity, async |conn, _| {
            code_review_snapshot_pointer(conn, sql).await
        })
        .await?
        .map(from_json)
        .transpose()
    }

    /// Publish `pointer` when the current pointer's tree is `expected`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity or a
    /// Postgres failure.
    #[tracing::instrument(skip_all)]
    async fn publish_snapshot_pointer(
        &self,
        scope: &SnapshotScope,
        expected: Option<&ObjectSha>,
        pointer: &SnapshotPointer,
    ) -> Result<bool, PersistenceError> {
        let repository = repository(&scope.identity)?;
        let sql = sql_scope(scope, repository)?;
        let json = to_json(pointer)?;
        self.tenant(&scope.identity, async |conn, _| {
            publish_code_review_snapshot_pointer(
                conn,
                sql,
                expected.map(ObjectSha::as_str),
                &json,
                pointer.tree.as_str(),
            )
            .await
        })
        .await
    }

    /// Insert or re-touch the record of `key`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Collecting`] when a collection claimed it
    /// and [`PersistenceError::Backend`] on a refused identity or a Postgres
    /// failure.
    #[tracing::instrument(skip_all)]
    async fn upsert_record(
        &self,
        identity: &Identity,
        key: &ObjectKey,
    ) -> Result<(), PersistenceError> {
        let touched = self
            .tenant(identity, async |conn, repository| {
                touch_code_review_snapshot_record(conn, repository, key.as_str()).await
            })
            .await?;
        if touched {
            Ok(())
        } else {
            Err(PersistenceError::Collecting { key: key.clone() })
        }
    }

    /// Claim and return the unreferenced records untouched within `grace`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity, a
    /// Postgres failure or an invalid stored key.
    #[tracing::instrument(skip_all)]
    async fn unreferenced(
        &self,
        identity: &Identity,
        grace: std::time::Duration,
    ) -> Result<Vec<ObjectKey>, PersistenceError> {
        self.tenant(identity, async |conn, repository| {
            claim_unreferenced_code_review_snapshots(conn, repository, grace).await
        })
        .await?
        .into_iter()
        .map(object_key)
        .collect()
    }

    /// Delete the record of `key`; an absent record succeeds.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity or a
    /// Postgres failure.
    #[tracing::instrument(skip_all)]
    async fn delete_record(
        &self,
        identity: &Identity,
        key: &ObjectKey,
    ) -> Result<(), PersistenceError> {
        self.tenant(identity, async |conn, repository| {
            delete_code_review_snapshot_record(conn, repository, key.as_str()).await
        })
        .await
    }

    /// Delete every pointer of `identity`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity or a
    /// Postgres failure.
    #[tracing::instrument(skip_all)]
    async fn remove_pointers(&self, identity: &Identity) -> Result<(), PersistenceError> {
        self.tenant(identity, async |conn, repository| {
            delete_code_review_snapshot_pointers(conn, repository).await
        })
        .await
    }
}

impl ContextStore for CodeReviewMetadata {
    /// Record `record`, keeping an existing record's original stamps.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity or a
    /// Postgres failure.
    #[tracing::instrument(skip_all)]
    async fn upsert_context(
        &self,
        identity: &Identity,
        record: &ContextRecord,
    ) -> Result<ContextLifetime, PersistenceError> {
        let (identity_json, record_json) = (to_json(identity)?, to_json(record)?);
        let stamps = self
            .tenant(identity, async |conn, repository| {
                upsert_code_review_context(
                    conn,
                    repository,
                    record.id.as_str(),
                    &identity_json,
                    &record_json,
                    CONTEXT_LIFETIME,
                )
                .await
            })
            .await?;
        Ok(ContextLifetime {
            published_at: SystemTime::from(stamps.published_at),
            expires_at: SystemTime::from(stamps.expires_at),
        })
    }

    /// Look up `id`, judging expiry on the database clock.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity, a
    /// Postgres failure or an invalid stored record.
    #[tracing::instrument(skip_all)]
    async fn context(
        &self,
        identity: &Identity,
        id: &ContextId,
    ) -> Result<ContextLookup, PersistenceError> {
        let row = self
            .tenant(identity, async |conn, repository| {
                code_review_context(conn, repository, id.as_str()).await
            })
            .await?;
        Ok(match row {
            None => ContextLookup::Absent,
            Some(row) if row.expired => ContextLookup::Expired,
            Some(row) => ContextLookup::Live(from_json(row.record)?),
        })
    }

    /// Up to `limit` expired contexts across tenants.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] when no operator pool is
    /// configured, Postgres fails, a stored value is invalid, or a row's
    /// stored identity names another tenant than the row's.
    #[tracing::instrument(skip_all, fields(limit))]
    async fn expired_contexts(
        &self,
        limit: usize,
    ) -> Result<Vec<(Identity, ContextRecord)>, PersistenceError> {
        let operator = self
            .postgres
            .operator_pool()
            .ok_or_else(|| backend("code-review context expiry needs the operator pool"))?;
        let rows =
            expired_code_review_contexts(&operator, i64::try_from(limit).unwrap_or(i64::MAX))
                .await
                .map_err(backend)?;
        rows.into_iter()
            .map(|row| {
                let identity: Identity = from_json(row.identity)?;
                if tenant(&identity)?.as_uuid() != row.data_tenant_id {
                    return Err(backend("code-review context identity names another tenant"));
                }
                Ok((identity, from_json(row.record)?))
            })
            .collect()
    }

    /// Delete `id`; an absent id succeeds.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity or a
    /// Postgres failure.
    #[tracing::instrument(skip_all)]
    async fn delete_context(
        &self,
        identity: &Identity,
        id: &ContextId,
    ) -> Result<(), PersistenceError> {
        self.tenant(identity, async |conn, repository| {
            delete_code_review_context(conn, repository, id.as_str()).await
        })
        .await
    }

    /// Delete every context of `identity`, returning their object keys.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity, a
    /// Postgres failure or an invalid stored key.
    #[tracing::instrument(skip_all)]
    async fn remove_repository(
        &self,
        identity: &Identity,
    ) -> Result<Vec<ObjectKey>, PersistenceError> {
        self.tenant(identity, async |conn, repository| {
            delete_code_review_contexts(conn, repository).await
        })
        .await?
        .into_iter()
        .map(object_key)
        .collect()
    }
}

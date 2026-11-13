//! Wyrd custody of the code-review pipeline cache.
//!
//! The pipeline's `ReviewPipeline` takes an object store and one metadata
//! owner. [`CodeReviewObjects`] keeps its immutable snapshot and context
//! objects in the configured object store under `<tenant>/code-review/`;
//! [`CodeReviewMetadata`] keeps its snapshot pointers, snapshot records and
//! context records in PostgreSQL. Every call maps the pipeline identity's
//! tenant to a Wyrd [`DataTenantId`] and runs on an RLS [`TenantConn`]. A
//! read takes its own short transaction. A write is queued and group
//! committed with the other writes of its tenant waiting at the same time,
//! and its caller is answered only after that transaction commits; see
//! [`CodeReviewMetadata`]. No transaction spans GitHub or object-storage IO.
//! The cache is never accepted execution evidence: expiring it never touches
//! what Bifrost retains.

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
use tokio::sync::{mpsc, oneshot};
use wyrd_spec::DataTenantId;
use wyrd_sql::queries::code_review_cache::{
    CodeReviewContextStamps, CodeReviewRepository, CodeReviewSnapshotScope,
    claim_unreferenced_code_review_snapshots, code_review_context, code_review_snapshot_pointer,
    delete_code_review_context, delete_code_review_contexts, delete_code_review_snapshot_pointers,
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
    /// An interrupted create may have written; a repeat then returns
    /// [`ObjectError::AlreadyExists`], which proves existence, not equal
    /// bytes, as [`StorageHandle::create_object`] documents.
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

    /// Delete `key`; an absent key succeeds, so an interrupted delete that
    /// already removed it repeats as success.
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

/// Flush workers, and so the most code-review metadata transactions open at
/// once. A tenant's writes always go to the same worker, so two of its
/// batches never hold row locks at the same time.
const WORKERS: usize = 4;

/// Most queued writes one worker commits in one transaction.
const BATCH: usize = 64;

/// Most writes one worker holds queued; a writer beyond it waits for room,
/// holding no connection or lock.
const PENDING: usize = 256;

/// The pipeline's snapshot and context metadata in Wyrd's PostgreSQL.
///
/// Rows are scoped by the identity's tenant through RLS, and by its
/// installation and numeric repository id; owner and name are labels, so a
/// rename keeps its rows. The expiry sweep alone reads across tenants, on
/// the operator pool, and refuses a row whose stored identity names another
/// tenant than the row's.
///
/// Reads run at once on their own short transaction. Writes are group
/// committed: each queues its statement on its tenant's worker, and a free
/// worker takes everything queued for it, up to [`BATCH`], and commits each
/// tenant's share in one transaction. Under load one transaction carries
/// many writes; a lone write commits as soon as its worker is free, never
/// waiting for another to arrive. Each caller is answered with its own
/// statement's result after the commit; a failed batch answers every one of
/// its callers with the failure and applies none of them. Statements of one
/// batch run in arrival order, which is one valid order of the concurrent
/// calls, so every write keeps the result it has alone.
#[derive(Clone)]
pub struct CodeReviewMetadata {
    /// Composed server Postgres handle.
    postgres: Arc<ServerPostgres>,
    /// Each worker's queue, indexed by tenant.
    workers: Arc<[mpsc::Sender<Queued>]>,
}

/// One queued metadata write.
struct Queued {
    /// The tenant whose transaction applies it.
    tenant: DataTenantId,
    /// The statement.
    write: Write,
    /// Receives the statement's result after its batch commits, or the
    /// batch's failure.
    reply: oneshot::Sender<Result<Written, Arc<str>>>,
}

/// One metadata write statement, owning its arguments.
enum Write {
    /// Publish a snapshot pointer when its tree is still `expected`.
    Publish {
        /// The scope's repository.
        repository: CodeReviewRepository,
        /// The scope's extraction version.
        extraction_version: String,
        /// The scope's format version.
        format_version: i64,
        /// The scope's file ceiling.
        max_file_bytes: i64,
        /// The tree the current pointer must name.
        expected: Option<String>,
        /// The new pointer.
        pointer: serde_json::Value,
        /// The new pointer's tree.
        tree: String,
    },
    /// Insert or re-touch a snapshot record.
    Touch(CodeReviewRepository, String),
    /// Claim unreferenced snapshot records older than the grace period.
    Claim(CodeReviewRepository, std::time::Duration),
    /// Delete one snapshot record.
    DeleteRecord(CodeReviewRepository, String),
    /// Delete every snapshot pointer of a repository.
    RemovePointers(CodeReviewRepository),
    /// Record a context, keeping existing stamps.
    UpsertContext {
        /// The context's repository.
        repository: CodeReviewRepository,
        /// Context id.
        id: String,
        /// Encoded identity.
        identity: serde_json::Value,
        /// Encoded record.
        record: serde_json::Value,
    },
    /// Delete one context.
    DeleteContext(CodeReviewRepository, String),
    /// Delete every context of a repository.
    RemoveContexts(CodeReviewRepository),
}

/// What one write statement returned.
enum Written {
    /// A condition held: a published pointer or a touched record.
    Flag(bool),
    /// Object keys claimed or removed.
    Keys(Vec<String>),
    /// A context's stamps.
    Stamps(CodeReviewContextStamps),
    /// Nothing to report.
    Done,
}

impl Write {
    /// Run this statement on `conn`.
    ///
    /// # Errors
    /// Returns the statement's [`SqlError`].
    async fn apply(&self, conn: &mut TenantConn<'_>) -> Result<Written, SqlError> {
        Ok(match self {
            Self::Publish {
                repository,
                extraction_version,
                format_version,
                max_file_bytes,
                expected,
                pointer,
                tree,
            } => Written::Flag(
                publish_code_review_snapshot_pointer(
                    conn,
                    CodeReviewSnapshotScope {
                        repository: *repository,
                        extraction_version,
                        format_version: *format_version,
                        max_file_bytes: *max_file_bytes,
                    },
                    expected.as_deref(),
                    pointer,
                    tree,
                )
                .await?,
            ),
            Self::Touch(repository, key) => {
                Written::Flag(touch_code_review_snapshot_record(conn, *repository, key).await?)
            }
            Self::Claim(repository, grace) => Written::Keys(
                claim_unreferenced_code_review_snapshots(conn, *repository, *grace).await?,
            ),
            Self::DeleteRecord(repository, key) => {
                delete_code_review_snapshot_record(conn, *repository, key).await?;
                Written::Done
            }
            Self::RemovePointers(repository) => {
                delete_code_review_snapshot_pointers(conn, *repository).await?;
                Written::Done
            }
            Self::UpsertContext {
                repository,
                id,
                identity,
                record,
            } => Written::Stamps(
                upsert_code_review_context(
                    conn,
                    *repository,
                    id,
                    identity,
                    record,
                    CONTEXT_LIFETIME,
                )
                .await?,
            ),
            Self::DeleteContext(repository, id) => {
                delete_code_review_context(conn, *repository, id).await?;
                Written::Done
            }
            Self::RemoveContexts(repository) => {
                Written::Keys(delete_code_review_contexts(conn, *repository).await?)
            }
        })
    }
}

/// One worker: commits its tenants' queued writes until every handle is
/// dropped and its queue is empty.
struct Worker {
    /// Composed server Postgres handle.
    postgres: Arc<ServerPostgres>,
    /// This worker's queue.
    queue: mpsc::Receiver<Queued>,
}

impl Worker {
    /// Take everything queued, up to [`BATCH`], commit each tenant's share,
    /// and repeat.
    async fn run(mut self) {
        let mut taken = Vec::with_capacity(BATCH);
        while self.queue.recv_many(&mut taken, BATCH).await > 0 {
            taken.sort_by_key(|queued| queued.tenant);
            while let Some(tenant) = taken.last().map(|queued| queued.tenant) {
                let first = taken.partition_point(|queued| queued.tenant < tenant);
                let batch = taken.split_off(first);
                self.commit(tenant, batch).await;
            }
        }
    }

    /// Apply `batch` in one transaction of `tenant` and answer each write.
    ///
    /// Every write is answered after the commit with its own result; any
    /// failure answers every write of the batch with it, and nothing applies.
    #[tracing::instrument(
        skip_all,
        fields(operation = "code_review.metadata.commit", tenant_id = %tenant, batch = batch.len())
    )]
    async fn commit(&self, tenant: DataTenantId, batch: Vec<Queued>) {
        let applied = async {
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let mut written = Vec::with_capacity(batch.len());
            for queued in &batch {
                written.push(queued.write.apply(&mut conn).await?);
            }
            conn.commit().await?;
            Ok::<_, SqlError>(written)
        }
        .await;
        match applied {
            Ok(written) => {
                tracing::debug!("code-review metadata batch committed");
                for (queued, written) in batch.into_iter().zip(written) {
                    let _ = queued.reply.send(Ok(written));
                }
            }
            Err(error) => {
                tracing::warn!(%error, "a code-review metadata batch failed; none of its writes applied");
                let failure: Arc<str> = error.to_string().into();
                for queued in batch {
                    let _ = queued.reply.send(Err(Arc::clone(&failure)));
                }
            }
        }
    }
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
    /// Code-review metadata over `postgres`, starting its [`WORKERS`] write
    /// workers. They stop once every clone is dropped and their queues are
    /// empty.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime, which spawning requires.
    #[must_use]
    pub fn new(postgres: Arc<ServerPostgres>) -> Self {
        let workers = (0..WORKERS)
            .map(|_| {
                let (sender, queue) = mpsc::channel(PENDING);
                tokio::spawn(
                    Worker {
                        postgres: Arc::clone(&postgres),
                        queue,
                    }
                    .run(),
                );
                sender
            })
            .collect();
        Self { postgres, workers }
    }

    /// Run one read on `identity`'s tenant connection in its own
    /// transaction.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] when the tenant is not a Wyrd
    /// tenant, an id exceeds `BIGINT`, or Postgres fails.
    async fn read<T>(
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

    /// Queue `write` of `identity`'s tenant and return its result once its
    /// batch committed.
    ///
    /// This is every metadata write's commit boundary, so each operation is
    /// atomic: it lands whole or not at all.
    ///
    /// # Cancellation
    /// A cancelled call or a lost commit reply can leave the statement
    /// committed while the caller sees an error or nothing. The caller learns
    /// the outcome only by reading or repeating; each operation documents what
    /// a repeat after a committed attempt returns.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] when the tenant is not a Wyrd
    /// tenant, the batch holding the write failed, or the workers stopped.
    async fn write(&self, identity: &Identity, write: Write) -> Result<Written, PersistenceError> {
        let tenant = tenant(identity)?;
        let worker = &self.workers
            [usize::try_from(tenant.as_uuid().as_u128() % WORKERS as u128).unwrap_or_default()];
        let (reply, written) = oneshot::channel();
        worker
            .send(Queued {
                tenant,
                write,
                reply,
            })
            .await
            .map_err(|_| backend("the code-review metadata writer stopped"))?;
        written
            .await
            .map_err(|_| backend("the code-review metadata writer stopped"))?
            .map_err(|failure| backend(format!("the batched metadata write failed: {failure}")))
    }
}

/// A write result of another shape than its statement returns.
fn unexpected() -> PersistenceError {
    backend("a code-review metadata write returned another result shape")
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
        self.read(&scope.identity, async |conn, _| {
            code_review_snapshot_pointer(conn, sql).await
        })
        .await?
        .map(from_json)
        .transpose()
    }

    /// Publish `pointer` when the current pointer's tree is `expected`.
    ///
    /// An interrupted publish may have moved the pointer; a repeat then finds
    /// another tree than `expected` and returns `false`, so the caller reads
    /// the pointer to learn which publication stands.
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
        let write = Write::Publish {
            repository,
            extraction_version: sql.extraction_version.to_owned(),
            format_version: sql.format_version,
            max_file_bytes: sql.max_file_bytes,
            expected: expected.map(|tree| tree.as_str().to_owned()),
            pointer: to_json(pointer)?,
            tree: pointer.tree.as_str().to_owned(),
        };
        match self.write(&scope.identity, write).await? {
            Written::Flag(published) => Ok(published),
            _ => Err(unexpected()),
        }
    }

    /// Insert or re-touch the record of `key`.
    ///
    /// An interrupted call may have touched the record; a repeat touches it
    /// again, or reports [`PersistenceError::Collecting`] if a collection
    /// claimed it in between.
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
        let write = Write::Touch(repository(identity)?, key.as_str().to_owned());
        match self.write(identity, write).await? {
            Written::Flag(true) => Ok(()),
            Written::Flag(false) => Err(PersistenceError::Collecting { key: key.clone() }),
            _ => Err(unexpected()),
        }
    }

    /// Claim and return the unreferenced records untouched within `grace`.
    ///
    /// An interrupted claim may have persisted; claimed records match again,
    /// so a repeat returns them and the collection retries.
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
        match self
            .write(identity, Write::Claim(repository(identity)?, grace))
            .await?
        {
            Written::Keys(keys) => keys.into_iter().map(object_key).collect(),
            _ => Err(unexpected()),
        }
    }

    /// Delete the record of `key`; an absent record succeeds, so a repeat
    /// after an interrupted delete succeeds.
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
        let write = Write::DeleteRecord(repository(identity)?, key.as_str().to_owned());
        self.write(identity, write).await.map(drop)
    }

    /// Delete every pointer of `identity`; a repeat after an interrupted
    /// delete succeeds.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity or a
    /// Postgres failure.
    #[tracing::instrument(skip_all)]
    async fn remove_pointers(&self, identity: &Identity) -> Result<(), PersistenceError> {
        self.write(identity, Write::RemovePointers(repository(identity)?))
            .await
            .map(drop)
    }
}

impl ContextStore for CodeReviewMetadata {
    /// Record `record`, keeping an existing record's original stamps.
    ///
    /// A repeat after an interrupted call keeps the original record and
    /// returns its stamps unchanged.
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
        let write = Write::UpsertContext {
            repository: repository(identity)?,
            id: record.id.as_str().to_owned(),
            identity: to_json(identity)?,
            record: to_json(record)?,
        };
        let Written::Stamps(stamps) = self.write(identity, write).await? else {
            return Err(unexpected());
        };
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
            .read(identity, async |conn, repository| {
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

    /// Delete `id`; an absent id succeeds, so a repeat after an interrupted
    /// delete succeeds.
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
        let write = Write::DeleteContext(repository(identity)?, id.as_str().to_owned());
        self.write(identity, write).await.map(drop)
    }

    /// Delete every context of `identity`, returning their object keys.
    ///
    /// A repeat after an interrupted delete succeeds but returns only the
    /// keys of contexts still recorded. Keys removed by the interrupted call
    /// are not returned again, so their objects are no longer named by this
    /// result.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Backend`] on a refused identity, a
    /// Postgres failure or an invalid stored key.
    #[tracing::instrument(skip_all)]
    async fn remove_repository(
        &self,
        identity: &Identity,
    ) -> Result<Vec<ObjectKey>, PersistenceError> {
        match self
            .write(identity, Write::RemoveContexts(repository(identity)?))
            .await?
        {
            Written::Keys(keys) => keys.into_iter().map(object_key).collect(),
            _ => Err(unexpected()),
        }
    }
}

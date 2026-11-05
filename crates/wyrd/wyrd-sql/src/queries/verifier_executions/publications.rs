//! Named publications of leased executions' invocation attempts.
//!
//! One identity authority serves every publication kind. A call's receipt is
//! recoverable by the lease holder from its stored row alone, before any
//! scratch or storage work,
//! and acceptance serializes on the execution row: the writer locks it
//! `FOR NO KEY UPDATE`, as every page-`sequence` writer does, then reads the
//! name's holder on a fresh statement snapshot, so a concurrent identical
//! call that waited on the lock replays the committed winner instead of
//! reporting a false conflict. The same lock charges the execution's
//! cumulative publication count and retained bytes, which survive reclaim
//! because they live on the execution row. Each accepted file keeps the
//! completed File descriptor shared completion returned, privately beside
//! the public manifest, so readers locate its bytes without rediscovery.
// raw-query grep allowlist: verifier execution tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use serde_json::Value;
use sqlx::Error as SqlxError;
use sqlx::types::Json;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::storage::StoredObjectRef;
use wyrd_spec::verification::PublicationName;
use wyrd_spec::verification::execution::{
    Producer, Publication, PublicationContent, PublicationKind, PublicationManifest,
};

use super::inspection::kind;
use super::{ExecutionLease, VerifierExecutionQueue, column, producer, stored, timed};
use crate::TenantConn;

/// The call, arguments digest, and kind holding name `$7` in invocation
/// attempt (`$2`, `$3`, `$4`, `$5`, `$6`) of execution `$1`, read through the
/// publication identity index on the statement's own snapshot.
const HOLDER_SQL: &str = r"
    SELECT call_id, arguments_sha256, kind
      FROM wyrd.verifier_execution_publications
     WHERE execution_id = $1 AND task_id = $2
       AND item_index IS NOT DISTINCT FROM $3
       AND iteration_index IS NOT DISTINCT FROM $4
       AND attempt = $5 AND name = $6
";

/// Whether the lease `$2` still holds execution `$1` on the database clock.
const HELD_SQL: &str = r"
    SELECT EXISTS (
        SELECT 1 FROM wyrd.verifier_executions
         WHERE execution_id = $1 AND lease_token = $2 AND status = 'running'
           AND lease_expires_at > statement_timestamp()
    )
";

/// Lock a leased execution for one publication acceptance and read its
/// cumulative publication count and retained bytes.
///
/// The lease and the execution deadline must hold on the database clock. The
/// `FOR NO KEY UPDATE` lock is held to commit, serializing this execution's
/// acceptances with each other and with invocation begins, so sequences
/// commit in order and accounting is never lost to a concurrent writer.
const LOCK_SQL: &str = r"
    SELECT publication_count, publication_bytes
      FROM wyrd.verifier_executions
     WHERE execution_id = $1 AND lease_token = $2 AND status = 'running'
       AND lease_expires_at > statement_timestamp()
       AND deadline_at > statement_timestamp()
       FOR NO KEY UPDATE
";

/// Insert one accepted publication with its completed File descriptors.
const INSERT_SQL: &str = r"
    INSERT INTO wyrd.verifier_execution_publications
        (data_tenant_id, execution_id, task_id, item_index, iteration_index, attempt,
         name, kind, call_id, arguments_sha256, value, manifest, objects, accepted_at)
    VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            statement_timestamp())
";

/// Charge one accepted publication of `$2` retained bytes to execution `$1`.
const CHARGE_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET publication_count = publication_count + 1,
           publication_bytes = publication_bytes + $2
     WHERE execution_id = $1
";

/// Read one step's publications in an execution with their completed File
/// descriptors, grouped by item, iteration, and attempt, in acceptance order
/// within an attempt.
const PUBLICATIONS_SQL: &str = r"
    SELECT task_id, item_index, iteration_index, attempt, name, kind, value, manifest, objects
      FROM wyrd.verifier_execution_publications
     WHERE execution_id = $1 AND task_id = $2
     ORDER BY item_index NULLS FIRST, iteration_index NULLS FIRST, attempt, sequence
";

/// The call that asks to accept one publication.
#[derive(Debug, Clone, Copy)]
pub struct PublicationCall<'a> {
    /// The publication to accept.
    pub publication: &'a Publication,
    /// The accepting call's identity, stable across its replays.
    pub call_id: &'a str,
    /// Digest of the call's canonical arguments.
    pub arguments: &'a Sha256Hex,
    /// The completed File descriptor of each manifest file, in manifest
    /// order; empty for a JSON publication.
    pub objects: &'a [StoredObjectRef],
    /// The bytes the publication retains: a JSON value's serialized length,
    /// or the sum of its manifest's file sizes.
    pub retained_bytes: u64,
}

/// The effective cumulative ceilings of one execution's publications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationBudget {
    /// Most publications the execution may accept across all attempts.
    pub max_count: u32,
    /// Most bytes its accepted publications may retain together.
    pub max_bytes: u64,
}

/// The cumulative ceiling a refused publication would cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationLimit {
    /// [`PublicationBudget::max_count`].
    Count,
    /// [`PublicationBudget::max_bytes`].
    Bytes,
}

/// Outcome of accepting one publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationAcceptance {
    /// The publication committed once the caller commits the transaction.
    Accepted,
    /// The same call with the same arguments already committed it; the
    /// stored kind is its receipt.
    Replayed(PublicationKind),
    /// Another call, or the same call with other arguments, holds the name
    /// in this attempt.
    Conflict,
    /// Accepting it would cross a cumulative ceiling; nothing committed.
    OverBudget(PublicationLimit),
    /// The lease or deadline no longer holds; nothing committed.
    StaleLease,
}

/// One accepted publication with the completed File descriptor of each of
/// its manifest files, in manifest order.
///
/// The descriptors are private server metadata: they locate the bytes, are
/// never part of a public manifest, and are never supplied by a caller.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPublication {
    /// The publication as public reads return it.
    pub publication: Publication,
    /// The descriptor of each manifest file; empty for JSON.
    pub objects: Vec<StoredObjectRef>,
}

/// One stored publication row.
#[derive(sqlx::FromRow)]
struct PublicationRow {
    /// Producer step or requirement.
    task_id: String,
    /// Zero-based fanout item; `None` without fanout.
    item_index: Option<i32>,
    /// Zero-based repeat iteration; `None` without repeat.
    iteration_index: Option<i32>,
    /// One-based producer attempt.
    attempt: i32,
    /// Logical publication name.
    name: String,
    /// `file`, `directory`, or `json`.
    kind: String,
    /// The JSON value; `None` for files and directories.
    value: Option<Json<Value>>,
    /// The manifest; `None` for JSON.
    manifest: Option<Json<PublicationManifest>>,
    /// Completed File descriptors in manifest order; `None` for JSON.
    objects: Option<Json<Vec<StoredObjectRef>>>,
}

impl PublicationRow {
    /// Decode the stored row.
    ///
    /// # Errors
    /// Returns a decode error when an identity, index, kind, or content
    /// column is malformed, or a manifest's descriptors do not align with
    /// its files.
    fn decode(self) -> Result<StoredPublication, SqlxError> {
        let objects = self
            .objects
            .map(|Json(objects)| objects)
            .unwrap_or_default();
        let content = match (self.kind.as_str(), self.value, self.manifest) {
            ("json", Some(Json(value)), None) => PublicationContent::Json { value },
            ("file", None, Some(Json(manifest))) => PublicationContent::File { manifest },
            ("directory", None, Some(Json(manifest))) => PublicationContent::Directory { manifest },
            (kind, ..) => {
                return Err(SqlxError::Decode(
                    format!("publication kind {kind:?} does not match its content").into(),
                ));
            }
        };
        let files = match &content {
            PublicationContent::Json { .. } => 0,
            PublicationContent::File { manifest } | PublicationContent::Directory { manifest } => {
                manifest.files.len()
            }
        };
        if objects.len() != files {
            return Err(SqlxError::Decode(
                "a publication's File descriptors do not align with its manifest".into(),
            ));
        }
        Ok(StoredPublication {
            publication: Publication {
                name: stored(PublicationName::new(self.name))?,
                producer: producer(
                    self.task_id,
                    self.item_index,
                    self.iteration_index,
                    self.attempt,
                )?,
                content,
            },
            objects,
        })
    }
}

impl VerifierExecutionQueue {
    /// The receipt `call` already holds in `lease`'s execution: replayed
    /// with the stored kind when it committed this publication with these
    /// arguments, a conflict when another call or other arguments hold the
    /// name, [`PublicationAcceptance::StaleLease`] when the lease no longer
    /// holds, and `None` when the name is free.
    ///
    /// The read takes no lock and needs no scratch or storage, so a replay
    /// after a lost reply recovers its receipt from the stored row alone.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or an encode or decode
    /// error when a producer index or the stored kind is malformed.
    pub async fn publication_receipt(
        &self,
        conn: &mut TenantConn<'_>,
        lease: ExecutionLease,
        call: PublicationCall<'_>,
    ) -> Result<Option<PublicationAcceptance>, SqlxError> {
        timed("publication_receipt", async {
            let held: bool = sqlx::query_scalar(HELD_SQL)
                .bind(lease.execution_id.as_uuid())
                .bind(lease.token.0)
                .fetch_one(&mut **conn.transaction())
                .await?;
            if !held {
                return Ok(Some(PublicationAcceptance::StaleLease));
            }
            Self::holder(conn, lease.execution_id, call).await
        })
        .await
    }

    /// Accept `call`'s publication under `lease`, once, within `budget`.
    ///
    /// Every File its manifest names must already be complete. The execution
    /// row is locked first; the name's holder is then read on a fresh
    /// snapshot, so the same call with the same arguments replays as
    /// [`PublicationAcceptance::Replayed`] even when it raced the committed
    /// winner, and any other holder is a conflict. A new publication that
    /// fits the budget is inserted and charged; the caller commits.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or an encode
    /// error when a producer index, attempt, size, or content cannot be
    /// stored.
    #[tracing::instrument(skip(self, conn, call), fields(operation = "verification.executions.accept_publication", execution_id = %lease.execution_id))]
    pub async fn accept_publication(
        &self,
        conn: &mut TenantConn<'_>,
        lease: ExecutionLease,
        call: PublicationCall<'_>,
        budget: PublicationBudget,
    ) -> Result<PublicationAcceptance, SqlxError> {
        timed("accept_publication", async {
            let execution = lease.execution_id.as_uuid();
            let Some((count, bytes)): Option<(i32, i64)> = sqlx::query_as(LOCK_SQL)
                .bind(execution)
                .bind(lease.token.0)
                .fetch_optional(&mut **conn.transaction())
                .await?
            else {
                return Ok(PublicationAcceptance::StaleLease);
            };
            if let Some(held) = Self::holder(conn, lease.execution_id, call).await? {
                return Ok(held);
            }
            let retained = i64::try_from(call.retained_bytes)
                .map_err(|error| SqlxError::Encode(Box::new(error)))?;
            if i64::from(count) >= i64::from(budget.max_count) {
                return Ok(PublicationAcceptance::OverBudget(PublicationLimit::Count));
            }
            if u64::try_from(bytes.saturating_add(retained)).unwrap_or(u64::MAX) > budget.max_bytes
            {
                return Ok(PublicationAcceptance::OverBudget(PublicationLimit::Bytes));
            }
            let publication = call.publication;
            let producer = &publication.producer;
            let (kind, value, manifest, objects) = match &publication.content {
                PublicationContent::Json { value } => ("json", Some(Json(value)), None, None),
                PublicationContent::File { manifest } => {
                    ("file", None, Some(Json(manifest)), Some(Json(call.objects)))
                }
                PublicationContent::Directory { manifest } => (
                    "directory",
                    None,
                    Some(Json(manifest)),
                    Some(Json(call.objects)),
                ),
            };
            sqlx::query(INSERT_SQL)
                .bind(execution)
                .bind(producer.step_id.as_str())
                .bind(producer.item_index.map(column).transpose()?)
                .bind(producer.iteration_index.map(column).transpose()?)
                .bind(column(producer.attempt)?)
                .bind(publication.name.as_str())
                .bind(kind)
                .bind(call.call_id)
                .bind(call.arguments.to_string())
                .bind(value)
                .bind(manifest)
                .bind(objects)
                .execute(&mut **conn.transaction())
                .await?;
            sqlx::query(CHARGE_SQL)
                .bind(execution)
                .bind(retained)
                .execute(&mut **conn.transaction())
                .await?;
            Ok(PublicationAcceptance::Accepted)
        })
        .await
    }

    /// Read every publication `step` accepted in `execution`, across its
    /// items, iterations, and attempts, grouped in that order and in
    /// acceptance order within an attempt, each with its completed File
    /// descriptors.
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored row is malformed.
    pub async fn publications(
        &self,
        conn: &mut TenantConn<'_>,
        execution: ExecutionId,
        step: &wyrd_spec::verification::TaskId,
    ) -> Result<Vec<StoredPublication>, SqlxError> {
        // ponytail: unpaged; one step's publications are bounded by the
        // execution's publication count ceiling. Page when public listing reads them.
        timed("publications", async {
            let rows: Vec<PublicationRow> = sqlx::query_as(PUBLICATIONS_SQL)
                .bind(execution.as_uuid())
                .bind(step.as_str())
                .fetch_all(&mut **conn.transaction())
                .await?;
            rows.into_iter().map(PublicationRow::decode).collect()
        })
        .await
    }

    /// The receipt of `call`'s name in `execution`, read on the statement's
    /// own snapshot: replayed, a conflict, or `None` when the name is free.
    ///
    /// # Errors
    /// As [`Self::publication_receipt`].
    async fn holder(
        conn: &mut TenantConn<'_>,
        execution: ExecutionId,
        call: PublicationCall<'_>,
    ) -> Result<Option<PublicationAcceptance>, SqlxError> {
        let producer: &Producer = &call.publication.producer;
        let held: Option<(String, String, String)> = sqlx::query_as(HOLDER_SQL)
            .bind(execution.as_uuid())
            .bind(producer.step_id.as_str())
            .bind(producer.item_index.map(column).transpose()?)
            .bind(producer.iteration_index.map(column).transpose()?)
            .bind(column(producer.attempt)?)
            .bind(call.publication.name.as_str())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        held.map(|(call_id, arguments, stored_kind)| {
            Ok(
                if call_id == call.call_id && arguments == call.arguments.to_string() {
                    PublicationAcceptance::Replayed(kind(&stored_kind)?)
                } else {
                    PublicationAcceptance::Conflict
                },
            )
        })
        .transpose()
    }
}

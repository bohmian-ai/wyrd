//! Stable pages of one execution's invocation attempts and publications.
//!
//! A listing's first page fixes its cut, the highest insertion `sequence`
//! committed then; every later page of that listing reads strictly after
//! its predecessor and never past the cut, so a listing has no gap or
//! duplicate while more entries commit. Every writer allocating an
//! execution's sequence holds its row `FOR NO KEY UPDATE` to commit, and the
//! cut is read only after [`CUT_LOCK_SQL`] waits out such a writer, so no
//! lower sequence can commit after a cut that excludes it. Publication pages carry names,
//! kinds, and producers only, never content.
// raw-query grep allowlist: verifier execution tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::Error as SqlxError;
use sqlx::types::Json;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::PublicationName;
use wyrd_spec::verification::execution::{InvocationSettlement, Producer, PublicationKind};

use super::{VerifierExecutionQueue, producer, stored, timed};
use crate::TenantConn;

/// Wait until no writer of execution `$1` holds an uncommitted sequence.
///
/// The share lock conflicts with the writers' `FOR NO KEY UPDATE` and is
/// released at the reader's commit; the cut is then read by a later
/// statement, whose snapshot includes every writer this one waited for.
const CUT_LOCK_SQL: &str = r"
    SELECT 1 FROM wyrd.verifier_executions WHERE execution_id = $1 FOR SHARE
";

/// The highest invocation sequence `$1` has committed.
const INVOCATION_CUT_SQL: &str = r"
    SELECT coalesce(max(sequence), 0)
      FROM wyrd.verifier_execution_invocations
     WHERE execution_id = $1
";

/// Up to `$4` invocation attempts of `$1` after sequence `$2`, through `$3`.
const INVOCATION_PAGE_SQL: &str = r"
    SELECT sequence, task_id, item_index, iteration_index, attempt, settlement
      FROM wyrd.verifier_execution_invocations
     WHERE execution_id = $1 AND sequence > $2 AND sequence <= $3
     ORDER BY sequence
     LIMIT $4
";

/// The highest publication sequence `$1` has committed.
const PUBLICATION_CUT_SQL: &str = r"
    SELECT coalesce(max(sequence), 0)
      FROM wyrd.verifier_execution_publications
     WHERE execution_id = $1
";

/// Up to `$4` publication names of `$1` after sequence `$2`, through `$3`.
const PUBLICATION_PAGE_SQL: &str = r"
    SELECT sequence, task_id, item_index, iteration_index, attempt, name, kind
      FROM wyrd.verifier_execution_publications
     WHERE execution_id = $1 AND sequence > $2 AND sequence <= $3
     ORDER BY sequence
     LIMIT $4
";

/// Where a page of one listing starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePosition {
    /// The listing's cut: the highest sequence its first page saw.
    pub through: i64,
    /// The sequence of the last entry already returned; `0` before any.
    pub after: i64,
}

/// One page of a listing.
#[derive(Debug, Clone, PartialEq)]
pub struct Page<T> {
    /// The listing's cut, which every continuation keeps.
    pub through: i64,
    /// Each entry with its sequence, in sequence order.
    pub entries: Vec<(i64, T)>,
}

/// One invocation attempt as a page lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct InvocationEntry {
    /// The step or requirement, item, iteration, and attempt.
    pub producer: Producer,
    /// Its committed settlement; `None` while unsettled.
    pub settlement: Option<InvocationSettlement>,
}

/// One publication as a page lists it, without its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationEntry {
    /// The logical name.
    pub name: PublicationName,
    /// The content kind.
    pub kind: PublicationKind,
    /// The accepting invocation attempt.
    pub producer: Producer,
}

/// One stored invocation attempt with its sequence.
#[derive(sqlx::FromRow)]
struct InvocationRow {
    /// Insertion order within the execution's listing.
    sequence: i64,
    /// Step or requirement invoked.
    task_id: String,
    /// Zero-based fanout item; `None` without fanout.
    item_index: Option<i32>,
    /// Zero-based repeat iteration; `None` without repeat.
    iteration_index: Option<i32>,
    /// One-based attempt.
    attempt: i32,
    /// Committed settlement; `None` while unsettled.
    settlement: Option<Json<InvocationSettlement>>,
}

/// One stored publication's name, kind, and producer with its sequence.
#[derive(sqlx::FromRow)]
struct PublicationEntryRow {
    /// Insertion order within the execution's listing.
    sequence: i64,
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
}

/// A stored publication kind.
///
/// # Errors
/// Returns a decode error for a kind the table does not admit.
pub(super) fn kind(kind: &str) -> Result<PublicationKind, SqlxError> {
    match kind {
        "file" => Ok(PublicationKind::File),
        "directory" => Ok(PublicationKind::Directory),
        "json" => Ok(PublicationKind::Json),
        other => Err(SqlxError::Decode(
            format!("unknown publication kind {other:?}").into(),
        )),
    }
}

/// A new listing's cut of `execution`: `cut_sql`'s highest committed
/// sequence, read once [`CUT_LOCK_SQL`] has waited out every writer still
/// holding an uncommitted one.
///
/// # Errors
/// Returns the database error when either read fails.
async fn cut(
    conn: &mut TenantConn<'_>,
    execution: ExecutionId,
    cut_sql: &'static str,
) -> Result<i64, SqlxError> {
    sqlx::query(CUT_LOCK_SQL)
        .bind(execution.as_uuid())
        .execute(&mut **conn.transaction())
        .await?;
    sqlx::query_scalar(cut_sql)
        .bind(execution.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
}

impl VerifierExecutionQueue {
    /// Up to `limit` of `execution`'s invocation attempts from `position`,
    /// or from a new cut when `None`.
    ///
    /// An unknown or other-tenant execution reads as an empty page.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when a
    /// stored identity or settlement is malformed.
    pub async fn invocation_page(
        &self,
        conn: &mut TenantConn<'_>,
        execution: ExecutionId,
        position: Option<PagePosition>,
        limit: u32,
    ) -> Result<Page<InvocationEntry>, SqlxError> {
        timed("invocation_page", async {
            let position = match position {
                Some(position) => position,
                None => PagePosition {
                    through: cut(conn, execution, INVOCATION_CUT_SQL).await?,
                    after: 0,
                },
            };
            let rows: Vec<InvocationRow> = sqlx::query_as(INVOCATION_PAGE_SQL)
                .bind(execution.as_uuid())
                .bind(position.after)
                .bind(position.through)
                .bind(i64::from(limit))
                .fetch_all(&mut **conn.transaction())
                .await?;
            let entries = rows
                .into_iter()
                .map(|row| {
                    Ok((
                        row.sequence,
                        InvocationEntry {
                            producer: producer(
                                row.task_id,
                                row.item_index,
                                row.iteration_index,
                                row.attempt,
                            )?,
                            settlement: row.settlement.map(|Json(settlement)| settlement),
                        },
                    ))
                })
                .collect::<Result<_, SqlxError>>()?;
            Ok(Page {
                through: position.through,
                entries,
            })
        })
        .await
    }

    /// Up to `limit` of `execution`'s publication names from `position`, or
    /// from a new cut when `None`.
    ///
    /// An unknown or other-tenant execution reads as an empty page.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when a
    /// stored identity, name, or kind is malformed.
    pub async fn publication_page(
        &self,
        conn: &mut TenantConn<'_>,
        execution: ExecutionId,
        position: Option<PagePosition>,
        limit: u32,
    ) -> Result<Page<PublicationEntry>, SqlxError> {
        timed("publication_page", async {
            let position = match position {
                Some(position) => position,
                None => PagePosition {
                    through: cut(conn, execution, PUBLICATION_CUT_SQL).await?,
                    after: 0,
                },
            };
            let rows: Vec<PublicationEntryRow> = sqlx::query_as(PUBLICATION_PAGE_SQL)
                .bind(execution.as_uuid())
                .bind(position.after)
                .bind(position.through)
                .bind(i64::from(limit))
                .fetch_all(&mut **conn.transaction())
                .await?;
            let entries = rows
                .into_iter()
                .map(|row| {
                    Ok((
                        row.sequence,
                        PublicationEntry {
                            name: stored(PublicationName::new(row.name))?,
                            kind: kind(&row.kind)?,
                            producer: producer(
                                row.task_id,
                                row.item_index,
                                row.iteration_index,
                                row.attempt,
                            )?,
                        },
                    ))
                })
                .collect::<Result<_, SqlxError>>()?;
            Ok(Page {
                through: position.through,
                entries,
            })
        })
        .await
    }
}

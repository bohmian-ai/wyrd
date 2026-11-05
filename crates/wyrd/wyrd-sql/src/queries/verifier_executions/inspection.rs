//! Stable pages of one execution's invocation attempts and publications.
//!
//! A listing's first page fixes its cut, the highest insertion `sequence`
//! committed then; every later page of that listing reads strictly after
//! its predecessor and never past the cut, so a listing has no gap or
//! duplicate while more entries commit. Publication pages carry names,
//! kinds, and producers only, never content.
// raw-query grep allowlist: verifier execution tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::Error as SqlxError;
use sqlx::types::Json;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::PublicationName;
use wyrd_spec::verification::execution::{InvocationSettlement, Producer, PublicationKind};

use super::{VerifierExecutionQueue, producer, stored, timed};
use crate::TenantConn;

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
    sequence: i64,
    task_id: String,
    item_index: Option<i32>,
    iteration_index: Option<i32>,
    attempt: i32,
    settlement: Option<Json<InvocationSettlement>>,
}

/// One stored publication's name, kind, and producer with its sequence.
#[derive(sqlx::FromRow)]
struct PublicationEntryRow {
    sequence: i64,
    task_id: String,
    item_index: Option<i32>,
    iteration_index: Option<i32>,
    attempt: i32,
    name: String,
    kind: String,
}

/// A stored publication kind.
///
/// # Errors
/// Returns a decode error for a kind the table does not admit.
fn kind(kind: &str) -> Result<PublicationKind, SqlxError> {
    match kind {
        "file" => Ok(PublicationKind::File),
        "directory" => Ok(PublicationKind::Directory),
        "json" => Ok(PublicationKind::Json),
        other => Err(SqlxError::Decode(
            format!("unknown publication kind {other:?}").into(),
        )),
    }
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
                    through: sqlx::query_scalar(INVOCATION_CUT_SQL)
                        .bind(execution.as_uuid())
                        .fetch_one(&mut **conn.transaction())
                        .await?,
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
                    through: sqlx::query_scalar(PUBLICATION_CUT_SQL)
                        .bind(execution.as_uuid())
                        .fetch_one(&mut **conn.transaction())
                        .await?,
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

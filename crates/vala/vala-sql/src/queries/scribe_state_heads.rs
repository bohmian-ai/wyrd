//! Tenant-scoped heads of the keyed states a state-fenced table records.
//!
//! A head names the one writer that may advance a keyed state and the
//! revision its last accepted write recorded. [`claim`] installs a writer and
//! answers the accepted position; [`advance`] accepts one write per state in
//! the caller's batch-fence transaction. A claim or advance evaluates no
//! principal permission, so it is lineage only and appends no audit event.
// raw-query grep allowlist: every statement is fixed and tenant-bound; `vala.scribe_state_heads` post-dates the sqlx offline cache, so run `mise run sqlx:prepare` to promote them to macros.

use chrono::{DateTime, Utc};

use crate::{SqlError, TenantConn};

/// One write's claim to advance a keyed state by exactly one revision.
///
/// Carried with the batch whose rows record the new revision, and accepted
/// only together with that batch's durable fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeStateAdvance {
    /// The keyed state the batch records.
    pub key: uuid::Uuid,
    /// The writer offering the revision; only the head's current writer is
    /// accepted.
    pub writer: uuid::Uuid,
    /// The revision the batch records; accepted only directly after the
    /// head's accepted revision.
    pub revision: i64,
    /// The instant after which the writer may no longer advance the state,
    /// compared with the `PostgreSQL` clock; `None` sets no bound.
    pub not_after: Option<DateTime<Utc>>,
}

/// The accepted position of one keyed state a claim answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScribeStateHead {
    /// The accepted revision; 0 means no revision was ever accepted.
    pub revision: i64,
    /// The batch whose fence accepted `revision`; `None` at revision 0.
    pub batch_id: Option<uuid::Uuid>,
}

/// Why a claim did not install its writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScribeStateClaim {
    /// The writer now owns the state at the accepted position.
    Claimed(ScribeStateHead),
    /// A claim of a newer generation already owns the state.
    Superseded,
}

/// Installs `writer` as the only writer of `key` in `logical_table_fqn` and
/// returns the state's accepted position.
///
/// Creates the head at revision 0 when the state has none, so the answer is
/// authoritative: revision 0 means no write of the state was ever accepted.
/// A claim of an older `generation` than the head's leaves the head unchanged
/// and answers [`ScribeStateClaim::Superseded`]. The head's row lock orders
/// a claim against every concurrent [`advance`]: a write accepted first is
/// part of the answered position, and a write of the replaced writer after it
/// is refused. The caller commits `conn`.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the statement or the
/// stored head is out of range.
pub async fn claim(
    conn: &mut TenantConn<'_>,
    logical_table_fqn: &str,
    key: uuid::Uuid,
    writer: uuid::Uuid,
    generation: i64,
) -> Result<ScribeStateClaim, SqlError> {
    let claimed: Option<(i64, Option<uuid::Uuid>)> = sqlx::query_as(
        "INSERT INTO vala.scribe_state_heads \
         (data_tenant_id, logical_table_fqn, state_key, writer, generation) \
         VALUES (wyrd.current_tenant(), $1, $2, $3, $4) \
         ON CONFLICT (data_tenant_id, logical_table_fqn, state_key) DO UPDATE \
            SET writer = EXCLUDED.writer, generation = EXCLUDED.generation \
          WHERE vala.scribe_state_heads.generation <= EXCLUDED.generation \
         RETURNING revision, batch_id",
    )
    .bind(logical_table_fqn)
    .bind(key)
    .bind(writer)
    .bind(generation)
    .fetch_optional(&mut **conn.transaction())
    .await?;
    Ok(match claimed {
        Some((revision, batch_id)) => {
            ScribeStateClaim::Claimed(ScribeStateHead { revision, batch_id })
        }
        None => ScribeStateClaim::Superseded,
    })
}

/// Accepts each offered write whose writer owns its state, whose revision
/// directly follows the accepted one, and whose bound has not passed, and
/// records the batch that accepted it.
///
/// Runs in the caller's batch-fence transaction, so a write is accepted
/// exactly when its batch fence commits. The result holds, per offered write
/// and in order, whether it was accepted. Two writes of one state in one call
/// accept at most the first.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot execute the statement.
pub async fn advance(
    conn: &mut TenantConn<'_>,
    offered: &[(&str, uuid::Uuid, ScribeStateAdvance)],
) -> Result<Vec<bool>, SqlError> {
    let accepted: Vec<(String, uuid::Uuid)> = sqlx::query_as(
        "UPDATE vala.scribe_state_heads AS head \
            SET revision = offered.revision, batch_id = offered.batch_id \
           FROM unnest($1::text[], $2::uuid[], $3::uuid[], $4::bigint[], $5::uuid[], \
                       $6::timestamptz[]) \
                AS offered(logical_table_fqn, state_key, writer, revision, batch_id, not_after) \
          WHERE head.data_tenant_id = wyrd.current_tenant() \
            AND head.logical_table_fqn = offered.logical_table_fqn \
            AND head.state_key = offered.state_key \
            AND head.writer = offered.writer \
            AND head.revision = offered.revision - 1 \
            AND (offered.not_after IS NULL OR statement_timestamp() < offered.not_after) \
         RETURNING head.logical_table_fqn, offered.batch_id",
    )
    .bind(
        offered
            .iter()
            .map(|(fqn, _, _)| (*fqn).to_owned())
            .collect::<Vec<_>>(),
    )
    .bind(offered.iter().map(|(_, _, s)| s.key).collect::<Vec<_>>())
    .bind(offered.iter().map(|(_, _, s)| s.writer).collect::<Vec<_>>())
    .bind(offered.iter().map(|(_, _, s)| s.revision).collect::<Vec<_>>())
    .bind(offered.iter().map(|(_, batch, _)| *batch).collect::<Vec<_>>())
    .bind(
        offered
            .iter()
            .map(|(_, _, s)| s.not_after)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&mut **conn.transaction())
    .await?;
    Ok(offered
        .iter()
        .map(|(fqn, batch, _)| {
            accepted
                .iter()
                .any(|(table, accepted)| table == fqn && accepted == batch)
        })
        .collect())
}

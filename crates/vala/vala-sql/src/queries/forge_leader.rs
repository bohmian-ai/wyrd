//! Singleton Forge leader election.
//!
//! One row in `vala.forge_scheduler_state` names the replica whose in-memory
//! scheduler dispatches Forge compaction and runs leader-timer maintenance. A
//! term is the pair of owner and fencing token, held only while `expires_at`
//! is in the future. The row also carries the leader's private peer URI, so a
//! compactor on another replica routes to the current live term and never to
//! an expired one. The row is routing and liveness only; it carries no
//! scheduling state.

use std::time::Duration;

use sqlx::types::Uuid;

use crate::{OperatorPool, SqlError};

/// The live leader term a compactor may route to.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ForgeLeaderTerm {
    /// Stable process identity that holds the term.
    pub owner: Uuid,
    /// Monotonic token minted when the term was acquired.
    pub fencing_token: i64,
    /// Private peer URI the leader serves, absent for an in-process deployment.
    pub peer_uri: Option<String>,
}

/// Concrete owner of the singleton leader election row.
#[derive(Clone)]
pub struct ForgeLeaderElection {
    /// Cross-tenant operator pool; the row has no tenant column.
    operator_pool: OperatorPool,
}

impl ForgeLeaderElection {
    /// Creates the election owner over the operator pool.
    #[must_use]
    pub fn new(operator_pool: OperatorPool) -> Self {
        Self { operator_pool }
    }

    /// Converts a term duration into whole milliseconds for SQL.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] for a zero or unrepresentable duration.
    fn ttl_millis(ttl: Duration) -> Result<i64, SqlError> {
        match i64::try_from(ttl.as_millis()) {
            Ok(millis) if millis > 0 => Ok(millis),
            _ => Err(SqlError::Conflict {
                detail: "Forge leader term must be a positive millisecond duration".to_owned(),
            }),
        }
    }

    /// Acquires a new leader term when none is live or `owner` held the last one.
    ///
    /// Every successful acquisition mints a fresh fencing token, including a
    /// restarted process reclaiming its own unexpired term, so a successor's
    /// empty in-memory schedule is never mistaken for its predecessor's. The
    /// peer URI is published in the same statement as the term.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] for an invalid duration and SQL errors
    /// from the single update.
    ///
    /// # Cancellation
    ///
    /// The single update either installs the whole term or has no effect.
    pub async fn acquire(
        &self,
        owner: Uuid,
        peer_uri: Option<&str>,
        ttl: Duration,
    ) -> Result<Option<i64>, SqlError> {
        let ttl = Self::ttl_millis(ttl)?;
        sqlx::query_scalar(
            "UPDATE vala.forge_scheduler_state SET owner=$1,peer_uri=$2,\
             fencing_token=fencing_token+1,\
             expires_at=statement_timestamp()+($3*interval '1 millisecond'),\
             updated_at=statement_timestamp() \
             WHERE singleton AND (expires_at IS NULL OR expires_at<=statement_timestamp() OR owner=$1) \
             RETURNING fencing_token",
        )
        .bind(owner)
        .bind(peer_uri)
        .bind(ttl)
        .fetch_optional(self.operator_pool.pool())
        .await
        .map_err(SqlError::from)
    }

    /// Extends one exact live term.
    ///
    /// Returns `false` when the term expired or was replaced, which the caller
    /// must treat as lost leadership.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] for an invalid duration and SQL errors
    /// from the single update.
    pub async fn renew(
        &self,
        owner: Uuid,
        fencing_token: i64,
        ttl: Duration,
    ) -> Result<bool, SqlError> {
        let ttl = Self::ttl_millis(ttl)?;
        let changed = sqlx::query(
            "UPDATE vala.forge_scheduler_state \
             SET expires_at=statement_timestamp()+($3*interval '1 millisecond'),\
             updated_at=statement_timestamp() \
             WHERE singleton AND owner=$1 AND fencing_token=$2 AND expires_at>statement_timestamp()",
        )
        .bind(owner)
        .bind(fencing_token)
        .bind(ttl)
        .execute(self.operator_pool.pool())
        .await
        .map_err(SqlError::from)?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Ends one exact term immediately so a standby can take over at once.
    ///
    /// The owner, peer URI and expiry are cleared while the token is kept, so
    /// the successor's acquisition still mints a larger token. Resigning a
    /// term that was already replaced has no effect.
    ///
    /// # Errors
    ///
    /// Returns SQL errors from the single update.
    pub async fn resign(&self, owner: Uuid, fencing_token: i64) -> Result<(), SqlError> {
        sqlx::query(
            "UPDATE vala.forge_scheduler_state SET owner=NULL,peer_uri=NULL,expires_at=NULL,\
             updated_at=statement_timestamp() \
             WHERE singleton AND owner=$1 AND fencing_token=$2",
        )
        .bind(owner)
        .bind(fencing_token)
        .execute(self.operator_pool.pool())
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Reads the live term, if any.
    ///
    /// An expired row is never returned, so a compactor cannot route to a
    /// leader whose term has lapsed.
    ///
    /// # Errors
    ///
    /// Returns SQL errors from the read.
    pub async fn current(&self) -> Result<Option<ForgeLeaderTerm>, SqlError> {
        sqlx::query_as(
            "SELECT owner,fencing_token,peer_uri FROM vala.forge_scheduler_state \
             WHERE singleton AND owner IS NOT NULL AND expires_at>statement_timestamp()",
        )
        .fetch_optional(self.operator_pool.pool())
        .await
        .map_err(SqlError::from)
    }
}

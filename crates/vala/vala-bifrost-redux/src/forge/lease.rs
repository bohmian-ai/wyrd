//! The claim lease on one Forge task, the table's only exclusive authority.
//!
//! The partial unique index `forge_tasks_publication_active` admits one
//! `claimed`, `running`, or `prepared` task per table, so the active task row is
//! the table lease: claiming it acquires the table, its terminal transition
//! releases it, and its heartbeat renews it. Contention is a refused claim at
//! admission, never a failure after work was admitted. This type carries that
//! claim's identity into every renewal, pre-commit window check, and tenant
//! transaction fence of the attempt that owns it.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use uuid::Uuid;

use vala_sql::TenantConn;
use vala_sql::queries::forge_tasks::ForgeTasks;

use super::error::ForgeError;

#[derive(Clone)]
/// The live claim lease of one Forge task attempt.
///
/// Every clone shares one renewal clock, so the attempt's heartbeat loop keeps
/// the remaining-time estimate current for each clone the attempt's stages
/// hold.
pub struct ForgeLease {
    /// Durable task owner the claim lives on.
    tasks: ForgeTasks,
    /// Claimed task row.
    pub task_id: Uuid,
    /// Attempt generation the claim was granted to.
    pub attempt_id: Uuid,
    /// UUID of the worker that owns the claim.
    pub owner: Uuid,
    /// Claim lifetime each renewal extends from `PostgreSQL`'s clock.
    ttl: Duration,
    /// Shared monotonic origin used by every clone of this claim.
    clock_started: Instant,
    /// Shared nanosecond offset of the most recent successful renewal.
    confirmed_at_nanos: Arc<AtomicU64>,
}

impl std::fmt::Debug for ForgeLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForgeLease")
            .field("task_id", &self.task_id)
            .field("attempt_id", &self.attempt_id)
            .field("owner", &self.owner)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl ForgeLease {
    /// Confirms a claim this worker already holds and starts its renewal clock.
    ///
    /// The claim was granted by the task row; confirming it renews that row
    /// once, so the local remaining-time estimate starts from a renewal the
    /// database acknowledged rather than from the earlier claim instant.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when the claim lapsed or passed to
    /// another attempt before confirmation, [`ForgeError::InvalidConfig`] when
    /// the TTL does not fit whole seconds, and [`ForgeError::Sql`] for other
    /// statement failures.
    pub async fn confirm(
        tasks: ForgeTasks,
        task_id: Uuid,
        attempt_id: Uuid,
        owner: Uuid,
        ttl: Duration,
    ) -> Result<Self, ForgeError> {
        let mut lease = Self {
            tasks,
            task_id,
            attempt_id,
            owner,
            ttl,
            clock_started: Instant::now(),
            confirmed_at_nanos: Arc::new(AtomicU64::new(0)),
        };
        lease.require_renewal().await?;
        Ok(lease)
    }

    /// Renews this claim and refreshes its local confirmation time.
    ///
    /// Returns `false` when the claim no longer belongs to this attempt and
    /// owner, or has lapsed.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the TTL does not fit whole
    /// seconds and [`ForgeError::Sql`] for statement failures other than a
    /// stale claim.
    pub async fn renew(&mut self) -> Result<bool, ForgeError> {
        let seconds = u32::try_from(self.ttl.as_secs()).map_err(|_| ForgeError::InvalidConfig {
            detail: "Forge claim TTL exceeds u32 seconds".to_owned(),
        })?;
        match self
            .tasks
            .heartbeat(self.task_id, self.attempt_id, self.owner, seconds)
            .await
        {
            Ok(()) => {
                self.confirmed_at_nanos
                    .store(self.elapsed_nanos(), Ordering::Release);
                Ok(true)
            }
            Err(vala_sql::SqlError::Conflict { .. }) => Ok(false),
            Err(error) => Err(ForgeError::Sql(error)),
        }
    }

    /// Returns the locally estimated time remaining before the claim lapses.
    pub fn remaining(&self) -> Duration {
        let confirmed_at = self.confirmed_at_nanos.load(Ordering::Acquire);
        self.ttl.saturating_sub(Duration::from_nanos(
            self.elapsed_nanos().saturating_sub(confirmed_at),
        ))
    }

    /// Checks whether the remaining claim covers a required commit window.
    pub fn commit_window_fits(&self, required: Duration) -> bool {
        self.remaining() > required
    }

    /// Renews the claim and fails closed if it cannot be confirmed.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when the local estimate has already
    /// lapsed or the renewal finds the claim gone, and the renewal's other
    /// failures unchanged.
    pub async fn require_fence(&mut self) -> Result<(), ForgeError> {
        if self.remaining().is_zero() {
            return Err(self.fence_lost());
        }
        self.require_renewal().await
    }

    /// Asserts this claim inside a tenant transaction immediately before commit.
    ///
    /// The task row is locked for the rest of the transaction, so the claim
    /// cannot be reclaimed or taken over between this check and the commit.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when the claim no longer belongs to
    /// this live attempt, and [`ForgeError::Sql`] for other statement failures.
    pub async fn assert_transaction_fence(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<(), ForgeError> {
        match ForgeTasks::lock_live_claim(conn, self.task_id, self.attempt_id, self.owner).await {
            Ok(()) => Ok(()),
            Err(vala_sql::SqlError::Conflict { .. }) => Err(self.fence_lost()),
            Err(error) => Err(ForgeError::Sql(error)),
        }
    }

    /// Renews the claim, reporting a stale claim as a lost fence.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] for a stale claim and the renewal's
    /// other failures unchanged.
    async fn require_renewal(&mut self) -> Result<(), ForgeError> {
        if self.renew().await? {
            Ok(())
        } else {
            Err(self.fence_lost())
        }
    }

    /// Names this claim in a lost-fence error.
    #[must_use]
    pub fn fence_lost(&self) -> ForgeError {
        ForgeError::FenceLost {
            lease_key: format!("forge:task:{}:{}", self.task_id, self.attempt_id),
        }
    }

    /// Returns the saturated monotonic nanosecond offset for shared renewal state.
    fn elapsed_nanos(&self) -> u64 {
        u64::try_from(self.clock_started.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

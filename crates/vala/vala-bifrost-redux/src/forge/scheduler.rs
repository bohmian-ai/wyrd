//! Serialized periodic and advisory-hint scheduling for the Forge owner.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::Forge;
use super::binpack::ForgeGroupKey;
use super::compact::{ForgeTableKey, ForgeTickBudget, ForgeTickOutcome};
use super::error::ForgeError;
use super::lease::{ForgeLease, forge_lease_key};
use super::live_reconcile::{DestructiveMaintenance, IcebergReconciliationOutcome};
use crate::catalog::TenantTableBinding;
use crate::maintenance::StagingFileCommitted;

/// Interim result of one table-local maintenance pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableStageDisposition {
    /// Every enabled stage completed.
    Completed,
    /// Live reconciliation requires destructive stages to remain skipped.
    Blocked,
    /// Global cancellation stopped work at a safe stage boundary.
    Cancelled,
}

/// Cancellation-aware result of one staging-fold call.
enum StagingStageResult<T> {
    /// Staging fold completed with its ordinary outcome.
    Completed(T),
    /// Staging fold observed global shutdown.
    Cancelled,
}

impl Forge {
    /// Run periodic and advisory-hint maintenance until cancellation.
    ///
    /// A second scheduler loop on the same owner fails immediately. Individual
    /// tick and hinted-key failures are logged and isolated so durable periodic
    /// discovery remains available after a local failure or closed hint inbox.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::AlreadyRunning`] when this owner already has a
    /// scheduler loop. Loop-internal maintenance failures are isolated and do
    /// not terminate the scheduler.
    pub async fn run(&self, shutdown: CancellationToken) -> Result<(), ForgeError> {
        let _run = self.acquire_run_guard()?;
        let interval = self.core.maintenance_interval;
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut hints_open = true;

        loop {
            tokio::select! {
                () = shutdown.cancelled() => return Ok(()),
                first = self.receive_hint(), if hints_open => {
                    match first {
                        Some(first) => match self.run_hinted_batch(first, &shutdown).await {
                            Ok(outcome) => tracing::debug!(
                                groups_seen = outcome.groups_seen,
                                bins_committed = outcome.bins_committed,
                                tables_failed = outcome.tables_failed,
                                "Forge hinted maintenance completed"
                            ),
                            Err(error) => tracing::error!(
                                error = %error,
                                "Forge hinted maintenance batch failed"
                            ),
                        },
                        None => hints_open = false,
                    }
                },
                _ = ticker.tick() => {
                    match self.run_periodic_tick(&shutdown).await {
                        Ok(outcome) => tracing::debug!(
                            groups_seen = outcome.groups_seen,
                            bins_committed = outcome.bins_committed,
                            reconciliation_recovered = outcome.reconciliation_recovered,
                            tables_succeeded = outcome.tables_succeeded,
                            tables_skipped = outcome.tables_skipped,
                            tables_failed = outcome.tables_failed,
                            "Forge periodic maintenance completed"
                        ),
                        Err(error) => tracing::error!(
                            error = %error,
                            "Forge periodic maintenance tick failed"
                        ),
                    }
                }
            }
        }
    }

    /// Execute one serialized complete periodic maintenance tick.
    ///
    /// This explicit operation waits for any active hinted or periodic tick,
    /// but does not claim the long-lived scheduler run guard.
    ///
    /// # Errors
    ///
    /// Returns a discovery or durable stage error that prevents the explicit
    /// tick from completing. Per-table stage errors remain isolated in its
    /// returned counters.
    pub async fn run_once(&self) -> Result<ForgeTickOutcome, ForgeError> {
        self.run_periodic_tick(&CancellationToken::new()).await
    }

    /// Acquire the fail-fast guard for the directly supervised scheduler.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::AlreadyRunning`] when another loop owns the guard.
    fn acquire_run_guard(&self) -> Result<ForgeRunGuard<'_>, ForgeError> {
        self.running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ForgeError::AlreadyRunning)?;
        Ok(ForgeRunGuard {
            running: &self.running,
        })
    }

    /// Receive one advisory event while holding only the inbox mutex.
    async fn receive_hint(&self) -> Option<StagingFileCommitted> {
        self.hints.lock().await.recv().await
    }

    /// Serialize and execute a complete durable-discovery maintenance tick.
    ///
    /// # Errors
    ///
    /// Returns a SQL discovery error when no ordered table work set can be
    /// formed. Cancellation stops before beginning the next table.
    async fn run_periodic_tick(
        &self,
        stop: &CancellationToken,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let _tick = self.tick.lock().await;
        self.run_periodic_tick_locked(stop).await
    }

    /// Execute periodic stages while the caller retains the tick mutex.
    ///
    /// # Errors
    ///
    /// Returns a table-discovery error. Individual table failures are counted
    /// and do not abort later tables.
    async fn run_periodic_tick_locked(
        &self,
        stop: &CancellationToken,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let owner = Uuid::now_v7();
        let mut outcome = ForgeTickOutcome::default();
        let (mut tables, discovery_failures) = self.discover_tables().await?;
        outcome.tables_failed = outcome.tables_failed.saturating_add(discovery_failures);
        tables.sort_by(|left, right| {
            left.tenant
                .cmp(&right.tenant)
                .then_with(|| left.table_ref.fqn().cmp(&right.table_ref.fqn()))
        });
        for key in tables {
            if stop.is_cancelled() {
                break;
            }
            match self
                .process_periodic_table(&key, owner, stop, &mut outcome)
                .await
            {
                TableRunStatus::Succeeded => outcome.tables_succeeded += 1,
                TableRunStatus::Skipped => outcome.tables_skipped += 1,
                TableRunStatus::Failed => outcome.tables_failed += 1,
            }
        }
        Ok(outcome)
    }

    /// Drain, deduplicate, and execute one bounded hinted batch.
    ///
    /// The inbox mutex is released before the tick mutex is acquired. All keys
    /// share one tick budget and a failure on one exact key does not stop later
    /// keys.
    ///
    /// # Errors
    ///
    /// Returns only an invariant-level batch error; keyed SQL, lease, catalog,
    /// and rewrite failures are counted and isolated.
    async fn run_hinted_batch(
        &self,
        first: StagingFileCommitted,
        stop: &CancellationToken,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let keys = self.drain_hint_keys(first).await;
        let _tick = self.tick.lock().await;
        let mut aggregate = ForgeTickOutcome::default();
        let mut budget = ForgeTickBudget::default();
        for key in keys {
            if stop.is_cancelled() {
                break;
            }
            match self.run_hinted_key(&key, stop, &mut budget).await {
                Ok(outcome) => aggregate.merge(outcome),
                Err(error) => {
                    aggregate.tables_failed += 1;
                    aggregate.stage_failures += 1;
                    aggregate.pending_work = true;
                    tracing::error!(
                        error = %error,
                        tenant = %key.tenant,
                        table = %key.table_ref.fqn(),
                        partition_day = %key.partition_day,
                        "Forge hinted key failed; continuing"
                    );
                }
            }
        }
        Ok(aggregate)
    }

    /// Execute reconciliation and exact-day compaction for one hinted key.
    ///
    /// The caller retains the tick mutex. Targeted discovery bypasses only the
    /// periodic age guard and shares the batch's file, byte, and bin budgets.
    ///
    /// # Errors
    ///
    /// Returns binding, lease, reconciliation, discovery, rewrite, catalog, or
    /// durable bookkeeping errors for this key. Cancellation stops before the
    /// next durable stage.
    async fn run_hinted_key(
        &self,
        key: &ForgeGroupKey,
        stop: &CancellationToken,
        budget: &mut ForgeTickBudget,
    ) -> Result<ForgeTickOutcome, ForgeError> {
        let table_key = ForgeTableKey {
            tenant: key.tenant,
            table_ref: key.table_ref.clone(),
        };
        let binding =
            TenantTableBinding::resolve((key.tenant, key.table_ref.clone())).map_err(|error| {
                ForgeError::Group {
                    detail: error.to_string(),
                }
            })?;
        let lease_key =
            forge_lease_key(key.tenant, &binding.logical_namespace, &binding.table_name);
        let Some(mut lease) = ForgeLease::acquire(
            &self.core.operator_pool,
            lease_key,
            Uuid::now_v7(),
            self.core.config.lease_ttl,
        )
        .await?
        else {
            let outcome = ForgeTickOutcome {
                lease_contention: 1,
                tables_skipped: 1,
                ..ForgeTickOutcome::default()
            };
            return Ok(outcome);
        };

        let result = async {
            let mut outcome = ForgeTickOutcome::default();
            let recovered = self
                .run_compaction_reconciliation_for_table(&mut lease, &table_key, &binding)
                .await?;
            outcome.reconciliation_recovered += recovered;
            outcome.reconciled += recovered;
            if stop.is_cancelled() {
                outcome.tables_skipped += 1;
                outcome.pending_work = true;
                return Ok(outcome);
            }
            let now = chrono::Utc::now();
            let live = match self
                .reconcile_live_replacements(&mut lease, &table_key, &binding, stop, now)
                .await
            {
                Ok(live) => live,
                Err(ForgeError::Shutdown) => {
                    outcome.tables_skipped += 1;
                    outcome.pending_work = true;
                    return Ok(outcome);
                }
                Err(error) => return Err(error),
            };
            record_live_reconciliation(&mut outcome, &live);
            match staging_stage_result(
                self.run_targeted_compaction_for_table(&mut lease, key, &binding, budget, stop)
                    .await,
            )? {
                StagingStageResult::Completed(staging) => outcome.merge(staging),
                StagingStageResult::Cancelled => {
                    outcome.tables_skipped += 1;
                    outcome.pending_work = true;
                    return Ok(outcome);
                }
            }
            if stop.is_cancelled()
                || live.destructive_maintenance == DestructiveMaintenance::Blocked
            {
                outcome.tables_skipped += 1;
                outcome.pending_work = true;
                return Ok(outcome);
            }
            outcome.tables_succeeded += 1;
            Ok(outcome)
        }
        .await;
        let release = self.release_lease(&lease).await;
        match (result, release) {
            (Ok(outcome), Ok(())) => Ok(outcome),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(release_error)) => {
                tracing::warn!(
                    error = %release_error,
                    lease_key = %lease.lease_key,
                    "Forge hinted stage failed and lease release also failed"
                );
                Err(error)
            }
        }
    }

    /// Drain at most the configured wake limit and return ordered unique keys.
    async fn drain_hint_keys(&self, first: StagingFileCommitted) -> Vec<ForgeGroupKey> {
        let mut events = vec![first];
        let mut hints = self.hints.lock().await;
        for _ in 1..self.core.config.max_hints_per_wake {
            match hints.try_recv() {
                Ok(event) => events.push(event),
                Err(
                    tokio::sync::mpsc::error::TryRecvError::Empty
                    | tokio::sync::mpsc::error::TryRecvError::Disconnected,
                ) => break,
            }
        }
        drop(hints);

        let mut keys = HashSet::new();
        for event in events {
            let (binding, partition_day) = event.into_parts();
            keys.insert(ForgeGroupKey {
                tenant: binding.tenant,
                table_ref: binding.table_ref,
                partition_day,
            });
        }
        order_hint_keys(keys)
    }

    /// Resolve, lease, execute, and release one complete periodic table flow.
    async fn process_periodic_table(
        &self,
        key: &ForgeTableKey,
        owner: Uuid,
        stop: &CancellationToken,
        outcome: &mut ForgeTickOutcome,
    ) -> TableRunStatus {
        let binding = match TenantTableBinding::resolve((key.tenant, key.table_ref.clone())) {
            Ok(binding) => binding,
            Err(error) => {
                tracing::error!(
                    tenant = %key.tenant,
                    table = %key.table_ref.fqn(),
                    error = %error,
                    "Forge table binding failed"
                );
                return TableRunStatus::Failed;
            }
        };
        let lease_key =
            forge_lease_key(key.tenant, &binding.logical_namespace, &binding.table_name);
        let mut lease = match ForgeLease::acquire(
            &self.core.operator_pool,
            lease_key,
            owner,
            self.core.config.lease_ttl,
        )
        .await
        {
            Ok(Some(lease)) => lease,
            Ok(None) => {
                outcome.lease_contention += 1;
                return TableRunStatus::Skipped;
            }
            Err(error) => {
                tracing::error!(error = %error, "Forge table lease acquisition failed");
                return TableRunStatus::Failed;
            }
        };
        let table_result = self
            .run_periodic_table_stages(&mut lease, key, &binding, stop, outcome)
            .await;
        let release_result = self.release_lease(&lease).await;
        match (table_result, release_result) {
            (Ok(TableStageDisposition::Completed), Ok(())) => TableRunStatus::Succeeded,
            (Ok(TableStageDisposition::Blocked | TableStageDisposition::Cancelled), Ok(())) => {
                TableRunStatus::Skipped
            }
            (Err(error), Ok(())) | (Ok(_), Err(error)) => {
                outcome.pending_work = true;
                Self::record_table_failure(key, outcome, &error);
                TableRunStatus::Failed
            }
            (Err(error), Err(release_error)) => {
                tracing::warn!(
                    error = %release_error,
                    lease_key = %lease.lease_key,
                    "Forge stage failed and lease release also failed"
                );
                outcome.pending_work = true;
                Self::record_table_failure(key, outcome, &error);
                TableRunStatus::Failed
            }
        }
    }

    /// Execute reconciliation, compaction, expiry, and GC under one table fence.
    ///
    /// # Errors
    ///
    /// Returns the first stage failure. Cancellation is observed between
    /// durable stages and reported as a non-failure skip to the caller.
    async fn run_periodic_table_stages(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
        outcome: &mut ForgeTickOutcome,
    ) -> Result<TableStageDisposition, ForgeError> {
        let reconciliation = self
            .run_compaction_reconciliation_for_table(lease, key, binding)
            .await?;
        outcome.reconciliation_recovered += reconciliation;
        outcome.reconciled += reconciliation;
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }
        let live = match self
            .reconcile_live_replacements(lease, key, binding, stop, chrono::Utc::now())
            .await
        {
            Ok(live) => live,
            Err(ForgeError::Shutdown) => {
                outcome.pending_work = true;
                return Ok(TableStageDisposition::Cancelled);
            }
            Err(error) => return Err(error),
        };
        record_live_reconciliation(outcome, &live);

        let compaction = match staging_stage_result(
            self.run_compaction_bins_for_table(lease, key, binding, stop)
                .await,
        )? {
            StagingStageResult::Completed(compaction) => compaction,
            StagingStageResult::Cancelled => {
                outcome.pending_work = true;
                return Ok(TableStageDisposition::Cancelled);
            }
        };
        outcome.merge(compaction);
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }
        if live.destructive_maintenance == DestructiveMaintenance::Blocked {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Blocked);
        }

        let expiry = self
            .run_snapshot_expiry_for_table(lease, key, binding)
            .await?;
        outcome.expiry_reconciled += expiry;
        outcome.reconciled += expiry;
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }

        let table = self.load_table(&binding.table_ident()).await?;
        let live_set = self.build_live_set(key, binding, &table).await?;
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }
        let gc = self
            .run_orphan_gc_for_table(lease, key, binding, &live_set)
            .await?;
        outcome.gc_reconciled += gc.recovered;
        outcome.gc_deleted += gc.deleted;
        outcome.gc_skipped += gc.skipped;
        outcome.reconciled += gc.recovered;
        Ok(TableStageDisposition::Completed)
    }

    /// Conditionally release one table lease through the configured timeout.
    ///
    /// # Errors
    ///
    /// Returns a lease or timeout error. A lost release fence is logged because
    /// the newer owner already prevents this process from mutating the table.
    async fn release_lease(&self, lease: &ForgeLease) -> Result<(), ForgeError> {
        let released = tokio::time::timeout(
            self.core.config.catalog_request_timeout,
            lease.release(&self.core.operator_pool),
        )
        .await
        .map_err(|_| ForgeError::Timeout {
            operation: "Forge lease release",
        })??;
        if !released {
            tracing::warn!(lease_key = %lease.lease_key, "Forge lease release lost its fence");
        }
        Ok(())
    }

    /// Add one isolated periodic-table failure to the tick outcome.
    fn record_table_failure(
        key: &ForgeTableKey,
        outcome: &mut ForgeTickOutcome,
        error: &ForgeError,
    ) {
        outcome.stage_failures += 1;
        if matches!(error, ForgeError::FenceLost { .. }) {
            outcome.fence_losses += 1;
        }
        tracing::error!(
            tenant = %key.tenant,
            table = %key.table_ref.fqn(),
            error = %error,
            "Forge table maintenance failed; continuing"
        );
    }
}

/// Merge exact live-reconciliation counters and conservative pending state.
fn record_live_reconciliation(tick: &mut ForgeTickOutcome, live: &IcebergReconciliationOutcome) {
    tick.live_recovered = tick.live_recovered.saturating_add(live.recovered);
    tick.live_reset = tick.live_reset.saturating_add(live.reset);
    tick.live_pending = tick.live_pending.saturating_add(live.pending);
    tick.live_unresolved = tick.live_unresolved.saturating_add(live.unresolved);
    tick.open_operation_overflows = tick
        .open_operation_overflows
        .saturating_add(usize::from(live.overflowed));
    tick.pending_work |= live.overflowed || live.pending > 0 || live.unresolved > 0;
}

/// Convert only shutdown into a non-error cancelled staging disposition.
///
/// # Errors
///
/// Returns every non-shutdown staging error unchanged.
fn staging_stage_result<T>(
    result: Result<T, ForgeError>,
) -> Result<StagingStageResult<T>, ForgeError> {
    match result {
        Ok(value) => Ok(StagingStageResult::Completed(value)),
        Err(ForgeError::Shutdown) => Ok(StagingStageResult::Cancelled),
        Err(error) => Err(error),
    }
}

/// RAII guard that releases the scheduler ownership flag on every exit path.
struct ForgeRunGuard<'a> {
    /// Atomic scheduler flag borrowed from the owning Forge handle.
    running: &'a std::sync::atomic::AtomicBool,
}

impl Drop for ForgeRunGuard<'_> {
    /// Release scheduler ownership for a deliberate supervised restart.
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

/// Result of one isolated periodic table execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableRunStatus {
    /// Every maintenance stage completed.
    Succeeded,
    /// Work was fenced by another owner or stopped between stages.
    Skipped,
    /// Binding, lease, stage, or release behavior failed.
    Failed,
}

/// Convert unique hinted identities into their deterministic execution order.
fn order_hint_keys(keys: HashSet<ForgeGroupKey>) -> Vec<ForgeGroupKey> {
    let mut keys = keys.into_iter().collect::<Vec<_>>();
    keys.sort_by(|left, right| {
        left.tenant
            .cmp(&right.tenant)
            .then_with(|| left.table_ref.fqn().cmp(&right.table_ref.fqn()))
            .then_with(|| left.partition_day.cmp(&right.partition_day))
    });
    keys
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use chrono::NaiveDate;
    use wyrd_spec::DataTenantId;

    use super::*;
    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;

    /// The scheduler run guard rejects a second owner without waiting.
    #[test]
    fn second_run_is_rejected() {
        let running = AtomicBool::new(false);
        running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .expect("first scheduler guard must be acquired");
        let _guard = ForgeRunGuard { running: &running };
        assert!(
            running
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        );
    }

    /// The tick mutex prevents an explicit tick from overlapping active work.
    #[tokio::test]
    async fn run_once_serializes_with_active_tick() {
        let tick = Arc::new(tokio::sync::Mutex::new(()));
        let active = tick.lock().await;
        let contender = {
            let tick = Arc::clone(&tick);
            tokio::spawn(async move {
                let _tick = tick.lock().await;
                true
            })
        };
        tokio::task::yield_now().await;
        assert!(!contender.is_finished());
        drop(active);
        assert!(contender.await.expect("tick contender must complete"));
    }

    /// Duplicate keys collapse and the retained bounded set is fully ordered.
    #[test]
    fn hint_drain_deduplicates_orders_and_bounds_keys() {
        let first_tenant = DataTenantId::try_from(Uuid::now_v7()).expect("tenant ID must be valid");
        let second_tenant =
            DataTenantId::try_from(Uuid::now_v7()).expect("tenant ID must be valid");
        let day = NaiveDate::from_ymd_opt(2026, 7, 27).expect("test day must be valid");
        let first = ForgeGroupKey {
            tenant: second_tenant,
            table_ref: TableRef::new(BifrostNamespace::Bifrost, "z"),
            partition_day: day,
        };
        let second = ForgeGroupKey {
            tenant: first_tenant,
            table_ref: TableRef::new(BifrostNamespace::Bifrost, "a"),
            partition_day: day,
        };
        let bounded = [first.clone(), second.clone(), first]
            .into_iter()
            .take(3)
            .collect::<HashSet<_>>();
        let ordered = order_hint_keys(bounded);
        assert_eq!(ordered.len(), 2);
        assert!(ordered.contains(&second));
        assert!(ordered.windows(2).all(|pair| {
            pair[0].tenant < pair[1].tenant
                || (pair[0].tenant == pair[1].tenant
                    && pair[0].table_ref.fqn() <= pair[1].table_ref.fqn())
        }));
    }

    /// Periodic and hinted staging calls share exact shutdown-only cancellation mapping.
    #[test]
    fn staging_fold_shutdown_maps_to_cancelled_without_hiding_failures() {
        assert!(matches!(
            staging_stage_result::<()>(Err(ForgeError::Shutdown))
                .expect("shutdown is a disposition"),
            StagingStageResult::Cancelled
        ));
        assert!(matches!(
            staging_stage_result(Ok(7)).expect("success remains completed"),
            StagingStageResult::Completed(7)
        ));
        assert!(
            staging_stage_result::<()>(Err(ForgeError::Invariant {
                detail: "test failure".to_owned(),
            }))
            .is_err()
        );
    }

    /// Both scheduler paths fold staging before consuming the blocked gate,
    /// while periodic expiry and GC remain reachable only after that gate.
    #[test]
    fn blocked_live_reconciliation_preserves_stage_order() {
        let source = include_str!("scheduler.rs");
        let hinted_start = source
            .find("async fn run_hinted_key")
            .expect("hinted owner");
        let periodic_start = source
            .find("async fn run_periodic_table_stages")
            .expect("periodic owner");
        let hinted = &source[hinted_start..periodic_start];
        assert!(
            hinted.find("reconcile_live_replacements")
                < hinted.find("run_targeted_compaction_for_table")
        );
        assert!(
            hinted.find("run_targeted_compaction_for_table")
                < hinted.find("DestructiveMaintenance::Blocked")
        );
        let periodic = &source[periodic_start..];
        assert!(
            periodic.find("reconcile_live_replacements")
                < periodic.find("run_compaction_bins_for_table")
        );
        assert!(
            periodic.find("run_compaction_bins_for_table")
                < periodic.find("DestructiveMaintenance::Blocked")
        );
        assert!(
            periodic.find("DestructiveMaintenance::Blocked")
                < periodic.find("run_snapshot_expiry_for_table")
        );
        assert!(
            periodic.find("run_snapshot_expiry_for_table")
                < periodic.find("run_orphan_gc_for_table")
        );
    }
}

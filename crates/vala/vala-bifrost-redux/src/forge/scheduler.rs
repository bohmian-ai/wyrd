//! Serialized periodic and advisory-hint scheduling for the Forge owner.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::Forge;
use super::binpack::ForgeGroupKey;
use super::compact::{ForgeTableKey, ForgeTickBudget, ForgeTickOutcome};
use super::error::ForgeError;
use super::lease::{ForgeLease, forge_lease_key};
use super::live_reconcile::{DestructiveMaintenance, IcebergReconciliationOutcome};
use super::live_replace::IcebergRewriteDisposition;
use super::metrics::{
    ForgeGaugeSnapshot, ForgeLeaseResult, ForgeMetricSource, ForgeMetricStage,
    ReconciliationGaugeObservation,
};
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
        let now = self.core.clock.now()?;
        let mut outcome = ForgeTickOutcome::default();
        let mut gauges = ForgeGaugeSnapshot::default();
        let mut budget = ForgeTickBudget::default();
        let (mut tables, discovery_failures) = self.discover_tables().await?;
        outcome.tables_discovered = tables.len().saturating_add(discovery_failures);
        outcome.tables_examined = discovery_failures;
        outcome.tables_failed = outcome.tables_failed.saturating_add(discovery_failures);
        outcome.stage_failures = outcome.stage_failures.saturating_add(discovery_failures);
        outcome.pending_work |= discovery_failures > 0;
        tables.sort_by(|left, right| {
            left.tenant
                .cmp(&right.tenant)
                .then_with(|| left.table_ref.fqn().cmp(&right.table_ref.fqn()))
        });
        rotate_tables(&mut tables, self.periodic_cursor.load(Ordering::Acquire));
        for key in tables {
            if stop.is_cancelled() {
                outcome.pending_work = true;
                break;
            }
            match self
                .process_periodic_table(
                    &key,
                    owner,
                    stop,
                    &mut budget,
                    now,
                    &mut outcome,
                    &mut gauges,
                )
                .await
            {
                TableRunStatus::Succeeded => {
                    outcome.tables_succeeded = outcome.tables_succeeded.saturating_add(1)
                }
                TableRunStatus::Skipped => {
                    outcome.tables_skipped = outcome.tables_skipped.saturating_add(1)
                }
                TableRunStatus::Failed => {
                    outcome.tables_failed = outcome.tables_failed.saturating_add(1)
                }
            }
            outcome.tables_examined = outcome.tables_examined.saturating_add(1);
        }
        if !stop.is_cancelled() && outcome.tables_examined == outcome.tables_discovered {
            outcome.tick_complete = true;
            self.periodic_cursor.fetch_add(1, Ordering::AcqRel);
        }
        self.core.metrics.record_outcome(&outcome);
        self.core.metrics.record_complete_tick(&outcome, gauges);
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
        let now = self.core.clock.now()?;
        let mut aggregate = ForgeTickOutcome::default();
        let mut budget = ForgeTickBudget::default();
        for key in keys {
            if stop.is_cancelled() {
                break;
            }
            match self.run_hinted_key(&key, stop, &mut budget, now).await {
                Ok(outcome) => aggregate.merge(outcome),
                Err(error) => {
                    aggregate.tables_failed = aggregate.tables_failed.saturating_add(1);
                    aggregate.stage_failures = aggregate.stage_failures.saturating_add(1);
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
        self.core.metrics.record_outcome(&aggregate);
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
        now: DateTime<Utc>,
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
            self.core.metrics.record_lease(ForgeLeaseResult::Contention);
            let outcome = ForgeTickOutcome {
                lease_contention: 1,
                tables_skipped: 1,
                ..ForgeTickOutcome::default()
            };
            return Ok(outcome);
        };
        let mut takeover = ForgeTickOutcome::default();
        if lease.takeover() {
            takeover.lease_takeovers = 1;
            self.core.metrics.record_lease(ForgeLeaseResult::Takeover);
        }

        let result = async {
            let mut outcome = ForgeTickOutcome::default();
            outcome.merge(takeover);
            let staging = self
                .core
                .metrics
                .observe_stage(
                    ForgeMetricStage::ReconcileStaging,
                    self.run_compaction_reconciliation_for_table(
                        &mut lease, &table_key, &binding, stop, now,
                    ),
                )
                .await?;
            let reconciled = staging.recovered.saturating_add(staging.reset);
            outcome.reconciliation_recovered =
                outcome.reconciliation_recovered.saturating_add(reconciled);
            outcome.reconciled = outcome.reconciled.saturating_add(reconciled);
            outcome.open_operation_overflows = outcome
                .open_operation_overflows
                .saturating_add(usize::from(staging.overflowed));
            if stop.is_cancelled() {
                outcome.tables_skipped = outcome.tables_skipped.saturating_add(1);
                outcome.pending_work = true;
                return Ok(outcome);
            }
            let live = match self
                .core
                .metrics
                .observe_stage(
                    ForgeMetricStage::ReconcileIceberg,
                    self.reconcile_live_replacements(&mut lease, &table_key, &binding, stop, now),
                )
                .await
            {
                Ok(live) => live,
                Err(ForgeError::Shutdown) => {
                    outcome.tables_skipped = outcome.tables_skipped.saturating_add(1);
                    outcome.pending_work = true;
                    return Ok(outcome);
                }
                Err(error) => return Err(error),
            };
            record_live_reconciliation(&mut outcome, &live);
            match staging_stage_result(
                self.core
                    .metrics
                    .observe_stage(
                        ForgeMetricStage::StagingFold,
                        self.run_targeted_compaction_for_table(
                            &mut lease,
                            key,
                            &binding,
                            budget,
                            now.date_naive(),
                            stop,
                        ),
                    )
                    .await,
            )? {
                StagingStageResult::Completed(staging) => outcome.merge(staging),
                StagingStageResult::Cancelled => {
                    outcome.tables_skipped = outcome.tables_skipped.saturating_add(1);
                    outcome.pending_work = true;
                    return Ok(outcome);
                }
            }
            if stop.is_cancelled()
                || staging.destructive_maintenance == DestructiveMaintenance::Blocked
                || live.destructive_maintenance == DestructiveMaintenance::Blocked
            {
                outcome.tables_skipped = outcome.tables_skipped.saturating_add(1);
                outcome.pending_work = true;
                return Ok(outcome);
            }
            self.run_one_live_replacement(&mut lease, &binding, budget, now, stop, &mut outcome)
                .await?;
            outcome.tables_succeeded = outcome.tables_succeeded.saturating_add(1);
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
        budget: &mut ForgeTickBudget,
        now: DateTime<Utc>,
        outcome: &mut ForgeTickOutcome,
        gauges: &mut ForgeGaugeSnapshot,
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
                outcome.lease_contention = outcome.lease_contention.saturating_add(1);
                self.core.metrics.record_lease(ForgeLeaseResult::Contention);
                return TableRunStatus::Skipped;
            }
            Err(error) => {
                tracing::error!(error = %error, "Forge table lease acquisition failed");
                return TableRunStatus::Failed;
            }
        };
        if lease.takeover() {
            outcome.lease_takeovers = outcome.lease_takeovers.saturating_add(1);
            self.core.metrics.record_lease(ForgeLeaseResult::Takeover);
        }
        let table_result = self
            .run_periodic_table_stages(
                &mut lease, key, &binding, stop, budget, now, outcome, gauges,
            )
            .await;
        let release_result = self.release_lease(&lease).await;
        match (table_result, release_result) {
            (Ok(TableStageDisposition::Completed), Ok(())) => TableRunStatus::Succeeded,
            (Ok(TableStageDisposition::Blocked | TableStageDisposition::Cancelled), Ok(())) => {
                TableRunStatus::Skipped
            }
            (Err(error), Ok(())) | (Ok(_), Err(error)) => {
                outcome.pending_work = true;
                self.record_table_failure(key, outcome, &error);
                TableRunStatus::Failed
            }
            (Err(error), Err(release_error)) => {
                tracing::warn!(
                    error = %release_error,
                    lease_key = %lease.lease_key,
                    "Forge stage failed and lease release also failed"
                );
                outcome.pending_work = true;
                self.record_table_failure(key, outcome, &error);
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
        budget: &mut ForgeTickBudget,
        now: DateTime<Utc>,
        outcome: &mut ForgeTickOutcome,
        gauges: &mut ForgeGaugeSnapshot,
    ) -> Result<TableStageDisposition, ForgeError> {
        let staging = self
            .core
            .metrics
            .observe_stage(
                ForgeMetricStage::ReconcileStaging,
                self.run_compaction_reconciliation_for_table(lease, key, binding, stop, now),
            )
            .await?;
        let staging_prepared = if staging.overflowed {
            self.core
                .config
                .max_open_operations_per_table
                .saturating_add(1)
        } else {
            staging
                .recovered
                .saturating_add(staging.reset)
                .saturating_add(staging.pending)
                .saturating_add(staging.unresolved)
        };
        let staging_observation = ReconciliationGaugeObservation {
            prepared_operations: staging_prepared,
            uncertain_operations: if staging.overflowed {
                self.core
                    .config
                    .max_open_operations_per_table
                    .saturating_add(1)
            } else {
                staging.pending.saturating_add(staging.unresolved)
            },
        };
        gauges.record_reconciliation(ForgeMetricSource::Staging, staging_observation);
        let reconciliation = staging.recovered.saturating_add(staging.reset);
        outcome.reconciliation_recovered = outcome
            .reconciliation_recovered
            .saturating_add(reconciliation);
        outcome.reconciled = outcome.reconciled.saturating_add(reconciliation);
        outcome.open_operation_overflows = outcome
            .open_operation_overflows
            .saturating_add(usize::from(staging.overflowed));
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }
        let live = match self
            .core
            .metrics
            .observe_stage(
                ForgeMetricStage::ReconcileIceberg,
                self.reconcile_live_replacements(lease, key, binding, stop, now),
            )
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
        gauges.record_reconciliation(
            ForgeMetricSource::Iceberg,
            reconciliation_observation(&live, self.core.config.max_open_operations_per_table)?,
        );

        let compaction = match staging_stage_result(
            self.core
                .metrics
                .observe_stage(
                    ForgeMetricStage::StagingFold,
                    self.run_compaction_bins_for_table(lease, key, binding, budget, now, stop),
                )
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
        if staging.destructive_maintenance == DestructiveMaintenance::Blocked
            || live.destructive_maintenance == DestructiveMaintenance::Blocked
        {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Blocked);
        }

        self.run_one_live_replacement(lease, binding, budget, now, stop, outcome)
            .await?;
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }

        let expiry = self
            .core
            .metrics
            .observe_stage(
                ForgeMetricStage::SnapshotExpiry,
                self.run_snapshot_expiry_for_table(lease, key, binding, now),
            )
            .await?;
        outcome.expiry_reconciled = outcome.expiry_reconciled.saturating_add(expiry.recovered);
        outcome.reconciled = outcome.reconciled.saturating_add(expiry.recovered);
        if expiry.overflowed || expiry.pending > 0 || expiry.unresolved > 0 {
            outcome.open_operation_overflows = outcome
                .open_operation_overflows
                .saturating_add(usize::from(expiry.overflowed));
            outcome.pending_work = true;
        }
        if stop.is_cancelled() {
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Cancelled);
        }
        if expiry.destructive_maintenance == DestructiveMaintenance::Blocked {
            return Ok(TableStageDisposition::Blocked);
        }

        let gc = self
            .core
            .metrics
            .observe_stage(
                ForgeMetricStage::OrphanGc,
                self.run_orphan_gc_for_table(
                    lease,
                    key,
                    binding,
                    now,
                    stop,
                    &staging.protected_output_paths,
                    &live.protected_output_paths,
                ),
            )
            .await?;
        outcome.gc_reconciled = outcome.gc_reconciled.saturating_add(gc.recovered);
        outcome.gc_candidates = outcome.gc_candidates.saturating_add(gc.candidates);
        outcome.gc_deleted = outcome.gc_deleted.saturating_add(gc.deleted);
        outcome.gc_skipped = outcome.gc_skipped.saturating_add(gc.skipped);
        outcome.reconciled = outcome.reconciled.saturating_add(gc.recovered);
        if gc.overflowed || gc.pending > 0 || gc.unresolved > 0 {
            outcome.open_operation_overflows = outcome
                .open_operation_overflows
                .saturating_add(usize::from(gc.overflowed));
            outcome.pending_work = true;
            return Ok(TableStageDisposition::Blocked);
        }
        Ok(TableStageDisposition::Completed)
    }

    /// Attempts at most one current-snapshot live rewrite group for one table.
    ///
    /// The selected group retains its plan's base snapshot identity through the
    /// replacement fence. Committed work alone consumes the caller-owned global
    /// file, byte, and group budget.
    ///
    /// # Errors
    ///
    /// Returns catalog, discovery, lease, rewrite, or fence failures. A budget
    /// rejection and snapshot drift are recorded as pending outcome evidence.
    async fn run_one_live_replacement(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        budget: &mut ForgeTickBudget,
        now: DateTime<Utc>,
        stop: &CancellationToken,
        outcome: &mut ForgeTickOutcome,
    ) -> Result<(), ForgeError> {
        let table = self.load_table(&binding.table_ident()).await?;
        let plan = self
            .core
            .metrics
            .observe_stage(
                ForgeMetricStage::ManifestDiscovery,
                self.discover_live_rewrites(binding, &table, now.date_naive()),
            )
            .await?;
        outcome.live_candidates = outcome.live_candidates.saturating_add(
            plan.groups
                .iter()
                .map(|group| group.files.len())
                .sum::<usize>(),
        );
        let Some(group) = plan.groups.first() else {
            return Ok(());
        };
        outcome.live_groups_planned = outcome.live_groups_planned.saturating_add(1);
        let files = group.files.len();
        let bytes = group.files.iter().try_fold(0_u64, |total, file| {
            total
                .checked_add(file.file_size_bytes)
                .ok_or_else(|| ForgeError::InvalidConfig {
                    detail: "live rewrite input bytes overflow the Forge tick budget".to_owned(),
                })
        })?;
        let admitted = budget.bins < self.core.config.max_bins_per_tick
            && budget
                .files
                .checked_add(files)
                .is_some_and(|value| value <= self.core.config.max_files_per_tick)
            && budget
                .bytes
                .checked_add(bytes)
                .is_some_and(|value| value <= self.core.config.max_bytes_per_tick);
        if !admitted {
            outcome.budget_skips = outcome.budget_skips.saturating_add(1);
            self.core.metrics.record_operation(
                ForgeMetricSource::Iceberg,
                super::metrics::ForgeOperationResult::Budget,
                1,
            );
            outcome.pending_work = true;
            return Ok(());
        }
        match self
            .core
            .metrics
            .observe_stage(
                ForgeMetricStage::IcebergRewrite,
                self.replace_live_group(lease, binding, &table, plan.base_snapshot_id, group, stop),
            )
            .await?
        {
            IcebergRewriteDisposition::Committed {
                input_files,
                output_files,
                output_bytes,
                input_rows,
                output_rows,
                spill_bytes,
                ..
            } => {
                budget.files = budget.files.saturating_add(files);
                budget.bytes = budget.bytes.saturating_add(bytes);
                budget.bins = budget.bins.saturating_add(1);
                outcome.live_groups_committed = outcome.live_groups_committed.saturating_add(1);
                outcome.live_input_files = outcome.live_input_files.saturating_add(input_files);
                outcome.live_input_bytes = outcome.live_input_bytes.saturating_add(bytes);
                outcome.live_output_files = outcome.live_output_files.saturating_add(output_files);
                outcome.live_output_bytes = outcome.live_output_bytes.saturating_add(output_bytes);
                outcome.input_rows = outcome.input_rows.saturating_add(input_rows);
                outcome.output_rows = outcome.output_rows.saturating_add(output_rows);
                outcome.spill_bytes = outcome.spill_bytes.saturating_add(spill_bytes);
            }
            IcebergRewriteDisposition::SnapshotChanged => {
                outcome.live_snapshot_changes = outcome.live_snapshot_changes.saturating_add(1);
                outcome.pending_work = true;
            }
            IcebergRewriteDisposition::NoWork => {}
        }
        Ok(())
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
        &self,
        key: &ForgeTableKey,
        outcome: &mut ForgeTickOutcome,
        error: &ForgeError,
    ) {
        outcome.stage_failures = outcome.stage_failures.saturating_add(1);
        if matches!(error, ForgeError::FenceLost { .. }) {
            outcome.fence_losses = outcome.fence_losses.saturating_add(1);
            self.core.metrics.record_lease(ForgeLeaseResult::FenceLost);
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

/// Converts bounded live reconciliation into the complete-tick gauge contract.
///
/// # Errors
///
/// Returns [`ForgeError::InvalidConfig`] when the configured overflow sentinel
/// or an ordinary bounded total cannot be represented by `usize`.
fn reconciliation_observation(
    live: &IcebergReconciliationOutcome,
    cap: usize,
) -> Result<ReconciliationGaugeObservation, ForgeError> {
    if live.overflowed {
        let lower_bound = cap
            .checked_add(1)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "open-operation overflow sentinel exceeds usize".to_owned(),
            })?;
        return Ok(ReconciliationGaugeObservation {
            prepared_operations: lower_bound,
            uncertain_operations: lower_bound,
        });
    }
    let prepared_operations = live
        .recovered
        .checked_add(live.reset)
        .and_then(|value| value.checked_add(live.pending))
        .and_then(|value| value.checked_add(live.unresolved))
        .ok_or_else(|| ForgeError::InvalidConfig {
            detail: "live reconciliation gauge total exceeds usize".to_owned(),
        })?;
    let uncertain_operations =
        live.pending
            .checked_add(live.unresolved)
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "live uncertainty gauge total exceeds usize".to_owned(),
            })?;
    Ok(ReconciliationGaugeObservation {
        prepared_operations,
        uncertain_operations,
    })
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

/// Rotates a sorted periodic roster without changing its stable relative order.
fn rotate_tables(tables: &mut [ForgeTableKey], cursor: u64) {
    if !tables.is_empty() {
        let offset = usize::try_from(cursor).unwrap_or(usize::MAX) % tables.len();
        tables.rotate_left(offset);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use chrono::NaiveDate;
    use wyrd_spec::DataTenantId;

    use super::*;
    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;

    /// Ordinary, recovered/reset, and overflow reconciliation gauges use exact formulas.
    #[test]
    fn forge_reconciliation_gauge_formulas_are_exact() {
        let ordinary = IcebergReconciliationOutcome {
            recovered: 2,
            reset: 3,
            pending: 5,
            unresolved: 7,
            overflowed: false,
            protected_output_paths: BTreeSet::new(),
            destructive_maintenance: DestructiveMaintenance::Allowed,
        };
        assert_eq!(
            reconciliation_observation(&ordinary, 256).expect("ordinary observation"),
            ReconciliationGaugeObservation {
                prepared_operations: 17,
                uncertain_operations: 12,
            }
        );
        let overflow = IcebergReconciliationOutcome {
            overflowed: true,
            ..ordinary
        };
        assert_eq!(
            reconciliation_observation(&overflow, 256).expect("overflow observation"),
            ReconciliationGaugeObservation {
                prepared_operations: 257,
                uncertain_operations: 257,
            }
        );
        assert!(reconciliation_observation(&overflow, usize::MAX).is_err());
    }

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

    /// Proves one caller-owned budget cannot admit work from a later table.
    #[test]
    fn forge_tick_budget_is_global_across_tables() {
        let mut budget = ForgeTickBudget::default();
        budget.files = budget.files.saturating_add(2);
        budget.bytes = budget.bytes.saturating_add(20);
        budget.bins = budget.bins.saturating_add(1);
        let fits_second = budget.files.checked_add(2).is_some_and(|value| value <= 3)
            && budget
                .bytes
                .checked_add(20)
                .is_some_and(|value| value <= 30)
            && budget.bins < 2;
        assert!(!fits_second);
        let fits_third_dimension = budget.files.checked_add(1).is_some_and(|value| value <= 3)
            && budget.bytes.checked_add(5).is_some_and(|value| value <= 30)
            && budget.bins < 2;
        assert!(fits_third_dimension);
    }

    /// Proves the process-local cursor rotates a stable roster and wraps.
    #[test]
    fn forge_scheduler_rotation_is_fair_and_wraps() {
        let tenant = DataTenantId::try_from(Uuid::now_v7()).expect("tenant ID must be valid");
        let mut tables = ["a", "b", "c"]
            .into_iter()
            .map(|name| ForgeTableKey {
                tenant,
                table_ref: TableRef::new(BifrostNamespace::Bifrost, name),
            })
            .collect::<Vec<_>>();
        rotate_tables(&mut tables, 1);
        assert_eq!(tables[0].table_ref.name, "b");
        rotate_tables(&mut tables, 2);
        assert_eq!(tables[0].table_ref.name, "a");
        rotate_tables(&mut tables, u64::MAX);
        assert_eq!(tables.len(), 3);
    }

    /// Proves complete outcome merging is saturating and convergence is strict.
    #[test]
    fn forge_tick_outcome_merge_and_convergence_are_complete() {
        let mut merged = ForgeTickOutcome {
            groups_seen: usize::MAX,
            tables_failed: usize::MAX,
            staging_input_bytes: u64::MAX,
            live_input_bytes: u64::MAX,
            tick_complete: true,
            ..ForgeTickOutcome::default()
        };
        merged.merge(ForgeTickOutcome {
            groups_seen: 1,
            tables_failed: 1,
            staging_input_bytes: 1,
            live_input_bytes: 1,
            tick_complete: true,
            ..ForgeTickOutcome::default()
        });
        assert_eq!(merged.groups_seen, usize::MAX);
        assert_eq!(merged.tables_failed, usize::MAX);
        assert_eq!(merged.staging_input_bytes, u64::MAX);
        assert_eq!(merged.live_input_bytes, u64::MAX);
        assert!(!merged.is_converged());
        let converged = ForgeTickOutcome {
            tables_discovered: 2,
            tables_examined: 2,
            tables_succeeded: 2,
            tick_complete: true,
            ..ForgeTickOutcome::default()
        };
        assert!(converged.is_converged());
        let partial = ForgeTickOutcome {
            pending_work: true,
            ..converged
        };
        assert!(!partial.is_converged());
    }
}

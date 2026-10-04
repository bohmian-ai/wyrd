//! The elected leader's Iceberg maintenance timer.
//!
//! This follows `RisingWave`'s Iceberg GC loop: the leader copies its two
//! volatile maintenance sets, rewrites fragmented manifests first, then expires
//! snapshots and cleans the files that expiry made unreachable. One table's
//! failure is logged and never stops the rest. A new leader starts with empty
//! sets, so a cold table rejoins only on its next commit notice.
//!
//! Expiry and cleanup run inline on the leader's own executor as recorded
//! attempts, so their durable evidence, table lease and recovery stay the
//! worker's. Wyrd's never-published orphan sweep runs last, only for tables
//! that owe no compaction, because hot Scribe objects and uncertain outputs
//! have no `RisingWave` equivalent.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Duration;

use iceberg::spec::{FormatVersion, ManifestContentType, ManifestFile};
use iceberg::transaction::{
    ApplyTransactionAction, MANIFEST_MIN_MERGE_COUNT, MANIFEST_MIN_MERGE_COUNT_DEFAULT,
    MANIFEST_TARGET_SIZE_BYTES, MANIFEST_TARGET_SIZE_BYTES_DEFAULT, Transaction,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_operations::ForgeOperationFamily;
use vala_sql::row_types::forge_tasks::{
    ExpiredCleanupPayload, FORGE_TASK_PAYLOAD_VERSION, ForgeTaskEstimates, ForgeTaskPlan,
    ForgeTaskStrategy, NewForgeTask, ORPHAN_CLEANUP_PAYLOAD_VERSION, OrphanCleanupPayload,
};

use super::compact::ForgeGroupKey;
use super::error::ForgeError;
use super::identity::task_table_binding;
use super::leader::{ForgeSchedule, ForgeTableKey};
use super::lease::{ForgeLease, forge_lease_key};
use super::path::catalog_path_to_object_key;
use super::planner::{ForgePlanCandidate, ForgeTableSnapshot, plan_hash, plan_table};
use super::scribe_promotion::PROMOTION_BRANCH;
use super::settings::ForgeTableSettings;
use super::{Forge, ForgeWorker};
use crate::catalog::TenantTableBinding;
use crate::catalog::layout::forge_data_location;

/// Cluster key every rewritten manifest shares, as in `RisingWave`.
///
/// Output manifests are still split by partition spec inside the action.
const MANIFEST_CLUSTER: &str = "wyrd-forge-maintenance";

/// Manifests one rewrite replaces, grouped by partition spec.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ManifestRewritePlan {
    /// Paths of every selected data manifest.
    pub(super) rewrite_paths: HashSet<String>,
}

/// Selects the data manifests one rewrite merges.
///
/// Mirrors `RisingWave`'s `plan_manifest_rewrite`: data manifests below the
/// target size are grouped by partition spec, ordered oldest first by sequence
/// number, and packed into target-sized bins. A spec contributes its first
/// completed bin, or its newest under-filled bin once that bin reaches the
/// target size or the minimum merge count. Either way a bin must merge at
/// least two manifests, so a rewrite never churns one manifest into one.
pub(super) fn plan_manifest_rewrite(
    manifests: &[ManifestFile],
    target_size_bytes: u64,
    min_count_to_merge: usize,
) -> ManifestRewritePlan {
    let mut by_spec = HashMap::<i32, Vec<&ManifestFile>>::new();
    for manifest in manifests {
        if manifest.content == ManifestContentType::Data
            && u64::try_from(manifest.manifest_length).is_ok_and(|len| len < target_size_bytes)
        {
            by_spec
                .entry(manifest.partition_spec_id)
                .or_default()
                .push(manifest);
        }
    }
    let mut plan = ManifestRewritePlan::default();
    for mut candidates in by_spec.into_values() {
        candidates.sort_by_key(|manifest| manifest.sequence_number);
        let mut bin = Vec::new();
        let mut bin_bytes = 0_u64;
        let mut completed = None;
        for candidate in candidates {
            let bytes = u64::try_from(candidate.manifest_length).unwrap_or(0);
            if !bin.is_empty() && bin_bytes.saturating_add(bytes) > target_size_bytes {
                if bin.len() >= 2 {
                    completed = Some(std::mem::take(&mut bin));
                    break;
                }
                bin.clear();
                bin_bytes = 0;
            }
            bin.push(candidate);
            bin_bytes = bin_bytes.saturating_add(bytes);
        }
        let selected = match completed {
            Some(completed) => completed,
            None if bin.len() >= 2
                && (bin_bytes >= target_size_bytes || bin.len() >= min_count_to_merge) =>
            {
                bin
            }
            None => continue,
        };
        plan.rewrite_paths.extend(
            selected
                .into_iter()
                .map(|manifest| manifest.manifest_path.clone()),
        );
    }
    plan
}

/// Reads one positive integer table property, defaulting when absent.
///
/// # Errors
///
/// Returns [`ForgeError::InvalidConfig`] when the property is present but not a
/// positive integer.
fn positive_property(
    properties: &HashMap<String, String>,
    key: &str,
    default: u32,
) -> Result<u64, ForgeError> {
    let Some(raw) = properties.get(key) else {
        return Ok(u64::from(default));
    };
    raw.parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| ForgeError::InvalidConfig {
            detail: format!("{key} must be a positive integer"),
        })
}

impl Forge {
    /// Runs one leader maintenance pass over the held term's maintenance sets.
    ///
    /// Manifest rewrite runs for every rewrite member first, so expiry in the
    /// same pass can retire the manifests it replaced. Each expiry member then
    /// drains any unconsumed cleanup handoff, expires, and cleans what that
    /// expiry committed. Orphan cleanup runs last for members that owe no
    /// compaction. A replica without the term does nothing.
    ///
    /// Per-table failures are logged and the pass continues; cancellation stops
    /// before the next table.
    pub(super) async fn run_maintenance(&self, executor: &ForgeWorker, stop: &CancellationToken) {
        let Some(term) = self.leadership.held() else {
            return;
        };
        let schedule = term.schedule();
        let (rewrite, expire) = schedule.maintenance_tables();
        tracing::info!(
            manifest_rewrite_tables = rewrite.len(),
            snapshot_expiration_tables = expire.len(),
            "Forge leader maintenance pass started"
        );
        for key in &rewrite {
            if stop.is_cancelled() {
                return;
            }
            if let Err(error) = self.rewrite_manifests(schedule, key).await {
                tracing::error!(error = %error, table = %key.table.table, "Forge manifest rewrite failed");
            }
        }
        for key in &expire {
            if stop.is_cancelled() {
                return;
            }
            if let Err(error) = self.expire_snapshots(schedule, executor, key, stop).await {
                tracing::error!(error = %error, table = %key.table.table, "Forge snapshot expiration failed");
            }
        }
        let members = rewrite.into_iter().chain(expire).collect::<BTreeSet<_>>();
        for key in members {
            if stop.is_cancelled() {
                return;
            }
            if schedule.owes_compaction(&key) {
                continue;
            }
            if let Err(error) = self.clean_orphans(executor, &key, stop).await {
                tracing::error!(error = %error, table = %key.table.table, "Forge orphan cleanup failed");
            }
        }
        tracing::info!("Forge leader maintenance pass completed");
    }

    /// Resolves one key to its binding and current table and settings.
    ///
    /// # Errors
    ///
    /// Returns identity, catalog and table-property errors.
    async fn load_member(
        &self,
        key: &ForgeTableKey,
    ) -> Result<
        (
            TenantTableBinding,
            iceberg::table::Table,
            ForgeTableSettings,
        ),
        ForgeError,
    > {
        let binding = task_table_binding(key.tenant, key.tenant, &key.table)?;
        let table = self.load_table(&binding.table_ident()).await?;
        let settings = ForgeTableSettings::from_properties(table.metadata().properties())?;
        Ok((binding, table, settings))
    }

    /// Rewrites one table's fragmented data manifests under its table lease.
    ///
    /// Current settings are re-read first, as `RisingWave` does: a disabled
    /// table leaves the set. A format v3 table is skipped because row lineage
    /// forbids this rewrite. The target size and minimum merge count are the
    /// standard Iceberg commit properties.
    ///
    /// # Errors
    ///
    /// Returns catalog, lease, property and commit errors.
    async fn rewrite_manifests(
        &self,
        schedule: &ForgeSchedule,
        key: &ForgeTableKey,
    ) -> Result<(), ForgeError> {
        let (binding, table, settings) = self.load_member(key).await?;
        if !settings.manifest_rewrite_enabled {
            schedule.refresh_membership(key, &settings);
            return Ok(());
        }
        if table.metadata().format_version() >= FormatVersion::V3 {
            tracing::warn!(table = %key.table.table, "Forge manifest rewrite skipped for format v3");
            return Ok(());
        }
        let Some(snapshot) = table.metadata().snapshot_for_ref(PROMOTION_BRANCH) else {
            return Ok(());
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let properties = table.metadata().properties();
        let plan = plan_manifest_rewrite(
            manifests.entries(),
            positive_property(
                properties,
                MANIFEST_TARGET_SIZE_BYTES,
                MANIFEST_TARGET_SIZE_BYTES_DEFAULT,
            )?,
            usize::try_from(positive_property(
                properties,
                MANIFEST_MIN_MERGE_COUNT,
                MANIFEST_MIN_MERGE_COUNT_DEFAULT,
            )?)
            .unwrap_or(usize::MAX),
        );
        if plan.rewrite_paths.is_empty() {
            return Ok(());
        }
        let lease_key = forge_lease_key(
            key.tenant,
            binding.logical_namespace.as_str(),
            &binding.table_ref.name,
        );
        let Some(lease) = ForgeLease::acquire(
            &self.core.operator_pool,
            lease_key,
            Uuid::now_v7(),
            self.core.config.lease_ttl,
        )
        .await?
        else {
            tracing::debug!(table = %key.table.table, "Forge manifest rewrite deferred behind a table lease");
            return Ok(());
        };
        let committed = self.commit_manifest_rewrite(&binding, plan).await;
        if let Err(error) = lease.release(&self.core.operator_pool).await {
            tracing::warn!(error = %error, table = %key.table.table, "Forge manifest rewrite lease was not released; it will expire");
        }
        let selected = committed?;
        tracing::info!(table = %key.table.table, selected, "Forge manifest rewrite committed");
        Ok(())
    }

    /// Commits one planned manifest rewrite against the freshly loaded table.
    ///
    /// The table is reloaded under the lease so the commit starts from the
    /// head no Forge attempt can move meanwhile; the transaction keeps only the
    /// planned paths that are still in that head.
    ///
    /// Returns the number of manifests the plan selected.
    ///
    /// # Errors
    ///
    /// Returns catalog load, action and commit errors, or a timeout.
    async fn commit_manifest_rewrite(
        &self,
        binding: &TenantTableBinding,
        plan: ManifestRewritePlan,
    ) -> Result<usize, ForgeError> {
        let table = self.load_table(&binding.table_ident()).await?;
        let selected = plan.rewrite_paths.len();
        let paths = plan.rewrite_paths;
        let transaction = Transaction::new(&table);
        let transaction = transaction
            .rewrite_manifests()
            .rewrite_if(Box::new(move |manifest| {
                paths.contains(&manifest.manifest_path)
            }))
            .cluster_by(Box::new(|_| MANIFEST_CLUSTER.to_owned()))
            .set_target_branch(PROMOTION_BRANCH.to_owned())
            .apply(transaction)
            .map_err(ForgeError::Catalog)?;
        tokio::time::timeout(
            self.core.config.catalog_request_timeout,
            transaction.commit(self.core.catalog.as_ref()),
        )
        .await
        .map_err(|_| ForgeError::Timeout {
            operation: "manifest rewrite commit",
        })?
        .map_err(ForgeError::Catalog)?;
        Ok(selected)
    }

    /// Expires one table's snapshots, then cleans the files that expiry freed.
    ///
    /// The pre-checks run on the leader: a processing compaction without an
    /// observed snapshot skips the table, one with a snapshot holds the chain
    /// from the current snapshot down to it, and a table with no other
    /// replaced snapshot skips. The recorded expiry attempt then re-derives
    /// its selection from durable roots — running attempts, claims, and
    /// unresolved operations — and preparation refuses while an Oracle query
    /// still reads the table. An unconsumed handoff from an earlier
    /// failed cleanup is drained first, so a retry deletes only what is still
    /// proven unreachable.
    ///
    /// # Errors
    ///
    /// Returns catalog, SQL, clock and attempt errors.
    async fn expire_snapshots(
        &self,
        schedule: &ForgeSchedule,
        executor: &ForgeWorker,
        key: &ForgeTableKey,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        self.clean_expired(executor, key, stop).await?;
        let (binding, table, settings) = self.load_member(key).await?;
        if !settings.snapshot_expiration_enabled {
            schedule.refresh_membership(key, &settings);
            return Ok(());
        }
        let reconciliation_due = self.open_rewrite_requires_reconciliation(&binding).await?;
        let expiry_due = match schedule.processing_watermark(key) {
            Some(None) => {
                tracing::info!(table = %key.table.table, "Forge expiry skipped: a compaction has no observed snapshot");
                false
            }
            watermark => expiry_due(&table, watermark.flatten()),
        };
        let Some(task) = self
            .expiry_task(key, &table, expiry_due, reconciliation_due)
            .await?
        else {
            return Ok(());
        };
        executor
            .execute_accepted(Uuid::now_v7(), &task, stop)
            .await?;
        self.clean_expired(executor, key, stop).await
    }

    /// Builds the recorded expiry attempt for one table, when one is due.
    ///
    /// The plan keeps the canonical four-field snapshot-expiry shape. Inputs
    /// are the current manifests, or a reconcile marker when only an open
    /// rewrite needs the pass.
    ///
    /// # Errors
    ///
    /// Returns catalog and plan-construction errors.
    async fn expiry_task(
        &self,
        key: &ForgeTableKey,
        table: &iceberg::table::Table,
        expiry_due: bool,
        reconciliation_due: bool,
    ) -> Result<Option<NewForgeTask>, ForgeError> {
        if !expiry_due && !reconciliation_due {
            return Ok(None);
        }
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(None);
        };
        let mut terms = BTreeSet::new();
        if expiry_due {
            let manifests = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .map_err(ForgeError::Catalog)?;
            for manifest in manifests.entries() {
                let bytes =
                    u64::try_from(manifest.manifest_length).map_err(|_| ForgeError::Invariant {
                        detail: "Iceberg manifest length is negative".to_owned(),
                    })?;
                terms.insert((manifest.manifest_path.clone(), bytes.max(1)));
            }
        }
        if terms.is_empty() {
            terms.insert((format!("forge://reconcile/{}", snapshot.snapshot_id()), 1));
        }
        let (inputs, input_bytes): (Vec<_>, Vec<_>) = terms.into_iter().unzip();
        let bytes = input_bytes
            .iter()
            .try_fold(0_u64, |sum, bytes| sum.checked_add(*bytes))
            .ok_or_else(|| ForgeError::Invariant {
                detail: "manifest maintenance byte estimate overflowed".to_owned(),
            })?;
        let candidate = ForgePlanCandidate {
            strategy: ForgeTaskStrategy::SnapshotExpiry,
            inputs,
            input_bytes,
            bytes,
            parameters: serde_json::json!({
                "kind": "maintenance",
                "trigger_commit_count": table.metadata().snapshots().count()
                    .saturating_sub(1),
                "snapshot_expiry_due": expiry_due,
                "reconciliation_due": reconciliation_due,
            }),
        };
        let planned = plan_table(&ForgeTableSnapshot {
            snapshot_id: snapshot.snapshot_id(),
            candidates: vec![candidate],
        })?;
        Ok(planned.into_iter().next().map(|task| NewForgeTask {
            data_tenant_id: key.tenant,
            table_ref: key.table.clone(),
            strategy: task.strategy,
            base_snapshot_id: task.base_snapshot_id,
            plan: task.plan,
            plan_hash: task.plan_hash,
            estimates: task.estimates,
            ready_at: None,
        }))
    }

    /// Reports whether one table has open rewrite evidence to reconcile.
    ///
    /// Open rewrite state is correctness work, so it runs expiry's fenced
    /// reconciliation even when retention is not due.
    ///
    /// # Errors
    ///
    /// Returns identity or SQL errors.
    async fn open_rewrite_requires_reconciliation(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<bool, ForgeError> {
        let resource = ForgeGroupKey::table_audit_resource(binding.tenant, &binding.table_ref);
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::IcebergRewrite)
            .map_err(ForgeError::Sql)?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = operations
            .list_open(&mut conn, 1, None)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        Ok(!page.operations.is_empty() || page.overflowed)
    }

    /// Runs the cleanup an unconsumed expiration handoff names, if any.
    ///
    /// # Errors
    ///
    /// Returns the handoff read, projection and attempt errors.
    async fn clean_expired(
        &self,
        executor: &ForgeWorker,
        key: &ForgeTableKey,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let Some(payload) = ForgeTasks::new(self.core.operator_pool.clone())
            .unconsumed_expiration_handoff(key.tenant, &key.table)
            .await
            .map_err(ForgeError::Sql)?
        else {
            return Ok(());
        };
        let task = cleanup_projection(key, &payload)?;
        executor
            .execute_accepted(Uuid::now_v7(), &task, stop)
            .await?;
        Ok(())
    }

    /// Runs one never-published orphan sweep for one table.
    ///
    /// The cutoff is fixed once here from the pass's clock, and the scan
    /// prefix is the table's current Forge data root, so the attempt checks
    /// current Iceberg, Scribe and Oracle roots before each deletion.
    ///
    /// # Errors
    ///
    /// Returns identity, catalog, clock, plan and attempt errors.
    async fn clean_orphans(
        &self,
        executor: &ForgeWorker,
        key: &ForgeTableKey,
        stop: &CancellationToken,
    ) -> Result<(), ForgeError> {
        let (binding, table, _) = self.load_member(key).await?;
        let location = table.metadata().location().to_owned();
        let scan_prefix = catalog_path_to_object_key(
            &location,
            &binding,
            &self.core.staging,
            &forge_data_location(&location),
        )?;
        let base_snapshot_id = table
            .metadata()
            .current_snapshot()
            .map_or(0, |snapshot| snapshot.snapshot_id());
        let task = orphan_cleanup_task(
            key,
            base_snapshot_id,
            scan_prefix,
            self.core.clock.now()?,
            self.core.config.orphan_gc_ttl,
        )?;
        executor
            .execute_accepted(Uuid::now_v7(), &task, stop)
            .await?;
        Ok(())
    }
}

/// Whether some retained snapshot is replaced and not held by a compaction.
///
/// The current snapshot is never due. A held compaction snapshot keeps the
/// chain from the current snapshot down to and including it, because the
/// rewrite's commit-time validation walks that chain; a held snapshot the
/// table no longer retains skips the table. This is only the leader's
/// pre-check: the recorded attempt re-derives its selection from every
/// durable root before preparing anything.
fn expiry_due(table: &iceberg::table::Table, watermark: Option<i64>) -> bool {
    let metadata = table.metadata();
    let Some(current) = metadata.current_snapshot_id() else {
        return false;
    };
    let mut held = HashSet::from([current]);
    if let Some(snapshot_id) = watermark {
        if metadata.snapshot_by_id(snapshot_id).is_none() {
            return false;
        }
        let mut cursor = metadata
            .snapshot_by_id(current)
            .and_then(|snapshot| snapshot.parent_snapshot_id());
        while let Some(id) = cursor.filter(|_| !held.contains(&snapshot_id)) {
            if !held.insert(id) {
                break;
            }
            cursor = metadata
                .snapshot_by_id(id)
                .and_then(|snapshot| snapshot.parent_snapshot_id());
        }
    }
    metadata
        .snapshots()
        .any(|snapshot| !held.contains(&snapshot.snapshot_id()))
}

/// Builds the orphan-cleanup attempt for one table at one fixed cut.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the TTL or cutoff is unrepresentable.
fn orphan_cleanup_task(
    key: &ForgeTableKey,
    base_snapshot_id: i64,
    scan_prefix: String,
    now: chrono::DateTime<chrono::Utc>,
    ttl: Duration,
) -> Result<NewForgeTask, ForgeError> {
    let ttl = chrono::Duration::from_std(ttl).map_err(|_| ForgeError::Invariant {
        detail: "Forge orphan GC TTL exceeds the representable range".to_owned(),
    })?;
    let age_cutoff_ms = now
        .checked_sub_signed(ttl)
        .ok_or_else(|| ForgeError::Invariant {
            detail: "Forge orphan cleanup cutoff underflows the representable range".to_owned(),
        })?
        .timestamp_millis();
    let plan = ForgeTaskPlan {
        version: FORGE_TASK_PAYLOAD_VERSION,
        inputs: vec![scan_prefix],
        parameters: OrphanCleanupPayload {
            version: ORPHAN_CLEANUP_PAYLOAD_VERSION,
            age_cutoff_ms,
        }
        .to_value(),
    };
    let plan_hash = plan_hash(&plan)?;
    Ok(NewForgeTask {
        data_tenant_id: key.tenant,
        table_ref: key.table.clone(),
        strategy: ForgeTaskStrategy::OrphanCleanup,
        base_snapshot_id,
        plan,
        plan_hash,
        estimates: ForgeTaskEstimates { files: 1, bytes: 1 },
        ready_at: None,
    })
}

/// Builds the bounded cleanup attempt one expiration handoff projects to.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the candidate count exceeds `u32`.
pub fn cleanup_projection(
    key: &ForgeTableKey,
    payload: &ExpiredCleanupPayload,
) -> Result<NewForgeTask, ForgeError> {
    let files =
        u32::try_from(payload.cleanup_candidates.len()).map_err(|_| ForgeError::Invariant {
            detail: "expired cleanup candidate count exceeds u32".to_owned(),
        })?;
    let plan = ForgeTaskPlan {
        version: FORGE_TASK_PAYLOAD_VERSION,
        inputs: Vec::new(),
        parameters: payload.to_value(),
    };
    let plan_hash = plan_hash(&plan)?;
    Ok(NewForgeTask {
        data_tenant_id: key.tenant,
        table_ref: key.table.clone(),
        strategy: ForgeTaskStrategy::ExpiredCleanup,
        base_snapshot_id: payload.committed_snapshot_id,
        plan,
        plan_hash,
        estimates: ForgeTaskEstimates {
            files,
            bytes: payload.serialized_candidate_bytes().max(1),
        },
        ready_at: None,
    })
}

#[cfg(test)]
mod tests {
    use iceberg::spec::{ManifestContentType, ManifestFile};

    use super::plan_manifest_rewrite;

    /// Builds one manifest-list entry with the fields the planner reads.
    fn manifest(path: &str, length: i64, spec: i32, sequence: i64) -> ManifestFile {
        ManifestFile {
            manifest_path: path.to_owned(),
            manifest_length: length,
            partition_spec_id: spec,
            content: ManifestContentType::Data,
            sequence_number: sequence,
            min_sequence_number: sequence,
            added_snapshot_id: 1,
            added_files_count: Some(1),
            existing_files_count: Some(0),
            deleted_files_count: Some(0),
            added_rows_count: Some(1),
            existing_rows_count: Some(0),
            deleted_rows_count: Some(0),
            partitions: None,
            key_metadata: None,
            first_row_id: None,
        }
    }

    /// Paths a plan selected, sorted for comparison.
    fn selected(manifests: &[ManifestFile], target: u64, min_count: usize) -> Vec<String> {
        let mut paths = plan_manifest_rewrite(manifests, target, min_count)
            .rewrite_paths
            .into_iter()
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    /// Fragmented same-spec manifests merge; one manifest or split specs do not.
    #[test]
    fn manifest_plan_merges_only_fragmented_same_spec_manifests() {
        let same_spec = [manifest("a", 10, 0, 1), manifest("b", 10, 0, 2)];
        assert_eq!(selected(&same_spec, 1_000, 2), ["a", "b"]);
        assert!(selected(&same_spec[..1], 1_000, 2).is_empty());
        let split = [manifest("a", 10, 0, 1), manifest("b", 10, 1, 2)];
        assert!(selected(&split, 1_000, 2).is_empty());
        assert!(selected(&same_spec, 1_000, 3).is_empty());
    }

    /// A completed target-sized bin merges oldest first; deletes never merge.
    #[test]
    fn manifest_plan_takes_the_oldest_full_bin() {
        let mut delete = manifest("d", 10, 0, 0);
        delete.content = ManifestContentType::Deletes;
        let manifests = [
            delete,
            manifest("c", 40, 0, 3),
            manifest("a", 40, 0, 1),
            manifest("b", 40, 0, 2),
            manifest("big", 500, 0, 4),
        ];
        assert_eq!(selected(&manifests, 100, 100), ["a", "b"]);
    }
}

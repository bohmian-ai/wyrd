//! Iceberg snapshot expiry for the Forge maintenance loop.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use iceberg::transaction::{
    ApplyTransactionAction, CleanupTraversalLimits, ExpiredFileSet, Transaction,
    expired_files_between,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::ForgeTasks;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use vala_sql::row_types::forge_tasks::{ForgeTaskTableIdentity, SnapshotWatermark};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeSnapshotExpirePhase,
    StoragePath,
};

use crate::catalog::{TableRef, TenantTableBinding};
#[cfg(test)]
use crate::namespaces::BifrostNamespace;

use super::Forge;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::expiry_policy::{SnapshotExpiryDecision, SnapshotExpiryPolicy, SnapshotProtectionRoots};
use super::lease::ForgeLease;
#[cfg(feature = "test-support")]
use super::lease::forge_lease_key;

const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

/// The minimum information needed to select snapshots without relying on
/// Iceberg's numeric snapshot identifiers for ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotSummary {
    /// Snapshot identifier.
    pub id: i64,
    /// Parent in the snapshot ancestry.
    pub parent_id: Option<i64>,
    /// Creation time in Unix milliseconds.
    pub timestamp_ms: i64,
}

/// Bounded snapshot-expiry reconciliation evidence for one table pass.
#[derive(Debug)]
pub(crate) struct ExpiryReconciliationOutcome {
    /// Prepared operations completed or proven externally complete.
    pub(crate) recovered: usize,
    /// Young prepared operations retained inside the uncertainty bound.
    pub(crate) pending: usize,
    /// Prepared selections no longer safe to replay.
    pub(crate) unresolved: usize,
    /// Fail-closed destructive-maintenance disposition.
    pub(crate) destructive_maintenance: super::live_reconcile::DestructiveMaintenance,
    /// Exact metadata-derived paths made unreachable by this pass.
    pub(crate) expired_files: ExpiredFileSet,
    /// Terminal expiry transitions delayed until task evidence is durable.
    pub(crate) terminals: Vec<PendingExpiryTerminal>,
}

/// One committed expiry operation awaiting its terminal audit transition.
#[derive(Debug)]
pub(crate) struct PendingExpiryTerminal {
    /// Canonical Prepared detail retained by the operation projection.
    pub(crate) detail: AuditDetail,
    /// Whether reconciliation, rather than the first caller, proved the commit.
    pub(crate) recovered: bool,
}

impl Default for ExpiryReconciliationOutcome {
    /// Starts one expiry reconciliation in the allowed, empty state.
    fn default() -> Self {
        Self {
            recovered: 0,
            pending: 0,
            unresolved: 0,
            destructive_maintenance: super::live_reconcile::DestructiveMaintenance::Allowed,
            expired_files: ExpiredFileSet::default(),
            terminals: Vec::new(),
        }
    }
}

/// Selects only snapshots older than `cutoff_ms` that are not current, ref
/// heads, or among the retained ancestry of a current/ref head.
#[must_use]
pub fn select_expirable_snapshots(
    snapshots: &[SnapshotSummary],
    current_snapshot_id: Option<i64>,
    ref_heads: &[i64],
    cutoff_ms: i64,
    retain_last: usize,
) -> Vec<i64> {
    let by_id = snapshots
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<HashMap<_, _>>();
    let mut heads = ref_heads.iter().copied().collect::<HashSet<_>>();
    if let Some(current) = current_snapshot_id {
        heads.insert(current);
    }
    let mut protected = HashSet::new();
    let retained_count = retain_last.max(1);
    for head in heads {
        let mut cursor = Some(head);
        for _ in 0..retained_count {
            let Some(id) = cursor else { break };
            if !protected.insert(id) {
                break;
            }
            cursor = by_id.get(&id).and_then(|snapshot| snapshot.parent_id);
        }
    }
    let mut selected = snapshots
        .iter()
        .filter(|snapshot| snapshot.timestamp_ms < cutoff_ms && !protected.contains(&snapshot.id))
        .map(|snapshot| snapshot.id)
        .collect::<Vec<_>>();
    selected.sort_unstable();
    selected
}

impl Forge {
    /// Discover the ordered durable table set for one periodic tick.
    ///
    /// # Errors
    ///
    /// Returns a SQL error when no complete discovery set can be formed.
    pub(super) async fn discover_tables(&self) -> Result<(Vec<ForgeTableKey>, usize), ForgeError> {
        self.discover_tables_inner().await
    }

    /// Reconcile and expire snapshots while retaining the shared table fence.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, SQL, audit, or reconciliation failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops before the next effect. If it races the catalog
    /// commit, the Prepared operation remains open and reconciliation is required.
    pub(super) async fn run_snapshot_expiry_for_table(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        now: DateTime<Utc>,
        stop: &CancellationToken,
    ) -> Result<ExpiryReconciliationOutcome, ForgeError> {
        self.run_snapshot_expiry_for_table_inner(lease, key, binding, now, stop)
            .await
    }

    /// Runs one table-scoped snapshot-expiry pass for integration fixtures.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, SQL, audit, or reconciliation failures from the
    /// unchanged production expiry owner.
    #[cfg(feature = "test-support")]
    pub async fn run_snapshot_expiry_for_test(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<usize, ForgeError> {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let lease_key = forge_lease_key(
            binding.tenant,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let mut lease = ForgeLease::acquire(
            &self.core.operator_pool,
            lease_key,
            Uuid::now_v7(),
            self.core.config.lease_ttl,
        )
        .await?
        .ok_or_else(|| ForgeError::FenceLost {
            lease_key: format!("forge:table:{}:{}", binding.tenant, binding.table_ref),
        })?;
        let mut outcome = self
            .run_snapshot_expiry_for_table(
                &mut lease,
                &key,
                binding,
                self.core.clock.now()?,
                &CancellationToken::new(),
            )
            .await;
        if let Ok(value) = &mut outcome {
            self.finalize_expiry_terminals(
                &mut lease,
                binding.tenant,
                std::mem::take(&mut value.terminals),
            )
            .await?;
        }
        lease.release(&self.core.operator_pool).await?;
        outcome.map(|outcome| outcome.recovered)
    }

    /// Stops at the injected post-commit/pre-task-evidence boundary.
    ///
    /// The returned counts prove that exact metadata candidates and an open
    /// expiry transition survive together before the worker may close either.
    ///
    /// # Errors
    ///
    /// Returns the production lease, catalog, watermark, expiry, and traversal
    /// failures. The committed expiry deliberately remains Prepared for replay.
    #[cfg(feature = "test-support")]
    pub async fn run_snapshot_expiry_commit_boundary_for_test(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<(usize, usize), ForgeError> {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let lease_key = forge_lease_key(
            binding.tenant,
            &binding.logical_namespace,
            &binding.table_name,
        );
        let mut lease = ForgeLease::acquire(
            &self.core.operator_pool,
            lease_key,
            Uuid::now_v7(),
            self.core.config.lease_ttl,
        )
        .await?
        .ok_or_else(|| ForgeError::FenceLost {
            lease_key: format!("forge:table:{}:{}", binding.tenant, binding.table_ref),
        })?;
        let outcome = self
            .run_snapshot_expiry_for_table(
                &mut lease,
                &key,
                binding,
                self.core.clock.now()?,
                &CancellationToken::new(),
            )
            .await?;
        let candidates = outcome.expired_files.data_files.len()
            + outcome.expired_files.delete_files.len()
            + outcome.expired_files.manifests.len()
            + outcome.expired_files.manifest_lists.len()
            + outcome.expired_files.statistics.len()
            + outcome.expired_files.metadata_logs.len();
        let terminals = outcome.terminals.len();
        lease.release(&self.core.operator_pool).await?;
        Ok((candidates, terminals))
    }
}

/// Discover tenant/table pairs from active durable catalog registrations.
///
/// Invalid rows are counted and skipped so one malformed table identity does
/// not prevent maintenance for the remaining tables.
impl Forge {
    async fn discover_tables_inner(&self) -> Result<(Vec<ForgeTableKey>, usize), ForgeError> {
        let rows = vala_sql::queries::forge_catalog_operator::list_active_tables_for_operator(
            &self.core.operator_pool,
        )
        .await
        .map_err(ForgeError::Sql)?;
        let mut failures = 0;
        let mut tables = Vec::new();
        for row in rows {
            let table: Result<ForgeTableKey, ForgeError> = (|| {
                let tenant = DataTenantId::try_from(row.data_tenant_id).map_err(|error| {
                    ForgeError::SnapshotExpiry {
                        detail: error.to_string(),
                    }
                })?;
                let table_ref =
                    TableRef::parse_fqn(&row.fqn).ok_or_else(|| ForgeError::SnapshotExpiry {
                        detail: format!("invalid registered Bifrost FQN `{}`", row.fqn),
                    })?;
                Ok(ForgeTableKey { tenant, table_ref })
            })();
            match table {
                Ok(table) => tables.push(table),
                Err(error) => {
                    failures += 1;
                    tracing::warn!(error = %error, "Forge table discovery skipped an invalid row");
                }
            }
        }
        Ok((tables, failures))
    }

    /// Reconcile prepared snapshot-expiry audits, then expire eligible snapshots.
    ///
    /// Current and reference heads, plus their retained ancestry, are protected by
    /// [`select_expirable_snapshots`]. Recovery of an accepted Prepared operation
    /// ends the pass so a successor never submits a second expiry effect from the
    /// same metadata load. New selections reload metadata before commit.
    async fn run_snapshot_expiry_for_table_inner(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        now: DateTime<Utc>,
        stop: &CancellationToken,
    ) -> Result<ExpiryReconciliationOutcome, ForgeError> {
        require_running(stop)?;
        let mut outcome = self
            .reconcile_expiry(lease, key, binding, now, stop)
            .await?;
        if outcome.recovered > 0 {
            return Ok(outcome);
        }
        if outcome.destructive_maintenance == super::live_reconcile::DestructiveMaintenance::Blocked
        {
            return Ok(outcome);
        }
        require_running(stop)?;
        lease.require_fence(&self.core.operator_pool).await?;
        let table = self.load_table(&binding.table_ident()).await?;
        let retention_cutoff_ms = expiry_cutoff_ms(now, self.core.config.snapshot_retention)?;
        let roots = self
            .snapshot_protection_roots(key, &table, outcome.destructive_maintenance)
            .await?;
        let (summaries, ref_heads) = snapshot_summaries(&table)?;
        let decision = SnapshotExpiryPolicy {
            snapshots: &summaries,
            current_snapshot_id: table.metadata().current_snapshot_id(),
            ref_heads: &ref_heads,
            roots: &roots,
            retention_cutoff_ms,
            retain_last: self.core.config.retain_last,
            traversal_limit: self.core.config.max_retained_snapshots_per_table,
        }
        .decide()?;
        let SnapshotExpiryDecision::Expire {
            snapshot_ids,
            cutoff_ms,
        } = decision
        else {
            return Ok(outcome);
        };
        let doomed: Vec<i64> = snapshot_ids.clone();
        let detail = expiry_detail(&table, key, cutoff_ms, snapshot_ids, ref_heads)?;
        require_running(stop)?;
        lease.require_fence(&self.core.operator_pool).await?;
        self.append_expiry_audit(lease, key.tenant, &detail, "forge.snapshot_expire.prepared")
            .await?;
        require_running(stop)?;
        self.revalidate_reader_protection(key, &doomed, cutoff_ms)
            .await?;
        outcome.expired_files = self
            .complete_expiry(lease, key, binding, &detail, stop)
            .await?;
        outcome.terminals.push(PendingExpiryTerminal {
            detail,
            recovered: false,
        });
        outcome.recovered = outcome.recovered.saturating_add(1);
        Ok(outcome)
    }
}

impl Forge {
    /// Re-reads reader protection immediately before the destructive commit.
    ///
    /// The selection was decided against a read of the reader watermarks taken
    /// before planning, the prepared audit, and the fence refresh. A query
    /// admitted in that window publishes its pin durably before it reads, so
    /// re-reading here is what turns "nobody needed this when we looked" into
    /// "nobody needs this now". The pass is abandoned rather than narrowed: the
    /// prepared operation settles on its own reconciliation path, and a
    /// successor re-decides against the newer reader set.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the watermarks cannot be re-read and
    /// [`ForgeError::SnapshotExpiry`] when the bounded query overflows or a
    /// live reader now depends on a snapshot this pass intended to expire.
    async fn revalidate_reader_protection(
        &self,
        key: &ForgeTableKey,
        doomed: &[i64],
        cutoff_ms: i64,
    ) -> Result<(), ForgeError> {
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let readers = super::reader_protection::ReaderProtection::new(&mut conn)
            .watermarks(key.tenant, &key.table_ref)
            .await?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        for reader in &readers {
            if doomed.contains(&reader.snapshot_id) || reader.timestamp_ms <= cutoff_ms {
                return Err(ForgeError::SnapshotExpiry {
                    detail: format!(
                        "a live reader on snapshot {} at {}ms was admitted after this expiry was \
                         decided against cutoff {cutoff_ms}ms",
                        reader.snapshot_id, reader.timestamp_ms
                    ),
                });
            }
        }
        Ok(())
    }

    /// Gathers every non-catalog authority that protects a snapshot.
    ///
    /// The three roots are read together, under the fence this pass already
    /// holds, because a decision made from two of them is not a smaller
    /// decision — it is an unsafe one. Watermarks are returned unvalidated;
    /// corroborating them against Iceberg belongs to the policy, which is the
    /// owner that also knows the ancestry they must be reachable from.
    ///
    /// # Errors
    ///
    /// Returns SQL or identity failures from the durable watermark reads, and
    /// [`ForgeError::SnapshotExpiry`] when either bounded query overflowed,
    /// which means the protected set is not provably complete.
    async fn snapshot_protection_roots(
        &self,
        key: &ForgeTableKey,
        table: &iceberg::table::Table,
        destructive_maintenance: super::live_reconcile::DestructiveMaintenance,
    ) -> Result<SnapshotProtectionRoots, ForgeError> {
        let identity = ForgeTaskTableIdentity::new(
            crate::catalog::BIFROST_CATALOG_NAME,
            key.table_ref.namespace.as_str(),
            key.table_ref.name.as_str(),
        )
        .map_err(ForgeError::Sql)?;
        let cap = u32::try_from(self.core.config.max_open_operations_per_table).map_err(|_| {
            ForgeError::InvalidConfig {
                detail: "Forge active-watermark cap exceeds u32".to_owned(),
            }
        })?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let (attempt_watermarks, attempts_overflowed) =
            ForgeTasks::new(self.core.operator_pool.clone())
                .watermarks(&mut conn, &identity, cap)
                .await
                .map_err(ForgeError::Sql)?;
        // Reader protection is not bounded by the open-operation cap: a
        // frontier is one member per incomparable lineage, and truncating it
        // would silently stop protecting one of them. A frontier that cannot be
        // read or validated fails the pass instead.
        let reader_watermarks = super::reader_protection::ReaderProtection::new(&mut conn)
            .watermarks(key.tenant, &key.table_ref)
            .await?;
        // Snapshots another prepared expiration already claimed are removed
        // from this pass in the same transaction the other roots are read in.
        // Preparation re-checks the claim index under the table lock; this read
        // only keeps a pass from planning work that is already owned.
        let claimed_snapshot_ids = super::reader_protection::ReaderProtection::new(&mut conn)
            .claimed_snapshot_ids(key.tenant, &key.table_ref)
            .await?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if attempts_overflowed {
            return Err(ForgeError::SnapshotExpiry {
                detail: "active Forge protection set exceeded its bounded query".to_owned(),
            });
        }
        Ok(SnapshotProtectionRoots {
            attempt_watermarks,
            reader_watermarks,
            lineage_snapshot_id: head_lineage_snapshot_id(table),
            claimed_snapshot_ids,
            destructive_maintenance,
        })
    }
}

/// Reads the base snapshot the branch head's rewrite lineage still references.
///
/// A head that is not a Forge rewrite snapshot protects no lineage, and a head
/// whose properties do not parse as a complete rewrite identity is not lineage
/// this owner wrote. Both are `None` rather than an error: neither is a reason
/// to refuse expiry, only a reason not to protect an extra snapshot.
fn head_lineage_snapshot_id(table: &iceberg::table::Table) -> Option<i64> {
    let properties = table
        .metadata()
        .current_snapshot()?
        .summary()
        .additional_properties
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    super::publication::RewriteSnapshotProperties::validate(&properties)
        .ok()
        .map(|rewrite| rewrite.base_snapshot_id)
}

/// Validates watermark identity/timestamp parity and returns a timestamp cutoff.
///
/// Snapshot identifiers are used only for ancestry lookup; ordering is based
/// exclusively on timestamps so non-monotonic Iceberg IDs remain valid.
///
/// # Errors
///
/// Returns snapshot-expiry failure when a watermark is absent from retained
/// ancestry, the ancestry graph is malformed or over its bound, or its
/// persisted timestamp does not match Iceberg metadata.
pub(super) fn validate_watermarks(
    snapshots: &[SnapshotSummary],
    current_snapshot_id: Option<i64>,
    ref_heads: &[i64],
    watermarks: &[SnapshotWatermark],
    retention_cutoff_ms: i64,
    traversal_limit: usize,
) -> Result<i64, ForgeError> {
    let by_id = snapshots
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<HashMap<_, _>>();
    let mut heads = ref_heads.iter().copied().collect::<HashSet<_>>();
    if let Some(current) = current_snapshot_id {
        heads.insert(current);
    }
    let table_is_empty = snapshots.is_empty() && heads.is_empty();
    if table_is_empty {
        return watermarks
            .iter()
            .try_fold(retention_cutoff_ms, |cutoff, watermark| {
                if watermark.snapshot_id == 0 && watermark.timestamp_ms == 0 {
                    Ok(cutoff.min(0))
                } else {
                    Err(ForgeError::SnapshotExpiry {
                        detail: "non-sentinel watermark protects an empty Iceberg table".to_owned(),
                    })
                }
            });
    }
    if watermarks
        .iter()
        .any(|watermark| watermark.snapshot_id == 0 || watermark.timestamp_ms == 0)
    {
        return Err(ForgeError::SnapshotExpiry {
            detail: "sentinel watermark is invalid for a non-empty Iceberg table".to_owned(),
        });
    }
    let mut reachable = HashSet::new();
    for head in heads {
        let mut path = HashSet::new();
        let mut cursor = Some(head);
        while let Some(id) = cursor {
            if !path.insert(id) {
                return Err(ForgeError::SnapshotExpiry {
                    detail: format!("Iceberg snapshot ancestry contains a cycle at {id}"),
                });
            }
            if reachable.len() >= traversal_limit && !reachable.contains(&id) {
                return Err(ForgeError::SnapshotExpiry {
                    detail: "Iceberg snapshot ancestry exceeded its traversal bound".to_owned(),
                });
            }
            reachable.insert(id);
            let snapshot = by_id.get(&id).ok_or_else(|| ForgeError::SnapshotExpiry {
                detail: format!("Iceberg snapshot ancestry is missing snapshot {id}"),
            })?;
            cursor = snapshot.parent_id;
        }
    }
    watermarks
        .iter()
        .try_fold(retention_cutoff_ms, |cutoff, watermark| {
            let snapshot =
                by_id
                    .get(&watermark.snapshot_id)
                    .ok_or_else(|| ForgeError::SnapshotExpiry {
                        detail: format!(
                            "active Forge watermark snapshot {} is not retained",
                            watermark.snapshot_id
                        ),
                    })?;
            if !reachable.contains(&watermark.snapshot_id) {
                return Err(ForgeError::SnapshotExpiry {
                    detail: format!(
                        "active Forge watermark snapshot {} is detached from approved heads",
                        watermark.snapshot_id
                    ),
                });
            }
            if snapshot.timestamp_ms != watermark.timestamp_ms {
                return Err(ForgeError::SnapshotExpiry {
                    detail: format!(
                        "active Forge watermark {} timestamp does not match Iceberg ancestry",
                        watermark.snapshot_id
                    ),
                });
            }
            Ok(cutoff.min(watermark.timestamp_ms))
        })
}

/// Rejects entry into a new expiry effect after operation authority is cancelled.
///
/// # Errors
///
/// Returns [`ForgeError::Shutdown`] when the shared claim/lease/shutdown token is cancelled.
fn require_running(stop: &CancellationToken) -> Result<(), ForgeError> {
    if stop.is_cancelled() {
        Err(ForgeError::Shutdown)
    } else {
        Ok(())
    }
}

/// Convert a retention duration into the UTC millisecond cutoff used by Iceberg.
fn expiry_cutoff_ms(now: DateTime<Utc>, retention: Duration) -> Result<i64, ForgeError> {
    let retention_ms =
        i64::try_from(retention.as_millis()).map_err(|_| ForgeError::InvalidConfig {
            detail: "snapshot_retention exceeds an i64 millisecond timestamp".to_owned(),
        })?;
    now.timestamp_millis()
        .checked_sub(retention_ms)
        .ok_or_else(|| ForgeError::InvalidConfig {
            detail: "snapshot retention cutoff overflows UTC milliseconds".to_owned(),
        })
}

/// Extract snapshot timestamps, parent links, and every metadata reference head.
///
/// # Errors
///
/// Returns snapshot-expiry failure when metadata serialization fails or any
/// named reference does not carry a resolvable snapshot head.
fn snapshot_summaries(
    table: &iceberg::table::Table,
) -> Result<(Vec<SnapshotSummary>, Vec<i64>), ForgeError> {
    let summaries = table
        .metadata()
        .snapshots()
        .map(|snapshot| SnapshotSummary {
            id: snapshot.snapshot_id(),
            parent_id: snapshot.parent_snapshot_id(),
            timestamp_ms: snapshot.timestamp_ms(),
        })
        .collect::<Vec<_>>();
    let metadata =
        serde_json::to_value(table.metadata()).map_err(|error| ForgeError::SnapshotExpiry {
            detail: format!("serialize Iceberg references: {error}"),
        })?;
    let ref_heads = metadata
        .get("refs")
        .and_then(serde_json::Value::as_object)
        .map(|refs| {
            refs.values()
                .map(|value| {
                    value
                        .get("snapshot-id")
                        .or_else(|| value.get("snapshot_id"))
                        .and_then(serde_json::Value::as_i64)
                        .ok_or_else(|| ForgeError::SnapshotExpiry {
                            detail: "Iceberg metadata reference has no resolvable snapshot head"
                                .to_owned(),
                        })
                })
                .collect::<Result<Vec<_>, ForgeError>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok((summaries, ref_heads))
}

/// Build canonical audit detail for a snapshot-expiry selection.
fn expiry_detail(
    table: &iceberg::table::Table,
    key: &ForgeTableKey,
    cutoff_ms: i64,
    mut selected_snapshot_ids: Vec<i64>,
    mut retained_ref_heads: Vec<i64>,
) -> Result<AuditDetail, ForgeError> {
    selected_snapshot_ids.sort_unstable();
    retained_ref_heads.sort_unstable();
    let base_metadata_location = table
        .metadata_location_result()
        .map_err(ForgeError::Catalog)
        .and_then(|location| {
            StoragePath::new(location.to_owned()).map_err(|error| ForgeError::SnapshotExpiry {
                detail: error.to_string(),
            })
        })?;
    Ok(AuditDetail::ForgeSnapshotExpire {
        operation_id: expiry_operation_id(key, cutoff_ms, &selected_snapshot_ids),
        phase: ForgeSnapshotExpirePhase::Prepared,
        group: table_resource_for_key(key),
        base_metadata_location,
        current_snapshot_id: table.metadata().current_snapshot_id(),
        retained_ref_heads,
        cutoff_ms,
        selected_snapshot_ids,
    })
}

/// Validates and borrows the canonical selection encoded by expiry audit detail.
///
/// # Errors
///
/// Returns [`ForgeError::SnapshotExpiry`] when `detail` is not snapshot-expiry
/// evidence for `key`, or when its selected snapshot identifiers are not a
/// non-empty strictly increasing sequence.
fn expiry_selection<'detail>(
    detail: &'detail AuditDetail,
    key: &ForgeTableKey,
) -> Result<(&'detail [i64], i64), ForgeError> {
    let AuditDetail::ForgeSnapshotExpire {
        operation_id: _,
        selected_snapshot_ids,
        cutoff_ms,
        group,
        ..
    } = detail
    else {
        return Err(ForgeError::SnapshotExpiry {
            detail: "expiry audit detail has the wrong kind".to_owned(),
        });
    };
    if group != &table_resource_for_key(key)
        || selected_snapshot_ids.is_empty()
        || selected_snapshot_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(ForgeError::SnapshotExpiry {
            detail: "expiry audit detail is not canonical".to_owned(),
        });
    }
    Ok((selected_snapshot_ids, *cutoff_ms))
}

/// Commit an expiry selection after rechecking metadata and the lease fence.
impl Forge {
    /// Submits the canonical expiry transaction while preserving uncertain acceptance.
    ///
    /// # Errors
    ///
    /// Returns validation, lease, catalog, traversal, or reconciliation failures.
    /// Timeout or cancellation after submission returns reconciliation-required
    /// and deliberately leaves the caller's Prepared operation open.
    ///
    /// # Cancellation
    ///
    /// Cancellation is raced against the in-flight catalog commit. The future's
    /// cancellation is never treated as proof that the remote commit was rejected.
    async fn complete_expiry(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        detail: &AuditDetail,
        stop: &CancellationToken,
    ) -> Result<ExpiredFileSet, ForgeError> {
        let (selected_snapshot_ids, cutoff_ms) = expiry_selection(detail, key)?;
        if !lease.renew(&self.core.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let table = self.load_table(&binding.table_ident()).await?;
        if !selected_ids_are_eligible(
            &table,
            selected_snapshot_ids,
            cutoff_ms,
            self.core.config.retain_last,
        )? {
            return Err(ForgeError::SnapshotExpiry {
                detail: "reloaded Iceberg metadata no longer matches the prepared selection"
                    .to_owned(),
            });
        }
        let tx = Transaction::new(&table);
        let action = tx
            .expire_snapshots()
            .expire_snapshot_ids(selected_snapshot_ids.iter().copied())
            .expire_older_than_ms(cutoff_ms)
            .retain_last(self.core.config.retain_last.max(1));
        let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        require_running(stop)?;
        let commit = transaction.commit(self.core.catalog.as_ref());
        tokio::pin!(commit);
        let committed = tokio::select! {
            response = tokio::time::timeout(
                self.core.config.iceberg_total_retry_timeout,
                &mut commit,
            ) => match response {
                Ok(Ok(table)) => table,
                Ok(Err(error)) => return Err(ForgeError::Catalog(error)),
                Err(_) => return Err(ForgeError::Reconciliation {
                    detail: "Iceberg snapshot expiry commit timed out with unknown acceptance"
                        .to_owned(),
                }),
            },
            () = stop.cancelled() => return Err(ForgeError::Reconciliation {
                detail: "Iceberg snapshot expiry commit was cancelled with unknown acceptance"
                    .to_owned(),
            }),
        };
        #[cfg(feature = "test-support")]
        if self
            .core
            .maintenance_controls
            .pause_expiry_accepted(stop)
            .await
        {
            return Err(ForgeError::Reconciliation {
                detail: "Iceberg snapshot expiry commit was accepted before cancellation"
                    .to_owned(),
            });
        }
        let expired_files = derive_expired_files(
            &table,
            &committed,
            cleanup_traversal_items(&self.core.config)?,
            self.core.config.max_maintenance_bytes_per_tick,
            self.core.config.max_gc_candidates_per_batch,
        )
        .await?;
        lease.require_fence(&self.core.operator_pool).await?;
        Ok(expired_files)
    }
}

/// Derives a complete bounded cleanup set after a successful expiry commit.
///
/// # Errors
///
/// Returns catalog failures or fails closed when traversal exhausts a bound.
async fn derive_expired_files(
    before: &iceberg::table::Table,
    after: &iceberg::table::Table,
    max_items: usize,
    max_bytes: u64,
    max_candidates: usize,
) -> Result<ExpiredFileSet, ForgeError> {
    let files = expired_files_between(
        before.file_io(),
        before.metadata(),
        after.metadata(),
        CleanupTraversalLimits {
            max_items,
            max_bytes,
        },
    )
    .await
    .map_err(ForgeError::Catalog)?;
    validate_expired_files(&files, max_candidates)?;
    Ok(files)
}

/// Validates that a completed traversal remains within the deletion cap.
///
/// # Errors
///
/// Returns snapshot-expiry failure when traversal was incomplete or its exact
/// candidate set exceeds the configured deletion bound.
fn validate_expired_files(files: &ExpiredFileSet, max_candidates: usize) -> Result<(), ForgeError> {
    if files.limit_exhausted {
        return Err(ForgeError::SnapshotExpiry {
            detail: "expired-file traversal exceeded its configured bound".to_owned(),
        });
    }
    let candidate_count = files.data_files.len()
        + files.delete_files.len()
        + files.manifests.len()
        + files.manifest_lists.len()
        + files.statistics.len()
        + files.metadata_logs.len();
    if candidate_count > max_candidates {
        return Err(ForgeError::SnapshotExpiry {
            detail: "expired-file candidate set exceeded its configured bound".to_owned(),
        });
    }
    Ok(())
}

/// Derives a bounded metadata traversal ceiling from retained-history limits.
///
/// # Errors
///
/// Returns invalid configuration when the multiplication overflows.
fn cleanup_traversal_items(config: &super::compact::ForgeConfig) -> Result<usize, ForgeError> {
    config
        .max_gc_candidates_per_batch
        .checked_mul(config.max_retained_snapshots_per_table)
        .ok_or_else(|| ForgeError::InvalidConfig {
            detail: "expired-file traversal item bound overflowed".to_owned(),
        })
}

/// Reconstructs exact cleanup evidence for an expiry already visible in metadata.
///
/// # Errors
///
/// Returns catalog, traversal-bound, candidate-bound, or configuration failures.
pub(super) async fn derive_recovered_files(
    table: &iceberg::table::Table,
    base_metadata_location: &str,
    config: &super::compact::ForgeConfig,
) -> Result<ExpiredFileSet, ForgeError> {
    let before = iceberg::spec::TableMetadata::read_from(table.file_io(), base_metadata_location)
        .await
        .map_err(ForgeError::Catalog)?;
    let files = expired_files_between(
        table.file_io(),
        &before,
        table.metadata(),
        CleanupTraversalLimits {
            max_items: cleanup_traversal_items(config)?,
            max_bytes: config.max_maintenance_bytes_per_tick,
        },
    )
    .await
    .map_err(ForgeError::Catalog)?;
    validate_expired_files(&files, config.max_gc_candidates_per_batch)?;
    Ok(files)
}

/// Check that every selected snapshot is still eligible under current metadata.
fn selected_ids_are_eligible(
    table: &iceberg::table::Table,
    selected: &[i64],
    cutoff_ms: i64,
    retain_last: usize,
) -> Result<bool, ForgeError> {
    let (summaries, ref_heads) = snapshot_summaries(table)?;
    let expected = select_expirable_snapshots(
        &summaries,
        table.metadata().current_snapshot_id(),
        &ref_heads,
        cutoff_ms,
        retain_last,
    )
    .into_iter()
    .collect::<HashSet<_>>();
    Ok(selected.iter().all(|id| expected.contains(id)))
}

/// Recover prepared expiry operations whose outcome is now knowable.
///
/// A selection already absent from Iceberg is recorded as recovered. A
/// selection still present is retried only after the uncertainty bound has
/// elapsed.
impl Forge {
    async fn reconcile_expiry(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        now: DateTime<Utc>,
        stop: &CancellationToken,
    ) -> Result<ExpiryReconciliationOutcome, ForgeError> {
        let resource = table_resource_for_key(key);
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = ForgeOperations::new(&resource, ForgeOperationFamily::SnapshotExpire)
            .map_err(ForgeError::Sql)?
            .list_open(&mut conn, self.core.config.max_open_operations_per_table)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        let mut outcome = ExpiryReconciliationOutcome::default();
        if page.overflowed {
            outcome.destructive_maintenance =
                super::live_reconcile::DestructiveMaintenance::Blocked;
            return Ok(outcome);
        }
        for row in page.operations {
            let detail = row.prepared_detail;
            let AuditDetail::ForgeSnapshotExpire {
                selected_snapshot_ids,
                cutoff_ms,
                base_metadata_location,
                ..
            } = &detail
            else {
                record_malformed_expiry(&mut outcome);
                continue;
            };
            require_running(stop)?;
            lease.require_fence(&self.core.operator_pool).await?;
            let table = self.load_table(&binding.table_ident()).await?;
            let all_absent = selected_snapshot_ids
                .iter()
                .all(|id| table.metadata().snapshot_by_id(*id).is_none());
            if all_absent {
                let recovered_files = derive_recovered_files(
                    &table,
                    base_metadata_location.as_str(),
                    &self.core.config,
                )
                .await?;
                merge_expired_files(&mut outcome.expired_files, recovered_files);
                outcome.terminals.push(PendingExpiryTerminal {
                    detail,
                    recovered: true,
                });
                outcome.recovered = outcome.recovered.saturating_add(1);
                continue;
            }
            if now
                .signed_duration_since(row.prepared_at)
                .to_std()
                .unwrap_or_default()
                < self.core.config.uncertainty_bound
            {
                outcome.pending = outcome.pending.saturating_add(1);
                continue;
            }
            if !selected_ids_are_eligible(
                &table,
                selected_snapshot_ids,
                *cutoff_ms,
                self.core.config.retain_last,
            )? {
                outcome.unresolved = outcome.unresolved.saturating_add(1);
                continue;
            }
            let expired = self
                .complete_expiry(lease, key, binding, &detail, stop)
                .await?;
            merge_expired_files(&mut outcome.expired_files, expired);
            outcome.terminals.push(PendingExpiryTerminal {
                detail,
                recovered: true,
            });
            outcome.recovered = outcome.recovered.saturating_add(1);
        }
        if outcome.pending > 0 || outcome.unresolved > 0 {
            outcome.destructive_maintenance =
                super::live_reconcile::DestructiveMaintenance::Blocked;
        }
        Ok(outcome)
    }
}

impl Forge {
    /// Closes committed expiry operations only after exact task evidence is durable.
    ///
    /// # Errors
    ///
    /// Returns lease, SQL, audit, or operation-state failures. A partial close
    /// remains idempotently replayable from the still-Prepared operation rows.
    pub(crate) async fn finalize_expiry_terminals(
        &self,
        lease: &mut ForgeLease,
        tenant: DataTenantId,
        terminals: Vec<PendingExpiryTerminal>,
    ) -> Result<(), ForgeError> {
        for pending in terminals {
            let phase = if pending.recovered {
                ForgeSnapshotExpirePhase::Recovered
            } else {
                ForgeSnapshotExpirePhase::Committed
            };
            let operation = if pending.recovered {
                "forge.snapshot_expire.recovered"
            } else {
                "forge.snapshot_expire.committed"
            };
            self.append_expiry_audit(
                lease,
                tenant,
                &terminal_expiry_detail(&pending.detail, phase),
                operation,
            )
            .await?;
        }
        Ok(())
    }
}

/// Merges typed expired-file sets while preserving category provenance.
fn merge_expired_files(target: &mut ExpiredFileSet, source: ExpiredFileSet) {
    target.data_files.extend(source.data_files);
    target.delete_files.extend(source.delete_files);
    target.manifests.extend(source.manifests);
    target.manifest_lists.extend(source.manifest_lists);
    target.statistics.extend(source.statistics);
    target.metadata_logs.extend(source.metadata_logs);
}

/// Mark a wrong-family open expiry row as unresolved and fail closed later.
fn record_malformed_expiry(outcome: &mut ExpiryReconciliationOutcome) {
    outcome.unresolved = outcome.unresolved.saturating_add(1);
}

impl Forge {
    /// Append a fenced snapshot-expiry audit and projection transition atomically.
    ///
    /// # Errors
    ///
    /// Returns a detail-validation, lease, SQL, operation-state, audit, fence,
    /// or commit error. The caller-owned transaction rolls back both durable
    /// rows.
    async fn append_expiry_audit(
        &self,
        lease: &mut ForgeLease,
        tenant: DataTenantId,
        detail: &AuditDetail,
        operation: &str,
    ) -> Result<(), ForgeError> {
        let resource = match detail {
            AuditDetail::ForgeSnapshotExpire { group, .. } => group.clone(),
            _ => {
                return Err(ForgeError::SnapshotExpiry {
                    detail: "snapshot-expiry audit detail has the wrong kind".to_owned(),
                });
            }
        };
        let event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: operation.to_owned(),
            resource,
            card_ref: None,
            principal_id: SYSTEM_PRINCIPAL,
            principal_kind: PrincipalKindTag::Service,
            auth_method: AuthMethod::Internal,
            permission: "bifrost:forge".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: operation.to_owned(),
            detail: Some(detail.clone()),
        };
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let operations =
            ForgeOperations::new(&event.resource, ForgeOperationFamily::SnapshotExpire)
                .map_err(ForgeError::Sql)?;
        let transition = if operation == "forge.snapshot_expire.prepared" {
            operations.append_prepared(&mut conn, &event).await
        } else {
            operations.append_terminal(&mut conn, &event).await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            ForgeOperationTransition::Applied { .. }
            | ForgeOperationTransition::AlreadyApplied { .. } => {}
        }
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }

    /// Drive the production expiry transition writer from DB integration tests.
    ///
    /// # Errors
    ///
    /// Returns the same validation, lease, SQL, transition, audit, fence, and
    /// commit errors as the production expiry and reconciliation paths.
    #[cfg(feature = "test-support")]
    pub async fn append_expiry_transition_for_test(
        &self,
        lease: &mut ForgeLease,
        tenant: DataTenantId,
        detail: &AuditDetail,
        operation: &str,
    ) -> Result<(), ForgeError> {
        self.append_expiry_audit(lease, tenant, detail, operation)
            .await
    }
}

/// Copy an expiry detail while replacing its lifecycle phase.
fn terminal_expiry_detail(detail: &AuditDetail, phase: ForgeSnapshotExpirePhase) -> AuditDetail {
    match detail {
        AuditDetail::ForgeSnapshotExpire {
            operation_id,
            group,
            base_metadata_location,
            current_snapshot_id,
            retained_ref_heads,
            cutoff_ms,
            selected_snapshot_ids,
            ..
        } => AuditDetail::ForgeSnapshotExpire {
            operation_id: *operation_id,
            phase,
            group: group.clone(),
            base_metadata_location: base_metadata_location.clone(),
            current_snapshot_id: *current_snapshot_id,
            retained_ref_heads: retained_ref_heads.clone(),
            cutoff_ms: *cutoff_ms,
            selected_snapshot_ids: selected_snapshot_ids.clone(),
        },
        _ => detail.clone(),
    }
}

/// Return the stable audit resource URI for a Forge table.
pub(crate) fn table_resource_for_key(key: &ForgeTableKey) -> String {
    format!(
        "bifrost://{}/{}/{}",
        key.tenant, key.table_ref.namespace, key.table_ref.name
    )
}

/// Derive a stable operation ID from the table, cutoff, and sorted snapshot IDs.
fn expiry_operation_id(key: &ForgeTableKey, cutoff_ms: i64, selected: &[i64]) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(table_resource_for_key(key));
    hasher.update(cutoff_ms.to_be_bytes());
    for id in selected {
        hasher.update(id.to_be_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrong-family expiry state contributes unresolved evidence that blocks GC.
    #[test]
    fn malformed_expiry_blocks_gc() {
        let mut outcome = ExpiryReconciliationOutcome::default();
        record_malformed_expiry(&mut outcome);
        if outcome.unresolved > 0 {
            outcome.destructive_maintenance =
                super::super::live_reconcile::DestructiveMaintenance::Blocked;
        }
        assert_eq!(outcome.unresolved, 1);
        assert_eq!(
            outcome.destructive_maintenance,
            super::super::live_reconcile::DestructiveMaintenance::Blocked
        );
    }

    #[test]
    fn snapshot_selection_never_includes_current_or_retained_head() {
        let snapshots = vec![
            SnapshotSummary {
                id: 10,
                parent_id: None,
                timestamp_ms: 1,
            },
            SnapshotSummary {
                id: 20,
                parent_id: Some(10),
                timestamp_ms: 2,
            },
            SnapshotSummary {
                id: 30,
                parent_id: Some(20),
                timestamp_ms: 3,
            },
        ];
        let selected = select_expirable_snapshots(&snapshots, Some(30), &[], 4, 2);
        assert_eq!(selected, vec![10]);
    }

    #[test]
    fn snapshot_selection_uses_timestamp_and_retain_last_not_numeric_id() {
        let snapshots = vec![
            SnapshotSummary {
                id: 100,
                parent_id: None,
                timestamp_ms: 1,
            },
            SnapshotSummary {
                id: 2,
                parent_id: Some(100),
                timestamp_ms: 2,
            },
            SnapshotSummary {
                id: 50,
                parent_id: Some(2),
                timestamp_ms: 3,
            },
        ];
        let selected = select_expirable_snapshots(&snapshots, Some(50), &[], 4, 2);
        assert_eq!(selected, vec![100]);
    }

    /// Active watermark ordering follows timestamps even when IDs move backward.
    #[test]
    fn watermark_cutoff_uses_timestamps_not_snapshot_ids() {
        let snapshots = vec![
            SnapshotSummary {
                id: 900,
                parent_id: None,
                timestamp_ms: 10,
            },
            SnapshotSummary {
                id: 2,
                parent_id: Some(900),
                timestamp_ms: 20,
            },
        ];
        let watermarks = vec![SnapshotWatermark {
            snapshot_id: 900,
            timestamp_ms: 10,
        }];
        let cutoff = validate_watermarks(&snapshots, Some(2), &[], &watermarks, 100, 2)
            .expect("valid non-monotonic watermark");
        assert_eq!(cutoff, 10);
    }

    /// A persisted timestamp mismatch fails closed before expiry selection.
    #[test]
    fn watermark_timestamp_mismatch_fails_closed() {
        let snapshots = vec![SnapshotSummary {
            id: 7,
            parent_id: None,
            timestamp_ms: 20,
        }];
        let error = validate_watermarks(
            &snapshots,
            Some(7),
            &[],
            &[SnapshotWatermark {
                snapshot_id: 7,
                timestamp_ms: 19,
            }],
            100,
            1,
        )
        .expect_err("mismatched watermark must fail");
        assert!(error.to_string().contains("does not match"));
    }

    /// Stable expiry identities bind an exact ordered selection and cutoff.
    #[test]
    fn maintenance_operation_id_is_stable_for_sorted_targets() {
        let key = ForgeTableKey {
            tenant: DataTenantId::new_v7(),
            table_ref: TableRef::new(BifrostNamespace::Traces, "spans"),
        };
        let left = expiry_operation_id(&key, 10, &[3, 8]);
        let right = expiry_operation_id(&key, 10, &[3, 8]);
        assert_eq!(left, right);
    }

    /// A timestamp-correct sibling cannot authorize expiry from another branch.
    #[test]
    fn detached_sibling_watermark_fails_closed() {
        let snapshots = vec![
            SnapshotSummary {
                id: 1,
                parent_id: None,
                timestamp_ms: 10,
            },
            SnapshotSummary {
                id: 2,
                parent_id: Some(1),
                timestamp_ms: 20,
            },
            SnapshotSummary {
                id: 3,
                parent_id: Some(1),
                timestamp_ms: 30,
            },
        ];
        let error = validate_watermarks(
            &snapshots,
            Some(2),
            &[],
            &[SnapshotWatermark {
                snapshot_id: 3,
                timestamp_ms: 30,
            }],
            100,
            3,
        )
        .expect_err("detached sibling must fail");
        assert!(error.to_string().contains("detached"));
    }

    /// An approved head with a missing parent fails before any watermark can authorize expiry.
    #[test]
    fn missing_parent_fails_closed() {
        let snapshots = vec![SnapshotSummary {
            id: 2,
            parent_id: Some(1),
            timestamp_ms: 20,
        }];
        let error = validate_watermarks(&snapshots, Some(2), &[], &[], 100, 2)
            .expect_err("missing parent must fail");
        assert!(error.to_string().contains("missing snapshot 1"));
    }

    /// Cyclic ancestry is rejected even when the watermark timestamp is exact.
    #[test]
    fn cycle_fails_closed() {
        let snapshots = vec![
            SnapshotSummary {
                id: 1,
                parent_id: Some(2),
                timestamp_ms: 10,
            },
            SnapshotSummary {
                id: 2,
                parent_id: Some(1),
                timestamp_ms: 20,
            },
        ];
        let error = validate_watermarks(
            &snapshots,
            Some(2),
            &[],
            &[SnapshotWatermark {
                snapshot_id: 1,
                timestamp_ms: 10,
            }],
            100,
            2,
        )
        .expect_err("cycle must fail");
        assert!(error.to_string().contains("cycle"));
    }

    /// Sentinel watermarks are reserved for tables without snapshots or references.
    #[test]
    fn sentinel_on_non_empty_table_fails_closed() {
        let snapshots = vec![SnapshotSummary {
            id: 1,
            parent_id: None,
            timestamp_ms: 10,
        }];
        let error = validate_watermarks(
            &snapshots,
            Some(1),
            &[],
            &[SnapshotWatermark {
                snapshot_id: 0,
                timestamp_ms: 0,
            }],
            100,
            1,
        )
        .expect_err("sentinel must fail for a non-empty table");
        assert!(error.to_string().contains("sentinel"));
    }

    /// Traversal stops at the configured retained-history ceiling.
    #[test]
    fn ancestry_traversal_overflow_fails_closed() {
        let snapshots = vec![
            SnapshotSummary {
                id: 1,
                parent_id: None,
                timestamp_ms: 10,
            },
            SnapshotSummary {
                id: 2,
                parent_id: Some(1),
                timestamp_ms: 20,
            },
        ];
        let error = validate_watermarks(&snapshots, Some(2), &[], &[], 100, 1)
            .expect_err("ancestry beyond the bound must fail");
        assert!(error.to_string().contains("traversal bound"));
    }
}

//! Iceberg snapshot expiry for the Forge maintenance loop.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
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
use super::lease::{ForgeLease, forge_lease_key};

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
}

impl Default for ExpiryReconciliationOutcome {
    /// Starts one expiry reconciliation in the allowed, empty state.
    fn default() -> Self {
        Self {
            recovered: 0,
            pending: 0,
            unresolved: 0,
            destructive_maintenance: super::live_reconcile::DestructiveMaintenance::Allowed,
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
    pub(super) async fn run_snapshot_expiry_for_table(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        now: DateTime<Utc>,
    ) -> Result<ExpiryReconciliationOutcome, ForgeError> {
        self.run_snapshot_expiry_for_table_inner(lease, key, binding, now)
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
        let outcome = self
            .run_snapshot_expiry_for_table(&mut lease, &key, binding, self.core.clock.now()?)
            .await;
        lease.release(&self.core.operator_pool).await?;
        outcome.map(|outcome| outcome.recovered)
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
    /// [`select_expirable_snapshots`]. Iceberg metadata is reloaded before the
    /// commit so a stale prepared selection cannot delete a newly protected head.
    async fn run_snapshot_expiry_for_table_inner(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        now: DateTime<Utc>,
    ) -> Result<ExpiryReconciliationOutcome, ForgeError> {
        let mut outcome = self.reconcile_expiry(lease, key, binding, now).await?;
        if outcome.destructive_maintenance == super::live_reconcile::DestructiveMaintenance::Blocked
        {
            return Ok(outcome);
        }
        let table = self.load_table(&binding.table_ident()).await?;
        let cutoff_ms = expiry_cutoff_ms(now, self.core.config.snapshot_retention)?;
        let (summaries, ref_heads) = snapshot_summaries(&table)?;
        let selected = select_expirable_snapshots(
            &summaries,
            table.metadata().current_snapshot_id(),
            &ref_heads,
            cutoff_ms,
            self.core.config.retain_last,
        );
        if selected.is_empty() {
            return Ok(outcome);
        }
        let detail = expiry_detail(&table, key, cutoff_ms, selected, ref_heads)?;
        self.append_expiry_audit(lease, key.tenant, &detail, "forge.snapshot_expire.prepared")
            .await?;
        self.complete_expiry(lease, key, binding, &detail, false)
            .await?;
        outcome.recovered = outcome.recovered.saturating_add(1);
        Ok(outcome)
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

/// Extract snapshot timestamps, parent links, and metadata reference heads.
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
                .filter_map(|value| {
                    value
                        .get("snapshot-id")
                        .or_else(|| value.get("snapshot_id"))
                        .and_then(serde_json::Value::as_i64)
                })
                .collect::<Vec<_>>()
        })
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

/// Commit an expiry selection after rechecking metadata and the lease fence.
///
/// `recovered` selects the terminal audit phase used when completing a
/// prepared operation left by an earlier Forge process.
impl Forge {
    async fn complete_expiry(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        detail: &AuditDetail,
        recovered: bool,
    ) -> Result<(), ForgeError> {
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
            *cutoff_ms,
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
            .expire_older_than_ms(*cutoff_ms)
            .retain_last(self.core.config.retain_last.max(1));
        let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        tokio::time::timeout(
            self.core.config.iceberg_total_retry_timeout,
            transaction.commit(self.core.catalog.as_ref()),
        )
        .await
        .map_err(|_| ForgeError::Timeout {
            operation: "Iceberg snapshot expiry commit",
        })?
        .map_err(ForgeError::Catalog)?;
        lease.require_fence(&self.core.operator_pool).await?;
        let terminal = terminal_expiry_detail(
            detail,
            if recovered {
                ForgeSnapshotExpirePhase::Recovered
            } else {
                ForgeSnapshotExpirePhase::Committed
            },
        );
        self.append_expiry_audit(
            lease,
            key.tenant,
            &terminal,
            if recovered {
                "forge.snapshot_expire.recovered"
            } else {
                "forge.snapshot_expire.committed"
            },
        )
        .await
    }
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
                ..
            } = &detail
            else {
                record_malformed_expiry(&mut outcome);
                continue;
            };
            let table = self.load_table(&binding.table_ident()).await?;
            let all_absent = selected_snapshot_ids
                .iter()
                .all(|id| table.metadata().snapshot_by_id(*id).is_none());
            if all_absent {
                lease.require_fence(&self.core.operator_pool).await?;
                self.append_expiry_audit(
                    lease,
                    key.tenant,
                    &terminal_expiry_detail(&detail, ForgeSnapshotExpirePhase::Recovered),
                    "forge.snapshot_expire.recovered",
                )
                .await?;
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
            self.complete_expiry(lease, key, binding, &detail, true)
                .await?;
            outcome.recovered = outcome.recovered.saturating_add(1);
        }
        if outcome.pending > 0 || outcome.unresolved > 0 {
            outcome.destructive_maintenance =
                super::live_reconcile::DestructiveMaintenance::Blocked;
        }
        Ok(outcome)
    }
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
}

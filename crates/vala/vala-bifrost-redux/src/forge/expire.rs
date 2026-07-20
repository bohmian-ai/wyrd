//! Iceberg snapshot expiry for the Forge maintenance loop.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use sqlx::Row;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeSnapshotExpirePhase,
    StoragePath,
};

use crate::catalog::{TableRef, TenantTableBinding};
use crate::namespaces::BifrostNamespace;

use super::compact::{ForgeContext, ForgeTableKey, load_table};
use super::error::ForgeError;
use super::lease::ForgeLease;

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

/// Discover tenant/table pairs represented in the server-owned file list.
///
/// Invalid rows are counted and skipped so one malformed table identity does
/// not prevent maintenance for the remaining tables.
pub(crate) async fn discover_tables(
    context: &ForgeContext,
) -> Result<(Vec<ForgeTableKey>, usize), ForgeError> {
    let rows = sqlx::query(
        r"SELECT DISTINCT data_tenant_id, namespace, table_name
             FROM vala.file_list
            ORDER BY data_tenant_id, namespace, table_name",
    )
    .fetch_all(context.operator_pool.pool())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    let mut failures = 0;
    let mut tables = Vec::new();
    for row in rows {
        let table: Result<ForgeTableKey, ForgeError> = (|| {
            let tenant_uuid: Uuid =
                row.try_get("data_tenant_id")
                    .map_err(|error| ForgeError::SnapshotExpiry {
                        detail: error.to_string(),
                    })?;
            let tenant = if tenant_uuid.is_nil() {
                DataTenantId::SYSTEM_OWNER
            } else {
                DataTenantId::try_from(tenant_uuid).map_err(|error| ForgeError::SnapshotExpiry {
                    detail: error.to_string(),
                })?
            };
            let namespace: String =
                row.try_get("namespace")
                    .map_err(|error| ForgeError::SnapshotExpiry {
                        detail: error.to_string(),
                    })?;
            let table_name: String =
                row.try_get("table_name")
                    .map_err(|error| ForgeError::SnapshotExpiry {
                        detail: error.to_string(),
                    })?;
            let namespace = BifrostNamespace::from_wire(&namespace).ok_or_else(|| {
                ForgeError::SnapshotExpiry {
                    detail: format!("unknown Bifrost namespace `{namespace}`"),
                }
            })?;
            Ok(ForgeTableKey {
                tenant,
                table_ref: TableRef::new(namespace, table_name),
            })
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
pub(crate) async fn run_snapshot_expiry_for_table(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
) -> Result<usize, ForgeError> {
    let mut recovered = reconcile_expiry(context, lease, key, binding).await?;
    let table = load_table(context, &binding.table_ident()).await?;
    let cutoff_ms = expiry_cutoff_ms(context.config.snapshot_retention)?;
    let (summaries, ref_heads) = snapshot_summaries(&table)?;
    let selected = select_expirable_snapshots(
        &summaries,
        table.metadata().current_snapshot_id(),
        &ref_heads,
        cutoff_ms,
        context.config.retain_last,
    );
    if selected.is_empty() {
        return Ok(recovered);
    }
    let detail = expiry_detail(&table, key, cutoff_ms, selected, ref_heads)?;
    append_expiry_audit(
        context,
        lease,
        key.tenant,
        &detail,
        "forge.snapshot_expire.prepared",
    )
    .await?;
    complete_expiry(context, lease, key, binding, &detail, false).await?;
    recovered += 1;
    Ok(recovered)
}

/// Convert a retention duration into the UTC millisecond cutoff used by Iceberg.
fn expiry_cutoff_ms(retention: Duration) -> Result<i64, ForgeError> {
    let retention_ms =
        i64::try_from(retention.as_millis()).map_err(|_| ForgeError::InvalidConfig {
            detail: "snapshot_retention exceeds an i64 millisecond timestamp".to_owned(),
        })?;
    Ok(Utc::now().timestamp_millis().saturating_sub(retention_ms))
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
async fn complete_expiry(
    context: &ForgeContext,
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
    if !lease.renew(&context.operator_pool).await? {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    if !lease.commit_window_fits(context.config.commit_window()) {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    let table = load_table(context, &binding.table_ident()).await?;
    if !selected_ids_are_eligible(
        &table,
        selected_snapshot_ids,
        *cutoff_ms,
        context.config.retain_last,
    )? {
        return Err(ForgeError::SnapshotExpiry {
            detail: "reloaded Iceberg metadata no longer matches the prepared selection".to_owned(),
        });
    }
    let tx = Transaction::new(&table);
    let action = tx
        .expire_snapshots()
        .expire_snapshot_ids(selected_snapshot_ids.iter().copied())
        .expire_older_than_ms(*cutoff_ms)
        .retain_last(context.config.retain_last.max(1));
    let transaction = ApplyTransactionAction::apply(action, tx).map_err(ForgeError::Catalog)?;
    lease.require_fence(&context.operator_pool).await?;
    if !lease.commit_window_fits(context.config.commit_window()) {
        return Err(ForgeError::FenceLost {
            lease_key: lease.lease_key.clone(),
        });
    }
    tokio::time::timeout(
        context.config.iceberg_total_retry_timeout,
        transaction.commit(context.catalog.as_ref()),
    )
    .await
    .map_err(|_| ForgeError::Timeout {
        operation: "Iceberg snapshot expiry commit",
    })?
    .map_err(ForgeError::Catalog)?;
    lease.require_fence(&context.operator_pool).await?;
    let terminal = terminal_expiry_detail(
        detail,
        if recovered {
            ForgeSnapshotExpirePhase::Recovered
        } else {
            ForgeSnapshotExpirePhase::Committed
        },
    );
    append_expiry_audit(
        context,
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
async fn reconcile_expiry(
    context: &ForgeContext,
    lease: &mut ForgeLease,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
) -> Result<usize, ForgeError> {
    let (prepared, terminal) = load_expiry_audits(context, key).await?;
    let mut recovered = 0;
    for (operation_id, (detail, created_at)) in prepared {
        if terminal.contains(&operation_id) {
            continue;
        }
        let AuditDetail::ForgeSnapshotExpire {
            selected_snapshot_ids,
            ..
        } = &detail
        else {
            continue;
        };
        let table = load_table(context, &binding.table_ident()).await?;
        let all_absent = selected_snapshot_ids
            .iter()
            .all(|id| table.metadata().snapshot_by_id(*id).is_none());
        if all_absent {
            lease.require_fence(&context.operator_pool).await?;
            append_expiry_audit(
                context,
                lease,
                key.tenant,
                &terminal_expiry_detail(&detail, ForgeSnapshotExpirePhase::Recovered),
                "forge.snapshot_expire.recovered",
            )
            .await?;
            recovered += 1;
            continue;
        }
        if Utc::now()
            .signed_duration_since(created_at)
            .to_std()
            .unwrap_or_default()
            < context.config.uncertainty_bound
        {
            continue;
        }
        complete_expiry(context, lease, key, binding, &detail, true).await?;
        recovered += 1;
    }
    Ok(recovered)
}

/// Read the latest prepared and terminal expiry audit state for one table.
async fn load_expiry_audits(
    context: &ForgeContext,
    key: &ForgeTableKey,
) -> Result<(HashMap<Uuid, (AuditDetail, DateTime<Utc>)>, HashSet<Uuid>), ForgeError> {
    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let mut after_seq = 0_i64;
    let mut prepared = HashMap::new();
    let mut terminal = HashSet::new();
    loop {
        let page = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
            &mut conn,
            &table_resource_for_key(key),
            after_seq,
            context.config.audit_page_size,
        )
        .await
        .map_err(ForgeError::Sql)?;
        let page_len = page.len();
        for row in page {
            after_seq = row.seq;
            let Some(detail) = row.detail else { continue };
            let detail = serde_json::from_str::<AuditDetail>(&detail).map_err(|error| {
                ForgeError::Reconciliation {
                    detail: error.to_string(),
                }
            })?;
            let AuditDetail::ForgeSnapshotExpire {
                operation_id,
                phase,
                group,
                selected_snapshot_ids,
                ..
            } = &detail
            else {
                continue;
            };
            if group != &table_resource_for_key(key)
                || selected_snapshot_ids.is_empty()
                || selected_snapshot_ids
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return Err(ForgeError::Reconciliation {
                    detail: "snapshot-expiry audit detail is not canonical".to_owned(),
                });
            }
            match phase {
                ForgeSnapshotExpirePhase::Prepared => {
                    prepared.insert(*operation_id, (detail, row.created_at));
                }
                ForgeSnapshotExpirePhase::Committed | ForgeSnapshotExpirePhase::Recovered => {
                    terminal.insert(*operation_id);
                }
            }
        }
        if page_len < usize::try_from(context.config.audit_page_size).unwrap_or(usize::MAX) {
            break;
        }
    }
    conn.commit().await.map_err(ForgeError::Sql)?;
    Ok((prepared, terminal))
}

/// Append a fenced snapshot-expiry audit event in the tenant transaction.
async fn append_expiry_audit(
    context: &ForgeContext,
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
    lease.require_fence(&context.operator_pool).await?;
    let mut conn = context
        .vala
        .tenant_conn(tenant)
        .await
        .map_err(ForgeError::Sql)?;
    vala_sql::queries::audit_outbox::append_audit(&mut conn, &event)
        .await
        .map_err(ForgeError::Sql)?;
    lease.assert_transaction_fence(&mut conn).await?;
    conn.commit().await.map_err(ForgeError::Sql)
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

//! Reference-aware orphan garbage collection for Forge objects.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Duration;

use iceberg::spec::TableMetadata;
use opendal::raw::Timestamp;
use opendal::{EntryMode, ErrorKind};
use sqlx::Row;
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeOrphanGcPhase,
    StoragePath,
};

use crate::catalog::TenantTableBinding;

use super::compact::{ForgeTableKey, load_table};
use super::error::ForgeError;
use super::expire::table_resource_for_key;
use super::lease::ForgeLease;
use super::{Forge, ForgeCore};

const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

/// The complete set of Iceberg and server-side paths that must not be deleted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectedLiveSet {
    paths: BTreeSet<String>,
}

impl ProtectedLiveSet {
    /// Add a path to the protected set.
    pub fn insert(&mut self, path: impl Into<String>) {
        self.paths.insert(path.into());
    }

    /// Check whether an object path is protected.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.paths.contains(path)
    }
}

/// Whether a known object is old enough and absent from the live set.
#[must_use]
pub fn is_gc_candidate(
    path: &str,
    live_set: &ProtectedLiveSet,
    last_modified: Option<Timestamp>,
    now: Timestamp,
    ttl: Duration,
) -> bool {
    known_iceberg_object(path)
        && !live_set.contains(path)
        && last_modified.is_some_and(|modified| modified < now - ttl)
}

impl Forge {
    /// Run reconciled orphan deletion for one fenced physical table.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, object-store, SQL, audit, or live-set failures.
    pub(super) async fn run_orphan_gc_for_table(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        live_set: &ProtectedLiveSet,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        run_orphan_gc_for_table_inner(&self.core, lease, key, binding, live_set).await
    }

    /// Build the complete retained snapshot and pending-staging live set.
    ///
    /// # Errors
    ///
    /// Returns catalog, SQL, path-validation, or live-set failures.
    pub(super) async fn build_live_set(
        &self,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        table: &iceberg::table::Table,
    ) -> Result<ProtectedLiveSet, ForgeError> {
        build_live_set_inner(&self.core, key, binding, table).await
    }
}

async fn run_orphan_gc_for_table_inner(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
    live_set: &ProtectedLiveSet,
) -> Result<OrphanGcOutcome, ForgeError> {
    let mut outcome = reconcile_gc(context, lease, key, binding).await?;
    let candidates = list_gc_candidates(context, binding, live_set).await?;
    if candidates.is_empty() {
        return Ok(outcome);
    }
    let detail = gc_detail(key, candidates)?;
    append_gc_audit(
        context,
        lease,
        key.tenant,
        &detail,
        "forge.orphan_gc.prepared",
    )
    .await?;
    let (deleted, skipped) = delete_gc_batch(context, lease, key, binding, &detail, false).await?;
    outcome.recovered += 1;
    outcome.deleted += deleted;
    outcome.skipped += skipped;
    Ok(outcome)
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OrphanGcOutcome {
    pub(crate) recovered: usize,
    pub(crate) deleted: usize,
    pub(crate) skipped: usize,
}

/// Build the live set from every retained Iceberg snapshot and every pending
/// server-side file-list row for this exact tenant/table predicate.
async fn build_live_set_inner(
    context: &ForgeCore,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
    table: &iceberg::table::Table,
) -> Result<ProtectedLiveSet, ForgeError> {
    assert_table_location(table.metadata(), binding)?;
    let mut live = ProtectedLiveSet::default();
    let prefix = &binding.object_prefix;
    add_path(
        &mut live,
        prefix,
        table
            .metadata_location_result()
            .map_err(ForgeError::Catalog)?,
    )?;
    for metadata_log in table.metadata().metadata_log() {
        add_path(&mut live, prefix, &metadata_log.metadata_file)?;
    }
    for snapshot in table.metadata().snapshots() {
        add_path(&mut live, prefix, snapshot.manifest_list())?;
        let manifest_list = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        for manifest_file in manifest_list.entries() {
            add_path(&mut live, prefix, &manifest_file.manifest_path)?;
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            for entry in manifest.entries() {
                add_path(&mut live, prefix, entry.data_file().file_path())?;
            }
        }
        if let Some(statistics) = table
            .metadata()
            .statistics_for_snapshot(snapshot.snapshot_id())
        {
            add_path(&mut live, prefix, &statistics.statistics_path)?;
        }
        if let Some(statistics) = table
            .metadata()
            .partition_statistics_for_snapshot(snapshot.snapshot_id())
        {
            add_path(&mut live, prefix, &statistics.statistics_path)?;
        }
    }

    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let rows = sqlx::query(
        r"SELECT file_path
             FROM vala.file_list
            WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3
            ORDER BY file_path",
    )
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    conn.commit().await.map_err(ForgeError::Sql)?;
    for row in rows {
        let path: String = row
            .try_get("file_path")
            .map_err(|error| ForgeError::LiveSet {
                detail: error.to_string(),
            })?;
        add_path(&mut live, prefix, &path)?;
    }
    Ok(live)
}

async fn list_gc_candidates(
    context: &ForgeCore,
    binding: &TenantTableBinding,
    live_set: &ProtectedLiveSet,
) -> Result<Vec<String>, ForgeError> {
    let prefix = format!("{}/", binding.object_prefix.trim_end_matches('/'));
    let entries = context
        .object_store
        .list(&prefix)
        .await
        .map_err(ForgeError::ObjectList)?;
    let now = Timestamp::now();
    let mut candidates = entries
        .into_iter()
        .filter(|entry| entry.metadata().mode() == EntryMode::FILE)
        .filter_map(|entry| {
            let path = entry.path().to_owned();
            let modified = entry.metadata().last_modified();
            is_gc_candidate(&path, live_set, modified, now, context.config.orphan_gc_ttl)
                .then_some(path)
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.truncate(context.config.max_gc_candidates_per_batch);
    Ok(candidates)
}

async fn delete_gc_batch(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
    detail: &AuditDetail,
    recovered: bool,
) -> Result<(usize, usize), ForgeError> {
    let AuditDetail::ForgeOrphanGc {
        candidate_paths,
        group,
        ..
    } = detail
    else {
        return Err(ForgeError::Reconciliation {
            detail: "orphan-GC audit detail has the wrong kind".to_owned(),
        });
    };
    if group != &table_resource_for_key(key)
        || candidate_paths.is_empty()
        || candidate_paths
            .windows(2)
            .any(|pair| pair[0].as_str() >= pair[1].as_str())
    {
        return Err(ForgeError::Reconciliation {
            detail: "orphan-GC audit detail is not canonical".to_owned(),
        });
    }
    let mut deleted = Vec::new();
    let mut skipped = Vec::new();
    for path in candidate_paths {
        lease.require_fence(&context.operator_pool).await?;
        let table = load_table(context, &binding.table_ident()).await?;
        let now = Timestamp::now();
        let normalized =
            normalize_path(&binding.object_prefix, path.as_str()).ok_or_else(|| {
                ForgeError::LiveSet {
                    detail: format!("GC candidate escaped table prefix: {path}"),
                }
            })?;
        let metadata = match context.object_store.stat(&normalized).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                deleted.push(path.as_str().to_owned());
                continue;
            }
            Err(error) => return Err(ForgeError::ObjectDelete(error)),
        };
        if metadata.mode() != EntryMode::FILE {
            skipped.push(path.as_str().to_owned());
            continue;
        }
        if path_is_referenced(context, key, binding, &table, &normalized).await?
            || !is_gc_candidate(
                &normalized,
                &ProtectedLiveSet::default(),
                metadata.last_modified(),
                now,
                context.config.orphan_gc_ttl,
            )
        {
            skipped.push(path.as_str().to_owned());
            continue;
        }
        lease.require_fence(&context.operator_pool).await?;
        match context.object_store.delete(&normalized).await {
            Ok(()) => deleted.push(path.as_str().to_owned()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                deleted.push(path.as_str().to_owned());
            }
            Err(error) => return Err(ForgeError::ObjectDelete(error)),
        }
    }
    lease.require_fence(&context.operator_pool).await?;
    let deleted_count = deleted.len();
    let skipped_count = skipped.len();
    let terminal = terminal_gc_detail(
        detail,
        deleted,
        skipped,
        if recovered {
            ForgeOrphanGcPhase::Recovered
        } else {
            ForgeOrphanGcPhase::Committed
        },
    );
    append_gc_audit(
        context,
        lease,
        key.tenant,
        &terminal,
        if recovered {
            "forge.orphan_gc.recovered"
        } else {
            "forge.orphan_gc.committed"
        },
    )
    .await?;
    Ok((deleted_count, skipped_count))
}

/// Recheck one candidate against the latest Iceberg metadata and SQL rows.
///
/// The scheduler builds one complete retained live set per table tick. This
/// narrower lookup is the required final race check immediately before an
/// object delete; it avoids rebuilding the complete set for every candidate
/// while still protecting a reference that appeared after the initial list.
async fn path_is_referenced(
    context: &ForgeCore,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
    table: &iceberg::table::Table,
    path: &str,
) -> Result<bool, ForgeError> {
    let path = normalize_path(&binding.object_prefix, path).ok_or_else(|| ForgeError::LiveSet {
        detail: format!("GC candidate escaped table prefix: {path}"),
    })?;
    if table
        .metadata_location_result()
        .map_err(ForgeError::Catalog)?
        == path
        || table
            .metadata()
            .metadata_log()
            .iter()
            .any(|entry| entry.metadata_file == path)
    {
        return Ok(true);
    }
    for snapshot in table.metadata().snapshots() {
        if snapshot.manifest_list() == path {
            return Ok(true);
        }
        let manifest_list = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        for manifest_file in manifest_list.entries() {
            if manifest_file.manifest_path == path {
                return Ok(true);
            }
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            if manifest
                .entries()
                .iter()
                .any(|entry| entry.data_file().file_path() == path)
            {
                return Ok(true);
            }
        }
        if table
            .metadata()
            .statistics_for_snapshot(snapshot.snapshot_id())
            .is_some_and(|statistics| statistics.statistics_path == path)
            || table
                .metadata()
                .partition_statistics_for_snapshot(snapshot.snapshot_id())
                .is_some_and(|statistics| statistics.statistics_path == path)
        {
            return Ok(true);
        }
    }

    let mut conn = context
        .vala
        .tenant_conn(key.tenant)
        .await
        .map_err(ForgeError::Sql)?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND file_path = $4",
    )
    .bind(key.tenant.as_uuid())
    .bind(key.table_ref.namespace.as_str())
    .bind(&key.table_ref.name)
    .bind(&path)
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(|error| ForgeError::Sql(error.into()))?;
    conn.commit().await.map_err(ForgeError::Sql)?;
    Ok(count > 0)
}

async fn reconcile_gc(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    key: &ForgeTableKey,
    binding: &TenantTableBinding,
) -> Result<OrphanGcOutcome, ForgeError> {
    let (prepared, terminal) = load_gc_audits(context, key).await?;
    let mut outcome = OrphanGcOutcome::default();
    for (operation_id, (detail, _created_at)) in prepared {
        if terminal.contains(&operation_id) {
            continue;
        }
        let (deleted, skipped) =
            delete_gc_batch(context, lease, key, binding, &detail, true).await?;
        outcome.recovered += 1;
        outcome.deleted += deleted;
        outcome.skipped += skipped;
    }
    Ok(outcome)
}

async fn load_gc_audits(
    context: &ForgeCore,
    key: &ForgeTableKey,
) -> Result<
    (
        HashMap<Uuid, (AuditDetail, chrono::DateTime<chrono::Utc>)>,
        HashSet<Uuid>,
    ),
    ForgeError,
> {
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
            let AuditDetail::ForgeOrphanGc {
                operation_id,
                phase,
                group,
                candidate_paths,
                ..
            } = &detail
            else {
                continue;
            };
            if group != &table_resource_for_key(key)
                || candidate_paths.is_empty()
                || candidate_paths
                    .windows(2)
                    .any(|pair| pair[0].as_str() >= pair[1].as_str())
            {
                return Err(ForgeError::Reconciliation {
                    detail: "orphan-GC audit detail is not canonical".to_owned(),
                });
            }
            match phase {
                ForgeOrphanGcPhase::Prepared => {
                    prepared.insert(*operation_id, (detail, row.created_at));
                }
                ForgeOrphanGcPhase::Committed | ForgeOrphanGcPhase::Recovered => {
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

fn gc_detail(key: &ForgeTableKey, mut candidates: Vec<String>) -> Result<AuditDetail, ForgeError> {
    candidates.sort_unstable();
    let candidate_paths = candidates
        .iter()
        .map(|path| StoragePath::new(path.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ForgeError::LiveSet {
            detail: error.to_string(),
        })?;
    Ok(AuditDetail::ForgeOrphanGc {
        operation_id: gc_operation_id(key, &candidates),
        phase: ForgeOrphanGcPhase::Prepared,
        group: table_resource_for_key(key),
        candidate_paths,
        deleted_paths: Vec::new(),
        skipped_paths: Vec::new(),
    })
}

fn terminal_gc_detail(
    detail: &AuditDetail,
    deleted: Vec<String>,
    skipped: Vec<String>,
    phase: ForgeOrphanGcPhase,
) -> AuditDetail {
    match detail {
        AuditDetail::ForgeOrphanGc {
            operation_id,
            group,
            candidate_paths,
            ..
        } => AuditDetail::ForgeOrphanGc {
            operation_id: *operation_id,
            phase,
            group: group.clone(),
            candidate_paths: candidate_paths.clone(),
            deleted_paths: paths_to_storage(deleted),
            skipped_paths: paths_to_storage(skipped),
        },
        _ => detail.clone(),
    }
}

fn paths_to_storage(paths: Vec<String>) -> Vec<StoragePath> {
    paths
        .into_iter()
        .filter_map(|path| StoragePath::new(path).ok())
        .collect()
}

async fn append_gc_audit(
    context: &ForgeCore,
    lease: &mut ForgeLease,
    tenant: DataTenantId,
    detail: &AuditDetail,
    operation: &str,
) -> Result<(), ForgeError> {
    let resource = match detail {
        AuditDetail::ForgeOrphanGc { group, .. } => group.clone(),
        _ => {
            return Err(ForgeError::Reconciliation {
                detail: "orphan-GC audit detail has the wrong kind".to_owned(),
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

fn add_path(live: &mut ProtectedLiveSet, prefix: &str, path: &str) -> Result<(), ForgeError> {
    let normalized = normalize_path(prefix, path).ok_or_else(|| ForgeError::LiveSet {
        detail: format!("Iceberg reference escaped table prefix: {path}"),
    })?;
    live.insert(normalized);
    Ok(())
}

fn normalize_path(prefix: &str, path: &str) -> Option<String> {
    let prefix = prefix.trim_end_matches('/');
    let prefixed = format!("{prefix}/");
    if path == prefix || path.starts_with(&prefixed) {
        if path.contains('\\')
            || path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return None;
        }
        return Some(path.to_owned());
    }
    path.match_indices(prefix).find_map(|(index, _)| {
        if index > 0 && !path[..index].ends_with('/') {
            return None;
        }
        let candidate = &path[index..];
        if candidate.contains('\\')
            || candidate
                .split('/')
                .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return None;
        }
        (candidate == prefix || candidate.starts_with(&prefixed)).then_some(candidate.to_owned())
    })
}

fn assert_table_location(
    metadata: &TableMetadata,
    binding: &TenantTableBinding,
) -> Result<(), ForgeError> {
    if normalize_path(&binding.object_prefix, metadata.location()).is_none() {
        return Err(ForgeError::LiveSet {
            detail: format!(
                "Iceberg table location {} escaped {}",
                metadata.location(),
                binding.object_prefix
            ),
        });
    }
    Ok(())
}

fn known_iceberg_object(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str);
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("parquet")) {
        return true;
    }
    if !lower.contains("/metadata/") {
        return false;
    }
    extension.is_some_and(|extension| {
        extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("avro")
    })
}

fn gc_operation_id(key: &ForgeTableKey, candidates: &[String]) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(table_resource_for_key(key));
    for candidate in candidates {
        hasher.update(candidate.as_bytes());
        hasher.update([0]);
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
    fn gc_live_set_includes_all_retained_iceberg_and_pending_file_list_paths() {
        let mut live = ProtectedLiveSet::default();
        live.insert("tenants/t/traces/spans/data/live.parquet");
        live.insert("tenants/t/traces/spans/metadata/v1.metadata.json");
        live.insert("tenants/t/traces/spans/metadata/snap-m0.avro");
        live.insert("tenants/t/traces/spans/metadata/snap-m1.avro");
        assert_eq!(live.paths.len(), 4);
        assert!(live.contains("tenants/t/traces/spans/metadata/snap-m1.avro"));
    }

    #[test]
    fn gc_candidate_requires_absent_live_reference_and_24h_age() {
        let now = Timestamp::from_millisecond(48 * 60 * 60 * 1_000).expect("timestamp");
        let old = Timestamp::from_millisecond(0).expect("timestamp");
        let young = Timestamp::from_millisecond(47 * 60 * 60 * 1_000).expect("timestamp");
        let mut live = ProtectedLiveSet::default();
        live.insert("tenants/t/traces/spans/data/live.parquet");
        let ttl = Duration::from_hours(24);
        assert!(!is_gc_candidate(
            "tenants/t/traces/spans/data/live.parquet",
            &live,
            Some(old),
            now,
            ttl
        ));
        assert!(!is_gc_candidate(
            "tenants/t/traces/spans/data/young.parquet",
            &live,
            Some(young),
            now,
            ttl
        ));
        assert!(is_gc_candidate(
            "tenants/t/traces/spans/data/old.parquet",
            &live,
            Some(old),
            now,
            ttl
        ));
    }
}

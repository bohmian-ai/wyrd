//! Reference-aware orphan garbage collection for Forge objects.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Duration;

use iceberg::spec::TableMetadata;
use opendal::raw::Timestamp;
use opendal::{EntryMode, ErrorKind};
use sqlx::Row;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeOrphanGcPhase,
    StoragePath,
};

use crate::catalog::TenantTableBinding;

use super::Forge;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::expire::table_resource_for_key;
use super::lease::ForgeLease;
use super::path::{catalog_path_to_object_key, validate_table_location};

const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

/// The complete set of Iceberg and server-side paths that must not be deleted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectedLiveSet {
    paths: BTreeSet<String>,
}

impl ProtectedLiveSet {
    /// Insert one object key already validated against its table binding.
    pub(crate) fn insert_validated(&mut self, object_key: String) {
        self.extend_validated([object_key]);
    }

    /// Extend the set with object keys already validated against their table binding.
    pub(crate) fn extend_validated(&mut self, object_keys: impl IntoIterator<Item = String>) {
        self.paths.extend(object_keys);
    }

    /// Add a path to the protected set.
    pub fn insert(&mut self, path: impl Into<String>) {
        self.insert_validated(path.into());
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
    Forge::known_iceberg_object(path)
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
        self.run_orphan_gc_for_table_inner(lease, key, binding, live_set)
            .await
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
        self.build_live_set_inner(key, binding, table).await
    }
}

impl Forge {
    async fn run_orphan_gc_for_table_inner(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        live_set: &ProtectedLiveSet,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        let mut outcome = self.reconcile_gc(lease, key, binding).await?;
        let candidates = self.list_gc_candidates(binding, live_set).await?;
        if candidates.is_empty() {
            return Ok(outcome);
        }
        let detail = Self::gc_detail(key, candidates)?;
        self.append_gc_audit(lease, key.tenant, &detail, "forge.orphan_gc.prepared")
            .await?;
        let (deleted, skipped) = self
            .delete_gc_batch(lease, key, binding, &detail, false)
            .await?;
        outcome.recovered += 1;
        outcome.deleted += deleted;
        outcome.skipped += skipped;
        Ok(outcome)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OrphanGcOutcome {
    pub(crate) recovered: usize,
    pub(crate) deleted: usize,
    pub(crate) skipped: usize,
}

/// Build the live set from every retained Iceberg snapshot and every pending
/// server-side file-list row for this exact tenant/table predicate.
impl Forge {
    async fn build_live_set_inner(
        &self,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        table: &iceberg::table::Table,
    ) -> Result<ProtectedLiveSet, ForgeError> {
        self.assert_table_location(table.metadata(), binding)?;
        let mut live = ProtectedLiveSet::default();
        let table_location = table.metadata().location();
        let mut add = |path: &str| self.add_path(&mut live, binding, table_location, path);
        add(table
            .metadata_location_result()
            .map_err(ForgeError::Catalog)?)?;
        for metadata_log in table.metadata().metadata_log() {
            add(&metadata_log.metadata_file)?;
        }
        for snapshot in table.metadata().snapshots() {
            add(snapshot.manifest_list())?;
            let manifest_list = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .map_err(ForgeError::Catalog)?;
            for manifest_file in manifest_list.entries() {
                add(&manifest_file.manifest_path)?;
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .map_err(ForgeError::Catalog)?;
                for entry in manifest.entries() {
                    add(entry.data_file().file_path())?;
                }
            }
            if let Some(statistics) = table
                .metadata()
                .statistics_for_snapshot(snapshot.snapshot_id())
            {
                add(&statistics.statistics_path)?;
            }
            if let Some(statistics) = table
                .metadata()
                .partition_statistics_for_snapshot(snapshot.snapshot_id())
            {
                add(&statistics.statistics_path)?;
            }
        }

        let mut conn = self
            .core
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
            add(&path)?;
        }
        Ok(live)
    }
}

impl Forge {
    /// Lists aged table-owned objects absent from the caller's protected live set.
    ///
    /// # Errors
    /// Returns object-store listing failures. Cancellation leaves objects untouched.
    async fn list_gc_candidates(
        &self,
        binding: &TenantTableBinding,
        live_set: &ProtectedLiveSet,
    ) -> Result<Vec<String>, ForgeError> {
        let prefix = format!("{}/", binding.object_prefix.trim_end_matches('/'));
        let entries = self
            .core
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
                is_gc_candidate(
                    &path,
                    live_set,
                    modified,
                    now,
                    self.core.config.orphan_gc_ttl,
                )
                .then_some(path)
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        candidates.truncate(self.core.config.max_gc_candidates_per_batch);
        Ok(candidates)
    }

    /// Deletes a prepared candidate batch with a final fence and reference check.
    ///
    /// # Errors
    /// Returns lease, catalog, object-store, SQL, or audit failures.
    async fn delete_gc_batch(
        &self,
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
            lease.require_fence(&self.core.operator_pool).await?;
            let table = self.load_table(&binding.table_ident()).await?;
            let now = Timestamp::now();
            let normalized = catalog_path_to_object_key(
                table.metadata().location(),
                binding,
                &self.core.staging,
                path.as_str(),
            )?;
            let metadata = match self.core.object_store.stat(&normalized).await {
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
            if self
                .path_is_referenced(key, binding, &table, &normalized)
                .await?
                || !is_gc_candidate(
                    &normalized,
                    &ProtectedLiveSet::default(),
                    metadata.last_modified(),
                    now,
                    self.core.config.orphan_gc_ttl,
                )
            {
                skipped.push(path.as_str().to_owned());
                continue;
            }
            lease.require_fence(&self.core.operator_pool).await?;
            match self.core.object_store.delete(&normalized).await {
                Ok(()) => deleted.push(path.as_str().to_owned()),
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    deleted.push(path.as_str().to_owned());
                }
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            }
        }
        lease.require_fence(&self.core.operator_pool).await?;
        let deleted_count = deleted.len();
        let skipped_count = skipped.len();
        let terminal = Self::terminal_gc_detail(
            detail,
            deleted,
            skipped,
            if recovered {
                ForgeOrphanGcPhase::Recovered
            } else {
                ForgeOrphanGcPhase::Committed
            },
        );
        self.append_gc_audit(
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
    /// Rechecks Iceberg and SQL references immediately before one deletion.
    ///
    /// # Errors
    /// Returns catalog or SQL lookup failures; no deletion occurs in this method.
    async fn path_is_referenced(
        &self,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        table: &iceberg::table::Table,
        path: &str,
    ) -> Result<bool, ForgeError> {
        let path = catalog_path_to_object_key(
            table.metadata().location(),
            binding,
            &self.core.staging,
            path,
        )?;
        let table_location = table.metadata().location();
        let matches = |reference: &str| {
            catalog_path_to_object_key(table_location, binding, &self.core.staging, reference)
                .map(|key| key == path)
        };
        if matches(
            table
                .metadata_location_result()
                .map_err(ForgeError::Catalog)?,
        )? {
            return Ok(true);
        }
        for entry in table.metadata().metadata_log() {
            if matches(&entry.metadata_file)? {
                return Ok(true);
            }
        }
        for snapshot in table.metadata().snapshots() {
            if matches(snapshot.manifest_list())? {
                return Ok(true);
            }
            let manifest_list = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .map_err(ForgeError::Catalog)?;
            for manifest_file in manifest_list.entries() {
                if matches(&manifest_file.manifest_path)? {
                    return Ok(true);
                }
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .map_err(ForgeError::Catalog)?;
                for entry in manifest.entries() {
                    if matches(entry.data_file().file_path())? {
                        return Ok(true);
                    }
                }
            }
            if table
                .metadata()
                .statistics_for_snapshot(snapshot.snapshot_id())
                .map(|statistics| matches(&statistics.statistics_path))
                .transpose()?
                .unwrap_or(false)
                || table
                    .metadata()
                    .partition_statistics_for_snapshot(snapshot.snapshot_id())
                    .map(|statistics| matches(&statistics.statistics_path))
                    .transpose()?
                    .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        let mut conn = self
            .core
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

    /// Replays prepared orphan-GC audits that lack a terminal event.
    ///
    /// # Errors
    /// Returns audit, lease, object-store, catalog, or SQL failures.
    async fn reconcile_gc(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        let (prepared, terminal) = self.load_gc_audits(key).await?;
        let mut outcome = OrphanGcOutcome::default();
        for (operation_id, (detail, _created_at)) in prepared {
            if terminal.contains(&operation_id) {
                continue;
            }
            let (deleted, skipped) = self
                .delete_gc_batch(lease, key, binding, &detail, true)
                .await?;
            outcome.recovered += 1;
            outcome.deleted += deleted;
            outcome.skipped += skipped;
        }
        Ok(outcome)
    }

    /// Loads prepared and terminal orphan-GC audit records for one table.
    ///
    /// # Errors
    /// Returns SQL or audit decoding failures. Cancellation leaves audit state unchanged.
    async fn load_gc_audits(
        &self,
        key: &ForgeTableKey,
    ) -> Result<
        (
            HashMap<Uuid, (AuditDetail, chrono::DateTime<chrono::Utc>)>,
            HashSet<Uuid>,
        ),
        ForgeError,
    > {
        let mut conn = self
            .core
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
                self.core.config.audit_page_size,
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
            if page_len < usize::try_from(self.core.config.audit_page_size).unwrap_or(usize::MAX) {
                break;
            }
        }
        conn.commit().await.map_err(ForgeError::Sql)?;
        Ok((prepared, terminal))
    }

    fn gc_detail(
        key: &ForgeTableKey,
        mut candidates: Vec<String>,
    ) -> Result<AuditDetail, ForgeError> {
        candidates.sort_unstable();
        let candidate_paths = candidates
            .iter()
            .map(|path| StoragePath::new(path.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ForgeError::LiveSet {
                detail: error.to_string(),
            })?;
        Ok(AuditDetail::ForgeOrphanGc {
            operation_id: Self::gc_operation_id(key, &candidates),
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
                deleted_paths: Self::paths_to_storage(deleted),
                skipped_paths: Self::paths_to_storage(skipped),
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

    /// Appends one fenced orphan-GC audit and projection transition atomically.
    ///
    /// # Errors
    /// Returns detail-validation, lease, SQL, operation-state, audit, fence, or
    /// commit failures. The caller-owned transaction rolls back both durable
    /// rows.
    async fn append_gc_audit(
        &self,
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
        lease.require_fence(&self.core.operator_pool).await?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let operations = ForgeOperations::new(&event.resource, ForgeOperationFamily::OrphanGc)
            .map_err(ForgeError::Sql)?;
        let transition = if operation == "forge.orphan_gc.prepared" {
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

    /// Drive the production orphan-GC transition writer from DB integration tests.
    ///
    /// # Errors
    ///
    /// Returns the same validation, lease, SQL, transition, audit, fence, and
    /// commit errors as the production GC and reconciliation paths.
    #[cfg(feature = "test-support")]
    pub async fn append_gc_transition_for_test(
        &self,
        lease: &mut ForgeLease,
        tenant: DataTenantId,
        detail: &AuditDetail,
        operation: &str,
    ) -> Result<(), ForgeError> {
        self.append_gc_audit(lease, tenant, detail, operation).await
    }

    /// Insert one catalog-owned reference after converting it through Forge's
    /// shared store and binding contract.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the location or path belongs to
    /// another table or object store.
    fn add_path(
        &self,
        live: &mut ProtectedLiveSet,
        binding: &TenantTableBinding,
        table_location: &str,
        path: &str,
    ) -> Result<(), ForgeError> {
        let normalized =
            catalog_path_to_object_key(table_location, binding, &self.core.staging, path)?;
        live.insert(normalized);
        Ok(())
    }

    /// Confirm this table's metadata location belongs to the configured store and binding.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the catalog location escapes the
    /// binding or does not match the configured object-store endpoint.
    pub(super) fn assert_table_location(
        &self,
        metadata: &TableMetadata,
        binding: &TenantTableBinding,
    ) -> Result<(), ForgeError> {
        validate_table_location(binding, metadata.location(), &self.core.staging)
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

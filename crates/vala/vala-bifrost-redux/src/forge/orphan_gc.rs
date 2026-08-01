//! Reference-aware orphan garbage collection for Forge objects.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use iceberg::spec::{DataContentType, ManifestContentType, TableMetadata};
use opendal::raw::Timestamp;
use opendal::{EntryMode, ErrorKind};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::TenantConn;
use vala_sql::queries::file_list::list_nonterminal_file_paths;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{
    ForgeOperationFamily, ForgeOperationPhase, ForgeOperationStateRow, ForgeOperationTransition,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeOrphanGcPhase,
    StoragePath, audit_detail_canonical_json,
};

use crate::catalog::TenantTableBinding;

use super::Forge;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::expire::table_resource_for_key;
use super::lease::{ForgeLease, forge_lease_key};
use super::live_reconcile::DestructiveMaintenance;
use super::path::{catalog_path_to_object_key, validate_table_location};

const SYSTEM_PRINCIPAL: PrincipalId = PrincipalId::new(uuid::Uuid::nil());

/// Fails closed when the table-scoped maintenance token has been cancelled.
///
/// # Errors
///
/// Returns [`ForgeError::Shutdown`] after cancellation.
fn require_running(stop: &CancellationToken) -> Result<(), ForgeError> {
    if stop.is_cancelled() {
        Err(ForgeError::Shutdown)
    } else {
        Ok(())
    }
}

/// Validates the only manifest/entry content pairs retained traversal accepts.
///
/// # Errors
///
/// Returns [`ForgeError::LiveSet`] when manifest content and entry content
/// disagree.
fn validate_retained_manifest_entry(
    manifest: ManifestContentType,
    entry: DataContentType,
) -> Result<(), ForgeError> {
    match (manifest, entry) {
        (ManifestContentType::Data, DataContentType::Data)
        | (
            ManifestContentType::Deletes,
            DataContentType::PositionDeletes | DataContentType::EqualityDeletes,
        ) => Ok(()),
        _ => Err(ForgeError::LiveSet {
            detail: "manifest content and entry content disagree".to_owned(),
        }),
    }
}

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

    /// Returns the exact protected keys for production-traversal integration tests.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn paths_for_test(&self) -> &BTreeSet<String> {
        &self.paths
    }
}

/// Fresh evidence used by both initial and immediate final GC decisions.
pub(crate) struct MaintenanceProtection {
    /// Every validated object reachable from retained or nonterminal work.
    pub(crate) live_set: ProtectedLiveSet,
    /// Fail-closed permission derived from every open operation family.
    pub(crate) destructive_maintenance: DestructiveMaintenance,
    /// Exact output paths whose operation durably Reset without publication.
    never_published: BTreeSet<String>,
    /// The table lease's single captured maintenance time.
    pub(crate) now: DateTime<Utc>,
    /// Inclusive last-modified cutoff derived once from `now`.
    object_age_cutoff: Timestamp,
}

/// Object-store evidence evaluated by the shared eligibility predicate.
#[derive(Clone, Copy)]
pub(crate) enum ObjectEvidence<'metadata> {
    /// The object exists with freshly loaded metadata.
    Present(&'metadata opendal::Metadata),
    /// The object was absent at the evidence load.
    Missing,
}

/// Closed reason produced by the shared initial/final GC predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GcEligibility {
    /// Every destructive-maintenance precondition is proven.
    Eligible,
    /// A retained/open reference or destructive gate protects the object.
    Protected,
    /// The object's age is absent or newer than the inclusive cutoff.
    TooYoung,
    /// The path resolves to a non-file object.
    NotFile,
    /// The path is outside the binding or is not a known Iceberg object type.
    InvalidPath,
    /// The object does not currently exist.
    Missing,
}

impl MaintenanceProtection {
    /// Constructs one complete protection snapshot from bounded durable evidence.
    fn new(
        live_set: ProtectedLiveSet,
        blocked: bool,
        never_published: BTreeSet<String>,
        now: DateTime<Utc>,
        object_age_cutoff: Timestamp,
    ) -> Self {
        Self {
            live_set,
            destructive_maintenance: if blocked {
                DestructiveMaintenance::Blocked
            } else {
                DestructiveMaintenance::Allowed
            },
            never_published,
            now,
            object_age_cutoff,
        }
    }

    /// Applies the single closed eligibility truth used before prepare and delete.
    #[must_use]
    pub(crate) fn gc_eligibility(
        &self,
        binding: &TenantTableBinding,
        path: &str,
        evidence: ObjectEvidence<'_>,
    ) -> GcEligibility {
        let Some(normalized) = binding.validate_object_path(path) else {
            return GcEligibility::InvalidPath;
        };
        if !is_never_published_forge_generation(&normalized) {
            return GcEligibility::InvalidPath;
        }
        if self.destructive_maintenance == DestructiveMaintenance::Blocked
            || self.live_set.contains(&normalized)
        {
            return GcEligibility::Protected;
        }
        if !self.never_published.contains(&normalized) {
            return GcEligibility::Protected;
        }
        let ObjectEvidence::Present(metadata) = evidence else {
            return GcEligibility::Missing;
        };
        if metadata.mode() != EntryMode::FILE {
            return GcEligibility::NotFile;
        }
        if metadata
            .last_modified()
            .is_none_or(|modified| modified > self.object_age_cutoff)
        {
            return GcEligibility::TooYoung;
        }
        GcEligibility::Eligible
    }
}

/// Recognize only immutable Forge attempt-generation data paths.
///
/// Scribe pod/ULID names and generic Iceberg metadata paths deliberately fail
/// this predicate and can be reclaimed only from committed expiry evidence.
fn is_never_published_forge_generation(path: &str) -> bool {
    let marker = format!(
        "/data/forge/{}/",
        crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION
    );
    let Some((_, file_name)) = path.rsplit_once(&marker) else {
        return false;
    };
    let Some(stem) = file_name.strip_suffix(".parquet") else {
        return false;
    };
    let Some((generation, ordinal)) = stem.rsplit_once('-') else {
        return false;
    };
    ordinal.len() == 5
        && ordinal.bytes().all(|byte| byte.is_ascii_digit())
        && Uuid::parse_str(generation).is_ok_and(|uuid| uuid.get_version_num() == 7)
}

/// Validated identity of the one prepared GC batch allowed to finish itself.
struct CurrentGcExemption {
    /// Exact prepared operation identity.
    operation_id: Uuid,
    /// Canonical immutable prepared audit evidence.
    canonical_prepared_detail: String,
    /// Exact normalized candidate set owned by the delete loop.
    candidate_paths: BTreeSet<String>,
}

/// Stable table-scoped inputs shared by one fenced GC workflow.
struct GcTableContext<'context> {
    /// Durable table identity used by operation projections.
    key: &'context ForgeTableKey,
    /// Validated physical table binding used by every path check.
    binding: &'context TenantTableBinding,
    /// Single maintenance time captured by the scheduler.
    now: DateTime<Utc>,
    /// Cancellation boundary checked before every destructive transition.
    stop: &'context CancellationToken,
    /// Outputs protected by open staging reconciliation.
    staging_protected_paths: &'context BTreeSet<String>,
    /// Outputs protected by open live-rewrite reconciliation.
    live_protected_paths: &'context BTreeSet<String>,
}

/// Inputs that distinguish a fresh protection load from a GC self-reload.
struct ProtectionRequest<'context> {
    /// Shared table-scoped workflow context.
    table: &'context GcTableContext<'context>,
    /// Prepared GC detail allowed to exempt only its exact operation.
    current_gc_detail: Option<&'context AuditDetail>,
}

/// Inputs for completing one prepared GC batch.
struct GcBatchRequest<'context> {
    /// Shared table-scoped workflow context.
    table: &'context GcTableContext<'context>,
    /// Canonical prepared audit detail owning the candidate paths.
    detail: &'context AuditDetail,
    /// Whether terminal evidence records recovery rather than first commit.
    recovered: bool,
}

/// Catalog-derived live paths and the validated table metadata location.
struct CatalogProtection {
    /// Paths reachable from retained Iceberg catalog state.
    live: ProtectedLiveSet,
    /// Catalog location used to normalize SQL and audit paths.
    table_location: String,
}

/// Reconciliation-owned output paths protected during one GC stage.
pub(super) struct GcProtectedPaths<'paths> {
    /// Outputs protected by open staging reconciliation.
    pub(super) staging: &'paths BTreeSet<String>,
    /// Outputs protected by open live-rewrite reconciliation.
    pub(super) live: &'paths BTreeSet<String>,
}

/// Returns output objects that nonterminal operations must retain.
fn operation_output_paths(detail: &AuditDetail) -> &[StoragePath] {
    match detail {
        AuditDetail::ForgeCompaction { output_paths, .. }
        | AuditDetail::ForgeIcebergRewrite { output_paths, .. } => output_paths,
        _ => &[],
    }
}

/// Validates the typed self-exemption against the parity-proven open-state row.
///
/// # Errors
///
/// Returns [`ForgeError::Reconciliation`] unless the supplied detail and
/// exactly one open row have identical resource, family, phase, audit JSON,
/// operation identity, and duplicate-free normalized candidates.
fn current_gc_exemption(
    resource: &str,
    detail: Option<&AuditDetail>,
    open_gc: &[ForgeOperationStateRow],
    normalize: impl Fn(&StoragePath) -> Result<String, ForgeError>,
) -> Result<Option<CurrentGcExemption>, ForgeError> {
    let Some(detail) = detail else {
        return Ok(None);
    };
    let AuditDetail::ForgeOrphanGc {
        operation_id,
        phase: ForgeOrphanGcPhase::Prepared,
        group,
        candidate_paths,
        deleted_paths,
        skipped_paths,
    } = detail
    else {
        return Err(ForgeError::Reconciliation {
            detail: "current orphan-GC exemption is not a prepared GC detail".to_owned(),
        });
    };
    if group != resource || !deleted_paths.is_empty() || !skipped_paths.is_empty() {
        return Err(ForgeError::Reconciliation {
            detail: "current orphan-GC exemption has noncanonical resource or results".to_owned(),
        });
    }
    let normalized = candidate_paths
        .iter()
        .map(&normalize)
        .collect::<Result<BTreeSet<_>, _>>()?;
    if normalized.len() != candidate_paths.len() {
        return Err(ForgeError::Reconciliation {
            detail: "current orphan-GC exemption contains duplicate candidates".to_owned(),
        });
    }
    let matches = open_gc
        .iter()
        .filter(|row| row.operation_id == *operation_id)
        .collect::<Vec<_>>();
    let [row] = matches.as_slice() else {
        return Err(ForgeError::Reconciliation {
            detail: "current orphan-GC exemption must match exactly one open operation".to_owned(),
        });
    };
    let row_candidates = match &row.prepared_detail {
        AuditDetail::ForgeOrphanGc {
            candidate_paths, ..
        } => candidate_paths
            .iter()
            .map(normalize)
            .collect::<Result<BTreeSet<_>, _>>()?,
        _ => BTreeSet::new(),
    };
    let canonical = audit_detail_canonical_json(detail);
    if row.resource != resource
        || row.family != ForgeOperationFamily::OrphanGc
        || row.phase != ForgeOperationPhase::Prepared
        || audit_detail_canonical_json(&row.prepared_detail) != canonical
        || row_candidates.len() != candidate_paths.len()
        || row_candidates != normalized
    {
        return Err(ForgeError::Reconciliation {
            detail: "current orphan-GC exemption failed prepared-state parity".to_owned(),
        });
    }
    Ok(Some(CurrentGcExemption {
        operation_id: *operation_id,
        canonical_prepared_detail: canonical,
        candidate_paths: normalized,
    }))
}

/// Rejects a self-exemption that lacks canonical candidate evidence.
///
/// # Errors
///
/// Returns [`ForgeError::Reconciliation`] for empty canonical detail or paths.
fn validate_gc_exemption(exemption: Option<&CurrentGcExemption>) -> Result<(), ForgeError> {
    if exemption.is_some_and(|value| {
        value.canonical_prepared_detail.is_empty() || value.candidate_paths.is_empty()
    }) {
        Err(ForgeError::Reconciliation {
            detail: "current orphan-GC exemption has empty canonical evidence".to_owned(),
        })
    } else {
        Ok(())
    }
}

/// Exercises the production self-exemption and remaining-open gate in integration tests.
///
/// # Errors
///
/// Returns the production reconciliation error when `detail` does not have
/// exact prepared-state parity with one row.
#[cfg(feature = "test-support")]
pub fn current_gc_gate_for_test(
    resource: &str,
    detail: &AuditDetail,
    open_gc: &[ForgeOperationStateRow],
) -> Result<bool, ForgeError> {
    let exemption = current_gc_exemption(resource, Some(detail), open_gc, |path| {
        Ok(path.as_str().to_owned())
    })?
    .ok_or_else(|| ForgeError::Reconciliation {
        detail: "test exemption unexpectedly absent".to_owned(),
    })?;
    Ok(!open_gc
        .iter()
        .any(|row| row.operation_id != exemption.operation_id))
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
        now: DateTime<Utc>,
        stop: &CancellationToken,
        protected_paths: GcProtectedPaths<'_>,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        let table = GcTableContext {
            key,
            binding,
            now,
            stop,
            staging_protected_paths: protected_paths.staging,
            live_protected_paths: protected_paths.live,
        };
        self.run_orphan_gc_for_table_inner(lease, &table).await
    }

    /// Loads one complete, reloadable maintenance-protection proof.
    ///
    /// # Errors
    ///
    /// Returns catalog, SQL, path-validation, or live-set failures.
    async fn load_maintenance_protection(
        &self,
        request: ProtectionRequest<'_>,
    ) -> Result<MaintenanceProtection, ForgeError> {
        self.load_maintenance_protection_inner(request).await
    }

    /// Loads production maintenance protection for one integration fixture.
    ///
    /// # Errors
    ///
    /// Returns the same catalog, manifest, SQL, operation-state, path, clock,
    /// and cancellation errors as the production maintenance loader.
    #[cfg(feature = "test-support")]
    pub async fn load_maintenance_protection_for_test(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<ProtectedLiveSet, ForgeError> {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let stop = CancellationToken::new();
        let staging = BTreeSet::new();
        let live = BTreeSet::new();
        let table = GcTableContext {
            key: &key,
            binding,
            now: self.core.clock.now()?,
            stop: &stop,
            staging_protected_paths: &staging,
            live_protected_paths: &live,
        };
        self.load_maintenance_protection(ProtectionRequest {
            table: &table,
            current_gc_detail: None,
        })
        .await
        .map(|protection| protection.live_set)
    }

    /// Classify one path through the complete production GC eligibility proof.
    ///
    /// # Errors
    ///
    /// Returns catalog, manifest, SQL, operation-state, path, clock, object
    /// metadata, or cancellation failures from the production protection path.
    #[cfg(feature = "test-support")]
    pub async fn gc_eligibility_for_test(
        &self,
        binding: &TenantTableBinding,
        path: &str,
    ) -> Result<String, ForgeError> {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let stop = CancellationToken::new();
        let staging = BTreeSet::new();
        let live = BTreeSet::new();
        let table = GcTableContext {
            key: &key,
            binding,
            now: self.core.clock.now()?,
            stop: &stop,
            staging_protected_paths: &staging,
            live_protected_paths: &live,
        };
        let protection = self
            .load_maintenance_protection(ProtectionRequest {
                table: &table,
                current_gc_detail: None,
            })
            .await?;
        let metadata = self
            .core
            .object_store
            .stat(path)
            .await
            .map_err(ForgeError::ObjectDelete)?;
        Ok(format!(
            "{:?}",
            protection.gc_eligibility(binding, path, ObjectEvidence::Present(&metadata))
        ))
    }

    /// Runs one table-scoped orphan collection pass for integration fixtures.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, object-store, SQL, audit, or live-set failures
    /// from the unchanged production GC owner.
    #[cfg(feature = "test-support")]
    pub async fn run_orphan_gc_for_test(
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
        let staging = BTreeSet::new();
        let live = BTreeSet::new();
        let outcome = self
            .run_orphan_gc_for_table(
                &mut lease,
                &key,
                binding,
                self.core.clock.now()?,
                &CancellationToken::new(),
                GcProtectedPaths {
                    staging: &staging,
                    live: &live,
                },
            )
            .await?;
        lease.release(&self.core.operator_pool).await?;
        Ok(outcome.deleted)
    }
}

impl Forge {
    async fn run_orphan_gc_for_table_inner(
        &self,
        lease: &mut ForgeLease,
        table: &GcTableContext<'_>,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        let mut outcome = self.reconcile_gc(lease, table).await?;
        let protection = self
            .load_maintenance_protection(ProtectionRequest {
                table,
                current_gc_detail: None,
            })
            .await?;
        if protection.destructive_maintenance == DestructiveMaintenance::Blocked {
            outcome.pending = outcome.pending.saturating_add(1);
            return Ok(outcome);
        }
        let candidates = self.list_gc_candidates(table.binding, &protection).await?;
        outcome.candidates = outcome.candidates.saturating_add(candidates.len());
        if candidates.is_empty() {
            return Ok(outcome);
        }
        let detail = Self::gc_detail(table.key, candidates)?;
        self.append_gc_audit(lease, table.key.tenant, &detail, "forge.orphan_gc.prepared")
            .await?;
        let (deleted, skipped) = self
            .delete_gc_batch(
                lease,
                GcBatchRequest {
                    table,
                    detail: &detail,
                    recovered: false,
                },
            )
            .await?;
        outcome.recovered = outcome.recovered.saturating_add(1);
        outcome.deleted = outcome.deleted.saturating_add(deleted);
        outcome.skipped = outcome.skipped.saturating_add(skipped);
        Ok(outcome)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OrphanGcOutcome {
    /// Exact candidate paths considered across new and reconciled batches.
    pub(crate) candidates: usize,
    /// Prepared batches completed or recovered by this pass.
    pub(crate) recovered: usize,
    /// Candidate paths deleted or already absent at deletion time.
    pub(crate) deleted: usize,
    /// Candidate paths retained after a fresh fence or live-set recheck.
    pub(crate) skipped: usize,
    /// Open prepared operations left pending after bounded reconciliation.
    pub(crate) pending: usize,
}

/// Build protection from every retained Iceberg object and every open workflow.
impl Forge {
    async fn load_maintenance_protection_inner(
        &self,
        request: ProtectionRequest<'_>,
    ) -> Result<MaintenanceProtection, ForgeError> {
        let table_context = request.table;
        let key = table_context.key;
        let binding = table_context.binding;
        require_running(table_context.stop)?;
        let CatalogProtection {
            mut live,
            table_location,
        } = self.load_catalog_protection(binding).await?;
        let mut add = |path: &str| self.add_path(&mut live, binding, &table_location, path);
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let rows = list_nonterminal_file_paths(
            &mut conn,
            key.table_ref.namespace.as_str(),
            &key.table_ref.name,
            None,
        )
        .await
        .map_err(ForgeError::Sql)?;
        for path in rows {
            add(&path)?;
        }

        for path in table_context
            .staging_protected_paths
            .iter()
            .chain(table_context.live_protected_paths.iter())
        {
            add(path)?;
        }

        let resource = table_resource_for_key(key);
        let cap = self.core.config.max_open_operations_per_table;
        let mut blocked = false;
        let (never_published, reset_overflowed) = self
            .load_never_published_generations(&mut conn, &resource, &table_location, binding, cap)
            .await?;
        blocked |= reset_overflowed;
        for family in [
            ForgeOperationFamily::IcebergRewrite,
            ForgeOperationFamily::SnapshotExpire,
        ] {
            let page = ForgeOperations::new(&resource, family)
                .map_err(ForgeError::Sql)?
                .list_open(&mut conn, cap)
                .await
                .map_err(ForgeError::Sql)?;
            blocked |= page.overflowed || !page.operations.is_empty();
            for row in &page.operations {
                for path in operation_output_paths(&row.prepared_detail) {
                    add(path.as_str())?;
                }
            }
        }
        let open_gc = ForgeOperations::new(&resource, ForgeOperationFamily::OrphanGc)
            .map_err(ForgeError::Sql)?
            .list_open(&mut conn, cap)
            .await
            .map_err(ForgeError::Sql)?;
        blocked |= open_gc.overflowed;
        let exemption = current_gc_exemption(
            &resource,
            request.current_gc_detail,
            &open_gc.operations,
            |path| {
                catalog_path_to_object_key(
                    &table_location,
                    binding,
                    &self.core.staging,
                    path.as_str(),
                )
            },
        )?;
        validate_gc_exemption(exemption.as_ref())?;
        for row in &open_gc.operations {
            if exemption
                .as_ref()
                .is_some_and(|value| value.operation_id == row.operation_id)
            {
                continue;
            }
            blocked = true;
            for path in operation_output_paths(&row.prepared_detail) {
                add(path.as_str())?;
            }
        }
        conn.commit().await.map_err(ForgeError::Sql)?;
        require_running(table_context.stop)?;

        let object_age_cutoff = self.gc_object_age_cutoff(table_context.now)?;
        Ok(MaintenanceProtection::new(
            live,
            blocked,
            never_published,
            table_context.now,
            object_age_cutoff,
        ))
    }

    /// Loads bounded terminal Reset outputs as exact no-resurrection evidence.
    ///
    /// # Errors
    ///
    /// Returns SQL, operation-state, or table-binding failures. Overflow is
    /// returned explicitly so the caller blocks all destructive maintenance.
    async fn load_never_published_generations(
        &self,
        conn: &mut TenantConn<'_>,
        resource: &str,
        table_location: &str,
        binding: &TenantTableBinding,
        cap: usize,
    ) -> Result<(BTreeSet<String>, bool), ForgeError> {
        let mut paths = BTreeSet::new();
        let mut overflowed = false;
        for family in [
            ForgeOperationFamily::StagingFold,
            ForgeOperationFamily::IcebergRewrite,
        ] {
            let page = ForgeOperations::new(resource, family)
                .map_err(ForgeError::Sql)?
                .list_reset(conn, cap)
                .await
                .map_err(ForgeError::Sql)?;
            overflowed |= page.overflowed;
            for row in page.operations {
                for path in operation_output_paths(&row.prepared_detail) {
                    paths.insert(catalog_path_to_object_key(
                        table_location,
                        binding,
                        &self.core.staging,
                        path.as_str(),
                    )?);
                }
            }
        }
        Ok((paths, overflowed))
    }

    /// Traverses every retained Iceberg object for one validated table.
    ///
    /// # Errors
    ///
    /// Returns catalog, manifest, path-validation, or retained-snapshot-cap failures.
    async fn load_catalog_protection(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<CatalogProtection, ForgeError> {
        let table = self.load_table(&binding.table_ident()).await?;
        self.assert_table_location(table.metadata(), binding)?;
        let retained_snapshot_count = table.metadata().snapshots().count();
        if retained_snapshot_count > self.core.config.max_retained_snapshots_per_table {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "retained snapshot count {retained_snapshot_count} exceeds configured cap {}",
                    self.core.config.max_retained_snapshots_per_table
                ),
            });
        }
        let mut live = ProtectedLiveSet::default();
        let table_location = table.metadata().location().to_owned();
        let mut add = |path: &str| self.add_path(&mut live, binding, &table_location, path);
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
                for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                    validate_retained_manifest_entry(manifest_file.content, entry.content_type())?;
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

        Ok(CatalogProtection {
            live,
            table_location,
        })
    }

    /// Derives the inclusive object-store age cutoff from one captured tick time.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the TTL or timestamp overflows.
    fn gc_object_age_cutoff(&self, now: DateTime<Utc>) -> Result<Timestamp, ForgeError> {
        let cutoff_chrono = now
            .checked_sub_signed(
                chrono::Duration::from_std(self.core.config.orphan_gc_ttl).map_err(|_| {
                    ForgeError::InvalidConfig {
                        detail: "orphan GC TTL cannot be represented by chrono".to_owned(),
                    }
                })?,
            )
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "orphan GC age cutoff overflows UTC".to_owned(),
            })?;
        Timestamp::from_millisecond(cutoff_chrono.timestamp_millis()).map_err(|_| {
            ForgeError::InvalidConfig {
                detail: "orphan GC cutoff cannot be represented by object storage".to_owned(),
            }
        })
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
        protection: &MaintenanceProtection,
    ) -> Result<Vec<String>, ForgeError> {
        tracing::debug!(
            captured_now = %protection.now,
            prefix = %binding.object_prefix,
            "enumerating bounded orphan-GC candidates"
        );
        let prefix = format!("{}/", binding.object_prefix.trim_end_matches('/'));
        let entries = self
            .core
            .object_store
            .list(&prefix)
            .await
            .map_err(ForgeError::ObjectList)?;
        let mut candidates = entries
            .into_iter()
            .filter_map(|entry| {
                let path = entry.path().to_owned();
                (protection.gc_eligibility(
                    binding,
                    &path,
                    ObjectEvidence::Present(entry.metadata()),
                ) == GcEligibility::Eligible)
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
        request: GcBatchRequest<'_>,
    ) -> Result<(usize, usize), ForgeError> {
        let table = request.table;
        let detail = request.detail;
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
        if group != &table_resource_for_key(table.key)
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
            require_running(table.stop)?;
            lease.require_fence(&self.core.operator_pool).await?;
            let normalized = table
                .binding
                .validate_object_path(path.as_str())
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: format!("orphan-GC candidate escaped table binding: {path:?}"),
                })?;
            let protection = self
                .load_maintenance_protection(ProtectionRequest {
                    table,
                    current_gc_detail: Some(detail),
                })
                .await?;
            let metadata = match self.core.object_store.stat(&normalized).await {
                Ok(metadata) => Some(metadata),
                Err(error) if error.kind() == ErrorKind::NotFound => None,
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            };
            let eligibility = protection.gc_eligibility(
                table.binding,
                &normalized,
                metadata
                    .as_ref()
                    .map_or(ObjectEvidence::Missing, ObjectEvidence::Present),
            );
            if eligibility == GcEligibility::Missing {
                deleted.push(path.as_str().to_owned());
                continue;
            }
            if eligibility != GcEligibility::Eligible {
                skipped.push(path.as_str().to_owned());
                continue;
            }
            require_running(table.stop)?;
            lease.require_fence(&self.core.operator_pool).await?;
            match self.core.object_store.delete(&normalized).await {
                Ok(()) => deleted.push(path.as_str().to_owned()),
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    deleted.push(path.as_str().to_owned());
                }
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            }
        }
        require_running(table.stop)?;
        lease.require_fence(&self.core.operator_pool).await?;
        let deleted_count = deleted.len();
        let skipped_count = skipped.len();
        let terminal = Self::terminal_gc_detail(
            detail,
            deleted,
            skipped,
            if request.recovered {
                ForgeOrphanGcPhase::Recovered
            } else {
                ForgeOrphanGcPhase::Committed
            },
        );
        self.append_gc_audit(
            lease,
            table.key.tenant,
            &terminal,
            if request.recovered {
                "forge.orphan_gc.recovered"
            } else {
                "forge.orphan_gc.committed"
            },
        )
        .await?;
        Ok((deleted_count, skipped_count))
    }

    /// Replays the cap-bounded, parity-proven open orphan-GC projection.
    ///
    /// # Errors
    /// Returns audit, lease, object-store, catalog, or SQL failures.
    async fn reconcile_gc(
        &self,
        lease: &mut ForgeLease,
        table: &GcTableContext<'_>,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        require_running(table.stop)?;
        let resource = table_resource_for_key(table.key);
        let mut conn = self
            .core
            .vala
            .tenant_conn(table.key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = ForgeOperations::new(&resource, ForgeOperationFamily::OrphanGc)
            .map_err(ForgeError::Sql)?
            .list_open(&mut conn, self.core.config.max_open_operations_per_table)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        let mut outcome = OrphanGcOutcome::default();
        if page.overflowed {
            outcome.pending = self
                .core
                .config
                .max_open_operations_per_table
                .saturating_add(1);
            return Ok(outcome);
        }
        for row in page.operations {
            require_running(table.stop)?;
            let detail = row.prepared_detail;
            let AuditDetail::ForgeOrphanGc {
                candidate_paths, ..
            } = &detail
            else {
                return Err(ForgeError::Reconciliation {
                    detail: "orphan-GC prepared audit has the wrong kind".to_owned(),
                });
            };
            let (deleted, skipped) = self
                .delete_gc_batch(
                    lease,
                    GcBatchRequest {
                        table,
                        detail: &detail,
                        recovered: true,
                    },
                )
                .await?;
            outcome.candidates = outcome.candidates.saturating_add(candidate_paths.len());
            outcome.recovered = outcome.recovered.saturating_add(1);
            outcome.deleted = outcome.deleted.saturating_add(deleted);
            outcome.skipped = outcome.skipped.saturating_add(skipped);
        }
        Ok(outcome)
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

    /// Apply generic Forge-generation eligibility to a test fixture path.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn known_iceberg_object_for_test(path: &str) -> bool {
        is_never_published_forge_generation(path)
    }

    /// Load the exact terminal Reset generation set used by production GC.
    ///
    /// # Errors
    ///
    /// Returns catalog, tenant-connection, operation-state, or path-normalization
    /// failures from the production protection loader.
    #[cfg(feature = "test-support")]
    pub async fn reset_generation_paths_for_test(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<Vec<String>, ForgeError> {
        let table = self.load_table(&binding.table_ident()).await?;
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let resource = table_resource_for_key(&key);
        let mut conn = self
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let (paths, overflowed) = self
            .load_never_published_generations(
                &mut conn,
                &resource,
                table.metadata().location(),
                binding,
                self.core.config.max_open_operations_per_table,
            )
            .await?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if overflowed {
            return Err(ForgeError::Reconciliation {
                detail: "Reset generation test query overflowed".to_owned(),
            });
        }
        Ok(paths.into_iter().collect())
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
    fn maintenance_protection_traverses_retained_history() {
        let mut live = ProtectedLiveSet::default();
        live.insert("tenants/t/traces/spans/data/live.parquet");
        live.insert("tenants/t/traces/spans/metadata/v1.metadata.json");
        live.insert("tenants/t/traces/spans/metadata/snap-m0.avro");
        live.insert("tenants/t/traces/spans/metadata/snap-m1.avro");
        assert_eq!(live.paths.len(), 4);
        assert!(live.contains("tenants/t/traces/spans/metadata/snap-m1.avro"));
    }

    #[test]
    /// Covers every accepted retained manifest pair and rejects cross-content drift.
    fn retained_manifest_entry_parity_covers_data_and_delete_files() {
        assert!(
            validate_retained_manifest_entry(ManifestContentType::Data, DataContentType::Data)
                .is_ok()
        );
        assert!(
            validate_retained_manifest_entry(
                ManifestContentType::Deletes,
                DataContentType::PositionDeletes
            )
            .is_ok()
        );
        assert!(
            validate_retained_manifest_entry(
                ManifestContentType::Deletes,
                DataContentType::EqualityDeletes
            )
            .is_ok()
        );
        assert!(
            validate_retained_manifest_entry(
                ManifestContentType::Data,
                DataContentType::PositionDeletes
            )
            .is_err()
        );
        assert!(
            validate_retained_manifest_entry(ManifestContentType::Deletes, DataContentType::Data)
                .is_err()
        );
    }

    #[test]
    fn gc_eligibility_uses_one_initial_and_final_truth_table() {
        use crate::catalog::TableRef;
        use crate::namespaces::BifrostNamespace;

        let old = Timestamp::from_millisecond(0).expect("timestamp");
        let young = Timestamp::from_millisecond(47 * 60 * 60 * 1_000).expect("timestamp");
        let cutoff = Timestamp::from_millisecond(24 * 60 * 60 * 1_000).expect("timestamp");
        let binding = TenantTableBinding::resolve((
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("binding");
        let forge_path = |generation: Uuid| {
            format!(
                "{}/data/forge/{}/{generation}-00000.parquet",
                binding.object_prefix,
                crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION
            )
        };
        let old_path = forge_path(Uuid::now_v7());
        let unproven_path = forge_path(Uuid::now_v7());
        let young_path = forge_path(Uuid::now_v7());
        let protected_path = forge_path(Uuid::now_v7());
        let mut live = ProtectedLiveSet::default();
        live.insert(protected_path.clone());
        let protection = MaintenanceProtection {
            live_set: live,
            destructive_maintenance: DestructiveMaintenance::Allowed,
            never_published: BTreeSet::from([old_path.clone(), young_path.clone()]),
            now: DateTime::<Utc>::from_timestamp_millis(48 * 60 * 60 * 1_000).expect("time"),
            object_age_cutoff: cutoff,
        };
        let old_metadata = opendal::Metadata::new(EntryMode::FILE).with_last_modified(old);
        let young_metadata = opendal::Metadata::new(EntryMode::FILE).with_last_modified(young);
        let directory = opendal::Metadata::new(EntryMode::DIR);

        assert_eq!(
            protection.gc_eligibility(&binding, &old_path, ObjectEvidence::Present(&old_metadata)),
            GcEligibility::Eligible
        );
        assert_eq!(
            protection.gc_eligibility(
                &binding,
                &unproven_path,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Protected,
            "UUIDv7 name and age do not substitute for terminal Reset evidence"
        );
        assert_eq!(
            protection.gc_eligibility(
                &binding,
                &protected_path,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Protected
        );
        assert_eq!(
            protection.gc_eligibility(
                &binding,
                &young_path,
                ObjectEvidence::Present(&young_metadata)
            ),
            GcEligibility::TooYoung
        );
        assert_eq!(
            protection.gc_eligibility(&binding, &old_path, ObjectEvidence::Present(&directory)),
            GcEligibility::NotFile
        );
        assert_eq!(
            protection.gc_eligibility(
                &binding,
                "other/data/file.parquet",
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::InvalidPath
        );
        assert_eq!(
            protection.gc_eligibility(&binding, &old_path, ObjectEvidence::Missing),
            GcEligibility::Missing
        );
        assert_eq!(protection.now.timestamp_millis(), 48 * 60 * 60 * 1_000);
    }

    /// Generic orphan collection excludes Scribe pod/ULID and Iceberg-owned paths.
    #[test]
    fn generic_gc_accepts_only_forge_attempt_generations() {
        let generation = Uuid::now_v7();
        assert!(is_never_published_forge_generation(&format!(
            "tenant/table/data/forge/{}/{generation}-00000.parquet",
            crate::parquet::writer_properties::BIFROST_WRITER_RECIPE_VERSION
        )));
        assert!(!is_never_published_forge_generation(
            "tenant/table/data/pod-a-01JABC.parquet"
        ));
        assert!(!is_never_published_forge_generation(
            "tenant/table/metadata/v1.metadata.json"
        ));
    }
}

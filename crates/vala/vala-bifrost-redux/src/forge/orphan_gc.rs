//! Reference-aware orphan garbage collection for Forge objects.

use std::collections::BTreeSet;
use std::time::Instant;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use iceberg::spec::{DataContentType, ManifestContentType, TableMetadata};
use opendal::raw::Timestamp;
use opendal::{EntryMode, ErrorKind};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
#[cfg(feature = "test-support")]
use vala_sql::TenantConn;
use vala_sql::queries::file_list::list_nonterminal_file_paths;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::queries::forge_tasks::list_prepared_cleanup_candidates;
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

use vala_sql::row_types::forge_tasks::{ForgeCleanupCandidate, ForgeTaskStrategy};

use crate::catalog::layout::{FORGE_DATA_MARKER, FORGE_WRITER_RECIPE};
use crate::catalog::{BIFROST_CATALOG_NAME, TenantTableBinding};

use super::Forge;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::expire::table_resource_for_key;
use super::lease::ForgeLease;
#[cfg(feature = "test-support")]
use super::lease::forge_lease_key;
use super::live_reconcile::DestructiveMaintenance;
use super::managed::identity::ForgeOutputIdentity;
use super::metrics::ForgeTelemetry;
use super::path::{catalog_path_to_object_key, validate_table_location};
use super::protection_roots::OrphanProtectionRoots;

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
            now,
            object_age_cutoff,
        }
    }

    /// Applies the single closed eligibility truth used before prepare and delete.
    ///
    /// The exact table lease excludes a concurrent producer while refreshed
    /// catalog, SQL, and operation evidence protects every reachable object.
    /// Once the configured TTL floor has elapsed, an unreferenced deterministic
    /// attempt generation needs no terminal operation row: this admits outputs
    /// whose rewrite failed before its `Prepared` transition could persist.
    #[must_use]
    pub(crate) fn gc_eligibility(
        &self,
        binding: &TenantTableBinding,
        path: &str,
        evidence: ObjectEvidence<'_>,
    ) -> GcEligibility {
        self.eligibility(binding, path, evidence, MaintenanceScope::AttemptGeneration)
    }

    /// Applies the same truth to one prepared expired-file cleanup candidate.
    ///
    /// The prepared candidate set is durable evidence of what was unreachable
    /// when expiry committed, not permission to delete later. A successor owner
    /// draining that set may be running long afterwards, so every proof is
    /// re-taken here against refreshed catalog, SQL, and object evidence: the
    /// candidate must still be inside this tenant's table binding, destructive
    /// maintenance must still be permitted, no retained head may reach it
    /// again, and it must still clear the object age floor.
    ///
    /// Unlike [`Self::gc_eligibility`] this admits any object the binding
    /// accepts, because expiry legitimately strands manifests, manifest lists,
    /// statistics, and metadata logs that no attempt-generation grammar covers.
    #[must_use]
    pub(crate) fn expired_cleanup_eligibility(
        &self,
        binding: &TenantTableBinding,
        path: &str,
        evidence: ObjectEvidence<'_>,
    ) -> GcEligibility {
        self.eligibility(binding, path, evidence, MaintenanceScope::ExpiredCandidate)
    }

    /// Decides one object under the scope-specific path rule.
    ///
    /// Scope changes only which paths are addressable at all. Every safety
    /// proof after that — binding, destructive gate, live-set containment,
    /// existence, object kind, and age — is shared, so the two destructive
    /// protocols cannot diverge on what protects an object.
    fn eligibility(
        &self,
        binding: &TenantTableBinding,
        path: &str,
        evidence: ObjectEvidence<'_>,
        scope: MaintenanceScope,
    ) -> GcEligibility {
        let Some(normalized) = binding.validate_object_path(path) else {
            return GcEligibility::InvalidPath;
        };
        if !scope.admits(binding, &normalized) {
            return GcEligibility::InvalidPath;
        }
        if self.destructive_maintenance == DestructiveMaintenance::Blocked
            || self.live_set.contains(&normalized)
        {
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

/// Which object paths one destructive maintenance protocol may address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaintenanceScope {
    /// Never-published orphan collection: immutable Forge attempt generations
    /// only, so no catalog-owned object can be reached by listing alone.
    AttemptGeneration,
    /// Expired-file cleanup: any object inside the binding, because expiry
    /// strands catalog metadata that no attempt grammar describes.
    ExpiredCandidate,
}

impl MaintenanceScope {
    /// Reports whether this scope may consider the normalized object key.
    ///
    /// Attempt-generation scope defers entirely to the writer's own recipe
    /// grammar, so cleanup can never recognize a shape the writer does not
    /// emit. Scribe pod/ULID names and catalog-owned metadata deliberately fail
    /// that parse and remain reclaimable only from committed expiry evidence.
    fn admits(self, binding: &TenantTableBinding, normalized: &str) -> bool {
        match self {
            Self::AttemptGeneration => {
                ForgeOutputIdentity::parse(normalized, &forge_data_prefix(binding)).is_ok()
            }
            Self::ExpiredCandidate => true,
        }
    }
}

/// Returns this binding's current Forge recipe root as a normalized object key.
///
/// The catalog's `forge_data_location` is derived from the table location; the
/// binding's `object_prefix` is that same table's object-key form, so appending
/// the fixed marker and recipe here yields the identical root without needing a
/// loaded table.
pub(super) fn forge_data_prefix(binding: &TenantTableBinding) -> String {
    format!(
        "{}{FORGE_DATA_MARKER}{FORGE_WRITER_RECIPE}",
        binding.object_prefix.trim_end_matches('/')
    )
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
    /// Immutable inclusive age cutoff captured when the periodic task was
    /// planned, in epoch milliseconds. `None` falls back to the configured
    /// orphan-GC TTL applied to `now`, which is what every non-task caller
    /// uses.
    age_cutoff_ms: Option<i64>,
    /// Durable scan cursor. Listing resumes strictly after this key, so a
    /// leading run of protected objects cannot starve the pages behind it.
    /// `None` starts the prefix from its beginning.
    start_after: Option<&'context str>,
    /// Durable task that owns this run, when a task drives it. The batch
    /// identity is seeded from this UUID so two periodic tasks selecting the
    /// same paths stay distinct while one task's retry and takeover do not.
    /// `None` keeps the table-seeded identity used by direct maintenance.
    task_id: Option<Uuid>,
}

/// Inputs that distinguish a fresh protection load from a GC self-reload.
struct ProtectionRequest<'context> {
    /// Shared table-scoped workflow context.
    table: &'context GcTableContext<'context>,
    /// Prepared GC detail allowed to exempt only its exact operation.
    current_gc_detail: Option<&'context AuditDetail>,
    /// Prepared expired-cleanup candidate allowed to exempt only itself.
    cleanup_exemption: Option<ExpiredCleanupExemption<'context>>,
}

/// Identity of the single prepared cleanup candidate one drain owns.
///
/// Every unresolved cleanup preparation protects its object, including this
/// one — a drain that trusted a stale preparation would delete an object whose
/// proof another attempt owns. The exemption is therefore exact in all four
/// fields: a mismatch in task, attempt, cursor index, or candidate leaves the
/// preparation as protection and the drain refuses its own candidate.
#[derive(Debug, Clone, Copy)]
pub(super) struct ExpiredCleanupExemption<'candidate> {
    /// Cleanup task the drain is executing.
    pub(super) task_id: Uuid,
    /// Attempt generation fencing that execution.
    pub(super) attempt_id: Uuid,
    /// Durable cursor index the drain prepared.
    pub(super) index: u32,
    /// Exact candidate that index resolves to.
    pub(super) candidate: &'candidate ForgeCleanupCandidate,
}

/// Inputs for completing one prepared GC batch.
struct GcBatchRequest<'context> {
    /// Shared table-scoped workflow context.
    table: &'context GcTableContext<'context>,
    /// Canonical prepared audit detail owning the candidate paths.
    detail: &'context AuditDetail,
    /// Whether terminal evidence records recovery rather than first commit.
    recovered: bool,
    /// Optional wall-clock budget for the deletion loop. `Some` bounds a fresh
    /// batch so it defers remaining candidates once the run budget is reached;
    /// `None` lets a reconciled recovery batch complete its prepared candidate
    /// set in full.
    deadline: Option<Instant>,
}

/// Durable root classes read from one tenant transaction.
#[derive(Default)]
struct DurableProtectionRoots {
    /// Committed Scribe objects without exact promotion evidence.
    hot_unpromoted: Vec<String>,
    /// Outputs produced or still producible by a nonterminal attempt.
    open_outputs: Vec<String>,
    /// Snapshots a pinned Oracle cut still depends on.
    pinned_snapshot_ids: Vec<i64>,
    /// Whether any open or unreconciled operation forbids destructive work.
    blocked: bool,
}

/// Catalog-derived live paths and the validated table metadata location.
struct CatalogProtection {
    /// Paths reachable from retained Iceberg catalog state.
    live: ProtectedLiveSet,
    /// Catalog location used to normalize SQL and audit paths.
    table_location: String,
    /// Snapshot ids the traversal visited, so a reader pin can be corroborated.
    traversed_snapshot_ids: Vec<i64>,
}

/// Returns output objects that nonterminal operations must retain.
fn operation_output_paths(detail: &AuditDetail) -> &[StoragePath] {
    match detail {
        AuditDetail::ForgeIcebergRewrite { output_paths, .. } => output_paths,
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

/// The immutable window one orphan-GC run scans.
///
/// A run driven by a durable task carries that task's own fixed cutoff and
/// resume position; the periodic maintenance path carries neither and derives
/// its cutoff from the run instant instead.
pub(super) struct OrphanGcScan<'scan> {
    /// Instant this run's protection proof is taken at.
    pub(super) now: DateTime<Utc>,
    /// Task-owned immutable object-age cutoff, when a task owns the run.
    pub(super) age_cutoff_ms: Option<i64>,
    /// Exclusive resume key taken from the task's durable cursor.
    pub(super) start_after: Option<&'scan str>,
    /// Durable task owning this run, when a task drives it. Seeds the prepared
    /// batch identity so retry and takeover of one task reuse it.
    pub(super) task_id: Option<Uuid>,
}

impl Forge {
    /// Run reconciled orphan deletion for one fenced physical table.
    ///
    /// The run is bounded: its candidate scan honors a per-run listing-page cap
    /// and wall-clock budget, and its deletion loop yields at the same budget.
    /// A run that hits either bound returns a Partial [`OrphanGcOutcome`] with
    /// no deletion attempted past the boundary; a successor run resumes from the
    /// durable committed-deletion frontier rather than a stored cursor.
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, object-store, SQL, audit, or live-set failures.
    #[tracing::instrument(skip_all, fields(tenant = %key.tenant))]
    pub(super) async fn run_orphan_gc_for_table(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        scan: OrphanGcScan<'_>,
        stop: &CancellationToken,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        let table = GcTableContext {
            key,
            binding,
            now: scan.now,
            stop,
            age_cutoff_ms: scan.age_cutoff_ms,
            start_after: scan.start_after,
            task_id: scan.task_id,
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

    /// Loads refreshed protection for one prepared expired-cleanup candidate.
    ///
    /// The proof is taken per candidate rather than once per drain because each
    /// candidate now owns its own durable preparation: the exemption below is
    /// what makes the drain's own prepared row stop protecting the object it is
    /// about to delete, and it names exactly one candidate.
    ///
    /// # Errors
    ///
    /// Returns catalog, manifest, SQL, operation-state, path-validation, clock,
    /// or cancellation failures.
    pub(super) async fn load_expired_cleanup_protection(
        &self,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        now: DateTime<Utc>,
        exemption: ExpiredCleanupExemption<'_>,
        stop: &CancellationToken,
    ) -> Result<MaintenanceProtection, ForgeError> {
        let table = GcTableContext {
            key,
            binding,
            now,
            stop,
            age_cutoff_ms: None,
            start_after: None,
            task_id: None,
        };
        self.load_maintenance_protection(ProtectionRequest {
            table: &table,
            current_gc_detail: None,
            cleanup_exemption: Some(exemption),
        })
        .await
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
        let table = GcTableContext {
            key: &key,
            binding,
            now: self.core.clock.now()?,
            stop: &stop,
            age_cutoff_ms: None,
            start_after: None,
            task_id: None,
        };
        self.load_maintenance_protection(ProtectionRequest {
            table: &table,
            current_gc_detail: None,
            cleanup_exemption: None,
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
        let table = GcTableContext {
            key: &key,
            binding,
            now: self.core.clock.now()?,
            stop: &stop,
            age_cutoff_ms: None,
            start_after: None,
            task_id: None,
        };
        let protection = self
            .load_maintenance_protection(ProtectionRequest {
                table: &table,
                current_gc_detail: None,
                cleanup_exemption: None,
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

    /// Classifies one prepared cleanup candidate under an exact self-exemption.
    ///
    /// This is the production loader and the production predicate: only the
    /// four-field exemption tuple is supplied by the caller, so an integration
    /// test can vary task, attempt, cursor index, and candidate independently
    /// and observe that every mismatch leaves the durable preparation acting as
    /// protection.
    ///
    /// # Errors
    ///
    /// Returns catalog, manifest, SQL, operation-state, path, clock, object
    /// metadata, or cancellation failures from the production protection path.
    #[cfg(feature = "test-support")]
    pub async fn expired_cleanup_eligibility_for_test(
        &self,
        binding: &TenantTableBinding,
        exemption_task_id: Uuid,
        exemption_attempt_id: Uuid,
        exemption_index: u32,
        exemption_candidate: &ForgeCleanupCandidate,
        path: &str,
    ) -> Result<String, ForgeError> {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let stop = CancellationToken::new();
        let protection = self
            .load_expired_cleanup_protection(
                &key,
                binding,
                self.core.clock.now()?,
                ExpiredCleanupExemption {
                    task_id: exemption_task_id,
                    attempt_id: exemption_attempt_id,
                    index: exemption_index,
                    candidate: exemption_candidate,
                },
                &stop,
            )
            .await?;
        let evidence = match self.core.object_store.stat(path).await {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => None,
            Err(error) => return Err(ForgeError::ObjectDelete(error)),
        };
        Ok(format!(
            "{:?}",
            protection.expired_cleanup_eligibility(
                binding,
                path,
                evidence
                    .as_ref()
                    .map_or(ObjectEvidence::Missing, ObjectEvidence::Present),
            )
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
        Ok(self.run_orphan_gc_report_for_test(binding).await?.deleted)
    }

    /// Runs one bounded orphan collection pass and reports its full outcome.
    ///
    /// Mirrors [`Self::run_orphan_gc_for_test`] but surfaces the bounded-run
    /// accounting an integration test asserts on — deleted, skipped, candidate,
    /// and pending counts plus whether a per-run bound made the run Partial — so
    /// tests can prove the page cap and run budget without reaching into the
    /// crate-private [`OrphanGcOutcome`].
    ///
    /// # Errors
    ///
    /// Returns lease, catalog, object-store, SQL, audit, or live-set failures
    /// from the unchanged production GC owner.
    #[cfg(feature = "test-support")]
    pub async fn run_orphan_gc_report_for_test(
        &self,
        binding: &TenantTableBinding,
    ) -> Result<OrphanGcReport, ForgeError> {
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
            .run_orphan_gc_for_table(
                &mut lease,
                &key,
                binding,
                OrphanGcScan {
                    now: self.core.clock.now()?,
                    age_cutoff_ms: None,
                    start_after: None,
                    task_id: None,
                },
                &CancellationToken::new(),
            )
            .await?;
        lease.release(&self.core.operator_pool).await?;
        Ok(OrphanGcReport {
            candidates: outcome.candidates,
            deleted: outcome.deleted,
            skipped: outcome.skipped,
            pending: outcome.pending,
            partial: outcome.partial,
        })
    }
}

/// Public projection of one orphan-GC run for integration assertions.
///
/// Exposes the bounded-run counters and the Partial flag across the crate
/// boundary so `test-support` consumers can assert that a page cap or run budget
/// yielded a Partial outcome and that successive runs drain the candidate set.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy)]
pub struct OrphanGcReport {
    /// Candidate paths considered across new and reconciled batches.
    pub candidates: usize,
    /// Candidate paths deleted or already absent at deletion time.
    pub deleted: usize,
    /// Candidate paths retained or deferred past the run budget.
    pub skipped: usize,
    /// Open prepared operations left pending after bounded reconciliation.
    pub pending: usize,
    /// Whether a per-run bound ended the run before its candidates were drained.
    pub partial: bool,
}

impl Forge {
    async fn run_orphan_gc_for_table_inner(
        &self,
        lease: &mut ForgeLease,
        table: &GcTableContext<'_>,
    ) -> Result<OrphanGcOutcome, ForgeError> {
        let deadline = Instant::now()
            .checked_add(self.core.config.orphan_gc_run_budget)
            .expect("orphan-GC run budget must fit the monotonic clock horizon");
        let page_cap = self.core.config.orphan_gc_max_list_pages;
        let mut outcome = self.reconcile_gc(lease, table).await?;
        let protection = self
            .load_maintenance_protection(ProtectionRequest {
                table,
                current_gc_detail: None,
                cleanup_exemption: None,
            })
            .await?;
        if protection.destructive_maintenance == DestructiveMaintenance::Blocked {
            outcome.pending = outcome.pending.saturating_add(1);
            return Ok(outcome);
        }
        let scan = self
            .list_gc_candidates(table, &protection, page_cap, deadline)
            .await?;
        outcome.partial |= scan.partial;
        outcome.candidates = outcome.candidates.saturating_add(scan.candidates.len());
        // The frontier is reported even when nothing was selected: a page of
        // only protected or unaddressable entries is completely processed work,
        // and advancing past it is exactly what stops it from being relisted on
        // every successor attempt.
        outcome.frontier = scan.frontier;
        if scan.candidates.is_empty() {
            return Ok(outcome);
        }
        let detail = Self::gc_detail(table.task_id, table.key, scan.candidates)?;
        self.append_gc_audit(lease, table.key.tenant, &detail, "forge.orphan_gc.prepared")
            .await?;
        // Past the prepared audit the operation row owns this batch. A caller
        // driving a durable task must not settle its attempt on any exit from
        // here: settling would return the task to the pool while the batch is
        // still unresolved, and a successor would list and select against an
        // open preparation. Reporting the exit as retained instead leaves the
        // attempt standing until its lease lapses, which is the one route that
        // replays this exact batch.
        let batch = match self
            .delete_gc_batch(
                lease,
                GcBatchRequest {
                    table,
                    detail: &detail,
                    recovered: false,
                    deadline: Some(deadline),
                },
            )
            .await
        {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "prepared orphan-GC batch left unresolved; retaining the attempt for reclaim"
                );
                return Err(ForgeError::ShutdownRetained);
            }
        };
        outcome.recovered = outcome.recovered.saturating_add(1);
        outcome.deleted = outcome.deleted.saturating_add(batch.deleted);
        outcome.skipped = outcome.skipped.saturating_add(batch.skipped);
        outcome.partial |= batch.deferred;
        Ok(outcome)
    }
}

#[derive(Debug, Default, Clone)]
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
    /// Last object key whose classification completed during this pass, if the
    /// scan reached one. A caller owning a durable task checkpoints exactly
    /// this key after its selected batch settles; every earlier key is proven
    /// handled and every later key is untouched.
    pub(crate) frontier: Option<String>,
    /// Whether a per-run bound (listing-page cap or wall-clock budget) ended the
    /// run before its candidate set was exhausted. A partial run committed only
    /// durable deletions and leaves the remainder for a successor run; it is not
    /// an error and does not weaken deletion safety.
    pub(crate) partial: bool,
}

/// Result of one bounded orphan-candidate scan.
///
/// Produced by [`Forge::list_gc_candidates`]. `partial` is set when the scan
/// stopped at a listing-page boundary because the per-run page cap or wall-clock
/// budget was reached rather than because the prefix was exhausted.
struct GcCandidateScan {
    /// Sorted, batch-capped eligible candidate object keys.
    candidates: Vec<String>,
    /// Whether a per-run bound stopped the scan before the prefix was exhausted.
    partial: bool,
    /// Last key whose classification completed, in listing order.
    frontier: Option<String>,
}

/// Result of completing one prepared GC batch under an optional run budget.
///
/// Produced by [`Forge::delete_gc_batch`]. `deferred` is set when the run's
/// wall-clock budget stopped the deletion loop before every candidate was
/// evaluated; the remaining candidates are recorded as skipped so the terminal
/// audit's deleted-plus-skipped partition still covers the full candidate set.
struct GcBatchResult {
    /// Candidates deleted or already absent at deletion time.
    deleted: usize,
    /// Candidates retained or deferred past the run budget.
    skipped: usize,
    /// Whether the run budget deferred remaining candidates to a successor run.
    deferred: bool,
}

/// Per-candidate partition produced by one bounded deletion loop.
///
/// Owns the deleted and skipped path lists that the terminal audit records, and
/// tracks whether the run budget deferred the unscanned remainder. The two lists
/// together always cover the full candidate set the loop was given.
#[derive(Default)]
struct GcDeletionTally {
    /// Paths deleted, or already absent, under the held fence.
    deleted: Vec<String>,
    /// Paths retained by protection or deferred past the run budget.
    skipped: Vec<String>,
    /// Whether the run budget deferred remaining candidates to a successor run.
    deferred: bool,
}

/// Build protection from every retained Iceberg object and every open workflow.
impl Forge {
    async fn load_maintenance_protection_inner(
        &self,
        request: ProtectionRequest<'_>,
    ) -> Result<MaintenanceProtection, ForgeError> {
        let table_context = request.table;
        require_running(table_context.stop)?;
        let CatalogProtection {
            live,
            table_location,
            traversed_snapshot_ids,
        } = self.load_catalog_protection(table_context.binding).await?;
        let durable = self
            .load_durable_protection_roots(&request, &table_location)
            .await?;
        require_running(table_context.stop)?;

        let composed = OrphanProtectionRoots {
            catalog: live,
            traversed_snapshot_ids,
            hot_unpromoted: durable.hot_unpromoted,
            open_outputs: durable.open_outputs,
            pinned_snapshot_ids: durable.pinned_snapshot_ids,
            blocked: durable.blocked,
        }
        .compose()?;
        let object_age_cutoff = match table_context.age_cutoff_ms {
            Some(cutoff_ms) => {
                Timestamp::from_millisecond(cutoff_ms).map_err(|_| ForgeError::InvalidConfig {
                    detail: "orphan cleanup cutoff cannot be represented by object storage"
                        .to_owned(),
                })?
            }
            None => self.gc_object_age_cutoff(table_context.now)?,
        };
        Ok(MaintenanceProtection::new(
            composed.live_set,
            composed.blocked,
            table_context.now,
            object_age_cutoff,
        ))
    }

    /// Reads every durable root class in one bounded tenant transaction.
    ///
    /// Catalog reachability is loaded separately because it needs manifest IO;
    /// everything here is Postgres evidence, and it is read under one snapshot
    /// so the classes cannot disagree about which operations were open.
    ///
    /// # Errors
    ///
    /// Returns SQL, operation-state, exemption-validation, path-normalization,
    /// or bound-overflow failures.
    async fn load_durable_protection_roots(
        &self,
        request: &ProtectionRequest<'_>,
        table_location: &str,
    ) -> Result<DurableProtectionRoots, ForgeError> {
        let key = request.table.key;
        let binding = request.table.binding;
        let normalize = |path: &str| {
            catalog_path_to_object_key(table_location, binding, &self.core.staging, path)
        };
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let mut roots = DurableProtectionRoots::default();
        let rows = list_nonterminal_file_paths(
            &mut conn,
            key.table_ref.namespace.as_str(),
            &key.table_ref.name,
            None,
        )
        .await
        .map_err(ForgeError::Sql)?;
        for path in rows {
            roots.hot_unpromoted.push(normalize(&path)?);
        }

        let resource = table_resource_for_key(key);
        let cap = self.core.config.max_open_operations_per_table;
        for family in [
            ForgeOperationFamily::IcebergRewrite,
            ForgeOperationFamily::SnapshotExpire,
        ] {
            let page = ForgeOperations::new(&resource, family)
                .map_err(ForgeError::Sql)?
                .list_open(&mut conn, cap, None)
                .await
                .map_err(ForgeError::Sql)?;
            roots.blocked |= page.overflowed || !page.operations.is_empty();
            for row in &page.operations {
                for path in operation_output_paths(&row.prepared_detail) {
                    roots.open_outputs.push(normalize(path.as_str())?);
                }
            }
        }
        let open_gc = ForgeOperations::new(&resource, ForgeOperationFamily::OrphanGc)
            .map_err(ForgeError::Sql)?
            .list_open(&mut conn, cap, None)
            .await
            .map_err(ForgeError::Sql)?;
        roots.blocked |= open_gc.overflowed;
        let exemption = current_gc_exemption(
            &resource,
            request.current_gc_detail,
            &open_gc.operations,
            |path| normalize(path.as_str()),
        )?;
        validate_gc_exemption(exemption.as_ref())?;
        for row in &open_gc.operations {
            if exemption
                .as_ref()
                .is_some_and(|value| value.operation_id == row.operation_id)
            {
                continue;
            }
            roots.blocked = true;
            for path in operation_output_paths(&row.prepared_detail) {
                roots.open_outputs.push(normalize(path.as_str())?);
            }
        }
        // An unresolved cleanup preparation names an object whose existence is
        // unknown: the delete may already have been submitted. Protecting it
        // keeps orphan collection and every other cleanup off it until the
        // owning attempt settles. Only the exact caller-named preparation is
        // exempt, so a stale or mismatched one still protects its object.
        for prepared in list_prepared_cleanup_candidates(
            &mut conn,
            BIFROST_CATALOG_NAME,
            key.table_ref.namespace.as_str(),
            &key.table_ref.name,
        )
        .await
        .map_err(ForgeError::Sql)?
        {
            if request.cleanup_exemption.is_some_and(|exemption| {
                exemption.task_id == prepared.task_id
                    && exemption.attempt_id == prepared.attempt_id
                    && exemption.index == prepared.index
                    && *exemption.candidate == prepared.candidate
            }) {
                continue;
            }
            roots
                .open_outputs
                .push(normalize(prepared.candidate.path.as_str())?);
        }
        // Every snapshot on every proven ancestry chain, not just the chain
        // endpoints: this pass protects the objects those snapshots reach, so a
        // partial chain would leave the middle of a reader's history collectable.
        roots.pinned_snapshot_ids = super::reader_protection::ReaderProtection::new(&mut conn)
            .protected_snapshot_ids(key.tenant, &key.table_ref)
            .await?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        Ok(roots)
    }

    /// Loads bounded terminal Reset outputs for test inspection of durable lineage.
    ///
    /// # Errors
    ///
    /// Returns SQL, operation-state, or table-binding failures. Overflow remains
    /// explicit so integration fixtures cannot mistake a truncated projection
    /// for the complete Reset history.
    #[cfg(feature = "test-support")]
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
        for family in [ForgeOperationFamily::IcebergRewrite] {
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
        let mut traversed_snapshot_ids = Vec::with_capacity(retained_snapshot_count);
        let mut add = |path: &str| self.add_path(&mut live, binding, &table_location, path);
        add(table
            .metadata_location_result()
            .map_err(ForgeError::Catalog)?)?;
        for metadata_log in table.metadata().metadata_log() {
            add(&metadata_log.metadata_file)?;
        }
        for snapshot in table.metadata().snapshots() {
            traversed_snapshot_ids.push(snapshot.snapshot_id());
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
            traversed_snapshot_ids,
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
    /// The scan is bounded per run: it pulls listing pages one at a time and, at
    /// each page boundary, stops once `page_cap` pages have been consumed or the
    /// `deadline` has passed, marking the returned scan `partial`. Bounds are
    /// checked before pulling each page so a run never begins work it cannot
    /// finish within budget; a run that stops early leaves the unscanned tail to
    /// a successor run. Eligibility classification and the batch cap are
    /// unchanged, so a partial scan cannot admit a candidate a full scan would
    /// have rejected.
    ///
    /// # Errors
    /// Returns object-store listing failures. Cancellation leaves objects untouched.
    async fn list_gc_candidates(
        &self,
        table: &GcTableContext<'_>,
        protection: &MaintenanceProtection,
        page_cap: usize,
        deadline: Instant,
    ) -> Result<GcCandidateScan, ForgeError> {
        let binding = table.binding;
        tracing::debug!(
            captured_now = %protection.now,
            prefix = %forge_data_prefix(binding),
            start_after = ?table.start_after,
            page_cap,
            "enumerating bounded orphan-GC candidates"
        );
        // The scan is bounded to the Forge recipe root rather than the whole
        // table prefix. Nothing outside that root is addressable by orphan
        // collection anyway, and a task-owned cursor must name a key strictly
        // beneath the prefix its plan fixes — which is exactly this root.
        let prefix = format!("{}/", forge_data_prefix(binding));
        let mut pages = self
            .core
            .object_store
            .list_pages(&prefix, table.start_after)
            .await
            .map_err(ForgeError::ObjectList)?;
        let batch_cap = self.core.config.max_gc_candidates_per_batch;
        let mut candidates = Vec::new();
        let mut frontier: Option<String> = None;
        let mut scanned_pages = 0_usize;
        let mut partial = false;
        'scan: loop {
            // Bounds are checked *after* a page is consumed so an exhausted
            // prefix is reported as complete rather than as a run that merely
            // ran out of budget. A durable task cursor turns that difference
            // into "succeeded" versus "resume forever".
            let Some(page) = pages.next().await else {
                break;
            };
            let mut entries = page.map_err(ForgeError::ObjectList)?;
            scanned_pages = scanned_pages.saturating_add(1);
            // Key order is what makes the frontier meaningful, and a backend is
            // only required to page — not to order within a page.
            entries.sort_unstable_by(|left, right| left.path().cmp(right.path()));
            for entry in entries {
                if candidates.len() >= batch_cap {
                    // Stopping *before* this entry is what keeps the frontier
                    // honest: the cursor must never move past work no batch
                    // considered.
                    partial = true;
                    break 'scan;
                }
                let path = entry.path().to_owned();
                if protection.gc_eligibility(
                    binding,
                    &path,
                    ObjectEvidence::Present(entry.metadata()),
                ) == GcEligibility::Eligible
                {
                    candidates.push(path.clone());
                }
                frontier = Some(path);
            }
            if scanned_pages >= page_cap || Instant::now() >= deadline {
                partial = true;
                break;
            }
        }
        candidates.sort_unstable();
        Ok(GcCandidateScan {
            candidates,
            partial,
            frontier,
        })
    }

    /// Fences, rechecks, and deletes each candidate under a single protection snapshot.
    ///
    /// The `protection` snapshot is loaded once by the caller and shared across
    /// every candidate; this method re-fences the lease and re-stats each object
    /// immediately before deletion so the once-per-batch snapshot is still gated
    /// by a per-candidate truth-table check. When `deadline` is `Some` the loop
    /// yields at a candidate boundary once the run budget is reached, recording
    /// the unscanned remainder as skipped and setting `deferred`, so the returned
    /// deleted-plus-skipped partition always covers the full candidate set and no
    /// deletion is attempted past the budget.
    ///
    /// # Errors
    /// Returns cancellation, lease-fence, or object-store failures. A candidate
    /// whose path escapes the table binding fails closed as a reconciliation
    /// error before any deletion.
    async fn apply_gc_deletions(
        &self,
        lease: &mut ForgeLease,
        table: &GcTableContext<'_>,
        protection: &MaintenanceProtection,
        candidate_paths: &[StoragePath],
        deadline: Option<Instant>,
    ) -> Result<GcDeletionTally, ForgeError> {
        let mut tally = GcDeletionTally::default();
        for path in candidate_paths {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                tally.skipped.push(path.as_str().to_owned());
                tally.deferred = true;
                continue;
            }
            require_running(table.stop)?;
            lease.require_fence(&self.core.operator_pool).await?;
            let normalized = table
                .binding
                .validate_object_path(path.as_str())
                .ok_or_else(|| ForgeError::Reconciliation {
                    detail: format!("orphan-GC candidate escaped table binding: {path:?}"),
                })?;
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
                tally.deleted.push(path.as_str().to_owned());
                continue;
            }
            if eligibility != GcEligibility::Eligible {
                tally.skipped.push(path.as_str().to_owned());
                continue;
            }
            require_running(table.stop)?;
            lease.require_fence(&self.core.operator_pool).await?;
            match self.core.object_store.delete(&normalized).await {
                Ok(()) => {
                    // Counted here, not from the tally: the tally also records
                    // an object proven missing before the delete and an
                    // idempotent `NotFound`, neither of which is a deletion
                    // this fence performed.
                    ForgeTelemetry::record_deleted_objects(ForgeTaskStrategy::OrphanCleanup, 1);
                    tally.deleted.push(path.as_str().to_owned());
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    tally.deleted.push(path.as_str().to_owned());
                }
                Err(error) => return Err(ForgeError::ObjectDelete(error)),
            }
        }
        Ok(tally)
    }

    /// Deletes a prepared candidate batch with a final fence and reference check.
    ///
    /// The maintenance-protection proof is loaded once, before the deletion
    /// loop, rather than per candidate. This batch holds the table's exclusive
    /// lease fence for its whole duration, so no other writer can add a
    /// protection to this table's catalog or open-operation state mid-batch; the
    /// orphan TTL floor means a candidate cannot become live-referenced without
    /// a lease-holding catalog commit that this fence excludes; and any file
    /// staged after the candidate set was listed is not in that set. The only
    /// admissible drift is therefore in the safe direction — a path this batch
    /// treats as an orphan being newly protected — and the per-candidate fence
    /// and metadata rechecks below still gate every deletion. A once-per-batch
    /// snapshot under the held fence is thus equivalent in safety to a
    /// per-candidate reload.
    ///
    /// When `request.deadline` is `Some`, the loop yields at a candidate
    /// boundary once the run budget is reached: remaining candidates are
    /// recorded as skipped and `deferred` is set, so the terminal audit's
    /// deleted-plus-skipped partition still covers the full candidate set and no
    /// deletion is attempted past the budget.
    ///
    /// # Errors
    /// Returns lease, catalog, object-store, SQL, or audit failures.
    async fn delete_gc_batch(
        &self,
        lease: &mut ForgeLease,
        request: GcBatchRequest<'_>,
    ) -> Result<GcBatchResult, ForgeError> {
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
        let protection = self
            .load_maintenance_protection(ProtectionRequest {
                table,
                current_gc_detail: Some(detail),
                cleanup_exemption: None,
            })
            .await?;
        let GcDeletionTally {
            deleted,
            skipped,
            deferred,
        } = self
            .apply_gc_deletions(lease, table, &protection, candidate_paths, request.deadline)
            .await?;
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
        Ok(GcBatchResult {
            deleted: deleted_count,
            skipped: skipped_count,
            deferred,
        })
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
            .list_open(
                &mut conn,
                self.core.config.max_open_operations_per_table,
                None,
            )
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
            let batch = self
                .delete_gc_batch(
                    lease,
                    GcBatchRequest {
                        table,
                        detail: &detail,
                        recovered: true,
                        deadline: None,
                    },
                )
                .await?;
            outcome.candidates = outcome.candidates.saturating_add(candidate_paths.len());
            outcome.recovered = outcome.recovered.saturating_add(1);
            outcome.deleted = outcome.deleted.saturating_add(batch.deleted);
            outcome.skipped = outcome.skipped.saturating_add(batch.skipped);
        }
        Ok(outcome)
    }

    /// Builds the canonical prepared audit detail for one orphan batch.
    ///
    /// `task_id` seeds the batch identity when a durable task owns the run;
    /// `None` keeps the table-seeded identity used by direct maintenance.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::LiveSet`] when a candidate is not a valid
    /// storage path.
    fn gc_detail(
        task_id: Option<Uuid>,
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
            operation_id: Self::gc_operation_id(task_id, key, &candidates),
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

    /// Derives the stable batch identity for one sorted candidate set.
    ///
    /// The digest is seeded with the owning task's UUID when a task drives the
    /// run and with the table resource otherwise, then absorbs every candidate
    /// path with the existing zero delimiter.
    fn gc_operation_id(task_id: Option<Uuid>, key: &ForgeTableKey, candidates: &[String]) -> Uuid {
        let mut hasher = Sha256::new();
        match task_id {
            Some(task_id) => hasher.update(task_id.as_bytes()),
            None => hasher.update(table_resource_for_key(key)),
        }
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
    use crate::forge::protection_roots::OrphanProtectionRoots;

    /// A task-driven batch keeps one identity across retry and takeover, stays
    /// distinct from another task selecting the same paths, and leaves the
    /// non-task maintenance identity table-seeded.
    #[test]
    fn task_scoped_gc_operation_identity_is_stable_across_takeover() {
        let key = ForgeTableKey {
            tenant: DataTenantId::new_v7(),
            table_ref: crate::catalog::TableRef::new(
                crate::namespaces::BifrostNamespace::Traces,
                "spans",
            ),
        };
        let first_task = Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111);
        let second_task = Uuid::from_u128(0x2222_2222_2222_2222_2222_2222_2222_2222);
        let forward = vec![
            "tenants/t/traces/spans/data/a.parquet".to_owned(),
            "tenants/t/traces/spans/data/b.parquet".to_owned(),
        ];
        let reversed = vec![
            "tenants/t/traces/spans/data/b.parquet".to_owned(),
            "tenants/t/traces/spans/data/a.parquet".to_owned(),
        ];
        let identity = |task_id: Option<Uuid>, candidates: Vec<String>| match Forge::gc_detail(
            task_id, &key, candidates,
        )
        .expect("candidate paths are valid storage paths")
        {
            AuditDetail::ForgeOrphanGc { operation_id, .. } => operation_id,
            other => panic!("orphan GC detail expected, got {other:?}"),
        };

        let claimed = identity(Some(first_task), forward.clone());
        assert_eq!(
            claimed,
            identity(Some(first_task), reversed.clone()),
            "retry and takeover of one task must reuse its batch identity"
        );
        assert_ne!(
            claimed,
            identity(Some(second_task), forward.clone()),
            "another periodic task selecting the same paths must be distinct"
        );
        let table_seeded = identity(None, forward.clone());
        assert_eq!(table_seeded, identity(None, reversed));
        assert_ne!(table_seeded, claimed);
        assert_eq!(
            table_seeded,
            Forge::gc_operation_id(None, &key, &forward),
            "the non-task maintenance identity stays table-seeded"
        );
    }

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
        let forge_path = |attempt: Uuid| {
            format!(
                "{}{FORGE_DATA_MARKER}{FORGE_WRITER_RECIPE}/{attempt}-00000-{}.parquet",
                binding.object_prefix,
                Uuid::now_v7()
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
            GcEligibility::Eligible,
            "an aged deterministic pre-Prepared generation is reclaimable after refreshed protection"
        );
        assert_eq!(
            protection.gc_eligibility(
                &binding,
                &protected_path,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Protected
        );
        let blocked = MaintenanceProtection {
            live_set: ProtectedLiveSet::default(),
            destructive_maintenance: DestructiveMaintenance::Blocked,
            now: protection.now,
            object_age_cutoff: cutoff,
        };
        assert_eq!(
            blocked.gc_eligibility(
                &binding,
                &unproven_path,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Protected,
            "a nonterminal operation blocks an otherwise eligible pre-Prepared generation"
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

    /// Expired-file deletion needs every proof, and never the candidate list alone.
    ///
    /// The prepared candidate set is derived once, before the first delete, and
    /// may be drained much later by a successor owner. Each case removes one
    /// proof the durable set cannot supply on its own — the tenant/table
    /// binding, the destructive-maintenance gate, fresh unreachability from
    /// every retained head, the object age floor, and the object's own
    /// existence and kind — and requires the deletion to be refused.
    #[test]
    fn forge_expired_cleanup_eligibility_matrix() {
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
        let expired_manifest = format!("{}/metadata/snap-expired.avro", binding.object_prefix);
        let rereferenced = format!("{}/metadata/snap-retained.avro", binding.object_prefix);
        let young_manifest = format!("{}/metadata/snap-young.avro", binding.object_prefix);
        let mut live = ProtectedLiveSet::default();
        live.insert(rereferenced.clone());
        let now = DateTime::<Utc>::from_timestamp_millis(48 * 60 * 60 * 1_000).expect("time");
        let protection = MaintenanceProtection {
            live_set: live,
            destructive_maintenance: DestructiveMaintenance::Allowed,
            now,
            object_age_cutoff: cutoff,
        };
        let old_metadata = opendal::Metadata::new(EntryMode::FILE).with_last_modified(old);
        let young_metadata = opendal::Metadata::new(EntryMode::FILE).with_last_modified(young);
        let directory = opendal::Metadata::new(EntryMode::DIR);

        assert_eq!(
            protection.expired_cleanup_eligibility(
                &binding,
                &expired_manifest,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Eligible,
            "an aged, unreachable expiry candidate is not a Forge attempt generation"
        );
        assert_eq!(
            protection.expired_cleanup_eligibility(
                &binding,
                &rereferenced,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Protected,
            "a candidate a retained head still reaches must survive its own prepared set"
        );
        let blocked = MaintenanceProtection {
            live_set: ProtectedLiveSet::default(),
            destructive_maintenance: DestructiveMaintenance::Blocked,
            now,
            object_age_cutoff: cutoff,
        };
        assert_eq!(
            blocked.expired_cleanup_eligibility(
                &binding,
                &expired_manifest,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::Protected,
            "an open or unreconciled operation blocks the prepared set"
        );
        assert_eq!(
            protection.expired_cleanup_eligibility(
                &binding,
                &young_manifest,
                ObjectEvidence::Present(&young_metadata)
            ),
            GcEligibility::TooYoung
        );
        assert_eq!(
            protection.expired_cleanup_eligibility(
                &binding,
                "other-tenant/spans/metadata/snap-expired.avro",
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::InvalidPath,
            "a candidate outside the tenant table binding is never deletable"
        );
        assert_eq!(
            protection.expired_cleanup_eligibility(
                &binding,
                &expired_manifest,
                ObjectEvidence::Present(&directory)
            ),
            GcEligibility::NotFile
        );
        assert_eq!(
            protection.expired_cleanup_eligibility(
                &binding,
                &expired_manifest,
                ObjectEvidence::Missing
            ),
            GcEligibility::Missing
        );
        assert_eq!(
            protection.gc_eligibility(
                &binding,
                &expired_manifest,
                ObjectEvidence::Present(&old_metadata)
            ),
            GcEligibility::InvalidPath,
            "the orphan collector still refuses every non-attempt-generation path"
        );
    }

    /// Object identities shared by every case of the root-mutation matrix.
    ///
    /// Held together because each root class is only meaningful next to the
    /// object it alone protects: the assertions below all read as "drop this
    /// root, watch that object become reachable".
    struct OrphanMatrixPaths {
        /// Validated physical binding every case classifies against.
        binding: TenantTableBinding,
        /// Output only catalog reachability protects.
        catalog_output: String,
        /// Output only the open-attempt root protects.
        open_output: String,
        /// Committed Scribe object only the `file_list` root protects.
        hot_scribe: String,
        /// Aged output no authority names, which is what this protocol deletes.
        orphan: String,
    }

    impl OrphanMatrixPaths {
        /// Builds one binding and the four canonical objects the matrix uses.
        fn new() -> Self {
            use crate::catalog::TableRef;
            use crate::namespaces::BifrostNamespace;

            let binding = TenantTableBinding::resolve((
                DataTenantId::new_v7(),
                TableRef::new(BifrostNamespace::Traces, "spans"),
            ))
            .expect("binding");
            let output = |prefix: &str| {
                format!(
                    "{prefix}{FORGE_DATA_MARKER}{FORGE_WRITER_RECIPE}/{}-00000-{}.parquet",
                    Uuid::now_v7(),
                    Uuid::now_v7()
                )
            };
            Self {
                catalog_output: output(&binding.object_prefix),
                open_output: output(&binding.object_prefix),
                orphan: output(&binding.object_prefix),
                hot_scribe: format!("{}/data/pod-a-01JHOT.parquet", binding.object_prefix),
                binding,
            }
        }

        /// Rebuilds the complete root set every case starts from.
        fn roots(&self) -> OrphanProtectionRoots {
            OrphanProtectionRoots {
                catalog: {
                    let mut set = ProtectedLiveSet::default();
                    set.insert(self.catalog_output.clone());
                    set
                },
                traversed_snapshot_ids: vec![10, 20],
                hot_unpromoted: vec![self.hot_scribe.clone()],
                open_outputs: vec![self.open_output.clone()],
                pinned_snapshot_ids: vec![20],
                blocked: false,
            }
        }
    }

    /// The fixed age cutoff and evaluation time every matrix case shares.
    fn orphan_matrix_protection(roots: OrphanProtectionRoots) -> MaintenanceProtection {
        let composed = roots.compose().expect("roots compose");
        MaintenanceProtection::new(
            composed.live_set,
            composed.blocked,
            DateTime::<Utc>::from_timestamp_millis(48 * 60 * 60 * 1_000).expect("time"),
            Timestamp::from_millisecond(24 * 60 * 60 * 1_000).expect("timestamp"),
        )
    }

    /// Object metadata older than the matrix cutoff.
    fn orphan_matrix_aged() -> opendal::Metadata {
        opendal::Metadata::new(EntryMode::FILE)
            .with_last_modified(Timestamp::from_millisecond(0).expect("timestamp"))
    }

    /// Observable consequence of removing exactly one protection root.
    ///
    /// Each root defends its object differently, so "load-bearing" cannot be a
    /// single assertion: dropping catalog reachability or the open-output root
    /// flips an object to eligible, dropping the `file_list` root removes it
    /// from the protected union without changing any verdict, and breaking the
    /// lineage or reader-pin agreement must refuse to compose at all.
    #[derive(Debug, Clone, Copy)]
    enum RootLoss {
        /// The subject becomes deletable, which is the data-loss case.
        SubjectBecomesEligible,
        /// The subject leaves the protected union without a verdict change.
        SubjectLeavesLiveSet,
        /// The root set no longer composes, so the pass refuses.
        CompositionFailsClosed,
    }

    /// One independent protection root, its subject, and its removal.
    struct RootCase {
        /// Root being removed, named as the authority it represents.
        authority: &'static str,
        /// Removes exactly this one root from an otherwise complete set.
        remove: fn(&mut OrphanProtectionRoots),
        /// Object this root alone defends, taken from the shared paths.
        subject: fn(&OrphanMatrixPaths) -> &String,
        /// What removing this root must make observable.
        loss: RootLoss,
    }

    /// Every protection root, one row each, with the loss its removal causes.
    ///
    /// A root added to [`OrphanProtectionRoots`] without a row here is a root
    /// nothing proves is load-bearing, so the table is the checklist.
    fn orphan_root_cases() -> Vec<RootCase> {
        vec![
            RootCase {
                authority: "catalog reachability",
                remove: |roots| roots.catalog = ProtectedLiveSet::default(),
                subject: |paths| &paths.catalog_output,
                loss: RootLoss::SubjectBecomesEligible,
            },
            RootCase {
                authority: "the staged/prepared/open/possible/uncertain output root",
                remove: |roots| roots.open_outputs.clear(),
                subject: |paths| &paths.open_output,
                loss: RootLoss::SubjectBecomesEligible,
            },
            RootCase {
                // Scribe pod names are outside the recipe grammar, so the
                // observable loss here is union membership, not a verdict flip.
                authority: "committed-but-unpromoted file_list objects",
                remove: |roots| roots.hot_unpromoted.clear(),
                subject: |paths| &paths.hot_scribe,
                loss: RootLoss::SubjectLeavesLiveSet,
            },
            RootCase {
                authority: "an Oracle reader pin inside the proven lineage",
                remove: |roots| roots.pinned_snapshot_ids = vec![30],
                subject: |paths| &paths.orphan,
                loss: RootLoss::CompositionFailsClosed,
            },
            RootCase {
                authority: "the traversed snapshot lineage",
                remove: |roots| roots.traversed_snapshot_ids.clear(),
                subject: |paths| &paths.orphan,
                loss: RootLoss::CompositionFailsClosed,
            },
        ]
    }

    /// Requires each protection root to be the only thing protecting its object.
    ///
    /// A root that can be dropped with no observable consequence is a root that
    /// is no longer protecting anything, so each case here must either expose an
    /// object as unsafely eligible, remove it from the protected union, or fail
    /// closed. Every case first asserts the complete root set does protect its
    /// subject, so a removal can never pass because the subject was unprotected
    /// to begin with.
    fn assert_each_orphan_root_is_load_bearing(paths: &OrphanMatrixPaths) {
        let aged = orphan_matrix_aged();
        for case in orphan_root_cases() {
            let subject = (case.subject)(paths);
            let authority = case.authority;
            let complete = orphan_matrix_protection(paths.roots());
            match case.loss {
                RootLoss::SubjectBecomesEligible => assert_eq!(
                    complete.gc_eligibility(
                        &paths.binding,
                        subject,
                        ObjectEvidence::Present(&aged)
                    ),
                    GcEligibility::Protected,
                    "{authority} protects its subject before it is removed"
                ),
                RootLoss::SubjectLeavesLiveSet => assert!(
                    complete.live_set.contains(subject),
                    "{authority} names its subject before it is removed"
                ),
                RootLoss::CompositionFailsClosed => {
                    assert!(
                        paths.roots().compose().is_ok(),
                        "{authority} composes before it is broken"
                    );
                }
            }

            let mut mutated = paths.roots();
            (case.remove)(&mut mutated);
            match case.loss {
                RootLoss::SubjectBecomesEligible => assert_eq!(
                    orphan_matrix_protection(mutated).gc_eligibility(
                        &paths.binding,
                        subject,
                        ObjectEvidence::Present(&aged)
                    ),
                    GcEligibility::Eligible,
                    "dropping {authority} exposes the only object it protected"
                ),
                RootLoss::SubjectLeavesLiveSet => assert!(
                    !orphan_matrix_protection(mutated).live_set.contains(subject),
                    "dropping {authority} removes the only authority naming its subject"
                ),
                RootLoss::CompositionFailsClosed => assert!(
                    mutated.compose().is_err(),
                    "breaking {authority} must refuse to compose rather than proceed"
                ),
            }
        }

        // The destructive-maintenance gate is not a per-object root: lease loss,
        // fence loss, and any open operation raise one gate over the whole
        // table, so its subject is the otherwise-eligible orphan.
        let mut blocked = paths.roots();
        blocked.blocked = true;
        assert_eq!(
            orphan_matrix_protection(blocked).gc_eligibility(
                &paths.binding,
                &paths.orphan,
                ObjectEvidence::Present(&aged)
            ),
            GcEligibility::Protected,
            "lease loss, fence loss, and open operations all raise one gate over the table"
        );
    }

    /// Requires the tenant binding, age floor, and object evidence to refuse
    /// independently of every protection root.
    fn assert_orphan_binding_age_and_evidence_refuse(paths: &OrphanMatrixPaths) {
        use crate::catalog::TableRef;
        use crate::namespaces::BifrostNamespace;

        let protection = orphan_matrix_protection(paths.roots());
        let aged = orphan_matrix_aged();
        let foreign = TenantTableBinding::resolve((
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("foreign binding");
        assert_eq!(
            protection.gc_eligibility(&foreign, &paths.orphan, ObjectEvidence::Present(&aged)),
            GcEligibility::InvalidPath,
            "another tenant's binding never addresses this table's objects"
        );
        let young = opendal::Metadata::new(EntryMode::FILE).with_last_modified(
            Timestamp::from_millisecond(47 * 60 * 60 * 1_000).expect("timestamp"),
        );
        assert_eq!(
            protection.gc_eligibility(
                &paths.binding,
                &paths.orphan,
                ObjectEvidence::Present(&young)
            ),
            GcEligibility::TooYoung
        );
        assert_eq!(
            protection.gc_eligibility(
                &paths.binding,
                &paths.orphan,
                ObjectEvidence::Present(&opendal::Metadata::new(EntryMode::FILE))
            ),
            GcEligibility::TooYoung,
            "an object with no last-modified evidence has not proven its age"
        );
        assert_eq!(
            protection.gc_eligibility(&paths.binding, &paths.orphan, ObjectEvidence::Missing),
            GcEligibility::Missing
        );
    }

    /// Requires only the writer's own recipe grammar to be addressable at all.
    fn assert_only_canonical_recipe_paths_are_addressable(paths: &OrphanMatrixPaths) {
        let protection = orphan_matrix_protection(paths.roots());
        let aged = orphan_matrix_aged();
        let prefix = &paths.binding.object_prefix;
        for (lookalike, why) in [
            (
                format!(
                    "{prefix}{FORGE_DATA_MARKER}{FORGE_WRITER_RECIPE}/{}-00000.parquet",
                    Uuid::now_v7()
                ),
                "an output with no per-writer identity is not a recipe output",
            ),
            (
                format!(
                    "{prefix}{FORGE_DATA_MARKER}v2/{}-00000-{}.parquet",
                    Uuid::now_v7(),
                    Uuid::now_v7()
                ),
                "another recipe root is outside this collector's reach",
            ),
            (
                format!("{prefix}/data/pod-a-01JABC.parquet"),
                "a Scribe pod object is reclaimable only from committed expiry evidence",
            ),
            (
                format!("{prefix}/metadata/v1.metadata.json"),
                "catalog-owned metadata is never reached by orphan listing alone",
            ),
        ] {
            assert_eq!(
                protection.gc_eligibility(
                    &paths.binding,
                    &lookalike,
                    ObjectEvidence::Present(&aged)
                ),
                GcEligibility::InvalidPath,
                "{why}"
            );
        }
    }

    /// Every orphan-protection root is load-bearing on its own.
    ///
    /// Never-published orphan collection deletes objects no catalog snapshot
    /// names, so the only thing standing between a bounded listing and data
    /// loss is the completeness of the protected union and the closed
    /// eligibility truth applied over it. This removes one authority at a time
    /// — catalog reachability, committed-but-unpromoted `file_list` objects,
    /// the staged/prepared/open/possible/uncertain output root, snapshot
    /// lineage, an Oracle reader pin, the tenant/table binding, the
    /// destructive-maintenance gate that lease loss and fence loss raise, and
    /// the object age floor — and requires each removal to either expose an
    /// object as unsafely eligible or fail closed.
    #[test]
    fn orphan_policy_root_mutation_matrix() {
        let paths = OrphanMatrixPaths::new();
        let complete = orphan_matrix_protection(paths.roots());
        let aged = orphan_matrix_aged();

        assert_eq!(
            complete.gc_eligibility(
                &paths.binding,
                &paths.orphan,
                ObjectEvidence::Present(&aged)
            ),
            GcEligibility::Eligible,
            "an aged output no authority names is exactly what this protocol collects"
        );
        for (protected, why) in [
            (
                &paths.catalog_output,
                "a catalog-reachable output is never an orphan",
            ),
            (
                &paths.open_output,
                "a staged, prepared, open, possible, or uncertain output is still owned",
            ),
        ] {
            assert_eq!(
                complete.gc_eligibility(&paths.binding, protected, ObjectEvidence::Present(&aged)),
                GcEligibility::Protected,
                "{why}"
            );
        }
        assert!(
            complete.live_set.contains(&paths.hot_scribe),
            "a committed but unpromoted Scribe object is inside the protected union"
        );

        assert_each_orphan_root_is_load_bearing(&paths);
        assert_orphan_binding_age_and_evidence_refuse(&paths);
        assert_only_canonical_recipe_paths_are_addressable(&paths);
    }
}

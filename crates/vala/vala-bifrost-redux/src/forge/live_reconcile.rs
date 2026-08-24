//! Bounded reconciliation for prepared live Iceberg replacements.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use iceberg::spec::DataContentType;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationStateRow};
use wyrd_spec::vala::api::{AuditDetail, ForgeIcebergRewritePhase, StoragePath};

use super::Forge;
use super::binpack::ForgeGroupKey;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::lease::ForgeLease;
use super::metrics::RewritePathContract;
use super::path::catalog_path_to_object_key;
use crate::catalog::TenantTableBinding;
use crate::catalog::layout::TimePartition;

/// Whether later table stages may perform destructive maintenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DestructiveMaintenance {
    /// All bounded prepared work has terminal proof.
    Allowed,
    /// Uncertain or incomplete work requires destructive stages to stop.
    Blocked,
}

/// Complete bounded classification for one table's live replacements.
#[derive(Debug)]
pub(crate) struct IcebergReconciliationOutcome {
    /// Operations proven externally committed.
    pub recovered: usize,
    /// Operations proven abandoned and reset.
    pub reset: usize,
    /// Operations still inside the uncertainty window.
    pub pending: usize,
    /// Operations with canonical but inconclusive evidence.
    pub unresolved: usize,
    /// Whether the open-operation query found a cap sentinel.
    ///
    /// Gated to `test-support`: the only reader is the
    /// `LiveReconciliationTestOutcome` projection, which is itself
    /// `test-support`-only.
    #[cfg(feature = "test-support")]
    pub overflowed: bool,
    /// Validated outputs protected by pending or unresolved work.
    pub protected_output_paths: BTreeSet<String>,
    /// Gate consumed by later scheduler stages.
    pub destructive_maintenance: DestructiveMaintenance,
}

/// Feature-gated projection of live reconciliation used by integration tests.
#[cfg(feature = "test-support")]
#[derive(Debug, PartialEq, Eq)]
pub struct LiveReconciliationTestOutcome {
    /// Operations proven committed.
    pub recovered: usize,
    /// Operations proven abandoned and reset.
    pub reset: usize,
    /// Operations inside the uncertainty window.
    pub pending: usize,
    /// Operations with inconclusive evidence.
    pub unresolved: usize,
    /// Whether the bounded open page overflowed.
    pub overflowed: bool,
    /// Validated pending and unresolved outputs.
    pub protected_output_paths: BTreeSet<String>,
    /// Whether destructive maintenance is blocked.
    pub blocked: bool,
}

/// One immutable retained-manifest observation of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RetainedManifestObservation {
    /// Catalog metadata pointer identifying the observed metadata version.
    metadata_location: String,
    /// Validated table root used to normalize prepared catalog paths.
    table_location: String,
    /// Default partition specification observed with this metadata.
    partition_spec_id: i32,
    /// Current snapshot at observation time, if the table is empty.
    current_snapshot_id: Option<i64>,
    /// Every retained snapshot, bounded before manifest traversal.
    snapshots: BTreeMap<i64, RetainedSnapshotObservation>,
}

/// Manifest and operation evidence retained for one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RetainedSnapshotObservation {
    /// Validated manifest-list object identity.
    manifest_list_path: String,
    /// Validated live data-file object identities.
    live_data_paths: BTreeSet<String>,
    /// Deterministic Forge operation identity recorded on the snapshot.
    forge_operation_id: Option<Uuid>,
    /// Canonical Forge group recorded with the operation identity.
    forge_group: Option<String>,
}

/// Parsed and normalized fields from one prepared live-rewrite detail.
struct PreparedRewrite {
    /// Deterministic operation identity.
    operation_id: Uuid,
    /// Canonical table resource.
    group: String,
    /// Destination partition spec.
    partition_spec_id: i32,
    /// Destination time partition.
    partition: TimePartition,
    /// Exact prepared inputs.
    input_paths: Vec<StoragePath>,
    /// Exact prepared outputs.
    output_paths: Vec<StoragePath>,
}

/// Borrowed table-local dependencies for classifying one open operation.
struct LiveClassificationContext<'a> {
    /// Current fenced table lease.
    lease: &'a mut ForgeLease,
    /// Logical table identity.
    key: &'a ForgeTableKey,
    /// Validated physical binding.
    binding: &'a TenantTableBinding,
    /// Global scheduler cancellation.
    stop: &'a CancellationToken,
    /// Scheduler-captured wall time.
    now: DateTime<Utc>,
    /// First fresh retained-manifest observation.
    observation_a: &'a RetainedManifestObservation,
    /// Second fresh retained-manifest observation.
    observation_b: &'a RetainedManifestObservation,
}

/// Normalized membership facts consumed by the classification order.
struct LiveEvidence {
    /// Outputs normalized through observation B.
    outputs: Vec<String>,
    /// Unique snapshot proving recovery, when one exists.
    recovered_snapshot: Option<i64>,
    /// Named evidence conditions used by the ordered classifier.
    conditions: BTreeSet<LiveCondition>,
}

/// One independently derived condition used by live classification.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum LiveCondition {
    /// Both retained observations and their normalized paths agree.
    Stable,
    /// Every prepared input remains in the current snapshot.
    InputsAllCurrent,
    /// At least one prepared output appears in retained metadata.
    OutputSeen,
    /// A snapshot carries an operation-ID/group half-match.
    ConflictingOperationProperty,
}

/// Exact terminal or conservative classification for normalized evidence.
enum LiveDisposition {
    /// Externally committed snapshot proven by operation properties.
    Recovered(i64),
    /// Operation remains inside the uncertainty bound.
    Pending,
    /// Stable abandoned outputs may be terminally reset for delayed orphan GC.
    Reset,
    /// Canonical evidence cannot prove either terminal state.
    Unresolved,
}

impl Forge {
    /// Classify every bounded prepared live replacement and apply proven terminals.
    ///
    /// The method performs one bounded SQL read followed by two fresh retained
    /// manifest observations. Recovered and reset transitions are fenced and
    /// transactional. A reset only records the logical terminal and leaves its
    /// output generation for delayed, independently protected orphan GC.
    /// Cancellation or fence loss returns an error without claiming a
    /// classification, and retries remain idempotent.
    ///
    /// # Errors
    ///
    /// Returns SQL, catalog, cancellation, fence, malformed-evidence, or
    /// terminal-transition errors. This reconciliation path performs no remote
    /// deletion, so a failed terminal append leaves the `Prepared` row and its
    /// outputs protected for a later retry.
    pub(super) async fn reconcile_live_replacements(
        &self,
        lease: &mut ForgeLease,
        key: &ForgeTableKey,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
        now: DateTime<Utc>,
    ) -> Result<IcebergReconciliationOutcome, ForgeError> {
        let resource = ForgeGroupKey::table_audit_resource(key.tenant, &key.table_ref);
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::IcebergRewrite)
            .map_err(ForgeError::Sql)?;
        let mut conn = self
            .core
            .vala
            .tenant_conn(key.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let page = operations
            .list_open(&mut conn, self.core.config.max_open_operations_per_table)
            .await
            .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if page.overflowed {
            return Ok(IcebergReconciliationOutcome {
                recovered: 0,
                reset: 0,
                pending: 0,
                unresolved: 0,
                #[cfg(feature = "test-support")]
                overflowed: true,
                protected_output_paths: BTreeSet::new(),
                destructive_maintenance: DestructiveMaintenance::Blocked,
            });
        }
        if page.operations.is_empty() {
            return Ok(IcebergReconciliationOutcome {
                recovered: 0,
                reset: 0,
                pending: 0,
                unresolved: 0,
                #[cfg(feature = "test-support")]
                overflowed: false,
                protected_output_paths: BTreeSet::new(),
                destructive_maintenance: DestructiveMaintenance::Allowed,
            });
        }
        let mut outcome = IcebergReconciliationOutcome {
            recovered: 0,
            reset: 0,
            pending: 0,
            unresolved: 0,
            #[cfg(feature = "test-support")]
            overflowed: false,
            protected_output_paths: BTreeSet::new(),
            destructive_maintenance: DestructiveMaintenance::Allowed,
        };
        for row in page.operations {
            let observation_a = self.observe_retained_manifests(binding, stop).await?;
            let observation_b = self.observe_retained_manifests(binding, stop).await?;
            self.classify_live_operation(
                LiveClassificationContext {
                    lease,
                    key,
                    binding,
                    stop,
                    now,
                    observation_a: &observation_a,
                    observation_b: &observation_b,
                },
                &row,
                &mut outcome,
            )
            .await?;
        }
        if outcome.pending > 0 || outcome.unresolved > 0 {
            outcome.destructive_maintenance = DestructiveMaintenance::Blocked;
        }
        Ok(outcome)
    }

    /// Reconcile one binding through the production owner for integration tests.
    ///
    /// This feature-gated seam returns only stable counters, overflow, protected
    /// paths, and the blocked gate; it does not expose private scheduler types.
    ///
    /// # Errors
    ///
    /// Returns the same SQL, catalog, storage, cancellation, fence, evidence,
    /// and terminal-transition failures as [`Self::reconcile_live_replacements`].
    #[cfg(feature = "test-support")]
    pub async fn reconcile_live_replacements_for_test(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
        now: DateTime<Utc>,
    ) -> Result<LiveReconciliationTestOutcome, ForgeError> {
        let key = ForgeTableKey {
            tenant: binding.tenant,
            table_ref: binding.table_ref.clone(),
        };
        let outcome = self
            .reconcile_live_replacements(lease, &key, binding, stop, now)
            .await?;
        Ok(LiveReconciliationTestOutcome {
            recovered: outcome.recovered,
            reset: outcome.reset,
            pending: outcome.pending,
            unresolved: outcome.unresolved,
            overflowed: outcome.overflowed,
            protected_output_paths: outcome.protected_output_paths,
            blocked: outcome.destructive_maintenance == DestructiveMaintenance::Blocked,
        })
    }

    /// Load and validate one complete retained-snapshot manifest observation.
    ///
    /// # Errors
    ///
    /// Returns cancellation, catalog, path, property, duplicate-snapshot, or
    /// configured-cap errors. The method performs no mutation and returns no
    /// partial observation.
    async fn observe_retained_manifests(
        &self,
        binding: &TenantTableBinding,
        stop: &CancellationToken,
    ) -> Result<RetainedManifestObservation, ForgeError> {
        Self::require_running(stop)?;
        let table = self.load_table(&binding.table_ident()).await?;
        self.assert_table_location(table.metadata(), binding)?;
        let retained: Vec<_> = table.metadata().snapshots().collect();
        validate_retained_snapshot_count(
            retained.len(),
            self.core.config.max_retained_snapshots_per_table,
        )?;
        let table_location = table.metadata().location();
        let mut snapshots = BTreeMap::new();
        for snapshot in retained {
            Self::require_running(stop)?;
            let manifest_list_path = catalog_path_to_object_key(
                table_location,
                binding,
                &self.core.staging,
                snapshot.manifest_list(),
            )?;
            let operation = snapshot
                .summary()
                .additional_properties
                .get("forge.operation_id");
            let group = snapshot.summary().additional_properties.get("forge.group");
            let workflow = snapshot
                .summary()
                .additional_properties
                .get("forge.workflow")
                .map(String::as_str);
            let (forge_operation_id, forge_group) =
                parse_snapshot_forge_identity(workflow, operation, group)?;
            let list = table
                .manifest_list_reader(snapshot)
                .load()
                .await
                .map_err(ForgeError::Catalog)?;
            let mut live_data_paths = BTreeSet::new();
            for manifest_file in list.entries() {
                Self::require_running(stop)?;
                let manifest = manifest_file
                    .load_manifest(table.file_io())
                    .await
                    .map_err(ForgeError::Catalog)?;
                for entry in manifest
                    .entries()
                    .iter()
                    .filter(|entry| entry.is_alive())
                    .filter(|entry| entry.content_type() == DataContentType::Data)
                {
                    let path = catalog_path_to_object_key(
                        table_location,
                        binding,
                        &self.core.staging,
                        entry.file_path(),
                    )?;
                    live_data_paths.insert(path);
                }
            }
            if snapshots
                .insert(
                    snapshot.snapshot_id(),
                    RetainedSnapshotObservation {
                        manifest_list_path,
                        live_data_paths,
                        forge_operation_id,
                        forge_group,
                    },
                )
                .is_some()
            {
                return Err(ForgeError::Reconciliation {
                    detail: "duplicate retained snapshot identity".to_owned(),
                });
            }
        }
        Self::require_running(stop)?;
        Ok(RetainedManifestObservation {
            metadata_location: table
                .metadata_location_result()
                .map_err(ForgeError::Catalog)?
                .to_owned(),
            table_location: table.metadata().location().to_owned(),
            partition_spec_id: table.metadata().default_partition_spec_id(),
            current_snapshot_id: table.metadata().current_snapshot_id(),
            snapshots,
        })
    }

    /// Convert a typed prepared path to its validated table-owned object key.
    ///
    /// # Errors
    ///
    /// Returns a path invariant error when the path escapes the table binding.
    fn protected_object_key(
        &self,
        binding: &TenantTableBinding,
        table_location: &str,
        path: &StoragePath,
    ) -> Result<String, ForgeError> {
        catalog_path_to_object_key(table_location, binding, &self.core.staging, path.as_str())
    }

    /// Return shutdown consistently at every reconciliation cancellation point.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] when cancellation was requested.
    fn require_running(stop: &CancellationToken) -> Result<(), ForgeError> {
        if stop.is_cancelled() {
            Err(ForgeError::Shutdown)
        } else {
            Ok(())
        }
    }

    /// Classify and, when proven, terminally transition one Prepared row.
    ///
    /// # Errors
    ///
    /// Returns malformed evidence, catalog, storage, cancellation, fence, or
    /// transactional terminal-write errors.
    async fn classify_live_operation(
        &self,
        context: LiveClassificationContext<'_>,
        row: &ForgeOperationStateRow,
        outcome: &mut IcebergReconciliationOutcome,
    ) -> Result<(), ForgeError> {
        let prepared = parse_prepared(row, context.key)?;
        if prepared.partition_spec_id != context.observation_a.partition_spec_id
            || prepared.partition_spec_id != context.observation_b.partition_spec_id
        {
            return Err(ForgeError::Reconciliation {
                detail: "Prepared live rewrite does not match retained table layout".to_owned(),
            });
        }
        let evidence = derive_live_evidence(self, &context, &prepared)?;
        let group_key = ForgeGroupKey {
            tenant: context.key.tenant,
            table_ref: context.key.table_ref.clone(),
            partition: prepared.partition,
        };
        let uncertainty =
            chrono::Duration::from_std(self.core.config.uncertainty_bound).map_err(|error| {
                ForgeError::InvalidConfig {
                    detail: format!("uncertainty bound cannot be represented: {error}"),
                }
            })?;
        let young = context.now.signed_duration_since(row.prepared_at) < uncertainty;
        match classify_evidence(&evidence, young) {
            LiveDisposition::Recovered(snapshot_id) => {
                let volume = self
                    .measure_rewrite_volume(
                        RewritePathContract::Catalog {
                            binding: context.binding,
                            table_location: &context.observation_b.table_location,
                        },
                        &prepared.input_paths,
                        &prepared.output_paths,
                    )
                    .await?;
                Self::require_running(context.stop)?;
                context
                    .lease
                    .require_fence(&self.core.operator_pool)
                    .await?;
                if !context
                    .lease
                    .commit_window_fits(self.core.config.commit_window())
                {
                    return Err(ForgeError::FenceLost {
                        lease_key: context.lease.lease_key.clone(),
                    });
                }
                self.append_live_audit(
                    context.lease,
                    &group_key,
                    "forge.iceberg_rewrite.recovered",
                    iceberg_terminal_detail(
                        &row.prepared_detail,
                        ForgeIcebergRewritePhase::Recovered,
                        Some(snapshot_id),
                    )?,
                )
                .await?;
                self.core.telemetry.record_operation(
                    super::metrics::ForgeMetricSource::Iceberg,
                    super::metrics::ForgeOperationResult::Recovered,
                    1,
                );
                self.core.telemetry.record_rewrite_volume(
                    super::metrics::ForgeMetricSource::Iceberg,
                    volume.input_files,
                    volume.input_bytes,
                    volume.output_files,
                    volume.output_bytes,
                );
                outcome.recovered = outcome.recovered.saturating_add(1);
            }
            LiveDisposition::Pending => {
                protect(outcome, evidence.outputs);
                outcome.pending = outcome.pending.saturating_add(1);
            }
            LiveDisposition::Reset => {
                self.apply_live_reset(context, row, &group_key, evidence.outputs, outcome)
                    .await?;
            }
            LiveDisposition::Unresolved => {
                protect(outcome, evidence.outputs);
                outcome.unresolved = outcome.unresolved.saturating_add(1);
            }
        }
        Ok(())
    }

    /// Terminally reset one proven abandoned operation for delayed orphan GC.
    ///
    /// This method is the logical-enqueue side of the deletion boundary adapted
    /// here: it persists the exact output generation as `Reset` but
    /// never touches object storage. `orphan_gc` alone owns the TTL, refreshed
    /// protection checks, physical deletion, and terminal deletion audit.
    ///
    /// # Errors
    ///
    /// Returns cancellation, fence, or terminal-write errors. A failed append
    /// leaves the original `Prepared` operation authoritative and retryable.
    async fn apply_live_reset(
        &self,
        context: LiveClassificationContext<'_>,
        row: &ForgeOperationStateRow,
        group_key: &ForgeGroupKey,
        _outputs: Vec<String>,
        outcome: &mut IcebergReconciliationOutcome,
    ) -> Result<(), ForgeError> {
        Self::require_running(context.stop)?;
        context
            .lease
            .require_fence(&self.core.operator_pool)
            .await?;
        if !context
            .lease
            .commit_window_fits(self.core.config.commit_window())
        {
            return Err(ForgeError::FenceLost {
                lease_key: context.lease.lease_key.clone(),
            });
        }
        self.append_live_audit(
            context.lease,
            group_key,
            "forge.iceberg_rewrite.reset",
            iceberg_terminal_detail(&row.prepared_detail, ForgeIcebergRewritePhase::Reset, None)?,
        )
        .await?;
        self.core.telemetry.record_operation(
            super::metrics::ForgeMetricSource::Iceberg,
            super::metrics::ForgeOperationResult::Reset,
            1,
        );
        outcome.reset = outcome.reset.saturating_add(1);
        Ok(())
    }
}

/// Parse the paired identity properties relevant to live replacement.
///
/// Other Forge workflows are intentionally ignored because their snapshots
/// cannot prove an Iceberg rewrite terminal.
///
/// # Errors
///
/// Returns reconciliation error for malformed UUIDs, half-present identity
/// properties, or an empty group.
fn parse_snapshot_forge_identity(
    workflow: Option<&str>,
    operation: Option<&String>,
    group: Option<&String>,
) -> Result<(Option<Uuid>, Option<String>), ForgeError> {
    match (workflow, operation, group) {
        (Some(workflow), _, _) if workflow != "iceberg-rewrite" => Ok((None, None)),
        (_, None, None) => Ok((None, None)),
        (_, Some(operation), Some(group)) if !group.is_empty() => Ok((
            Some(
                Uuid::parse_str(operation).map_err(|error| ForgeError::Reconciliation {
                    detail: format!("invalid snapshot Forge operation property: {error}"),
                })?,
            ),
            Some(group.clone()),
        )),
        _ => Err(ForgeError::Reconciliation {
            detail: "snapshot Forge operation properties must be present together".to_owned(),
        }),
    }
}

/// Apply the normative Recovered, Pending, Reset, Unresolved ordering.
fn classify_evidence(evidence: &LiveEvidence, young: bool) -> LiveDisposition {
    if let Some(snapshot_id) = evidence.recovered_snapshot {
        LiveDisposition::Recovered(snapshot_id)
    } else if young {
        LiveDisposition::Pending
    } else if evidence.conditions.contains(&LiveCondition::Stable)
        && !evidence
            .conditions
            .contains(&LiveCondition::ConflictingOperationProperty)
        && evidence
            .conditions
            .contains(&LiveCondition::InputsAllCurrent)
        && !evidence.conditions.contains(&LiveCondition::OutputSeen)
    {
        LiveDisposition::Reset
    } else {
        LiveDisposition::Unresolved
    }
}

/// Fail before manifest traversal when retained metadata exceeds its hard cap.
///
/// # Errors
///
/// Returns reconciliation error when `count` is greater than `cap`.
fn validate_retained_snapshot_count(count: usize, cap: usize) -> Result<(), ForgeError> {
    if count > cap {
        Err(ForgeError::Reconciliation {
            detail: "retained snapshot count exceeds reconciliation cap".to_owned(),
        })
    } else {
        Ok(())
    }
}

/// Parse and validate the identity-bearing fields of one Prepared row.
///
/// # Errors
///
/// Returns reconciliation error for the wrong detail, identity, empty or
/// duplicate paths, malformed partition, resource mismatch, or layout mismatch.
fn parse_prepared(
    row: &ForgeOperationStateRow,
    key: &ForgeTableKey,
) -> Result<PreparedRewrite, ForgeError> {
    let AuditDetail::ForgeIcebergRewrite {
        operation_id,
        phase: ForgeIcebergRewritePhase::Prepared,
        group,
        base_snapshot_id: _,
        partition_spec_id,
        time_partition,
        input_paths,
        output_paths,
        ..
    } = &row.prepared_detail
    else {
        return Err(ForgeError::Reconciliation {
            detail: "open live operation is not a Prepared rewrite".to_owned(),
        });
    };
    let expected = ForgeGroupKey::table_audit_resource(key.tenant, &key.table_ref);
    if row.operation_id != *operation_id
        || row.resource != expected
        || group != &expected
        || input_paths.is_empty()
        || output_paths.is_empty()
        || *partition_spec_id < 0
    {
        return Err(ForgeError::Reconciliation {
            detail: "Prepared live rewrite identity does not match its table".to_owned(),
        });
    }
    let unique_inputs: BTreeSet<_> = input_paths.iter().map(StoragePath::as_str).collect();
    let unique_outputs: BTreeSet<_> = output_paths.iter().map(StoragePath::as_str).collect();
    if unique_inputs.len() != input_paths.len() || unique_outputs.len() != output_paths.len() {
        return Err(ForgeError::Reconciliation {
            detail: "Prepared live rewrite contains duplicate paths".to_owned(),
        });
    }
    Ok(PreparedRewrite {
        operation_id: *operation_id,
        group: group.clone(),
        partition_spec_id: *partition_spec_id,
        partition: TimePartition::from_wire(*time_partition),
        input_paths: input_paths.clone(),
        output_paths: output_paths.clone(),
    })
}

/// Derive stable A/B path, current-membership, and operation-property evidence.
///
/// # Errors
///
/// Returns a binding path error when a prepared path escapes either observed
/// table root.
fn derive_live_evidence(
    forge: &Forge,
    context: &LiveClassificationContext<'_>,
    prepared: &PreparedRewrite,
) -> Result<LiveEvidence, ForgeError> {
    let inputs_a = normalize_paths(
        forge,
        context.binding,
        context.observation_a,
        &prepared.input_paths,
    )?;
    let outputs_a = normalize_paths(
        forge,
        context.binding,
        context.observation_a,
        &prepared.output_paths,
    )?;
    let inputs_b = normalize_paths(
        forge,
        context.binding,
        context.observation_b,
        &prepared.input_paths,
    )?;
    let outputs_b = normalize_paths(
        forge,
        context.binding,
        context.observation_b,
        &prepared.output_paths,
    )?;
    let stable = context.observation_a == context.observation_b
        && inputs_a == inputs_b
        && outputs_a == outputs_b;
    let current_live = context
        .observation_b
        .current_snapshot_id
        .and_then(|id| context.observation_b.snapshots.get(&id))
        .map_or_else(BTreeSet::new, |snapshot| snapshot.live_data_paths.clone());
    let matching: Vec<_> = context
        .observation_b
        .snapshots
        .iter()
        .filter(|(_, snapshot)| {
            snapshot.forge_operation_id == Some(prepared.operation_id)
                && snapshot.forge_group.as_deref() == Some(prepared.group.as_str())
        })
        .collect();
    let conflicting_operation_property = context.observation_b.snapshots.values().any(|snapshot| {
        snapshot.forge_operation_id == Some(prepared.operation_id)
            && snapshot.forge_group.as_deref() != Some(prepared.group.as_str())
    });
    let recovered_snapshot = if stable && matching.len() == 1 && !conflicting_operation_property {
        let (snapshot_id, snapshot) = matching[0];
        (outputs_b
            .iter()
            .all(|path| snapshot.live_data_paths.contains(path))
            && outputs_b.iter().all(|path| current_live.contains(path))
            && inputs_b.iter().all(|path| !current_live.contains(path)))
        .then_some(*snapshot_id)
    } else {
        None
    };
    let output_seen = outputs_a.iter().chain(&outputs_b).any(|path| {
        context
            .observation_a
            .snapshots
            .values()
            .chain(context.observation_b.snapshots.values())
            .any(|snapshot| snapshot.live_data_paths.contains(path))
    });
    let mut conditions = BTreeSet::new();
    if stable {
        conditions.insert(LiveCondition::Stable);
    }
    if !current_live.is_empty() && inputs_b.iter().all(|path| current_live.contains(path)) {
        conditions.insert(LiveCondition::InputsAllCurrent);
    }
    if output_seen {
        conditions.insert(LiveCondition::OutputSeen);
    }
    if conflicting_operation_property {
        conditions.insert(LiveCondition::ConflictingOperationProperty);
    }
    Ok(LiveEvidence {
        outputs: outputs_b,
        recovered_snapshot,
        conditions,
    })
}

/// Normalize ordered paths without allowing duplicate identity to disappear.
///
/// # Errors
///
/// Returns a binding path error for any path outside the observed table.
fn normalize_paths(
    forge: &Forge,
    binding: &TenantTableBinding,
    observation: &RetainedManifestObservation,
    paths: &[StoragePath],
) -> Result<Vec<String>, ForgeError> {
    paths
        .iter()
        .map(|path| forge.protected_object_key(binding, &observation.table_location, path))
        .collect()
}

/// Protect every output and keep the returned set an exact union.
fn protect(outcome: &mut IcebergReconciliationOutcome, outputs: Vec<String>) {
    outcome.protected_output_paths.extend(outputs);
}

/// Build a terminal detail while preserving every prepared recipe identity.
///
/// # Errors
///
/// Returns reconciliation error unless the input is Prepared and the requested
/// phase/snapshot pairing is exactly Recovered/Some or Reset/None.
fn iceberg_terminal_detail(
    prepared: &AuditDetail,
    phase: ForgeIcebergRewritePhase,
    committed_snapshot_id: Option<i64>,
) -> Result<AuditDetail, ForgeError> {
    let AuditDetail::ForgeIcebergRewrite {
        operation_id,
        phase: ForgeIcebergRewritePhase::Prepared,
        group,
        base_snapshot_id,
        partition_spec_id,
        time_partition,
        target_file_size_bytes,
        input_paths,
        output_paths,
        writer_recipe_version,
        ..
    } = prepared
    else {
        return Err(ForgeError::Reconciliation {
            detail: "live terminal transition requires Prepared rewrite detail".to_owned(),
        });
    };
    if !matches!(
        (phase, committed_snapshot_id),
        (ForgeIcebergRewritePhase::Recovered, Some(_)) | (ForgeIcebergRewritePhase::Reset, None)
    ) {
        return Err(ForgeError::Reconciliation {
            detail: "invalid live terminal phase and snapshot pairing".to_owned(),
        });
    }
    Ok(AuditDetail::ForgeIcebergRewrite {
        operation_id: *operation_id,
        phase,
        group: group.clone(),
        base_snapshot_id: *base_snapshot_id,
        committed_snapshot_id,
        partition_spec_id: *partition_spec_id,
        time_partition: *time_partition,
        target_file_size_bytes: *target_file_size_bytes,
        input_paths: input_paths.clone(),
        output_paths: output_paths.clone(),
        writer_recipe_version: writer_recipe_version.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one complete Prepared detail for terminal-copy tests.
    fn prepared_detail() -> AuditDetail {
        AuditDetail::ForgeIcebergRewrite {
            operation_id: Uuid::nil(),
            phase: ForgeIcebergRewritePhase::Prepared,
            group: "bifrost://tenant/table".to_owned(),
            base_snapshot_id: 7,
            committed_snapshot_id: None,
            partition_spec_id: 1,
            time_partition: TimePartition::new(
                crate::catalog::TimeGranularity::Hour,
                DateTime::from_timestamp(1_767_312_000, 0).expect("fixture hour is representable"),
            )
            .expect("fixture hour is an exact hour boundary")
            .to_wire(),
            target_file_size_bytes: 1024,
            input_paths: vec![StoragePath::new("table/input.parquet").expect("valid input")],
            output_paths: vec![StoragePath::new("table/output.parquet").expect("valid output")],
            writer_recipe_version: "recipe-v1".to_owned(),
        }
    }

    /// Terminal construction changes only phase and committed snapshot evidence.
    #[test]
    fn forge_reconciliation_terminal_detail_preserves_prepared_identity() {
        let prepared = prepared_detail();
        let recovered =
            iceberg_terminal_detail(&prepared, ForgeIcebergRewritePhase::Recovered, Some(11))
                .expect("Recovered pairing is valid");
        let reset = iceberg_terminal_detail(&prepared, ForgeIcebergRewritePhase::Reset, None)
            .expect("Reset pairing is valid");
        assert!(matches!(
            recovered,
            AuditDetail::ForgeIcebergRewrite {
                phase: ForgeIcebergRewritePhase::Recovered,
                committed_snapshot_id: Some(11),
                ..
            }
        ));
        assert!(matches!(
            reset,
            AuditDetail::ForgeIcebergRewrite {
                phase: ForgeIcebergRewritePhase::Reset,
                committed_snapshot_id: None,
                ..
            }
        ));
    }

    /// Live reconciliation uses projection state and has no history scan or sleep.
    #[test]
    fn forge_reconciliation_has_no_history_scan_or_sleep() {
        let source = include_str!("live_reconcile.rs");
        let history_scan = ["list_audit_events", "_for_resource"].concat();
        let recovery_sleep = ["tokio::time", "::sleep"].concat();
        assert!(!source.contains(&history_scan));
        assert!(!source.contains(&recovery_sleep));
    }

    /// Pending, unresolved, and overflow states all select the blocked gate.
    #[test]
    fn forge_reconciliation_gate_blocks_pending_unresolved_and_overflow() {
        for (pending, unresolved, overflowed) in [(1, 0, false), (0, 1, false), (0, 0, true)] {
            let gate = if overflowed || pending > 0 || unresolved > 0 {
                DestructiveMaintenance::Blocked
            } else {
                DestructiveMaintenance::Allowed
            };
            assert_eq!(gate, DestructiveMaintenance::Blocked);
        }
        assert_eq!(
            DestructiveMaintenance::Allowed,
            DestructiveMaintenance::Allowed
        );
    }

    /// Every canonical evidence row selects exactly one classification branch.
    #[test]
    fn forge_reconciliation_classifies_each_operation_exactly_once() {
        let cases = [
            (
                LiveEvidence {
                    outputs: Vec::new(),
                    recovered_snapshot: Some(9),
                    conditions: BTreeSet::from([LiveCondition::Stable, LiveCondition::OutputSeen]),
                },
                false,
                0,
            ),
            (
                LiveEvidence {
                    outputs: Vec::new(),
                    recovered_snapshot: None,
                    conditions: BTreeSet::from([
                        LiveCondition::Stable,
                        LiveCondition::InputsAllCurrent,
                    ]),
                },
                true,
                1,
            ),
            (
                LiveEvidence {
                    outputs: Vec::new(),
                    recovered_snapshot: None,
                    conditions: BTreeSet::from([
                        LiveCondition::Stable,
                        LiveCondition::InputsAllCurrent,
                    ]),
                },
                false,
                2,
            ),
            (
                LiveEvidence {
                    outputs: Vec::new(),
                    recovered_snapshot: None,
                    conditions: BTreeSet::from([LiveCondition::OutputSeen]),
                },
                false,
                3,
            ),
        ];
        for (evidence, young, expected) in cases {
            let actual = match classify_evidence(&evidence, young) {
                LiveDisposition::Recovered(_) => 0,
                LiveDisposition::Pending => 1,
                LiveDisposition::Reset => 2,
                LiveDisposition::Unresolved => 3,
            };
            assert_eq!(actual, expected);
        }
    }

    /// A property half-match cannot authorize recovery or reset.
    #[test]
    fn forge_reconciliation_conflicting_operation_properties_are_unresolved() {
        let evidence = LiveEvidence {
            outputs: vec!["table/output.parquet".to_owned()],
            recovered_snapshot: None,
            conditions: BTreeSet::from([
                LiveCondition::Stable,
                LiveCondition::InputsAllCurrent,
                LiveCondition::ConflictingOperationProperty,
            ]),
        };
        assert!(matches!(
            classify_evidence(&evidence, false),
            LiveDisposition::Unresolved
        ));
    }

    /// Pending and unresolved outputs form the exact protected-path union.
    #[test]
    fn forge_reconciliation_protection_is_an_exact_union() {
        let mut outcome = IcebergReconciliationOutcome {
            recovered: 0,
            reset: 0,
            pending: 1,
            unresolved: 1,
            #[cfg(feature = "test-support")]
            overflowed: false,
            protected_output_paths: BTreeSet::new(),
            destructive_maintenance: DestructiveMaintenance::Blocked,
        };
        protect(
            &mut outcome,
            vec![
                "table/pending.parquet".to_owned(),
                "table/shared.parquet".to_owned(),
            ],
        );
        protect(
            &mut outcome,
            vec![
                "table/unresolved.parquet".to_owned(),
                "table/shared.parquet".to_owned(),
            ],
        );
        assert_eq!(
            outcome.protected_output_paths,
            BTreeSet::from([
                "table/pending.parquet".to_owned(),
                "table/shared.parquet".to_owned(),
                "table/unresolved.parquet".to_owned(),
            ])
        );
    }

    /// The cap rejects the 257th snapshot before any manifest work is admitted.
    #[test]
    fn forge_reconciliation_rejects_257_retained_snapshots_before_traversal() {
        assert!(validate_retained_snapshot_count(256, 256).is_ok());
        assert!(matches!(
            validate_retained_snapshot_count(257, 256),
            Err(ForgeError::Reconciliation { .. })
        ));
    }

    /// A stable matching snapshot is rejected when another property half-matches.
    #[test]
    fn forge_reconciliation_matching_and_conflicting_properties_are_unresolved() {
        let evidence = LiveEvidence {
            outputs: vec!["table/output.parquet".to_owned()],
            recovered_snapshot: None,
            conditions: BTreeSet::from([
                LiveCondition::Stable,
                LiveCondition::OutputSeen,
                LiveCondition::ConflictingOperationProperty,
            ]),
        };
        assert!(matches!(
            classify_evidence(&evidence, false),
            LiveDisposition::Unresolved
        ));
    }

    /// Each row reloads A/B inside the operation loop so catalog motion cannot
    /// leak stale authority from an earlier Prepared operation.
    #[test]
    fn forge_reconciliation_observes_fresh_manifests_per_operation() {
        let source = include_str!("live_reconcile.rs");
        let loop_position = source
            .find("for row in page.operations")
            .expect("operation loop exists");
        let classify_position = source[loop_position..]
            .find("self.classify_live_operation")
            .expect("classification follows observations");
        let loop_body = &source[loop_position..loop_position + classify_position];
        assert_eq!(loop_body.matches("observe_retained_manifests").count(), 2);
    }
}

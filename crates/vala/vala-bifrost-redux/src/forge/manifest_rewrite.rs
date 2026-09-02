//! Metadata-only Iceberg manifest rewrite.
//!
//! Manifest rewrite regroups manifest entries to reduce planning cost. It is
//! not data rewrite and must never behave like one: every data file the table
//! logically contains before the rewrite is still contained after it, path for
//! path, and the only objects the operation adds or removes are manifests.
//! Keeping that proof in one owner is what lets the protocol compose with data
//! rewrite and destructive maintenance without becoming either of them.

use std::collections::BTreeMap;

use uuid::Uuid;
use wyrd_spec::vala::StoragePath;
use wyrd_spec::vala::api::{AuditDetail, ForgeManifestRewritePhase};

use super::Forge;
use super::compact::ForgeTableKey;
use super::error::ForgeError;
use super::expire::table_resource_for_key;
use super::lease::ForgeLease;

/// System principal recorded on every Forge-owned manifest-rewrite audit row.
const SYSTEM_PRINCIPAL: wyrd_spec::auth::PrincipalId =
    wyrd_spec::auth::PrincipalId::new(uuid::Uuid::nil());

/// Audit operation recorded when the rewrite's pre-image is fixed.
pub(super) const MANIFEST_REWRITE_PREPARED: &str = "forge.manifest_rewrite.prepared";
/// Audit operation recorded when the rewrite's catalog commit is proven.
pub(super) const MANIFEST_REWRITE_COMMITTED: &str = "forge.manifest_rewrite.committed";
/// Audit operation recorded when the prepared rewrite provably never committed.
pub(super) const MANIFEST_REWRITE_RESET: &str = "forge.manifest_rewrite.reset";

/// Derives the stable operation identity for one manifest-rewrite selection.
///
/// The identity is a function of the table and the exact selected manifests, so
/// a successor that reconstructs the same selection reconstructs the same
/// operation and settles it once rather than opening a second one.
pub(super) fn manifest_rewrite_operation_id(key: &ForgeTableKey, selected: &[String]) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(table_resource_for_key(key));
    for path in selected {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Uuid::from_bytes(bytes)
}

impl Forge {
    /// Appends one fenced manifest-rewrite audit and projection transition.
    ///
    /// # Errors
    ///
    /// Returns detail-validation, lease, SQL, operation-state, audit, fence, or
    /// commit failures. The caller-owned transaction rolls back both durable
    /// rows together, so the audit row and the projection can never disagree.
    pub(super) async fn append_manifest_rewrite_audit(
        &self,
        lease: &mut ForgeLease,
        tenant: wyrd_spec::DataTenantId,
        detail: &AuditDetail,
        operation: &str,
    ) -> Result<(), ForgeError> {
        let AuditDetail::ForgeManifestRewrite { group, .. } = detail else {
            return Err(ForgeError::Reconciliation {
                detail: "manifest-rewrite audit detail has the wrong kind".to_owned(),
            });
        };
        let event = wyrd_spec::vala::api::AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: operation.to_owned(),
            resource: group.clone(),
            card_ref: None,
            principal_id: SYSTEM_PRINCIPAL,
            principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
            auth_method: wyrd_spec::vala::api::AuthMethod::Internal,
            permission: "bifrost:forge".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
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
        let operations = vala_sql::queries::forge_operations::ForgeOperations::new(
            &event.resource,
            vala_sql::row_types::forge_operations::ForgeOperationFamily::ManifestRewrite,
        )
        .map_err(ForgeError::Sql)?;
        let transition = if operation == MANIFEST_REWRITE_PREPARED {
            operations.append_prepared(&mut conn, &event).await
        } else {
            operations.append_terminal(&mut conn, &event).await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            vala_sql::row_types::forge_operations::ForgeOperationTransition::Applied { .. }
            | vala_sql::row_types::forge_operations::ForgeOperationTransition::AlreadyApplied {
                ..
            } => {}
        }
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}

/// One manifest and the data files its live entries name.
///
/// Grouping is the only thing a manifest rewrite may change, so the unit of
/// both the input and the output description is a manifest plus its membership
/// rather than a flat file list.
#[derive(Debug, Clone)]
pub(super) struct ManifestGrouping {
    /// Catalog path of the manifest itself.
    pub(super) manifest_path: StoragePath,
    /// Data files this manifest's live entries name.
    pub(super) data_file_paths: Vec<StoragePath>,
}

/// Identity and pre-image of one manifest-rewrite operation.
pub(super) struct ManifestRewriteInputs {
    /// Stable identity shared by prepared and terminal audit rows.
    pub(super) operation_id: Uuid,
    /// Canonical tenant/table resource identity.
    pub(super) group: String,
    /// Metadata location observed before the rewrite.
    pub(super) base_metadata_location: StoragePath,
    /// Manifests the rewrite replaces, in catalog order.
    pub(super) inputs: Vec<ManifestGrouping>,
}

/// One manifest-rewrite operation across its durable lifecycle.
///
/// The operation is prepared before the catalog commit, when only the inputs
/// are known, and settled afterwards into exactly one terminal phase. The
/// metadata-only proof lives on the commit transition because that is the only
/// point at which both the before and after groupings exist.
pub(super) struct ManifestRewriteOperation {
    /// Stable identity shared by prepared and terminal audit rows.
    operation_id: Uuid,
    /// Canonical tenant/table resource identity.
    group: String,
    /// Metadata location observed before the rewrite.
    base_metadata_location: StoragePath,
    /// Manifests the rewrite replaces, in catalog order.
    inputs: Vec<ManifestGrouping>,
}

impl ManifestRewriteOperation {
    /// Fixes the operation's identity and pre-image before the catalog commit.
    pub(super) fn prepare(inputs: ManifestRewriteInputs) -> Self {
        Self {
            operation_id: inputs.operation_id,
            group: inputs.group,
            base_metadata_location: inputs.base_metadata_location,
            inputs: inputs.inputs,
        }
    }

    /// Builds the audit detail recorded before the catalog commit.
    ///
    /// The output manifests are empty here rather than predicted: Iceberg names
    /// them while committing, and an audit row that guessed them would claim an
    /// identity the operation might never produce.
    pub(super) fn prepared_detail(&self) -> AuditDetail {
        self.detail(ForgeManifestRewritePhase::Prepared, Vec::new(), None)
    }

    /// Proves the committed rewrite changed grouping and nothing else.
    ///
    /// Data-file membership is compared as a multiset because regrouping is
    /// exactly a permutation across manifests: which manifest names a file is
    /// what the rewrite changes, how many times the table names it is not.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Reconciliation`] when the output manifests do not
    /// name precisely the input manifests' data files — a dropped file would
    /// make the operation a deletion and an added one an append — when an
    /// output was written over an input manifest, leaving the rewrite
    /// unrecoverable, or when two outputs claim one manifest path.
    pub(super) fn commit(
        &self,
        outputs: &[ManifestGrouping],
        committed_metadata_location: StoragePath,
        phase: ForgeManifestRewritePhase,
    ) -> Result<AuditDetail, ForgeError> {
        if data_file_multiset(&self.inputs) != data_file_multiset(outputs) {
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "manifest rewrite {} changed the table's data files, not their grouping",
                    self.operation_id
                ),
            });
        }
        let input_paths: Vec<&str> = self
            .inputs
            .iter()
            .map(|manifest| manifest.manifest_path.as_str())
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for output in outputs {
            let path = output.manifest_path.as_str();
            if !seen.insert(path) {
                return Err(ForgeError::Reconciliation {
                    detail: format!(
                        "manifest rewrite {} names output manifest {path} twice",
                        self.operation_id
                    ),
                });
            }
            if input_paths.contains(&path) {
                return Err(ForgeError::Reconciliation {
                    detail: format!(
                        "manifest rewrite {} wrote output manifest {path} over its own input",
                        self.operation_id
                    ),
                });
            }
        }
        Ok(self.detail(
            phase,
            outputs
                .iter()
                .map(|manifest| manifest.manifest_path.clone())
                .collect(),
            Some(committed_metadata_location),
        ))
    }

    /// Builds the terminal detail for a prepared rewrite that never committed.
    pub(super) fn reset_detail(&self) -> AuditDetail {
        self.detail(ForgeManifestRewritePhase::Reset, Vec::new(), None)
    }

    /// Projects the operation into one audit detail.
    fn detail(
        &self,
        phase: ForgeManifestRewritePhase,
        output_manifest_paths: Vec<StoragePath>,
        committed_metadata_location: Option<StoragePath>,
    ) -> AuditDetail {
        AuditDetail::ForgeManifestRewrite {
            operation_id: self.operation_id,
            phase,
            group: self.group.clone(),
            base_metadata_location: self.base_metadata_location.clone(),
            committed_metadata_location,
            input_manifest_paths: self
                .inputs
                .iter()
                .map(|manifest| manifest.manifest_path.clone())
                .collect(),
            output_manifest_paths,
        }
    }
}

/// Counts how many times each data file is named across a manifest set.
fn data_file_multiset(manifests: &[ManifestGrouping]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for manifest in manifests {
        for file in &manifest.data_file_paths {
            *counts.entry(file.as_str()).or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one manifest holding the named data files.
    fn manifest(path: &str, data_files: &[&str]) -> ManifestGrouping {
        ManifestGrouping {
            manifest_path: StoragePath::new(path).expect("manifest path"),
            data_file_paths: data_files
                .iter()
                .map(|file| StoragePath::new(*file).expect("data path"))
                .collect(),
        }
    }

    /// Proves the prepared detail names its inputs and predicts no output.
    ///
    /// Prepared is the phase a successor reads when the worker dies mid-commit,
    /// so it must carry the exact input identity and must not claim an output
    /// Iceberg has not yet named.
    ///
    /// # Panics
    ///
    /// Panics when the detail is not a manifest-rewrite record or when any
    /// recorded field diverges from the prepared operation.
    fn assert_prepared_detail_is_exact(
        operation: &ManifestRewriteOperation,
        base: &StoragePath,
        input_paths: &[StoragePath],
    ) {
        let AuditDetail::ForgeManifestRewrite {
            phase,
            input_manifest_paths,
            output_manifest_paths,
            committed_metadata_location,
            base_metadata_location,
            ..
        } = operation.prepared_detail()
        else {
            panic!("manifest rewrite must record its own audit kind");
        };
        assert_eq!(phase, ForgeManifestRewritePhase::Prepared);
        assert_eq!(&base_metadata_location, base);
        assert_eq!(committed_metadata_location, None);
        assert_eq!(input_manifest_paths, input_paths);
        assert!(
            output_manifest_paths.is_empty(),
            "the outputs are named by the commit, never predicted"
        );
    }

    /// Proves every way a commit stops being a pure regrouping is refused.
    ///
    /// Each case names one real failure of the metadata-only guarantee rather
    /// than a variation on the same one: a lost file, an invented file, an
    /// output written over an input, and two outputs claiming one path.
    ///
    /// # Panics
    ///
    /// Panics when any of those commits is accepted.
    fn assert_refuses_every_non_regrouping(
        operation: &ManifestRewriteOperation,
        committed: &StoragePath,
    ) {
        let refuse = |outputs: Vec<ManifestGrouping>| {
            operation.commit(
                &outputs,
                committed.clone(),
                ForgeManifestRewritePhase::Committed,
            )
        };
        assert!(
            refuse(vec![manifest(
                "t/spans/metadata/m-out-0.avro",
                &[
                    "t/spans/data/a.parquet",
                    "t/spans/data/b.parquet",
                    "t/spans/data/c.parquet"
                ]
            )])
            .is_err(),
            "dropping a data file makes the rewrite a deletion"
        );
        assert!(
            refuse(vec![manifest(
                "t/spans/metadata/m-out-0.avro",
                &[
                    "t/spans/data/a.parquet",
                    "t/spans/data/b.parquet",
                    "t/spans/data/c.parquet",
                    "t/spans/data/d.parquet",
                    "t/spans/data/e.parquet"
                ]
            )])
            .is_err(),
            "inventing a data file makes the rewrite an append"
        );
        assert!(
            refuse(vec![
                manifest(
                    "t/spans/metadata/m-in-0.avro",
                    &["t/spans/data/a.parquet", "t/spans/data/b.parquet"]
                ),
                manifest(
                    "t/spans/metadata/m-out-1.avro",
                    &["t/spans/data/c.parquet", "t/spans/data/d.parquet"]
                ),
            ])
            .is_err(),
            "an output written over an input manifest is not recoverable"
        );
        assert!(
            refuse(vec![
                manifest("t/spans/metadata/m-out-0.avro", &["t/spans/data/a.parquet"]),
                manifest(
                    "t/spans/metadata/m-out-0.avro",
                    &[
                        "t/spans/data/b.parquet",
                        "t/spans/data/c.parquet",
                        "t/spans/data/d.parquet"
                    ]
                ),
            ])
            .is_err(),
            "two outputs cannot claim one manifest path"
        );
    }

    /// A manifest rewrite may regroup entries and nothing else.
    ///
    /// The accepted case regroups four data files from three manifests into
    /// two, which is the whole point of the protocol. Every refusal is a way
    /// the operation could have stopped being metadata-only: losing a data
    /// file, inventing one, or writing an output manifest over an input.
    #[test]
    fn forge_manifest_rewrite_preserves_data_file_identity() {
        let base = StoragePath::new("t/spans/metadata/v7.metadata.json").expect("metadata path");
        let committed =
            StoragePath::new("t/spans/metadata/v8.metadata.json").expect("metadata path");
        let operation_id = Uuid::now_v7();
        let inputs = vec![
            manifest("t/spans/metadata/m-in-0.avro", &["t/spans/data/a.parquet"]),
            manifest(
                "t/spans/metadata/m-in-1.avro",
                &["t/spans/data/b.parquet", "t/spans/data/c.parquet"],
            ),
            manifest("t/spans/metadata/m-in-2.avro", &["t/spans/data/d.parquet"]),
        ];
        let operation = ManifestRewriteOperation::prepare(ManifestRewriteInputs {
            operation_id,
            group: "tenant/traces/spans".to_owned(),
            base_metadata_location: base.clone(),
            inputs: inputs.clone(),
        });
        let input_paths: Vec<StoragePath> = inputs
            .iter()
            .map(|manifest| manifest.manifest_path.clone())
            .collect();

        assert_prepared_detail_is_exact(&operation, &base, &input_paths);

        let outputs = vec![
            manifest(
                "t/spans/metadata/m-out-0.avro",
                &["t/spans/data/d.parquet", "t/spans/data/a.parquet"],
            ),
            manifest(
                "t/spans/metadata/m-out-1.avro",
                &["t/spans/data/c.parquet", "t/spans/data/b.parquet"],
            ),
        ];
        let AuditDetail::ForgeManifestRewrite {
            phase,
            input_manifest_paths,
            output_manifest_paths,
            committed_metadata_location,
            ..
        } = operation
            .commit(
                &outputs,
                committed.clone(),
                ForgeManifestRewritePhase::Committed,
            )
            .expect("a pure regrouping is a valid manifest rewrite")
        else {
            panic!("manifest rewrite must record its own audit kind");
        };
        assert_eq!(phase, ForgeManifestRewritePhase::Committed);
        assert_eq!(committed_metadata_location, Some(committed.clone()));
        assert_eq!(
            input_manifest_paths, input_paths,
            "the recorded inputs are exact and ordered"
        );
        assert_eq!(
            output_manifest_paths,
            outputs
                .iter()
                .map(|manifest| manifest.manifest_path.clone())
                .collect::<Vec<_>>(),
            "the recorded outputs are exact and ordered"
        );

        assert_refuses_every_non_regrouping(&operation, &committed);

        let AuditDetail::ForgeManifestRewrite {
            phase,
            output_manifest_paths,
            committed_metadata_location,
            ..
        } = operation.reset_detail()
        else {
            panic!("manifest rewrite must record its own audit kind");
        };
        assert_eq!(phase, ForgeManifestRewritePhase::Reset);
        assert!(output_manifest_paths.is_empty());
        assert_eq!(committed_metadata_location, None);
    }
}

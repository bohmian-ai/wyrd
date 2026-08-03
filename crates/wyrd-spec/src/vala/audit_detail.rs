//! Typed, redacted detail carried by the transactional audit event.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::auth::{PrincipalId, PrincipalKindTag};
use crate::envelope::{CardKind, SpecHash};
use crate::ids::CardUid;
use crate::origin::Origin;
use crate::reference::CardRef;
use crate::vala::api::AuditDecision;
use crate::vala::api::{QueryClass, VisibilityMode};

/// Error returned when an audit-detail identifier is empty, malformed, or
/// contains a value that must never enter an audit record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditDetailValueError {
    /// The value was empty after boundary normalization.
    #[error("{field} must not be empty")]
    Empty {
        /// The audit-detail field being validated.
        field: &'static str,
    },
    /// The value contains a control character.
    #[error("{field} contains a control character")]
    ControlCharacter {
        /// The audit-detail field being validated.
        field: &'static str,
    },
    /// The value resembles a credential or other secret.
    #[error("{field} contains a secret-like value")]
    SecretLike {
        /// The audit-detail field being validated.
        field: &'static str,
    },
    /// The value exceeds the scrubbed audit-detail byte ceiling.
    #[error("{field} exceeds 1024 UTF-8 bytes")]
    TooLong {
        /// The audit-detail field being validated.
        field: &'static str,
    },
    /// Related audit fields violate a closed contract invariant.
    #[error("audit detail fields violate the {invariant} invariant")]
    InvalidCombination {
        /// Name of the violated invariant.
        invariant: &'static str,
    },
}

macro_rules! audit_detail_value {
    ($name:ident, $field:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        #[serde(transparent)]
        pub struct $name(
            /// Validated normalized audit-safe text.
            String,
        );

        impl $name {
            /// Construct a normalized, non-secret audit-detail value.
            ///
            /// # Errors
            /// Returns an error when the value is empty, contains controls, or
            /// resembles a credential.
            pub fn new(value: impl Into<String>) -> Result<Self, AuditDetailValueError> {
                let value = value.into();
                let value = value.trim();
                if value.is_empty() {
                    return Err(AuditDetailValueError::Empty { field: $field });
                }
                if value.chars().any(char::is_control) {
                    return Err(AuditDetailValueError::ControlCharacter { field: $field });
                }
                if value.len() > 1_024 {
                    return Err(AuditDetailValueError::TooLong { field: $field });
                }
                if is_secret_like(value) {
                    return Err(AuditDetailValueError::SecretLike { field: $field });
                }
                Ok(Self(value.to_owned()))
            }

            /// Borrow the normalized value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

audit_detail_value!(
    ScopeHash,
    "scope_hash",
    "Stable, non-secret digest identifying a card scope."
);
audit_detail_value!(
    StoragePath,
    "storage_path",
    "Logical, non-secret path identifying stored audit data."
);
audit_detail_value!(
    BatchId,
    "batch_id",
    "Idempotent, non-secret identifier for an ingest batch."
);
audit_detail_value!(
    QueryAuditDigest,
    "query_audit_digest",
    "Stable non-secret digest used by a Bifrost query audit decision."
);

/// Closed execution topology recorded by a Bifrost read decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum QueryExecutionMode {
    /// The leader executes every scan locally.
    Local,
    /// The leader delegates immutable sealed scan leaves.
    Distributed,
}

/// Closed phase where a Bifrost security invariant failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BifrostSecurityPhase {
    /// Query planning and binding.
    Planning,
    /// Source access or tenant tripwire.
    Source,
    /// Private peer authentication and execution.
    Peer,
}

/// Closed Bifrost security violation classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BifrostSecurityViolationKind {
    /// Authenticated tenant binding mismatch.
    TenantBinding,
    /// Tenant-scoped object path mismatch.
    TenantPath,
    /// Runtime row tenant mismatch.
    TenantRow,
    /// Invalid peer signature.
    PeerSignature,
    /// Unknown peer key identifier.
    PeerUnknownKey,
    /// Replayed peer nonce.
    PeerReplay,
    /// Wrong peer audience.
    PeerAudience,
    /// Stale peer fence.
    PeerFence,
    /// Verified peer tenant mismatch.
    PeerTenant,
    /// Peer manifest digest mismatch.
    PeerManifest,
    /// Peer fragment digest mismatch.
    PeerFragment,
    /// Invalid Scribe-tail ticket audience.
    TailAudience,
    /// Scribe-tail tenant or table binding mismatch.
    TailBinding,
    /// Scribe-tail fence or writer epoch mismatch.
    TailFence,
    /// Replayed Scribe-tail ticket or capability.
    TailReplay,
}

fn is_secret_like(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("-----begin ")
        || lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || lower.starts_with("pk-")
        || lower.starts_with("eyj")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("authorization=")
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("token=")
        || lower.contains("/secrets/")
}

/// Closed operation names for storage lifecycle audit rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StorageAuditOperation {
    /// An upload session was durably created.
    SessionCreated,
    /// The object-store backend was initialized.
    BackendInitialized,
    /// The object-store backend initialization failed.
    BackendFailed,
    /// An upload completed.
    Complete,
    /// An upload was aborted.
    Abort,
    /// A download was initialized.
    Download,
    /// A stale upload was reclaimed.
    Reclaimed,
}

/// Closed stable codes used for audit failures and denial reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditErrorCode {
    /// The caller lacked the required permission.
    PermissionDenied,
    /// The audit outbox was unavailable.
    AuditUnavailable,
    /// The supplied token was invalid.
    InvalidToken,
    /// The requested resource was not found.
    NotFound,
    /// The operation failed in the storage backend.
    StorageBackendFailure,
    /// The operation failed validation.
    ValidationFailed,
}

/// Closed card-registration operation names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CardRegistrationOperation {
    /// A card was registered.
    Register,
    /// A card was updated.
    Update,
    /// A card was deleted.
    Delete,
}

/// Closed card-registration outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CardRegistrationOutcome {
    /// A new card was created.
    Created,
    /// The registration was an idempotent no-op.
    IdempotentNoop,
    /// The registration was deduplicated.
    Deduplicated,
}

/// Closed card-scope mint operation names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CardScopeMintKind {
    /// Minted during API-key exchange.
    ApiKeyExchange,
    /// Minted during token refresh.
    Refresh,
    /// Minted for delegation.
    Delegation,
    /// Minted during JWT bearer exchange.
    JwtBearer,
}

/// Structured detail for one auditable operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditDetail {
    /// Bounded deployment-wide aggregates captured by Oracle admission recovery.
    OracleAdmissionRecovery {
        /// Number of expired admission leases removed by recovery.
        expired_lease_count: u64,
        /// Number of unexpired admission leases retained by recovery.
        active_lease_count: u64,
        /// Interactive slot units retained across active leases.
        interactive_slots: u64,
        /// Analytical slot units retained across active leases.
        analytical_slots: u64,
        /// Total slot units retained across every active lease.
        total_slots: u64,
    },
    /// Immutable, scrubbed Bifrost visibility-cut read decision.
    BifrostQueryReadDecision {
        /// Digest of the normalized query.
        query_digest: QueryAuditDigest,
        /// Server-derived admission class.
        query_class: QueryClass,
        /// Visibility mode committed by the cut.
        visibility: VisibilityMode,
        /// Sorted tenant-table binding digests.
        binding_digests: Vec<QueryAuditDigest>,
        /// Pinned snapshot summary digest.
        snapshot_digest: QueryAuditDigest,
        /// Hot manifest summary digest.
        manifest_digest: QueryAuditDigest,
        /// Authorized projection digest.
        projection_digest: QueryAuditDigest,
        /// Permission decision digest.
        permission_digest: QueryAuditDigest,
        /// Local or distributed execution decision.
        execution: QueryExecutionMode,
        /// Selected execution nodes including leader.
        selected_node_count: u8,
        /// Selected workers excluding leader.
        worker_count: u8,
        /// Total admission slot demand.
        slot_units: u32,
        /// Whole-cut retry ordinal, zero or one.
        retry_ordinal: u8,
        /// Settled deadline in milliseconds.
        deadline_ms: u64,
    },
    /// Scrubbed tenant or peer security violation.
    BifrostSecurityViolation {
        /// Closed violation class.
        violation: BifrostSecurityViolationKind,
        /// Boundary where validation failed.
        phase: BifrostSecurityPhase,
        /// Trusted query digest when one exists.
        query_digest: Option<QueryAuditDigest>,
    },
    /// A live Iceberg file replacement and its recoverable external boundary.
    ///
    /// The ordered paths are the exact current-snapshot inputs and the exact
    /// rewrite outputs. `base_snapshot_id` is the plan fence, never an input
    /// file's addition snapshot.
    ForgeIcebergRewrite {
        /// Stable identity shared by output names, Iceberg, and audit rows.
        operation_id: uuid::Uuid,
        /// Durable lifecycle transition represented by this row.
        phase: ForgeIcebergRewritePhase,
        /// Canonical tenant/table resource identity.
        group: String,
        /// Snapshot observed by the table plan before discovery.
        base_snapshot_id: i64,
        /// Snapshot returned after a proven replacement commit.
        committed_snapshot_id: Option<i64>,
        /// Destination partition specification identity.
        partition_spec_id: i32,
        /// Shared day partition for every exact input.
        partition_day: String,
        /// Target output size captured from the table metadata.
        target_file_size_bytes: u64,
        /// Exact ordered catalog paths deleted by the Iceberg action.
        input_paths: Vec<StoragePath>,
        /// Exact ordered rewritten object paths added by the Iceberg action.
        output_paths: Vec<StoragePath>,
        /// Physical recipe used to produce the outputs.
        writer_recipe_version: String,
    },
    /// A Forge compaction operation and its external Iceberg boundary.
    ///
    /// The ordered `output_paths` list is the sole greenfield output shape;
    /// prepared and terminal audit rows preserve writer-rotation order and do
    /// not expose a scalar compatibility form.
    ForgeCompaction {
        /// Deterministic identifier shared by prepared and terminal rows.
        operation_id: uuid::Uuid,
        /// Durable phase represented by this audit row.
        phase: ForgeCompactionPhase,
        /// Canonical tenant/table/day group identity.
        group: String,
        /// Exact staging rows transitioned by the operation.
        input_file_ids: Vec<uuid::Uuid>,
        /// Exact staging paths consumed by the operation.
        input_paths: Vec<StoragePath>,
        /// Deterministic compacted output paths in writer-rotation order.
        ///
        /// This ordered collection is the only output representation for
        /// greenfield Forge audit rows, shared by prepared and terminal
        /// phases without a scalar compatibility field.
        output_paths: Vec<StoragePath>,
        /// Iceberg snapshot returned by a committed operation, when known.
        snapshot_id: Option<i64>,
        /// Writer recipe identifier used to create the output.
        writer_recipe_version: String,
    },
    /// A Forge snapshot-expiry operation and its Iceberg metadata boundary.
    ForgeSnapshotExpire {
        /// Deterministic identifier shared by prepared and terminal rows.
        operation_id: uuid::Uuid,
        /// Durable phase represented by this audit row.
        phase: ForgeSnapshotExpirePhase,
        /// Canonical tenant/table resource identity.
        group: String,
        /// Metadata location observed before the expiry commit.
        base_metadata_location: StoragePath,
        /// Current snapshot observed before the expiry commit.
        current_snapshot_id: Option<i64>,
        /// Snapshot IDs at the heads of retained Iceberg refs.
        retained_ref_heads: Vec<i64>,
        /// Strict timestamp cutoff used for selection.
        cutoff_ms: i64,
        /// Exact snapshot IDs selected for expiry, sorted ascending.
        selected_snapshot_ids: Vec<i64>,
    },
    /// A Forge orphan-GC operation and its bounded object batch.
    ForgeOrphanGc {
        /// Deterministic identifier shared by prepared and terminal rows.
        operation_id: uuid::Uuid,
        /// Durable phase represented by this audit row.
        phase: ForgeOrphanGcPhase,
        /// Canonical tenant/table resource identity.
        group: String,
        /// Exact sorted object candidates observed before deletion.
        candidate_paths: Vec<StoragePath>,
        /// Objects deleted by the terminal attempt.
        deleted_paths: Vec<StoragePath>,
        /// Objects skipped after the final live-set/age check.
        skipped_paths: Vec<StoragePath>,
    },
    /// Authentication failure metadata; credentials are never representable here.
    AuthFailure {
        /// Stable reason for the authentication refusal.
        error_code: AuditErrorCode,
    },
    /// API-key issuance metadata; the key value is never representable here.
    CredentialIssuance {
        /// Principal receiving the credential.
        target_principal_id: PrincipalId,
        /// Identifier of the issued API key, not its secret value.
        api_key_id: uuid::Uuid,
        /// Credential expiry.
        expires_at: chrono::DateTime<chrono::Utc>,
    },
    /// Token exchange metadata; bearer values are never representable here.
    TokenExchange {
        /// Subject principal in the exchanged token.
        subject_principal_id: PrincipalId,
        /// Principal that performed the exchange.
        actor_principal_id: PrincipalId,
        /// Typed delegation chain.
        delegation_chain: Vec<CardRef>,
        /// Token expiry.
        expires_at: chrono::DateTime<chrono::Utc>,
    },
    /// Refresh-token family revocation metadata.
    RefreshFamilyRevocation {
        /// Principal whose refresh-token family was revoked.
        principal_id: PrincipalId,
        /// Principal kind owning the family.
        principal_kind: PrincipalKindTag,
        /// Number of active refresh rows revoked.
        revoked_token_count: u64,
    },
    /// Authorization decision metadata.
    AuthzCheck {
        /// Calling principal.
        caller_principal_id: PrincipalId,
        /// Called principal.
        callee_principal_id: PrincipalId,
        /// Typed delegation chain.
        delegation_chain: Vec<CardRef>,
        /// Authorization result.
        decision: AuditDecision,
        /// Stable denial reason, present only for a denial.
        deny_reason: Option<AuditErrorCode>,
    },
    /// Card registration metadata.
    CardRegistration {
        /// Registered card UID.
        card_uid: CardUid,
        /// Registered card kind.
        card_kind: CardKind,
        /// Registration operation.
        operation: CardRegistrationOperation,
        /// Registration outcome, when applicable.
        outcome: Option<CardRegistrationOutcome>,
        /// Prior spec hash, when applicable.
        before_spec_hash: Option<SpecHash>,
        /// Resulting spec hash, when applicable.
        after_spec_hash: Option<SpecHash>,
    },
    /// Card-scope mint metadata; scope members are references, never secrets.
    CardScopeMint {
        /// How the scope was minted.
        mint_kind: CardScopeMintKind,
        /// Root card reference.
        root_card_ref: CardRef,
        /// Stable scope digest.
        scope_hash: Option<ScopeHash>,
        /// Number of scope members.
        scope_member_count: Option<u32>,
        /// Typed scope members.
        scope_members: Vec<CardRef>,
        /// Stable failure code, present only on failure.
        failure_code: Option<AuditErrorCode>,
    },
    /// Storage lifecycle transition metadata.
    Storage {
        /// Storage transition.
        operation: StorageAuditOperation,
        /// Multipart upload identifier, when applicable.
        upload_id: Option<uuid::Uuid>,
        /// Logical storage path.
        storage_path: StoragePath,
        /// Closed backend identifier.
        backend: StorageBackend,
        /// HTTP/backend status code.
        status_code: u16,
        /// Stable failure code, when applicable.
        error_code: Option<AuditErrorCode>,
    },
    /// Ingest batch metadata.
    Ingest {
        /// Code-origin of the emitting card or agent.
        origin: Origin,
        /// Idempotent batch identifier.
        batch_id: BatchId,
        /// Destination Bifrost table.
        table: crate::vala::api::BifrostTableName,
        /// Number of records in the batch.
        record_count: u64,
        /// Ingest authorization decision.
        decision: AuditDecision,
    },
}

impl AuditDetail {
    /// Validates bounded Bifrost audit collection and topology invariants.
    ///
    /// Other variants contain their own constructor-validated scalar values.
    ///
    /// # Errors
    /// Returns [`AuditDetailValueError`] when a Bifrost read decision exceeds
    /// 64 bindings/nodes or carries inconsistent execution/retry/deadline data.
    pub fn validate(&self) -> Result<(), AuditDetailValueError> {
        let Self::BifrostQueryReadDecision {
            binding_digests,
            execution,
            selected_node_count,
            worker_count,
            slot_units,
            retry_ordinal,
            deadline_ms,
            ..
        } = self
        else {
            return Ok(());
        };
        if binding_digests.is_empty() || binding_digests.len() > 64 {
            return Err(AuditDetailValueError::InvalidCombination {
                invariant: "binding count",
            });
        }
        if binding_digests
            .windows(2)
            .any(|pair| pair[0].as_str() >= pair[1].as_str())
        {
            return Err(AuditDetailValueError::InvalidCombination {
                invariant: "binding digest order",
            });
        }
        if !(1..=64).contains(selected_node_count)
            || *retry_ordinal > 1
            || *slot_units == 0
            || *deadline_ms == 0
        {
            return Err(AuditDetailValueError::InvalidCombination {
                invariant: "Bifrost read bounds",
            });
        }
        let expected_workers = match execution {
            QueryExecutionMode::Local => 0,
            QueryExecutionMode::Distributed => selected_node_count - 1,
        };
        if *worker_count != expected_workers {
            return Err(AuditDetailValueError::InvalidCombination {
                invariant: "execution topology",
            });
        }
        Ok(())
    }
}

/// Durable phase recorded for a Forge compaction operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ForgeCompactionPhase {
    /// Inputs were hidden before the external Iceberg commit.
    Prepared,
    /// The external Iceberg commit completed.
    Committed,
    /// Reconciliation recovered a previously completed external commit.
    Recovered,
    /// Reconciliation made the exact inputs visible again after expiry.
    Reset,
}

/// Durable phase recorded for a live Iceberg replacement operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ForgeIcebergRewritePhase {
    /// Rewritten outputs were persisted before the external catalog commit.
    Prepared,
    /// The exact Iceberg replacement commit completed.
    Committed,
    /// Reconciliation proved a previously uncertain commit completed.
    Recovered,
    /// Reconciliation proved the prepared operation was not committed.
    Reset,
}

/// Durable phase recorded for a Forge snapshot-expiry operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ForgeSnapshotExpirePhase {
    /// Snapshot IDs were selected and the external commit is about to start.
    Prepared,
    /// The external Iceberg commit completed.
    Committed,
    /// Reconciliation proved the external commit completed.
    Recovered,
}

/// Durable phase recorded for a Forge orphan-GC operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ForgeOrphanGcPhase {
    /// A bounded candidate batch was prepared for deletion.
    Prepared,
    /// Every eligible candidate in the batch was handled.
    Committed,
    /// Reconciliation completed a previously prepared batch.
    Recovered,
}

/// Closed storage backend identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    /// Local filesystem backend.
    Local,
    /// Amazon S3 backend.
    S3,
    /// Google Cloud Storage backend.
    Gcs,
    /// Azure Blob Storage backend.
    Azure,
}

/// Serialize detail into the deterministic JSON string used as the audit hash preimage.
#[must_use]
pub fn audit_detail_canonical_json(detail: &AuditDetail) -> String {
    serde_jcs::to_string(detail).expect("AuditDetail is always JSON-serializable")
}

#[cfg(test)]
mod tests {
    use super::{
        AuditDetail, AuditDetailValueError, AuditErrorCode, BatchId, QueryAuditDigest,
        QueryExecutionMode, ScopeHash, StorageAuditOperation, StoragePath,
        audit_detail_canonical_json,
    };
    use crate::auth::{PrincipalId, PrincipalKindTag};
    use crate::origin::{CommitSha, Origin};
    use crate::request_id::RequestId;
    use crate::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostTableName};
    use crate::vala::api::{QueryClass, VisibilityMode};

    #[test]
    fn canonical_json_is_compact_and_stable() {
        let detail = AuditDetail::Storage {
            operation: StorageAuditOperation::BackendFailed,
            upload_id: None,
            storage_path: StoragePath::new("cards/a").expect("valid path"),
            backend: super::StorageBackend::S3,
            status_code: 503,
            error_code: Some(AuditErrorCode::StorageBackendFailure),
        };
        assert_eq!(
            audit_detail_canonical_json(&detail),
            r#"{"backend":"s3","error_code":"STORAGE_BACKEND_FAILURE","kind":"storage","operation":"backend_failed","status_code":503,"storage_path":"cards/a","upload_id":null}"#
        );
        assert!(!audit_detail_canonical_json(&detail).contains(' '));
    }

    /// Pins the scrubbed recovery aggregate as a closed canonical contract.
    #[test]
    fn oracle_admission_recovery_is_bounded_and_canonical() {
        let detail = AuditDetail::OracleAdmissionRecovery {
            expired_lease_count: 2,
            active_lease_count: 3,
            interactive_slots: 5,
            analytical_slots: 8,
            total_slots: 13,
        };
        let canonical = audit_detail_canonical_json(&detail);
        assert_eq!(
            canonical,
            r#"{"active_lease_count":3,"analytical_slots":8,"expired_lease_count":2,"interactive_slots":5,"kind":"oracle_admission_recovery","total_slots":13}"#
        );
        assert_eq!(
            serde_json::from_str::<AuditDetail>(&canonical).expect("deserialize recovery detail"),
            detail
        );
        assert_eq!(
            serde_json::to_value(&detail)
                .expect("serialize recovery detail")
                .as_object()
                .expect("recovery detail object")
                .len(),
            6
        );
    }

    /// Preserves the live replacement audit shape across its persisted JSON boundary.
    #[test]
    fn forge_iceberg_rewrite_round_trips_with_explicit_plan_base() {
        let detail = AuditDetail::ForgeIcebergRewrite {
            operation_id: uuid::Uuid::from_u128(7),
            phase: super::ForgeIcebergRewritePhase::Prepared,
            group: "bifrost://tenant/vala/table".to_owned(),
            base_snapshot_id: 41,
            committed_snapshot_id: None,
            partition_spec_id: 3,
            partition_day: "2026-09-01".to_owned(),
            target_file_size_bytes: 1024,
            input_paths: vec![StoragePath::new("table/live-a.parquet").expect("valid input")],
            output_paths: vec![StoragePath::new("table/rewrite-a.parquet").expect("valid output")],
            writer_recipe_version: "bifrost-writer-v1".to_owned(),
        };
        let json = serde_json::to_value(&detail).expect("serialize detail");
        assert_eq!(json["kind"], "forge_iceberg_rewrite");
        assert_eq!(json["base_snapshot_id"], 41);
        assert_eq!(
            serde_json::from_value::<AuditDetail>(json).expect("deserialize detail"),
            detail
        );
    }

    #[test]
    fn all_detail_variants_round_trip() {
        let origin = Origin {
            repo: "github.com/wyrd-ai/wyrd".to_string(),
            commit: CommitSha::new("0123456").expect("valid commit"),
            path: Some("cards/agent.yaml".to_string()),
            dirty: false,
        };
        let detail = AuditDetail::Ingest {
            origin,
            batch_id: BatchId::new("batch-1").expect("valid batch id"),
            table: BifrostTableName::new("vala.events"),
            record_count: 2,
            decision: AuditDecision::Allow,
        };
        let value = serde_json::to_value(&detail).expect("serialize");
        let back: AuditDetail = serde_json::from_value(value).expect("deserialize");
        assert_eq!(detail, back);
        let _ = PrincipalId::new(uuid::Uuid::now_v7());
    }

    #[test]
    fn constructors_normalize_and_reject_secret_like_values() {
        assert_eq!(
            StoragePath::new("  cards/a  ")
                .expect("valid path")
                .as_str(),
            "cards/a"
        );
        assert!(matches!(
            ScopeHash::new("Bearer very-secret"),
            Err(AuditDetailValueError::SecretLike {
                field: "scope_hash"
            })
        ));
        assert!(matches!(
            StoragePath::new("/var/run/secrets/wyrd/api-key"),
            Err(AuditDetailValueError::SecretLike {
                field: "storage_path"
            })
        ));
        assert!(matches!(
            BatchId::new("token=plaintext"),
            Err(AuditDetailValueError::SecretLike { field: "batch_id" })
        ));
    }

    /// Bifrost read details serialize only digests and enforce topology bounds.
    #[test]
    fn bifrost_read_detail_is_scrubbed_and_bounded() {
        let digest = || QueryAuditDigest::new("sha256:abc").expect("valid digest");
        let detail = AuditDetail::BifrostQueryReadDecision {
            query_digest: digest(),
            query_class: QueryClass::Interactive,
            visibility: VisibilityMode::PublishedOnly,
            binding_digests: vec![digest()],
            snapshot_digest: digest(),
            manifest_digest: digest(),
            projection_digest: digest(),
            permission_digest: digest(),
            execution: QueryExecutionMode::Local,
            selected_node_count: 1,
            worker_count: 0,
            slot_units: 1,
            retry_ordinal: 0,
            deadline_ms: 100,
        };
        detail.validate().expect("bounded detail validates");
        let json = audit_detail_canonical_json(&detail);
        assert!(!json.contains("SELECT"));
        assert!(!json.contains("/var/"));

        let invalid = AuditDetail::BifrostQueryReadDecision {
            query_digest: digest(),
            query_class: QueryClass::Interactive,
            visibility: VisibilityMode::PublishedOnly,
            binding_digests: (0..65).map(|_| digest()).collect(),
            snapshot_digest: digest(),
            manifest_digest: digest(),
            projection_digest: digest(),
            permission_digest: digest(),
            execution: QueryExecutionMode::Local,
            selected_node_count: 1,
            worker_count: 0,
            slot_units: 1,
            retry_ordinal: 0,
            deadline_ms: 100,
        };
        assert!(matches!(
            invalid.validate(),
            Err(AuditDetailValueError::InvalidCombination {
                invariant: "binding count"
            })
        ));

        let duplicate = AuditDetail::BifrostQueryReadDecision {
            query_digest: digest(),
            query_class: QueryClass::Interactive,
            visibility: VisibilityMode::PublishedOnly,
            binding_digests: vec![digest(), digest()],
            snapshot_digest: digest(),
            manifest_digest: digest(),
            projection_digest: digest(),
            permission_digest: digest(),
            execution: QueryExecutionMode::Local,
            selected_node_count: 1,
            worker_count: 0,
            slot_units: 1,
            retry_ordinal: 0,
            deadline_ms: 100,
        };
        assert!(matches!(
            duplicate.validate(),
            Err(AuditDetailValueError::InvalidCombination {
                invariant: "binding digest order"
            })
        ));

        let unsorted = AuditDetail::BifrostQueryReadDecision {
            query_digest: digest(),
            query_class: QueryClass::Interactive,
            visibility: VisibilityMode::PublishedOnly,
            binding_digests: vec![
                QueryAuditDigest::new("sha256:z").expect("valid digest"),
                QueryAuditDigest::new("sha256:a").expect("valid digest"),
            ],
            snapshot_digest: digest(),
            manifest_digest: digest(),
            projection_digest: digest(),
            permission_digest: digest(),
            execution: QueryExecutionMode::Local,
            selected_node_count: 1,
            worker_count: 0,
            slot_units: 1,
            retry_ordinal: 0,
            deadline_ms: 100,
        };
        assert!(unsorted.validate().is_err());
    }

    #[test]
    fn serde_cannot_bypass_secret_classification() {
        for (field, value) in [
            ("scope_hash", serde_json::json!("api_key=plaintext")),
            ("storage_path", serde_json::json!("Bearer plaintext")),
            ("batch_id", serde_json::json!("password=plaintext")),
        ] {
            let json = match field {
                "scope_hash" => serde_json::json!({
                    "kind": "card_scope_mint",
                    "mint_kind": "refresh",
                    "root_card_ref": {"kind": "Agent", "name": "worker", "version": "1.0.0", "space": "prod"},
                    "scope_hash": value,
                    "scope_members": []
                }),
                "storage_path" => serde_json::json!({
                    "kind": "storage",
                    "operation": "complete",
                    "storage_path": value,
                    "backend": "s3",
                    "status_code": 200
                }),
                _ => serde_json::json!({
                    "kind": "ingest",
                    "origin": {"repo": "github.com/wyrd-ai/wyrd", "commit": "0123456", "dirty": false},
                    "batch_id": value,
                    "table": "vala.events",
                    "record_count": 1,
                    "decision": "allow"
                }),
            };
            assert!(
                serde_json::from_value::<AuditDetail>(json).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn audit_detail_golden_vectors_cover_all_variants_and_nested_values() {
        let vectors = [
            (
                r#"{"expires_at":"2026-01-02T03:04:05Z","kind":"credential_issuance","target_principal_id":"00000000-0000-0000-0000-000000000001","api_key_id":"00000000-0000-0000-0000-000000000002"}"#,
                r#"{"api_key_id":"00000000-0000-0000-0000-000000000002","expires_at":"2026-01-02T03:04:05Z","kind":"credential_issuance","target_principal_id":"00000000-0000-0000-0000-000000000001"}"#,
            ),
            (
                r#"{"kind":"token_exchange","expires_at":"2026-01-02T03:04:05Z","delegation_chain":[{"space":"prod","version":"1.0.0","name":"worker","kind":"Agent"}],"actor_principal_id":"00000000-0000-0000-0000-000000000001","subject_principal_id":"00000000-0000-0000-0000-000000000002"}"#,
                r#"{"actor_principal_id":"00000000-0000-0000-0000-000000000001","delegation_chain":[{"kind":"Agent","name":"worker","space":"prod","version":"1.0.0"}],"expires_at":"2026-01-02T03:04:05Z","kind":"token_exchange","subject_principal_id":"00000000-0000-0000-0000-000000000002"}"#,
            ),
            (
                r#"{"deny_reason":"PERMISSION_DENIED","delegation_chain":[],"decision":"deny","callee_principal_id":"00000000-0000-0000-0000-000000000002","kind":"authz_check","caller_principal_id":"00000000-0000-0000-0000-000000000001"}"#,
                r#"{"callee_principal_id":"00000000-0000-0000-0000-000000000002","caller_principal_id":"00000000-0000-0000-0000-000000000001","decision":"deny","delegation_chain":[],"deny_reason":"PERMISSION_DENIED","kind":"authz_check"}"#,
            ),
            (
                r#"{"after_spec_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","card_uid":"01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00","operation":"register","kind":"card_registration","card_kind":"Agent","outcome":"created","before_spec_hash":null}"#,
                r#"{"after_spec_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","before_spec_hash":null,"card_kind":"Agent","card_uid":"01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00","kind":"card_registration","operation":"register","outcome":"created"}"#,
            ),
            (
                r#"{"scope_members":[{"kind":"Agent","name":"worker","version":"1.0.0","space":"prod"}],"root_card_ref":{"space":"prod","name":"root","version":"1.0.0","kind":"Agent"},"scope_hash":"scope-digest","mint_kind":"refresh","kind":"card_scope_mint","scope_member_count":1,"failure_code":null}"#,
                r#"{"failure_code":null,"kind":"card_scope_mint","mint_kind":"refresh","root_card_ref":{"kind":"Agent","name":"root","space":"prod","version":"1.0.0"},"scope_hash":"scope-digest","scope_member_count":1,"scope_members":[{"kind":"Agent","name":"worker","space":"prod","version":"1.0.0"}]}"#,
            ),
            (
                r#"{"status_code":200,"storage_path":" cards/a ","backend":"s3","operation":"complete","kind":"storage","upload_id":null,"error_code":null}"#,
                r#"{"backend":"s3","error_code":null,"kind":"storage","operation":"complete","status_code":200,"storage_path":"cards/a","upload_id":null}"#,
            ),
            (
                r#"{"record_count":2,"table":"vala.events","decision":"allow","batch_id":"batch-1","origin":{"dirty":false,"path":"cards/agent.yaml","commit":"0123456","repo":"github.com/wyrd-ai/wyrd"},"kind":"ingest"}"#,
                r#"{"batch_id":"batch-1","decision":"allow","kind":"ingest","origin":{"commit":"0123456","path":"cards/agent.yaml","repo":"github.com/wyrd-ai/wyrd"},"record_count":2,"table":"vala.events"}"#,
            ),
        ];

        for (input, expected) in vectors {
            let detail: AuditDetail = serde_json::from_str(input).expect("golden input");
            assert_eq!(audit_detail_canonical_json(&detail), expected);
        }
    }

    #[test]
    fn audit_event_golden_shape_distinguishes_absent_and_present_detail() {
        let event = AuditEvent::new(
            RequestId::now_v7(),
            None,
            "bifrost.ingest".to_owned(),
            "vala.events".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::nil()),
            PrincipalKindTag::User,
            AuthMethod::Internal,
            "bifrost:write".to_owned(),
            AuditDecision::Allow,
            AuditResult::Success,
            "accepted".to_owned(),
        );
        let absent = serde_json::to_value(&event).expect("serialize absent detail");
        assert!(absent.get("detail").is_none());

        let present = event.with_detail(AuditDetail::Storage {
            operation: StorageAuditOperation::Complete,
            upload_id: None,
            storage_path: StoragePath::new("cards/a").expect("valid path"),
            backend: super::StorageBackend::S3,
            status_code: 200,
            error_code: None,
        });
        assert_eq!(
            serde_json::to_value(present).expect("serialize present detail")["detail"]["kind"],
            "storage"
        );
    }
}

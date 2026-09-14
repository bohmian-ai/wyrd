//! Typed, redacted detail carried by the transactional audit event.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::auth::{PrincipalId, PrincipalKindTag};
use crate::envelope::{CardKind, SpecHash};
use crate::ids::CardUid;
use crate::origin::Origin;
use crate::reference::CardRef;
use crate::vala::api::AuditOutcome;
use crate::vala::api::{QueryClass, TimePartitionWire, VisibilityMode};

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
    /// The value does not match the closed canonical form for its field.
    #[error("{field} is not in its canonical form")]
    Malformed {
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

/// Domain separator prefixed to every Scribe-promotion digest preimage.
///
/// It binds the digest to this exact algorithm and field order so a byte
/// stream produced for any other Wyrd purpose can never collide with a
/// promoted-file-set identity.
const SCRIBE_PROMOTION_DIGEST_DOMAIN: &str = "wyrd.forge.scribe-promotion.v1";

/// Stable, non-secret digest identifying the exact ordered file set promoted by
/// one Forge Scribe-promotion operation.
///
/// The only representable form is the literal `sha256:` prefix followed by
/// exactly 64 lowercase hexadecimal characters. Prepared and terminal audit
/// rows carry the same value, so a reader can prove that a committed promotion
/// moved precisely the file set that was prepared.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct ForgePromotedFileSetDigest(
    /// Canonical `sha256:`-prefixed lowercase hexadecimal digest.
    String,
);

impl ForgePromotedFileSetDigest {
    /// Byte length of the canonical `sha256:<64 hex>` representation.
    const CANONICAL_LEN: usize = 7 + 64;

    /// Accepts an already-formed digest in its exact canonical form.
    ///
    /// No normalization is applied: an uppercase, unprefixed, truncated, or
    /// differently labelled value is a caller error rather than something to
    /// repair, because a repaired digest would silently claim an identity the
    /// caller never computed.
    ///
    /// # Errors
    /// Returns [`AuditDetailValueError::Malformed`] when the value is not
    /// `sha256:` followed by 64 lowercase hexadecimal characters.
    pub fn new(value: impl Into<String>) -> Result<Self, AuditDetailValueError> {
        let value = value.into();
        let Some(body) = value.strip_prefix("sha256:") else {
            return Err(AuditDetailValueError::Malformed {
                field: "promoted_file_set_digest",
            });
        };
        if value.len() != Self::CANONICAL_LEN
            || !body
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AuditDetailValueError::Malformed {
                field: "promoted_file_set_digest",
            });
        }
        Ok(Self(value))
    }

    /// Computes the digest over one ordered promoted-file set.
    ///
    /// The preimage is the domain separator followed, in the caller's promoted
    /// order, by each file's lowercase hyphenated file-list UUID, canonical
    /// logical path, and normalized checksum. Every field is prefixed by its
    /// unsigned 64-bit big-endian UTF-8 byte length so no field boundary can be
    /// shifted without changing the digest, and the whole stream is hashed with
    /// SHA-256. Order is significant: the promoted order is part of the
    /// identity, so callers must not sort here.
    #[must_use]
    pub fn compute(files: &[ForgePromotedFile]) -> Self {
        use sha2::Digest as _;

        fn framed(hasher: &mut sha2::Sha256, field: &str) {
            hasher.update((field.len() as u64).to_be_bytes());
            hasher.update(field.as_bytes());
        }

        let mut hasher = sha2::Sha256::new();
        framed(&mut hasher, SCRIBE_PROMOTION_DIGEST_DOMAIN);
        for file in files {
            framed(&mut hasher, &file.file_id.hyphenated().to_string());
            framed(&mut hasher, file.path.as_str());
            framed(&mut hasher, &file.checksum);
        }
        Self(format!("sha256:{}", hex::encode(hasher.finalize())))
    }

    /// Borrows the canonical digest text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ForgePromotedFileSetDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ForgePromotedFileSetDigest> for String {
    fn from(value: ForgePromotedFileSetDigest) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for ForgePromotedFileSetDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// One promoted hot object contributing a tuple to the promoted-file-set digest.
///
/// This is a pure domain value: it normalizes and validates its own fields at
/// construction so [`ForgePromotedFileSetDigest::compute`] can hash them
/// without re-deciding what "normalized" means. It carries no row payload,
/// SQL, credential, or secret.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForgePromotedFile {
    /// Durable `file_list` identity of the promoted hot object.
    file_id: uuid::Uuid,
    /// Canonical logical path of the promoted object.
    path: StoragePath,
    /// Normalized lowercase checksum recorded for the promoted object.
    checksum: String,
}

impl ForgePromotedFile {
    /// Constructs one validated promoted-file tuple.
    ///
    /// The checksum is normalized by trimming surrounding whitespace and
    /// lowercasing ASCII, because publication and revalidation read it from
    /// different producers and the digest must not depend on which one wrote
    /// it. The path is already canonical by [`StoragePath`]'s own contract.
    ///
    /// # Errors
    /// Returns [`AuditDetailValueError::Empty`] when the checksum is empty
    /// after normalization and [`AuditDetailValueError::ControlCharacter`] when
    /// it contains a control character.
    pub fn new(
        file_id: uuid::Uuid,
        path: StoragePath,
        checksum: impl Into<String>,
    ) -> Result<Self, AuditDetailValueError> {
        let checksum = checksum.into();
        let checksum = checksum.trim().to_ascii_lowercase();
        if checksum.is_empty() {
            return Err(AuditDetailValueError::Empty { field: "checksum" });
        }
        if checksum.chars().any(char::is_control) {
            return Err(AuditDetailValueError::ControlCharacter { field: "checksum" });
        }
        Ok(Self {
            file_id,
            path,
            checksum,
        })
    }

    /// Returns the durable `file_list` identity.
    #[must_use]
    pub const fn file_id(&self) -> uuid::Uuid {
        self.file_id
    }

    /// Borrows the canonical logical path.
    #[must_use]
    pub const fn path(&self) -> &StoragePath {
        &self.path
    }

    /// Borrows the normalized checksum.
    #[must_use]
    pub fn checksum(&self) -> &str {
        &self.checksum
    }
}

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
    /// Peer assignment-authority digest mismatch: the recomputed digest over
    /// the follower's actual dispatched assignments does not match the
    /// digest signed into the ticket claims, so the closed predicate and
    /// projection closure cannot be trusted.
    PeerAssignmentAuthority,
    /// Private stage-operation binding mismatch: a signed Analytical stage
    /// ticket did not match the receiving follower's own expectation for the
    /// operation, either query identity, the pinned snapshot, the stage, the
    /// task, the attempt, the reservation, or the authorized permissions, so
    /// the operation was refused before any plan decode, task-cache lookup,
    /// provider construction, or object I/O.
    PeerStageBinding,
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
    /// Audit staging was unavailable.
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

/// One verified delegation hop projected into a durable audit record.
///
/// The projection is deliberately identity-only: a delegation chain reaches
/// audit from a token the verifier already accepted, so the record needs the
/// delegator's identity and the card authority it was acting under, never the
/// credential that carried them. Steps are stored in the same initiator-first
/// order the verifier produced, because the order is the delegation, not an
/// incidental encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AuditDelegationStep {
    /// Stable identity of the delegating principal.
    pub principal_id: PrincipalId,
    /// Card-free kind discriminator of the delegating principal.
    pub principal_kind: PrincipalKindTag,
    /// Card bound to the delegator; absent for a `User` principal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<CardRef>,
    /// Transitive card authorization the delegator acted under; empty for a
    /// `User` principal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub card_ref_scope: Vec<CardRef>,
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
        /// Verified initiator-first delegation chain, empty when the caller
        /// presented no `act` claim. Omitted from the canonical encoding when
        /// empty so records written before delegation attribution keep their
        /// original bytes and hash.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        delegation_chain: Vec<AuditDelegationStep>,
    },
    /// Scrubbed tenant or peer security violation.
    BifrostSecurityViolation {
        /// Closed violation class.
        violation: BifrostSecurityViolationKind,
        /// Boundary where validation failed.
        phase: BifrostSecurityPhase,
        /// Trusted query digest when one exists.
        query_digest: Option<QueryAuditDigest>,
        /// Verified initiator-first delegation chain, empty and omitted when
        /// the refused caller presented no `act` claim.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        delegation_chain: Vec<AuditDelegationStep>,
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
        /// Shared exact time partition for every input in the rewrite group.
        time_partition: TimePartitionWire,
        /// Target output size captured from the table metadata.
        target_file_size_bytes: u64,
        /// Exact ordered catalog paths deleted by the Iceberg action.
        input_paths: Vec<StoragePath>,
        /// Exact ordered rewritten object paths added by the Iceberg action.
        output_paths: Vec<StoragePath>,
    },
    /// A Forge Scribe-promotion operation and its Iceberg catalog boundary.
    ///
    /// Promotion appends already-published hot objects unchanged, so the row
    /// records exactly which `file_list` rows and object paths moved, the
    /// snapshot the append started from, the snapshot it produced when known,
    /// and the digest binding that ordered file set to this operation.
    ForgeScribePromotion {
        /// Deterministic identifier shared by prepared and terminal rows.
        operation_id: uuid::Uuid,
        /// Durable phase represented by this audit row.
        phase: ForgeScribePromotionPhase,
        /// Canonical tenant/table resource identity.
        group: String,
        /// Snapshot observed before the promotion append.
        base_snapshot_id: i64,
        /// Snapshot returned by a proven promotion commit, when known.
        committed_snapshot_id: Option<i64>,
        /// Exact ordered `file_list` rows promoted by the operation.
        input_file_ids: Vec<uuid::Uuid>,
        /// Exact ordered logical object paths promoted by the operation.
        input_paths: Vec<StoragePath>,
        /// Digest binding the ordered promoted file set to this operation.
        promoted_file_set_digest: ForgePromotedFileSetDigest,
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
    /// A Forge metadata-only manifest rewrite and its Iceberg boundary.
    ///
    /// The operation regroups manifest entries and nothing else: the data files
    /// the table logically contains are identical before and after it. Both
    /// path collections are exact and ordered, so a reader can prove which
    /// manifests were replaced by which without consulting the catalog.
    ForgeManifestRewrite {
        /// Deterministic identifier shared by prepared and terminal rows.
        operation_id: uuid::Uuid,
        /// Durable phase represented by this audit row.
        phase: ForgeManifestRewritePhase,
        /// Canonical tenant/table resource identity.
        group: String,
        /// Metadata location observed before the rewrite commit.
        base_metadata_location: StoragePath,
        /// Metadata location returned by a proven commit, when known.
        committed_metadata_location: Option<StoragePath>,
        /// Exact ordered manifests replaced by the rewrite.
        input_manifest_paths: Vec<StoragePath>,
        /// Exact ordered manifests written by the rewrite.
        output_manifest_paths: Vec<StoragePath>,
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
        /// Authorization outcome.
        outcome: AuditOutcome,
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
    /// Delegation attribution for an audited operation that carries no
    /// operation-specific detail of its own.
    ///
    /// This variant exists so an RBAC denial or other detail-free event can
    /// still name who was acting for whom. It never replaces an
    /// operation-specific detail: an event that already has one carries its
    /// delegation inside that detail instead.
    DelegationAttribution {
        /// Verified initiator-first delegation chain.
        delegation_chain: Vec<AuditDelegationStep>,
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
        /// Ingest authorization outcome.
        outcome: AuditOutcome,
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

/// Durable phase recorded for a Forge Scribe-promotion operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ForgeScribePromotionPhase {
    /// The exact promoted file set was fixed before the catalog append.
    Prepared,
    /// The catalog append completed and returned a promotion snapshot.
    Committed,
    /// Reconciliation proved a previously uncertain append had completed.
    Recovered,
    /// Reconciliation proved the prepared append was not committed.
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

/// Durable phase recorded for a Forge metadata-only manifest rewrite.
///
/// The rewrite writes new manifests and then swaps them in with one catalog
/// commit, so it has the same recoverable external boundary as data rewrite:
/// a prepared operation whose commit outcome is unknown is settled by
/// reconciliation into exactly one of `Recovered` or `Reset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ForgeManifestRewritePhase {
    /// The exact input and output manifests were fixed before the commit.
    Prepared,
    /// The catalog commit completed and returned a metadata location.
    Committed,
    /// Reconciliation proved a previously uncertain commit completed.
    Recovered,
    /// Reconciliation proved the prepared commit was not applied.
    Reset,
}

/// Durable transition recorded for one epoch's per-table reader protection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OracleTableProtectionPhase {
    /// First protection for the table, or a conservative widening of it.
    Expanded,
    /// A narrowing that still protects every remaining active cut.
    Narrowed,
    /// The last active cut ended and the protection header was removed.
    Released,
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
    use crate::vala::api::{AuditEvent, AuditOutcome, BifrostTableName};
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
            time_partition: crate::vala::api::TimePartitionWire::new(
                crate::vala::api::TimeGranularityWire::Hour,
                chrono::DateTime::from_timestamp(1_767_312_000, 0).expect("fixture instant"),
            )
            .expect("fixture instant is an exact hour boundary"),
            target_file_size_bytes: 1024,
            input_paths: vec![StoragePath::new("table/live-a.parquet").expect("valid input")],
            output_paths: vec![StoragePath::new("table/rewrite-a.parquet").expect("valid output")],
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
            repo: "github.com/bohmian-ai/wyrd".to_string(),
            commit: CommitSha::new("0123456").expect("valid commit"),
            path: Some("cards/agent.yaml".to_string()),
            dirty: false,
        };
        let detail = AuditDetail::Ingest {
            origin,
            batch_id: BatchId::new("batch-1").expect("valid batch id"),
            table: BifrostTableName::new("vala.events"),
            record_count: 2,
            outcome: AuditOutcome::Allowed,
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

    /// Delegation attribution is hash-covered when present and invisible when
    /// absent.
    ///
    /// The two halves are one claim: an audit chain can only stay verifiable
    /// across this change if a record written without delegation keeps the
    /// exact bytes it had before the field existed, while a delegated record
    /// carries the chain inside the same canonical detail the entry hash covers.
    #[test]
    fn delegation_attribution_is_omitted_when_absent_and_hashed_when_present() {
        let digest = || QueryAuditDigest::new("sha256:abc").expect("valid digest");
        let read = |delegation_chain: Vec<super::AuditDelegationStep>| {
            AuditDetail::BifrostQueryReadDecision {
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
                delegation_chain,
            }
        };

        let historical = audit_detail_canonical_json(&read(Vec::new()));
        assert!(
            !historical.contains("delegation_chain"),
            "a nondelegated record keeps its original encoding: {historical}"
        );

        let step = super::AuditDelegationStep {
            principal_id: PrincipalId::new(uuid::Uuid::nil()),
            principal_kind: PrincipalKindTag::Service,
            card_ref: None,
            card_ref_scope: Vec::new(),
        };
        let delegated = audit_detail_canonical_json(&read(vec![step.clone()]));
        assert_ne!(
            delegated, historical,
            "delegation changes the canonical detail the entry hash covers"
        );
        assert!(delegated.contains("delegation_chain"));
        assert_ne!(
            audit_detail_canonical_json(&read(vec![step.clone(), step.clone()])),
            delegated,
            "a longer chain is a different canonical record"
        );

        // Order is the delegation, so reversing it must change the record.
        let other = super::AuditDelegationStep {
            principal_id: PrincipalId::new(uuid::Uuid::max()),
            ..step.clone()
        };
        assert_ne!(
            audit_detail_canonical_json(&read(vec![step.clone(), other.clone()])),
            audit_detail_canonical_json(&read(vec![other, step])),
            "initiator-first order survives canonicalization"
        );
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
            delegation_chain: Vec::new(),
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
            delegation_chain: Vec::new(),
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
            delegation_chain: Vec::new(),
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
            delegation_chain: Vec::new(),
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
                    "origin": {"repo": "github.com/bohmian-ai/wyrd", "commit": "0123456", "dirty": false},
                    "batch_id": value,
                    "table": "vala.events",
                    "record_count": 1,
                    "outcome": "allowed"
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
                r#"{"deny_reason":"PERMISSION_DENIED","delegation_chain":[],"outcome":"denied","callee_principal_id":"00000000-0000-0000-0000-000000000002","kind":"authz_check","caller_principal_id":"00000000-0000-0000-0000-000000000001"}"#,
                r#"{"callee_principal_id":"00000000-0000-0000-0000-000000000002","caller_principal_id":"00000000-0000-0000-0000-000000000001","delegation_chain":[],"deny_reason":"PERMISSION_DENIED","kind":"authz_check","outcome":"denied"}"#,
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
                r#"{"record_count":2,"table":"vala.events","outcome":"allowed","batch_id":"batch-1","origin":{"dirty":false,"path":"cards/agent.yaml","commit":"0123456","repo":"github.com/bohmian-ai/wyrd"},"kind":"ingest"}"#,
                r#"{"batch_id":"batch-1","kind":"ingest","origin":{"commit":"0123456","path":"cards/agent.yaml","repo":"github.com/bohmian-ai/wyrd"},"outcome":"allowed","record_count":2,"table":"vala.events"}"#,
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
            "bifrost:write".to_owned(),
            AuditOutcome::Allowed,
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

    /// The promoted-file-set digest newtype admits only the locked canonical
    /// form: the literal `sha256:` prefix followed by exactly 64 lowercase
    /// hexadecimal characters.
    ///
    /// Every other shape a caller could plausibly produce — an uppercase
    /// digest, a bare hex digest, a different algorithm label, a truncated or
    /// over-long body, or a non-hexadecimal character — must be refused at the
    /// constructor so no audit row can carry an unverifiable promotion
    /// identity.
    #[test]
    fn forge_promoted_file_set_digest_accepts_only_canonical_sha256() {
        let canonical = format!("sha256:{}", "ab12cd34".repeat(8));
        assert_eq!(canonical.len(), 7 + 64);
        let digest = super::ForgePromotedFileSetDigest::new(&canonical).expect("canonical digest");
        assert_eq!(digest.as_str(), canonical);

        for rejected in [
            canonical.to_ascii_uppercase(),
            format!("SHA256:{}", "ab12cd34".repeat(8)),
            "ab12cd34".repeat(8),
            format!("sha512:{}", "ab12cd34".repeat(8)),
            format!("sha256:{}", "ab12cd34".repeat(7)),
            format!("sha256:{}0", "ab12cd34".repeat(8)),
            format!("sha256:{}zz", "ab12cd34".repeat(7) + "ab12cd"),
            "sha256:".to_owned(),
            String::new(),
        ] {
            assert!(
                super::ForgePromotedFileSetDigest::new(&rejected).is_err(),
                "digest newtype accepted non-canonical value `{rejected}`"
            );
        }
    }

    /// The digest preimage is exactly the locked byte stream: the
    /// `wyrd.forge.scribe-promotion.v1` domain separator followed, in promoted
    /// order, by each tuple's lowercase hyphenated file-list UUID, canonical
    /// logical path, and normalized checksum, every field prefixed by its
    /// unsigned 64-bit big-endian UTF-8 byte length.
    ///
    /// The independent recomputation below pins the separator, the field
    /// order, and the framing. The mutations then prove sensitivity: reordering
    /// the tuples changes the digest, and shifting one byte across a field
    /// boundary changes it too, which unframed concatenation could not detect.
    #[test]
    fn forge_promoted_file_set_digest_is_ordered_length_framed_and_domain_separated() {
        use sha2::Digest as _;

        fn expected(files: &[super::ForgePromotedFile]) -> String {
            fn framed(hasher: &mut sha2::Sha256, field: &str) {
                hasher.update((field.len() as u64).to_be_bytes());
                hasher.update(field.as_bytes());
            }
            let mut hasher = sha2::Sha256::new();
            framed(&mut hasher, "wyrd.forge.scribe-promotion.v1");
            for file in files {
                framed(&mut hasher, &file.file_id().hyphenated().to_string());
                framed(&mut hasher, file.path().as_str());
                framed(&mut hasher, file.checksum());
            }
            format!("sha256:{}", hex::encode(hasher.finalize()))
        }

        let first = super::ForgePromotedFile::new(
            uuid::Uuid::from_u128(1),
            StoragePath::new("tenant/table/data/a.parquet").expect("path"),
            "AB12",
        )
        .expect("first promoted file");
        let second = super::ForgePromotedFile::new(
            uuid::Uuid::from_u128(2),
            StoragePath::new("tenant/table/data/b.parquet").expect("path"),
            "cd34",
        )
        .expect("second promoted file");

        assert_eq!(first.checksum(), "ab12", "checksum must be normalized");

        let ordered = [first.clone(), second.clone()];
        let digest = super::ForgePromotedFileSetDigest::compute(&ordered);
        assert_eq!(digest.as_str(), expected(&ordered));

        let reversed = [second, first.clone()];
        assert_ne!(
            super::ForgePromotedFileSetDigest::compute(&reversed).as_str(),
            digest.as_str(),
            "digest must depend on promoted order"
        );

        let shifted = [
            first,
            super::ForgePromotedFile::new(
                uuid::Uuid::from_u128(2),
                StoragePath::new("tenant/table/data/b.parquetc").expect("path"),
                "d34",
            )
            .expect("boundary-shifted promoted file"),
        ];
        assert_ne!(
            super::ForgePromotedFileSetDigest::compute(&shifted).as_str(),
            super::ForgePromotedFileSetDigest::compute(&ordered).as_str(),
            "length framing must separate adjacent fields"
        );
    }

    /// The Scribe-promotion audit detail serializes under the locked
    /// `forge_scribe_promotion` kind, carries exactly the eight locked fields,
    /// round-trips without loss, and refuses an unknown phase.
    #[test]
    fn forge_scribe_promotion_audit_detail_round_trips_exactly() {
        let promoted = [super::ForgePromotedFile::new(
            uuid::Uuid::from_u128(7),
            StoragePath::new("tenant/table/data/a.parquet").expect("path"),
            "ab12",
        )
        .expect("promoted file")];
        let detail = AuditDetail::ForgeScribePromotion {
            operation_id: uuid::Uuid::from_u128(11),
            phase: super::ForgeScribePromotionPhase::Committed,
            group: "tenant/00000000-0000-0000-0000-000000000001/table/events".to_owned(),
            base_snapshot_id: 41,
            committed_snapshot_id: Some(42),
            input_file_ids: vec![uuid::Uuid::from_u128(7)],
            input_paths: vec![StoragePath::new("tenant/table/data/a.parquet").expect("path")],
            promoted_file_set_digest: super::ForgePromotedFileSetDigest::compute(&promoted),
        };
        detail.validate().expect("promotion detail is valid");

        let value = serde_json::to_value(&detail).expect("serialize promotion detail");
        assert_eq!(value["kind"], "forge_scribe_promotion");
        assert_eq!(value["phase"], "committed");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object detail")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "base_snapshot_id",
                "committed_snapshot_id",
                "group",
                "input_file_ids",
                "input_paths",
                "kind",
                "operation_id",
                "phase",
                "promoted_file_set_digest",
            ]
        );

        let round_tripped: AuditDetail =
            serde_json::from_value(value).expect("deserialize promotion detail");
        assert_eq!(round_tripped, detail);
        assert!(!audit_detail_canonical_json(&detail).is_empty());

        for phase in ["prepared", "committed", "recovered", "reset"] {
            let parsed: super::ForgeScribePromotionPhase =
                serde_json::from_value(serde_json::Value::String(phase.to_owned()))
                    .expect("closed phase parses");
            assert_eq!(
                serde_json::to_value(parsed).expect("phase serializes"),
                serde_json::Value::String(phase.to_owned())
            );
        }
        assert!(
            serde_json::from_value::<super::ForgeScribePromotionPhase>(serde_json::Value::String(
                "settled".to_owned()
            ))
            .is_err(),
            "phase set must stay closed"
        );
    }

    /// Previously accepted `forge_manifest_rewrite` audit rows must keep
    /// deserializing even though no production strategy can create new ones.
    ///
    /// The variant is historical wire compatibility, so this proves every
    /// phase serializes under the canonical kind and round-trips exactly.
    #[test]
    fn forge_manifest_rewrite_public_audit_detail_round_trips() {
        for phase in [
            super::ForgeManifestRewritePhase::Prepared,
            super::ForgeManifestRewritePhase::Committed,
            super::ForgeManifestRewritePhase::Recovered,
            super::ForgeManifestRewritePhase::Reset,
        ] {
            let detail = AuditDetail::ForgeManifestRewrite {
                operation_id: uuid::Uuid::from_u128(23),
                phase,
                group: "tenant/00000000-0000-0000-0000-000000000001/table/events".to_owned(),
                base_metadata_location: StoragePath::new("tenant/table/metadata/v1.json")
                    .expect("path"),
                committed_metadata_location: Some(
                    StoragePath::new("tenant/table/metadata/v2.json").expect("path"),
                ),
                input_manifest_paths: vec![
                    StoragePath::new("tenant/table/metadata/m1.avro").expect("path"),
                ],
                output_manifest_paths: vec![
                    StoragePath::new("tenant/table/metadata/m2.avro").expect("path"),
                ],
            };

            let value = serde_json::to_value(&detail).expect("serialize rewrite detail");
            assert_eq!(value["kind"], "forge_manifest_rewrite");

            let round_tripped: AuditDetail =
                serde_json::from_value(value).expect("deserialize rewrite detail");
            assert_eq!(round_tripped, detail);
        }
    }
}

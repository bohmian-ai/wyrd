//! Idempotent INSERT path for `wyrd.cards`.
#![deny(missing_docs)]
// raw-query grep allowlist: register insert/lookup run on a TenantConn (RLS) and
// scope every row to `wyrd.current_tenant()`; the INSERT ON CONFLICT targets the
// partial `cards_identity_unique` index by predicate. Run `mise run sqlx:prepare`
// to promote to macros.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt::Display;
use uuid::Uuid;

use wyrd_runtime::principal::{Principal, PrincipalId};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{ArtifactManifestEntry, RegistrationOperationId};
use wyrd_spec::request_id::RequestId;

use crate::queries::cards::audit::{CardRegistrationAuditInput, record_card_registration_audit};
use crate::queries::cards::auth_projection::{
    lookup_existing_principal_id, upsert_service_account_from_card,
};
use crate::queries::cards::version_resolve::{Resolution, SubmittedCardIdentity, resolve_version};
use crate::queries::cards::version_sql::lock_version_line;
use crate::row_types::cards::CardStatus;
use crate::tenant_conn::TenantConn;
use wyrd_spec::vala::audit_detail::{CardRegistrationOperation, CardRegistrationOutcome};

/// Maximum canonical-JSON byte size for a single `Spec`.
pub const MAX_SPEC_BYTES: usize = 256 * 1024;

fn registry_db_error(error: impl Display) -> WyrdError {
    tracing::error!(error = %error, "card registry database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

struct PreparedCardData<'a> {
    spec_hash: &'a str,
    spec_json: &'a JsonValue,
    labels_json: &'a JsonValue,
    annotations_json: &'a JsonValue,
}

/// Inputs to [`register_card`].
pub struct RegisterCardRequest<'a> {
    /// Source card envelope.
    pub card: &'a Card,
    /// Verified actor for audit and projection writes.
    pub actor: &'a Principal,
    /// Optional correlation id from the originating request.
    pub request_id: Option<&'a RequestId>,
}

/// Classification of the registration outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterCardOutcomeKind {
    /// New row inserted.
    Created,
    /// Existing row matched on `(identity, spec_hash)` for a Pin; no DB write.
    IdempotentNoop,
    /// Auto/scope re-register of content identical to latest-in-line.
    Deduplicated,
}

/// Result of a [`register_card`] call.
#[derive(Debug, Clone)]
pub struct RegisterCardOutcome {
    /// Card UID — newly minted on create, existing on no-op/dedup.
    pub card_uid: CardUid,
    /// Three-way outcome classification.
    pub kind: RegisterCardOutcomeKind,
    /// Durable `auth_service_accounts.id` for Service/Agent cards.
    pub principal_id: Option<PrincipalId>,
}

/// Register a card inside the caller's [`TenantConn`] tx.
///
/// Pins are exact and idempotent. `None`/`Scope` auto-resolve for non-principal
/// kinds under a per-line advisory lock and deduplicate against only the latest
/// stable row in that line.
///
/// # Errors
/// Returns [`WyrdError::RegistryInvalidCardSpec`] when boundary validation fails.
/// Returns [`WyrdError::RegistrySpecDrift`] when a pinned spec hash mismatch is
/// detected. Returns [`WyrdError::RegistryUnavailable`] on transient DB errors.
#[tracing::instrument(
    skip(conn, req),
    fields(
        tenant_id = %conn.data_tenant_id(),
        kind = ?req.card.kind,
        name = %req.card.metadata.name,
    ),
)]
pub async fn register_card(
    conn: &mut TenantConn<'_>,
    req: RegisterCardRequest<'_>,
) -> Result<RegisterCardOutcome, WyrdError> {
    validate_boundary(req.card)?;

    let (spec_hash, canonical_bytes) = req
        .card
        .spec
        .canonical_hash_with_bytes()
        .map_err(WyrdError::from_spec_canonicalization)?;
    if canonical_bytes.len() > MAX_SPEC_BYTES {
        return Err(WyrdError::registry_spec_too_large(
            canonical_bytes.len(),
            MAX_SPEC_BYTES,
        ));
    }

    let spec_json: JsonValue =
        serde_json::to_value(&req.card.spec).map_err(WyrdError::from_spec_serialization)?;
    let labels_json: JsonValue = serde_json::to_value(&req.card.metadata.labels)
        .map_err(WyrdError::from_spec_serialization)?;
    let annotations_json: JsonValue = serde_json::to_value(&req.card.metadata.annotations)
        .map_err(WyrdError::from_spec_serialization)?;

    let space = req
        .card
        .metadata
        .space
        .as_ref()
        .expect("space presence verified in validate_boundary");
    let name = &req.card.metadata.name;
    let data = PreparedCardData {
        spec_hash: spec_hash.as_str(),
        spec_json: &spec_json,
        labels_json: &labels_json,
        annotations_json: &annotations_json,
    };

    match &req.card.metadata.version {
        Some(VersionSpec::Pin(block)) => insert_pin(conn, &req, space, name, block, &data).await,
        other => {
            register_auto(
                conn,
                &req,
                space,
                name,
                other.as_ref(),
                req.card.metadata.bump.as_ref(),
                &data,
            )
            .await
        }
    }
}

/// Reject cards that cannot be registered before touching the database.
///
/// Checks: supported `apiVersion`, presence of `metadata.space`, `kind` /
/// `spec` consistency, non-empty `metadata.version`, and the Service/Agent
/// pin-only rule. All failures map to `WYRD_REG_400_*` errors.
fn validate_boundary(card: &Card) -> Result<(), WyrdError> {
    if card.api_version.as_str() != ApiVersion::V1 {
        return Err(WyrdError::registry_invalid_card_spec(
            "unsupported apiVersion; only wyrd/v1 is registrable",
        ));
    }
    if card.metadata.space.is_none() {
        return Err(WyrdError::registry_invalid_card_spec(
            "metadata.space is required at the registration boundary",
        ));
    }
    if card.kind != card.spec.kind() {
        return Err(WyrdError::registry_invalid_card_spec(
            "card.kind does not match spec variant",
        ));
    }
    match &card.metadata.version {
        Some(v) if v.as_str().is_empty() => {
            return Err(WyrdError::registry_version_required(
                "metadata.version must not be empty",
            ));
        }
        _ => {}
    }
    if matches!(&card.kind, CardKind::Service | CardKind::Agent)
        && !card
            .metadata
            .version
            .as_ref()
            .is_some_and(|v| !v.as_str().is_empty() && v.is_pin())
    {
        return Err(WyrdError::registry_invalid_version_block(
            "Service and Agent cards require a pinned version",
        ));
    }
    Ok(())
}

/// Attempt to register a card at an exact pinned version.
///
/// Tries an INSERT with `ON CONFLICT … DO NOTHING`. If the row already exists
/// (INSERT returned nothing), delegates to [`handle_conflict`] to determine
/// whether the existing row is an idempotent re-apply or a spec-hash drift.
/// For Service and Agent kinds, also projects the service account on first
/// creation.
async fn insert_pin(
    conn: &mut TenantConn<'_>,
    req: &RegisterCardRequest<'_>,
    space: &SpaceName,
    name: &CardName,
    block: &VersionBlock,
    data: &PreparedCardData<'_>,
) -> Result<RegisterCardOutcome, WyrdError> {
    let inserted = insert_legacy_card_row(conn, req, space, name, block, data).await?;

    let uid_for_writes = match inserted {
        Some(uid) => uid,
        None => return handle_conflict(conn, req.card, space, block, data.spec_hash, req).await,
    };

    // Service and Agent cards project a durable service account on first
    // registration so that their identity is immediately usable for auth.
    let principal_id = match &req.card.kind {
        CardKind::Service | CardKind::Agent => {
            let pid = upsert_service_account_from_card(conn, &uid_for_writes, req.card, req.actor)
                .await?;
            Some(pid)
        }
        _ => None,
    };

    record_registration_audit(
        conn,
        req,
        &uid_for_writes,
        data.spec_hash,
        CardRegistrationOutcome::Created,
    )
    .await?;

    Ok(RegisterCardOutcome {
        card_uid: uid_for_writes,
        kind: RegisterCardOutcomeKind::Created,
        principal_id,
    })
}

/// Register a card whose version is `None` (auto) or a `Scope` range.
///
/// Acquires a per-line advisory lock so concurrent callers cannot race to
/// assign the same next version. After locking, calls [`resolve_version`] to
/// determine whether the submitted spec hash matches the latest stable row in
/// the line (content-identical dedup → `Deduplicated`) or to compute the next
/// auto-incremented version (content-changed → `Created`).
///
/// Service and Agent cards are forbidden here — `validate_boundary` rejects
/// them before this point.
async fn register_auto(
    conn: &mut TenantConn<'_>,
    req: &RegisterCardRequest<'_>,
    space: &SpaceName,
    name: &CardName,
    version: Option<&VersionSpec>,
    bump: Option<&VersionBump>,
    data: &PreparedCardData<'_>,
) -> Result<RegisterCardOutcome, WyrdError> {
    debug_assert!(
        !matches!(&req.card.kind, CardKind::Service | CardKind::Agent),
        "Service/Agent are pin-only; validate_boundary forbids them here"
    );

    lock_version_line(conn, req.card.kind.clone(), space, name).await?;

    match resolve_version(
        conn,
        req.card.kind.clone(),
        space,
        name,
        version,
        bump,
        SubmittedCardIdentity {
            spec_hash: data.spec_hash,
            artifact_hash: req.card.metadata.artifact_hash.as_deref(),
        },
    )
    .await?
    {
        Resolution::Deduplicated { version } => {
            let uid = lookup_uid_by_ref(conn, req.card.kind.clone(), space, name, &version).await?;
            record_registration_audit(
                conn,
                req,
                &uid,
                data.spec_hash,
                CardRegistrationOutcome::Deduplicated,
            )
            .await?;
            Ok(RegisterCardOutcome {
                card_uid: uid,
                kind: RegisterCardOutcomeKind::Deduplicated,
                principal_id: None,
            })
        }
        Resolution::Fresh(next) => {
            let uid = insert_legacy_card_row(conn, req, space, name, &next, data)
                .await?
                .ok_or_else(|| {
                    WyrdError::registry_unavailable(
                        "auto-version insert conflicted under advisory lock",
                    )
                })?;
            record_registration_audit(
                conn,
                req,
                &uid,
                data.spec_hash,
                CardRegistrationOutcome::Created,
            )
            .await?;
            Ok(RegisterCardOutcome {
                card_uid: uid,
                kind: RegisterCardOutcomeKind::Created,
                principal_id: None,
            })
        }
    }
}

/// INSERT one row into `wyrd.cards`, returning the new uid on success.
///
/// Uses `ON CONFLICT … WHERE status <> 'deleted' DO NOTHING` so that a
/// concurrent INSERT for the same active identity is silently skipped rather
/// than erroring. Returns `None` when the conflict path fires; the caller is
/// responsible for deciding what to do (idempotent re-apply check vs. advisory
/// lock guarantee).
///
/// Validates that each semver component fits in `i64` before the INSERT, since
/// the generated `version_major/minor/patch` columns are `BIGINT`.
async fn insert_legacy_card_row(
    conn: &mut TenantConn<'_>,
    req: &RegisterCardRequest<'_>,
    space: &SpaceName,
    name: &CardName,
    version: &VersionBlock,
    data: &PreparedCardData<'_>,
) -> Result<Option<CardUid>, WyrdError> {
    let sv = version
        .semver()
        .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?;
    for component in [sv.major, sv.minor, sv.patch] {
        i64::try_from(component).map_err(|_| {
            WyrdError::registry_invalid_version_block("version component exceeds i64::MAX")
        })?;
    }

    let card_uid = CardUid::from_uuid(Uuid::now_v7()).map_err(WyrdError::from_card_uid_error)?;
    let inserted = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO wyrd.cards (
            card_uid, data_tenant_id, kind, space, name, version,
            spec, spec_hash, artifact_hash, labels, annotations,
            status, created_by
        ) VALUES (
            $1, wyrd.current_tenant(), $2, $3, $4, $5,
            $6, $7, $8, $9, $10,
            'active', $11
        )
        ON CONFLICT (data_tenant_id, kind, space, name, version)
            WHERE status NOT IN ('deleted', 'failed', 'expired') DO NOTHING
        RETURNING card_uid
        "#,
    )
    .bind(card_uid.as_uuid())
    .bind(req.card.kind.wire_name())
    .bind(space.as_str())
    .bind(name.as_str())
    .bind(version.as_str())
    .bind(data.spec_json)
    .bind(data.spec_hash)
    .bind(req.card.metadata.artifact_hash.as_deref())
    .bind(data.labels_json)
    .bind(data.annotations_json)
    .bind(req.actor.id.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;

    inserted
        .map(|(uid,)| CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error))
        .transpose()
}

/// Fetch the uid of the active row that matches a resolved `(kind, space,
/// name, version)` identity.
///
/// Called by `register_auto` after a `Deduplicated` resolution to convert the
/// resolved version string back to a uid without re-doing the full content
/// comparison. Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` if the row has
/// disappeared between resolution and lookup (should not happen under the
/// advisory lock, but guarded defensively).
async fn lookup_uid_by_ref(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    version: &VersionBlock,
) -> Result<CardUid, WyrdError> {
    let uid: Option<(Uuid,)> = sqlx::query_as(
        "SELECT card_uid FROM wyrd.cards \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND kind = $1 AND space = $2 AND name = $3 AND version = $4 \
           AND status = 'active'",
    )
    .bind(kind.wire_name())
    .bind(space.as_str())
    .bind(name.as_str())
    .bind(version.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;

    let (uid,) = uid.ok_or_else(|| {
        WyrdError::registry_unavailable("resolved version row vanished before uid lookup")
    })?;
    CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)
}

/// Resolve a pin conflict: either idempotent re-apply or spec-hash drift.
///
/// Called when `insert_card_row` returns `None` for a pin registration,
/// meaning an active row with the same identity already exists. Reads the
/// existing row's `spec_hash` and compares it against the submitted hash:
///
/// - Same hash → `IdempotentNoop` (caller re-submitted identical content).
/// - Different hash → `WYRD_REG_409_SPEC_DRIFT` (caller is trying to change
///   a pinned version in place, which is forbidden).
///
/// The `AND status <> 'deleted'` guard ensures that a previously soft-deleted
/// pin at this identity (which the partial unique index now allows) does not
/// produce a false conflict reading.
async fn handle_conflict(
    conn: &mut TenantConn<'_>,
    card: &Card,
    space: &SpaceName,
    version_block: &VersionBlock,
    submitted_hash: &str,
    req: &RegisterCardRequest<'_>,
) -> Result<RegisterCardOutcome, WyrdError> {
    let existing = sqlx::query_as::<_, (Uuid, String)>(
        r#"SELECT card_uid, spec_hash FROM wyrd.cards
           WHERE kind = $1 AND space = $2 AND name = $3 AND version = $4
             AND data_tenant_id = wyrd.current_tenant()
             AND status = 'active'"#,
    )
    .bind(card.kind.wire_name())
    .bind(space.as_str())
    .bind(card.metadata.name.as_str())
    .bind(version_block.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;

    let (existing_uid, existing_hash) = existing.ok_or_else(|| {
        WyrdError::registry_unavailable("conflict row vanished between INSERT and SELECT")
    })?;

    if existing_hash != submitted_hash {
        return Err(WyrdError::registry_spec_drift(
            existing_uid.to_string(),
            existing_hash,
            submitted_hash,
        ));
    }

    let card_uid = CardUid::from_uuid(existing_uid).map_err(WyrdError::from_card_uid_error)?;
    let principal_id = match &card.kind {
        CardKind::Service | CardKind::Agent => {
            lookup_existing_principal_id(conn, &card_uid).await?
        }
        _ => None,
    };

    record_registration_audit(
        conn,
        req,
        &card_uid,
        submitted_hash,
        CardRegistrationOutcome::IdempotentNoop,
    )
    .await?;

    Ok(RegisterCardOutcome {
        card_uid,
        kind: RegisterCardOutcomeKind::IdempotentNoop,
        principal_id,
    })
}

/// Build and write the typed audit event for a registration event.
async fn record_registration_audit(
    conn: &mut TenantConn<'_>,
    req: &RegisterCardRequest<'_>,
    card_uid: &CardUid,
    spec_hash: &str,
    outcome: CardRegistrationOutcome,
) -> Result<(), WyrdError> {
    record_card_registration_audit(
        conn,
        CardRegistrationAuditInput {
            card_uid,
            card_kind: req.card.kind.clone(),
            operation: CardRegistrationOperation::Register,
            outcome: Some(outcome),
            actor: req.actor,
            before_spec_hash: None,
            after_spec_hash: Some(spec_hash),
            request_id: req.request_id,
        },
    )
    .await
}

/// Persisted registration operation used for request replay.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardRegistrationOperationRow {
    /// Server-minted operation identifier.
    pub operation_id: Uuid,
    /// Tenant owning the operation.
    pub data_tenant_id: Uuid,
    /// Principal that supplied the idempotency key.
    pub principal_id: Uuid,
    /// Caller-provided idempotency key.
    pub idempotency_key: String,
    /// BLAKE3/JCS request hash.
    pub request_hash: String,
    /// Card created by the operation.
    pub card_uid: Uuid,
    /// Stored upload plan inventory. Presigned URLs are never reused.
    #[sqlx(json)]
    pub upload_plans: JsonValue,
    /// Registration outcome literal.
    pub outcome: String,
    /// Operation status literal.
    pub status: String,
    /// Operation creation time.
    pub created_at: DateTime<Utc>,
    /// Last operation state change.
    pub updated_at: DateTime<Utc>,
}

/// Inputs for the idempotency operation insert.
pub struct NewRegistrationOperation<'a> {
    /// Server-minted operation identifier.
    pub operation_id: RegistrationOperationId,
    /// Authenticated principal identifier.
    pub principal_id: PrincipalId,
    /// Caller-provided idempotency key.
    pub idempotency_key: &'a str,
    /// BLAKE3/JCS request hash.
    pub request_hash: &'a str,
    /// Card identifier reserved by this operation.
    pub card_uid: &'a CardUid,
    /// Stored upload plan inventory.
    pub upload_plans: &'a JsonValue,
    /// Initial outcome literal.
    pub outcome: &'a str,
    /// Initial operation status.
    pub status: &'a str,
}

/// Inputs for the card row insert owned by a registration operation.
pub struct NewCardRow<'a> {
    /// Resolved card envelope.
    pub card: &'a Card,
    /// Server-minted card identifier shared with the operation row.
    pub card_uid: CardUid,
    /// Principal that created the row.
    pub principal_id: PrincipalId,
    /// Registration operation owning the row.
    pub operation_id: RegistrationOperationId,
    /// Initial lifecycle status.
    pub status: CardStatus,
    /// Canonical spec hash.
    pub spec_hash: &'a str,
    /// Canonical artifact manifest hash.
    pub artifact_hash: Option<&'a str>,
}

/// A manifest row eligible for post-commit upload initialization.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardArtifactManifestRow {
    /// Owning card identifier.
    pub card_uid: Uuid,
    /// Validated relative artifact path.
    pub relative_path: String,
    /// Expected base64 SHA-256 digest.
    pub expected_sha256: String,
    /// Expected artifact size.
    pub expected_size_bytes: i64,
    /// Optional content type.
    pub content_type: Option<String>,
    /// Current post-commit init state.
    pub upload_status: String,
    /// Storage upload identifier once initialization succeeds.
    pub upload_id: Option<Uuid>,
}

/// Card row values needed to construct the registration response.
#[derive(Debug, Clone)]
pub struct RegisteredCardRow {
    /// Card UID.
    pub card_uid: CardUid,
    /// Card kind.
    pub kind: CardKind,
    /// Card space.
    pub space: SpaceName,
    /// Card name.
    pub name: CardName,
    /// Resolved version.
    pub version: VersionBlock,
    /// Canonical spec hash.
    pub spec_hash: String,
    /// Canonical artifact manifest hash.
    pub artifact_hash: Option<String>,
    /// Current lifecycle status.
    pub status: CardStatus,
    /// Row creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Principal that created the row.
    pub principal_id: PrincipalId,
    /// Registration operation identifier.
    pub operation_id: RegistrationOperationId,
}

/// Compute the canonical BLAKE3/JCS hash for a submitted artifact manifest.
#[must_use]
pub fn artifact_manifest_hash(artifacts: &[ArtifactManifestEntry]) -> Option<String> {
    if artifacts.is_empty() {
        return None;
    }
    let bytes = serde_jcs::to_vec(artifacts).expect("artifact manifest is serializable");
    Some(blake3::hash(&bytes).to_hex().to_string())
}

/// Compute the canonical BLAKE3/JCS hash used by registration idempotency.
#[must_use]
pub fn registration_request_hash(spec_hash: &str, artifact_hash: Option<&str>) -> String {
    let payload = serde_json::json!({
        "artifact_manifest_hash": artifact_hash,
        "spec_hash": spec_hash,
    });
    let bytes = serde_jcs::to_vec(&payload).expect("request hash payload is serializable");
    blake3::hash(&bytes).to_hex().to_string()
}

/// Find an operation by tenant-scoped principal and idempotency key.
///
/// The caller owns the transaction. RLS supplies the tenant boundary and the
/// explicit tenant predicate makes the query intent clear to reviewers.
///
/// # Errors
/// Returns [`WyrdError::RegistryUnavailable`] when the lookup fails.
pub async fn lookup_existing_operation(
    conn: &mut TenantConn<'_>,
    principal_id: PrincipalId,
    idempotency_key: &str,
) -> Result<Option<CardRegistrationOperationRow>, WyrdError> {
    sqlx::query_as::<_, CardRegistrationOperationRow>(
        r#"
        SELECT operation_id, data_tenant_id, principal_id, idempotency_key,
               request_hash, card_uid, upload_plans, outcome, status,
               created_at, updated_at
          FROM wyrd.card_registration_operations
         WHERE data_tenant_id = wyrd.current_tenant()
           AND principal_id = $1
           AND idempotency_key = $2
        "#,
    )
    .bind(principal_id.as_uuid())
    .bind(idempotency_key)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Insert the operation row that reserves an idempotency key.
///
/// Returns `true` when this call inserted the operation and `false` when a
/// concurrent request already won the unique key. The caller must re-read the
/// winner before attempting any card or manifest write.
///
/// # Errors
/// Returns [`WyrdError::RegistryUnavailable`] for database failures.
pub async fn insert_registration_operation(
    conn: &mut TenantConn<'_>,
    operation: NewRegistrationOperation<'_>,
) -> Result<bool, WyrdError> {
    let inserted = sqlx::query(
        r#"
        INSERT INTO wyrd.card_registration_operations
            (operation_id, data_tenant_id, principal_id, idempotency_key,
             request_hash, card_uid, upload_plans, outcome, status)
        VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (data_tenant_id, principal_id, idempotency_key) DO NOTHING
        "#,
    )
    .bind(operation.operation_id.as_uuid())
    .bind(operation.principal_id.as_uuid())
    .bind(operation.idempotency_key)
    .bind(operation.request_hash)
    .bind(operation.card_uid.as_uuid())
    .bind(operation.upload_plans)
    .bind(operation.outcome)
    .bind(operation.status)
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(inserted.rows_affected() == 1)
}

/// Insert a card row for the registration operation.
///
/// The operation ID is written into the card row so later lifecycle handlers
/// can resolve the pending operation without trusting a client-supplied ID.
///
/// # Errors
/// Returns [`WyrdError::RegistryUnavailable`] for database failures and a
/// typed registry error when the resolved card identity cannot be parsed.
pub async fn insert_card_row(
    conn: &mut TenantConn<'_>,
    input: NewCardRow<'_>,
) -> Result<RegisteredCardRow, WyrdError> {
    let space = input
        .card
        .metadata
        .space
        .as_ref()
        .ok_or_else(|| WyrdError::registry_invalid_card_spec("metadata.space is required"))?;
    let version = match input.card.metadata.version.as_ref() {
        Some(VersionSpec::Pin(version)) => version,
        _ => {
            return Err(WyrdError::registry_invalid_version_block(
                "registration requires a resolved version",
            ));
        }
    };
    let pending_since = (input.status == CardStatus::Pending).then(Utc::now);
    let row = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        r#"
        INSERT INTO wyrd.cards
            (card_uid, data_tenant_id, kind, space, name, version,
             spec, spec_hash, artifact_hash, labels, annotations, status,
             created_by, registration_operation_id, pending_since)
        VALUES (
            $1, wyrd.current_tenant(), $2, $3, $4, $5,
            $6, $7, $8, $9, $10, $11,
            $12, $13, $14
        )
        RETURNING card_uid, created_at
        "#,
    )
    .bind(input.card_uid.as_uuid())
    .bind(input.card.kind.wire_name())
    .bind(space.as_str())
    .bind(input.card.metadata.name.as_str())
    .bind(version.as_str())
    .bind(serde_json::to_value(&input.card.spec).map_err(WyrdError::from_spec_serialization)?)
    .bind(input.spec_hash)
    .bind(input.artifact_hash)
    .bind(
        serde_json::to_value(&input.card.metadata.labels)
            .map_err(WyrdError::from_spec_serialization)?,
    )
    .bind(
        serde_json::to_value(&input.card.metadata.annotations)
            .map_err(WyrdError::from_spec_serialization)?,
    )
    .bind(input.status.as_db_str())
    .bind(input.principal_id.as_uuid())
    .bind(input.operation_id.as_uuid())
    .bind(pending_since)
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;

    Ok(RegisteredCardRow {
        card_uid: CardUid::from_uuid(row.0).map_err(WyrdError::from_card_uid_error)?,
        kind: input.card.kind.clone(),
        space: space.clone(),
        name: input.card.metadata.name.clone(),
        version: version.clone(),
        spec_hash: input.spec_hash.to_owned(),
        artifact_hash: input.artifact_hash.map(str::to_owned),
        status: input.status,
        created_at: row.1,
        principal_id: input.principal_id,
        operation_id: input.operation_id,
    })
}

/// Insert the manifest rows in the same transaction as the card row.
///
/// Every row starts at `awaiting_init`; no storage backend is contacted by
/// this function.
///
/// # Errors
/// Returns [`WyrdError::RegistryUnavailable`] for database failures.
pub async fn insert_artifact_manifest_rows(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    artifacts: &[ArtifactManifestEntry],
) -> Result<(), WyrdError> {
    for artifact in artifacts {
        sqlx::query(
            r#"
            INSERT INTO wyrd.card_artifact_manifest
                (card_uid, relative_path, expected_sha256, expected_size_bytes,
                 content_type, upload_status, upload_id, data_tenant_id)
            VALUES ($1, $2, $3, $4, $5, 'awaiting_init', NULL, wyrd.current_tenant())
            "#,
        )
        .bind(card_uid.as_uuid())
        .bind(artifact.relative_path.as_str())
        .bind(&artifact.expected_sha256)
        .bind(artifact.expected_size_bytes)
        .bind(&artifact.content_type)
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
    }
    Ok(())
}

/// Load manifest rows that need post-commit initialization or replay.
///
/// The caller performs storage initialization after committing the registry
/// transaction. This query is read-only and never opens a storage transaction.
///
/// # Errors
/// Returns [`WyrdError::RegistryUnavailable`] when the lookup fails.
pub async fn manifest_rows_for_init(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Vec<CardArtifactManifestRow>, WyrdError> {
    sqlx::query_as::<_, CardArtifactManifestRow>(
        r#"
        SELECT card_uid, relative_path, expected_sha256, expected_size_bytes,
               content_type, upload_status, upload_id
          FROM wyrd.card_artifact_manifest
         WHERE data_tenant_id = wyrd.current_tenant()
           AND card_uid = $1
           AND upload_status IN ('awaiting_init', 'pending')
         ORDER BY relative_path
        "#,
    )
    .bind(card_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Mark one manifest row ready for client upload after successful init.
///
/// This is intentionally a separate caller-owned tenant transaction because
/// storage initialization happens after the registry transaction commits.
///
/// # Errors
/// Returns [`WyrdError::RegistryUnavailable`] for database failures.
pub async fn mark_manifest_upload_initialized(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    relative_path: &str,
    upload_id: Uuid,
) -> Result<(), WyrdError> {
    sqlx::query(
        r#"
        UPDATE wyrd.card_artifact_manifest
           SET upload_status = 'pending', upload_id = $3
         WHERE data_tenant_id = wyrd.current_tenant()
           AND card_uid = $1
           AND relative_path = $2
           AND upload_status IN ('awaiting_init', 'pending')
        "#,
    )
    .bind(card_uid.as_uuid())
    .bind(relative_path)
    .bind(upload_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(())
}

/// Map a stored operation outcome literal to the wire outcome enum.
///
/// Unknown literals are treated as a database invariant failure instead of
/// silently projecting them as a successful registration.
pub fn operation_outcome(value: &str) -> Result<wyrd_spec::registry::RegisterOutcome, WyrdError> {
    match value {
        "created" => Ok(wyrd_spec::registry::RegisterOutcome::Created),
        "idempotent_noop" => Ok(wyrd_spec::registry::RegisterOutcome::IdempotentNoop),
        "deduplicated" => Ok(wyrd_spec::registry::RegisterOutcome::Deduplicated),
        _ => Err(WyrdError::registry_unavailable("card registry unavailable")),
    }
}

/// Map a stored operation status literal to the SQL card lifecycle enum.
pub fn operation_status(value: &str) -> Result<CardStatus, WyrdError> {
    CardStatus::from_db_str(value).map_err(registry_db_error)
}

/// Update the replay inventory after post-commit upload initialization.
pub async fn update_registration_operation_plans(
    conn: &mut TenantConn<'_>,
    operation_id: RegistrationOperationId,
    upload_plans: &JsonValue,
) -> Result<(), WyrdError> {
    sqlx::query(
        r#"
        UPDATE wyrd.card_registration_operations
           SET upload_plans = $1, updated_at = now()
         WHERE operation_id = $2
           AND data_tenant_id = wyrd.current_tenant()
        "#,
    )
    .bind(upload_plans)
    .bind(operation_id.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(())
}

/// Resolve a batch of exact card identities inside the caller-owned tenant transaction.
pub async fn select_card_uids_by_ref_batch(
    conn: &mut TenantConn<'_>,
    refs: &[CardRef],
) -> Result<Vec<(CardRef, CardUid)>, WyrdError> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT refs.kind, refs.space, refs.name, refs.version, cards.card_uid \
         FROM (VALUES ",
    );
    query.push_values(refs.iter(), |mut values, card_ref| {
        values
            .push_bind(card_ref.kind.wire_name())
            .push_bind(card_ref.space.as_str())
            .push_bind(card_ref.name.as_str())
            .push_bind(card_ref.version.as_str());
    });
    query.push(
        ") AS refs(kind, space, name, version) \
         LEFT JOIN wyrd.cards cards \
           ON cards.data_tenant_id = wyrd.current_tenant() \
          AND cards.kind = refs.kind \
          AND cards.space = refs.space \
          AND cards.name = refs.name \
          AND cards.version = refs.version \
          AND cards.status NOT IN ('deleted', 'failed', 'expired')",
    );
    let rows = query
        .build_query_as::<(String, String, String, String, Option<Uuid>)>()
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(|_| WyrdError::registry_unavailable("card registry unavailable"))?;
    rows.into_iter()
        .filter_map(|(kind, space, name, version, uid)| {
            uid.map(|uid| (kind, space, name, version, uid))
        })
        .map(|(kind, space, name, version, uid)| {
            let card_ref = refs
                .iter()
                .find(|card_ref| {
                    card_ref.kind.wire_name() == kind
                        && card_ref.space.as_str() == space
                        && card_ref.name.as_str() == name
                        && card_ref.version.as_str() == version
                })
                .expect("VALUES relation only contains requested card references");
            Ok((
                card_ref.clone(),
                CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{artifact_manifest_hash, operation_outcome, registration_request_hash};
    use wyrd_spec::registry::{ArtifactManifestEntry, RegisterOutcome, RelativeArtifactPath};

    #[test]
    fn request_hash_is_lowercase_blake3_over_the_jcs_shape() {
        let hash = registration_request_hash(&"a".repeat(64), None);

        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(hash, registration_request_hash(&"a".repeat(64), None));
        assert_ne!(hash, registration_request_hash(&"b".repeat(64), None));
        assert_ne!(
            hash,
            registration_request_hash(&"a".repeat(64), Some(&"b".repeat(64)))
        );
    }

    #[test]
    fn manifest_hash_is_order_sensitive_and_empty_manifests_are_absent() {
        assert_eq!(artifact_manifest_hash(&[]), None);
        let first = ArtifactManifestEntry {
            relative_path: RelativeArtifactPath::new("a.bin").expect("valid path"),
            expected_sha256: "YQ==".to_owned(),
            expected_size_bytes: 1,
            content_type: None,
        };
        let second = ArtifactManifestEntry {
            relative_path: RelativeArtifactPath::new("b.bin").expect("valid path"),
            expected_sha256: "Yg==".to_owned(),
            expected_size_bytes: 1,
            content_type: None,
        };
        assert_ne!(
            artifact_manifest_hash(&[first.clone(), second.clone()]),
            artifact_manifest_hash(&[second, first])
        );
    }

    #[test]
    fn operation_outcome_rejects_unknown_database_literals() {
        assert_eq!(
            operation_outcome("created").expect("known outcome"),
            RegisterOutcome::Created
        );
        assert!(operation_outcome("unexpected").is_err());
    }
}

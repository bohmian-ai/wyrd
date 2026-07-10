//! Idempotent INSERT path for `wyrd.cards`.
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use ulid::Ulid;
use uuid::Uuid;

use wyrd_runtime::principal::{Principal, PrincipalId};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::request_id::RequestId;

use crate::queries::cards::audit::record_card_registration_audit;
use crate::queries::cards::auth_projection::{
    lookup_existing_principal_id, upsert_service_account_from_card,
};
use crate::queries::cards::version_resolve::{Resolution, resolve_version};
use crate::queries::cards::version_sql::lock_version_line;
use crate::row_types::cards::{
    CardRegistrationOperation, CardRegistrationOutcome, NewAuditCardRegistrationRow,
};
use crate::tenant_conn::TenantConn;

/// Maximum canonical-JSON byte size for a single `Spec`.
pub const MAX_SPEC_BYTES: usize = 256 * 1024;

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
    let inserted = insert_card_row(conn, req, space, name, block, data).await?;

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
        data.spec_hash,
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
            let uid = insert_card_row(conn, req, space, name, &next, data)
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
async fn insert_card_row(
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
        ON CONFLICT (data_tenant_id, kind, space, name, version) WHERE status <> 'deleted' DO NOTHING
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
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

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
           AND status <> 'deleted'",
    )
    .bind(kind.wire_name())
    .bind(space.as_str())
    .bind(name.as_str())
    .bind(version.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

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
             AND status <> 'deleted'"#,
    )
    .bind(card.kind.wire_name())
    .bind(space.as_str())
    .bind(card.metadata.name.as_str())
    .bind(version_block.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

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

/// Build and write the audit row for a registration event.
///
/// Thin wrapper that assembles a [`NewAuditCardRegistrationRow`] from the
/// parts available at the call site and delegates to
/// [`record_card_registration_audit`]. Always writes a `Register` operation
/// row; `Update` and `Delete` audit rows are written by other callers.
async fn record_registration_audit(
    conn: &mut TenantConn<'_>,
    req: &RegisterCardRequest<'_>,
    card_uid: &CardUid,
    spec_hash: &str,
    outcome: CardRegistrationOutcome,
) -> Result<(), WyrdError> {
    record_card_registration_audit(
        conn,
        NewAuditCardRegistrationRow {
            audit_id: Uuid::from_bytes(Ulid::new().to_bytes()),
            data_tenant_id: conn.data_tenant_id().as_uuid(),
            card_uid,
            kind: req.card.kind.clone(),
            operation: CardRegistrationOperation::Register,
            outcome: Some(outcome),
            actor_principal_id: req.actor.id,
            actor_kind: req.actor.kind.clone(),
            before_spec_hash: None,
            after_spec_hash: Some(spec_hash),
            request_id: req.request_id.map(RequestId::as_str),
        },
    )
    .await
}

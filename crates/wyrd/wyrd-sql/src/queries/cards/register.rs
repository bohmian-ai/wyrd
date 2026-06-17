//! Idempotent INSERT path for `wyrd.cards`.
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use ulid::Ulid;
use uuid::Uuid;

use wyrd_runtime::principal::{Principal, PrincipalId};
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::request_id::RequestId;

use crate::queries::cards::audit::record_card_registration_audit;
use crate::queries::cards::auth_projection::{
    lookup_existing_principal_id, upsert_service_account_from_card,
};
use crate::row_types::cards::{CardRegistrationOperation, NewAuditCardRegistrationRow};
use crate::tenant_conn::TenantConn;

/// Maximum canonical-JSON byte size for a single `Spec`.
pub const MAX_SPEC_BYTES: usize = 256 * 1024;

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
    /// New row inserted into `wyrd.cards`.
    Created,
    /// Existing row matched on `(identity, spec_hash)`; no DB write.
    IdempotentNoop,
}

/// Result of a [`register_card`] call.
#[derive(Debug, Clone)]
pub struct RegisterCardOutcome {
    /// Card UID — newly minted on create, existing on idempotent re-apply.
    pub card_uid: CardUid,
    /// Three-way outcome classification.
    pub kind: RegisterCardOutcomeKind,
    /// Durable `auth_service_accounts.id` for Service/Agent cards.
    pub principal_id: Option<PrincipalId>,
}

/// Register a card (or no-op re-apply) inside the caller's [`TenantConn`] tx.
///
/// Transaction order: cards INSERT → auth_service_accounts upsert → audit INSERT.
///
/// # Errors
/// Returns [`WyrdError::RegistryInvalidCardSpec`] when boundary validation fails.
/// Returns [`WyrdError::RegistrySpecDrift`] when a spec hash mismatch is detected.
/// Returns [`WyrdError::RegistryUnavailable`] on transient DB errors.
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

    let version_block: VersionBlock = match &req.card.metadata.version {
        Some(VersionSpec::Pin(block)) => block.clone(),
        Some(VersionSpec::Scope(_)) => {
            return Err(WyrdError::registry_version_required(
                "scope version resolution is not yet implemented; pass an explicit semver pin",
            ));
        }
        None => {
            return Err(WyrdError::registry_version_required(
                "metadata.version is required",
            ));
        }
    };

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

    let spec_json: JsonValue = serde_json::to_value(&req.card.spec)
        .map_err(WyrdError::from_spec_serialization)?;
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

    let card_uid = CardUid::from_uuid7(Uuid::now_v7())
        .map_err(WyrdError::from_card_uid_error)?;

    let inserted = sqlx::query_as::<_, (Uuid, String)>(
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
        ON CONFLICT (data_tenant_id, kind, space, name, version) DO NOTHING
        RETURNING card_uid, spec_hash
        "#,
    )
    .bind(card_uid.as_uuid())
    .bind(req.card.kind.wire_name())
    .bind(space.as_str())
    .bind(req.card.metadata.name.as_str())
    .bind(version_block.as_str())
    .bind(&spec_json)
    .bind(spec_hash.as_str())
    .bind(req.card.metadata.artifact_hash.as_deref())
    .bind(&labels_json)
    .bind(&annotations_json)
    .bind(req.actor.id.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    let uid_for_writes = match inserted {
        Some((inserted_uid, _)) => {
            CardUid::from_uuid(inserted_uid).map_err(WyrdError::from_card_uid_error)?
        }
        None => {
            return handle_conflict(conn, req.card, space.as_str(), &version_block, spec_hash.as_str(), &req).await;
        }
    };

    let principal_id = match &req.card.spec {
        Spec::Service(_) | Spec::Agent(_) => {
            let pid =
                upsert_service_account_from_card(conn, &uid_for_writes, req.card, req.actor)
                    .await?;
            Some(pid)
        }
        _ => None,
    };

    record_card_registration_audit(
        conn,
        NewAuditCardRegistrationRow {
            audit_id: Uuid::from_bytes(Ulid::new().to_bytes()),
            data_tenant_id: conn.data_tenant_id().as_uuid(),
            card_uid: &uid_for_writes,
            kind: req.card.kind.clone(),
            operation: CardRegistrationOperation::Register,
            actor_principal_id: req.actor.id,
            actor_kind: req.actor.kind.clone(),
            before_spec_hash: None,
            after_spec_hash: Some(spec_hash.as_str()),
            request_id: req.request_id.map(RequestId::as_str),
        },
    )
    .await?;

    Ok(RegisterCardOutcome {
        card_uid: uid_for_writes,
        kind: RegisterCardOutcomeKind::Created,
        principal_id,
    })
}

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
    let version_spec = card.metadata.version.as_ref().ok_or_else(|| {
        WyrdError::registry_version_required("metadata.version is required")
    })?;
    if version_spec.as_str().is_empty() {
        return Err(WyrdError::registry_version_required(
            "metadata.version must not be empty",
        ));
    }
    if matches!(card.spec, Spec::Service(_) | Spec::Agent(_)) {
        if !version_spec.is_pin() {
            return Err(WyrdError::registry_invalid_version_block(
                "Service and Agent cards require a pinned version",
            ));
        }
    }
    Ok(())
}

async fn handle_conflict(
    conn: &mut TenantConn<'_>,
    card: &Card,
    space: &str,
    version_block: &VersionBlock,
    submitted_hash: &str,
    req: &RegisterCardRequest<'_>,
) -> Result<RegisterCardOutcome, WyrdError> {
    let existing = sqlx::query_as::<_, (Uuid, String)>(
        r#"SELECT card_uid, spec_hash FROM wyrd.cards
           WHERE kind = $1 AND space = $2 AND name = $3 AND version = $4"#,
    )
    .bind(card.kind.wire_name())
    .bind(space)
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
    let principal_id = match &card.spec {
        Spec::Service(_) | Spec::Agent(_) => lookup_existing_principal_id(conn, &card_uid).await?,
        _ => None,
    };

    record_card_registration_audit(
        conn,
        NewAuditCardRegistrationRow {
            audit_id: Uuid::from_bytes(Ulid::new().to_bytes()),
            data_tenant_id: conn.data_tenant_id().as_uuid(),
            card_uid: &card_uid,
            kind: card.kind.clone(),
            operation: CardRegistrationOperation::Register,
            actor_principal_id: req.actor.id,
            actor_kind: req.actor.kind.clone(),
            before_spec_hash: None,
            after_spec_hash: Some(submitted_hash),
            request_id: req.request_id.map(RequestId::as_str),
        },
    )
    .await?;

    Ok(RegisterCardOutcome {
        card_uid,
        kind: RegisterCardOutcomeKind::IdempotentNoop,
        principal_id,
    })
}

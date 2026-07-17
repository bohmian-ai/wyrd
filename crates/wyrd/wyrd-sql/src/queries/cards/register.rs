//! Composite card-registration persistence primitives.
#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::QueryBuilder;
use uuid::Uuid;
use wyrd_runtime::principal::PrincipalId;
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::envelope::{Card, CardKind, SpecCanonicalizationError};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{ArtifactManifestEntry, CardSubmission, RegistrationOperationId};

use crate::row_types::cards::CardStatus;
use crate::tenant_conn::TenantConn;

// Dynamic query is intentional: the reference batch has request-dependent
// cardinality, and every bind remains parameterized through QueryBuilder.

/// Persisted registration operation used for idempotency replay.
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
    /// Stored composite response for exact replay.
    #[sqlx(json)]
    pub stored_response: Option<JsonValue>,
    /// Operation status literal.
    pub status: String,
    /// Operation creation time.
    pub created_at: DateTime<Utc>,
    /// Last operation state change.
    pub updated_at: DateTime<Utc>,
}

/// Inputs for reserving a registration idempotency key.
pub struct NewRegistrationOperation<'a> {
    /// Server-minted operation identifier.
    pub operation_id: RegistrationOperationId,
    /// Authenticated principal identifier.
    pub principal_id: PrincipalId,
    /// Caller-provided idempotency key.
    pub idempotency_key: &'a str,
    /// BLAKE3/JCS request hash.
    pub request_hash: &'a str,
}

/// Inputs for inserting one resolved card in a composite operation.
pub struct NewCardRow<'a> {
    /// Resolved card envelope.
    pub card: &'a Card,
    /// Server-minted card identifier.
    pub card_uid: CardUid,
    /// Principal that created the row.
    pub principal_id: PrincipalId,
    /// Registration operation owning the row.
    pub operation_id: RegistrationOperationId,
    /// Initial lifecycle status.
    pub status: CardStatus,
    /// Canonical resolved-spec hash.
    pub spec_hash: &'a str,
    /// Canonical artifact-manifest hash.
    pub artifact_hash: Option<&'a str>,
}

/// A manifest row eligible for post-commit upload initialization.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardArtifactManifestRow {
    /// Owning card identifier.
    pub card_uid: Uuid,
    /// Server-minted manifest identifier.
    pub manifest_id: Uuid,
    /// Validated relative artifact path.
    pub relative_path: String,
    /// Expected base64 SHA-256 digest.
    pub expected_sha256: String,
    /// Expected artifact size.
    pub size_bytes: i64,
    /// Optional content type.
    pub content_type: Option<String>,
    /// Current post-commit initialization state.
    pub upload_status: String,
    /// Storage upload identifier once initialization succeeds.
    pub upload_id: Option<Uuid>,
}

/// Card row values needed to construct a registration response.
#[derive(Debug, Clone, sqlx::FromRow)]
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
    /// Canonical artifact-manifest hash.
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
pub fn artifact_manifest_hash(
    artifacts: &[ArtifactManifestEntry],
) -> Result<Option<String>, WyrdError> {
    if artifacts.is_empty() {
        return Ok(None);
    }
    let bytes = serde_jcs::to_vec(artifacts).map_err(|error| {
        WyrdError::from_spec_canonicalization(SpecCanonicalizationError::Serialize(error))
    })?;
    Ok(Some(blake3::hash(&bytes).to_hex().to_string()))
}

/// Compute the canonical request hash from ordered submissions and manifest hashes.
pub fn registration_request_hash(
    canonical_submissions: &[&CardSubmission],
    artifact_hashes: &[Option<String>],
) -> Result<String, WyrdError> {
    let payload = serde_json::json!({
        "submissions": canonical_submissions,
        "artifact_manifest_hashes": artifact_hashes,
    });
    let bytes = serde_jcs::to_vec(&payload).map_err(|error| {
        WyrdError::from_spec_canonicalization(SpecCanonicalizationError::Serialize(error))
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Find an operation by tenant-scoped principal and idempotency key.
pub async fn lookup_existing_operation(
    conn: &mut TenantConn<'_>,
    principal_id: PrincipalId,
    idempotency_key: &str,
) -> Result<Option<CardRegistrationOperationRow>, WyrdError> {
    sqlx::query_as::<_, CardRegistrationOperationRow>(
        r#"SELECT operation_id, data_tenant_id, principal_id, idempotency_key,
                  request_hash, stored_response, status,
                  created_at, updated_at
             FROM wyrd.card_registration_operations
            WHERE data_tenant_id = wyrd.current_tenant()
              AND status <> 'expired'
              AND principal_id = $1 AND idempotency_key = $2"#,
    )
    .bind(principal_id.as_uuid())
    .bind(idempotency_key)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Find an expired operation so the caller receives a stable terminal error.
pub async fn lookup_expired_operation(
    conn: &mut TenantConn<'_>,
    principal_id: PrincipalId,
    idempotency_key: &str,
) -> Result<Option<CardRegistrationOperationRow>, WyrdError> {
    sqlx::query_as::<_, CardRegistrationOperationRow>(
        r#"SELECT operation_id, data_tenant_id, principal_id, idempotency_key,
                  request_hash, stored_response, status,
                  created_at, updated_at
             FROM wyrd.card_registration_operations
            WHERE data_tenant_id = wyrd.current_tenant()
              AND status = 'expired'
              AND principal_id = $1 AND idempotency_key = $2"#,
    )
    .bind(principal_id.as_uuid())
    .bind(idempotency_key)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Reserve an idempotency key before any card, manifest, or audit write.
pub async fn insert_registration_operation(
    conn: &mut TenantConn<'_>,
    operation: NewRegistrationOperation<'_>,
) -> Result<bool, WyrdError> {
    let inserted = sqlx::query(
        r#"INSERT INTO wyrd.card_registration_operations
               (operation_id, data_tenant_id, principal_id, idempotency_key,
                request_hash, stored_response, status)
           VALUES ($1, wyrd.current_tenant(), $2, $3, $4, NULL, 'pending')
           ON CONFLICT (data_tenant_id, principal_id, idempotency_key) DO NOTHING"#,
    )
    .bind(operation.operation_id.as_uuid())
    .bind(operation.principal_id.as_uuid())
    .bind(operation.idempotency_key)
    .bind(operation.request_hash)
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(inserted.rows_affected() == 1)
}

/// Insert one resolved card owned by a registration operation.
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
    let created_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"INSERT INTO wyrd.cards
               (card_uid, data_tenant_id, kind, space, name, version, spec,
                spec_hash, artifact_hash, labels, annotations, status, created_by,
                registration_operation_id, pending_since)
           VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, $8,
                   $9, $10, $11, $12, $13, $14)
           RETURNING created_at"#,
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
        card_uid: input.card_uid,
        kind: input.card.kind.clone(),
        space: space.clone(),
        name: input.card.metadata.name.clone(),
        version: version.clone(),
        spec_hash: input.spec_hash.to_owned(),
        artifact_hash: input.artifact_hash.map(str::to_owned),
        status: input.status,
        created_at,
        principal_id: input.principal_id,
        operation_id: input.operation_id,
    })
}

/// Insert manifest rows in the same transaction as their card.
pub async fn insert_artifact_manifest_rows(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    artifacts: &[ArtifactManifestEntry],
) -> Result<(), WyrdError> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let rows = artifacts
        .iter()
        .map(|artifact| {
            let size = i64::try_from(artifact.size_bytes).map_err(|_| {
                WyrdError::registry_spec_too_large(artifact.size_bytes as usize, i64::MAX as usize)
            })?;
            Ok((artifact, size))
        })
        .collect::<Result<Vec<_>, WyrdError>>()?;
    let mut query = QueryBuilder::<sqlx::Postgres>::new(
        "INSERT INTO wyrd.card_artifact_manifest \
         (manifest_id, data_tenant_id, card_uid, relative_path, expected_sha256, size_bytes, \
          content_type, upload_status) ",
    );
    query.push_values(rows, |mut values, (artifact, size)| {
        values
            .push_bind(Uuid::now_v7())
            .push("wyrd.current_tenant()")
            .push_bind(card_uid.as_uuid())
            .push_bind(artifact.relative_path.as_str())
            .push_bind(&artifact.sha256)
            .push_bind(size)
            .push_bind(&artifact.content_type)
            .push_bind("awaiting_init");
    });
    query
        .build()
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
    Ok(())
}

/// Load manifest rows that need post-commit initialization or replay.
pub async fn manifest_rows_for_init(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Vec<CardArtifactManifestRow>, WyrdError> {
    sqlx::query_as::<_, CardArtifactManifestRow>(
        r#"SELECT card_uid, manifest_id, relative_path, expected_sha256, size_bytes,
                  content_type, upload_status, upload_id
             FROM wyrd.card_artifact_manifest
            WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1
              AND upload_status IN ('awaiting_init', 'pending')
            ORDER BY relative_path"#,
    )
    .bind(card_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Mark one manifest row initialized after storage returns an upload identifier.
pub async fn mark_manifest_upload_initialized(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    relative_path: &str,
    upload_id: Uuid,
) -> Result<(), WyrdError> {
    sqlx::query(
        r#"UPDATE wyrd.card_artifact_manifest
              SET upload_status = 'pending', upload_id = $3
            WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1
              AND relative_path = $2 AND upload_status IN ('awaiting_init', 'pending')"#,
    )
    .bind(card_uid.as_uuid())
    .bind(relative_path)
    .bind(upload_id)
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(())
}

/// Commit the exact composite response used for future idempotent replay.
pub async fn commit_registration_operation(
    conn: &mut TenantConn<'_>,
    operation_id: RegistrationOperationId,
    seed: &wyrd_spec::registry::RegistrationReplaySeed,
) -> Result<(), WyrdError> {
    let stored_response = serde_json::to_value(seed).map_err(WyrdError::from_spec_serialization)?;
    let result = sqlx::query(
        r#"UPDATE wyrd.card_registration_operations
              SET stored_response = $1, status = 'committed', updated_at = now()
            WHERE operation_id = $2 AND data_tenant_id = wyrd.current_tenant()
              AND status = 'pending'"#,
    )
    .bind(stored_response)
    .bind(operation_id.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    if result.rows_affected() == 0 {
        return Err(WyrdError::RegistryOperationExpired {
            message: "registration operation is no longer pending".to_owned(),
            details: serde_json::json!({ "operation_id": operation_id }),
        });
    }
    Ok(())
}

/// Resolve exact references to non-terminal cards in one tenant-scoped query.
pub async fn select_card_uids_by_ref_batch(
    conn: &mut TenantConn<'_>,
    refs: &[CardRef],
) -> Result<Vec<(CardRef, CardUid)>, WyrdError> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<sqlx::Postgres>::new(
        "SELECT refs.kind, refs.space, refs.name, refs.version, cards.card_uid FROM (",
    );
    query.push_values(refs, |mut values, card_ref| {
        values
            .push_bind(card_ref.kind.wire_name())
            .push_bind(card_ref.space.as_str())
            .push_bind(card_ref.name.as_str())
            .push_bind(card_ref.version.as_str());
    });
    query.push(
        ") AS refs(kind, space, name, version) LEFT JOIN wyrd.cards cards \
         ON cards.data_tenant_id = wyrd.current_tenant() AND cards.kind = refs.kind \
         AND cards.space = refs.space AND cards.name = refs.name AND cards.version = refs.version \
              AND cards.status = 'active'",
    );
    let rows = query
        .build_query_as::<(String, String, String, String, Option<Uuid>)>()
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
    rows.into_iter()
        .filter_map(|(kind, space, name, version, uid)| {
            uid.map(|uid| {
                let card_ref = refs
                    .iter()
                    .find(|item| {
                        item.kind.wire_name() == kind
                            && item.space.as_str() == space
                            && item.name.as_str() == name
                            && item.version.as_str() == version
                    })
                    .cloned()
                    .ok_or_else(|| WyrdError::registry_unavailable("card registry unavailable"))?;
                let card_uid = CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)?;
                Ok((card_ref, card_uid))
            })
        })
        .collect()
}

/// Redact database details at the public registry boundary.
fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(%error, "card registry database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

//! Composite card-declaration persistence primitives.
#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::QueryBuilder;
use uuid::Uuid;
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::auth::Attribution;
use wyrd_spec::envelope::{Card, CardKind, SpecCanonicalizationError};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{ArtifactManifestEntry, CardSubmission, DeclarationOperationId};

use crate::row_types::cards::CardStatus;
use crate::tenant_conn::TenantConn;

// Dynamic query is intentional: the reference batch has request-dependent
// cardinality, and every bind remains parameterized through QueryBuilder.

/// Persisted declaration operation used for idempotency replay.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardDeclarationOperationRow {
    /// Server-minted operation identifier.
    pub operation_id: Uuid,
    /// Tenant owning the operation.
    pub data_tenant_id: Uuid,
    /// Subject the operation was accepted for.
    pub sub_principal_id: Uuid,
    /// Actor that supplied the idempotency key.
    pub act_principal_id: Uuid,
    /// Caller-provided idempotency key.
    pub idempotency_key: String,
    /// BLAKE3/JCS request hash.
    pub request_hash: String,
    /// Stored composite response for exact replay; NULL while the operation is
    /// pending, so it decodes as plain nullable JSONB.
    pub stored_response: Option<JsonValue>,
    /// Operation status literal.
    pub status: String,
    /// Operation creation time.
    pub created_at: DateTime<Utc>,
    /// Last operation state change.
    pub updated_at: DateTime<Utc>,
}

/// Inputs for reserving a declaration idempotency key.
pub struct NewDeclarationOperation<'a> {
    /// Server-minted operation identifier.
    pub operation_id: DeclarationOperationId,
    /// Verified subject and actor of the declaring request; together with
    /// the key they scope replay.
    pub attribution: Attribution,
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
    /// Verified subject and actor that created the row; never rewritten by
    /// later updates.
    pub attribution: Attribution,
    /// Declaration operation owning the row.
    pub operation_id: DeclarationOperationId,
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

/// Card row values needed to construct a declaration response.
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
    /// Subject and actor that created the row.
    pub attribution: Attribution,
    /// Declaration operation identifier.
    pub operation_id: DeclarationOperationId,
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

/// Find an operation by tenant-scoped subject, actor, and idempotency key.
///
/// Reads the live (pending or committed) operation that exact
/// `attribution` pair reserved under `idempotency_key` in the transaction's
/// tenant. A different subject or a different actor never matches, so two
/// principals reusing one key keep separate operations. Expired operations
/// are excluded; [`lookup_expired_operation`] reads those. The read joins the
/// caller's transaction and writes nothing.
///
/// Returns `None` when that pair has no live operation for the key.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`, with the database detail logged
/// and redacted, when the read fails.
pub async fn lookup_existing_operation(
    conn: &mut TenantConn<'_>,
    attribution: Attribution,
    idempotency_key: &str,
) -> Result<Option<CardDeclarationOperationRow>, WyrdError> {
    sqlx::query_as::<_, CardDeclarationOperationRow>(
        r"SELECT operation_id, data_tenant_id, sub_principal_id, act_principal_id,
                  idempotency_key,
                  request_hash, stored_response, status,
                  created_at, updated_at
             FROM wyrd.card_declaration_operations
            WHERE status <> 'expired'
              AND sub_principal_id = $1 AND act_principal_id = $2
              AND idempotency_key = $3",
    )
    .bind(attribution.sub_principal_id.as_uuid())
    .bind(attribution.act_principal_id.as_uuid())
    .bind(idempotency_key)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Find an expired operation so the caller receives a stable terminal error.
///
/// Reads the expired operation the exact `attribution` pair reserved under
/// `idempotency_key` in the transaction's tenant, matched like
/// [`lookup_existing_operation`]. The read joins the caller's transaction and
/// writes nothing.
///
/// Returns `None` when that pair has no expired operation for the key.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`, with the database detail logged
/// and redacted, when the read fails.
pub async fn lookup_expired_operation(
    conn: &mut TenantConn<'_>,
    attribution: Attribution,
    idempotency_key: &str,
) -> Result<Option<CardDeclarationOperationRow>, WyrdError> {
    sqlx::query_as::<_, CardDeclarationOperationRow>(
        r"SELECT operation_id, data_tenant_id, sub_principal_id, act_principal_id,
                  idempotency_key,
                  request_hash, stored_response, status,
                  created_at, updated_at
             FROM wyrd.card_declaration_operations
            WHERE status = 'expired'
              AND sub_principal_id = $1 AND act_principal_id = $2
              AND idempotency_key = $3",
    )
    .bind(attribution.sub_principal_id.as_uuid())
    .bind(attribution.act_principal_id.as_uuid())
    .bind(idempotency_key)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// Reserve an idempotency key before any card, manifest, or audit write.
///
/// Inserts a `pending` operation keyed by tenant, subject, actor, and
/// idempotency key, recording the operation's attribution pair once. The row
/// joins the caller's transaction, so it commits or rolls back with the
/// declaration it reserves; a cancelled or failed transaction leaves no
/// reservation.
///
/// Returns `true` when this call reserved the key and `false` when the same
/// pair already holds an operation for it, which the caller then reads and
/// replays or refuses.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`, with the database detail logged
/// and redacted, when the insert fails for any reason other than that
/// conflict.
pub async fn insert_declaration_operation(
    conn: &mut TenantConn<'_>,
    operation: NewDeclarationOperation<'_>,
) -> Result<bool, WyrdError> {
    let inserted = sqlx::query(
        r"INSERT INTO wyrd.card_declaration_operations
               (operation_id, data_tenant_id, sub_principal_id, act_principal_id,
                idempotency_key, request_hash, stored_response, status)
           VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, NULL, 'pending')
           ON CONFLICT (data_tenant_id, sub_principal_id, act_principal_id, idempotency_key)
           DO NOTHING",
    )
    .bind(operation.operation_id.as_uuid())
    .bind(operation.attribution.sub_principal_id.as_uuid())
    .bind(operation.attribution.act_principal_id.as_uuid())
    .bind(operation.idempotency_key)
    .bind(operation.request_hash)
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(inserted.rows_affected() == 1)
}

/// Insert one resolved card owned by a declaration operation.
///
/// Writes the Card with its resolved space and pinned version, its creation
/// pair, and the operation that owns it; a `pending` Card is stamped with its
/// pending time and immediately due for reconciliation. The row joins the
/// caller's transaction, so nothing is durable until the declaration
/// commits, and the creation pair is never rewritten afterwards.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the Card has no
/// `metadata.space`, `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` when its version
/// is not a resolved pin, the spec serialization error when its spec, labels,
/// or annotations cannot be encoded, and `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`, with
/// the database detail logged and redacted, when the insert fails, including
/// on a duplicate Card identity.
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
    let Some(VersionSpec::Pin(version)) = input.card.metadata.version.as_ref() else {
        return Err(WyrdError::registry_invalid_version_block(
            "registration requires a resolved version",
        ));
    };
    let pending_since = (input.status == CardStatus::Pending).then(Utc::now);
    let created_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        r"INSERT INTO wyrd.cards
               (card_uid, data_tenant_id, kind, space, name, version, spec,
        spec_hash, artifact_hash, labels, annotations, status,
                sub_principal_id, act_principal_id,
                declaration_operation_id, pending_since,
                reconcile_status, reconcile_next_attempt_at)
           VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, $8,
                   $9, $10, $11, $12, $13, $14, $15,
                   CASE WHEN $11 = 'pending' THEN 'pending' ELSE 'idle' END,
                   CASE WHEN $11 = 'pending' THEN statement_timestamp() ELSE NULL END)
           RETURNING created_at",
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
    .bind(input.attribution.sub_principal_id.as_uuid())
    .bind(input.attribution.act_principal_id.as_uuid())
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
        attribution: input.attribution,
        operation_id: input.operation_id,
    })
}

/// Insert manifest rows in the same transaction as their card.
///
/// The client computes every authored-omitted digest and size before
/// declaration, so each entry on the wire carries both.
///
/// # Errors
/// Returns `RegistryInvalidCardSpec` when an entry lacks `sha256` or
/// `size_bytes`, `RegistrySpecTooLarge` when a size exceeds `i64`, and a
/// registry database error when the insert fails.
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
            let (Some(sha256), Some(size_bytes)) = (&artifact.sha256, artifact.size_bytes) else {
                return Err(WyrdError::registry_invalid_card_spec(
                    "artifact manifest entries must carry sha256 and size_bytes",
                ));
            };
            let size = i64::try_from(size_bytes).map_err(|_| {
                WyrdError::registry_spec_too_large(
                    usize::try_from(size_bytes).unwrap_or(usize::MAX),
                    usize::try_from(i64::MAX).unwrap_or(usize::MAX),
                )
            })?;
            Ok((artifact, sha256, size))
        })
        .collect::<Result<Vec<_>, WyrdError>>()?;
    let mut query = QueryBuilder::<sqlx::Postgres>::new(
        "INSERT INTO wyrd.card_artifact_manifest \
         (manifest_id, data_tenant_id, card_uid, relative_path, expected_sha256, size_bytes, \
          content_type, upload_status) ",
    );
    query.push_values(rows, |mut values, (artifact, sha256, size)| {
        values
            .push_bind(Uuid::now_v7())
            .push("wyrd.current_tenant()")
            .push_bind(card_uid.as_uuid())
            .push_bind(artifact.relative_path.as_str())
            .push_bind(sha256)
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

/// Load manifest rows that still need post-commit initialization.
pub async fn manifest_rows_for_init(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Vec<CardArtifactManifestRow>, WyrdError> {
    sqlx::query_as::<_, CardArtifactManifestRow>(
        r"SELECT card_uid, manifest_id, relative_path, expected_sha256, size_bytes,
                  content_type, upload_status, upload_id
             FROM wyrd.card_artifact_manifest
            WHERE card_uid = $1
              AND upload_status = 'awaiting_init'
            ORDER BY relative_path",
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
        r"UPDATE wyrd.card_artifact_manifest
              SET upload_status = 'pending', upload_id = $3
            WHERE card_uid = $1
              AND relative_path = $2 AND upload_status IN ('awaiting_init', 'pending')",
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
pub async fn commit_declaration_operation(
    conn: &mut TenantConn<'_>,
    operation_id: DeclarationOperationId,
    seed: &wyrd_spec::registry::DeclarationReplaySeed,
) -> Result<(), WyrdError> {
    let stored_response = serde_json::to_value(seed).map_err(WyrdError::from_spec_serialization)?;
    let result = sqlx::query(
        r"UPDATE wyrd.card_declaration_operations
              SET stored_response = $1, status = 'committed', updated_at = now()
            WHERE operation_id = $2 AND status = 'pending'",
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
///
/// # Panics
///
/// Panics if a reference lacks the space validated earlier in this function.
pub async fn select_card_uids_by_ref_batch(
    conn: &mut TenantConn<'_>,
    refs: &[CardRef],
) -> Result<Vec<(CardRef, CardUid)>, WyrdError> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    if refs.iter().any(|card_ref| card_ref.space.is_none()) {
        return Err(WyrdError::registry_invalid_card_spec(
            "CardRef.space is required at the registry boundary",
        ));
    }
    let mut query = QueryBuilder::<sqlx::Postgres>::new(
        "SELECT refs.kind, refs.space, refs.name, refs.version, cards.card_uid FROM (",
    );
    query.push_values(refs, |mut values, card_ref| {
        values
            .push_bind(card_ref.kind.wire_name())
            .push_bind(
                card_ref
                    .space
                    .as_ref()
                    .expect("invariant: CardRef spaces were validated above")
                    .as_str(),
            )
            .push_bind(card_ref.name.as_str())
            .push_bind(card_ref.version.as_str());
    });
    query.push(
        ") AS refs(kind, space, name, version) LEFT JOIN wyrd.cards cards \
         ON cards.kind = refs.kind \
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
                            && item
                                .space
                                .as_ref()
                                .is_some_and(|item_space| item_space.as_str() == space)
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

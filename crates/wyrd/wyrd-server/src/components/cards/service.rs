//! Card registration service orchestration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use uuid::Uuid;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_semver::{VersionBlock, VersionRange, VersionSpec};
use wyrd_spec::DataTenantId;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Spec, Status};
use wyrd_spec::error::{WyrdError, storage::WyrdStorageError};
use wyrd_spec::graph::{
    GraphError, RootPick, TopoOrder, build, canonical_order, graph_ready_submissions, pick_root,
    publication_validation_errors, relationships_from_spec, topo_sort, validate_composition,
};
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::{CardRef, CardRefIdentity, scope_child_card_refs};
use wyrd_spec::registry::{
    ArtifactInventoryResponse, ArtifactManifestEntry, CardLifecycleStatus, CardRegistrationOutcome,
    CardSubmission, CardSummary, CardUploadEntry, CardUploadPlan, CreateCardRequest,
    CreateCardResponse, DeleteCardResponse, GetCardResponse, ListCardsRequest, ListCardsResponse,
    ListVersionsResponse, RegistrationOperationId, RegistrationOutcomeKind, RegistrationReplaySeed,
    RelativeArtifactPath, canonical_artifact_manifest_hash,
};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::storage::{UploadId, UploadInitRequest, UploadPlan};
use wyrd_spec::vala::api::AuditEvent;
use wyrd_sql::CardStatus;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{
    CardArtifactManifestRow, CardDeleteState, CardManifestCompletionRow, CardQuery,
    CardReconcileClaim, CardRegistrationOperationRow, ListCursor, NewCardRow,
    NewRegistrationOperation, RECONCILE_KIND_BLOB, RECONCILE_KIND_CLEANUP,
    RECONCILE_KIND_FINALIZATION, RECONCILE_KIND_REGISTRATION, Resolution, SubmittedCardIdentity,
    activate_card, commit_registration_operation, fail_card, find_card_by_ref,
    get_card_by_uid as sql_get_card_by_uid,
    get_card_for_reconciliation as sql_get_card_for_reconciliation, get_latest_card_by_range,
    inbound_relationships, insert_artifact_manifest_rows, insert_card_row,
    insert_registration_operation, list_versions, lock_card_reconciliation_lease,
    lock_pending_card_for_activation, lock_version_line, lookup_existing_operation,
    lookup_expired_operation, lookup_operation_by_id, manifest_completion_rows,
    manifest_rows_for_init, mark_card_reconciliation_succeeded, mark_manifest_upload_initialized,
    mark_manifest_verified, persist_outbound_relationships, query_cards, recheck_active_card_refs,
    record_blob_failure, record_card_blob, record_card_reconciliation_failure,
    registration_request_hash, reschedule_card_reconciliation, resolve_version,
    schedule_card_reconciliation, soft_delete_card_by_ref, soft_delete_card_with_kind,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::storage::{artifact_metadata, multipart_uploads};
use wyrd_sql::row_types::cards::{CardRow, ParsedCardRow};
use wyrd_storage::StorageError;
use wyrd_storage::service::{upload_abort, upload_init};
use wyrd_storage::tenant_path;

use crate::audit;
use crate::components::auth::Caller;
use crate::components::cards::mapping::{existing_row_to_response, outcome_row_to_response};
use crate::components::cards::resolve::{
    ResolvedRefs, bind_card_references, resolve_card_references,
};
use crate::components::storage::routes::storage_caller;
use crate::state::{AppState, registry_db_error};

const DEFAULT_LIST_LIMIT: u32 = 50;

#[derive(Debug, Deserialize, Serialize)]
struct CardListCursor {
    created_at: DateTime<Utc>,
    card_uid: CardUid,
}

/// Load a fully hydrated Card by its tenant-scoped UID.
pub async fn get_card_by_uid(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<GetCardResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let row = sql_get_card_by_uid(&mut conn, card_uid).await?;
    let inbound = inbound_relationships(&mut conn, card_uid).await?;
    let inventory = artifact_metadata::list_for_card(&mut conn, card_uid.as_str())
        .await
        .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
    conn.commit().await.map_err(registry_db_error)?;
    hydrate_card(state, caller, row, inbound, inventory).await
}

/// Load a fully hydrated Card by its exact tenant-scoped reference.
pub async fn get_card_by_ref(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
) -> Result<GetCardResponse, WyrdError> {
    let space = card_ref.space.as_ref().ok_or_else(|| {
        WyrdError::registry_invalid_card_spec("CardRef.space is required for a card read")
    })?;
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let row = wyrd_sql::queries::cards::get_card_by_ref(
        &mut conn,
        card_ref.kind.clone(),
        space,
        &card_ref.name,
        &card_ref.version,
    )
    .await?;
    let inbound = inbound_relationships(&mut conn, &row.card_uid).await?;
    let inventory = artifact_metadata::list_for_card(&mut conn, row.card_uid.as_str())
        .await
        .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
    conn.commit().await.map_err(registry_db_error)?;
    hydrate_card(state, caller, row, inbound, inventory).await
}

/// Resolve and load the newest stable Active Card in one identity line.
pub async fn get_latest_card(
    state: &AppState,
    caller: &Caller,
    kind: CardKind,
    space: SpaceName,
    name: CardName,
) -> Result<GetCardResponse, WyrdError> {
    let range = VersionRange::default();
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let row = get_latest_card_by_range(&mut conn, kind, &space, &name, &range).await?;
    let inbound = inbound_relationships(&mut conn, &row.card_uid).await?;
    let inventory = artifact_metadata::list_for_card(&mut conn, row.card_uid.as_str())
        .await
        .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
    conn.commit().await.map_err(registry_db_error)?;
    hydrate_card(state, caller, row, inbound, inventory).await
}

/// List versions in one card identity line, excluding deleted rows.
pub async fn list_card_versions(
    state: &AppState,
    caller: &Caller,
    kind: CardKind,
    space: SpaceName,
    name: CardName,
    include_prerelease: bool,
) -> Result<ListVersionsResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let versions = list_versions(&mut conn, kind, &space, &name, include_prerelease).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(ListVersionsResponse { versions })
}

/// List metadata-only Cards with deterministic tenant-scoped keyset pagination.
pub async fn list_cards(
    state: &AppState,
    caller: &Caller,
    request: ListCardsRequest,
) -> Result<ListCardsResponse, WyrdError> {
    if request.status == Some(CardLifecycleStatus::Deleted) {
        return Ok(ListCardsResponse {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let limit = match request.limit {
        Some(value) => u32::try_from(value)
            .map_err(|_| WyrdError::registry_list_limit_out_of_range(value.unsigned_abs(), 200))?,
        None => DEFAULT_LIST_LIMIT,
    };
    let cursor = decode_list_cursor(request.cursor.as_deref(), limit)?;
    let query = CardQuery {
        kind: request.kind,
        space: request.space,
        name: request.name,
        version_range: request
            .version_range
            .map(VersionRange::parse_loose)
            .transpose()
            .map_err(|error| WyrdError::registry_invalid_version_block(error.to_string()))?,
        status: request.status.map(sql_status),
        filter: request.filter,
        include_prerelease: request.include_prerelease,
    };
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let page = query_cards(&mut conn, &query, cursor).await?;
    conn.commit().await.map_err(registry_db_error)?;
    let items = page
        .items
        .iter()
        .map(summary_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = page.next.as_ref().map(encode_list_cursor).transpose()?;
    Ok(ListCardsResponse { items, next_cursor })
}

/// Return the server-authoritative artifact inventory for one Card.
pub async fn list_card_artifacts(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<ArtifactInventoryResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let _ = sql_get_card_by_uid(&mut conn, card_uid).await?;
    let rows = artifact_metadata::list_for_card(&mut conn, card_uid.as_str())
        .await
        .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
    conn.commit().await.map_err(registry_db_error)?;
    let artifacts = rows
        .into_iter()
        .map(|row| {
            let path = tenant_path::validate(&row.storage_path, caller.data_tenant_id)
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            if path.card_uid != card_uid.as_str() {
                return Err(WyrdError::registry_invalid_card_spec(
                    "stored artifact is not bound to the requested Card",
                ));
            }
            Ok(wyrd_spec::registry::StoredArtifactEntry {
                relative_path: RelativeArtifactPath::new(&path.relative_path)
                    .map_err(WyrdError::from)?,
                sha256: row.sha256,
                size_bytes: row.size_bytes,
                content_type: row.content_type,
            })
        })
        .collect::<Result<Vec<_>, WyrdError>>()?;
    Ok(ArtifactInventoryResponse { artifacts })
}

async fn hydrate_card(
    state: &AppState,
    caller: &Caller,
    row: wyrd_sql::row_types::cards::ParsedCardRow,
    inbound: Vec<CardRef>,
    inventory: Vec<artifact_metadata::ArtifactMetadataRow>,
) -> Result<GetCardResponse, WyrdError> {
    let uri = row.card_blob_uri.as_deref().ok_or_else(|| {
        WyrdError::registry_card_not_found(
            "Card is not available until its immutable blob is active",
        )
    })?;
    let path = uri.strip_prefix("wyrd://").ok_or_else(|| {
        WyrdError::registry_invalid_card_spec("stored Card blob URI has an unsupported scheme")
    })?;
    let validated = tenant_path::validate(path, caller.data_tenant_id)
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    if validated.card_uid != row.card_uid.as_str() {
        return Err(WyrdError::registry_invalid_card_spec(
            "stored Card blob is not bound to its registry row",
        ));
    }
    let bytes = state
        .storage
        .get_object(&validated)
        .await
        .map_err(map_storage_error)?;
    let mut card: Card = serde_json::from_slice(&bytes).map_err(|error| {
        WyrdError::registry_invalid_card_spec(format!("stored Card blob failed to parse: {error}"))
    })?;
    if card.kind != row.kind
        || card.metadata.name != row.name
        || card.metadata.uid.as_ref() != Some(&row.card_uid)
        || card.metadata.resolved_pin() != Some(&row.version)
    {
        return Err(WyrdError::registry_invalid_card_spec(
            "stored Card blob identity does not match its registry row",
        ));
    }
    verify_card_read_integrity(&row, &card, &inventory, caller)?;
    card.relationships.inbound = inbound.iter().map(ToString::to_string).collect();
    card.relationships.inbound_refs = inbound
        .into_iter()
        .map(|card_ref| wyrd_spec::envelope::CardRelationship {
            card_ref,
            alias: None,
        })
        .collect();
    card.status = Some(Status {
        phase: row.status.as_db_str().to_owned(),
        message: None,
        updated_at: Some(row.updated_at),
    });
    Ok(GetCardResponse {
        card,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Compare the immutable blob and stored artifact inventory with one registry row.
///
/// The registry row is the durable index, while the blob and storage inventory
/// are independent materializations. A read must refuse to project a Card when
/// any of the three copies diverge, rather than returning a plausible envelope
/// with stale or substituted payload metadata.
///
/// # Errors
///
/// Returns a stable invalid-card-spec error when the blob's canonical spec or
/// artifact hash disagrees with the registry row, an inventory path is outside
/// the caller tenant/Card, an inventory size is invalid, or the canonical hash
/// reconstructed from inventory differs from the row and blob.
fn verify_card_read_integrity(
    row: &wyrd_sql::row_types::cards::ParsedCardRow,
    card: &Card,
    inventory: &[artifact_metadata::ArtifactMetadataRow],
    caller: &Caller,
) -> Result<(), WyrdError> {
    let manifest = inventory
        .iter()
        .map(|entry| {
            let path = tenant_path::validate(&entry.storage_path, caller.data_tenant_id)
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            if path.card_uid != row.card_uid.as_str() {
                return Err(WyrdError::registry_invalid_card_spec(
                    "stored artifact inventory is not bound to its registry row",
                ));
            }
            Ok(ArtifactManifestEntry {
                relative_path: RelativeArtifactPath::new(&path.relative_path)
                    .map_err(WyrdError::from)?,
                sha256: entry.sha256.clone(),
                size_bytes: u64::try_from(entry.size_bytes).map_err(|_| {
                    WyrdError::registry_invalid_card_spec(
                        "stored artifact inventory has a negative byte size",
                    )
                })?,
                content_type: entry.content_type.clone(),
            })
        })
        .collect::<Result<Vec<_>, WyrdError>>()?;
    verify_card_hashes(
        &row.spec_hash,
        row.artifact_hash.as_deref(),
        card,
        &manifest,
    )
}

/// Compare canonical blob hashes with the registry row and reconstructed inventory.
///
/// This is the pure integrity stage used after the server has tenant-validated
/// storage metadata. Keeping it independent of SQL and storage makes the three
/// materialized-value comparisons directly testable without a live server.
///
/// # Errors
///
/// Returns a stable invalid-card-spec error when the canonical blob spec hash,
/// blob metadata hash, or sorted inventory manifest hash differs from the row.
fn verify_card_hashes(
    row_spec_hash: &str,
    row_artifact_hash: Option<&str>,
    card: &Card,
    manifest: &[ArtifactManifestEntry],
) -> Result<(), WyrdError> {
    let blob_spec_hash = card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    if blob_spec_hash.as_str() != row_spec_hash
        || card
            .metadata
            .spec_hash
            .as_ref()
            .map(ToString::to_string)
            .as_deref()
            != Some(row_spec_hash)
    {
        return Err(WyrdError::registry_invalid_card_spec(
            "stored Card blob spec hash does not match its registry row",
        ));
    }
    let inventory_hash = canonical_artifact_manifest_hash(manifest)?;
    if inventory_hash.as_deref() != row_artifact_hash
        || card.metadata.artifact_hash.as_deref() != row_artifact_hash
    {
        return Err(WyrdError::registry_invalid_card_spec(
            "stored Card blob artifact hash does not match its registry inventory",
        ));
    }
    Ok(())
}

fn summary_from_row(row: &CardRow) -> Result<CardSummary, WyrdError> {
    let status = CardStatus::from_db_str(&row.status)
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(CardSummary {
        card_uid: CardUid::from_uuid(row.card_uid).map_err(WyrdError::from_card_uid_error)?,
        kind: CardKind::from_wire_name(&row.kind)
            .ok_or_else(|| WyrdError::registry_invalid_card_spec("stored Card kind is invalid"))?,
        space: SpaceName::new(row.space.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?,
        name: CardName::new(row.name.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?,
        version: row
            .version
            .parse::<VersionBlock>()
            .map_err(|error| WyrdError::registry_invalid_version_block(error.to_string()))?,
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        status: lifecycle_status(status)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn lifecycle_status(status: CardStatus) -> Result<CardLifecycleStatus, WyrdError> {
    Ok(match status {
        CardStatus::Pending => CardLifecycleStatus::Pending,
        CardStatus::Active => CardLifecycleStatus::Active,
        CardStatus::Deprecated => CardLifecycleStatus::Deprecated,
        CardStatus::Deleted => CardLifecycleStatus::Deleted,
        CardStatus::Failed => CardLifecycleStatus::Failed,
        CardStatus::Expired => CardLifecycleStatus::Expired,
    })
}

fn sql_status(status: CardLifecycleStatus) -> CardStatus {
    match status {
        CardLifecycleStatus::Pending => CardStatus::Pending,
        CardLifecycleStatus::Active => CardStatus::Active,
        CardLifecycleStatus::Deprecated => CardStatus::Deprecated,
        CardLifecycleStatus::Deleted => CardStatus::Deleted,
        CardLifecycleStatus::Failed => CardStatus::Failed,
        CardLifecycleStatus::Expired => CardStatus::Expired,
    }
}

fn decode_list_cursor(value: Option<&str>, limit: u32) -> Result<ListCursor, WyrdError> {
    let Some(value) = value else {
        return Ok(ListCursor {
            after_created_at: None,
            after_uid: None,
            limit,
        });
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| WyrdError::registry_invalid_card_spec("cursor is not valid base64"))?;
    let token: CardListCursor = serde_json::from_slice(&bytes)
        .map_err(|_| WyrdError::registry_invalid_card_spec("cursor payload is invalid"))?;
    Ok(ListCursor {
        after_created_at: Some(token.created_at),
        after_uid: Some(token.card_uid),
        limit,
    })
}

fn encode_list_cursor(cursor: &ListCursor) -> Result<String, WyrdError> {
    let (Some(created_at), Some(card_uid)) = (cursor.after_created_at, cursor.after_uid.clone())
    else {
        return Err(WyrdError::internal("next card cursor is incomplete"));
    };
    let bytes = serde_json::to_vec(&CardListCursor {
        created_at,
        card_uid,
    })
    .map_err(|error| WyrdError::internal(format!("card cursor serialization failed: {error}")))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Deterministic request plan produced before the write transaction opens.
struct RegistrationPlan {
    submissions: Vec<CardSubmission>,
    request_hash: String,
    order: TopoOrder,
    root: RootPick,
    external_refs: ResolvedRefs,
}

/// Existing row plus the outcome implied by the authored version mode.
struct ExistingNode {
    row: ParsedCardRow,
    outcome: RegistrationOutcomeKind,
}

/// Register a composite card request through the server-owned transaction.
///
/// This operation resolves references, applies idempotency replay, writes the
/// registration transaction, and initializes any artifact uploads.
///
/// `allowed` is the route's `card:write` verdict. A fresh write appends it on
/// the registration transaction before any mutation, so both commit or roll
/// back together. Every outcome that commits no registration — replay,
/// validation or dependency failure, a lost idempotency race, or a rolled-back
/// write — records it standalone once instead. Upload initialization runs
/// after that point and never records it again.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the verdict cannot be recorded,
/// and otherwise the validation, idempotency, dependency, registry, or upload
/// failure the registration raised.
#[tracing::instrument(skip(state, caller, allowed), fields(operation = "card.registration"))]
pub async fn register_card(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
    allowed: &AuditEvent,
) -> Result<CreateCardResponse, WyrdError> {
    let written = async {
        let request_hash = hash_request(&request)?;
        if let Some(replayed) = replay(state, caller, idempotency_key, &request_hash).await? {
            return Ok((replayed, false));
        }
        validate_request(&request)?;
        let (order, root) = plan_registration_graph(&request.submissions)?;
        let external_refs = resolve_external(state, caller, &request.submissions).await?;
        let plan = plan_registration(request, request_hash, external_refs, order, root);
        write_registration(state, caller, idempotency_key, plan, allowed).await
    }
    .await;
    let (operation_id, seed) = match written {
        Ok((written, true)) => written,
        Ok((written, false)) => {
            audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, allowed).await?;
            written
        }
        Err(error) => {
            audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, allowed).await?;
            return Err(error);
        }
    };
    initialize_uploads(state, caller, operation_id, seed, idempotency_key).await
}

/// Return a committed response for an identical idempotency key.
async fn replay(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<(RegistrationOperationId, RegistrationReplaySeed)>, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let operation = match lookup_existing_operation(&mut conn, caller.principal.id, idempotency_key)
        .await?
    {
        Some(operation) => Some(operation),
        None => lookup_expired_operation(&mut conn, caller.principal.id, idempotency_key).await?,
    };
    conn.commit().await.map_err(registry_db_error)?;
    let Some(operation) = operation else {
        return Ok(None);
    };
    let operation_id = RegistrationOperationId::new(operation.operation_id);
    replay_operation(operation, request_hash, idempotency_key)
        .map(|seed| seed.map(|seed| (operation_id, seed)))
}

/// Validate and decode one persisted operation response.
fn replay_operation(
    operation: CardRegistrationOperationRow,
    request_hash: &str,
    idempotency_key: &str,
) -> Result<Option<RegistrationReplaySeed>, WyrdError> {
    if operation.status == "expired" {
        return Err(WyrdError::RegistryOperationExpired {
            message: "registration operation has expired".to_owned(),
            details: serde_json::json!({ "operation_id": operation.operation_id }),
        });
    }
    if operation.request_hash != request_hash {
        return Err(idempotency_conflict(idempotency_key));
    }
    let Some(stored) = operation.stored_response else {
        return Ok(None);
    };
    let mut seed: RegistrationReplaySeed = serde_json::from_value(stored)
        .map_err(|error| registry_db_error(format!("invalid stored response: {error}")))?;
    for outcome in &mut seed.outcomes {
        outcome.outcome = RegistrationOutcomeKind::IdempotentNoop;
    }
    Ok(Some(seed))
}

/// Poll a pending idempotency winner with a bounded backoff schedule.
async fn wait_for_replay(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<(RegistrationOperationId, RegistrationReplaySeed), WyrdError> {
    for delay_ms in [50_u64, 100, 250, 500, 1_000, 1_000, 1_000] {
        if let Some(result) = replay(state, caller, idempotency_key, request_hash).await? {
            return Ok(result);
        }
        tokio::time::sleep(StdDuration::from_millis(delay_ms)).await;
    }
    Err(WyrdError::Conflict {
        message: "registration winner did not commit within the retry window".to_owned(),
        details: serde_json::json!({ "idempotency_key": idempotency_key }),
    })
}

/// Initialize pending artifact uploads after the composite transaction commits.
#[tracing::instrument(
    skip(state, caller, seed),
    fields(operation = "card.registration.initialize_uploads")
)]
async fn initialize_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    seed: RegistrationReplaySeed,
    idempotency_key: &str,
) -> Result<CreateCardResponse, WyrdError> {
    let mut response = CreateCardResponse {
        root: seed.root.clone(),
        outcomes: seed.outcomes.clone(),
        upload_plans: Vec::new(),
    };
    let semaphore = Arc::new(Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();
    for outcome in &response.outcomes {
        let Some(card_uid) = outcome.card_ref.uid.as_ref() else {
            continue;
        };
        let state = state.clone();
        let caller = caller.clone();
        let card_uid = card_uid.clone();
        let card_ref = outcome.card_ref.clone();
        let allowed_paths = seed
            .artifact_manifest_paths
            .iter()
            .find(|(reference, _)| reference.same_identity(&card_ref))
            .map(|(_, paths)| {
                paths
                    .iter()
                    .map(|path| path.as_str().to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.map_err(|_| {
                WyrdError::internal("registration upload initialization semaphore closed")
            })?;
            let entries = tokio::time::timeout(
                StdDuration::from_secs(30),
                initialize_card_uploads(&state, &caller, operation_id, &card_uid, &allowed_paths),
            )
            .await
            .map_err(|_| WyrdError::RequestTimeout {
                message: "card upload initialization timed out".to_owned(),
                details: serde_json::json!({ "card_uid": card_uid }),
            })??;
            Ok::<_, WyrdError>((card_ref, entries))
        });
    }
    let mut plans = Vec::new();
    let mut initialization_error = None;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok((card_ref, entries))) if !entries.is_empty() => {
                plans.push(CardUploadPlan { card_ref, entries });
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::error!(%error, "card upload initialization task failed");
                if initialization_error.is_none() {
                    initialization_error = Some(error);
                }
            }
            Err(error) => {
                let error =
                    WyrdError::internal(format!("upload initialization task failed: {error}"));
                tracing::error!(%error, "card upload initialization task failed");
                if initialization_error.is_none() {
                    initialization_error = Some(error);
                }
            }
        }
    }
    if let Some(error) = initialization_error {
        abort_pending_registration_cards(state, caller, &response.outcomes, idempotency_key).await;
        return Err(error);
    }
    plans.sort_by(|left, right| {
        left.card_ref
            .kind
            .wire_name()
            .cmp(right.card_ref.kind.wire_name())
            .then_with(|| left.card_ref.space.cmp(&right.card_ref.space))
            .then_with(|| left.card_ref.name.cmp(&right.card_ref.name))
    });
    response.upload_plans = plans;

    // Metadata-only cards, and replayed artifact cards whose manifests are
    // already verified, complete in the same server-owned lifecycle seam. A
    // card with a still-pending upload plan remains Pending for the client
    // upload phase.
    for index in 0..response.outcomes.len() {
        let outcome = response.outcomes[index].clone();
        if outcome.status != CardLifecycleStatus::Pending
            || response
                .upload_plans
                .iter()
                .any(|plan| plan.card_ref.same_identity(&outcome.card_ref))
        {
            continue;
        }
        let Some(card_uid) = outcome.card_ref.uid.as_ref() else {
            continue;
        };
        let completion_state = load_card_completion_state(state, caller, card_uid).await?;
        if completion_state
            .manifests
            .iter()
            .any(|manifest| !manifest_ready_for_completion(manifest))
        {
            continue;
        }
        response.outcomes[index] =
            match complete_card(state, caller, card_uid, idempotency_key).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    abort_pending_registration_cards(
                        state,
                        caller,
                        &response.outcomes,
                        idempotency_key,
                    )
                    .await;
                    return Err(error);
                }
            };
    }
    Ok(response)
}

/// Initialize every pending manifest row for one card.
async fn initialize_card_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    card_uid: &CardUid,
    allowed_paths: &[String],
) -> Result<Vec<CardUploadEntry>, WyrdError> {
    let rows = match load_manifest_rows(state, caller, card_uid).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%card_uid, "artifact upload initialization failed: manifest lookup failed");
            tracing::error!(%error, %card_uid, "manifest lookup failed after registration");
            return Err(error);
        }
    };
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        if !allowed_paths.iter().any(|path| path == &row.relative_path) {
            continue;
        }
        let entry = match initialize_manifest_row(state, caller, operation_id, card_uid, &row).await
        {
            Ok(entry) => entry,
            Err(error) => {
                tracing::error!(%card_uid, path = %row.relative_path, "artifact upload initialization failed");
                tracing::error!(%error, %card_uid, path = %row.relative_path, "upload init failed");
                return Err(error);
            }
        };
        entries.push(entry);
    }
    Ok(entries)
}

/// Abort every still-pending Card when post-commit initialization fails.
async fn abort_pending_registration_cards(
    state: &AppState,
    caller: &Caller,
    outcomes: &[CardRegistrationOutcome],
    idempotency_key: &str,
) {
    for outcome in outcomes {
        if outcome.status != CardLifecycleStatus::Pending {
            continue;
        }
        let Some(card_uid) = outcome.card_ref.uid.as_ref() else {
            continue;
        };
        if let Err(error) = abort_card(state, caller, card_uid, idempotency_key).await {
            tracing::error!(%error, %card_uid, "registration failure cleanup did not complete");
        }
    }
}

/// Load manifest rows in a short tenant transaction.
async fn load_manifest_rows(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<Vec<CardArtifactManifestRow>, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let rows = manifest_rows_for_init(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(rows)
}

/// Initialize one manifest row through the storage service and persist its upload id.
async fn initialize_manifest_row(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    card_uid: &CardUid,
    row: &CardArtifactManifestRow,
) -> Result<CardUploadEntry, WyrdError> {
    let key = IdempotencyKey::new(format!(
        "registration-{}-{}",
        operation_id.as_uuid(),
        row.relative_path
    ))
    .map_err(|error| WyrdError::internal(format!("upload idempotency key invalid: {error}")))?;
    let initialized = tokio::time::timeout(
        StdDuration::from_secs(5),
        upload_init(
            &state.storage,
            state.postgres.wyrd(),
            &storage_caller(caller),
            Some(key),
            UploadInitRequest {
                card_uid: card_uid.clone(),
                relative_path: row.relative_path.clone(),
                expected_sha256: row.expected_sha256.clone(),
                expected_size_bytes: u64::try_from(row.size_bytes)
                    .map_err(|_| WyrdError::internal("manifest size became negative"))?,
                content_type: row.content_type.clone(),
            },
        ),
    )
    .await
    .map_err(|_| WyrdError::RequestTimeout {
        message: "artifact upload initialization timed out".to_owned(),
        details: serde_json::json!({ "card_uid": card_uid, "relative_path": row.relative_path }),
    })??;
    let initialized_upload_id = initialized.upload_id.clone();
    let upload_uuid = initialized_upload_id
        .as_uuid()
        .map_err(|error| WyrdError::internal(format!("upload id invalid: {error}")))?;
    let entry = match card_upload_entry(&row.relative_path, initialized.upload_id, initialized.plan)
    {
        Ok(entry) => entry,
        Err(error) => {
            abort_initialized_upload(state, caller, initialized_upload_id).await;
            return Err(error);
        }
    };
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let persisted = async {
        mark_manifest_upload_initialized(&mut conn, card_uid, &row.relative_path, upload_uuid)
            .await?;
        conn.commit().await.map_err(registry_db_error)
    }
    .await;
    if let Err(error) = persisted {
        abort_initialized_upload(state, caller, initialized_upload_id).await;
        return Err(error);
    }
    Ok(entry)
}

/// Abort a storage upload when its post-init manifest transaction fails.
async fn abort_initialized_upload(state: &AppState, caller: &Caller, upload_id: UploadId) {
    if let Err(error) = upload_abort(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller(caller),
        upload_id,
        None,
    )
    .await
    {
        tracing::error!(%error, "initialized upload cleanup failed");
    }
}

/// Project the complete storage upload contract into the registry response.
fn card_upload_entry(
    relative_path: &str,
    upload_id: UploadId,
    plan: UploadPlan,
) -> Result<CardUploadEntry, WyrdError> {
    Ok(CardUploadEntry {
        relative_path: RelativeArtifactPath::new(relative_path).map_err(WyrdError::from)?,
        upload_id,
        plan,
    })
}

/// Resolve external references before opening the composite write transaction.
async fn resolve_external(
    state: &AppState,
    caller: &Caller,
    submissions: &[CardSubmission],
) -> Result<ResolvedRefs, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let resolved = resolve_card_references(&mut conn, submissions).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(resolved)
}

/// Validate composition and build graph order before any registry I/O.
///
/// # Errors
/// Returns a stable Wyrd error when graph construction, root selection, or
/// Service-root composition validation fails.
fn plan_registration_graph(
    submissions: &[CardSubmission],
) -> Result<(TopoOrder, RootPick), WyrdError> {
    let graph_submissions = graph_ready_submissions(submissions).map_err(graph_error)?;
    let (nodes, edges) = build(&graph_submissions).map_err(graph_error)?;
    let order = topo_sort(&nodes, &edges).map_err(graph_error)?;
    let root = pick_root(&order).map_err(graph_error)?;
    validate_composition(&graph_submissions, &root).map_err(graph_error)?;
    Ok((order, root))
}

/// Assemble the validated graph with resolved external dependencies.
fn plan_registration(
    request: CreateCardRequest,
    request_hash: String,
    external_refs: ResolvedRefs,
    order: TopoOrder,
    root: RootPick,
) -> RegistrationPlan {
    RegistrationPlan {
        submissions: request.submissions,
        request_hash,
        order,
        root,
        external_refs,
    }
}

/// Reserve idempotency and atomically persist every topo-ordered node and audit.
///
/// The `allowed` verdict is appended before any mutation and commits with the
/// registration. The returned flag is `true` only when that transaction
/// committed; a lost idempotency race rolls it back and returns the winner's
/// replay with `false`, leaving the caller to record the verdict standalone.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the append fails, and the
/// dependency, idempotency, validation, or registry failure otherwise; nothing
/// commits on any error.
#[tracing::instrument(
    skip(state, caller, plan, allowed),
    fields(operation = "card.registration.write")
)]
async fn write_registration(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    mut plan: RegistrationPlan,
    allowed: &AuditEvent,
) -> Result<((RegistrationOperationId, RegistrationReplaySeed), bool), WyrdError> {
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    audit::append_on(&mut conn, allowed).await?;
    // Recheck before reserving idempotency so a dependency rejection rolls back
    // the entire attempt, including its bookkeeping row. The row locks remain
    // held while cards and relationships are written below.
    let external_identities = plan
        .external_refs
        .iter()
        .map(|(card_ref, _)| card_ref.clone())
        .collect::<Vec<_>>();
    plan.external_refs = recheck_active_card_refs(&mut conn, &external_identities).await?;
    let inserted = insert_registration_operation(
        &mut conn,
        NewRegistrationOperation {
            operation_id,
            principal_id: caller.principal.id,
            idempotency_key,
            request_hash: &plan.request_hash,
        },
    )
    .await?;
    if !inserted {
        drop(conn);
        let replayed = wait_for_replay(state, caller, idempotency_key, &plan.request_hash).await?;
        return Ok((replayed, false));
    }

    let mut sibling_uids = HashMap::new();
    let mut outcomes = Vec::with_capacity(plan.order.nodes.len());
    let mut resolved_root = None;
    for node in &plan.order.nodes {
        let is_root = same_identity(&node.card_ref, &plan.root.root);
        let authored = find_submission(&plan.submissions, &node.card_ref)?;
        let mut submission = authored.clone();
        let mut spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
        bind_card_references(&mut spec, &plan.external_refs, &sibling_uids)?;
        submission.spec =
            serde_json::to_value(&spec).map_err(WyrdError::from_spec_serialization)?;
        let outcome = persist_node(&mut conn, caller, operation_id, &mut submission).await?;
        sibling_uids.insert(
            graph_identity(&outcome.card_ref),
            outcome
                .card_ref
                .uid
                .clone()
                .ok_or_else(|| WyrdError::internal("registered outcome is missing uid"))?,
        );
        if is_root {
            resolved_root = Some(outcome.card_ref.clone());
        }
        outcomes.push(outcome);
    }
    let root =
        resolved_root.ok_or_else(|| WyrdError::internal("root registration outcome is missing"))?;
    let response = CreateCardResponse {
        root,
        outcomes,
        upload_plans: Vec::new(),
    };
    let seed = replay_seed(&response, &plan.submissions)?;
    commit_registration_operation(&mut conn, operation_id, &seed).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(((operation_id, seed), true))
}

/// Resolve one node's version, deduplicate when possible, and persist when fresh.
async fn persist_node(
    conn: &mut TenantConn<'_>,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    submission: &mut CardSubmission,
) -> Result<CardRegistrationOutcome, WyrdError> {
    let mut card = submission_card(submission)?;
    let outbound_refs = scope_child_card_refs(&card.spec);
    let spec_hash = card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    let artifact_hash = canonical_artifact_manifest_hash(&submission.artifacts)?;
    let space = card
        .metadata
        .space
        .clone()
        .ok_or_else(|| WyrdError::registry_invalid_card_spec("metadata.space is required"))?;
    lock_version_line(conn, card.kind.clone(), &space, &card.metadata.name).await?;
    if let Some(existing) = resolve_existing(
        conn,
        &mut card,
        spec_hash.as_str(),
        artifact_hash.as_deref(),
    )
    .await?
    {
        persist_outbound_relationships(conn, &existing.row.card_uid, &outbound_refs).await?;
        return Ok(existing_row_to_response(&existing.row, existing.outcome));
    }
    let card_uid = CardUid::from_uuid(Uuid::now_v7()).map_err(WyrdError::from_card_uid_error)?;
    let row = insert_card_row(
        conn,
        NewCardRow {
            card: &card,
            card_uid,
            principal_id: caller.principal.id,
            operation_id,
            status: CardStatus::Pending,
            spec_hash: spec_hash.as_str(),
            artifact_hash: artifact_hash.as_deref(),
        },
    )
    .await?;
    persist_outbound_relationships(conn, &row.card_uid, &outbound_refs).await?;
    insert_artifact_manifest_rows(conn, &row.card_uid, &submission.artifacts).await?;
    if matches!(card.kind, CardKind::Service | CardKind::Agent) {
        upsert_service_account_from_card(conn, &row.card_uid, &card, &caller.principal).await?;
    }
    Ok(outcome_row_to_response(
        &row,
        RegistrationOutcomeKind::Registered,
    ))
}

/// Return an identical existing row or pin the fresh resolved version on the card.
async fn resolve_existing(
    conn: &mut TenantConn<'_>,
    card: &mut Card,
    spec_hash: &str,
    artifact_hash: Option<&str>,
) -> Result<Option<ExistingNode>, WyrdError> {
    let space = card
        .metadata
        .space
        .as_ref()
        .ok_or_else(|| WyrdError::registry_invalid_card_spec("metadata.space is required"))?;
    match card.metadata.version.clone() {
        Some(VersionSpec::Pin(version)) => {
            let existing = find_card_by_ref(
                conn,
                card.kind.clone(),
                space,
                &card.metadata.name,
                &version,
            )
            .await?;
            if let Some(row) = &existing
                && (row.spec_hash != spec_hash || row.artifact_hash.as_deref() != artifact_hash)
            {
                return Err(WyrdError::registry_spec_drift(
                    row.card_uid.to_string(),
                    row.spec_hash.clone(),
                    spec_hash,
                ));
            }
            Ok(existing.map(|row| ExistingNode {
                row,
                outcome: RegistrationOutcomeKind::IdempotentNoop,
            }))
        }
        authored => match resolve_version(
            conn,
            card.kind.clone(),
            space,
            &card.metadata.name,
            authored.as_ref(),
            card.metadata.bump.as_ref(),
            SubmittedCardIdentity {
                spec_hash,
                artifact_hash,
            },
        )
        .await?
        {
            Resolution::Fresh(version) => {
                card.metadata.version = Some(VersionSpec::Pin(version));
                Ok(None)
            }
            Resolution::Deduplicated { version } => find_card_by_ref(
                conn,
                card.kind.clone(),
                space,
                &card.metadata.name,
                &version,
            )
            .await
            .map(|row| {
                row.map(|row| ExistingNode {
                    row,
                    outcome: RegistrationOutcomeKind::Deduplicated,
                })
            }),
        },
    }
}

/// Enforce request-wide invariants before database access.
fn validate_request(request: &CreateCardRequest) -> Result<(), WyrdError> {
    if request.submissions.len() > 1
        && request
            .submissions
            .iter()
            .any(|submission| !submission.artifacts.is_empty())
    {
        return Err(WyrdError::RegistryHeavyArtifactNotSoleSubmission {
            message: "artifact-bearing cards must be registered alone".to_owned(),
            details: serde_json::json!({ "submission_count": request.submissions.len() }),
        });
    }
    for submission in &request.submissions {
        if submission.api_version.as_str() != ApiVersion::V1 {
            return Err(WyrdError::registry_invalid_card_spec(format!(
                "expected apiVersion {}, got {}",
                ApiVersion::V1,
                submission.api_version
            )));
        }
        let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
        match &spec {
            Spec::Agent(agent) => {
                if let Some(error) =
                    publication_validation_errors(&agent.publishes_to, "spec.publishes_to")
                        .into_iter()
                        .next()
                {
                    return Err(error);
                }
            }
            Spec::Service(service) => {
                if let Some(error) =
                    publication_validation_errors(&service.publishes_to, "spec.publishes_to")
                        .into_iter()
                        .next()
                {
                    return Err(error);
                }
                for (index, component) in service.components.iter().enumerate() {
                    let field = format!("spec.components[{index}].publishes_to");
                    if let Some(error) =
                        publication_validation_errors(&component.publishes_to, &field)
                            .into_iter()
                            .next()
                    {
                        return Err(error);
                    }
                }
            }
            _ => {}
        }
        let computed = canonical_artifact_manifest_hash(&submission.artifacts)?;
        if submission.metadata.artifact_hash.as_deref() != computed.as_deref()
            && submission.metadata.artifact_hash.is_some()
        {
            return Err(WyrdError::RegistryManifestHashMismatch {
                message: "metadata.artifact_hash does not match the submitted manifest".to_owned(),
                details: serde_json::json!({
                    "expected": submission.metadata.artifact_hash,
                    "computed": computed,
                }),
            });
        }
    }
    Ok(())
}

/// Compute the canonical request hash required by idempotency replay.
fn hash_request(request: &CreateCardRequest) -> Result<String, WyrdError> {
    let canonical = canonical_order(&request.submissions)
        .into_iter()
        .map(|index| &request.submissions[index])
        .collect::<Vec<_>>();
    let hashes = canonical
        .iter()
        .map(|submission| canonical_artifact_manifest_hash(&submission.artifacts))
        .collect::<Result<Vec<_>, _>>()?;
    registration_request_hash(&canonical, &hashes)
}

/// Strip upload URLs from the durable response and retain only replay inputs.
fn replay_seed(
    response: &CreateCardResponse,
    submissions: &[CardSubmission],
) -> Result<RegistrationReplaySeed, WyrdError> {
    let artifact_manifest_paths = response
        .outcomes
        .iter()
        .filter_map(|outcome| {
            let submission = find_submission(submissions, &outcome.card_ref).ok()?;
            (!submission.artifacts.is_empty()).then(|| {
                (
                    outcome.card_ref.clone(),
                    submission
                        .artifacts
                        .iter()
                        .map(|artifact| artifact.relative_path.clone())
                        .collect(),
                )
            })
        })
        .collect();
    Ok(RegistrationReplaySeed {
        root: response.root.clone(),
        outcomes: response.outcomes.clone(),
        artifact_manifest_paths,
    })
}

/// Decode one submission into the typed card envelope persisted by SQL.
fn submission_card(submission: &CardSubmission) -> Result<Card, WyrdError> {
    let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    let relationships = relationships_from_spec(&spec);
    Ok(Card {
        api_version: submission.api_version.clone(),
        kind: submission.kind.clone(),
        metadata: submission.metadata.clone(),
        spec,
        relationships,
        status: None,
    })
}

/// Find the authored submission represented by a graph node.
fn find_submission<'a>(
    submissions: &'a [CardSubmission],
    node: &CardRef,
) -> Result<&'a CardSubmission, WyrdError> {
    submissions
        .iter()
        .find(|submission| {
            submission.kind == node.kind
                && submission.metadata.name == node.name
                && submission.metadata.space.as_ref() == node.space.as_ref()
                && submission
                    .metadata
                    .resolved_pin()
                    .is_none_or(|version| version == &node.version)
        })
        .ok_or_else(|| WyrdError::internal("graph node lost its submission"))
}

/// Build the exact sibling identity key.
fn graph_identity(card_ref: &CardRef) -> CardRefIdentity {
    card_ref.identity_key()
}

/// Compare two graph identities without the server-derived UID.
fn same_identity(left: &CardRef, right: &CardRef) -> bool {
    left.same_identity(right)
}

/// Map graph failures to the stable registry error catalog.
fn graph_error(error: GraphError) -> WyrdError {
    match error {
        GraphError::Cycle { cycle } => WyrdError::RegistryDependencyCycle {
            message: "card submission graph contains a dependency cycle".to_owned(),
            details: serde_json::json!({ "cycle": cycle }),
        },
        GraphError::Empty => WyrdError::registry_invalid_card_spec("submissions must not be empty"),
        GraphError::MissingSpace => {
            WyrdError::registry_invalid_card_spec("metadata.space is required")
        }
        GraphError::MultipleRoots { candidates } => {
            WyrdError::internal(format!("multiple graph roots: {candidates:?}"))
        }
        GraphError::DuplicateIdentity { candidates } => WyrdError::SpecDuplicateSubmission {
            message: "submission identities must be unique within a registration request"
                .to_owned(),
            details: serde_json::json!({ "candidates": candidates }),
        },
        GraphError::InvalidServiceComponentKind {
            service,
            alias,
            component,
            field,
        } => WyrdError::SpecInvalidServiceComponentKind {
            message: format!(
                "Service {} component {alias} cannot reference {}",
                service.name,
                component.kind.wire_name()
            ),
            details: serde_json::json!({
                "service": service,
                "alias": alias,
                "component_ref": component,
                "field": field,
            }),
        },
        GraphError::UnpublishedObservabilityPeer { root, peer } => {
            WyrdError::SpecUnpublishedObservabilityPeer {
                message: format!(
                    "{} {} has no submitted publisher in Service-root bundle {}",
                    peer.kind.wire_name(),
                    peer.name,
                    root.name
                ),
                details: serde_json::json!({
                    "root": root,
                    "peer": peer,
                    "publisher_kinds": ["Data", "Model", "Agent", "Service"],
                }),
            }
        }
        GraphError::InvalidSpec { message } => WyrdError::registry_invalid_card_spec(message),
    }
}

/// Build the stable idempotency conflict response.
fn idempotency_conflict(idempotency_key: &str) -> WyrdError {
    WyrdError::RegistryIdempotencyConflict {
        message: "idempotency key was already used for different content".to_owned(),
        details: serde_json::json!({ "idempotency_key": idempotency_key }),
    }
}

/// Card and manifest state loaded before completion or abort storage IO.
struct CardCompletionState {
    card: ParsedCardRow,
    manifests: Vec<CardManifestCompletionRow>,
}

/// Complete one pending Card after its storage uploads have been verified.
///
/// Storage IO runs before the final short SQL transaction. The transition is
/// therefore safe to retry without holding a database connection across
/// backend calls. The route validates and forwards the caller's idempotency
/// key so transport retries retain the registration saga key.
#[tracing::instrument(
    skip(state, caller, idempotency_key),
    fields(
        operation = "card.registration.complete",
        idempotency_key_present = !idempotency_key.is_empty()
    )
)]
pub async fn complete_card(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    idempotency_key: &str,
) -> Result<CardRegistrationOutcome, WyrdError> {
    complete_card_inner(state, caller, card_uid, idempotency_key, None, true).await
}

/// Execute completion for a reconciler claim without changing its durable lease.
pub(crate) async fn reconcile_card_claim(
    state: &AppState,
    caller: &Caller,
    claim: &CardReconcileClaim,
) -> Result<(), WyrdError> {
    let card_uid = CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?;
    if claim.reconcile_kind == RECONCILE_KIND_REGISTRATION {
        initialize_reconciled_uploads(state, caller, claim, &card_uid).await?;
    }
    let completion_state = load_card_reconciliation_state(state, caller, &card_uid).await?;
    if completion_state
        .manifests
        .iter()
        .any(manifest_needs_cleanup)
    {
        let cleanup_failures = cleanup_card_artifacts(
            state,
            caller,
            &card_uid,
            &completion_state.card,
            &completion_state.manifests,
        )
        .await;
        commit_card_failure(state, caller, &card_uid, cleanup_failures.is_empty()).await?;
        if !cleanup_failures.is_empty() {
            return Err(WyrdError::RegistryArtifactVerifyFailed {
                message: "card registration cleanup remains incomplete".to_owned(),
                details: serde_json::json!({ "failure_count": cleanup_failures.len() }),
            });
        }
        return Ok(());
    }
    complete_card_inner(
        state,
        caller,
        &card_uid,
        &format!("reconciliation-{}", claim.card_uid),
        Some(claim.reconcile_lease_owner),
        false,
    )
    .await
    .map(|_| ())
}

/// Retry cleanup for a claimed deleted or failed Card.
pub(crate) async fn reconcile_cleanup_claim(
    state: &AppState,
    caller: &Caller,
    claim: &CardReconcileClaim,
) -> Result<(), WyrdError> {
    let card_uid = CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?;
    let completion_state = load_card_reconciliation_state(state, caller, &card_uid).await?;
    let cleanup_failures = cleanup_card_artifacts(
        state,
        caller,
        &card_uid,
        &completion_state.card,
        &completion_state.manifests,
    )
    .await;
    if !cleanup_failures.is_empty() {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "card cleanup remains incomplete".to_owned(),
            details: serde_json::json!({ "failure_count": cleanup_failures.len() }),
        });
    }
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    if !lock_card_reconciliation_lease(&mut conn, &card_uid, claim.reconcile_lease_owner).await? {
        conn.commit().await.map_err(registry_db_error)?;
        return Ok(());
    }
    artifact_metadata::delete_for_card(&mut conn, card_uid.as_str())
        .await
        .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
    mark_card_reconciliation_succeeded(&mut conn, &card_uid, Some(claim.reconcile_lease_owner))
        .await?;
    conn.commit().await.map_err(registry_db_error)
}

/// Record a failed reconciler attempt and report a dead-letter transition.
///
/// Reconciliation is a background registry transition: it evaluates no principal
/// permission, so the dead-letter outcome it owes is the durable reconciliation
/// row it just wrote plus structured diagnostics, never a canonical audit event.
pub(crate) async fn record_reconciliation_failure(
    state: &AppState,
    caller: &Caller,
    claim: &CardReconcileClaim,
    error: &WyrdError,
    next_attempt_at: DateTime<Utc>,
) -> Result<bool, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let dead_lettered = record_card_reconciliation_failure(
        &mut conn,
        &CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?,
        claim.reconcile_lease_owner,
        next_attempt_at,
        error.code(),
        reconciliation_error_message(&claim.reconcile_kind),
    )
    .await?;
    if dead_lettered {
        tracing::warn!(
            card_uid = %claim.card_uid,
            reconcile_kind = ?claim.reconcile_kind,
            error_code = error.code(),
            "card reconciliation dead-lettered"
        );
    }
    conn.commit().await.map_err(registry_db_error)?;
    Ok(dead_lettered)
}

/// Return a reconciler claim to durable retry state when its lease budget is
/// too small for another storage operation.
pub(crate) async fn reschedule_reconciliation_claim(
    state: &AppState,
    caller: &Caller,
    claim: &CardReconcileClaim,
    next_attempt_at: DateTime<Utc>,
) -> Result<(), WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    reschedule_card_reconciliation(
        &mut conn,
        &CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?,
        claim.reconcile_lease_owner,
        next_attempt_at,
        "WYRD_REGISTRY_503_RECONCILIATION_LEASE_BUDGET",
        "reconciliation lease budget was exhausted before storage work began",
    )
    .await?;
    conn.commit().await.map_err(registry_db_error)
}

/// Build the internal caller used by a tenant-scoped reconciliation attempt.
pub(crate) fn reconciliation_caller(tenant_id: DataTenantId) -> Caller {
    Caller {
        data_tenant_id: tenant_id,
        principal: Principal::new(
            PLATFORM_AUDIT_PRINCIPAL,
            PrincipalKind::User,
            tenant_id,
            Vec::new(),
            PermissionSet::new(),
        ),
        request_id: RequestId::now_v7(),
        delegation_chain: Vec::new(),
    }
}

async fn complete_card_inner(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    _idempotency_key: &str,
    lease_owner: Option<Uuid>,
    schedule_failures: bool,
) -> Result<CardRegistrationOutcome, WyrdError> {
    let completion_state = load_card_completion_state(state, caller, card_uid).await?;
    validate_card_completion_status(&completion_state.card, card_uid)?;
    if completion_state.card.status == CardStatus::Active {
        if schedule_failures || lease_owner.is_some() {
            let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
            mark_card_reconciliation_succeeded(&mut conn, card_uid, lease_owner).await?;
            conn.commit().await.map_err(registry_db_error)?;
        }
        return load_card_registration_outcome(
            state,
            caller,
            card_uid,
            RegistrationOutcomeKind::IdempotentNoop,
        )
        .await;
    }
    if let Err(error) =
        verify_card_manifests(state, caller, card_uid, &completion_state.manifests).await
    {
        if schedule_failures {
            schedule_client_reconciliation(
                state,
                caller,
                card_uid,
                RECONCILE_KIND_FINALIZATION,
                &error,
            )
            .await?;
        }
        return Err(error);
    }
    let blob_uri = match ensure_card_blob(state, caller, &completion_state.card).await {
        Ok(uri) => uri,
        Err(error) => {
            if schedule_failures {
                schedule_client_reconciliation(
                    state,
                    caller,
                    card_uid,
                    RECONCILE_KIND_BLOB,
                    &error,
                )
                .await?;
            }
            return Err(error);
        }
    };
    let activated = commit_card_activation(
        state,
        caller,
        card_uid,
        &completion_state.manifests,
        &blob_uri,
        completion_state.card.card_blob_uri.is_none(),
        lease_owner,
    )
    .await?;
    load_card_registration_outcome(
        state,
        caller,
        card_uid,
        if activated {
            RegistrationOutcomeKind::Registered
        } else {
            RegistrationOutcomeKind::IdempotentNoop
        },
    )
    .await
}

async fn initialize_reconciled_uploads(
    state: &AppState,
    caller: &Caller,
    claim: &CardReconcileClaim,
    card_uid: &CardUid,
) -> Result<(), WyrdError> {
    let operation_id = claim
        .registration_operation_id
        .map(RegistrationOperationId::new)
        .ok_or_else(|| WyrdError::internal("pending Card has no registration operation"))?;
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let operation = lookup_operation_by_id(&mut conn, operation_id)
        .await?
        .ok_or_else(|| WyrdError::internal("Card registration operation was not found"))?;
    conn.commit().await.map_err(registry_db_error)?;
    let stored = operation
        .stored_response
        .ok_or_else(|| WyrdError::internal("Card registration operation has no replay seed"))?;
    let seed: RegistrationReplaySeed = serde_json::from_value(stored).map_err(|error| {
        WyrdError::internal(format!("invalid Card registration replay seed: {error}"))
    })?;
    let card_ref = seed
        .outcomes
        .iter()
        .find(|outcome| outcome.card_ref.uid.as_ref() == Some(card_uid))
        .map(|outcome| &outcome.card_ref)
        .ok_or_else(|| WyrdError::internal("Card is missing from its registration replay seed"))?;
    let paths = seed
        .artifact_manifest_paths
        .iter()
        .find(|(reference, _)| reference.same_identity(card_ref))
        .map(|(_, paths)| {
            paths
                .iter()
                .map(|path| path.as_str().to_owned())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    initialize_card_uploads(state, caller, operation_id, card_uid, &paths)
        .await
        .map(|_| ())
}

async fn schedule_client_reconciliation(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    kind: &str,
    error: &WyrdError,
) -> Result<(), WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    schedule_card_reconciliation(
        &mut conn,
        card_uid,
        kind,
        Utc::now() + ChronoDuration::seconds(1),
        error.code(),
        reconciliation_error_message(kind),
    )
    .await?;
    conn.commit().await.map_err(registry_db_error)
}

fn reconciliation_error_message(kind: &str) -> &'static str {
    match kind {
        RECONCILE_KIND_BLOB => "Card blob persistence failed; retry the storage transition",
        RECONCILE_KIND_FINALIZATION => {
            "Card artifact verification or activation failed; retry the lifecycle transition"
        }
        RECONCILE_KIND_CLEANUP => "Card artifact cleanup failed; retry the cleanup transition",
        _ => "Card registration side effect failed; retry the lifecycle transition",
    }
}

/// Soft-delete one exact Card UID while asserting its kind namespace.
///
/// The route's `allowed` verdict is appended before the SQL transition, and
/// both commit before any backend delete. A not-found, conflict, or failed
/// transaction commits neither, so the verdict is recorded standalone once. If
/// a backend cleanup fails, the deleted card and its cleanup rows remain
/// intact so a later retry can repeat the idempotent operation.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the verdict cannot be recorded,
/// the not-found, conflict, or registry failure of the transition, and
/// [`WyrdError::RegistryArtifactVerifyFailed`] when backend cleanup is incomplete.
pub async fn delete_card_with_kind(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    kind: CardKind,
    allowed: &AuditEvent,
) -> Result<DeleteCardResponse, WyrdError> {
    let deleted = async {
        let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
        audit::append_on(&mut conn, allowed).await?;
        let delete_state = soft_delete_card_with_kind(
            &mut conn,
            card_uid,
            kind,
            &caller.principal,
            Some(&caller.request_id),
        )
        .await?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok(delete_state)
    }
    .await;
    let delete_state = record_unless_committed(state, caller, allowed, deleted).await?;
    finish_card_delete(state, caller, delete_state).await
}

/// Soft-delete one Card selected by its exact public CardRef.
///
/// Audits the route's `allowed` verdict exactly as [`delete_card_with_kind`].
///
/// # Errors
/// Returns the same failures as [`delete_card_with_kind`].
#[tracing::instrument(
    skip(state, caller, allowed),
    fields(operation = "card.registration.delete")
)]
pub async fn delete_card_by_ref(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
    allowed: &AuditEvent,
) -> Result<DeleteCardResponse, WyrdError> {
    let deleted = async {
        let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
        audit::append_on(&mut conn, allowed).await?;
        let delete_state = soft_delete_card_by_ref(
            &mut conn,
            card_ref,
            &caller.principal,
            Some(&caller.request_id),
        )
        .await?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok(delete_state)
    }
    .await;
    let delete_state = record_unless_committed(state, caller, allowed, deleted).await?;
    finish_card_delete(state, caller, delete_state).await
}

/// Records the verdict standalone when its operation transaction did not commit.
///
/// Every error from a delete transaction means nothing committed, including the
/// in-transaction append, so the verdict is recorded once here.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the standalone record fails,
/// otherwise the transaction's own error.
async fn record_unless_committed<T>(
    state: &AppState,
    caller: &Caller,
    allowed: &AuditEvent,
    result: Result<T, WyrdError>,
) -> Result<T, WyrdError> {
    if result.is_err() {
        audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, allowed).await?;
    }
    result
}

async fn finish_card_delete(
    state: &AppState,
    caller: &Caller,
    delete_state: CardDeleteState,
) -> Result<DeleteCardResponse, WyrdError> {
    if delete_state.deleted || delete_state.card.status == CardStatus::Deleted {
        let cleanup_failures = cleanup_card_artifacts(
            state,
            caller,
            &delete_state.card.card_uid,
            &delete_state.card,
            &delete_state.manifests,
        )
        .await;
        if !cleanup_failures.is_empty() {
            tracing::error!(
                card_uid = %delete_state.card.card_uid,
                failure_count = cleanup_failures.len(),
                "card deletion cleanup was incomplete"
            );
            return Err(WyrdError::RegistryArtifactVerifyFailed {
                message: "card deletion cleanup was incomplete".to_owned(),
                details: serde_json::json!({
                    "card_uid": delete_state.card.card_uid,
                    "failure_count": cleanup_failures.len(),
                }),
            });
        }
        let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
        artifact_metadata::delete_for_card(&mut conn, delete_state.card.card_uid.as_str())
            .await
            .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
        mark_card_reconciliation_succeeded(&mut conn, &delete_state.card.card_uid, None).await?;
        conn.commit().await.map_err(registry_db_error)?;
    }
    Ok(DeleteCardResponse {
        card_uid: delete_state.card.card_uid,
        deleted: delete_state.deleted,
    })
}

/// Abort one incomplete Card and clean its upload/object state.
///
/// This is an internal lifecycle cleanup seam. It is intentionally not exposed
/// as a method on the public `Cards` handle. Cleanup is best-effort, while the
/// Pending→Failed transition commits atomically as registry lineage; it
/// evaluates no permission and appends no canonical audit event.
#[tracing::instrument(
    skip(state, caller, idempotency_key),
    fields(
        operation = "card.registration.cleanup",
        idempotency_key_present = !idempotency_key.is_empty()
    )
)]
async fn abort_card(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    idempotency_key: &str,
) -> Result<CardRegistrationOutcome, WyrdError> {
    let completion_state = load_card_completion_state(state, caller, card_uid).await?;
    validate_card_abort_status(&completion_state.card, card_uid)?;
    let cleanup_failures = cleanup_card_artifacts(
        state,
        caller,
        card_uid,
        &completion_state.card,
        &completion_state.manifests,
    )
    .await;
    commit_card_failure(state, caller, card_uid, cleanup_failures.is_empty()).await?;
    if !cleanup_failures.is_empty() {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "card registration cleanup was incomplete".to_owned(),
            details: serde_json::json!({
                "card_uid": card_uid,
                "failure_count": cleanup_failures.len(),
            }),
        });
    }
    load_card_registration_outcome(
        state,
        caller,
        card_uid,
        RegistrationOutcomeKind::IdempotentNoop,
    )
    .await
}

/// Load the parsed Card and tenant-bound manifest/upload rows.
async fn load_card_completion_state(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<CardCompletionState, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let card = sql_get_card_by_uid(&mut conn, card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(CardCompletionState { card, manifests })
}

/// Load Card lifecycle state without hiding a deleted tombstone.
async fn load_card_reconciliation_state(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<CardCompletionState, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let card = sql_get_card_for_reconciliation(&mut conn, card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(CardCompletionState { card, manifests })
}

/// Return whether a manifest has a live, resumable storage session.
fn manifest_has_live_upload(manifest: &CardManifestCompletionRow) -> bool {
    matches!(
        manifest.storage_status.as_deref(),
        Some("initiating" | "pending")
    ) && manifest
        .storage_expires_at
        .is_some_and(|expires_at| expires_at > Utc::now())
}

/// Return whether the manifest has reached a storage state completion can use.
fn manifest_ready_for_completion(manifest: &CardManifestCompletionRow) -> bool {
    manifest.manifest_status == "verified"
        || manifest.storage_status.as_deref() == Some("completed")
}

/// Return whether a manifest must be abandoned and cleaned up.
fn manifest_needs_cleanup(manifest: &CardManifestCompletionRow) -> bool {
    if manifest_ready_for_completion(manifest) || manifest_has_live_upload(manifest) {
        return false;
    }
    manifest.upload_id.is_some()
        && (manifest.storage_status.is_none()
            || matches!(
                manifest.storage_status.as_deref(),
                Some("initiating" | "pending" | "failed" | "aborted")
            ))
}

/// Validate the completion lifecycle state.
fn validate_card_completion_status(
    card: &ParsedCardRow,
    card_uid: &CardUid,
) -> Result<(), WyrdError> {
    if card.status == CardStatus::Active {
        return Ok(());
    }
    if card.status != CardStatus::Pending {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "card registration is no longer pending".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid, "status": card.status }),
        });
    }
    Ok(())
}

/// Verify every manifest against the server-owned storage state.
async fn verify_card_manifests(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    manifests: &[CardManifestCompletionRow],
) -> Result<(), WyrdError> {
    for manifest in manifests {
        verify_manifest_storage(state, caller, card_uid, manifest).await?;
    }
    Ok(())
}

/// Ensure the immutable Card blob exists before activation.
async fn ensure_card_blob(
    state: &AppState,
    caller: &Caller,
    card: &ParsedCardRow,
) -> Result<String, WyrdError> {
    match &card.card_blob_uri {
        Some(uri) => Ok(uri.clone()),
        None => write_card_blob(state, caller, card).await,
    }
}

/// Persist verified manifests, the blob URI, activation, and audit atomically.
async fn commit_card_activation(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    manifests: &[CardManifestCompletionRow],
    blob_uri: &str,
    record_blob: bool,
    lease_owner: Option<Uuid>,
) -> Result<bool, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    if let Some(lease_owner) = lease_owner
        && !lock_card_reconciliation_lease(&mut conn, card_uid, lease_owner).await?
    {
        conn.commit().await.map_err(registry_db_error)?;
        return Ok(false);
    }
    if !lock_pending_card_for_activation(&mut conn, card_uid).await? {
        if lease_owner.is_some() {
            let _ = mark_card_reconciliation_succeeded(&mut conn, card_uid, lease_owner).await?;
        }
        conn.commit().await.map_err(registry_db_error)?;
        return Ok(false);
    }
    for manifest in manifests {
        if let Some(upload_id) = manifest.upload_id {
            multipart_uploads::mark_completed_if_pending(&mut conn, upload_id)
                .await
                .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
            let _ = mark_manifest_verified(&mut conn, card_uid, upload_id).await?;
        }
    }
    if record_blob {
        record_card_blob(&mut conn, card_uid, blob_uri).await?;
    }
    let activated = activate_card(&mut conn, card_uid).await?;
    if activated || lease_owner.is_some() {
        mark_card_reconciliation_succeeded(&mut conn, card_uid, lease_owner).await?;
    }
    if activated {
        tracing::info!(%card_uid, "card activated");
    }
    conn.commit().await.map_err(registry_db_error)?;
    Ok(activated)
}

/// Load the server-derived outcome after a lifecycle transition.
async fn load_card_registration_outcome(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    outcome: RegistrationOutcomeKind,
) -> Result<CardRegistrationOutcome, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let card = sql_get_card_by_uid(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(existing_row_to_response(&card, outcome))
}

/// Validate that abort is legal and allow Failed cleanup retries.
fn validate_card_abort_status(card: &ParsedCardRow, card_uid: &CardUid) -> Result<(), WyrdError> {
    match card.status {
        CardStatus::Pending | CardStatus::Failed => Ok(()),
        CardStatus::Active => Err(WyrdError::Conflict {
            message: "active cards cannot be aborted".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid }),
        }),
        status => Err(WyrdError::Conflict {
            message: "card registration is not abortable in its current state".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid, "status": status }),
        }),
    }
}

/// Attempt cleanup for every manifest and return only failure details.
async fn cleanup_card_artifacts(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    card: &ParsedCardRow,
    manifests: &[CardManifestCompletionRow],
) -> Vec<String> {
    let mut failures = Vec::new();
    for manifest in manifests {
        if let Err(error) = cleanup_manifest_artifact(state, caller, manifest).await {
            failures.push(error.to_string());
        }
    }
    if let Err(error) = cleanup_card_blob(state, caller, card).await {
        failures.push(error);
    }
    let open_uploads = match load_open_card_uploads(state, caller, card_uid).await {
        Ok(rows) => rows,
        Err(error) => {
            failures.push(error.to_string());
            return failures;
        }
    };
    for upload in open_uploads {
        let upload_id = UploadId::from_uuid(upload.id);
        if let Err(error) = upload_abort(
            &state.storage,
            state.postgres.wyrd(),
            &storage_caller(caller),
            upload_id,
            None,
        )
        .await
        {
            failures.push(error.to_string());
        }
    }
    failures
}

/// Remove the deterministic blob written before a registration failure.
async fn cleanup_card_blob(
    state: &AppState,
    caller: &Caller,
    card: &ParsedCardRow,
) -> Result<(), String> {
    let path = card
        .card_blob_uri
        .as_deref()
        .and_then(|uri| uri.strip_prefix("wyrd://"))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            tenant_path::build(
                caller.data_tenant_id,
                &card.card_uid.to_string(),
                &format!("blob/{}.json", card.spec_hash),
            )
        });
    let validated =
        tenant_path::validate(&path, caller.data_tenant_id).map_err(|error| error.to_string())?;
    match state.storage.delete_object(&validated).await {
        Ok(()) | Err(StorageError::ObjectNotFound { .. }) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Load upload rows that were not linked to a manifest before a failure.
async fn load_open_card_uploads(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<Vec<multipart_uploads::MultipartUploadRow>, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let rows = multipart_uploads::find_open_for_card(&mut conn, card_uid.as_str())
        .await
        .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(rows)
}

/// Clean one completed object or pending backend upload.
async fn cleanup_manifest_artifact(
    state: &AppState,
    caller: &Caller,
    manifest: &CardManifestCompletionRow,
) -> Result<(), String> {
    let Some(upload_id) = manifest.upload_id else {
        return Ok(());
    };
    let upload_id = UploadId::from_uuid(upload_id);
    match manifest.storage_status.as_deref() {
        Some("completed") => {
            if let Some(path) = manifest.storage_path.as_deref() {
                let validated = tenant_path::validate(path, caller.data_tenant_id)
                    .map_err(|error| error.to_string())?;
                match state.storage.delete_object(&validated).await {
                    Ok(()) | Err(StorageError::ObjectNotFound { .. }) => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
        }
        Some("pending") | Some("initiating") => {
            upload_abort(
                &state.storage,
                state.postgres.wyrd(),
                &storage_caller(caller),
                upload_id,
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        _ => {}
    }
    Ok(())
}

/// Persist the Pending→Failed lifecycle transition in one tenant transaction.
///
/// Cleanup evaluates no principal permission — the caller's `card:write`
/// decision was already audited when registration was received — so the durable
/// record it owes is the failed card row plus structured diagnostics.
async fn commit_card_failure(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    cleanup_succeeded: bool,
) -> Result<bool, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let failed = fail_card(&mut conn, card_uid).await?;
    if failed {
        if cleanup_succeeded {
            mark_card_reconciliation_succeeded(&mut conn, card_uid, None).await?;
        }
        tracing::warn!(%card_uid, cleanup_succeeded, "card registration failed and was cleaned up");
    }
    conn.commit().await.map_err(registry_db_error)?;
    Ok(failed)
}

/// Verify one manifest/upload binding against the configured storage backend.
async fn verify_manifest_storage(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    manifest: &CardManifestCompletionRow,
) -> Result<(), WyrdError> {
    let valid = matches!(
        manifest.manifest_status.as_str(),
        "pending" | "uploaded" | "verified"
    ) && matches!(
        manifest.storage_status.as_deref(),
        Some("pending" | "completed")
    ) && manifest
        .storage_card_uid
        .as_deref()
        .is_some_and(|value| value == card_uid.to_string())
        && manifest.storage_relative_path.as_deref() == Some(manifest.relative_path.as_str())
        && manifest.storage_expected_sha256.as_deref() == Some(manifest.expected_sha256.as_str())
        && manifest.storage_expected_size_bytes == Some(manifest.expected_size_bytes)
        && manifest.storage_backend.as_deref()
            == Some(state.storage.backend().to_string().as_str());
    if !valid {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "artifact upload does not match its registration manifest".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid, "relative_path": manifest.relative_path }),
        });
    }
    let path = manifest.storage_path.as_deref().ok_or_else(|| {
        WyrdError::RegistryArtifactVerifyFailed {
            message: "completed artifact upload has no storage path".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid, "relative_path": manifest.relative_path }),
        }
    })?;
    let validated = tenant_path::validate(path, caller.data_tenant_id).map_err(|error| {
        WyrdError::RegistryArtifactVerifyFailed {
            message: "artifact storage path failed tenant validation".to_owned(),
            details: serde_json::json!({ "reason": error.to_string() }),
        }
    })?;
    if validated.card_uid != card_uid.to_string()
        || validated.relative_path != manifest.relative_path
    {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "artifact storage path is not bound to the Card manifest".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid, "relative_path": manifest.relative_path }),
        });
    }
    let head = state
        .storage
        .signer()
        .head_for_verification(&validated)
        .await
        .map_err(map_storage_error)?;
    let expected_size = u64::try_from(manifest.expected_size_bytes).map_err(|_| {
        WyrdError::RegistryArtifactVerifyFailed {
            message: "artifact manifest size is invalid".to_owned(),
            details: serde_json::json!({ "relative_path": manifest.relative_path }),
        }
    })?;
    if head.size_bytes != expected_size {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "stored artifact size does not match its manifest".to_owned(),
            details: serde_json::json!({
                "relative_path": manifest.relative_path,
                "expected_size_bytes": expected_size,
                "actual_size_bytes": head.size_bytes,
            }),
        });
    }
    if state.storage.require_encryption() && head.sse_marker.is_none() {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "stored artifact is missing required encryption metadata".to_owned(),
            details: serde_json::json!({ "relative_path": manifest.relative_path }),
        });
    }
    Ok(())
}

/// Normalize a Card into the immutable definition stored in its blob.
///
/// Relationships are derived from the already-bound spec, and live status plus
/// inbound edges are deliberately excluded from this content-addressed value.
fn immutable_card_projection(mut card: Card) -> Card {
    card.relationships = relationships_from_spec(&card.spec);
    card.status = None;
    card
}

/// Serialize one immutable Card definition with deterministic JCS bytes.
fn immutable_card_blob_bytes(card: Card) -> Result<Vec<u8>, WyrdError> {
    serde_jcs::to_vec(&immutable_card_projection(card)).map_err(WyrdError::from_spec_serialization)
}

/// Persist the immutable resolved Card envelope and return its stable URI.
async fn write_card_blob(
    state: &AppState,
    caller: &Caller,
    card: &ParsedCardRow,
) -> Result<String, WyrdError> {
    let blob_path = tenant_path::build(
        caller.data_tenant_id,
        &card.card_uid.to_string(),
        &format!("blob/{}.json", card.spec_hash),
    );
    let validated = tenant_path::validate(&blob_path, caller.data_tenant_id).map_err(|error| {
        WyrdError::internal(format!("card blob path failed validation: {error}"))
    })?;
    let envelope = Card {
        api_version: ApiVersion::v1(),
        kind: card.kind.clone(),
        metadata: Metadata {
            name: card.name.clone(),
            version: Some(VersionSpec::Pin(card.version.clone())),
            bump: None,
            space: Some(card.space.clone()),
            uid: Some(card.card_uid.clone()),
            labels: card.labels.clone(),
            annotations: card.annotations.clone(),
            spec_hash: Some(card.spec_hash.parse().map_err(|error| {
                WyrdError::internal(format!("stored spec hash invalid: {error}"))
            })?),
            artifact_hash: card.artifact_hash.clone(),
            origin: None,
        },
        spec: card.spec.clone(),
        relationships: Default::default(),
        status: None,
    };
    let bytes = immutable_card_blob_bytes(envelope)?;
    if let Err(error) = state.storage.put_object(&validated, bytes).await {
        let mut conn = state
            .postgres
            .tenant_conn(caller.data_tenant_id)
            .await
            .map_err(registry_db_error)?;
        tracing::warn!(
            card_uid = %card.card_uid,
            blob_path = %blob_path,
            %error,
            "immutable card blob write failed"
        );
        record_blob_failure(&mut conn, &card.card_uid).await?;
        conn.commit().await.map_err(registry_db_error)?;
        return Err(map_storage_error(error));
    }
    Ok(format!("wyrd://{blob_path}"))
}

/// Convert a server-tier storage error to the shared Wyrd error catalog.
fn map_storage_error(error: StorageError) -> WyrdError {
    WyrdStorageError::from(error).into()
}

#[cfg(test)]
mod tests {
    use super::{
        card_upload_entry, hash_request, immutable_card_blob_bytes, plan_registration_graph,
        validate_request, verify_card_hashes,
    };
    use uuid::Uuid;
    use wyrd_spec::registry::{
        ArtifactManifestEntry, CreateCardRequest, RelativeArtifactPath,
        canonical_artifact_manifest_hash,
    };
    use wyrd_spec::storage::{UploadId, UploadPlan};

    /// Decode a compact registration request used by pure composition tests.
    fn request(value: serde_json::Value) -> CreateCardRequest {
        serde_json::from_value(value).expect("test_setup: registration fixture must deserialize")
    }

    /// Build one metadata-only Prompt submission fixture.
    fn prompt(name: &str) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": name, "version": "1.0.0", "space": "default" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
            "artifacts": []
        })
    }

    /// Build one Service submission fixture with the provided spec.
    fn service(name: &str, spec: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Service",
            "metadata": { "name": name, "version": "1.0.0", "space": "default" },
            "spec": spec,
            "artifacts": []
        })
    }

    /// Build one empty Eval submission fixture.
    fn eval(name: &str) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Eval",
            "metadata": { "name": name, "version": "1.0.0", "space": "default" },
            "spec": { "tasks": {} },
            "artifacts": []
        })
    }

    /// Build matching row/blob/inventory values for read-integrity regression tests.
    fn matching_read_integrity_values() -> (
        wyrd_spec::envelope::Card,
        Vec<ArtifactManifestEntry>,
        String,
        String,
    ) {
        let manifest = vec![ArtifactManifestEntry {
            relative_path: RelativeArtifactPath::new("weights.bin")
                .expect("test_setup: artifact path is valid"),
            sha256: "YQ==".to_owned(),
            size_bytes: 1,
            content_type: Some("application/octet-stream".to_owned()),
        }];
        let mut card: wyrd_spec::envelope::Card = serde_json::from_value(serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": "integrity", "version": "1.0.0", "space": "default" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] }
        }))
        .expect("test_setup: blob card fixture is valid");
        let spec_hash = card
            .spec
            .canonical_hash()
            .expect("test_setup: prompt spec canonicalizes");
        let artifact_hash = canonical_artifact_manifest_hash(&manifest)
            .expect("test_setup: manifest canonicalizes")
            .expect("test_setup: nonempty manifest has a hash");
        card.metadata.spec_hash = Some(spec_hash.clone());
        card.metadata.artifact_hash = Some(artifact_hash.clone());
        (card, manifest, spec_hash.to_string(), artifact_hash)
    }

    /// Reject peer-only Service components before registry access.
    #[test]
    fn registration_graph_rejects_eval_service_component() {
        let request = request(serde_json::json!({
            "submissions": [service("app", serde_json::json!({
                "components": [{
                    "alias": "quality",
                    "ref": {
                        "kind": "Eval",
                        "name": "quality",
                        "version": "1.0.0",
                        "space": "default"
                    }
                }]
            }))]
        }));

        let error = plan_registration_graph(&request.submissions)
            .expect_err("Eval Service component must fail before registry I/O");

        assert_eq!(error.code(), "WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND");
    }

    /// Reject an orphan Eval submitted beside a Service root before registry access.
    #[test]
    fn registration_graph_rejects_unpublished_eval_peer() {
        let request = request(serde_json::json!({
            "submissions": [service("app", serde_json::json!({})), eval("quality")]
        }));

        let error = plan_registration_graph(&request.submissions)
            .expect_err("Service-root Eval requires a submitted publisher");

        assert_eq!(error.code(), "WYRD_SPEC_400_UNPUBLISHED_OBSERVABILITY_PEER");
    }

    /// Preserve standalone Eval registration at the server boundary.
    #[test]
    fn registration_graph_accepts_standalone_eval() {
        let request = request(serde_json::json!({
            "submissions": [eval("quality")]
        }));

        plan_registration_graph(&request.submissions)
            .expect("standalone Eval registration remains valid");
    }

    /// Prove canonical request hashing is independent of submission wire order.
    #[test]
    fn server_request_hash_matches_canonical_order_recompute() {
        let first = request(serde_json::json!({
            "submissions": [prompt("charlie"), prompt("alpha"), prompt("bravo")]
        }));
        let second = request(serde_json::json!({
            "submissions": [prompt("bravo"), prompt("charlie"), prompt("alpha")]
        }));

        assert_eq!(
            hash_request(&first).expect("test_setup: first request hash is valid"),
            hash_request(&second).expect("test_setup: second request hash is valid")
        );
    }

    /// Reject a caller-supplied artifact hash that differs from the manifest.
    #[test]
    fn manifest_hash_mismatch_returns_400() {
        let mut submission = prompt("manifest-mismatch");
        submission["metadata"]["artifact_hash"] = serde_json::json!("not-the-server-hash");
        submission["artifacts"] = serde_json::json!([{
            "relative_path": "weights.bin",
            "sha256": "YQ==",
            "size_bytes": 1
        }]);
        let error = validate_request(&request(serde_json::json!({
            "submissions": [submission]
        })))
        .expect_err("test_setup: mismatched manifest hash must be rejected");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH");
        assert_eq!(error.status(), 400);
    }

    /// Reject artifact-bearing cards when the request contains another submission.
    #[test]
    fn compose_rejects_heavy_manifest_when_not_sole_submission() {
        let mut heavy = prompt("heavy");
        heavy["artifacts"] = serde_json::json!([{
            "relative_path": "weights.bin",
            "sha256": "YQ==",
            "size_bytes": 1
        }]);
        let error = validate_request(&request(serde_json::json!({
            "submissions": [heavy, prompt("other")]
        })))
        .expect_err("test_setup: non-sole artifact manifest must be rejected");

        assert_eq!(
            error.code(),
            "WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION"
        );
        assert_eq!(error.status(), 400);
    }

    /// Keep every storage protocol and its durable upload ID in the card wire
    /// response; cloud plans must not be collapsed into a single PUT shape.
    #[test]
    fn card_upload_entry_preserves_cloud_storage_plans() {
        let upload_id = UploadId::from_uuid(Uuid::now_v7());
        let plans = [
            UploadPlan::S3Multipart {
                part_count: 2,
                part_size_bytes: 8,
                part_url_ttl_secs: 60,
                required_headers: Vec::new(),
            },
            UploadPlan::GcsResumable {
                session_uri: "https://storage.googleapis.com/session".to_owned(),
                chunk_size_bytes: 8,
            },
            UploadPlan::AzureBlockBlob {
                sas_url: "https://blob.core.windows.net/object?sas".to_owned(),
                block_size_bytes: 8,
                block_count_planned: 2,
            },
        ];

        for plan in plans {
            let entry = card_upload_entry("weights.bin", upload_id.clone(), plan.clone())
                .expect("test_setup: valid card upload entry");
            assert_eq!(entry.upload_id, upload_id);
            assert_eq!(entry.plan, plan);
        }
    }

    /// Keep blob bytes stable while mutable status and inbound edges change.
    #[test]
    fn immutable_blob_projection_is_deterministic_and_excludes_live_edges() {
        let mut card: wyrd_spec::envelope::Card = serde_json::from_value(serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": "blob", "version": "1.0.0", "space": "default" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
            "relationships": { "outbound": [], "inbound": ["live-edge"] },
            "status": { "phase": "pending" }
        }))
        .expect("test_setup: blob card fixture is valid");

        let first = immutable_card_blob_bytes(card.clone()).expect("blob serializes");
        card.status = Some(wyrd_spec::envelope::Status {
            phase: "active".to_owned(),
            message: None,
            updated_at: None,
        });
        card.relationships.inbound = vec!["different-live-edge".to_owned()];
        let second = immutable_card_blob_bytes(card).expect("blob serializes");

        assert_eq!(first, second);
        assert!(
            !String::from_utf8(first)
                .expect("JCS bytes are UTF-8")
                .contains("status")
        );
    }

    /// Rejects a registry row whose spec hash diverges from immutable blob bytes.
    #[test]
    fn read_integrity_rejects_row_blob_spec_hash_divergence() {
        let (card, manifest, _spec_hash, artifact_hash) = matching_read_integrity_values();

        let error =
            verify_card_hashes("different-row-hash", Some(&artifact_hash), &card, &manifest)
                .expect_err("row/blob spec divergence must reject the read");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
        assert!(error.to_string().contains("spec hash"));
    }

    /// Rejects an immutable blob whose artifact metadata diverges from the row.
    #[test]
    fn read_integrity_rejects_row_blob_artifact_hash_divergence() {
        let (mut card, manifest, spec_hash, artifact_hash) = matching_read_integrity_values();
        card.metadata.artifact_hash = Some("different-blob-hash".to_owned());

        let error = verify_card_hashes(&spec_hash, Some(&artifact_hash), &card, &manifest)
            .expect_err("row/blob artifact divergence must reject the read");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
        assert!(error.to_string().contains("artifact hash"));
    }

    /// Rejects storage inventory whose sorted manifest hash diverges from row and blob.
    #[test]
    fn read_integrity_rejects_inventory_hash_divergence() {
        let (card, mut manifest, spec_hash, artifact_hash) = matching_read_integrity_values();
        manifest[0].sha256 = "Yg==".to_owned();

        let error = verify_card_hashes(&spec_hash, Some(&artifact_hash), &card, &manifest)
            .expect_err("inventory divergence must reject the read");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
        assert!(error.to_string().contains("artifact hash"));
    }
}

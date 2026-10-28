//! Card declaration service orchestration.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use uuid::Uuid;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_semver::{VersionBlock, VersionRange, VersionSpec};
use wyrd_spec::DataTenantId;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
use wyrd_spec::card::drift::DriftSignal;
use wyrd_spec::card::trigger::TriggerActivation;
use wyrd_spec::card::verifier::{
    InputSlot, OWNER_OCCURRENCE_KEY, OrderBy, VerificationBinding, VerificationStatus,
};
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec, Status};
use wyrd_spec::error::{WyrdError, storage::WyrdStorageError};
use wyrd_spec::graph::{
    GraphError, RootPick, TopoOrder, build, canonical_order, graph_ready_submissions, pick_root,
    relationships_from_spec, spec_binding_errors, topo_sort, validate_composition,
};
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::ids::{CardName, CardUid, SlotName};
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, CardRefIdentity, InlineableRef, scope_child_card_refs};
use wyrd_spec::registry::{
    ArtifactInventoryResponse, ArtifactManifestEntry, CardDeclarationOutcome, CardLifecycleStatus,
    CardSubmission, CardSummary, CardUploadEntry, CardUploadPlan, DeclarationOperationId,
    DeclarationOutcomeKind, DeclarationReplaySeed, DeclareCardRequest, DeclareCardResponse,
    DeleteCardResponse, GetCardResponse, ListCardsRequest, ListCardsResponse, ListVersionsResponse,
    RelativeArtifactPath, canonical_artifact_manifest_hash, validate_artifact_manifest,
};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::storage::{UploadId, UploadInitRequest, UploadPlan};
use wyrd_spec::vala::api::RegisterTableRequest;
use wyrd_spec::verification::{GroupEndSettings, GroupSource};
use wyrd_sql::CardStatus;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::{
    CardArtifactManifestRow, CardDeclarationOperationRow, CardDeleteState,
    CardManifestCompletionRow, CardQuery, CardReconcileClaim, ListCursor, NewCardRow,
    NewDeclarationOperation, RECONCILE_KIND_BLOB, RECONCILE_KIND_CLEANUP,
    RECONCILE_KIND_FINALIZATION, Resolution, SubmittedCardIdentity, activate_card,
    commit_declaration_operation, declaration_request_hash, fail_card, find_card_by_ref,
    get_card_by_uid as sql_get_card_by_uid,
    get_card_for_reconciliation as sql_get_card_for_reconciliation, get_latest_card_by_range,
    inbound_relationships, insert_artifact_manifest_rows, insert_card_row,
    insert_declaration_operation, list_versions, lock_card_reconciliation_lease,
    lock_pending_card_for_activation, lock_version_line, lookup_existing_operation,
    lookup_expired_operation, manifest_completion_rows, manifest_rows_for_init,
    mark_card_reconciliation_succeeded, mark_manifest_upload_initialized, mark_manifest_verified,
    persist_outbound_relationships, query_cards, recheck_active_card_refs, record_blob_failure,
    record_card_blob, record_card_reconciliation_failure, reschedule_card_reconciliation,
    resolve_version, schedule_card_reconciliation, soft_delete_card_by_ref,
    soft_delete_card_with_kind,
};
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::storage::multipart_uploads;
use wyrd_sql::queries::verification::{
    BindingActivation, FrozenTarget, NewBinding, owner_binding_ids, project_bindings,
};
use wyrd_sql::row_types::cards::{CardRow, ParsedCardRow};
use wyrd_storage::StorageError;
use wyrd_storage::service::{reconcile_upload, upload_abort, upload_init};
use wyrd_storage::tenant_path;
use wyrd_types::json_schema_to_fieldspec;

use crate::bifrost::service::register_table;
use crate::components::auth::Caller;
use crate::components::cards::mapping::{
    existing_row_to_response, lifecycle_status, outcome_row_to_response,
};
use crate::components::cards::resolve::{
    EffectiveSpecs, ResolvedRefs, bind_card_references, source_binding,
};
use crate::components::storage::inventory::{
    CardObjects, ObjectFact, ObjectInventory, ObjectOrigin, ObjectState,
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
///
/// Reads the row, inbound edges, artifact manifest, and owned verification
/// binding IDs in one tenant transaction, then the object inventory, then
/// hydrates the immutable blob.
///
/// # Errors
/// Returns the registry not-found, unavailable, or integrity errors raised by
/// the tenant reads and [`hydrate_card`], and the retryable readiness errors
/// of [`card_files`].
pub async fn get_card_by_uid(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<GetCardResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let row = sql_get_card_by_uid(&mut conn, card_uid).await?;
    let inbound = inbound_relationships(&mut conn, card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, card_uid).await?;
    let verification = verification_status(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    let inventory = card_files(state, caller, card_uid, &manifests).await?;
    hydrate_card(state, caller, row, inbound, inventory, verification).await
}

/// Load a fully hydrated Card by its exact tenant-scoped reference.
///
/// Reads the row, inbound edges, artifact manifest, and owned verification
/// binding IDs in one tenant transaction, then the object inventory, then
/// hydrates the immutable blob.
///
/// # Errors
/// Returns the registry not-found, unavailable, or integrity errors raised by
/// the tenant reads and [`hydrate_card`], and the retryable readiness errors
/// of [`card_files`].
pub async fn get_card_by_ref(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
) -> Result<GetCardResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let row = wyrd_sql::queries::cards::get_card_by_ref(
        &mut conn,
        card_ref.kind.clone(),
        &card_ref.name,
        &card_ref.version,
    )
    .await?;
    let inbound = inbound_relationships(&mut conn, &row.card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, &row.card_uid).await?;
    let verification = verification_status(&mut conn, &row.card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    let inventory = card_files(state, caller, &row.card_uid, &manifests).await?;
    hydrate_card(state, caller, row, inbound, inventory, verification).await
}

/// Resolve and load the newest stable Active Card in one identity line.
///
/// Reads the row, inbound edges, artifact manifest, and owned verification
/// binding IDs in one tenant transaction, then the object inventory, then
/// hydrates the immutable blob.
///
/// # Errors
/// Returns the registry not-found, unavailable, or integrity errors raised by
/// the tenant reads and [`hydrate_card`], and the retryable readiness errors
/// of [`card_files`].
pub async fn get_latest_card(
    state: &AppState,
    caller: &Caller,
    kind: CardKind,
    name: CardName,
) -> Result<GetCardResponse, WyrdError> {
    let range = VersionRange::default();
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let row = get_latest_card_by_range(&mut conn, kind, &name, &range).await?;
    let inbound = inbound_relationships(&mut conn, &row.card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, &row.card_uid).await?;
    let verification = verification_status(&mut conn, &row.card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    let inventory = card_files(state, caller, &row.card_uid, &manifests).await?;
    hydrate_card(state, caller, row, inbound, inventory, verification).await
}

/// List versions in one card identity line, excluding deleted rows.
///
/// The line is the caller tenant's flat `kind`/`name`; the read runs in one
/// tenant transaction that commits before the response is built.
///
/// # Errors
/// Returns registry unavailability when the tenant connection, version read,
/// or commit fails, and `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` for a stored
/// version that no longer parses.
pub async fn list_card_versions(
    state: &AppState,
    caller: &Caller,
    kind: CardKind,
    name: CardName,
    include_prerelease: bool,
) -> Result<ListVersionsResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let versions = list_versions(&mut conn, kind, &name, include_prerelease).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(ListVersionsResponse { versions })
}

/// List metadata-only Cards with deterministic tenant-scoped keyset pagination.
///
/// A `Deleted` status filter answers empty without a read. Otherwise one
/// tenant transaction reads the page and commits before each row is projected
/// through [`summary_from_row`].
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_LIST_LIMIT_OUT_OF_RANGE` for a negative or
/// oversized limit, `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable
/// cursor, `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` for an invalid version
/// range, the query errors of an invalid metadata filter or cursor mismatch,
/// registry unavailability when the connection, read, or commit fails, and
/// the decode errors of [`summary_from_row`].
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
///
/// # Errors
/// Returns the registry not-found or unavailable errors of the Card read and
/// the retryable readiness errors of [`card_files`].
pub async fn list_card_artifacts(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<ArtifactInventoryResponse, WyrdError> {
    let manifests = card_manifests(state, caller, card_uid).await?;
    let artifacts = card_files(state, caller, card_uid, &manifests)
        .await?
        .files
        .into_values()
        .collect();
    Ok(ArtifactInventoryResponse { artifacts })
}

/// Resolve one completed file of a Card from the object inventory.
///
/// Card listing, downloads, verification evidence, and baseline fitting
/// share this one bounded lookup after their own Card authorization.
///
/// # Errors
/// Returns the registry not-found or unavailable errors of the Card read,
/// the retryable readiness errors of [`card_files`], and
/// `WYRD_STORAGE_404_OBJECT_NOT_FOUND` when the Card declares no completed
/// file at `relative_path`.
pub async fn card_file(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    relative_path: &str,
) -> Result<wyrd_spec::registry::StoredArtifactEntry, WyrdError> {
    let manifests = card_manifests(state, caller, card_uid).await?;
    card_files(state, caller, card_uid, &manifests)
        .await?
        .files
        .remove(relative_path)
        .ok_or_else(|| {
            WyrdStorageError::ObjectNotFound {
                storage_path: tenant_path::build(
                    caller.data_tenant_id,
                    card_uid.as_str(),
                    relative_path,
                ),
            }
            .into()
        })
}

/// Read a live Card's manifest in one tenant transaction.
///
/// # Errors
/// Returns the registry not-found or unavailable errors of the reads.
async fn card_manifests(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<Vec<CardManifestCompletionRow>, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let _ = sql_get_card_by_uid(&mut conn, card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(manifests)
}

/// Read a Card's completed files from the object inventory.
///
/// A manifest entry Postgres already completed must be visible: an absent
/// one is not-yet-visible inventory, never proof of absence.
///
/// # Errors
/// Returns `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` when a completed
/// manifest entry is not visible yet, and the inventory read's errors.
async fn card_files(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    manifests: &[CardManifestCompletionRow],
) -> Result<CardObjects, WyrdError> {
    let objects = ObjectInventory::new(state)
        .card_objects(caller.data_tenant_id, card_uid)
        .await?;
    if let Some(missing) = manifests
        .iter()
        .filter(|manifest| manifest_ready_for_completion(manifest))
        .find(|manifest| !objects.files.contains_key(&manifest.relative_path))
    {
        tracing::info!(%card_uid, relative_path = %missing.relative_path, "a completed Card file is not visible in the object inventory yet");
        return Err(wyrd_spec::vala::BifrostError::QueryVisibilityUnavailable.into());
    }
    Ok(objects)
}

/// Rebuild the served Card from its immutable blob and live registry state.
///
/// Reads the blob only once the object inventory records an envelope at the
/// registry's retained pointer, verifies it against the registry row and
/// artifact inventory, then overlays server-derived state: inbound
/// relationships and `status`, whose `verification.binding_ids` lists the
/// owner's projected bindings and is omitted when the Card owns none.
///
/// # Errors
/// Returns `WYRD_REGISTRY_404_CARD_NOT_FOUND` before the blob is active,
/// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the blob URI, identity, or
/// integrity check fails, `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` when
/// no envelope is recorded at the pointer yet, and the mapped storage error
/// when the read fails.
async fn hydrate_card(
    state: &AppState,
    caller: &Caller,
    row: wyrd_sql::row_types::cards::ParsedCardRow,
    inbound: Vec<CardRef>,
    inventory: CardObjects,
    verification: Option<VerificationStatus>,
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
    if inventory
        .envelope
        .as_ref()
        .is_none_or(|(recorded, _)| recorded != path)
    {
        tracing::info!(card_uid = %row.card_uid, "the Card envelope is not visible in the object inventory yet");
        return Err(wyrd_spec::vala::BifrostError::QueryVisibilityUnavailable.into());
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
    verify_card_read_integrity(&row, &card, &inventory)?;
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
        verification,
    });
    Ok(GetCardResponse {
        card,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Read the server-derived verification status a Card read serves.
///
/// Combines the Card's owned binding IDs with the fitted-baseline status of a
/// PSI or SPC Drift Verifier, inside the read's tenant transaction, and
/// returns `None` when the Card has neither.
///
/// # Errors
/// Returns the mapped registry database error when either read fails.
async fn verification_status(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Option<VerificationStatus>, WyrdError> {
    let binding_ids = owner_binding_ids(conn, card_uid)
        .await
        .map_err(registry_db_error)?;
    let baseline = DriftBaselineQueue::default()
        .status(conn, card_uid)
        .await
        .map_err(registry_db_error)?;
    Ok(VerificationStatus::derived(binding_ids, baseline))
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
/// artifact hash disagrees with the registry row, an inventory size is
/// invalid, or the canonical hash
/// reconstructed from inventory differs from the row and blob.
fn verify_card_read_integrity(
    row: &wyrd_sql::row_types::cards::ParsedCardRow,
    card: &Card,
    inventory: &CardObjects,
) -> Result<(), WyrdError> {
    let manifest = inventory
        .files
        .values()
        .map(|entry| {
            Ok(ArtifactManifestEntry {
                relative_path: entry.relative_path.clone(),
                sha256: Some(entry.sha256.clone()),
                size_bytes: Some(u64::try_from(entry.size_bytes).map_err(|_| {
                    WyrdError::registry_invalid_card_spec(
                        "stored artifact inventory has a negative byte size",
                    )
                })?),
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

/// Project one stored registry row onto its metadata-only list summary.
///
/// The summary carries the flat `kind`/`name`/`version` identity, its UID,
/// hashes, lifecycle, and timestamps; it never reads the Card blob.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the stored status, kind,
/// or name no longer decodes, `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` for a
/// stored version that no longer parses, and the Card UID error for an invalid
/// stored UID.
fn summary_from_row(row: &CardRow) -> Result<CardSummary, WyrdError> {
    let status = CardStatus::from_db_str(&row.status)
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(CardSummary {
        card_uid: CardUid::from_uuid(row.card_uid).map_err(WyrdError::from_card_uid_error)?,
        kind: CardKind::from_wire_name(&row.kind)
            .ok_or_else(|| WyrdError::registry_invalid_card_spec("stored Card kind is invalid"))?,
        name: CardName::new(row.name.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?,
        version: row
            .version
            .parse::<VersionBlock>()
            .map_err(|error| WyrdError::registry_invalid_version_block(error.to_string()))?,
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        status: lifecycle_status(status),
        created_at: row.created_at,
        updated_at: row.updated_at,
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
struct DeclarationPlan {
    submissions: Vec<CardSubmission>,
    request_hash: String,
    order: TopoOrder,
    root: RootPick,
    external_refs: ResolvedRefs,
}

/// Existing row plus the outcome implied by the authored version mode.
struct ExistingNode {
    row: ParsedCardRow,
    outcome: DeclarationOutcomeKind,
}

/// Declare a composite card request through the server-owned transaction.
///
/// This operation resolves references, applies idempotency replay, writes the
/// declaration transaction, and initializes any artifact uploads.
///
/// The route has already staged its allowed verdicts (`card:write`, plus
/// `operators:invoke` for an Operator-bearing request) on the process audit
/// outbox, so no outcome here records them again.
///
/// # Errors
/// Returns the validation, idempotency, dependency, registry, or upload
/// failure the declaration raised.
#[tracing::instrument(skip(state, caller), fields(operation = "card.declaration"))]
pub async fn declare_card(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: DeclareCardRequest,
) -> Result<DeclareCardResponse, WyrdError> {
    let request_hash = hash_request(&request)?;
    let (operation_id, seed) =
        if let Some(replayed) = replay(state, caller, idempotency_key, &request_hash).await? {
            replayed
        } else {
            validate_request(&request)?;
            let tables = declared_tables(&request.submissions)?;
            let (order, root) = plan_declaration_graph(&request.submissions)?;
            let external_refs = resolve_external(state, caller, &request.submissions).await?;
            for table in tables {
                register_table(state, caller.clone(), table).await?;
            }
            let plan = plan_declaration(request, request_hash, external_refs, order, root);
            DeclarationWriter { state, caller }
                .write(idempotency_key, plan)
                .await?
        };
    initialize_uploads(state, caller, operation_id, seed, idempotency_key).await
}

/// The Bifrost dataset registrations every submitted Service declares.
///
/// Each declared JSON Schema is parsed into Bifrost user fields here, before
/// any table or Card is written, so an unsupported schema refuses the whole
/// registration. [`declare_card`] then ensures each table through the
/// catalog owner, which refuses a conflicting existing schema before the
/// Card transaction opens; tables are never dropped afterwards.
///
/// # Errors
/// Returns `WYRD_VALA_400_SCHEMA_PARSE` for a schema outside the supported
/// subset and `WYRD_REGISTRY_422_INVALID_CARD_SPEC` for an undecodable spec.
fn declared_tables(submissions: &[CardSubmission]) -> Result<Vec<RegisterTableRequest>, WyrdError> {
    let mut tables = Vec::new();
    for submission in submissions {
        if submission.kind != CardKind::Service {
            continue;
        }
        let Spec::Service(spec) =
            Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?
        else {
            continue;
        };
        for table in spec.tables {
            tables.push(RegisterTableRequest {
                namespace: BifrostNamespace::Datasets.as_str().to_owned(),
                fields: json_schema_to_fieldspec(&table.schema)?,
                name: table.name,
                physical_layout: None,
                compaction_target_file_size_bytes: None,
                compaction_type: None,
            });
        }
    }
    Ok(tables)
}

/// Whether any submitted verification binding names an `on_failure` Operator.
///
/// Declaration needs `operators:invoke` in addition to `card:write` exactly
/// when this holds, inline or referenced alike, because the frozen Operator is
/// later dispatched without another end-user decision. Only top-level binding
/// sites count; nested sites and undecodable specs are refused by validation
/// before anything is persisted, so they can never freeze an Operator.
pub(crate) fn dispatches_operators(request: &DeclareCardRequest) -> bool {
    request.submissions.iter().any(|submission| {
        Spec::from_kind_and_value(&submission.kind, submission.spec.clone()).is_ok_and(|spec| {
            spec.binding_sites()
                .iter()
                .filter(|site| !site.nested)
                .flat_map(|site| site.bindings)
                .any(|binding| !binding.on_failure.is_empty())
        })
    })
}

/// Return a committed response for an identical idempotency key.
///
/// Reads, in its own tenant transaction, the live operation the caller's
/// exact subject/actor pair reserved under `idempotency_key`, falling back to
/// an expired one, then commits without writing. Another pair's operation
/// under the same key is never consulted, so replay cannot cross principals
/// or rewrite the stored attribution.
///
/// Returns `None` when the pair has no operation for the key, or when its
/// operation is still pending without a stored response; the caller then
/// declares or waits for the winner.
///
/// # Errors
/// Returns `WYRD_REGISTRY_410_OPERATION_EXPIRED` for an expired operation, the
/// `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT` when the stored request digest differs from
/// `request_hash`, and `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the
/// tenant connection, read, or commit fails or the stored response does not
/// decode.
async fn replay(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<(DeclarationOperationId, DeclarationReplaySeed)>, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let operation =
        match lookup_existing_operation(&mut conn, caller.attribution(), idempotency_key).await? {
            Some(operation) => Some(operation),
            None => {
                lookup_expired_operation(&mut conn, caller.attribution(), idempotency_key).await?
            }
        };
    conn.commit().await.map_err(registry_db_error)?;
    let Some(operation) = operation else {
        return Ok(None);
    };
    let operation_id = DeclarationOperationId::new(operation.operation_id);
    replay_operation(operation, request_hash, idempotency_key)
        .map(|seed| seed.map(|seed| (operation_id, seed)))
}

/// Validate and decode one persisted operation response.
fn replay_operation(
    operation: CardDeclarationOperationRow,
    request_hash: &str,
    idempotency_key: &str,
) -> Result<Option<DeclarationReplaySeed>, WyrdError> {
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
    let mut seed: DeclarationReplaySeed = serde_json::from_value(stored)
        .map_err(|error| registry_db_error(format!("invalid stored response: {error}")))?;
    for outcome in &mut seed.outcomes {
        outcome.outcome = DeclarationOutcomeKind::IdempotentNoop;
    }
    Ok(Some(seed))
}

/// Poll a pending idempotency winner with a bounded backoff schedule.
async fn wait_for_replay(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<(DeclarationOperationId, DeclarationReplaySeed), WyrdError> {
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
    fields(operation = "card.declaration.initialize_uploads")
)]
async fn initialize_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: DeclarationOperationId,
    seed: DeclarationReplaySeed,
    idempotency_key: &str,
) -> Result<DeclareCardResponse, WyrdError> {
    let mut response = DeclareCardResponse {
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
        abort_pending_declaration_cards(state, caller, &response.outcomes, idempotency_key).await;
        return Err(error);
    }
    plans.sort_by(|left, right| {
        left.card_ref
            .kind
            .wire_name()
            .cmp(right.card_ref.kind.wire_name())
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
        response.outcomes[index] = match complete_card(state, caller, card_uid, idempotency_key)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                abort_pending_declaration_cards(state, caller, &response.outcomes, idempotency_key)
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
    operation_id: DeclarationOperationId,
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
async fn abort_pending_declaration_cards(
    state: &AppState,
    caller: &Caller,
    outcomes: &[CardDeclarationOutcome],
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
    operation_id: DeclarationOperationId,
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
    let resolved = EffectiveSpecs::resolve(&mut conn, submissions).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(resolved)
}

/// Validate composition and build graph order before any registry I/O.
///
/// # Errors
/// Returns a stable Wyrd error when graph construction, root selection, or
/// Service-root composition validation fails.
fn plan_declaration_graph(
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
fn plan_declaration(
    request: DeclareCardRequest,
    request_hash: String,
    external_refs: ResolvedRefs,
    order: TopoOrder,
    root: RootPick,
) -> DeclarationPlan {
    DeclarationPlan {
        submissions: request.submissions,
        request_hash,
        order,
        root,
        external_refs,
    }
}

/// Writes one planned declaration in a single tenant transaction.
///
/// Owns the server state and authenticated caller the atomic write needs:
/// the tenant connection comes from `state`, and the caller's tenant and
/// principal scope every idempotency row and persisted node.
/// [`DeclarationPlan`] stays a pure value; this writer carries the IO.
struct DeclarationWriter<'a> {
    /// Server state that opens the tenant registry connection.
    state: &'a AppState,
    /// Authenticated caller whose tenant and principal scope the write.
    caller: &'a Caller,
}

impl DeclarationWriter<'_> {
    /// Reserve idempotency and atomically persist every topo-ordered node.
    ///
    /// A lost idempotency race rolls the attempt back and returns the winner's
    /// replay instead. Each external dependency must still be the exact Active UID preflight
    /// validated; a replacement at the same identity is refused, not bound.
    /// Cancellation before commit rolls the whole transaction back.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when a preflight-validated UID is
    /// no longer Active, and the idempotency, validation, or registry failure
    /// otherwise; nothing commits on any error.
    #[tracing::instrument(skip(self, plan), fields(operation = "card.declaration.write"))]
    async fn write(
        &self,
        idempotency_key: &str,
        mut plan: DeclarationPlan,
    ) -> Result<(DeclarationOperationId, DeclarationReplaySeed), WyrdError> {
        let operation_id = DeclarationOperationId::new(Uuid::now_v7());
        let mut conn = self
            .state
            .registry_tenant_conn(self.caller.data_tenant_id)
            .await?;
        // Recheck the exact UIDs preflight validated before reserving idempotency,
        // so a dependency rejection — including a replacement Card at the same
        // identity — rolls back the entire attempt, including its bookkeeping row.
        // The row locks remain held while cards and relationships are written below.
        plan.external_refs = recheck_active_card_refs(&mut conn, &plan.external_refs).await?;
        let inserted = insert_declaration_operation(
            &mut conn,
            NewDeclarationOperation {
                operation_id,
                attribution: self.caller.attribution(),
                idempotency_key,
                request_hash: &plan.request_hash,
            },
        )
        .await?;
        if !inserted {
            drop(conn);
            return wait_for_replay(self.state, self.caller, idempotency_key, &plan.request_hash)
                .await;
        }

        let mut sibling_uids = HashMap::new();
        let mut outcomes: Vec<CardDeclarationOutcome> = Vec::with_capacity(plan.order.nodes.len());
        let mut resolved_root = None;
        for node in &plan.order.nodes {
            let is_root = same_identity(&node.card_ref, &plan.root.root);
            let authored = find_submission(&plan.submissions, &node.card_ref)?;
            let mut submission = authored.clone();
            let mut spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
                .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
            bind_card_references(&mut spec, &plan.external_refs, &sibling_uids)?;
            if let Spec::Agent(agent) = &mut spec
                && let InlineableRef::Inline(prompt) = &agent.prompt
            {
                let mut prompt_submission = inline_prompt_submission(&submission, prompt)?;
                pin_existing_inline_prompt(&mut conn, &submission, &mut prompt_submission).await?;
                let outcome = persist_node(
                    &mut conn,
                    self.state,
                    self.caller,
                    operation_id,
                    &mut prompt_submission,
                )
                .await?;
                agent.prompt = InlineableRef::Ref(outcome.card_ref.clone());
                if !outcomes
                    .iter()
                    .any(|selected| selected.card_ref.uid == outcome.card_ref.uid)
                {
                    outcomes.push(outcome);
                }
            }
            submission.spec =
                serde_json::to_value(&spec).map_err(WyrdError::from_spec_serialization)?;
            let outcome = persist_node(
                &mut conn,
                self.state,
                self.caller,
                operation_id,
                &mut submission,
            )
            .await?;
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
        let root = resolved_root
            .ok_or_else(|| WyrdError::internal("root registration outcome is missing"))?;
        let response = DeclareCardResponse {
            root,
            outcomes,
            upload_plans: Vec::new(),
        };
        let seed = replay_seed(&response, &plan.submissions);
        commit_declaration_operation(&mut conn, operation_id, &seed).await?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok((operation_id, seed))
    }
}

/// Resolve one node's version, deduplicate when possible, and persist when fresh.
///
/// A fresh node writes its Card row, edges, artifact inventory, projected
/// Card-bound principal, and — through [`BindingProjector`] — its frozen
/// verification bindings on the caller's declaration transaction; nothing is
/// committed here, so any failure rolls the whole declaration back. A Card
/// that declares artifacts becomes due for reconciliation only once its
/// client's presigned upload window has closed, so the reconciler recovers
/// abandoned uploads without racing the client's own completion.
///
/// # Errors
/// Returns a registry error when the submission cannot be canonicalized or
/// version-resolved, a referenced binding target cannot be frozen, or any
/// Card, edge, principal, or binding write fails.
async fn persist_node(
    conn: &mut TenantConn<'_>,
    state: &AppState,
    caller: &Caller,
    operation_id: DeclarationOperationId,
    submission: &mut CardSubmission,
) -> Result<CardDeclarationOutcome, WyrdError> {
    let mut card = submission_card(submission)?;
    let outbound_refs = scope_child_card_refs(&card.spec);
    let spec_hash = card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    let artifact_hash = canonical_artifact_manifest_hash(&submission.artifacts)?;
    lock_version_line(conn, card.kind.clone(), &card.metadata.name).await?;
    if let Some(existing) = resolve_existing(
        conn,
        operation_id,
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
            attribution: caller.attribution(),
            operation_id,
            status: CardStatus::Pending,
            spec_hash: spec_hash.as_str(),
            artifact_hash: artifact_hash.as_deref(),
            reconcile_delay: if submission.artifacts.is_empty() {
                StdDuration::ZERO
            } else {
                state.storage.presign_ttl()
            },
        },
    )
    .await?;
    persist_outbound_relationships(conn, &row.card_uid, &outbound_refs).await?;
    insert_artifact_manifest_rows(conn, &row.card_uid, &submission.artifacts).await?;
    if matches!(card.kind, CardKind::Service | CardKind::Agent) {
        BindingProjector::new(conn)
            .project(&row.card_uid, &card.kind, &card.spec)
            .await?;
    }
    if let Spec::Verifier(verifier) = &card.spec {
        for (requirement, check) in verifier.drift_checks() {
            if let DriftSignal::Distribution { baseline_ref, .. } = &check.spec.signal {
                let data_card_uid = pinned_uid(baseline_ref.as_card_ref())?;
                DriftBaselineQueue::default()
                    .insert_pending(conn, &row.card_uid, requirement, &data_card_uid)
                    .await
                    .map_err(registry_db_error)?;
            }
        }
    }
    Ok(outcome_row_to_response(
        &row,
        DeclarationOutcomeKind::Declared,
    ))
}

/// Transaction-scoped projector of one owner Card's verification bindings.
///
/// Declaration builds one per bound Service or Agent inside its own
/// caller-owned [`TenantConn`], after the owner Card is written, so the Card
/// and binding rows commit or roll back together. It never commits.
struct BindingProjector<'a, 'c> {
    /// The declaration transaction; sibling Cards persisted earlier in it,
    /// such as a referenced Trigger, are visible here.
    conn: &'a mut TenantConn<'c>,
}

impl<'a, 'c> BindingProjector<'a, 'c> {
    /// Borrow the declaration transaction for one owner's projection.
    fn new(conn: &'a mut TenantConn<'c>) -> Self {
        Self { conn }
    }

    /// Freeze and project an owner Card version's effective bindings.
    ///
    /// Freezes every binding through [`BindingProjector::freeze`] and persists
    /// the rows with [`project_bindings`] under their natural keys, so a
    /// re-projection of the same immutable Card keeps its binding IDs.
    ///
    /// # Errors
    /// Returns the freeze errors documented on [`BindingProjector::freeze`]
    /// and the mapped registry database error when a row cannot be written;
    /// the caller then rolls back the whole declaration.
    async fn project(
        &mut self,
        owner: &CardUid,
        kind: &CardKind,
        spec: &Spec,
    ) -> Result<(), WyrdError> {
        let bindings = self.freeze(owner, spec).await?;
        project_bindings(self.conn, owner, kind, &bindings)
            .await
            .map_err(registry_db_error)?;
        Ok(())
    }

    /// Freeze a bound Service or Agent spec's effective verification bindings.
    ///
    /// Walks the three legal owner locations — Service level and standalone
    /// Agent (keyed by [`OWNER_OCCURRENCE_KEY`]) and each Service component
    /// occurrence (keyed by its alias) — of a spec whose refs resolution
    /// already pinned to exact UIDs. A referenced Trigger is read back by UID
    /// inside this transaction (a sibling Trigger was persisted earlier in it)
    /// so its schedule is frozen with the binding; an inline Trigger or
    /// Operator is frozen as its canonical spec digest. A binding without
    /// `runs_on` is explicit-only and has no activation to project, so it is
    /// not frozen; a client selects its Verifier through the Card's
    /// `verified_by` instead.
    ///
    /// # Errors
    /// Returns `WYRD_INTERNAL` when a bound ref lacks its pinned UID or a
    /// referenced Trigger UID names another kind, the canonicalization error
    /// for an inline body that cannot be hashed, and the registry error from
    /// reading a referenced Trigger.
    async fn freeze(&mut self, owner: &CardUid, spec: &Spec) -> Result<Vec<NewBinding>, WyrdError> {
        let mut sites: Vec<(&str, CardUid, &[VerificationBinding])> = Vec::new();
        match spec {
            Spec::Agent(agent) => {
                sites.push((OWNER_OCCURRENCE_KEY, owner.clone(), &agent.verified_by));
            }
            Spec::Service(service) => {
                sites.push((OWNER_OCCURRENCE_KEY, owner.clone(), &service.verified_by));
                for component in &service.components {
                    let subject = pinned_uid(component.card_ref.as_card_ref())?;
                    sites.push((&component.alias, subject, &component.verified_by));
                }
            }
            _ => {}
        }
        let mut bindings = Vec::new();
        for (occurrence, subject, site) in sites {
            for binding in site {
                let Some(runs_on) = &binding.runs_on else {
                    continue;
                };
                let (trigger, activation) = match runs_on {
                    InlineableRef::Inline(trigger) => {
                        let body = Spec::Trigger((**trigger).clone());
                        (inline_digest(&body)?, trigger.activation.clone())
                    }
                    reference => {
                        let uid = pinned_uid(reference.as_card_ref())?;
                        let Spec::Trigger(trigger) =
                            sql_get_card_by_uid(self.conn, &uid).await?.spec
                        else {
                            return Err(WyrdError::internal("runs_on UID is not a Trigger Card"));
                        };
                        (FrozenTarget::Uid(uid), trigger.activation)
                    }
                };
                let verifier_card_uid = pinned_uid(binding.verifier.as_card_ref())?;
                let activation = match activation {
                    TriggerActivation::Schedule { cron, tz } => {
                        BindingActivation::Schedule { cron, tz }
                    }
                    TriggerActivation::GroupEnd { after } => {
                        let (sources, order_by) = self
                            .group_sources(occurrence, &subject, site, &verifier_card_uid)
                            .await?;
                        BindingActivation::GroupEnd(GroupEndSettings {
                            after,
                            sources,
                            order_by,
                        })
                    }
                };
                // Registration admits only the list its activation reacts
                // through, so chaining both freezes exactly that one.
                let operators = binding
                    .operator_lists()
                    .into_iter()
                    .flat_map(|(_, operators)| operators)
                    .map(|operator| match operator {
                        InlineableRef::Inline(body) => {
                            inline_digest(&Spec::Operator((**body).clone()))
                        }
                        reference => pinned_uid(reference.as_card_ref()).map(FrozenTarget::Uid),
                    })
                    .collect::<Result<_, _>>()?;
                bindings.push(NewBinding {
                    subject_occurrence_key: occurrence.to_owned(),
                    card_uid: subject.clone(),
                    verifier_card_uid,
                    trigger,
                    operators,
                    activation,
                });
            }
        }
        Ok(bindings)
    }

    /// Pin each source input of a `group_end` binding's Verifier to the exact
    /// source binding it reads, with the Verifier's declared `order_by`.
    ///
    /// The Verifier is read back by UID inside this transaction; each source
    /// resolves within the same `site` through [`source_binding`], which
    /// registration preflight already proved unique, so the pinned identity
    /// is this site's occurrence and subject with the source's exact Verifier.
    ///
    /// # Errors
    /// Returns `WYRD_INTERNAL` when the Verifier UID names another kind, a
    /// source no longer resolves to one binding, or a source ref is unpinned,
    /// and the registry error from reading the Verifier.
    async fn group_sources(
        &mut self,
        occurrence: &str,
        subject: &CardUid,
        site: &[VerificationBinding],
        verifier: &CardUid,
    ) -> Result<(BTreeMap<SlotName, GroupSource>, Option<OrderBy>), WyrdError> {
        let Spec::Verifier(verifier) = sql_get_card_by_uid(self.conn, verifier).await?.spec else {
            return Err(WyrdError::internal(
                "bound Verifier UID is not a Verifier Card",
            ));
        };
        let mut sources = BTreeMap::new();
        for (slot, input) in verifier.inputs {
            let InputSlot::Source(reference) = input else {
                continue;
            };
            let name = reference
                .parse()
                .map_err(|error| WyrdError::internal(error.to_string()))?
                .verifier;
            let source = source_binding(site, &name)
                .map_err(|_| WyrdError::internal("group source does not resolve to one binding"))?;
            sources.insert(
                slot,
                GroupSource {
                    reference,
                    subject_occurrence_key: occurrence.to_owned(),
                    card_uid: subject.clone(),
                    verifier_card_uid: pinned_uid(source.verifier.as_card_ref())?,
                },
            );
        }
        Ok((sources, verifier.order_by))
    }
}

/// Return the UID resolution pinned on a bound ref.
///
/// # Errors
/// Returns `WYRD_INTERNAL` when the ref is absent or unpinned, which
/// resolution already refuses before persistence.
fn pinned_uid(card_ref: Option<&CardRef>) -> Result<CardUid, WyrdError> {
    card_ref
        .and_then(|card_ref| card_ref.uid.clone())
        .ok_or_else(|| WyrdError::internal("verification binding ref is not pinned to a UID"))
}

/// Freeze an inline Trigger or Operator body as its canonical spec digest.
///
/// # Errors
/// Returns the canonicalization error when the body cannot be hashed.
fn inline_digest(body: &Spec) -> Result<FrozenTarget, WyrdError> {
    let digest = body
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    Ok(FrozenTarget::Digest(digest.to_string()))
}

/// Return an identical existing row or pin the fresh resolved version on the card.
///
/// An exact pin compares against the stored row at that identity. Auto and
/// scoped versions resolve against the line's Active rows plus the Pending rows
/// `operation_id` already wrote, so one composite selecting the same line twice
/// reuses or advances its own earlier choice instead of colliding with it.
///
/// # Errors
/// Returns `WYRD_REGISTRY_409_SPEC_DRIFT` when the pinned row's content differs,
/// and the version resolution or registry error otherwise.
async fn resolve_existing(
    conn: &mut TenantConn<'_>,
    operation_id: DeclarationOperationId,
    card: &mut Card,
    spec_hash: &str,
    artifact_hash: Option<&str>,
) -> Result<Option<ExistingNode>, WyrdError> {
    match card.metadata.version.clone() {
        Some(VersionSpec::Pin(version)) => {
            let existing =
                find_card_by_ref(conn, card.kind.clone(), &card.metadata.name, &version).await?;
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
                outcome: DeclarationOutcomeKind::IdempotentNoop,
            }))
        }
        authored => match resolve_version(
            conn,
            card.kind.clone(),
            &card.metadata.name,
            authored.as_ref(),
            card.metadata.bump.as_ref(),
            SubmittedCardIdentity {
                operation_id,
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
            Resolution::Deduplicated { version } => {
                find_card_by_ref(conn, card.kind.clone(), &card.metadata.name, &version)
                    .await
                    .map(|row| {
                        row.map(|row| ExistingNode {
                            row,
                            outcome: DeclarationOutcomeKind::Deduplicated,
                        })
                    })
            }
        },
    }
}

/// Enforce request-wide invariants before database access.
///
/// The server is the authority for every registration rule; the loader's
/// checks only report the same refusals earlier. Each submission must be
/// `wyrd/v1`, decode as its kind, bind validly, publish distinct non-nested
/// artifact paths, match its declared artifact hash, and, as a Verifier, pass
/// [`VerifierSpec::validate`](wyrd_spec::card::verifier::VerifierSpec::validate).
///
/// # Errors
/// Returns `WYRD_REGISTRY_*` for a multi-card artifact request, a wrong
/// apiVersion, an undecodable spec, a repeated or nested artifact path or a
/// manifest hash mismatch, and the binding or Verifier
/// `WYRD_SPEC_400_VALIDATION` refusal otherwise.
fn validate_request(request: &DeclareCardRequest) -> Result<(), WyrdError> {
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
        if let Some(error) = spec_binding_errors(&spec).into_iter().next() {
            return Err(error);
        }
        match &spec {
            Spec::Verifier(verifier) => verifier.validate()?,
            Spec::Artifact(artifact) => artifact.validate()?,
            _ => {}
        }
        validate_artifact_manifest(&submission.artifacts)?;
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
fn hash_request(request: &DeclareCardRequest) -> Result<String, WyrdError> {
    let canonical = canonical_order(&request.submissions)
        .into_iter()
        .map(|index| &request.submissions[index])
        .collect::<Vec<_>>();
    let hashes = canonical
        .iter()
        .map(|submission| canonical_artifact_manifest_hash(&submission.artifacts))
        .collect::<Result<Vec<_>, _>>()?;
    declaration_request_hash(&canonical, &hashes)
}

/// Strip upload URLs from the durable response and retain only replay inputs.
fn replay_seed(
    response: &DeclareCardResponse,
    submissions: &[CardSubmission],
) -> DeclarationReplaySeed {
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
    DeclarationReplaySeed {
        root: response.root.clone(),
        outcomes: response.outcomes.clone(),
        artifact_manifest_paths,
    }
}

/// Build the Prompt Card submission an Agent's inline Prompt declares as.
///
/// The Prompt is named `<agent-name>-prompt` and carries
/// no version intent, so [`persist_node`] reuses an identical existing version
/// or allocates the next one on the Prompt's own line. The Agent then pins the
/// exact UID-bearing reference [`persist_node`] returns.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the derived name is not a
/// valid Card name or the Prompt body does not serialize.
fn inline_prompt_submission(
    agent: &CardSubmission,
    prompt: &skald_spec::Prompt,
) -> Result<CardSubmission, WyrdError> {
    let name = CardName::new(format!("{}-prompt", agent.metadata.name))
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(CardSubmission {
        api_version: ApiVersion::v1(),
        kind: CardKind::Prompt,
        metadata: Metadata {
            name,
            version: None,
            bump: None,
            uid: None,
            labels: Labels::new(),
            annotations: Annotations::new(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        },
        spec: serde_json::to_value(prompt).map_err(WyrdError::from_spec_serialization)?,
        artifacts: Vec::new(),
    })
}

/// Pin a generated inline Prompt to the version an existing exact Agent pins.
///
/// When `agent` names an exact version that is already declared and that
/// stored Agent pins a Prompt at the generated `<agent>-prompt` identity,
/// `prompt` takes that exact version instead of auto-selecting from the line's
/// latest. [`persist_node`] then reuses the stored Prompt when the inline body
/// is unchanged or refuses with spec drift when it changed, so reapplying an
/// unchanged exact Agent keeps its original Prompt after the line advanced.
/// A fresh Agent version, or a stored Agent whose Prompt has another identity,
/// leaves `prompt` on normal auto selection.
///
/// # Errors
/// Returns the registry error when the existing Agent lookup fails.
async fn pin_existing_inline_prompt(
    conn: &mut TenantConn<'_>,
    agent: &CardSubmission,
    prompt: &mut CardSubmission,
) -> Result<(), WyrdError> {
    let Some(version) = agent.metadata.resolved_pin() else {
        return Ok(());
    };
    let Some(existing) =
        find_card_by_ref(conn, CardKind::Agent, &agent.metadata.name, version).await?
    else {
        return Ok(());
    };
    if let Spec::Agent(stored) = existing.spec
        && let InlineableRef::Ref(pinned) = stored.prompt
        && pinned.kind == CardKind::Prompt
        && pinned.name == prompt.metadata.name
    {
        prompt.metadata.version = Some(VersionSpec::Pin(pinned.version));
    }
    Ok(())
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
///
/// A submission matches on flat `kind` and `name`, and on `version` only when
/// it authored an exact pin; an unpinned submission matches the node's
/// allocated version.
///
/// # Errors
/// Returns an internal error when no submission matches, which means graph
/// construction lost the node's authored submission.
fn find_submission<'a>(
    submissions: &'a [CardSubmission],
    node: &CardRef,
) -> Result<&'a CardSubmission, WyrdError> {
    submissions
        .iter()
        .find(|submission| {
            submission.kind == node.kind
                && submission.metadata.name == node.name
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
        GraphError::UnboundVerifierPeer { root, peer } => WyrdError::SpecUnboundVerifierPeer {
            message: format!(
                "{} {} is bound by no submitted subject in Service-root bundle {}",
                peer.kind.wire_name(),
                peer.name,
                root.name
            ),
            details: serde_json::json!({
                "root": root,
                "peer": peer,
                "binding_owner_kinds": ["Service", "Agent"],
            }),
        },
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
/// key so transport retries retain the declaration saga key.
#[tracing::instrument(
    skip(state, caller, idempotency_key),
    fields(
        operation = "card.declaration.complete",
        idempotency_key_present = !idempotency_key.is_empty()
    )
)]
pub async fn complete_card(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    idempotency_key: &str,
) -> Result<CardDeclarationOutcome, WyrdError> {
    complete_card_inner(state, caller, card_uid, idempotency_key, None, true).await
}

/// Execute completion for a reconciler claim without changing its durable lease.
pub(crate) async fn reconcile_card_claim(
    state: &AppState,
    caller: &Caller,
    claim: &CardReconcileClaim,
) -> Result<(), WyrdError> {
    let card_uid = CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?;
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
    record_card_deletion(state, caller, &completion_state.card).await?;
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    if !lock_card_reconciliation_lease(&mut conn, &card_uid, claim.reconcile_lease_owner).await? {
        conn.commit().await.map_err(registry_db_error)?;
        return Ok(());
    }
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
    retry_delay_seconds: i64,
) -> Result<bool, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let dead_lettered = record_card_reconciliation_failure(
        &mut conn,
        &CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?,
        claim.reconcile_lease_owner,
        retry_delay_seconds,
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
    retry_delay_seconds: i64,
) -> Result<(), WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    reschedule_card_reconciliation(
        &mut conn,
        &CardUid::from_uuid(claim.card_uid).map_err(WyrdError::from_card_uid_error)?,
        claim.reconcile_lease_owner,
        retry_delay_seconds,
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
        executed_by: None,
    }
}

async fn complete_card_inner(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    _idempotency_key: &str,
    lease_owner: Option<Uuid>,
    schedule_failures: bool,
) -> Result<CardDeclarationOutcome, WyrdError> {
    let completion_state = load_card_completion_state(state, caller, card_uid).await?;
    validate_card_completion_status(&completion_state.card, card_uid)?;
    if completion_state.card.status == CardStatus::Active {
        if schedule_failures || lease_owner.is_some() {
            let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
            mark_card_reconciliation_succeeded(&mut conn, card_uid, lease_owner).await?;
            conn.commit().await.map_err(registry_db_error)?;
        }
        return load_card_declaration_outcome(
            state,
            caller,
            card_uid,
            DeclarationOutcomeKind::IdempotentNoop,
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
    load_card_declaration_outcome(
        state,
        caller,
        card_uid,
        if activated {
            DeclarationOutcomeKind::Declared
        } else {
            DeclarationOutcomeKind::IdempotentNoop
        },
    )
    .await
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
        1,
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
/// The route has already staged its allowed verdict on the process audit
/// outbox. The SQL transition commits before any backend delete. If a backend
/// cleanup fails, the deleted card and its cleanup rows remain intact so a
/// later retry can repeat the idempotent operation.
///
/// # Errors
/// Returns the not-found, conflict, or registry failure of the transition, and
/// [`WyrdError::RegistryArtifactVerifyFailed`] when backend cleanup is incomplete.
pub async fn delete_card_with_kind(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    kind: CardKind,
) -> Result<DeleteCardResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let delete_state = soft_delete_card_with_kind(&mut conn, card_uid, kind).await?;
    conn.commit().await.map_err(registry_db_error)?;
    finish_card_delete(state, caller, delete_state).await
}

/// Soft-delete one Card selected by its exact public CardRef.
///
/// Behaves exactly as [`delete_card_with_kind`], selecting by CardRef.
///
/// # Errors
/// Returns the same failures as [`delete_card_with_kind`].
#[tracing::instrument(skip(state, caller), fields(operation = "card.declaration.delete"))]
pub async fn delete_card_by_ref(
    state: &AppState,
    caller: &Caller,
    card_ref: &CardRef,
) -> Result<DeleteCardResponse, WyrdError> {
    let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
    let delete_state = soft_delete_card_by_ref(&mut conn, card_ref).await?;
    conn.commit().await.map_err(registry_db_error)?;
    finish_card_delete(state, caller, delete_state).await
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
        record_card_deletion(state, caller, &delete_state.card).await?;
        let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
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
        operation = "card.declaration.cleanup",
        idempotency_key_present = !idempotency_key.is_empty()
    )
)]
async fn abort_card(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    idempotency_key: &str,
) -> Result<CardDeclarationOutcome, WyrdError> {
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
    load_card_declaration_outcome(
        state,
        caller,
        card_uid,
        DeclarationOutcomeKind::IdempotentNoop,
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

/// Return whether the manifest has reached a storage state completion can use.
fn manifest_ready_for_completion(manifest: &CardManifestCompletionRow) -> bool {
    manifest.manifest_status == "verified"
        || manifest.storage_status.as_deref() == Some("completed")
}

/// Return whether a manifest must be abandoned and cleaned up.
fn manifest_needs_cleanup(manifest: &CardManifestCompletionRow) -> bool {
    if manifest_ready_for_completion(manifest) || manifest.storage_upload_live {
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

/// Verify every manifest against the server-owned storage state, complete
/// each upload through the shared storage completion owner, and record every
/// file in the object inventory before activation.
///
/// # Errors
/// Returns the manifest verification and storage completion failures, and
/// the inventory's retryable write failure.
async fn verify_card_manifests(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    manifests: &[CardManifestCompletionRow],
) -> Result<(), WyrdError> {
    let mut facts = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        verify_manifest_storage(state, caller, card_uid, manifest).await?;
        let Some(upload_id) = manifest.upload_id else {
            continue;
        };
        // ponytail: completion re-heads each object verify_manifest_storage
        // already headed; fold the checks together if Card fan-out grows.
        let stored = reconcile_upload(
            &state.storage,
            state.postgres.wyrd(),
            &storage_caller(caller),
            UploadId::from_uuid(upload_id),
        )
        .await?
        .stored;
        facts.push(ObjectFact {
            storage_path: stored.storage_path,
            upload_id: Some(upload_id.to_string()),
            backend: state.storage.backend().to_string(),
            sha256: Some(stored.sha256),
            size_bytes: i64::try_from(stored.size_bytes)
                .map_err(|_| WyrdError::internal("stored artifact size overflows"))?,
            content_type: stored.content_type,
            sse_marker: stored.sse_marker,
            origin: ObjectOrigin::CardFile,
            card_uid: Some(card_uid.clone()),
            relative_path: Some(manifest.relative_path.clone()),
            state: ObjectState::Completed,
        });
    }
    ObjectInventory::new(state)
        .record(caller.data_tenant_id, caller.attribution(), &facts)
        .await
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
async fn load_card_declaration_outcome(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    outcome: DeclarationOutcomeKind,
) -> Result<CardDeclarationOutcome, WyrdError> {
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
            failures.push(error.clone());
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

/// The storage path of `card`'s immutable blob: its recorded URI, else the
/// deterministic path a declaration writes.
fn card_blob_path(caller: &Caller, card: &ParsedCardRow) -> String {
    card.card_blob_uri
        .as_deref()
        .and_then(|uri| uri.strip_prefix("wyrd://"))
        .map_or_else(
            || {
                tenant_path::build(
                    caller.data_tenant_id,
                    &card.card_uid.to_string(),
                    &format!("blob/{}.json", card.spec_hash),
                )
            },
            str::to_owned,
        )
}

/// Record the existing lifecycle's deletion of every object of `card` in the
/// object inventory, which no later completion replay undoes.
///
/// # Errors
/// Returns the inventory's retryable write failure; the caller's cleanup
/// reconciliation retries it.
async fn record_card_deletion(
    state: &AppState,
    caller: &Caller,
    card: &ParsedCardRow,
) -> Result<(), WyrdError> {
    let deleted = ObjectFact::deleted(
        &card.card_uid,
        card_blob_path(caller, card),
        state.storage.backend().to_string(),
    );
    ObjectInventory::new(state)
        .record(caller.data_tenant_id, caller.attribution(), &[deleted])
        .await
}

/// Remove the deterministic blob written before a declaration failure.
async fn cleanup_card_blob(
    state: &AppState,
    caller: &Caller,
    card: &ParsedCardRow,
) -> Result<(), String> {
    let path = card_blob_path(caller, card);
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
        Some("pending" | "initiating") => {
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
/// decision was already audited when declaration was received — so the durable
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
        relationships: Relationships::default(),
        status: None,
    };
    let bytes = immutable_card_blob_bytes(envelope)?;
    let envelope = ObjectFact {
        storage_path: blob_path.clone(),
        upload_id: None,
        backend: state.storage.backend().to_string(),
        sha256: None,
        size_bytes: i64::try_from(bytes.len())
            .map_err(|_| WyrdError::internal("card blob size overflows"))?,
        content_type: Some("application/json".to_owned()),
        sse_marker: None,
        origin: ObjectOrigin::CardEnvelope,
        card_uid: Some(card.card_uid.clone()),
        relative_path: None,
        state: ObjectState::Completed,
    };
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
    ObjectInventory::new(state)
        .record(caller.data_tenant_id, caller.attribution(), &[envelope])
        .await?;
    Ok(format!("wyrd://{blob_path}"))
}

/// Convert a server-tier storage error to the shared Wyrd error catalog.
fn map_storage_error(error: StorageError) -> WyrdError {
    WyrdStorageError::from(error).into()
}

#[cfg(test)]
mod tests {
    use super::{
        card_upload_entry, hash_request, immutable_card_blob_bytes, plan_declaration_graph,
        validate_request, verify_card_hashes,
    };
    use uuid::Uuid;
    use wyrd_spec::registry::{
        ArtifactManifestEntry, DeclareCardRequest, RelativeArtifactPath,
        canonical_artifact_manifest_hash,
    };
    use wyrd_spec::storage::{UploadId, UploadPlan};

    /// Decode a compact declaration request used by pure composition tests.
    fn request(value: serde_json::Value) -> DeclareCardRequest {
        serde_json::from_value(value).expect("test_setup: registration fixture must deserialize")
    }

    /// Build one metadata-only Prompt submission fixture.
    fn prompt(name: &str) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": name, "version": "1.0.0" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}] },
            "artifacts": []
        })
    }

    /// Build one Service submission fixture with the provided spec.
    fn service(name: &str, spec: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Service",
            "metadata": { "name": name, "version": "1.0.0" },
            "spec": spec,
            "artifacts": []
        })
    }

    /// Build one Verifier submission fixture with one assertion.
    fn verifier(name: &str) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Verifier",
            "metadata": { "name": name, "version": "1.0.0" },
            "spec": {
                "inputs": { "x": { "type": "json" } },
                "requirements": [{
                    "id": "valid",
                    "statement": "The input is true.",
                    "check": { "kind": "assert", "path": "inputs.x", "operator": "equals", "expected": true }
                }]
            },
            "artifacts": []
        })
    }

    /// Build matching row/blob/inventory values for read-integrity regression tests.
    ///
    /// # Panics
    /// Panics when a static fixture value is invalid.
    fn matching_read_integrity_values() -> (
        wyrd_spec::envelope::Card,
        Vec<ArtifactManifestEntry>,
        String,
        String,
    ) {
        let manifest = vec![ArtifactManifestEntry {
            relative_path: RelativeArtifactPath::new("weights.bin")
                .expect("test_setup: artifact path is valid"),
            sha256: Some("YQ==".to_owned()),
            size_bytes: Some(1),
            content_type: Some("application/octet-stream".to_owned()),
        }];
        let mut card: wyrd_spec::envelope::Card = serde_json::from_value(serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": "integrity", "version": "1.0.0" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}] }
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

    /// Reject a peer-only Verifier Service component before registry access.
    #[test]
    fn declaration_graph_rejects_verifier_service_component() {
        let request = request(serde_json::json!({
            "submissions": [service("app", serde_json::json!({
                "components": [{
                    "alias": "quality",
                    "ref": {
                        "kind": "Verifier",
                        "name": "quality",
                        "version": "1.0.0"
                    }
                }]
            }))]
        }));

        let error = plan_declaration_graph(&request.submissions)
            .expect_err("Verifier Service component must fail before registry I/O");

        assert_eq!(error.code(), "WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND");
    }

    /// Reject an unbound Verifier submitted beside a Service root before registry access.
    #[test]
    fn declaration_graph_rejects_unbound_verifier_peer() {
        let request = request(serde_json::json!({
            "submissions": [service("app", serde_json::json!({})), verifier("quality")]
        }));

        let error = plan_declaration_graph(&request.submissions)
            .expect_err("Service-root Verifier requires a submitted binding");

        assert_eq!(error.code(), "WYRD_SPEC_400_UNBOUND_VERIFIER_PEER");
    }

    /// Preserve standalone Verifier declaration at the server boundary.
    #[test]
    fn declaration_graph_accepts_standalone_verifier() {
        let request = request(serde_json::json!({
            "submissions": [verifier("quality")]
        }));

        plan_declaration_graph(&request.submissions)
            .expect("standalone Verifier registration remains valid");
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
            UploadPlan::Multipart {
                part_count: 2,
                part_size_bytes: 8,
                part_url_ttl_secs: 60,
                required_headers: Vec::new(),
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
    ///
    /// # Panics
    /// Panics when the fixture Card does not decode, a blob fails to
    /// serialize, or the two blobs differ.
    #[test]
    fn immutable_blob_projection_is_deterministic_and_excludes_live_edges() {
        let mut card: wyrd_spec::envelope::Card = serde_json::from_value(serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": "blob", "version": "1.0.0" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}] },
            "relationships": { "outbound": [], "inbound": ["live-edge"] },
            "status": { "phase": "pending" }
        }))
        .expect("test_setup: blob card fixture is valid");

        let first = immutable_card_blob_bytes(card.clone()).expect("blob serializes");
        card.status = Some(wyrd_spec::envelope::Status {
            phase: "active".to_owned(),
            message: None,
            updated_at: None,
            verification: None,
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
        manifest[0].sha256 = Some("Yg==".to_owned());

        let error = verify_card_hashes(&spec_hash, Some(&artifact_hash), &card, &manifest)
            .expect_err("inventory divergence must reject the read");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
        assert!(error.to_string().contains("artifact hash"));
    }
}

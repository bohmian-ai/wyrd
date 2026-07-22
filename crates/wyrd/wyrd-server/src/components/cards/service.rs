//! Card registration service orchestration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use tokio::sync::Semaphore;
use uuid::Uuid;
use wyrd_semver::VersionSpec;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::envelope::{Card, Metadata, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::{
    GraphError, RootPick, TopoOrder, build, canonical_order, graph_ready_submissions, pick_root,
    topo_sort,
};
use wyrd_spec::ids::CardUid;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::reference::{CardRef, CardRefIdentity};
use wyrd_spec::registry::{
    CardLifecycleStatus, CardRegistrationOutcome, CardSubmission, CardUploadEntry, CardUploadPlan,
    CreateCardRequest, CreateCardResponse, RegistrationOperationId, RegistrationOutcomeKind,
    RegistrationReplaySeed, RelativeArtifactPath,
};
use wyrd_spec::storage::{UploadId, UploadInitRequest, UploadPlan};
use wyrd_spec::vala::api::{AuditDecision, AuditResult};
use wyrd_sql::CardStatus;
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, Resolution, SubmittedCardIdentity, activate_card,
    artifact_manifest_hash, commit_registration_operation, fail_card, find_card_by_ref,
    get_card_by_uid, insert_artifact_manifest_rows, insert_card_row, insert_registration_operation,
    lock_version_line, lookup_existing_operation, lookup_expired_operation,
    manifest_completion_rows, manifest_rows_for_init, mark_manifest_upload_initialized,
    record_blob_failure, record_card_blob, registration_request_hash, resolve_version,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::storage::multipart_uploads;
use wyrd_storage::StorageError;
use wyrd_storage::service::{upload_abort, upload_init};
use wyrd_storage::tenant_path;

use crate::audit::{append_on, audit_event, record_audit};
use crate::components::auth::Caller;
use crate::components::cards::mapping::{
    existing_row_to_response, outcome_row_to_response, relationships_from_spec,
};
use crate::components::cards::resolve::{
    ResolvedRefs, bind_card_references, resolve_card_references,
};
use crate::components::storage::routes::storage_caller;
use crate::state::AppState;

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
    row: wyrd_sql::row_types::cards::ParsedCardRow,
    outcome: RegistrationOutcomeKind,
}

/// Execute the resolve, graph, idempotency, write, audit, and replay pipeline.
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.compose"))]
pub async fn compose_registration(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    validate_request(&request)?;
    let request_hash = hash_request(&request)?;
    if let Some((operation_id, seed)) =
        replay(state, caller, idempotency_key, &request_hash).await?
    {
        return initialize_uploads(state, caller, operation_id, seed).await;
    }
    let external_refs = resolve_external(state, caller, &request.submissions).await?;
    let plan = plan_registration(request, request_hash, external_refs)?;
    let (operation_id, seed) = write_registration(state, caller, idempotency_key, plan).await?;
    initialize_uploads(state, caller, operation_id, seed).await
}

/// Return a committed response for an identical idempotency key.
async fn replay(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<(RegistrationOperationId, RegistrationReplaySeed)>, WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
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
    operation: wyrd_sql::queries::cards::CardRegistrationOperationRow,
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
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return (card_ref, Vec::new());
            };
            let entries = match tokio::time::timeout(
                StdDuration::from_secs(30),
                initialize_card_uploads(&state, &caller, operation_id, &card_uid, &allowed_paths),
            )
            .await
            {
                Ok(entries) => entries,
                Err(_) => {
                    tracing::error!(%card_uid, "card upload initialization exceeded its budget");
                    Vec::new()
                }
            };
            (card_ref, entries)
        });
    }
    let mut plans = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((card_ref, entries)) if !entries.is_empty() => {
                plans.push(CardUploadPlan { card_ref, entries });
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "card upload initialization task failed"),
        }
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
        response.outcomes[index] = complete_card(state, caller, card_uid).await?;
    }
    Ok(response)
}

/// Initialize every pending manifest row for one card, omitting failed entries.
async fn initialize_card_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    card_uid: &CardUid,
    allowed_paths: &[String],
) -> Vec<CardUploadEntry> {
    let rows = match load_manifest_rows(state, caller, card_uid).await {
        Ok(rows) => rows,
        Err(error) => {
            audit_upload_init_failure(state, caller, card_uid, "manifest lookup failed").await;
            tracing::error!(%error, %card_uid, "manifest lookup failed after registration");
            return Vec::new();
        }
    };
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        if !allowed_paths.iter().any(|path| path == &row.relative_path) {
            continue;
        }
        match initialize_manifest_row(state, caller, operation_id, card_uid, &row).await {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                tracing::error!(%error, %card_uid, path = %row.relative_path, "upload init failed");
                audit_upload_init_failure(state, caller, card_uid, &row.relative_path).await;
            }
        }
    }
    entries
}

/// Load manifest rows in a short tenant transaction.
async fn load_manifest_rows(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<Vec<wyrd_sql::queries::cards::CardArtifactManifestRow>, WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
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
    row: &wyrd_sql::queries::cards::CardArtifactManifestRow,
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
    let upload_uuid = initialized
        .upload_id
        .as_uuid()
        .map_err(|error| WyrdError::internal(format!("upload id invalid: {error}")))?;
    let entry = card_upload_entry(&row.relative_path, initialized.upload_id, initialized.plan)?;
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let event = audit_event(
        caller,
        "card.artifact.upload_init.success",
        &format!("manifest:{}", row.manifest_id),
        "card:write",
        AuditDecision::Allow,
        AuditResult::Success,
        "artifact upload initialization succeeded",
    );
    append_on(&mut conn, &event).await?;
    mark_manifest_upload_initialized(&mut conn, card_uid, &row.relative_path, upload_uuid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(entry)
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

/// Record one post-commit initialization failure without changing registration success.
async fn audit_upload_init_failure(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    detail: &str,
) {
    let event = audit_event(
        caller,
        "card.artifact.upload_init.failed",
        &format!("card:{card_uid}"),
        "card:write",
        AuditDecision::Allow,
        AuditResult::Failure,
        detail,
    );
    if let Err(error) =
        record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &event).await
    {
        tracing::error!(%error, %card_uid, "upload-init failure audit could not be recorded");
    }
}

/// Resolve external references before opening the composite write transaction.
async fn resolve_external(
    state: &AppState,
    caller: &Caller,
    submissions: &[CardSubmission],
) -> Result<ResolvedRefs, WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let resolved = resolve_card_references(&mut conn, submissions).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(resolved)
}

/// Build graph order and root from the authored request.
fn plan_registration(
    request: CreateCardRequest,
    request_hash: String,
    external_refs: ResolvedRefs,
) -> Result<RegistrationPlan, WyrdError> {
    let graph_submissions = graph_ready_submissions(&request.submissions).map_err(graph_error)?;
    let (nodes, edges) = build(&graph_submissions).map_err(graph_error)?;
    let order = topo_sort(&nodes, &edges).map_err(graph_error)?;
    let root = pick_root(&order).map_err(graph_error)?;
    Ok(RegistrationPlan {
        submissions: request.submissions,
        request_hash,
        order,
        root,
        external_refs,
    })
}

/// Reserve idempotency and atomically persist every topo-ordered node and audit.
#[tracing::instrument(
    skip(state, caller, plan),
    fields(operation = "card.registration.write")
)]
async fn write_registration(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    plan: RegistrationPlan,
) -> Result<(RegistrationOperationId, RegistrationReplaySeed), WyrdError> {
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
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
        return wait_for_replay(state, caller, idempotency_key, &plan.request_hash).await;
    }

    let mut sibling_uids = HashMap::new();
    let mut outcomes = Vec::with_capacity(plan.order.nodes.len());
    for node in &plan.order.nodes {
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
        outcomes.push(outcome);
    }
    let root = outcomes
        .iter()
        .find(|outcome| same_identity(&outcome.card_ref, &plan.root.root))
        .map(|outcome| outcome.card_ref.clone())
        .ok_or_else(|| WyrdError::internal("root registration outcome is missing"))?;
    let response = CreateCardResponse {
        root,
        outcomes,
        upload_plans: Vec::new(),
    };
    let seed = replay_seed(&response, &plan.submissions)?;
    commit_registration_operation(&mut conn, operation_id, &seed).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok((operation_id, seed))
}

/// Resolve one node's version, deduplicate when possible, and persist when fresh.
async fn persist_node(
    conn: &mut wyrd_sql::TenantConn<'_>,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    submission: &mut CardSubmission,
) -> Result<wyrd_spec::registry::CardRegistrationOutcome, WyrdError> {
    let mut card = submission_card(submission)?;
    let spec_hash = card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    let artifact_hash = artifact_manifest_hash(&submission.artifacts)?;
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
        append_registration_audit(conn, caller, &existing.row.card_uid).await?;
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
    insert_artifact_manifest_rows(conn, &row.card_uid, &submission.artifacts).await?;
    if matches!(
        card.kind,
        wyrd_spec::envelope::CardKind::Service | wyrd_spec::envelope::CardKind::Agent
    ) {
        upsert_service_account_from_card(conn, &row.card_uid, &card, &caller.principal).await?;
    }
    append_registration_audit(conn, caller, &row.card_uid).await?;
    Ok(outcome_row_to_response(
        &row,
        RegistrationOutcomeKind::Registered,
    ))
}

/// Return an identical existing row or pin the fresh resolved version on the card.
async fn resolve_existing(
    conn: &mut wyrd_sql::TenantConn<'_>,
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

/// Append the required per-node audit event on the caller's transaction.
async fn append_registration_audit(
    conn: &mut wyrd_sql::TenantConn<'_>,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<(), WyrdError> {
    let event = audit_event(
        caller,
        "card.registration.create",
        &format!("card:{card_uid}"),
        "card:write",
        AuditDecision::Allow,
        AuditResult::Success,
        "card registration persisted",
    );
    append_on(conn, &event).await
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
        let computed = artifact_manifest_hash(&submission.artifacts)?;
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
        .map(|submission| artifact_manifest_hash(&submission.artifacts))
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

/// Redact database failures at the public registry boundary.
fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(%error, "card registration database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

/// Register a composite card request through the server-owned transaction.
pub async fn register_card(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    compose_registration(state, caller, idempotency_key, request).await
}

/// Complete one pending Card after its storage uploads have been verified.
///
/// Storage IO runs before the final short SQL transaction. The transition is
/// therefore safe to retry without holding a database connection across
/// backend calls.
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.complete"))]
pub async fn complete_card(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<CardRegistrationOutcome, WyrdError> {
    let (card, manifests) = load_completion_state(state, caller, card_uid).await?;
    if card.status == wyrd_sql::row_types::cards::CardStatus::Active {
        return Ok(existing_row_to_response(
            &card,
            RegistrationOutcomeKind::IdempotentNoop,
        ));
    }
    if card.status != wyrd_sql::row_types::cards::CardStatus::Pending {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "card registration is no longer pending".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid, "status": card.status }),
        });
    }

    for manifest in &manifests {
        verify_manifest_storage(state, caller, card_uid, manifest).await?;
    }

    let blob_uri = if card.card_blob_uri.is_some() {
        card.card_blob_uri
            .clone()
            .ok_or_else(|| WyrdError::internal("card blob URI disappeared during completion"))?
    } else {
        write_card_blob(state, caller, &card).await?
    };

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    for manifest in &manifests {
        if let Some(upload_id) = manifest.upload_id {
            multipart_uploads::mark_completed_if_pending(&mut conn, upload_id)
                .await
                .map_err(|error| WyrdError::registry_unavailable(error.to_string()))?;
            let _ =
                wyrd_sql::queries::cards::mark_manifest_verified(&mut conn, card_uid, upload_id)
                    .await?;
        }
    }
    // Metadata-only cards still need the blob URI recorded before activation.
    if card.card_blob_uri.is_none() {
        record_card_blob(&mut conn, card_uid, &blob_uri).await?;
    }
    let activated = activate_card(&mut conn, card_uid).await?;
    if activated {
        let event = audit_event(
            caller,
            "card.registration.complete",
            &format!("card:{card_uid}"),
            "card:write",
            AuditDecision::Allow,
            AuditResult::Success,
            "card registration completed and activated",
        );
        append_on(&mut conn, &event).await?;
    }
    conn.commit().await.map_err(registry_db_error)?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let card = get_card_by_uid(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(existing_row_to_response(
        &card,
        if activated {
            RegistrationOutcomeKind::Registered
        } else {
            RegistrationOutcomeKind::IdempotentNoop
        },
    ))
}

/// Abort one pending Card and clean its upload/object state.
///
/// This is an internal compensation seam. It is intentionally not exposed as
/// a method on the public `Cards` handle.
#[tracing::instrument(skip(state, caller), fields(operation = "card.registration.abort"))]
pub async fn abort_card(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<CardRegistrationOutcome, WyrdError> {
    let (card, manifests) = load_completion_state(state, caller, card_uid).await?;
    if card.status == wyrd_sql::row_types::cards::CardStatus::Active {
        return Err(WyrdError::Conflict {
            message: "active cards cannot be aborted".to_owned(),
            details: serde_json::json!({ "card_uid": card_uid }),
        });
    }
    if card.status != wyrd_sql::row_types::cards::CardStatus::Pending {
        return Ok(existing_row_to_response(
            &card,
            RegistrationOutcomeKind::IdempotentNoop,
        ));
    }

    let mut cleanup_failures = Vec::new();
    for manifest in manifests {
        let Some(upload_id) = manifest.upload_id else {
            continue;
        };
        let upload_id = UploadId::from_uuid(upload_id);
        match manifest.storage_status.as_deref() {
            Some("completed") => {
                if let Some(path) = manifest.storage_path.as_deref() {
                    match tenant_path::validate(path, caller.data_tenant_id) {
                        Ok(validated) => {
                            if let Err(error) = state.storage.delete_object(&validated).await {
                                cleanup_failures.push(error.to_string());
                            }
                        }
                        Err(error) => cleanup_failures.push(error.to_string()),
                    }
                }
            }
            Some("pending") | Some("initiating") => {
                if let Err(error) = upload_abort(
                    &state.storage,
                    state.postgres.wyrd(),
                    &storage_caller(caller),
                    upload_id,
                )
                .await
                {
                    cleanup_failures.push(error.to_string());
                }
            }
            _ => {}
        }
    }

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let failed = fail_card(&mut conn, card_uid).await?;
    if failed {
        let detail = if cleanup_failures.is_empty() {
            "card registration compensation completed"
        } else {
            "card registration compensation completed with cleanup failures"
        };
        let event = audit_event(
            caller,
            "card.registration.abort",
            &format!("card:{card_uid}"),
            "card:write",
            AuditDecision::Allow,
            if cleanup_failures.is_empty() {
                AuditResult::Success
            } else {
                AuditResult::Failure
            },
            detail,
        );
        append_on(&mut conn, &event).await?;
    }
    conn.commit().await.map_err(registry_db_error)?;

    if !cleanup_failures.is_empty() {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "card registration cleanup was incomplete".to_owned(),
            details: serde_json::json!({
                "card_uid": card_uid,
                "failure_count": cleanup_failures.len(),
            }),
        });
    }

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let card = get_card_by_uid(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(existing_row_to_response(
        &card,
        RegistrationOutcomeKind::IdempotentNoop,
    ))
}

/// Load the parsed Card and tenant-bound manifest/upload rows for completion.
async fn load_completion_state(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
) -> Result<
    (
        wyrd_sql::row_types::cards::ParsedCardRow,
        Vec<wyrd_sql::queries::cards::CardManifestCompletionRow>,
    ),
    WyrdError,
> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let card = get_card_by_uid(&mut conn, card_uid).await?;
    let manifests = manifest_completion_rows(&mut conn, card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok((card, manifests))
}

/// Verify one manifest/upload binding against the configured storage backend.
async fn verify_manifest_storage(
    state: &AppState,
    caller: &Caller,
    card_uid: &CardUid,
    manifest: &wyrd_sql::queries::cards::CardManifestCompletionRow,
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

/// Persist the immutable resolved Card envelope and return its stable URI.
async fn write_card_blob(
    state: &AppState,
    caller: &Caller,
    card: &wyrd_sql::row_types::cards::ParsedCardRow,
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
        relationships: relationships_from_spec(&card.spec),
        status: None,
    };
    let bytes = serde_jcs::to_vec(&envelope).map_err(WyrdError::from_spec_serialization)?;
    if let Err(error) = state.storage.put_object(&validated, bytes).await {
        let mut conn = state
            .postgres
            .tenant_conn(caller.data_tenant_id)
            .await
            .map_err(registry_db_error)?;
        record_blob_failure(&mut conn, &card.card_uid).await?;
        conn.commit().await.map_err(registry_db_error)?;
        return Err(map_storage_error(error));
    }
    Ok(format!("wyrd://{blob_path}"))
}

/// Convert a server-tier storage error to the shared Wyrd error catalog.
fn map_storage_error(error: StorageError) -> WyrdError {
    wyrd_spec::error::storage::WyrdStorageError::from(error).into()
}

#[cfg(test)]
mod tests {
    use super::{card_upload_entry, hash_request, validate_request};
    use uuid::Uuid;
    use wyrd_spec::registry::CreateCardRequest;
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
}

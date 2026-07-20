//! Card registration service orchestration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use tokio::sync::Semaphore;
use uuid::Uuid;
use wyrd_semver::VersionSpec;
use wyrd_spec::envelope::{Card, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::{
    GraphError, RootPick, TopoOrder, build, canonical_order, graph_ready_submissions, pick_root,
    topo_sort,
};
use wyrd_spec::ids::CardUid;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::reference::{CardRef, CardRefIdentity};
use wyrd_spec::registry::{
    CardSubmission, CardUploadPlan, CreateCardRequest, CreateCardResponse, HttpMethod,
    PresignedUpload, RegistrationOperationId, RegistrationOutcomeKind, RegistrationReplaySeed,
    RelativeArtifactPath,
};
use wyrd_spec::storage::{UploadInitRequest, UploadPlan};
use wyrd_spec::vala::api::{AuditDecision, AuditResult};
use wyrd_sql::CardStatus;
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, Resolution, SubmittedCardIdentity,
    artifact_manifest_hash, commit_registration_operation, find_card_by_ref,
    insert_artifact_manifest_rows, insert_card_row, insert_registration_operation,
    lock_version_line, lookup_existing_operation, lookup_expired_operation, manifest_rows_for_init,
    mark_manifest_upload_initialized, registration_request_hash, resolve_version,
    upsert_service_account_from_card,
};
use wyrd_storage::service::upload_init;

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
    Ok(response)
}

/// Initialize every pending manifest row for one card, omitting failed entries.
async fn initialize_card_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    card_uid: &CardUid,
    allowed_paths: &[String],
) -> Vec<PresignedUpload> {
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
) -> Result<PresignedUpload, WyrdError> {
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
    let entry = presigned_upload(&row.relative_path, initialized.plan)?;
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

/// Project a storage single-PUT plan into the registry upload contract.
fn presigned_upload(relative_path: &str, plan: UploadPlan) -> Result<PresignedUpload, WyrdError> {
    let (url, ttl_secs, required_headers) = match plan {
        UploadPlan::SinglePut {
            put_url,
            ttl_secs,
            required_headers,
        } => (
            put_url,
            ttl_secs,
            required_headers
                .into_iter()
                .map(|header| (header.name, header.value))
                .collect(),
        ),
        UploadPlan::LocalFs { put_url, ttl_secs } => (put_url, ttl_secs, Vec::new()),
        UploadPlan::S3Multipart { .. }
        | UploadPlan::GcsResumable { .. }
        | UploadPlan::AzureBlockBlob { .. } => {
            return Err(WyrdError::registry_unavailable(
                "card artifact registration requires a single PUT upload plan",
            ));
        }
    };
    Ok(PresignedUpload {
        relative_path: RelativeArtifactPath::new(relative_path).map_err(WyrdError::from)?,
        url: url::Url::parse(&url).map_err(|error| {
            WyrdError::internal(format!("storage returned invalid URL: {error}"))
        })?,
        method: HttpMethod::Put,
        expires_at: Utc::now() + Duration::seconds(i64::from(ttl_secs)),
        required_headers,
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

#[cfg(test)]
mod tests {
    use super::{hash_request, validate_request};
    use wyrd_spec::registry::CreateCardRequest;

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
}

//! Composite card-registration transaction orchestration.

use std::collections::HashMap;

use chrono::{Duration, Utc};
use uuid::Uuid;
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::envelope::{Card, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::{
    GraphError, RootPick, TopoOrder, build, canonical_order, pick_root, topo_sort,
};
use wyrd_spec::ids::CardUid;
use wyrd_spec::ids::IdempotencyKey;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{
    CardSubmission, CardUploadPlan, CreateCardRequest, CreateCardResponse, HttpMethod,
    PresignedUpload, RegistrationOperationId, RegistrationOutcomeKind, RelativeArtifactPath,
};
use wyrd_spec::storage::{UploadInitRequest, UploadPlan};
use wyrd_spec::vala::api::{AuditDecision, AuditResult};
use wyrd_sql::CardStatus;
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, Resolution, SubmittedCardIdentity,
    artifact_manifest_hash, commit_registration_operation, find_card_by_ref,
    insert_artifact_manifest_rows, insert_card_row, insert_registration_operation,
    lock_version_line, lookup_existing_operation, manifest_rows_for_init,
    mark_manifest_upload_initialized, registration_request_hash, resolve_version,
    upsert_service_account_from_card,
};
use wyrd_storage::service::upload_init;

use crate::audit::{append_on, audit_event, record_audit};
use crate::components::auth::Caller;
use crate::components::cards::mapping::{existing_row_to_response, outcome_row_to_response};
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
pub async fn compose_registration(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    validate_request(&request)?;
    let request_hash = hash_request(&request);
    if let Some((operation_id, response)) =
        replay(state, caller, idempotency_key, &request_hash).await?
    {
        return initialize_uploads(state, caller, operation_id, response).await;
    }
    let external_refs = resolve_external(state, caller, &request.submissions).await?;
    let plan = plan_registration(request, request_hash, external_refs)?;
    let (operation_id, response) = write_registration(state, caller, idempotency_key, plan).await?;
    initialize_uploads(state, caller, operation_id, response).await
}

/// Return a committed response for an identical idempotency key.
async fn replay(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<(RegistrationOperationId, CreateCardResponse)>, WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let operation =
        lookup_existing_operation(&mut conn, caller.principal.id, idempotency_key).await?;
    conn.commit().await.map_err(registry_db_error)?;
    operation
        .map(|operation| {
            let operation_id = RegistrationOperationId::new(operation.operation_id);
            replay_operation(operation, request_hash, idempotency_key)
                .map(|response| (operation_id, response))
        })
        .transpose()
}

/// Validate and decode one persisted operation response.
fn replay_operation(
    operation: wyrd_sql::queries::cards::CardRegistrationOperationRow,
    request_hash: &str,
    idempotency_key: &str,
) -> Result<CreateCardResponse, WyrdError> {
    if operation.request_hash != request_hash {
        return Err(idempotency_conflict(idempotency_key));
    }
    let stored = operation
        .stored_response
        .ok_or_else(|| WyrdError::registry_unavailable("card registration is still pending"))?;
    let mut response: CreateCardResponse = serde_json::from_value(stored)
        .map_err(|error| registry_db_error(format!("invalid stored response: {error}")))?;
    for outcome in &mut response.outcomes {
        outcome.outcome = RegistrationOutcomeKind::IdempotentNoop;
    }
    Ok(response)
}

/// Initialize pending artifact uploads after the composite transaction commits.
async fn initialize_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    mut response: CreateCardResponse,
) -> Result<CreateCardResponse, WyrdError> {
    let mut plans = Vec::new();
    for outcome in &response.outcomes {
        let Some(card_uid) = outcome.card_ref.uid.as_ref() else {
            continue;
        };
        let entries = initialize_card_uploads(state, caller, operation_id, card_uid).await;
        if !entries.is_empty() {
            plans.push(CardUploadPlan {
                card_ref: outcome.card_ref.clone(),
                entries,
            });
        }
    }
    response.upload_plans = plans;
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    commit_registration_operation(&mut conn, operation_id, &response).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok(response)
}

/// Initialize every pending manifest row for one card, omitting failed entries.
async fn initialize_card_uploads(
    state: &AppState,
    caller: &Caller,
    operation_id: RegistrationOperationId,
    card_uid: &CardUid,
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
    let initialized = upload_init(
        &state.storage,
        state.postgres.wyrd(),
        &storage_caller(caller),
        Some(key),
        UploadInitRequest {
            card_uid: card_uid.clone(),
            relative_path: row.relative_path.clone(),
            expected_sha256: row.expected_sha256.clone(),
            expected_size_bytes: u64::try_from(row.expected_size_bytes)
                .map_err(|_| WyrdError::internal("manifest size became negative"))?,
            content_type: row.content_type.clone(),
        },
    )
    .await?;
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
    let graph_submissions = graph_ready_submissions(&request.submissions)?;
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
async fn write_registration(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    plan: RegistrationPlan,
) -> Result<(RegistrationOperationId, CreateCardResponse), WyrdError> {
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
        return replay(state, caller, idempotency_key, &plan.request_hash)
            .await?
            .ok_or_else(|| WyrdError::registry_unavailable("idempotency winner disappeared"));
    }

    let mut sibling_uids = HashMap::new();
    let mut outcomes = Vec::with_capacity(plan.order.nodes.len());
    for node in &plan.order.nodes {
        let authored = find_submission(&plan.submissions, &node.card_ref)?;
        let mut submission = authored.clone();
        bind_card_references(&mut submission.spec, &plan.external_refs, &sibling_uids)?;
        let outcome = persist_node(&mut conn, caller, operation_id, &mut submission).await?;
        sibling_uids.insert(
            identity_key(&outcome.card_ref),
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
    commit_registration_operation(&mut conn, operation_id, &response).await?;
    conn.commit().await.map_err(registry_db_error)?;
    Ok((operation_id, response))
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
    let artifact_hash = artifact_manifest_hash(&submission.artifacts);
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
        let computed = artifact_manifest_hash(&submission.artifacts);
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
fn hash_request(request: &CreateCardRequest) -> String {
    let canonical = canonical_order(&request.submissions)
        .into_iter()
        .map(|index| &request.submissions[index])
        .collect::<Vec<_>>();
    let hashes = canonical
        .iter()
        .map(|submission| artifact_manifest_hash(&submission.artifacts))
        .collect::<Vec<_>>();
    registration_request_hash(&canonical, &hashes)
}

/// Supply concrete placeholder versions only to the pure graph builder.
fn graph_ready_submissions(
    submissions: &[CardSubmission],
) -> Result<Vec<CardSubmission>, WyrdError> {
    let placeholder = VersionBlock::parse("0.0.0")
        .map_err(|error| WyrdError::internal(format!("invalid graph placeholder: {error}")))?;
    submissions
        .iter()
        .cloned()
        .map(|mut submission| {
            if submission.metadata.space.is_none() {
                return Err(WyrdError::registry_invalid_card_spec(
                    "metadata.space is required",
                ));
            }
            if !submission
                .metadata
                .version
                .as_ref()
                .is_some_and(VersionSpec::is_pin)
            {
                submission.metadata.version = Some(VersionSpec::Pin(placeholder.clone()));
            }
            Ok(submission)
        })
        .collect()
}

/// Decode one submission into the typed card envelope persisted by SQL.
fn submission_card(submission: &CardSubmission) -> Result<Card, WyrdError> {
    let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(Card {
        api_version: submission.api_version.clone(),
        kind: submission.kind.clone(),
        metadata: submission.metadata.clone(),
        spec,
        relationships: Relationships {
            outbound: Vec::new(),
            inbound: Vec::new(),
        },
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
                && submission.metadata.space.as_ref() == Some(&node.space)
        })
        .ok_or_else(|| WyrdError::internal("graph node lost its submission"))
}

/// Build the version-independent sibling identity key.
fn identity_key(card_ref: &CardRef) -> (String, String, String) {
    (
        card_ref.kind.wire_name().to_owned(),
        card_ref.space.as_str().to_owned(),
        card_ref.name.as_str().to_owned(),
    )
}

/// Compare two graph identities without server-derived UID or version.
fn same_identity(left: &CardRef, right: &CardRef) -> bool {
    left.kind == right.kind && left.space == right.space && left.name == right.name
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

#[cfg(test)]
mod tests {
    use super::{hash_request, validate_request};
    use wyrd_spec::registry::CreateCardRequest;

    /// Decode a compact registration request used by pure composition tests.
    fn request(value: serde_json::Value) -> CreateCardRequest {
        serde_json::from_value(value).expect("registration fixture must deserialize")
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

        assert_eq!(hash_request(&first), hash_request(&second));
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
        .expect_err("mismatched manifest hash must be rejected");

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
        .expect_err("non-sole artifact manifest must be rejected");

        assert_eq!(
            error.code(),
            "WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION"
        );
        assert_eq!(error.status(), 400);
    }
}

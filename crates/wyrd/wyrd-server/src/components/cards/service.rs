//! Server-owned card registration orchestration.

use std::collections::HashSet;
use uuid::Uuid;
use wyrd_runtime::PrincipalId;
use wyrd_semver::VersionSpec;
use wyrd_spec::envelope::{Card, SpecHash};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::registry::{
    ArtifactManifestEntry, CreateCardRequest, CreateCardResponse, RegisterOutcome,
    RegistrationOperationId,
};
use wyrd_sql::queries::cards::{
    CardRegistrationOperationRow, NewCardRow, NewRegistrationOperation, RegisteredCardRow,
    Resolution, SubmittedCardIdentity, artifact_manifest_hash, insert_artifact_manifest_rows,
    insert_card_row, insert_registration_operation, lock_version_line, lookup_existing_operation,
    manifest_rows_for_init, registration_request_hash, resolve_version,
};
use wyrd_sql::row_types::cards::{CardStatus, ParsedCardRow};

use crate::audit::{append_on, audit_event};
use crate::components::auth::Caller;
use crate::components::cards::mapping;
use crate::components::cards::post_commit::drive_post_commit_inits;
use crate::components::cards::resolve::resolve_submission;
use crate::state::AppState;

/// Register one card and initialize artifact uploads after the registry commit.
pub async fn register_card(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    let PreparedRegistration {
        mut request,
        identity,
        spec_hash,
        artifact_hash,
        request_hash,
    } = prepare_registration(state, caller, request).await?;
    let principal_id = caller.principal.id;
    let mut conn = open_registry_connection(state, caller).await?;

    if let Some(operation) =
        lookup_existing_operation(&mut conn, principal_id, idempotency_key).await?
    {
        return replay_registration(
            state,
            caller,
            conn,
            operation,
            &request_hash,
            idempotency_key,
        )
        .await;
    }

    let ResolvedRegistration {
        existing,
        deduplicated,
        pinned_noop,
    } = resolve_registration(
        &mut conn,
        &mut request,
        &identity,
        spec_hash.as_str(),
        artifact_hash.as_deref(),
    )
    .await?;
    let (outcome, response_outcome) = registration_outcome(deduplicated, pinned_noop);
    let operation_id = RegistrationOperationId::new(Uuid::now_v7());
    let card_uid = existing
        .as_ref()
        .map(|card| card.card_uid.clone())
        .unwrap_or(CardUid::from_uuid(Uuid::now_v7()).map_err(WyrdError::from_card_uid_error)?);
    let status = existing.as_ref().map_or(
        if request.artifacts.is_empty() {
            CardStatus::Active
        } else {
            CardStatus::Pending
        },
        |card| card.status,
    );
    let operation = NewRegistrationOperation {
        operation_id,
        principal_id,
        idempotency_key,
        request_hash: &request_hash,
        card_uid: &card_uid,
        upload_plans: &serde_json::json!([]),
        outcome,
        status: status.as_db_str(),
    };

    if !insert_registration_operation(&mut conn, operation).await? {
        let winner = lookup_existing_operation(&mut conn, principal_id, idempotency_key)
            .await?
            .ok_or_else(|| WyrdError::registry_unavailable("idempotency winner disappeared"))?;
        return replay_registration(state, caller, conn, winner, &request_hash, idempotency_key)
            .await;
    }

    let row = if let Some(existing) = existing {
        RegisteredCardRow {
            card_uid: existing.card_uid,
            kind: existing.kind,
            space: existing.space,
            name: existing.name,
            version: existing.version,
            spec_hash: existing.spec_hash,
            artifact_hash: existing.artifact_hash,
            status: existing.status,
            created_at: existing.created_at,
            principal_id,
            operation_id,
        }
    } else {
        let card = build_card(&request)?;
        let row = insert_card_row(
            &mut conn,
            NewCardRow {
                card: &card,
                card_uid,
                principal_id,
                operation_id,
                status,
                spec_hash: spec_hash.as_str(),
                artifact_hash: artifact_hash.as_deref(),
            },
        )
        .await?;
        insert_artifact_manifest_rows(&mut conn, &row.card_uid, &request.artifacts).await?;
        row
    };
    // Registration audit is intentionally appended on this same tenant
    // transaction. Global middleware can record request authentication, but it
    // cannot atomically join the registry transaction or include the resolved
    // card UID and outcome.
    append_on(
        &mut conn,
        &audit_event(
            caller,
            "card.registration.create",
            &format!("card:{}", row.card_uid),
            "card:write",
            wyrd_spec::vala::api::AuditDecision::Allow,
            wyrd_spec::vala::api::AuditResult::Success,
            "card registration",
        ),
    )
    .await?;
    let manifests = manifest_rows_for_init(&mut conn, &row.card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;

    let operation_row = operation_row_for_response(
        &row,
        caller.data_tenant_id.as_uuid(),
        request_hash,
        idempotency_key,
        outcome,
    );
    let response = mapping::registered_row_to_response(&row, response_outcome, Vec::new());
    drive_post_commit_inits(state, caller, operation_row, response, manifests).await
}

/// Reject duplicate artifact paths before any database or storage work begins.
fn validate_manifest(artifacts: &[ArtifactManifestEntry]) -> Result<(), WyrdError> {
    let mut paths = HashSet::with_capacity(artifacts.len());
    for artifact in artifacts {
        let normalized = artifact.relative_path.as_str().to_ascii_lowercase();
        if !paths.insert(normalized) {
            return Err(WyrdError::RegistryInvalidArtifactPath {
                message: "artifact manifest contains duplicate paths".to_owned(),
                details: serde_json::json!({ "path": artifact.relative_path }),
            });
        }
    }
    Ok(())
}

/// Build the server-owned card envelope after version resolution.
///
/// The request is a submission shape, so this helper supplies the resolved
/// version, canonical hashes, and empty server-derived relationships/status
/// fields before the row is inserted.
fn build_card(request: &CreateCardRequest) -> Result<Card, WyrdError> {
    let space = request.card.metadata.space.clone();
    let version = match request.card.metadata.version.clone() {
        Some(VersionSpec::Pin(version)) => version,
        _ => {
            return Err(WyrdError::registry_invalid_version_block(
                "registration requires metadata.version to be a resolved pin",
            ));
        }
    };
    let spec_hash = request
        .card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    let relationships = mapping::relationships_from_row();
    Ok(Card {
        api_version: request.card.api_version.clone(),
        kind: request.card.kind.clone(),
        metadata: wyrd_spec::envelope::Metadata {
            name: request.card.metadata.name.clone(),
            version: Some(VersionSpec::Pin(version)),
            bump: None,
            space: Some(space),
            uid: None,
            labels: request.card.metadata.labels.clone(),
            annotations: request.card.metadata.annotations.clone(),
            spec_hash: Some(spec_hash),
            artifact_hash: artifact_manifest_hash(&request.artifacts),
            origin: None,
        },
        spec: request.card.spec.clone(),
        relationships,
        status: None,
    })
}

/// Project a stored operation and card row into a retry response.
///
/// `outcome` is supplied by the caller because replay is deliberately exposed
/// as `IdempotentNoop` even when the original operation was `Created` or
/// `Deduplicated`; the stored operation itself is never mutated.
async fn replay_response(
    conn: &mut wyrd_sql::TenantConn<'_>,
    operation: &CardRegistrationOperationRow,
    outcome: RegisterOutcome,
) -> Result<CreateCardResponse, WyrdError> {
    let card = wyrd_sql::queries::cards::get_card_by_uid(
        conn,
        &CardUid::from_uuid(operation.card_uid).map_err(WyrdError::from_card_uid_error)?,
    )
    .await?;
    let row = RegisteredCardRow {
        card_uid: card.card_uid,
        kind: card.kind,
        space: card.space,
        name: card.name,
        version: card.version,
        spec_hash: card.spec_hash,
        artifact_hash: card.artifact_hash,
        status: card.status,
        created_at: card.created_at,
        principal_id: PrincipalId::new(operation.principal_id),
        operation_id: RegistrationOperationId::new(operation.operation_id),
    };
    let uploads = serde_json::from_value(operation.upload_plans.clone()).map_err(|error| {
        tracing::error!(error = %error, "stored card upload plans are invalid");
        WyrdError::registry_unavailable("card registry unavailable")
    })?;
    Ok(mapping::registered_row_to_response(&row, outcome, uploads))
}

/// Log an internal database failure and return the stable registry error.
fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "card registry database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

/// Construct the operation row used by post-commit replay and persistence.
fn operation_row_for_response(
    row: &RegisteredCardRow,
    data_tenant_id: uuid::Uuid,
    request_hash: String,
    idempotency_key: &str,
    outcome: &str,
) -> CardRegistrationOperationRow {
    CardRegistrationOperationRow {
        operation_id: row.operation_id.as_uuid(),
        data_tenant_id,
        principal_id: row.principal_id.as_uuid(),
        idempotency_key: idempotency_key.to_owned(),
        request_hash,
        card_uid: row.card_uid.as_uuid(),
        upload_plans: serde_json::json!([]),
        outcome: outcome.to_owned(),
        status: row.status.as_db_str().to_owned(),
        created_at: row.created_at,
        updated_at: row.created_at,
    }
}

/// Values computed before the registration transaction begins.
struct PreparedRegistration {
    /// Submission after reference resolution.
    request: CreateCardRequest,
    /// Required parent-card identity used by every registry stage.
    identity: RegistrationIdentity,
    /// Canonical hash of the typed spec.
    spec_hash: SpecHash,
    /// Canonical hash of the artifact manifest, when one was supplied.
    artifact_hash: Option<String>,
    /// Hash used to bind an idempotency key to request content.
    request_hash: String,
}

/// Required identity fields for the card being registered.
struct RegistrationIdentity {
    /// Parent card workspace.
    space: SpaceName,
    /// Parent card name.
    name: CardName,
}

/// Results of version resolution and an exact-version lookup.
struct ResolvedRegistration {
    /// Existing row when the request points at an existing version.
    existing: Option<ParsedCardRow>,
    /// Whether auto-version resolution found identical active content.
    deduplicated: bool,
    /// Whether an exact pinned request matches the stored content.
    pinned_noop: bool,
}

/// Validate and normalize a request before opening the write transaction.
///
/// Child card references are resolved in a short tenant transaction. The
/// resulting spec and artifact manifest hashes are then used for idempotency
/// and content comparison in the write transaction.
async fn prepare_registration(
    state: &AppState,
    caller: &Caller,
    mut request: CreateCardRequest,
) -> Result<PreparedRegistration, WyrdError> {
    validate_manifest(&request.artifacts)?;
    let mut conn = open_registry_connection(state, caller).await?;
    request.card = resolve_submission(&mut conn, request.card).await?;
    let spec_hash = request
        .card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    let artifact_hash = artifact_manifest_hash(&request.artifacts);
    let request_hash = registration_request_hash(spec_hash.as_str(), artifact_hash.as_deref());
    let identity = RegistrationIdentity {
        space: request.card.metadata.space.clone(),
        name: request.card.metadata.name.clone(),
    };
    conn.commit().await.map_err(registry_db_error)?;
    Ok(PreparedRegistration {
        request,
        identity,
        spec_hash,
        artifact_hash,
        request_hash,
    })
}

/// Acquire the tenant-bound connection used for registry reads and writes.
async fn open_registry_connection<'a>(
    state: &'a AppState,
    caller: &Caller,
) -> Result<wyrd_sql::TenantConn<'a>, WyrdError> {
    state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)
}

/// Resolve the requested version and load the exact row, if it already exists.
///
/// Auto and range requests can deduplicate against the latest active row when
/// both spec and artifact hashes match. Pinned requests never auto-advance, so
/// an existing row is checked for immutable-content drift instead.
async fn resolve_registration(
    conn: &mut wyrd_sql::TenantConn<'_>,
    request: &mut CreateCardRequest,
    identity: &RegistrationIdentity,
    spec_hash: &str,
    artifact_hash: Option<&str>,
) -> Result<ResolvedRegistration, WyrdError> {
    lock_version_line(
        conn,
        request.card.kind.clone(),
        &identity.space,
        &identity.name,
    )
    .await?;
    let resolution = match request.card.metadata.version.as_ref() {
        Some(VersionSpec::Pin(version)) => Resolution::Fresh(version.clone()),
        version => {
            resolve_version(
                conn,
                request.card.kind.clone(),
                &identity.space,
                &identity.name,
                version,
                request.card.metadata.bump.as_ref(),
                SubmittedCardIdentity {
                    spec_hash,
                    artifact_hash,
                },
            )
            .await?
        }
    };
    let deduplicated = matches!(resolution, Resolution::Deduplicated { .. });
    let resolved_version = match resolution {
        Resolution::Fresh(version) | Resolution::Deduplicated { version } => version,
    };
    request.card.metadata.version = Some(VersionSpec::Pin(resolved_version));
    let version = match request.card.metadata.version.as_ref() {
        Some(VersionSpec::Pin(version)) => version,
        _ => unreachable!("version resolution always produces a pin"),
    };
    let existing = wyrd_sql::queries::cards::find_card_by_ref(
        conn,
        request.card.kind.clone(),
        &identity.space,
        &identity.name,
        version,
    )
    .await?;
    let pinned_noop = existing.as_ref().is_some_and(|card| {
        !deduplicated
            && card.spec_hash == spec_hash
            && card.artifact_hash.as_deref() == artifact_hash
    });
    validate_existing_card(
        existing.as_ref(),
        deduplicated,
        pinned_noop,
        spec_hash,
        artifact_hash,
    )?;
    Ok(ResolvedRegistration {
        existing,
        deduplicated,
        pinned_noop,
    })
}

/// Reject a pinned registration that would mutate an immutable card version.
fn validate_existing_card(
    existing: Option<&ParsedCardRow>,
    deduplicated: bool,
    pinned_noop: bool,
    spec_hash: &str,
    artifact_hash: Option<&str>,
) -> Result<(), WyrdError> {
    if let Some(card) = existing
        && !deduplicated
        && !pinned_noop
        && (card.spec_hash != spec_hash || card.artifact_hash.as_deref() != artifact_hash)
    {
        return Err(WyrdError::registry_spec_drift(
            card.card_uid.to_string(),
            &card.spec_hash,
            spec_hash,
        ));
    }
    Ok(())
}

/// Map internal registration state to its stored and wire outcomes.
fn registration_outcome(deduplicated: bool, pinned_noop: bool) -> (&'static str, RegisterOutcome) {
    if deduplicated {
        ("deduplicated", RegisterOutcome::Deduplicated)
    } else if pinned_noop {
        ("idempotent_noop", RegisterOutcome::IdempotentNoop)
    } else {
        ("created", RegisterOutcome::Created)
    }
}

/// Replay an operation after checking that its key still names this request.
///
/// This handles both an operation found before version resolution and a
/// concurrent insert race. The stored response is projected without changing
/// the operation's original outcome, then post-commit upload initialization is
/// resumed from the manifest table.
async fn replay_registration(
    state: &AppState,
    caller: &Caller,
    mut conn: wyrd_sql::TenantConn<'_>,
    operation: CardRegistrationOperationRow,
    request_hash: &str,
    idempotency_key: &str,
) -> Result<CreateCardResponse, WyrdError> {
    if operation.request_hash != request_hash {
        return Err(WyrdError::RegistryIdempotencyConflict {
            message: "idempotency key was reused for different card content".to_owned(),
            details: serde_json::json!({ "idempotency_key": idempotency_key }),
        });
    }
    let response = replay_response(&mut conn, &operation, RegisterOutcome::IdempotentNoop).await?;
    conn.commit().await.map_err(registry_db_error)?;
    finish_response(state, caller, operation, response).await
}

/// Resume post-commit artifact initialization for a replayed registration.
///
/// The registry transaction has already committed when this runs, so manifest
/// rows are reloaded from Postgres before storage calls are attempted.
async fn finish_response(
    state: &AppState,
    caller: &Caller,
    operation: CardRegistrationOperationRow,
    response: CreateCardResponse,
) -> Result<CreateCardResponse, WyrdError> {
    let card_uid = response.card_uid.clone();
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let manifests = manifest_rows_for_init(&mut conn, &card_uid).await?;
    conn.commit().await.map_err(registry_db_error)?;
    drive_post_commit_inits(state, caller, operation, response, manifests).await
}

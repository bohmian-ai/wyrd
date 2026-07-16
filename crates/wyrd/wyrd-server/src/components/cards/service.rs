//! Server-owned card registration orchestration.

use std::collections::HashSet;
use uuid::Uuid;
use wyrd_runtime::PrincipalId;
use wyrd_semver::VersionSpec;
use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
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
use wyrd_sql::row_types::cards::CardStatus;

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
    mut request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    validate_manifest(&request.artifacts)?;

    let mut resolve_conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    request.card = resolve_submission(&mut resolve_conn, request.card).await?;
    let spec_hash = request
        .card
        .spec
        .canonical_hash()
        .map_err(WyrdError::from_spec_canonicalization)?;
    let artifact_hash = artifact_manifest_hash(&request.artifacts);
    let request_hash = registration_request_hash(spec_hash.as_str(), artifact_hash.as_deref());
    let principal_id = caller.principal.id;
    resolve_conn.commit().await.map_err(registry_db_error)?;

    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;

    if let Some(operation) =
        lookup_existing_operation(&mut conn, principal_id, idempotency_key).await?
    {
        if operation.request_hash != request_hash {
            return Err(WyrdError::RegistryIdempotencyConflict {
                message: "idempotency key was reused for different card content".to_owned(),
                details: serde_json::json!({ "idempotency_key": idempotency_key }),
            });
        }
        let response =
            replay_response(&mut conn, &operation, RegisterOutcome::IdempotentNoop).await?;
        conn.commit().await.map_err(registry_db_error)?;
        return finish_response(state, caller, operation, response).await;
    }

    let space = request
        .card
        .metadata
        .space
        .as_ref()
        .ok_or_else(|| WyrdError::registry_invalid_card_spec("metadata.space is required"))?;
    lock_version_line(
        &mut conn,
        request.card.kind.clone(),
        space,
        &request.card.metadata.name,
    )
    .await?;
    let resolution = match request.card.metadata.version.as_ref() {
        Some(VersionSpec::Pin(version)) => Resolution::Fresh(version.clone()),
        version => {
            resolve_version(
                &mut conn,
                request.card.kind.clone(),
                space,
                &request.card.metadata.name,
                version,
                request.card.metadata.bump.as_ref(),
                SubmittedCardIdentity {
                    spec_hash: spec_hash.as_str(),
                    artifact_hash: artifact_hash.as_deref(),
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
    let existing = if deduplicated
        || request
            .card
            .metadata
            .version
            .as_ref()
            .is_some_and(VersionSpec::is_pin)
    {
        let version = match request.card.metadata.version.as_ref() {
            Some(VersionSpec::Pin(version)) => version,
            _ => unreachable!("resolved registration version is pinned"),
        };
        wyrd_sql::queries::cards::find_card_by_ref(
            &mut conn,
            request.card.kind.clone(),
            space,
            &request.card.metadata.name,
            version,
        )
        .await?
    } else {
        None
    };
    let pinned_noop = existing.as_ref().is_some_and(|card| {
        !deduplicated
            && card.spec_hash == spec_hash.as_str()
            && card.artifact_hash.as_deref() == artifact_hash.as_deref()
    });
    if let Some(card) = existing.as_ref()
        && !deduplicated
        && !pinned_noop
        && (card.spec_hash != spec_hash.as_str()
            || card.artifact_hash.as_deref() != artifact_hash.as_deref())
    {
        return Err(WyrdError::registry_spec_drift(
            card.card_uid.to_string(),
            &card.spec_hash,
            spec_hash.as_str(),
        ));
    }
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
    let outcome = if deduplicated {
        "deduplicated"
    } else if pinned_noop {
        "idempotent_noop"
    } else {
        "created"
    };
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
        if winner.request_hash != request_hash {
            return Err(WyrdError::RegistryIdempotencyConflict {
                message: "idempotency key was reused for different card content".to_owned(),
                details: serde_json::json!({ "idempotency_key": idempotency_key }),
            });
        }
        let response = replay_response(&mut conn, &winner, RegisterOutcome::IdempotentNoop).await?;
        conn.commit().await.map_err(registry_db_error)?;
        return finish_response(state, caller, winner, response).await;
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
    let response_outcome = if deduplicated {
        RegisterOutcome::Deduplicated
    } else if pinned_noop {
        RegisterOutcome::IdempotentNoop
    } else {
        RegisterOutcome::Created
    };
    let response = mapping::registered_row_to_response(&row, response_outcome, Vec::new());
    drive_post_commit_inits(state, caller, operation_row, response, manifests).await
}

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

fn build_card(request: &CreateCardRequest) -> Result<Card, WyrdError> {
    let space = request
        .card
        .metadata
        .space
        .clone()
        .ok_or_else(|| WyrdError::registry_invalid_card_spec("metadata.space is required"))?;
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

fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "card registry database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

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

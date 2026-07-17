//! Composite card registration orchestration.

use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::envelope::{Card, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::{GraphError, build, canonical_order, pick_root, topo_sort};
use wyrd_spec::graph::{RootPick, TopoOrder};
use wyrd_spec::registry::{
    CardLifecycleStatus, CardSubmission, CreateCardRequest, CreateCardResponse,
    RegistrationOperationId, RegistrationOutcomeKind,
};
use wyrd_spec::vala::api::{AuditDecision, AuditResult};
use wyrd_sql::CardStatus;
use wyrd_sql::queries::cards::{
    NewRegistrationOperation, RegisterCardRequest, get_card_by_uid, insert_artifact_manifest_rows,
    insert_registration_operation, lookup_existing_operation, register_card as insert_card,
};

use crate::audit::{append_on, audit_event};
use crate::components::auth::Caller;
use crate::components::cards::mapping::outcome_row_to_response;
use crate::components::cards::resolve::resolve_card_references;
use crate::state::AppState;

/// Fully validated, deterministically ordered input for the write transaction.
struct RegistrationPlan {
    request: CreateCardRequest,
    request_hash: String,
    order: TopoOrder,
    root: RootPick,
}

/// Register a composite request through replay, resolve, plan, and write stages.
///
/// # Errors
/// Returns a stable registry error when validation, dependency resolution,
/// idempotency, persistence, or audit append fails.
pub async fn register_card(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    validate_request(&request)?;
    let request_hash = request_hash(&request)?;
    if let Some(response) =
        replay_existing_operation(state, caller, idempotency_key, &request_hash).await?
    {
        return Ok(response);
    }
    resolve_dependencies(state, caller, &request.submissions).await?;
    let plan = build_registration_plan(request, request_hash)?;
    execute_registration(state, caller, idempotency_key, plan).await
}

/// Look up and decode a previously committed registration response.
async fn replay_existing_operation(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<CreateCardResponse>, WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let Some(operation) =
        lookup_existing_operation(&mut conn, caller.principal.id, idempotency_key).await?
    else {
        return Ok(None);
    };
    if operation.request_hash != request_hash {
        return Err(WyrdError::RegistryIdempotencyConflict {
            message: "idempotency key was already used for different content".to_owned(),
            details: serde_json::json!({ "idempotency_key": idempotency_key }),
        });
    }
    let mut response: CreateCardResponse =
        serde_json::from_value(operation.upload_plans).map_err(|error| {
            tracing::error!(%error, "stored card registration response is invalid");
            WyrdError::registry_unavailable("card registry unavailable")
        })?;
    for outcome in &mut response.outcomes {
        outcome.outcome = RegistrationOutcomeKind::IdempotentNoop;
    }
    Ok(Some(response))
}

/// Resolve all non-sibling references and close the read transaction.
async fn resolve_dependencies(
    state: &AppState,
    caller: &Caller,
    submissions: &[CardSubmission],
) -> Result<(), WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    resolve_card_references(&mut conn, submissions).await?;
    conn.commit().await.map_err(registry_db_error)
}

/// Build the deterministic leaf-first graph plan without performing I/O.
fn build_registration_plan(
    request: CreateCardRequest,
    request_hash: String,
) -> Result<RegistrationPlan, WyrdError> {
    let graph_submissions = graph_ready_submissions(&request.submissions)?;
    let (nodes, edges) = build(&graph_submissions).map_err(graph_error)?;
    let order = topo_sort(&nodes, &edges).map_err(graph_error)?;
    let root = pick_root(&order).map_err(graph_error)?;
    Ok(RegistrationPlan {
        request,
        request_hash,
        order,
        root,
    })
}

/// Execute all card, manifest, audit, and idempotency writes atomically.
async fn execute_registration(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    plan: RegistrationPlan,
) -> Result<CreateCardResponse, WyrdError> {
    let mut conn = state
        .postgres
        .tenant_conn(caller.data_tenant_id)
        .await
        .map_err(registry_db_error)?;
    let mut outcomes = Vec::with_capacity(plan.order.nodes.len());
    for node in &plan.order.nodes {
        let submission = plan
            .request
            .submissions
            .iter()
            .find(|submission| same_identity(submission, &node.card_ref))
            .ok_or_else(|| WyrdError::internal("topological node lost its submission"))?;
        let card = submission_card(submission)?;
        let status = if submission.artifacts.is_empty() {
            CardStatus::Active
        } else {
            CardStatus::Pending
        };
        let registered = insert_card(
            &mut conn,
            RegisterCardRequest {
                card: &card,
                actor: &caller.principal,
                status,
            },
        )
        .await?;
        insert_artifact_manifest_rows(&mut conn, &registered.card_uid, &submission.artifacts)
            .await?;
        let row = get_card_by_uid(&mut conn, &registered.card_uid).await?;
        let event = audit_event(
            caller,
            "card.registration.create",
            &format!("card:{}", registered.card_uid),
            "card:write",
            AuditDecision::Allow,
            AuditResult::Success,
            "card registration persisted",
        );
        append_on(&mut conn, &event).await?;
        outcomes.push(outcome_row_to_response(&row, registered.kind));
    }

    let root = outcomes
        .iter()
        .find(|outcome| same_ref_identity(&outcome.card_ref, &plan.root.root))
        .map(|outcome| outcome.card_ref.clone())
        .ok_or_else(|| WyrdError::internal("root registration outcome is missing"))?;
    let response = CreateCardResponse {
        root,
        outcomes,
        upload_plans: Vec::new(),
    };
    let stored_response =
        serde_json::to_value(&response).map_err(WyrdError::from_spec_serialization)?;
    let root_uid = response
        .root
        .uid
        .as_ref()
        .ok_or_else(|| WyrdError::internal("registered root is missing its uid"))?;
    let inserted = insert_registration_operation(
        &mut conn,
        NewRegistrationOperation {
            operation_id: RegistrationOperationId::new(uuid::Uuid::now_v7()),
            principal_id: caller.principal.id,
            idempotency_key,
            request_hash: &plan.request_hash,
            card_uid: root_uid,
            upload_plans: &stored_response,
            outcome: "created",
            status: if response
                .outcomes
                .iter()
                .any(|outcome| outcome.status == CardLifecycleStatus::Pending)
            {
                "pending"
            } else {
                "active"
            },
        },
    )
    .await?;
    if !inserted {
        return Err(WyrdError::RegistryIdempotencyConflict {
            message: "idempotency key was claimed by a concurrent request".to_owned(),
            details: serde_json::json!({ "idempotency_key": idempotency_key }),
        });
    }
    conn.commit().await.map_err(registry_db_error)?;
    Ok(response)
}

/// Enforce request-wide invariants that must fail before any database access.
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
    Ok(())
}

/// Hash submissions after deterministic identity ordering and JCS encoding.
fn request_hash(request: &CreateCardRequest) -> Result<String, WyrdError> {
    let canonical = canonical_order(&request.submissions)
        .into_iter()
        .map(|index| &request.submissions[index])
        .collect::<Vec<_>>();
    let bytes = serde_jcs::to_vec(&canonical).map_err(WyrdError::from_spec_serialization)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Clone submissions into graph-ready identities without changing write intent.
fn graph_ready_submissions(
    submissions: &[CardSubmission],
) -> Result<Vec<CardSubmission>, WyrdError> {
    let placeholder = VersionBlock::parse("0.0.0").map_err(|error| {
        WyrdError::internal(format!("graph placeholder version failed: {error}"))
    })?;
    submissions
        .iter()
        .cloned()
        .map(|mut submission| {
            if submission.metadata.space.is_none() {
                return Err(WyrdError::registry_invalid_card_spec(
                    "metadata.space is required at registration",
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

/// Decode a submitted JSON spec into the typed Card used by SQL registration.
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

/// Compare a submission and graph node by their version-independent identity.
fn same_identity(submission: &CardSubmission, card_ref: &wyrd_spec::reference::CardRef) -> bool {
    submission.kind == card_ref.kind
        && submission.metadata.name == card_ref.name
        && submission.metadata.space.as_ref() == Some(&card_ref.space)
}

/// Compare two references by the identity coordinates used for sibling edges.
fn same_ref_identity(
    left: &wyrd_spec::reference::CardRef,
    right: &wyrd_spec::reference::CardRef,
) -> bool {
    left.kind == right.kind && left.name == right.name && left.space == right.space
}

/// Map pure graph failures onto the registry's public error boundary.
fn graph_error(error: GraphError) -> WyrdError {
    match error {
        GraphError::Cycle { cycle } => WyrdError::RegistryDependencyCycle {
            message: "card submission graph contains a dependency cycle".to_owned(),
            details: serde_json::json!({ "cycle": cycle }),
        },
        GraphError::Empty => WyrdError::registry_invalid_card_spec("submissions must not be empty"),
        GraphError::MultipleRoots { candidates } => WyrdError::Internal {
            message: "card submission graph has multiple roots".to_owned(),
            details: serde_json::json!({ "candidates": candidates }),
        },
    }
}

/// Redact a database failure into the stable registry-unavailable response.
fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(%error, "card registration database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

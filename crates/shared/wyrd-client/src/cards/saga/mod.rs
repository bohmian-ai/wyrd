//! Private Card declaration saga.

mod build_submission;
mod complete;
mod hash_artifacts;
mod idempotency;
mod submit;
mod upload;

use wyrd_loader::DeclarationInput;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CardLifecycleStatus, DeclarationReceipt, DeclareCardResponse};

use crate::cards::engine::RegistryEngine;
use crate::cards::error::RegistryEngineError;
use crate::cards::progress::{DeclarationPhase, DeclarationProgressEvent, DeclarationProgressSink};

/// Drive one declarative declaration input through the ordered declaration
/// lifecycle:
///
/// 1. resolve and prepare authored input;
/// 2. mint an idempotency key and submit the flat composite request;
/// 3. upload planned artifacts with bounded concurrency;
/// 4. ask the server to verify and complete uploaded Cards;
/// 5. verify that every outcome is Active.
///
/// If artifact upload or completion fails, the server-owned reconciliation
/// state remains available for cleanup and the original saga error is
/// returned. The client owns orchestration and local transfer calls;
/// `wyrd-server` owns durable manifest, lifecycle, activation, audit, and
/// cleanup state. This saga is not transactionally atomic across those
/// external boundaries.
///
/// The loader owns local source resolution. `WyrdClient` owns authenticated
/// control-plane transport and declaration idempotency. `WyrdStorageClient`
/// owns every artifact transfer detail. `wyrd-server` owns durable manifest,
/// activation, audit, and cleanup state. This module only sequences those
/// operations and maps the final response into a receipt.
///
/// # Errors
///
/// Returns the local preparation errors of `build_submission::prepare`, the
/// declaration submission refusal, artifact transfer failures, completion
/// errors, and `RegistryArtifactVerifyFailed` when a Card does not end Active.
///
/// # Cancellation
///
/// Cancelling after submission can leave Cards declared but pending, with
/// artifacts partially uploaded or some Cards completed; that server-owned
/// state remains available for reconciliation.
pub(crate) async fn declare(
    engine: &RegistryEngine,
    input: &DeclarationInput,
    progress: Option<DeclarationProgressSink>,
) -> Result<DeclarationReceipt, RegistryEngineError> {
    emit_phase(progress.as_ref(), DeclarationPhase::Preparing);
    let prepared = build_submission::prepare(input).await?;
    let idempotency_key = idempotency::mint();
    emit_phase(progress.as_ref(), DeclarationPhase::Submitting);
    let mut response =
        submit::submit_card_declaration(&engine.client, &prepared.request, &idempotency_key)
            .await?;

    emit_phase(progress.as_ref(), DeclarationPhase::Uploading);
    upload::upload_artifacts(
        &engine.storage,
        &response,
        &prepared.artifact_sources,
        &idempotency_key,
        progress.clone(),
    )
    .await?;

    emit_phase(progress.as_ref(), DeclarationPhase::Completing);
    match complete::complete_uploaded_cards(&engine.client, &response, &idempotency_key).await {
        Ok(finalized) => response = finalized,
        Err(error) => {
            return Err(error);
        }
    }
    emit_phase(progress.as_ref(), DeclarationPhase::Verifying);
    ensure_active(&response)?;
    Ok(response.into())
}

/// Forward one phase event without changing the no-progress declaration path.
fn emit_phase(progress: Option<&DeclarationProgressSink>, phase: DeclarationPhase) {
    if let Some(progress) = progress {
        progress(DeclarationProgressEvent::Phase(phase));
    }
}

/// Reject a response that has not reached a server-owned terminal success.
///
/// # Errors
///
/// Returns `RegistryArtifactVerifyFailed` naming the first Card not Active.
fn ensure_active(response: &DeclareCardResponse) -> Result<(), RegistryEngineError> {
    if let Some(pending) = response
        .outcomes
        .iter()
        .find(|outcome| outcome.status != CardLifecycleStatus::Active)
    {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "declaration did not reach Active state".to_owned(),
            details: serde_json::json!({
                "card_uid": pending.card_ref.uid,
                "status": pending.status,
            }),
        }
        .into());
    }
    Ok(())
}

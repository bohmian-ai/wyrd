//! Private Card registration saga.

mod abort;
mod build_submission;
mod complete;
mod hash_artifacts;
mod idempotency;
mod submit;
mod upload;

use wyrd_loader::RegistrationInput;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CardLifecycleStatus, CreateCardResponse, RegistrationReceipt};

use crate::engine::RegistryEngine;
use crate::error::RegistryEngineError;
use crate::progress::{RegistrationPhase, RegistrationProgressEvent, RegistrationProgressSink};

/// Drive one declarative registration input through the ordered registration
/// lifecycle:
///
/// 1. resolve and prepare authored input;
/// 2. mint an idempotency key and submit the flat composite request;
/// 3. upload planned artifacts with bounded concurrency;
/// 4. ask the server to verify and complete uploaded Cards;
/// 5. verify that every outcome is Active.
///
/// If artifact upload or completion fails, pending or failed Cards are aborted
/// on a best-effort basis and the original saga error is returned. The client
/// owns orchestration and local transfer calls; `wyrd-server` owns durable
/// manifest, lifecycle, activation, audit, and cleanup state. This saga is not
/// transactionally atomic across those external boundaries.
///
/// The loader owns local source resolution. `WyrdClient` owns authenticated
/// control-plane transport and registration idempotency. `WyrdStorageClient`
/// owns every artifact transfer detail. `wyrd-server` owns durable manifest,
/// activation, audit, and cleanup state. This module only sequences those
/// operations and maps the final response into a receipt.
pub(crate) async fn register(
    engine: &RegistryEngine,
    input: &RegistrationInput,
    progress: Option<RegistrationProgressSink>,
) -> Result<RegistrationReceipt, RegistryEngineError> {
    emit_phase(&progress, RegistrationPhase::Preparing);
    let prepared = build_submission::prepare(input).await?;
    let idempotency_key = idempotency::mint();
    emit_phase(&progress, RegistrationPhase::Submitting);
    let mut response =
        submit::submit_card_registration(&engine.client, &prepared.request, &idempotency_key)
            .await?;

    emit_phase(&progress, RegistrationPhase::Uploading);
    if let Err(error) = upload::upload_artifacts(
        &engine.storage,
        &response,
        &prepared.artifact_sources,
        &idempotency_key,
        progress.clone(),
    )
    .await
    {
        emit_phase(&progress, RegistrationPhase::CleaningUp);
        abort::abort_pending_cards(&engine.client, &response, &idempotency_key).await;
        return Err(error);
    }

    emit_phase(&progress, RegistrationPhase::Completing);
    match complete::complete_uploaded_cards(&engine.client, &response, &idempotency_key).await {
        Ok(finalized) => response = finalized,
        Err(error) => {
            emit_phase(&progress, RegistrationPhase::CleaningUp);
            abort::abort_pending_cards(&engine.client, &response, &idempotency_key).await;
            return Err(error);
        }
    }
    emit_phase(&progress, RegistrationPhase::Verifying);
    ensure_active(&response)?;
    Ok(response.into())
}

/// Forward one phase event without changing the no-progress registration path.
fn emit_phase(progress: &Option<RegistrationProgressSink>, phase: RegistrationPhase) {
    if let Some(progress) = progress {
        progress(RegistrationProgressEvent::Phase(phase));
    }
}

/// Reject a response that has not reached a server-owned terminal success.
fn ensure_active(response: &CreateCardResponse) -> Result<(), RegistryEngineError> {
    if let Some(pending) = response
        .outcomes
        .iter()
        .find(|outcome| outcome.status != CardLifecycleStatus::Active)
    {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "registration did not reach Active state".to_owned(),
            details: serde_json::json!({
                "card_uid": pending.card_ref.uid,
                "status": pending.status,
            }),
        }
        .into());
    }
    Ok(())
}

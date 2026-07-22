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

/// Drive one declarative registration input through its server-owned lifecycle:
/// resolve authored input, build a typed request, register the composite,
/// upload planned artifacts through [`WyrdStorageClient`], complete artifact
/// Cards, verify Active outcomes, and abort pending Cards after failure.
///
/// The loader owns local source resolution. `WyrdClient` owns authenticated
/// control-plane transport and registration idempotency. `WyrdStorageClient`
/// owns every artifact transfer detail. `wyrd-server` owns durable manifest,
/// activation, audit, and cleanup state. This module only sequences those
/// operations and maps the final response into a receipt.
pub(crate) async fn register(
    engine: &RegistryEngine,
    input: &RegistrationInput,
) -> Result<RegistrationReceipt, RegistryEngineError> {
    let prepared = build_submission::prepare(input).await?;
    let idempotency_key = idempotency::mint();
    let mut response =
        submit::registration(&engine.client, &prepared.request, &idempotency_key).await?;

    if let Err(error) = upload::upload_artifacts(
        &engine.storage,
        &response,
        &prepared.artifact_sources,
        &idempotency_key,
    )
    .await
    {
        abort::abort_pending_cards(&engine.client, &response, &idempotency_key).await;
        return Err(error);
    }

    match complete::complete_uploaded_cards(&engine.client, &response, &idempotency_key).await {
        Ok(finalized) => response = finalized,
        Err(error) => {
            abort::abort_pending_cards(&engine.client, &response, &idempotency_key).await;
            return Err(error);
        }
    }
    ensure_active(&response)?;
    Ok(response.into())
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

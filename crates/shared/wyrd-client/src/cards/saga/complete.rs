//! Card completion orchestration through the server lifecycle endpoint.
//!
//! Registry completion does not verify provider state. The storage client has
//! already completed each planned transfer; the server then verifies and
//! commits the durable Card transition.

use std::collections::BTreeSet;

use crate::WyrdClient;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::registry::CreateCardResponse;

use crate::cards::error::RegistryEngineError;

/// Complete every uploaded Card and merge server-owned final state.
///
/// Cards are completed sequentially in upload-plan order under the saga's one
/// idempotency key, and each server outcome is validated before it replaces
/// the matching registration outcome.
///
/// # Errors
///
/// Returns `RegistryInvalidCardSpec` when an upload plan carries no Card UID,
/// the transport or structured server error from a completion request, and
/// `RegistryArtifactVerifyFailed` when a completion response does not
/// identify exactly the requested Card. Earlier Cards may already be complete
/// when a later one fails.
///
/// # Cancellation
///
/// Cancelling can leave some Cards completed and others pending. Every request
/// carries the saga's idempotency key, so a retry is keyed to the same saga.
pub(crate) async fn complete_uploaded_cards(
    client: &WyrdClient,
    response: &CreateCardResponse,
    idempotency_key: &str,
) -> Result<CreateCardResponse, RegistryEngineError> {
    let card_uids = collect_uploaded_card_uids(response)?;
    let mut completed = response.clone();
    for uid in card_uids {
        let outcome = complete_uploaded_card(client, &uid, idempotency_key).await?;
        validate_completion_response(response, &uid, &outcome)?;
        merge_completed_outcome(&mut completed, outcome)?;
    }
    Ok(completed)
}

/// Extract the unique Card UIDs represented by server upload plans.
///
/// # Errors
///
/// Returns `RegistryInvalidCardSpec` when a plan's Card reference has no UID.
fn collect_uploaded_card_uids(
    response: &CreateCardResponse,
) -> Result<Vec<CardUid>, RegistryEngineError> {
    let mut seen = BTreeSet::new();
    let mut uids = Vec::with_capacity(response.upload_plans.len());
    for plan in &response.upload_plans {
        let uid = plan
            .card_ref
            .uid
            .clone()
            .ok_or_else(|| WyrdError::RegistryInvalidCardSpec {
                message: "server upload plan card reference has no Card UID".to_owned(),
                details: serde_json::json!({}),
            })?;
        if seen.insert(uid.clone()) {
            uids.push(uid);
        }
    }
    Ok(uids)
}

/// Complete one Card using the registration saga's stable idempotency key.
///
/// # Errors
///
/// Returns the client transport error or the structured server refusal for
/// the completion request.
///
/// # Cancellation
///
/// Cancelling after the request is sent leaves the server outcome unknown;
/// the request carries the saga's idempotency key for a keyed retry.
async fn complete_uploaded_card(
    client: &WyrdClient,
    uid: &CardUid,
    idempotency_key: &str,
) -> Result<CreateCardResponse, RegistryEngineError> {
    let path = format!("/v1/cards/{uid}/complete");
    client
        .submit_with_idempotency_key(reqwest::Method::POST, &path, &(), idempotency_key)
        .await
        .map_err(Into::into)
}

/// Reject a completion response that is missing the requested identity or
/// contains a duplicate/unrelated outcome.
///
/// # Errors
///
/// Returns `RegistryArtifactVerifyFailed` unless the response holds exactly
/// one outcome and it carries `uid`.
fn validate_completion_response(
    original: &CreateCardResponse,
    uid: &CardUid,
    completed: &CreateCardResponse,
) -> Result<(), RegistryEngineError> {
    let matching = completed
        .outcomes
        .iter()
        .filter(|outcome| outcome.card_ref.uid.as_ref() == Some(uid))
        .count();
    if matching != 1 || completed.outcomes.len() != 1 {
        return Err(WyrdError::RegistryArtifactVerifyFailed {
            message: "card completion response identity did not match the requested Card"
                .to_owned(),
            details: serde_json::json!({
                "card_uid": uid,
                "outcome_count": completed.outcomes.len(),
                "matching_count": matching,
                "registered_outcomes": original.outcomes.len(),
            }),
        }
        .into());
    }
    Ok(())
}

/// Replace only the matching outcome and preserve the server-derived root.
///
/// # Errors
///
/// Returns `RegistryArtifactVerifyFailed` when `completed` has no outcome or
/// its outcome matches no Card in `response`; `response` is unchanged then.
fn merge_completed_outcome(
    response: &mut CreateCardResponse,
    completed: CreateCardResponse,
) -> Result<(), RegistryEngineError> {
    let outcome = completed.outcomes.into_iter().next().ok_or_else(|| {
        WyrdError::RegistryArtifactVerifyFailed {
            message: "card completion response did not contain an outcome".to_owned(),
            details: serde_json::json!({}),
        }
    })?;
    let existing = response
        .outcomes
        .iter_mut()
        .find(|candidate| candidate.card_ref.same_identity(&outcome.card_ref))
        .ok_or_else(|| WyrdError::RegistryArtifactVerifyFailed {
            message: "card completion response returned an unrelated outcome".to_owned(),
            details: serde_json::json!({ "card_ref": outcome.card_ref }),
        })?;
    *existing = outcome;
    Ok(())
}

/// Completion response validation and outcome merging.
#[cfg(test)]
mod tests {
    use super::{merge_completed_outcome, validate_completion_response};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::CreateCardResponse;
    use wyrd_spec::registry::{
        CardLifecycleStatus, CardRegistrationOutcome, RegistrationOutcomeKind,
    };

    /// Merging keeps the composite root and the server outcome order.
    #[test]
    fn merge_preserves_composite_root_and_outcome_order() {
        let first_uid = CardUid::new("018f0000-0000-7000-8000-000000000001").expect("test UID");
        let second_uid = CardUid::new("018f0000-0000-7000-8000-000000000002").expect("test UID");
        let card_ref = |uid: CardUid, name: &str| CardRef {
            kind: CardKind::Prompt,
            name: CardName::new(name).expect("test name"),
            version: VersionBlock::parse("1.0.0").expect("test version"),
            space: Some(SpaceName::new("default").expect("test space")),
            uid: Some(uid),
        };
        let first = CardRegistrationOutcome {
            card_ref: card_ref(first_uid, "first"),
            spec_hash: "first".to_owned(),
            artifact_hash: None,
            status: CardLifecycleStatus::Pending,
            outcome: RegistrationOutcomeKind::Registered,
            card_blob_uri: None,
        };
        let second = CardRegistrationOutcome {
            card_ref: card_ref(second_uid, "second"),
            spec_hash: "second".to_owned(),
            artifact_hash: None,
            status: CardLifecycleStatus::Pending,
            outcome: RegistrationOutcomeKind::Registered,
            card_blob_uri: None,
        };
        let mut response = CreateCardResponse {
            root: first.card_ref.clone(),
            outcomes: vec![first, second.clone()],
            upload_plans: Vec::new(),
        };
        let completed = CreateCardResponse {
            root: second.card_ref.clone(),
            outcomes: vec![CardRegistrationOutcome {
                status: CardLifecycleStatus::Active,
                ..second
            }],
            upload_plans: Vec::new(),
        };

        merge_completed_outcome(&mut response, completed).expect("matching outcome merges");

        assert_eq!(response.root, response.outcomes[0].card_ref);
        assert_eq!(response.outcomes[0].card_ref.name.as_str(), "first");
        assert_eq!(response.outcomes[1].status, CardLifecycleStatus::Active);
    }

    /// A completion response missing an expected outcome is refused.
    #[test]
    fn rejects_missing_completion_outcome() {
        let uid = CardUid::new("018f0000-0000-7000-8000-000000000001").expect("test UID");
        let card_ref = CardRef {
            kind: CardKind::Prompt,
            name: CardName::new("card-1").expect("test name"),
            version: VersionBlock::parse("1.0.0").expect("test version"),
            space: Some(SpaceName::new("default").expect("test space")),
            uid: Some(uid.clone()),
        };
        let response = CreateCardResponse {
            root: card_ref,
            outcomes: Vec::new(),
            upload_plans: Vec::new(),
        };
        let error: WyrdError = validate_completion_response(&response, &uid, &response)
            .expect_err("missing outcome is rejected")
            .into();
        assert_eq!(error.code(), "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED");
    }

    /// Duplicate or unrelated outcomes in a completion response are refused.
    #[test]
    fn rejects_duplicate_or_unrelated_completion_outcomes() {
        let requested = CardUid::new("018f0000-0000-7000-8000-000000000001").expect("test UID");
        let unrelated = CardUid::new("018f0000-0000-7000-8000-000000000002").expect("test UID");
        let response = |uid: CardUid, name: &str| CardRegistrationOutcome {
            card_ref: CardRef {
                kind: CardKind::Prompt,
                name: CardName::new(name).expect("test name"),
                version: VersionBlock::parse("1.0.0").expect("test version"),
                space: Some(SpaceName::new("default").expect("test space")),
                uid: Some(uid),
            },
            spec_hash: "sha256:spec".to_owned(),
            artifact_hash: None,
            status: CardLifecycleStatus::Active,
            outcome: RegistrationOutcomeKind::Registered,
            card_blob_uri: Some("wyrd://blob".parse().expect("test blob URI")),
        };
        let duplicate = CreateCardResponse {
            root: response(requested.clone(), "requested").card_ref,
            outcomes: vec![
                response(requested.clone(), "requested"),
                response(requested.clone(), "requested"),
            ],
            upload_plans: Vec::new(),
        };
        assert!(validate_completion_response(&duplicate, &requested, &duplicate).is_err());

        let unrelated_response = CreateCardResponse {
            root: response(unrelated.clone(), "unrelated").card_ref,
            outcomes: vec![response(unrelated, "unrelated")],
            upload_plans: Vec::new(),
        };
        assert!(validate_completion_response(&duplicate, &requested, &unrelated_response).is_err());
    }
}

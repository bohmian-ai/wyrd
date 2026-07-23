//! Best-effort cleanup of pending Card registration state.

use wyrd_client::WyrdClient;
use wyrd_spec::registry::{CardLifecycleStatus, CreateCardResponse};

/// Abort every eligible non-Active Card and preserve the original saga error.
pub(crate) async fn abort_pending_cards(
    client: &WyrdClient,
    response: &CreateCardResponse,
    idempotency_key: &str,
) {
    for uid in pending_card_uids(response) {
        if let Err(error) = abort_card_registration(client, &uid, idempotency_key).await {
            tracing::warn!(card_uid = %uid, error = %error, "card registration cleanup failed");
        }
    }
}

/// Select only outcomes eligible for cleanup; Active Cards are never aborted.
fn pending_card_uids(response: &CreateCardResponse) -> Vec<wyrd_spec::ids::CardUid> {
    response
        .outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome.status,
                CardLifecycleStatus::Pending | CardLifecycleStatus::Failed
            )
        })
        .filter_map(|outcome| outcome.card_ref.uid.clone())
        .collect()
}

/// Issue one idempotent server lifecycle cleanup request.
async fn abort_card_registration(
    client: &WyrdClient,
    uid: &wyrd_spec::ids::CardUid,
    idempotency_key: &str,
) -> Result<CreateCardResponse, wyrd_spec::error::WyrdError> {
    let path = format!("/v1/cards/{uid}/abort");
    client
        .submit_with_idempotency_key(reqwest::Method::POST, &path, &(), idempotency_key)
        .await
}

#[cfg(test)]
mod tests {
    use super::pending_card_uids;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::{CardLifecycleStatus, CardRegistrationOutcome, CreateCardResponse};

    fn card_ref(name: &str) -> CardRef {
        let uid = match name {
            "active" => "018f0000-0000-7000-8000-000000000001",
            "pending" => "018f0000-0000-7000-8000-000000000002",
            _ => "018f0000-0000-7000-8000-000000000003",
        };
        CardRef {
            kind: CardKind::Prompt,
            name: CardName::new(name).expect("test name"),
            version: VersionBlock::parse("1.0.0").expect("test version"),
            space: Some(SpaceName::new("default").expect("test space")),
            uid: Some(CardUid::new(uid).expect("test UID")),
        }
    }

    #[test]
    fn active_cards_are_not_cleanup_candidates() {
        let active = CardRegistrationOutcome {
            card_ref: card_ref("active"),
            spec_hash: String::new(),
            artifact_hash: None,
            status: CardLifecycleStatus::Active,
            outcome: wyrd_spec::registry::RegistrationOutcomeKind::Registered,
            card_blob_uri: None,
        };
        let pending = CardRegistrationOutcome {
            card_ref: card_ref("pending"),
            spec_hash: String::new(),
            artifact_hash: None,
            status: CardLifecycleStatus::Pending,
            outcome: wyrd_spec::registry::RegistrationOutcomeKind::Registered,
            card_blob_uri: None,
        };
        let failed = CardRegistrationOutcome {
            card_ref: card_ref("failed"),
            spec_hash: String::new(),
            artifact_hash: None,
            status: CardLifecycleStatus::Failed,
            outcome: wyrd_spec::registry::RegistrationOutcomeKind::Registered,
            card_blob_uri: None,
        };
        let response = CreateCardResponse {
            root: pending.card_ref.clone(),
            outcomes: vec![active, pending, failed],
            upload_plans: Vec::new(),
        };
        let uids = pending_card_uids(&response);
        assert_eq!(uids.len(), 2);
        assert_eq!(
            uids[0],
            response.outcomes[1].card_ref.uid.clone().expect("test UID")
        );
        assert_eq!(
            uids[1],
            response.outcomes[2].card_ref.uid.clone().expect("test UID")
        );
    }
}

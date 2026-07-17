//! Projections from registry rows into composite registration responses.

use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{CardLifecycleStatus, CardRegistrationOutcome, RegistrationOutcomeKind};
use wyrd_sql::queries::cards::RegisteredCardRow;
use wyrd_sql::row_types::cards::{CardStatus, ParsedCardRow};

/// Project one durable card row and write classification onto the wire.
#[must_use]
pub fn outcome_row_to_response(
    row: &RegisteredCardRow,
    outcome: RegistrationOutcomeKind,
) -> CardRegistrationOutcome {
    CardRegistrationOutcome {
        card_ref: CardRef {
            kind: row.kind.clone(),
            name: row.name.clone(),
            version: row.version.clone(),
            space: row.space.clone(),
            uid: Some(row.card_uid.clone()),
        },
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        status: lifecycle_status(row.status),
        outcome,
        card_blob_uri: None,
    }
}

/// Project an existing row used by pin idempotency or content deduplication.
#[must_use]
pub fn existing_row_to_response(
    row: &ParsedCardRow,
    outcome: RegistrationOutcomeKind,
) -> CardRegistrationOutcome {
    CardRegistrationOutcome {
        card_ref: CardRef {
            kind: row.kind.clone(),
            name: row.name.clone(),
            version: row.version.clone(),
            space: row.space.clone(),
            uid: Some(row.card_uid.clone()),
        },
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        status: lifecycle_status(row.status),
        outcome,
        card_blob_uri: None,
    }
}

/// Convert the SQL lifecycle enum without stringly response mapping.
const fn lifecycle_status(status: CardStatus) -> CardLifecycleStatus {
    match status {
        CardStatus::Pending => CardLifecycleStatus::Pending,
        CardStatus::Active => CardLifecycleStatus::Active,
        CardStatus::Deprecated => CardLifecycleStatus::Deprecated,
        CardStatus::Deleted => CardLifecycleStatus::Deleted,
        CardStatus::Failed => CardLifecycleStatus::Failed,
        CardStatus::Expired => CardLifecycleStatus::Expired,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;
    use wyrd_runtime::principal::PrincipalId;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid, SpaceName};
    use wyrd_spec::registry::{
        CardLifecycleStatus, RegistrationOperationId, RegistrationOutcomeKind,
    };
    use wyrd_sql::queries::cards::RegisteredCardRow;
    use wyrd_sql::row_types::cards::CardStatus;

    use super::outcome_row_to_response;

    /// Build one deterministic durable row for response projection tests.
    fn row(status: CardStatus) -> RegisteredCardRow {
        RegisteredCardRow {
            card_uid: CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID"),
            kind: CardKind::Prompt,
            space: SpaceName::new("default").expect("static space is valid"),
            name: CardName::new("projection").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            spec_hash: "spec-hash".to_owned(),
            artifact_hash: None,
            status,
            created_at: Utc::now(),
            principal_id: PrincipalId::new(Uuid::now_v7()),
            operation_id: RegistrationOperationId::new(Uuid::now_v7()),
        }
    }

    /// Project every durable lifecycle status through its typed wire variant.
    #[test]
    fn outcome_projects_typed_status() {
        for (stored, expected) in [
            (CardStatus::Pending, CardLifecycleStatus::Pending),
            (CardStatus::Active, CardLifecycleStatus::Active),
            (CardStatus::Deprecated, CardLifecycleStatus::Deprecated),
            (CardStatus::Deleted, CardLifecycleStatus::Deleted),
            (CardStatus::Failed, CardLifecycleStatus::Failed),
            (CardStatus::Expired, CardLifecycleStatus::Expired),
        ] {
            let outcome =
                outcome_row_to_response(&row(stored), RegistrationOutcomeKind::Registered);
            assert_eq!(outcome.status, expected);
        }
    }

    /// Preserve UID-bearing card identity and registration classification.
    #[test]
    fn outcome_projects_registered_identity() {
        let row = row(CardStatus::Pending);
        let projected = outcome_row_to_response(&row, RegistrationOutcomeKind::Registered);

        assert_eq!(projected.card_ref.uid.as_ref(), Some(&row.card_uid));
        assert_eq!(projected.spec_hash, row.spec_hash);
        assert_eq!(projected.outcome, RegistrationOutcomeKind::Registered);
        assert_eq!(projected.status, CardLifecycleStatus::Pending);
    }
}

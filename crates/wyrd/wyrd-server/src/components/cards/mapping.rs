//! Projections from registry rows into composite registration responses.

use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{CardLifecycleStatus, CardRegistrationOutcome, RegistrationOutcomeKind};
use wyrd_sql::queries::cards::RegisterCardOutcomeKind;
use wyrd_sql::row_types::cards::{CardStatus, ParsedCardRow};

/// Project one durable card row and write classification onto the wire.
#[must_use]
pub fn outcome_row_to_response(
    row: &ParsedCardRow,
    outcome: RegisterCardOutcomeKind,
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
        outcome: match outcome {
            RegisterCardOutcomeKind::Created => RegistrationOutcomeKind::Registered,
            RegisterCardOutcomeKind::IdempotentNoop => RegistrationOutcomeKind::IdempotentNoop,
            RegisterCardOutcomeKind::Deduplicated => RegistrationOutcomeKind::Deduplicated,
        },
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

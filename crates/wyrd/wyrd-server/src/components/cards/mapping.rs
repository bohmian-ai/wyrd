//! Registry row to wire-response mapping.

use wyrd_spec::envelope::Relationships;
use wyrd_spec::registry::{CardLifecycleStatus, CreateCardResponse, RegisterOutcome};
use wyrd_spec::storage::UploadInitResponse;
use wyrd_sql::queries::cards::RegisteredCardRow;

/// Build the flat create-card response from server-owned values.
#[must_use]
pub fn registered_row_to_response(
    row: &RegisteredCardRow,
    outcome: RegisterOutcome,
    uploads: Vec<UploadInitResponse>,
) -> CreateCardResponse {
    CreateCardResponse {
        card_uid: row.card_uid.clone(),
        kind: row.kind.clone(),
        space: row.space.clone(),
        name: row.name.clone(),
        version: row.version.clone(),
        spec_hash: row.spec_hash.clone(),
        artifact_hash: row.artifact_hash.clone(),
        outcome,
        status: match row.status {
            wyrd_sql::row_types::cards::CardStatus::Pending => CardLifecycleStatus::Pending,
            wyrd_sql::row_types::cards::CardStatus::Active => CardLifecycleStatus::Active,
            wyrd_sql::row_types::cards::CardStatus::Deprecated => CardLifecycleStatus::Deprecated,
            wyrd_sql::row_types::cards::CardStatus::Deleted => CardLifecycleStatus::Deleted,
            wyrd_sql::row_types::cards::CardStatus::Failed => CardLifecycleStatus::Failed,
            wyrd_sql::row_types::cards::CardStatus::Expired => CardLifecycleStatus::Expired,
        },
        created_at: row.created_at,
        principal_id: Some(row.principal_id),
        operation_id: Some(row.operation_id),
        uploads,
    }
}

/// Return the concrete empty relationship projection used before graph reads land.
#[must_use]
pub fn relationships_from_row() -> Relationships {
    Relationships {
        outbound: Vec::new(),
        inbound: Vec::new(),
    }
}

/// Serialize non-bearer upload plans for durable replay inventory.
pub fn upload_plans_json(
    uploads: &[UploadInitResponse],
) -> Result<serde_json::Value, wyrd_spec::error::WyrdError> {
    serde_json::to_value(uploads).map_err(wyrd_spec::error::WyrdError::from_spec_serialization)
}

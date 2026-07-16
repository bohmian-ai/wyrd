//! Registry row to wire-response mapping.

use wyrd_spec::envelope::Relationships;
use wyrd_spec::registry::{CreateCardResponse, RegisterOutcome};
use wyrd_spec::storage::UploadInitResponse;
use wyrd_sql::queries::cards::{RegisteredCardRow, build_create_response};

/// Build the flat create-card response from server-owned values.
#[must_use]
pub fn registered_row_to_response(
    row: &RegisteredCardRow,
    outcome: RegisterOutcome,
    uploads: Vec<UploadInitResponse>,
) -> CreateCardResponse {
    build_create_response(row, outcome, uploads)
}

/// Return the concrete empty relationship projection used before graph reads land.
#[must_use]
pub fn relationships_from_row() -> Relationships {
    Relationships {
        outbound: Vec::new(),
        inbound: Vec::new(),
    }
}

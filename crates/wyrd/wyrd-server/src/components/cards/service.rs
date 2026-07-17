//! Thin card-registration service boundary.

use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CreateCardRequest, CreateCardResponse};

use crate::components::auth::Caller;
use crate::components::cards::compose::compose_registration;
use crate::state::AppState;

/// Register a composite card request through the server-owned transaction.
pub async fn register_card(
    state: &AppState,
    caller: &Caller,
    idempotency_key: &str,
    request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    compose_registration(state, caller, idempotency_key, request).await
}

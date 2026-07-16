//! Card registration service boundary.
//!
//! The pre-doctrine single-card registration pipeline was retired when the
//! composite registry contract landed. Engine Slice 1 owns the wire types and
//! client seams; the Registry PR#1 slice will replace this boundary with the
//! new server implementation.

use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CreateCardRequest, CreateCardResponse};

use crate::components::auth::Caller;
use crate::state::AppState;

/// Register a composite card submission.
///
/// The route remains present for the server build while the old orchestration
/// is retired. Registry PR#1 owns the durable implementation.
pub async fn register_card(
    _state: &AppState,
    _caller: &Caller,
    _idempotency_key: &str,
    _request: CreateCardRequest,
) -> Result<CreateCardResponse, WyrdError> {
    Err(WyrdError::internal(
        "composite card registration is not implemented in this engine slice",
    ))
}

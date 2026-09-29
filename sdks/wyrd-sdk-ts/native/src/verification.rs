//! Thin napi projection of the Rust-owned Verification control-plane handle.
//!
//! Every operation delegates to `wyrd_client::verification::Verification`;
//! this module only parses Node strings into wire types and projects results
//! through [`NativeLifecycleResult`], so failures keep their catalog metadata.

use std::result::Result as StdResult;

use napi::Result;
use napi_derive::napi;
use serde::de::DeserializeOwned;
use wyrd_client::verification::{
    BindingId, StartVerificationRunRequest, StartVerificationRunResponse, Verification,
    VerificationRunId,
};
use wyrd_spec::error::WyrdError;

use crate::{NativeLifecycleResult, NativeWyrdError};

/// Decode one serialized JavaScript argument, naming it on failure.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` with `details.field` set to `field` when
/// `json` does not match the wire contract.
fn decode<T: DeserializeOwned>(field: &str, json: &str) -> StdResult<T, WyrdError> {
    serde_json::from_str(json).map_err(|error| WyrdError::Validation {
        message: format!("{field} is invalid: {error}"),
        details: serde_json::json!({ "field": field, "reason": error.to_string() }),
    })
}

/// Tenant-scoped Verification handle over the shared `wyrd_client` handle.
#[napi]
pub struct NativeVerification {
    /// Shared handle owning transport and authentication.
    verification: Verification,
}

/// Closed result of building one Verification handle: a handle or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeVerificationConnection {
    /// Verification handle when construction succeeded.
    pub verification: Option<NativeVerification>,
    /// Catalog failure when no credential resolves or the client cannot be built.
    pub error: Option<NativeWyrdError>,
}

/// Builds one Verification handle without performing IO.
///
/// Omitted arguments resolve through the same shared client configuration
/// chain as `connectCards`, so every capability authenticates identically.
#[napi]
pub fn connect_verification(
    server_url: Option<String>,
    credential: Option<String>,
) -> NativeVerificationConnection {
    let client = wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        None,
    );
    drop(server_url);
    drop(credential);
    match client {
        Ok(client) => NativeVerificationConnection {
            verification: Some(NativeVerification {
                verification: Verification::with_client(client),
            }),
            error: None,
        },
        Err(error) => NativeVerificationConnection {
            verification: None,
            error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
        },
    }
}

#[napi]
impl NativeVerification {
    /// Reads one binding's identities, activity, readiness, and cursor.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the status cannot be serialized; a
    /// malformed ID or server refusal is returned in the result.
    #[napi]
    pub async fn get_binding(&self, binding_id: String) -> Result<NativeLifecycleResult> {
        let result = match decode::<BindingId>(
            "binding_id",
            &serde_json::Value::String(binding_id).to_string(),
        ) {
            Ok(binding_id) => self.verification.get_binding(&binding_id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Durably enqueues one manual Drift run and returns `{ run_id }`.
    ///
    /// `request_json` is one serialized `StartVerificationRunRequest`; a retry
    /// with the same `idempotency_key` and request returns the same run.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the response cannot be serialized; a
    /// malformed request or server refusal is returned in the result.
    #[napi]
    pub async fn start_run(
        &self,
        request_json: String,
        idempotency_key: Option<String>,
    ) -> Result<NativeLifecycleResult> {
        let result = match decode::<StartVerificationRunRequest>("request", &request_json) {
            Ok(request) => self
                .verification
                .start_run(&request, idempotency_key.as_deref())
                .await
                .map(|run_id| StartVerificationRunResponse { run_id }),
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Reads one run's status, requester, result pointer, and dispatches.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the status cannot be serialized; a
    /// malformed ID or server refusal is returned in the result.
    #[napi]
    pub async fn get_run(&self, run_id: String) -> Result<NativeLifecycleResult> {
        let result = match decode::<VerificationRunId>(
            "run_id",
            &serde_json::Value::String(run_id).to_string(),
        ) {
            Ok(run_id) => self.verification.get_run(&run_id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }
}

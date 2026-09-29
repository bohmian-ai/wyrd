//! Verification binding status, manual Verifier runs, and run status.
//!
//! The shared implementation every first-class SDK projects. It owns no
//! durable state and no analysis engine: the server decides readiness,
//! authorizes and audits each request, enqueues runs, and reports their
//! status. Verdicts and Drift/Eval details are read through
//! [`Bifrost`](crate::Bifrost) queries by `result_id`; baseline status and
//! binding IDs are read through [`Cards`](crate::cards::Cards).

use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Arc;

use reqwest::Method;

use crate::client::WyrdClient;

// The wire contract this handle speaks, re-exported so an SDK user reaches one
// module for the capability and its types.
pub use wyrd_spec::error::WyrdError;
pub use wyrd_spec::ids::{BindingId, VerificationResultId, VerificationRunId};
pub use wyrd_spec::verification::{
    DriftWindow, OperatorDispatchState, OperatorDispatchStatus, StartVerificationRunRequest,
    StartVerificationRunResponse, VerificationBindingStatus, VerificationError,
    VerificationExecutionStatus, VerificationRunInput, VerificationRunStatus,
    VerificationRunTarget, VerifierReadiness,
};

/// Cheap-to-clone, tenant-scoped Verification control-plane handle.
///
/// Shaped like [`Cards`](crate::cards::Cards): one `Arc`-shared authenticated
/// client and discoverable inherent methods. Cloning only bumps the `Arc`.
#[derive(Clone)]
pub struct Verification {
    /// Shared authenticated client owning transport, credentials, and the
    /// access-token cache.
    client: Arc<WyrdClient>,
}

impl Debug for Verification {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Verification").finish_non_exhaustive()
    }
}

impl Verification {
    /// Construct a handle around an already assembled client.
    #[must_use]
    pub fn with_client(client: WyrdClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Construct a handle from the ambient client configuration.
    ///
    /// No network call or token exchange happens here; the first request does
    /// the exchange.
    ///
    /// # Errors
    /// Returns a Wyrd error when the local configuration or credential cannot
    /// be resolved.
    pub fn from_env() -> Result<Self, WyrdError> {
        let client = WyrdClient::from_env().map_err(WyrdError::from)?;
        Ok(Self::with_client(client))
    }

    /// Read one binding's exact identities, activity gate, readiness, and cursor.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `cards:read`, the binding is
    /// not in the caller's tenant, or the request fails.
    pub async fn get_binding(
        &self,
        binding_id: &BindingId,
    ) -> Result<VerificationBindingStatus, WyrdError> {
        self.client
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/verification/bindings/{binding_id}"),
                None,
            )
            .await
    }

    /// Durably enqueue one manual Drift run and return its ID.
    ///
    /// Returns once the server has enqueued the run, without waiting for
    /// scoring; poll [`Self::get_run`]. Every request carries an
    /// `Idempotency-Key`, replayed verbatim across transport retries so a lost
    /// response never enqueues twice: `idempotency_key` when given, so a
    /// caller can also retry across processes, otherwise one minted for this
    /// call.
    ///
    /// # Errors
    /// Returns a Wyrd error when the window or target is invalid, the caller
    /// lacks `evals:run` or scope over the subject, the binding is unknown,
    /// the Verifier is not ready, the key was used for a different request,
    /// or the request fails.
    pub async fn start_run(
        &self,
        request: &StartVerificationRunRequest,
        idempotency_key: Option<&str>,
    ) -> Result<VerificationRunId, WyrdError> {
        let response: StartVerificationRunResponse = match idempotency_key {
            Some(key) => {
                self.client
                    .submit_with_idempotency_key(
                        Method::POST,
                        "/v1/verification/runs",
                        request,
                        key,
                    )
                    .await?
            }
            None => {
                self.client
                    .submit_idempotent(Method::POST, "/v1/verification/runs", request)
                    .await?
            }
        };
        Ok(response.run_id)
    }

    /// Read one run's execution status, requester, result pointer, and dispatches.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `cards:read`, the run is not
    /// in the caller's tenant, or the request fails.
    pub async fn get_run(
        &self,
        run_id: &VerificationRunId,
    ) -> Result<VerificationRunStatus, WyrdError> {
        self.client
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/verification/runs/{run_id}"),
                None,
            )
            .await
    }
}

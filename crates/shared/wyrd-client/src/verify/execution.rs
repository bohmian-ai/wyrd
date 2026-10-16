//! [`Execution`]: the handle of one queued Verifier execution.
//!
//! `submit` returns it for Run evidence, and any process can rebuild it
//! from the execution ID with `connect`. It holds only the ID and one shared
//! client; the queue, retries, and results stay server-owned. `wait` polls
//! the status route with capped backoff and returns only a real result, the
//! execution's terminal error, or a timeout.

use std::time::Duration;

use reqwest::Method;
use serde_json::json;
use tokio::time::Instant;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::VerificationExecutionStatus;
use wyrd_spec::verification::execution::{VerificationExecutionState, VerificationResult};

use crate::client::WyrdClient;
use crate::error::code_to_wyrd_error;

/// First pause between status polls.
const FIRST_POLL: Duration = Duration::from_millis(100);
/// Longest pause between status polls.
const MAX_POLL: Duration = Duration::from_secs(2);

/// One queued Verifier execution, addressed by its server-issued ID.
#[derive(Debug, Clone)]
pub struct Execution {
    /// The execution identity.
    id: ExecutionId,
    /// The client every status read goes through.
    client: WyrdClient,
}

impl Execution {
    /// The handle of `id`, read through `client`.
    pub(crate) fn new(id: ExecutionId, client: WyrdClient) -> Self {
        Self { id, client }
    }

    /// Rebuild the handle of `id` as the ambient client's principal, such as
    /// in another process than the one that submitted it.
    ///
    /// # Arguments
    /// * `id` - The execution ID `submit` returned.
    ///
    /// # Errors
    /// Returns the client configuration or credential resolution error.
    pub async fn connect(id: ExecutionId) -> Result<Self, WyrdError> {
        Ok(Self::new(id, WyrdClient::connect_ambient().await?))
    }

    /// Rebuild the handle of `id` as `client`'s principal, used as given.
    ///
    /// # Arguments
    /// * `id` - The execution ID `submit` returned.
    /// * `client` - The client every status read is sent through.
    ///
    /// # Errors
    /// Returns the client's credential resolution error.
    pub async fn connect_with(id: ExecutionId, client: &WyrdClient) -> Result<Self, WyrdError> {
        Ok(Self::new(id, client.connected().await?))
    }

    /// The execution ID.
    #[must_use]
    pub fn id(&self) -> &ExecutionId {
        &self.id
    }

    /// Read the execution's current status, terminal error, and result.
    ///
    /// # Errors
    /// Returns the server's not-found error for an execution that is not
    /// visible to the caller's tenant yet or at all, and its authorization
    /// or transport error.
    pub async fn status(&self) -> Result<VerificationExecutionState, WyrdError> {
        self.client
            .http
            .request_json(
                Method::GET,
                &format!("/v1/verification/executions/{}", self.id),
                None::<&()>,
            )
            .await
    }

    /// Poll until the execution yields a result, fails, or `timeout` passes.
    ///
    /// Polls back off from 100 ms to at most 2 s and never sleep past the
    /// deadline. Not-found and server-unavailable reads are retried until
    /// then, since a staged submission becomes visible only after the queue
    /// accepts it. A completed execution whose result is not visible yet
    /// keeps being polled: a result is never invented from a status.
    ///
    /// # Arguments
    /// * `timeout` - How long to keep polling.
    ///
    /// # Errors
    /// Returns the execution's own structured error when it settled as
    /// errored, cancelled, or timed out, including a delayed
    /// idempotency conflict; `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` when
    /// `timeout` passes first; and any other status-read error unchanged.
    ///
    /// # Cancellation
    /// Dropping the future stops polling; the execution is unaffected.
    pub async fn wait(&self, timeout: Duration) -> Result<VerificationResult, WyrdError> {
        let deadline = Instant::now() + timeout;
        let mut pause = FIRST_POLL;
        let mut last = None;
        loop {
            match self.status().await {
                Ok(state) => match (state.status, state.result, state.error) {
                    (_, Some(result), _) => return Ok(result),
                    (
                        VerificationExecutionStatus::Errored
                        | VerificationExecutionStatus::Cancelled
                        | VerificationExecutionStatus::TimedOut,
                        None,
                        error,
                    ) => {
                        let details = json!({ "execution_id": self.id, "status": state.status });
                        return Err(match error {
                            Some(error) => code_to_wyrd_error(&error.code, error.message, details),
                            None => WyrdError::Internal {
                                message: "execution settled without its error".to_owned(),
                                details,
                            },
                        });
                    }
                    (status, None, _) => last = Some(status),
                },
                Err(error) if error.status() == 404 || error.status() >= 500 => {}
                Err(error) => return Err(error),
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(WyrdError::SdkVerificationWaitTimedOut {
                    message: format!("execution {} did not settle within {timeout:?}", self.id),
                    details: json!({ "execution_id": self.id, "last_status": last }),
                });
            }
            tokio::time::sleep(pause.min(deadline - now)).await;
            pause = (pause * 2).min(MAX_POLL);
        }
    }
}

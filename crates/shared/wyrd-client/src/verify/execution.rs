//! [`Execution`]: the handle of one queued Verifier execution.
//!
//! `submit` returns it for Run evidence, and any process can rebuild it
//! from the execution ID with `connect`. It holds only the ID and one shared
//! client; the queue, retries, and results stay server-owned. `wait` polls
//! the status route with capped backoff and returns only a real result, the
//! execution's terminal error, or a timeout.

use std::time::Duration;

use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::time::Instant;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::storage::download::DownloadInitResponse;
use wyrd_spec::verification::VerificationExecutionStatus;
use wyrd_spec::verification::execution::{
    CancelExecutionResponse, ExecutionPageQuery, InvocationPage, Publication, PublicationPage,
    PublicationSelector, VerificationExecutionState, VerificationResult,
};
use wyrd_spec::verification::{PublicationName, TaskId};

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

    /// Read one page of the execution's checkpointed invocation attempts,
    /// with its status and history projection position.
    ///
    /// Progress reads from the execution's records, so it is current while
    /// history projection lags. Follow `next_cursor` unchanged to read the
    /// rest of the same listing cut; start without a cursor to see attempts
    /// committed since.
    ///
    /// # Arguments
    /// * `page` - Page size and continuation.
    ///
    /// # Errors
    /// Returns the server's validation error for a limit outside 1–200 or a
    /// cursor of another listing, its permission error without
    /// `verifier:read` on the execution's Verifier, its not-found error for
    /// an execution not visible to the caller's tenant, and transport errors.
    ///
    /// # Panics
    /// Never in practice: a page query of optional scalars always URL-encodes.
    pub async fn invocations(
        &self,
        page: &ExecutionPageQuery,
    ) -> Result<InvocationPage, WyrdError> {
        self.get(&format!("invocations?{}", Self::query(page)))
            .await
    }

    /// Read one page of the execution's accepted publications, names, kinds,
    /// and producers only, without loading any content.
    ///
    /// # Arguments
    /// * `page` - Page size and continuation.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::invocations`].
    ///
    /// # Panics
    /// Never in practice: a page query of optional scalars always URL-encodes.
    pub async fn publications(
        &self,
        page: &ExecutionPageQuery,
    ) -> Result<PublicationPage, WyrdError> {
        self.get(&format!("publications?{}", Self::query(page)))
            .await
    }

    /// Read publication `name` of `step`'s exact producer attempt: a JSON
    /// publication's value, `null` included, or a file or directory
    /// publication's manifest.
    ///
    /// # Arguments
    /// * `step` - The producing step.
    /// * `name` - The publication's logical name.
    /// * `selector` - The exact attempt, item, and iteration a listing reports.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::invocations`], and
    /// `WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND` when that producer
    /// attempt accepted no publication of that name.
    ///
    /// # Panics
    /// Never in practice: a selector of scalars always URL-encodes.
    pub async fn publication(
        &self,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
    ) -> Result<Publication, WyrdError> {
        self.get(&format!(
            "publications/{}/{}?{}",
            step.as_str(),
            name.as_str(),
            Self::query(selector)
        ))
        .await
    }

    /// Plan the download of a file publication, or of the regular file at
    /// `selector.path` of a directory publication, as the shared File
    /// download plan.
    ///
    /// # Arguments
    /// * `step` - The producing step.
    /// * `name` - The publication's logical name.
    /// * `selector` - The exact producer attempt, and a directory file's path.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::publication`], the validation error
    /// for a JSON publication, a missing or unlisted directory path, or a
    /// path on a file publication, and
    /// `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` while the File's
    /// completed record is not yet visible.
    ///
    /// # Panics
    /// Never in practice: a selector of scalars always URL-encodes.
    pub async fn download(
        &self,
        step: &TaskId,
        name: &PublicationName,
        selector: &PublicationSelector,
    ) -> Result<DownloadInitResponse, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(
                Method::POST,
                &format!(
                    "/v1/verification/executions/{}/publications/{}/{}/download?{}",
                    self.id,
                    step.as_str(),
                    name.as_str(),
                    Self::query(selector)
                ),
                None,
            )
            .await
    }

    /// Cancel the execution if it is unfinished, returning its status after
    /// the durable cancellation fence.
    ///
    /// Unfinished work becomes `cancelled`; its checkpoints and
    /// publications are kept and its worker is fenced. Already terminal work
    /// keeps its status and result. Repeating it is harmless.
    ///
    /// # Errors
    /// Returns the server's permission error without `verifier:run` on the
    /// execution's Verifier, its not-found error for an execution not visible
    /// to the caller's tenant, and transport errors.
    pub async fn cancel(&self) -> Result<CancelExecutionResponse, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(
                Method::POST,
                &format!("/v1/verification/executions/{}/cancel", self.id),
                None,
            )
            .await
    }

    /// `GET` the execution's sub-resource `tail`.
    ///
    /// # Errors
    /// Returns the server's or transport's error.
    async fn get<D: DeserializeOwned>(&self, tail: &str) -> Result<D, WyrdError> {
        self.client
            .http
            .request_json::<(), _>(
                Method::GET,
                &format!("/v1/verification/executions/{}/{tail}", self.id),
                None,
            )
            .await
    }

    /// `query` URL-encoded.
    ///
    /// # Panics
    /// Panics when `query` holds a value that cannot URL-encode, which no
    /// query of optional scalars does.
    fn query(query: &impl serde::Serialize) -> String {
        serde_urlencoded::to_string(query).expect("a query of optional scalars always URL-encodes")
    }

    /// Poll until the execution yields a result, fails, or `timeout` passes.
    ///
    /// Equivalent to [`Execution::wait_until`] with a deadline `timeout`
    /// from now.
    ///
    /// # Arguments
    /// * `timeout` - How long to keep polling.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::wait_until`].
    ///
    /// # Cancellation
    /// Dropping the future stops polling; the execution is unaffected.
    pub async fn wait(&self, timeout: Duration) -> Result<VerificationResult, WyrdError> {
        self.wait_until(Instant::now() + timeout).await
    }

    /// Poll until the execution yields a result, fails, or `deadline` passes.
    ///
    /// Polls back off from 100 ms to at most 2 s and never sleep past the
    /// deadline. The deadline also bounds each status read, including its
    /// credential resolution, transport retries, and body decoding, and no
    /// read starts once it has passed, so an expired deadline reads nothing.
    /// A caller that spends part of its budget before polling, such as
    /// connecting the handle, passes the deadline it started with.
    /// Not-found and server-unavailable reads are retried until
    /// then, since a staged submission becomes visible only after the queue
    /// accepts it. A completed execution whose result is not visible yet
    /// keeps being polled: a result is never invented from a status.
    ///
    /// # Arguments
    /// * `deadline` - When to stop polling.
    ///
    /// # Errors
    /// Returns the execution's own structured error when it settled as
    /// errored, cancelled, or timed out; [`Execution::wait_timed_out`]'s error when
    /// `deadline` passes first; and any other status-read error unchanged.
    ///
    /// # Cancellation
    /// Dropping the future stops polling; the execution is unaffected.
    pub async fn wait_until(&self, deadline: Instant) -> Result<VerificationResult, WyrdError> {
        let mut pause = FIRST_POLL;
        let mut last = None;
        loop {
            let read = if Instant::now() < deadline {
                tokio::time::timeout_at(deadline, self.status()).await.ok()
            } else {
                None
            };
            let Some(read) = read else {
                return Err(Self::wait_timed_out(&self.id, last));
            };
            match read {
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
            tokio::time::sleep_until(deadline.min(Instant::now() + pause)).await;
            pause = (pause * 2).min(MAX_POLL);
        }
    }

    /// [`Execution::status`], with a present result's custom `data` decoded
    /// as `T`.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::status`], then
    /// `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not
    /// decode as `T`.
    pub async fn status_typed<T: DeserializeOwned>(
        &self,
    ) -> Result<VerificationExecutionState<T>, WyrdError> {
        self.status().await?.decode_data()
    }

    /// [`Execution::wait`], with the result's custom `data` decoded as `T`.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::wait`], then
    /// `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not
    /// decode as `T`.
    ///
    /// # Cancellation
    /// Dropping the future stops polling; the execution is unaffected.
    pub async fn wait_typed<T: DeserializeOwned>(
        &self,
        timeout: Duration,
    ) -> Result<VerificationResult<T>, WyrdError> {
        self.wait(timeout).await?.decode_data()
    }

    /// [`Execution::wait_until`], with the result's custom `data` decoded as
    /// `T`.
    ///
    /// # Errors
    /// Returns the errors of [`Execution::wait_until`], then
    /// `WYRD_SDK_422_VERIFICATION_DATA_INVALID` when present data does not
    /// decode as `T`.
    ///
    /// # Cancellation
    /// Dropping the future stops polling; the execution is unaffected.
    pub async fn wait_until_typed<T: DeserializeOwned>(
        &self,
        deadline: Instant,
    ) -> Result<VerificationResult<T>, WyrdError> {
        self.wait_until(deadline).await?.decode_data()
    }

    /// The `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` error a wait on `id`
    /// returns when its deadline passes, with the `last` status it read, if
    /// any; shared by every phase of a wait, including connecting the handle.
    #[must_use]
    pub fn wait_timed_out(
        id: &ExecutionId,
        last: Option<VerificationExecutionStatus>,
    ) -> WyrdError {
        WyrdError::SdkVerificationWaitTimedOut {
            message: format!("execution {id} did not settle before the wait deadline"),
            details: json!({ "execution_id": id, "last_status": last }),
        }
    }
}

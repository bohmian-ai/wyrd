//! Server Workflow runs over the shared authenticated transport.
//!
//! [`Workflows`] projects the `/v1/workflow-runs` resource onto the portable
//! `wyrd-spec` run contract. The server resolves, authorizes, audits, and
//! executes; this handle chooses the route, reuses the transport's
//! idempotent submission for create, and polls for a terminal snapshot.

use std::time::Duration;

use reqwest::Method;
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, WorkflowRun};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::WorkflowRunId;

use crate::WyrdClient;

/// Route of the Workflow-run collection.
const WORKFLOW_RUNS: &str = "/v1/workflow-runs";

/// Interval between [`Workflows::wait`] polls.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Server Workflow-run handle over one shared [`WyrdClient`].
///
/// Cloning is cheap; clones share the client's connection pool and token
/// cache. Every method returns the server's direct [`WorkflowRun`] snapshot;
/// a failed, cancelled, or timed-out run is a returned value, not an error.
#[derive(Debug, Clone)]
pub struct Workflows {
    /// Authenticated transport used for every run request.
    client: WyrdClient,
}

impl Workflows {
    /// Bind the handle to an assembled client.
    #[must_use]
    pub fn new(client: WyrdClient) -> Self {
        Self { client }
    }

    /// Submit a registered Workflow run.
    ///
    /// The transport mints one `Idempotency-Key` before its retry loop, so a
    /// retried submission replays the same key and the server returns the
    /// already-accepted run instead of starting another. First acceptance
    /// answers `202` and a replay `200`; both carry the run snapshot.
    ///
    /// Dropping the future only abandons the local request. Once a submission
    /// has been sent, the server may already have accepted and started the
    /// run even though its snapshot is never observed; nothing is rolled
    /// back. The key lives only for this call, so calling `create` again is a
    /// new submission and can start a second run.
    ///
    /// # Errors
    /// Returns the server's stable request, permission, resolution,
    /// validation, admission, or idempotency-conflict error, or a transport
    /// failure.
    pub async fn create(
        &self,
        request: &CreateWorkflowRunRequest,
    ) -> Result<WorkflowRun, WyrdError> {
        self.client
            .submit_idempotent(Method::POST, WORKFLOW_RUNS, request)
            .await
    }

    /// Read the current snapshot of one run.
    ///
    /// # Errors
    /// Returns the server's permission or not-found error, or a transport
    /// failure.
    pub async fn get(&self, run_id: &WorkflowRunId) -> Result<WorkflowRun, WyrdError> {
        self.client
            .request_json(Method::GET, &run_path(run_id), None::<&()>)
            .await
    }

    /// Request cancellation and return the resulting snapshot.
    ///
    /// Cancelling an already-terminal run returns it unchanged. Because
    /// cancellation is idempotent, a lost answer is safe to request again.
    ///
    /// Dropping the future only abandons the local request. Once the request
    /// has been sent, the server may already have applied the cancellation
    /// even though the resulting snapshot is never observed; read the run with
    /// [`Self::get`] to learn its state.
    ///
    /// # Errors
    /// Returns the server's permission or not-found error, or a transport
    /// failure.
    pub async fn cancel(&self, run_id: &WorkflowRunId) -> Result<WorkflowRun, WyrdError> {
        self.client
            .request_json(
                Method::POST,
                &format!("{}/cancel", run_path(run_id)),
                None::<&()>,
            )
            .await
    }

    /// Poll once per second until the run reaches a terminal status.
    ///
    /// Dropping the future stops polling only; it never cancels or resubmits
    /// the server run.
    ///
    /// # Errors
    /// Returns the first error of [`Self::get`].
    pub async fn wait(&self, run_id: &WorkflowRunId) -> Result<WorkflowRun, WyrdError> {
        loop {
            let run = self.get(run_id).await?;
            if run.status.is_terminal() {
                return Ok(run);
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

/// Route of one run; run IDs are UUIDs and therefore URL-safe.
fn run_path(run_id: &WorkflowRunId) -> String {
    format!("{WORKFLOW_RUNS}/{run_id}")
}

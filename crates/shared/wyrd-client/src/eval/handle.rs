//! The eval pull-protocol handles.

use std::sync::Arc;

use reqwest::Method;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::eval::protocol::{
    AgentTurnSubmission, EvalRunOpenRequest, EvalRunOpenResponse, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::ids::RunId;

use crate::client::WyrdClient;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::{self, Debug, Formatter};

/// Header carrying a run's lease token.
///
/// Matches the server's `x-wyrd-eval-lease`. The lease is a second credential on
/// a request that already carries the caller's Wyrd token, so it needs a header
/// of its own; it is not `Authorization`, which belongs to the calling
/// application and which no Wyrd surface reads.
const EVAL_LEASE_HEADER: &str = "x-wyrd-eval-lease";

/// Cheap-to-clone handle for opening eval runs.
///
/// Shaped like [`Principals`](crate::principals::Principals): one `Arc`-shared
/// authenticated client and discoverable inherent methods. It holds no run
/// state — an opened run is an [`EvalRun`], which carries the identity and lease
/// the rest of the protocol needs.
#[derive(Clone)]
pub struct EvalProtocol {
    /// Shared authenticated client owning transport, credentials, and the
    /// access-token cache.
    client: Arc<WyrdClient>,
}

impl Debug for EvalProtocol {
    /// Prints the handle without its client, which holds credential material.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalProtocol").finish_non_exhaustive()
    }
}

impl EvalProtocol {
    /// Construct a handle around an already assembled client.
    #[must_use]
    pub fn with_client(client: WyrdClient) -> Self {
        Self::with_shared(Arc::new(client))
    }

    /// Construct a handle around a client already shared with another caller.
    ///
    /// Use this when the same connection pool and token cache must also serve a
    /// non-Wyrd endpoint — the eval CLI shares one client between the protocol
    /// calls and the credential-free calls to the agent under evaluation.
    #[must_use]
    pub fn with_shared(client: Arc<WyrdClient>) -> Self {
        Self { client }
    }

    /// Open a run for one Eval card and receive its lease.
    ///
    /// The tenant is never named in the request: the server takes it from the
    /// verified token, so a client cannot open a run against another tenant's
    /// card even by constructing the request by hand.
    ///
    /// # Errors
    /// Returns a Wyrd error when the caller lacks `evals:run`, the referenced
    /// Eval or Data card does not resolve in the caller's tenant, the tenant
    /// has too many open runs, or the server rejects the request.
    pub async fn open(&self, request: &EvalRunOpenRequest) -> Result<EvalRun, WyrdError> {
        let opened: EvalRunOpenResponse = self
            .client
            .request_json(Method::POST, "/v1/eval/runs", Some(request))
            .await?;
        Ok(EvalRun {
            client: Arc::clone(&self.client),
            run_id: opened.run_id,
            lease: format!("Bearer {}", opened.lease_token.as_str()),
        })
    }
}

/// One open eval run, holding the lease that authorizes work on it.
///
/// Every method attaches the lease, so the protocol's second credential exists
/// in exactly one place. The lease is stored pre-formatted as the header value
/// and never exposed, because a caller has no use for it that is not one of
/// these calls.
pub struct EvalRun {
    /// Shared authenticated client, the same one that opened the run.
    client: Arc<WyrdClient>,
    /// Server-assigned run identity, part of every path below.
    run_id: RunId,
    /// The run's lease as a `Bearer <token>` header value.
    lease: String,
}

impl Debug for EvalRun {
    /// Prints the run's identity and withholds its lease, which is a secret.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalRun")
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

impl EvalRun {
    /// Borrow the server-assigned run identity.
    #[must_use]
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Pull the next directive for this run.
    ///
    /// Returns [`TurnDirective::RunComplete`] once every scenario is finished;
    /// a caller drives the loop until it sees that.
    ///
    /// # Errors
    /// Returns a Wyrd error when the lease is rejected, the run is unknown to
    /// the server, or the orchestrator fails.
    pub async fn next_directive(&self) -> Result<TurnDirective, WyrdError> {
        self.leased(Method::POST, "next", Option::<&()>::None).await
    }

    /// Report the agent's answer for one turn, with any observations it emitted.
    ///
    /// # Errors
    /// Returns a Wyrd error when the lease is rejected, the run is unknown, or
    /// the submission does not match the turn the server is waiting on.
    pub async fn submit_agent_turn(
        &self,
        submission: &AgentTurnSubmission,
    ) -> Result<(), WyrdError> {
        self.leased(Method::POST, "agent-turn", Some(submission))
            .await
    }

    /// Supply a client-scripted simulated-user turn.
    ///
    /// # Errors
    /// Returns a Wyrd error when the lease is rejected, the run is unknown, or
    /// the submission does not match the turn the server is waiting on.
    pub async fn submit_user_turn(&self, submission: &UserTurnSubmission) -> Result<(), WyrdError> {
        self.leased(Method::POST, "user-turn", Some(submission))
            .await
    }

    /// Send one run-scoped protocol call with the lease attached.
    ///
    /// The single place this run's path prefix and lease header are assembled.
    ///
    /// # Errors
    /// Returns the stable Wyrd error the server produced, or a transport error
    /// from the shared client.
    async fn leased<S, D>(
        &self,
        method: Method,
        leaf: &str,
        body: Option<&S>,
    ) -> Result<D, WyrdError>
    where
        S: Serialize,
        D: DeserializeOwned,
    {
        let path = format!("/v1/eval/runs/{}/{leaf}", self.run_id);
        self.client
            .http()
            .request_json_with_headers(method, &path, body, &[(EVAL_LEASE_HEADER, &self.lease)])
            .await
    }
}

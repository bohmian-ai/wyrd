//! Authenticated private transport for owner-local Oracle lifecycle controls.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::oracle::dispatcher::{OraclePeerCredentials, BifrostPeerTls};
use wyrd_spec::vala::api::NodeId;
use wyrd_spec::vala::api::{
    CancelOracleLifecycleRequest, CancelOracleLifecycleResponse, ListOracleLifecyclesRequest,
    OracleLifecycleLookupRequest, RunningQuerySummary,
};
use wyrd_tonic::tonic::metadata::MetadataValue;
use wyrd_tonic::tonic::{Code, Request};
use wyrd_tonic::wyrd::v1::oracle_lifecycle_service_client::OracleLifecycleServiceClient;

/// Maximum time one owner-local lifecycle call may occupy during bounded fanout.
const LIFECYCLE_CALL_TIMEOUT: Duration = Duration::from_secs(3);

/// Redacted result of one ready-Oracle lifecycle call.
#[derive(Debug)]
pub enum OracleLifecycleOutcome<T> {
    /// The remote owner returned a typed result.
    Found(T),
    /// This node did not own the requested tenant/request pair.
    Absent,
    /// The node was unavailable, denied the call, or returned an invalid contract.
    Unavailable,
}

/// One current-ready-Oracle result paired only with its opaque node identity.
#[derive(Debug)]
pub struct OracleLifecycleNodeOutcome<T> {
    /// Oracle node contacted from the single membership snapshot.
    pub node_id: NodeId,
    /// Redacted owner-local result.
    pub outcome: OracleLifecycleOutcome<T>,
}

/// Canonical dependency-owning client for private lifecycle fanout.
///
/// The transport snapshots current membership once per operation, excludes the
/// local node, obtains its own platform Service credential, and applies one
/// deadline per peer. Dropping the returned future cancels outstanding calls;
/// completed peers are returned without retrying an ambiguous mutation.
pub struct OracleLifecycleTransport {
    /// Existing authoritative cluster membership owner.
    cluster: Arc<ClusterRegistry>,
    /// Canonical refreshing platform Service credential owner.
    credentials: Arc<dyn OraclePeerCredentials>,
    /// Stable local node excluded from remote fanout.
    local_node_id: NodeId,
    /// Optional immutable peer TLS trust policy.
    tls: Option<BifrostPeerTls>,
}

impl OracleLifecycleTransport {
    /// Constructs a plaintext development transport from canonical boot owners.
    #[must_use]
    pub fn new(
        cluster: Arc<ClusterRegistry>,
        credentials: Arc<dyn OraclePeerCredentials>,
        local_node_id: NodeId,
    ) -> Self {
        Self {
            cluster,
            credentials,
            local_node_id,
            tls: None,
        }
    }

    /// Constructs a TLS-authenticated transport from canonical boot owners.
    #[must_use]
    pub fn with_tls(
        cluster: Arc<ClusterRegistry>,
        credentials: Arc<dyn OraclePeerCredentials>,
        local_node_id: NodeId,
        tls: BifrostPeerTls,
    ) -> Self {
        Self {
            cluster,
            credentials,
            local_node_id,
            tls: Some(tls),
        }
    }

    /// Lists the complete tenant-local registry on every current ready remote Oracle.
    ///
    /// # Errors
    ///
    /// Individual credential, connection, deadline, and decoding failures are
    /// redacted into [`OracleLifecycleOutcome::Unavailable`]. Cancellation drops
    /// unfinished calls while retaining no durable partial progress.
    pub async fn list(
        &self,
        request: ListOracleLifecyclesRequest,
    ) -> Vec<OracleLifecycleNodeOutcome<Vec<RunningQuerySummary>>> {
        let nodes = self.ready_remote_nodes();
        let mut calls = FuturesUnordered::new();
        for (node_id, address) in nodes {
            let request = request.clone();
            calls.push(async move {
                let outcome = self.list_one(&address, request).await;
                OracleLifecycleNodeOutcome { node_id, outcome }
            });
        }
        let mut outcomes = Vec::new();
        while let Some(outcome) = calls.next().await {
            outcomes.push(outcome);
        }
        outcomes
    }

    /// Gets the exact tenant/request pair from every current ready remote Oracle.
    ///
    /// # Errors
    ///
    /// Individual failures are redacted. Dropping the future cancels unfinished
    /// calls and no peer result is persisted by this transport.
    pub async fn get(
        &self,
        request: OracleLifecycleLookupRequest,
    ) -> Vec<OracleLifecycleNodeOutcome<RunningQuerySummary>> {
        let nodes = self.ready_remote_nodes();
        let mut calls = FuturesUnordered::new();
        for (node_id, address) in nodes {
            let request = request.clone();
            calls.push(async move {
                let outcome = self.get_one(&address, request).await;
                OracleLifecycleNodeOutcome { node_id, outcome }
            });
        }
        let mut outcomes = Vec::new();
        while let Some(outcome) = calls.next().await {
            outcomes.push(outcome);
        }
        outcomes
    }

    /// Cancels the exact tenant/request pair on every current ready remote Oracle.
    ///
    /// # Errors
    ///
    /// Individual failures are redacted and are never retried after an
    /// ambiguous mutation. Cancellation can therefore leave partial peer
    /// progress, which the idempotent owner-local cancellation contract absorbs.
    pub async fn cancel(
        &self,
        request: CancelOracleLifecycleRequest,
    ) -> Vec<OracleLifecycleNodeOutcome<CancelOracleLifecycleResponse>> {
        let nodes = self.ready_remote_nodes();
        let mut calls = FuturesUnordered::new();
        for (node_id, address) in nodes {
            let request = request.clone();
            calls.push(async move {
                let outcome = self.cancel_one(&address, request).await;
                OracleLifecycleNodeOutcome { node_id, outcome }
            });
        }
        let mut outcomes = Vec::new();
        while let Some(outcome) = calls.next().await {
            outcomes.push(outcome);
        }
        outcomes
    }

    /// Captures each current ready remote Oracle at most once.
    fn ready_remote_nodes(&self) -> Vec<(NodeId, String)> {
        self.cluster
            .snapshot()
            .live_oracles()
            .into_iter()
            .filter(|lease| lease.ready && lease.key.node_id != self.local_node_id)
            .map(|lease| (lease.key.node_id, lease.address.clone()))
            .collect()
    }

    /// Connects with the immutable TLS policy selected during boot.
    ///
    /// # Errors
    ///
    /// Returns `Err(())` when the configured endpoint or shared TLS policy is
    /// invalid, or when connecting to the selected peer fails. Dropping the
    /// future cancels an in-progress connection without durable side effects.
    async fn client(
        &self,
        address: String,
    ) -> Result<OracleLifecycleServiceClient<wyrd_tonic::tonic::transport::Channel>, ()> {
        let endpoint = self
            .tls
            .as_ref()
            .ok_or(())?
            .endpoint(address)
            .map_err(|_| ())?;
        let channel = endpoint.connect().await.map_err(|_| ())?;
        Ok(OracleLifecycleServiceClient::new(channel))
    }

    /// Builds authenticated metadata solely from the retained credential owner.
    ///
    /// # Errors
    ///
    /// Returns `Err(())` when credential acquisition fails or the returned
    /// bearer cannot be represented as gRPC metadata. Cancellation can abandon
    /// an in-progress credential refresh but persists no transport state.
    async fn authenticated<T>(&self, value: T, force_refresh: bool) -> Result<Request<T>, ()> {
        let bearer = self
            .credentials
            .bearer(force_refresh)
            .await
            .map_err(|_| ())?;
        let metadata: MetadataValue<_> = format!("Bearer {bearer}").parse().map_err(|_| ())?;
        let mut request = Request::new(value);
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", metadata);
        Ok(request)
    }

    /// Performs one bounded owner-local list without exposing transport detail.
    async fn list_one(
        &self,
        address: &str,
        request: ListOracleLifecyclesRequest,
    ) -> OracleLifecycleOutcome<Vec<RunningQuerySummary>> {
        let call = async {
            let mut client = self.client(address.to_owned()).await?;
            let wire: wyrd_tonic::wyrd::v1::ListOracleLifecyclesRequest = request.into();
            let response = match client
                .list_lifecycles(self.authenticated(wire.clone(), false).await?)
                .await
            {
                Err(status) if status.code() == Code::Unauthenticated => client
                    .list_lifecycles(self.authenticated(wire, true).await?)
                    .await
                    .map_err(|_| ())?,
                result => result.map_err(|_| ())?,
            }
            .into_inner();
            wyrd_spec::vala::api::ListOracleLifecyclesResponse::try_from(response).map_err(|_| ())
        };
        match tokio::time::timeout(LIFECYCLE_CALL_TIMEOUT, call).await {
            Ok(Ok(response)) => OracleLifecycleOutcome::Found(response.queries),
            _ => OracleLifecycleOutcome::Unavailable,
        }
    }

    /// Performs one bounded owner-local get and preserves opaque absence.
    async fn get_one(
        &self,
        address: &str,
        request: OracleLifecycleLookupRequest,
    ) -> OracleLifecycleOutcome<RunningQuerySummary> {
        let call = async {
            let mut client = self
                .client(address.to_owned())
                .await
                .map_err(|()| wyrd_tonic::tonic::Status::unavailable("peer unavailable"))?;
            let wire: wyrd_tonic::wyrd::v1::GetOracleLifecycleRequest = request.into();
            let request = self
                .authenticated(wire.clone(), false)
                .await
                .map_err(|()| wyrd_tonic::tonic::Status::unavailable("peer unavailable"))?;
            match client.get_lifecycle(request).await {
                Err(status) if status.code() == Code::Unauthenticated => {
                    let request = self
                        .authenticated(wire, true)
                        .await
                        .map_err(|()| wyrd_tonic::tonic::Status::unavailable("peer unavailable"))?;
                    client.get_lifecycle(request).await
                }
                result => result,
            }
        };
        match tokio::time::timeout(LIFECYCLE_CALL_TIMEOUT, call).await {
            Ok(Ok(response)) => match wyrd_spec::vala::api::GetOracleLifecycleResponse::try_from(
                response.into_inner(),
            ) {
                Ok(response) => OracleLifecycleOutcome::Found(response.query),
                Err(_) => OracleLifecycleOutcome::Unavailable,
            },
            Ok(Err(status)) if status.code() == Code::NotFound => OracleLifecycleOutcome::Absent,
            _ => OracleLifecycleOutcome::Unavailable,
        }
    }

    /// Performs one bounded mutation without retrying ambiguous progress.
    async fn cancel_one(
        &self,
        address: &str,
        request: CancelOracleLifecycleRequest,
    ) -> OracleLifecycleOutcome<CancelOracleLifecycleResponse> {
        let call = async {
            let mut client = self
                .client(address.to_owned())
                .await
                .map_err(|()| wyrd_tonic::tonic::Status::unavailable("peer unavailable"))?;
            let wire: wyrd_tonic::wyrd::v1::CancelOracleLifecycleRequest = request.into();
            let request = self
                .authenticated(wire.clone(), false)
                .await
                .map_err(|()| wyrd_tonic::tonic::Status::unavailable("peer unavailable"))?;
            match client.cancel_lifecycle(request).await {
                Err(status) if status.code() == Code::Unauthenticated => {
                    let request = self
                        .authenticated(wire, true)
                        .await
                        .map_err(|()| wyrd_tonic::tonic::Status::unavailable("peer unavailable"))?;
                    client.cancel_lifecycle(request).await
                }
                result => result,
            }
        };
        match tokio::time::timeout(LIFECYCLE_CALL_TIMEOUT, call).await {
            Ok(Ok(response)) => {
                match CancelOracleLifecycleResponse::try_from(response.into_inner()) {
                    Ok(response) => OracleLifecycleOutcome::Found(response),
                    Err(_) => OracleLifecycleOutcome::Unavailable,
                }
            }
            Ok(Err(status)) if status.code() == Code::NotFound => OracleLifecycleOutcome::Absent,
            _ => OracleLifecycleOutcome::Unavailable,
        }
    }
}

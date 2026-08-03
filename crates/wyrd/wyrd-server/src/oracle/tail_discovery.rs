//! Query-scoped live Scribe discovery backed by fenced cluster membership.

use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::oracle::dispatcher::{OraclePeerCredentials, OraclePeerTls};
use vala_bifrost_redux::oracle::{DiscoveredTailRoute, TailStreamDiscovery};
use vala_bifrost_redux::scribe::tail_rpc::{
    TailReadError, TailReadTransport, TailTicketAudience, TailTicketClaims, TailTicketMinter,
    TonicTailReadTransport,
};
use wyrd_spec::vala::api::{NodeId, TenantTableBinding};

/// Production resolver that owns no persistent route registry.
pub struct RegistryTailStreamDiscovery {
    /// Authoritative role membership owner.
    cluster: Arc<ClusterRegistry>,
    /// Short-lived private service credential provider.
    credentials: Arc<dyn OraclePeerCredentials>,
    /// Shared Oracle/Scribe private-service trust material.
    tls: Option<OraclePeerTls>,
    /// Server-owned domain-separated ticket signer.
    minter: Arc<dyn TailTicketMinter>,
    /// Local node identity used to select an optional zero-copy transport.
    local_node: NodeId,
    /// Optional local authorized transport.
    local_transport: Option<Arc<dyn TailReadTransport>>,
    /// Test-tier private discovery outage switch.
    #[cfg(feature = "test-support")]
    unavailable: AtomicBool,
}

impl RegistryTailStreamDiscovery {
    /// Creates a resolver over one authoritative membership registry.
    #[must_use]
    pub fn new(
        cluster: Arc<ClusterRegistry>,
        credentials: Arc<dyn OraclePeerCredentials>,
        tls: Option<OraclePeerTls>,
        minter: Arc<dyn TailTicketMinter>,
        local_node: NodeId,
        local_transport: Option<Arc<dyn TailReadTransport>>,
    ) -> Self {
        Self {
            cluster,
            credentials,
            tls,
            minter,
            local_node,
            local_transport,
            #[cfg(feature = "test-support")]
            unavailable: AtomicBool::new(false),
        }
    }

    /// Connects to one ready Scribe endpoint with the bounded private bearer.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when endpoint construction, connection,
    /// credential exchange, or metadata encoding fails before listing.
    async fn remote_transport(
        &self,
        address: &str,
        deadline: Instant,
    ) -> Result<Arc<dyn TailReadTransport>, TailReadError> {
        let endpoint = if let Some(tls) = &self.tls {
            wyrd_tonic::transport::authenticated_tls_endpoint(
                address.to_owned(),
                tls.ca_certificate_pem(),
                tls.server_name().to_owned(),
            )
            .map_err(|_| TailReadError::State {
                detail: "tail TLS endpoint is invalid".to_owned(),
            })?
        } else {
            wyrd_tonic::transport::plaintext_endpoint(address.to_owned()).map_err(|_| {
                TailReadError::State {
                    detail: "tail endpoint is invalid".to_owned(),
                }
            })?
        };
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(TailReadError::DeadlineElapsed)?;
        let channel = tokio::time::timeout(remaining, endpoint.connect())
            .await
            .map_err(|_| TailReadError::DeadlineElapsed)?
            .map_err(|_| TailReadError::State {
                detail: "tail endpoint connection failed".to_owned(),
            })?;
        let bearer = self
            .credentials
            .bearer(false)
            .await
            .map_err(|_| TailReadError::State {
                detail: "tail credential exchange failed".to_owned(),
            })?;
        let client =
            wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient::new(channel);
        Ok(Arc::new(
            TonicTailReadTransport::new(client, &bearer).map_err(|_| TailReadError::State {
                detail: "tail credential metadata is invalid".to_owned(),
            })?,
        ))
    }
}

#[async_trait::async_trait]
impl TailStreamDiscovery for RegistryTailStreamDiscovery {
    /// Refreshes membership, lists active streams, and returns ephemeral routes.
    ///
    /// # Errors
    /// Returns [`TailReadError`] when membership, authentication, listing, or
    /// returned stream identity validation fails before a route is published.
    async fn discover(
        &self,
        binding: &TenantTableBinding,
        query_id: uuid::Uuid,
        deadline: Instant,
    ) -> Result<Vec<DiscoveredTailRoute>, TailReadError> {
        #[cfg(feature = "test-support")]
        if self.unavailable.load(Ordering::Acquire) {
            return Err(TailReadError::State {
                detail: "injected private Scribe discovery outage".to_owned(),
            });
        }
        if deadline <= Instant::now() {
            return Err(TailReadError::DeadlineElapsed);
        }
        self.cluster
            .refresh_snapshot()
            .await
            .map_err(|_| TailReadError::State {
                detail: "cluster snapshot refresh failed".to_owned(),
            })?;
        let snapshot = self.cluster.snapshot();
        let live_scribes = snapshot.live_scribes();
        tracing::debug!(
            query_id = %query_id,
            table = %binding.table,
            live_scribe_count = live_scribes.len(),
            "discovered live Scribe membership"
        );
        let mut routes = Vec::new();
        let canonical = if binding.namespace.starts_with("vala.") {
            format!("{}.{}", binding.namespace, binding.table)
        } else {
            format!("vala.{}.{}", binding.namespace, binding.table)
        };
        for lease in live_scribes {
            if !lease.ready {
                continue;
            }
            let node_id = lease.key.node_id;
            let transport = if node_id == self.local_node {
                if let Some(transport) = self.local_transport.clone() {
                    transport
                } else {
                    self.remote_transport(&lease.address, deadline).await?
                }
            } else {
                self.remote_transport(&lease.address, deadline).await?
            };
            let epoch = lease.fencing_token;
            let ticket = self
                .minter
                .mint_tail_ticket(&TailTicketClaims {
                    query_id,
                    tenant_id: binding.tenant_id,
                    canonical_table: canonical.clone(),
                    node_id: node_id.as_uuid(),
                    writer_epoch: epoch,
                    deadline: chrono::Utc::now()
                        + chrono::Duration::from_std(
                            deadline.saturating_duration_since(Instant::now()),
                        )
                        .map_err(|_| TailReadError::DeadlineElapsed)?,
                    audience: TailTicketAudience::List,
                    nonce: uuid::Uuid::now_v7().as_bytes().to_vec(),
                })
                .map_err(|_| TailReadError::State {
                    detail: "tail ticket mint failed".to_owned(),
                })?;
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(TailReadError::DeadlineElapsed)?;
            let streams = tokio::time::timeout(
                remaining,
                transport.list_active_streams(binding.clone(), query_id, ticket),
            )
            .await
            .map_err(|_| TailReadError::DeadlineElapsed)?
            .map_err(|error| {
                tracing::warn!(error = ?error, node_id = %node_id.as_uuid(), "private Scribe tail listing failed");
                TailReadError::State {
                    detail: "tail stream listing failed".to_owned(),
                }
            })?;
            for stream in streams {
                if stream.stream.node_id != node_id || stream.stream.writer_epoch != epoch {
                    return Err(TailReadError::State {
                        detail: "tail stream identity is stale".to_owned(),
                    });
                }
                routes.push(DiscoveredTailRoute {
                    event_day: stream.event_day,
                    stream: stream.stream,
                    transport: Arc::clone(&transport),
                });
            }
        }
        Ok(routes)
    }

    /// Injects or clears the private discovery outage used by test journeys.
    #[cfg(feature = "test-support")]
    fn set_unavailable_for_test(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::Release);
    }
}

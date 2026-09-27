//! Query-scoped live Scribe discovery backed by fenced cluster membership.

use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use vala_bifrost_redux::cluster::ClusterRegistry;
use vala_bifrost_redux::oracle::dispatcher::{BifrostPeerTls, OraclePeerCredentials};
use vala_bifrost_redux::oracle::{DiscoveredTailRoute, TailStreamDiscovery};
use vala_bifrost_redux::scribe::tail_rpc::{
    TailReadError, TailTicketAudience, TailTicketClaims, TailTicketMinter, TonicTailReadTransport,
};
use wyrd_spec::vala::api::TenantTableBinding;

/// One-shot switch that sends the next listing without its tail ticket.
///
/// A journey needs a listing the ready Scribe itself refuses for a credential
/// reason, through its real verifier and status mapping, to prove Oracle fails
/// rather than degrades on a trust-boundary refusal.
#[cfg(feature = "test-support")]
static REJECT_NEXT_LISTING_TICKET: AtomicBool = AtomicBool::new(false);

/// Arms the next private listing in this process to carry an empty ticket,
/// which the receiving Scribe refuses as unauthorized.
#[cfg(feature = "test-support")]
pub fn arm_tail_listing_ticket_rejection_for_test() {
    REJECT_NEXT_LISTING_TICKET.store(true, Ordering::Release);
}

/// Production resolver that owns no persistent route registry.
pub struct RegistryTailStreamDiscovery {
    /// Authoritative role membership owner.
    cluster: Arc<ClusterRegistry>,
    /// Short-lived private service credential provider.
    credentials: Arc<dyn OraclePeerCredentials>,
    /// Shared Oracle/Scribe private-service trust material.
    tls: Option<BifrostPeerTls>,
    /// Server-owned domain-separated ticket signer.
    minter: Arc<dyn TailTicketMinter>,
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
        tls: Option<BifrostPeerTls>,
        minter: Arc<dyn TailTicketMinter>,
    ) -> Self {
        Self {
            cluster,
            credentials,
            tls,
            minter,
            #[cfg(feature = "test-support")]
            unavailable: AtomicBool::new(false),
        }
    }

    /// Connects to one ready Scribe endpoint with the bounded private bearer.
    ///
    /// # Errors
    /// Returns [`TailReadError::Unavailable`] when the ready endpoint cannot be
    /// connected, [`TailReadError::DeadlineElapsed`] when connecting outlasts
    /// the deadline, [`TailReadError::Authorization`] when this node's workload
    /// credential cannot be issued or encoded, and [`TailReadError::State`]
    /// when the peer identity or endpoint is misconfigured.
    async fn remote_transport(
        &self,
        address: &str,
        deadline: Instant,
    ) -> Result<TonicTailReadTransport, TailReadError> {
        let endpoint = self
            .tls
            .as_ref()
            .ok_or_else(|| TailReadError::State {
                detail: "tail transport requires the Bifrost peer identity".to_owned(),
            })?
            .endpoint(address.to_owned())
            .map_err(|_| TailReadError::State {
                detail: "tail TLS endpoint is invalid".to_owned(),
            })?;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(TailReadError::DeadlineElapsed)?;
        let channel = tokio::time::timeout(remaining, endpoint.connect())
            .await
            .map_err(|_| TailReadError::DeadlineElapsed)?
            .map_err(|_| TailReadError::Unavailable {
                detail: "tail endpoint connection failed".to_owned(),
            })?;
        let bearer =
            self.credentials
                .bearer(false)
                .await
                .map_err(|_| TailReadError::Authorization {
                    detail: "tail credential exchange failed".to_owned(),
                })?;
        let client =
            wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient::new(channel);
        TonicTailReadTransport::new(client, &bearer).map_err(|_| TailReadError::Authorization {
            detail: "tail credential metadata is invalid".to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl TailStreamDiscovery for RegistryTailStreamDiscovery {
    /// Refreshes membership, lists active streams, and returns ephemeral routes.
    ///
    /// # Errors
    /// Returns the listing's own [`TailReadError`] class unchanged:
    /// [`TailReadError::Unavailable`] only when a ready Scribe cannot be
    /// reached or refuses as unavailable, [`TailReadError::StaleIdentity`]
    /// when a listed stream names another incarnation or epoch,
    /// [`TailReadError::DeadlineElapsed`] at the deadline, and a fatal
    /// authorization, binding, or state class for membership refresh, ticket
    /// mint, credential, tenant, binding, or malformed-response failures.
    async fn discover(
        &self,
        binding: &TenantTableBinding,
        query_id: uuid::Uuid,
        deadline: Instant,
    ) -> Result<Vec<DiscoveredTailRoute>, TailReadError> {
        #[cfg(feature = "test-support")]
        if self.unavailable.load(Ordering::Acquire) {
            return Err(TailReadError::Unavailable {
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
            let transport = self.remote_transport(&lease.address, deadline).await?;
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
            #[cfg(feature = "test-support")]
            let ticket = if REJECT_NEXT_LISTING_TICKET.swap(false, Ordering::AcqRel) {
                Vec::new()
            } else {
                ticket
            };
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(TailReadError::DeadlineElapsed)?;
            let streams = tokio::time::timeout(
                remaining,
                transport.list_active_streams(binding.clone(), query_id, ticket),
            )
            .await
            .map_err(|_| TailReadError::DeadlineElapsed)?
            .inspect_err(|error| {
                tracing::warn!(error = ?error, node_id = %node_id.as_uuid(), "private Scribe tail listing failed");
            })?;
            for stream in streams {
                if stream.stream.node_id != node_id || stream.stream.writer_epoch != epoch {
                    return Err(TailReadError::StaleIdentity);
                }
                routes.push(DiscoveredTailRoute {
                    time_partition: stream.time_partition,
                    stream: stream.stream,
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

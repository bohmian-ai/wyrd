//! Query-scoped live Scribe discovery over the attempt's frozen Scribe roster.
//!
//! Listing returns partition metadata, never rows. It trusts the mutually
//! authenticated `wyrd-peer` channel alone; row-serving fragments additionally
//! carry the receiver-validated typed peer context. A process-local node has
//! no peer channel: its roster names only its own Scribe, listed in-process.

use std::collections::HashMap;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
use vala_bifrost_redux::oracle::{
    DiscoveredTailRoute, OracleQueryParticipant, TailStreamDiscovery,
};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailService, TailReadError, TonicTailReadTransport,
};
use wyrd_spec::vala::api::{NodeId, TenantTableBinding};
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

/// Count of upcoming listings in this process that answer with a stale identity.
///
/// A journey needs Oracle to see a writer-epoch change between roster freeze
/// and listing, which a real cluster cannot time deterministically, to prove
/// the attempt restarts once on a refrozen roster and then degrades.
#[cfg(feature = "test-support")]
static STALE_LISTINGS: AtomicUsize = AtomicUsize::new(0);

/// One-shot switch that holds the next listing until its deadline expires.
///
/// The stall runs inside the real deadline-bounded listing future, so a
/// journey proves the query deadline cancels in-flight listing RPCs.
#[cfg(feature = "test-support")]
static STALL_NEXT_LISTING: AtomicBool = AtomicBool::new(false);

/// Arms the next `count` listings in this process to answer as stale.
///
/// Each armed discovery returns [`TailReadError::StaleIdentity`] without
/// contacting a Scribe, exactly as a listed stream from another writer epoch
/// would.
#[cfg(feature = "test-support")]
pub fn arm_tail_listing_stale_for_test(count: usize) {
    STALE_LISTINGS.store(count, Ordering::Release);
}

/// Arms the next listing in this process to stall until its deadline.
#[cfg(feature = "test-support")]
pub fn arm_tail_listing_stall_for_test() {
    STALL_NEXT_LISTING.store(true, Ordering::Release);
}

/// Production resolver that lists the Scribes one query attempt froze.
///
/// It owns no route registry. Its only state is one reusable tonic channel per
/// Scribe node, so repeated queries do not pay a TLS handshake per listing.
pub struct RegistryTailStreamDiscovery {
    /// Shared Oracle/Scribe private-service trust material.
    tls: Option<BifrostPeerTls>,
    /// Co-located Scribe live source, listed in-process when `tls` is absent.
    local: Option<Arc<FetchLiveTailService>>,
    /// Reusable channel per Scribe node, keyed with the endpoint it dials.
    ///
    /// An entry is replaced when the frozen roster names another endpoint for
    /// the node and dropped when the node leaves the listed roster.
    channels: Mutex<HashMap<NodeId, (String, Channel)>>,
    /// Test-tier private discovery outage switch.
    #[cfg(feature = "test-support")]
    unavailable: AtomicBool,
}

impl RegistryTailStreamDiscovery {
    /// Creates a resolver over the private mTLS peer trust.
    ///
    /// `local` is this process's own Scribe live source. Without `tls` the
    /// node serves no peer listener, so its own Scribe is listed in-process
    /// and any other frozen Scribe is unreachable.
    #[must_use]
    pub fn new(tls: Option<BifrostPeerTls>, local: Option<Arc<FetchLiveTailService>>) -> Self {
        Self {
            tls,
            local,
            channels: Mutex::new(HashMap::new()),
            #[cfg(feature = "test-support")]
            unavailable: AtomicBool::new(false),
        }
    }

    /// Returns the reusable channel for each frozen Scribe, in roster order.
    ///
    /// A cached channel is reused while its endpoint matches; otherwise a lazy
    /// channel is built, so a connect failure surfaces on the listing RPC as
    /// `Unavailable`. Nodes absent from `scribes` are evicted.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] when the peer identity is missing, an
    /// endpoint is invalid, or the channel map lock is poisoned.
    fn channels_for(
        &self,
        scribes: &[OracleQueryParticipant],
    ) -> Result<Vec<Channel>, TailReadError> {
        let tls = self.tls.as_ref().ok_or_else(|| TailReadError::State {
            detail: "tail transport requires the Bifrost peer identity".to_owned(),
        })?;
        let mut channels = self.channels.lock().map_err(|_| TailReadError::State {
            detail: "tail channel map lock is poisoned".to_owned(),
        })?;
        channels.retain(|node_id, _| scribes.iter().any(|scribe| scribe.node_id == *node_id));
        scribes
            .iter()
            .map(|scribe| {
                if let Some((endpoint, channel)) = channels.get(&scribe.node_id)
                    && *endpoint == scribe.endpoint
                {
                    return Ok(channel.clone());
                }
                let channel = tls
                    .endpoint(scribe.endpoint.clone())
                    .map_err(|_| TailReadError::State {
                        detail: "tail TLS endpoint is invalid".to_owned(),
                    })?
                    .connect_lazy();
                channels.insert(scribe.node_id, (scribe.endpoint.clone(), channel.clone()));
                Ok(channel)
            })
            .collect()
    }

    /// Lists one frozen Scribe and checks every stream against its fence.
    ///
    /// # Errors
    /// Returns the listing's [`TailReadError`] class, or
    /// [`TailReadError::StaleIdentity`] when a stream names another node or
    /// writer epoch than the frozen participant.
    async fn list_one(
        scribe: &OracleQueryParticipant,
        channel: Channel,
        binding: &TenantTableBinding,
    ) -> Result<Vec<DiscoveredTailRoute>, TailReadError> {
        let transport = TonicTailReadTransport::new(ScribeTailServiceClient::new(channel));
        let streams = transport
            .list_active_streams(binding.clone())
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    error = ?error,
                    node_id = %scribe.node_id.as_uuid(),
                    "private Scribe tail listing failed"
                );
            })?;
        streams
            .into_iter()
            .map(|stream| Self::route(scribe, stream.time_partition, stream.stream))
            .collect()
    }

    /// Lists the frozen roster in-process on a node with no peer channel.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] when the roster names a Scribe other
    /// than this process's own, and the local listing or
    /// [`TailReadError::StaleIdentity`] class otherwise.
    fn list_local(
        local: &FetchLiveTailService,
        scribes: &[OracleQueryParticipant],
        binding: &TenantTableBinding,
    ) -> Result<Vec<DiscoveredTailRoute>, TailReadError> {
        let mut routes = Vec::new();
        for scribe in scribes {
            if scribe.node_id.as_uuid() != local.stream().node_id.as_uuid() {
                return Err(TailReadError::State {
                    detail: "tail transport requires the Bifrost peer identity".to_owned(),
                });
            }
            for (time_partition, stream) in local.list_active_streams(binding)? {
                routes.push(Self::route(scribe, time_partition, stream)?);
            }
        }
        Ok(routes)
    }

    /// Admits one listed stream only when it names the frozen incarnation.
    ///
    /// # Errors
    /// Returns [`TailReadError::StaleIdentity`] when the stream names another
    /// node or writer epoch than the frozen participant.
    fn route(
        scribe: &OracleQueryParticipant,
        time_partition: wyrd_spec::vala::api::TimePartitionWire,
        stream: wyrd_spec::vala::api::TailStreamIdentity,
    ) -> Result<DiscoveredTailRoute, TailReadError> {
        if stream.node_id != scribe.node_id || stream.writer_epoch != scribe.fencing_token {
            return Err(TailReadError::StaleIdentity);
        }
        Ok(DiscoveredTailRoute {
            time_partition,
            stream,
        })
    }
}

#[async_trait::async_trait]
impl TailStreamDiscovery for RegistryTailStreamDiscovery {
    /// Lists every frozen Scribe concurrently and returns ephemeral routes.
    ///
    /// All listings share one deadline; the first failure in roster order
    /// decides the result, exactly as a sequential walk would.
    ///
    /// # Errors
    /// Returns the listing's own [`TailReadError`] class unchanged:
    /// [`TailReadError::Unavailable`] only when a Scribe cannot be reached or
    /// refuses as unavailable, [`TailReadError::StaleIdentity`] when a listed
    /// stream names another incarnation or epoch than the frozen roster,
    /// [`TailReadError::DeadlineElapsed`] at the deadline, and a fatal
    /// authorization, binding, or state class for peer, tenant, binding,
    /// or malformed-response failures.
    async fn discover(
        &self,
        binding: &TenantTableBinding,
        scribes: &[OracleQueryParticipant],
        deadline: Instant,
    ) -> Result<Vec<DiscoveredTailRoute>, TailReadError> {
        #[cfg(feature = "test-support")]
        if self.unavailable.load(Ordering::Acquire) {
            return Err(TailReadError::Unavailable {
                detail: "injected private Scribe discovery outage".to_owned(),
            });
        }
        #[cfg(feature = "test-support")]
        if STALE_LISTINGS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return Err(TailReadError::StaleIdentity);
        }
        if scribes.is_empty() {
            return Ok(Vec::new());
        }
        if self.tls.is_none()
            && let Some(local) = &self.local
        {
            return Self::list_local(local, scribes, binding);
        }
        let channels = self.channels_for(scribes)?;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(TailReadError::DeadlineElapsed)?;
        let listings = futures_util::future::join_all(
            scribes
                .iter()
                .zip(channels)
                .map(|(scribe, channel)| Self::list_one(scribe, channel, binding)),
        );
        #[cfg(feature = "test-support")]
        let listings = async {
            if STALL_NEXT_LISTING.swap(false, Ordering::AcqRel) {
                std::future::pending::<()>().await;
            }
            listings.await
        };
        let listed = tokio::time::timeout(remaining, listings)
            .await
            .map_err(|_| TailReadError::DeadlineElapsed)?;
        tracing::debug!(
            table = %binding.table,
            scribe_count = scribes.len(),
            "listed frozen Scribe roster"
        );
        let mut routes = Vec::new();
        for listing in listed {
            routes.extend(listing?);
        }
        Ok(routes)
    }

    /// Injects or clears the private discovery outage used by test journeys.
    #[cfg(feature = "test-support")]
    fn set_unavailable_for_test(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::Release);
    }
}

//! Query-scoped live Scribe discovery over the attempt's frozen Scribe roster.
//!
//! Listing returns partition metadata and retains a query-bound capture on
//! each Scribe, never rows. It trusts the mutually
//! authenticated `wyrd-peer` channel alone; row-serving fragments additionally
//! carry the receiver-validated typed peer context. A process-local node has
//! no peer channel: its roster names only its own Scribe, listed in-process.

use std::collections::HashMap;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
use vala_bifrost_redux::oracle::{
    DiscoveredTail, DiscoveredTailRoute, OracleQueryParticipant, TailStreamDiscovery,
};
use vala_bifrost_redux::scan_sizing::ScanScope;
use vala_bifrost_redux::scribe::tail_rpc::{
    ActiveTailStream, FetchLiveTailService, TailCapture, TailReadError, TonicTailReadTransport,
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
        self.channels
            .lock()
            .map_err(|_| TailReadError::State {
                detail: "tail channel map lock is poisoned".to_owned(),
            })?
            .retain(|node_id, _| scribes.iter().any(|scribe| scribe.node_id == *node_id));
        self.cached_channels(scribes)
    }

    /// Returns the reusable channel for each named Scribe without evicting
    /// any other node, so releasing a subset keeps the roster's channels.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] when the peer identity is missing, an
    /// endpoint is invalid, or the channel map lock is poisoned.
    fn cached_channels(
        &self,
        scribes: &[OracleQueryParticipant],
    ) -> Result<Vec<Channel>, TailReadError> {
        let tls = self.tls.as_ref().ok_or_else(|| TailReadError::State {
            detail: "tail transport requires the Bifrost peer identity".to_owned(),
        })?;
        let mut channels = self.channels.lock().map_err(|_| TailReadError::State {
            detail: "tail channel map lock is poisoned".to_owned(),
        })?;
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

    /// Captures one frozen Scribe under `scope` for `query_id` until
    /// `expires_at`.
    ///
    /// # Errors
    /// Returns the listing's [`TailReadError`] class.
    async fn list_one(
        scribe: &OracleQueryParticipant,
        channel: Channel,
        binding: &TenantTableBinding,
        scope: &ScanScope,
        query_id: uuid::Uuid,
        expires_at: SystemTime,
    ) -> Result<TailCapture, TailReadError> {
        TonicTailReadTransport::new(ScribeTailServiceClient::new(channel))
            .list_active_streams(binding.clone(), scope, query_id, expires_at)
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    error = ?error,
                    node_id = %scribe.node_id.as_uuid(),
                    "private Scribe tail listing failed"
                );
            })
    }

    /// Captures the frozen roster in-process under `scope` on a node with no
    /// peer channel, returning each participant's capture in roster order.
    ///
    /// # Errors
    /// Returns [`TailReadError::State`] when the roster names a Scribe other
    /// than this process's own, and the local capture's class otherwise.
    async fn list_local(
        local: &FetchLiveTailService,
        scribes: &[OracleQueryParticipant],
        binding: &TenantTableBinding,
        scope: &ScanScope,
        query_id: uuid::Uuid,
        expires_at: SystemTime,
    ) -> Vec<Result<TailCapture, TailReadError>> {
        let mut captures = Vec::with_capacity(scribes.len());
        for scribe in scribes {
            if scribe.node_id.as_uuid() != local.stream().node_id.as_uuid() {
                captures.push(Err(TailReadError::State {
                    detail: "tail transport requires the Bifrost peer identity".to_owned(),
                }));
                continue;
            }
            captures.push(
                local
                    .capture_active_streams(binding, scope, query_id, expires_at)
                    .await,
            );
        }
        captures
    }

    /// Admits one listed stream only when it names the frozen incarnation.
    ///
    /// # Errors
    /// Returns [`TailReadError::StaleIdentity`] when the stream names another
    /// node or writer epoch than the frozen participant.
    fn route(
        scribe: &OracleQueryParticipant,
        listed: ActiveTailStream,
        capture_id: uuid::Uuid,
    ) -> Result<DiscoveredTailRoute, TailReadError> {
        let ActiveTailStream {
            time_partition,
            stream,
            bytes,
        } = listed;
        if stream.node_id != scribe.node_id || stream.writer_epoch != scribe.fencing_token {
            return Err(TailReadError::StaleIdentity);
        }
        Ok(DiscoveredTailRoute {
            time_partition,
            stream,
            bytes,
            capture_id,
        })
    }

    /// Admits every roster participant's capture, or releases all of them.
    ///
    /// Every answering participant's publication revision is kept, an empty
    /// capture included. The first failure in roster order decides the
    /// result, exactly as a sequential walk would; every capture another
    /// listing retained is released before it returns.
    ///
    /// # Errors
    /// Returns the first listing failure or [`TailReadError::StaleIdentity`]
    /// for a stream that names another incarnation than its participant.
    async fn admit(
        &self,
        binding: &TenantTableBinding,
        scribes: &[OracleQueryParticipant],
        query_id: uuid::Uuid,
        listed: Vec<Result<TailCapture, TailReadError>>,
    ) -> Result<DiscoveredTail, TailReadError> {
        let mut routes = Vec::new();
        let mut revisions = Vec::with_capacity(scribes.len());
        let mut retained = Vec::new();
        let mut failure = None;
        for (scribe, listing) in scribes.iter().zip(listed) {
            let capture = match listing {
                Ok(capture) => capture,
                Err(error) => {
                    failure.get_or_insert(error);
                    continue;
                }
            };
            revisions.push((scribe.node_id.as_uuid(), capture.publication_revision));
            let Some(capture_id) = capture.capture_id else {
                continue;
            };
            retained.push((scribe.clone(), capture_id));
            for stream in capture.streams {
                match Self::route(scribe, stream, capture_id) {
                    Ok(route) => routes.push(route),
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                }
            }
        }
        let Some(error) = failure else {
            return Ok(DiscoveredTail { routes, revisions });
        };
        self.release(binding, query_id, &retained).await;
        Err(error)
    }
}

#[async_trait::async_trait]
impl TailStreamDiscovery for RegistryTailStreamDiscovery {
    /// Captures every frozen Scribe concurrently and returns the routes of
    /// those captures.
    ///
    /// All listings share one deadline, which is also every capture's expiry;
    /// the first failure in roster order decides the result, exactly as a
    /// sequential walk would. Captures that the other Scribes retained are
    /// released before a failure returns; a listing still in flight at the
    /// deadline leaves its capture to expire.
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
        scope: &ScanScope,
        scribes: &[OracleQueryParticipant],
        query_id: uuid::Uuid,
        deadline: Instant,
    ) -> Result<DiscoveredTail, TailReadError> {
        #[cfg(feature = "test-support")]
        if self.unavailable.load(Ordering::Acquire) {
            return Err(TailReadError::Unavailable {
                detail: "injected private Scribe discovery outage".to_owned(),
            });
        }
        #[cfg(feature = "test-support")]
        if STALE_LISTINGS
            .try_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return Err(TailReadError::StaleIdentity);
        }
        if scribes.is_empty() {
            return Ok(DiscoveredTail::default());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(TailReadError::DeadlineElapsed)?;
        let expires_at = SystemTime::now() + remaining;
        if self.tls.is_none()
            && let Some(local) = &self.local
        {
            let listed =
                Self::list_local(local, scribes, binding, scope, query_id, expires_at).await;
            return self.admit(binding, scribes, query_id, listed).await;
        }
        let channels = self.channels_for(scribes)?;
        let listings = futures_util::future::join_all(scribes.iter().zip(channels).map(
            |(scribe, channel)| {
                Self::list_one(scribe, channel, binding, scope, query_id, expires_at)
            },
        ));
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
        self.admit(binding, scribes, query_id, listed).await
    }

    /// Releases each capture on its Scribe concurrently, in-process for the
    /// local Scribe on a node with no peer channel.
    ///
    /// A failed release is logged; the capture then expires at its deadline.
    async fn release(
        &self,
        binding: &TenantTableBinding,
        query_id: uuid::Uuid,
        captures: &[(OracleQueryParticipant, uuid::Uuid)],
    ) {
        if captures.is_empty() {
            return;
        }
        if self.tls.is_none()
            && let Some(local) = &self.local
        {
            for (_, capture_id) in captures {
                if let Err(error) = local.release_capture(binding, query_id, *capture_id) {
                    tracing::warn!(?error, "local Scribe capture release failed");
                }
            }
            return;
        }
        let scribes = captures
            .iter()
            .map(|(scribe, _)| scribe.clone())
            .collect::<Vec<_>>();
        let channels = match self.cached_channels(&scribes) {
            Ok(channels) => channels,
            Err(error) => {
                tracing::warn!(?error, "Scribe capture release found no channel");
                return;
            }
        };
        futures_util::future::join_all(captures.iter().zip(channels).map(
            |((scribe, capture_id), channel)| async move {
                let released = TonicTailReadTransport::new(ScribeTailServiceClient::new(channel))
                    .release_capture(binding.clone(), query_id, *capture_id)
                    .await;
                if let Err(error) = released {
                    tracing::warn!(
                        ?error,
                        node_id = %scribe.node_id.as_uuid(),
                        "private Scribe capture release failed"
                    );
                }
            },
        ))
        .await;
    }

    /// Injects or clears the private discovery outage used by test journeys.
    #[cfg(feature = "test-support")]
    fn set_unavailable_for_test(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::Release);
    }
}

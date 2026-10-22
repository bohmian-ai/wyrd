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
    /// listing retained is released before it returns, waiting no later than
    /// `deadline`, after which an unreleased capture expires on its Scribe.
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
        deadline: Instant,
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
        self.release(binding, query_id, &retained, deadline).await;
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
            return self
                .admit(binding, scribes, query_id, listed, deadline)
                .await;
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
        self.admit(binding, scribes, query_id, listed, deadline)
            .await
    }

    /// Releases each capture on its Scribe concurrently, in-process for the
    /// local Scribe on a node with no peer channel.
    ///
    /// The peer RPCs share the query `deadline`: a Scribe that has not
    /// answered by then is abandoned, cancelling its in-flight RPC. A failed
    /// or abandoned release is logged; the capture then expires at that
    /// deadline on its Scribe.
    async fn release(
        &self,
        binding: &TenantTableBinding,
        query_id: uuid::Uuid,
        captures: &[(OracleQueryParticipant, uuid::Uuid)],
        deadline: Instant,
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
        let releases = futures_util::future::join_all(captures.iter().zip(channels).map(
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
        ));
        if tokio::time::timeout_at(deadline.into(), releases)
            .await
            .is_err()
        {
            tracing::warn!(
                capture_count = captures.len(),
                "Scribe capture release reached the query deadline"
            );
        }
    }

    /// Injects or clears the private discovery outage used by test journeys.
    #[cfg(feature = "test-support")]
    fn set_unavailable_for_test(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::Release);
    }
}

/// Proves retained-capture cleanup settles by the query deadline.
///
/// A real mutually authenticated loopback Scribe, issued by the test peer CA,
/// accepts every `release_capture` call and never answers it. Admission then
/// fails after one capture was retained, and its cleanup release, like the
/// release when live routes drop, must still return by the original query
/// deadline. The Scribe's counter shows each release reached the peer, and the
/// unanswered capture is left to its own expiry there.
#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use vala_bifrost_redux::oracle::dispatcher::BifrostPeerTls;
    use vala_bifrost_redux::oracle::{OracleQueryParticipant, TailStreamDiscovery as _};
    use vala_bifrost_redux::scribe::tail_rpc::{TailCapture, TailReadError};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::api::{
        ClusterCapabilities, ClusterRole, NodeId, ScribeCapabilitiesV1, TenantTableBinding,
    };
    use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
    use wyrd_tonic::tonic::{Request, Response, Status};
    use wyrd_tonic::wyrd::v1::scribe_tail_service_server::{
        ScribeTailService, ScribeTailServiceServer,
    };
    use wyrd_tonic::wyrd::v1::{
        ListActiveStreamsRequest, ListActiveStreamsResponse, ReleaseTailCaptureRequest,
        ReleaseTailCaptureResponse,
    };

    use super::RegistryTailStreamDiscovery;

    /// A peer Scribe whose release RPC is received and never answered.
    #[derive(Clone, Default)]
    struct StalledRelease {
        /// Release calls received.
        received: Arc<AtomicUsize>,
    }

    #[wyrd_tonic::tonic::async_trait]
    impl ScribeTailService for StalledRelease {
        /// Refuses listing; the proof drives admission with a supplied capture.
        ///
        /// # Errors
        ///
        /// Always returns an unimplemented status.
        async fn list_active_streams(
            &self,
            _request: Request<ListActiveStreamsRequest>,
        ) -> Result<Response<ListActiveStreamsResponse>, Status> {
            Err(Status::unimplemented(
                "listing is not served by this fixture",
            ))
        }

        /// Counts the release and never returns.
        ///
        /// # Errors
        ///
        /// Never returns.
        async fn release_capture(
            &self,
            _request: Request<ReleaseTailCaptureRequest>,
        ) -> Result<Response<ReleaseTailCaptureResponse>, Status> {
            self.received.fetch_add(1, Ordering::AcqRel);
            std::future::pending().await
        }
    }

    /// Serves `service` over mutual TLS under `ca` on a loopback port and
    /// returns the Scribe participant answering there.
    ///
    /// # Panics
    ///
    /// Panics when the listener, leaf, or TLS server cannot start.
    async fn serve_scribe(ca: &BifrostPeerCa, service: StalledRelease) -> OracleQueryParticipant {
        let leaf = ca.issue_leaf("scribe").expect("peer leaf");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("peer listener binds");
        let port = listener.local_addr().expect("listener address").port();
        let incoming = async_stream::stream! {
            loop {
                yield listener.accept().await.map(|(stream, _)| stream);
            }
        };
        let server = wyrd_tonic::server::mutual_tls_server(
            wyrd_tonic::server::MutualTlsServerConfig::from_pem(
                leaf.certificate_pem().as_bytes(),
                leaf.private_key_pem().as_bytes(),
                ca.ca_certificate_pem().as_bytes(),
            ),
        )
        .expect("mutual TLS server builds");
        tokio::spawn(
            server
                .clone()
                .add_service(ScribeTailServiceServer::new(service))
                .serve_with_incoming(incoming),
        );
        OracleQueryParticipant {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            endpoint: format!("https://127.0.0.1:{port}"),
            role: ClusterRole::Scribe,
            fencing_token: 1,
            capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                tail_protocol_version: 1,
            }),
        }
    }

    /// Proves a stalled release cannot outlive the query: one peer capture
    /// was retained, another listing failed, and the retaining Scribe never
    /// answers its release, yet admission refuses with the listing failure by
    /// the original deadline, and a later detached release is bounded the
    /// same way.
    ///
    /// # Panics
    ///
    /// Panics when the release is never sent, admission does not return the
    /// listing failure, or either cleanup outlives its deadline.
    #[tokio::test]
    async fn a_stalled_release_settles_by_the_query_deadline() {
        let ca = BifrostPeerCa::generate("scribe.peer.test").expect("peer CA");
        let peer = StalledRelease::default();
        let retaining = serve_scribe(&ca, peer.clone()).await;
        let mut failed = retaining.clone();
        failed.node_id = NodeId::new(uuid::Uuid::now_v7());
        let leaf = ca.issue_leaf("oracle").expect("oracle leaf");
        let discovery = RegistryTailStreamDiscovery::new(
            Some(BifrostPeerTls::new(
                ca.ca_certificate_pem().as_bytes().to_vec(),
                ca.server_name().to_owned(),
                leaf.certificate_pem().as_bytes().to_vec(),
                secrecy::SecretString::from(leaf.private_key_pem().to_owned()),
            )),
            None,
        );
        let binding = TenantTableBinding {
            tenant_id: DataTenantId::new_v7(),
            namespace: "traces".to_owned(),
            table: "spans".to_owned(),
        };
        let query_id = uuid::Uuid::now_v7();
        let capture_id = uuid::Uuid::now_v7();
        let bound = Duration::from_secs(5);

        let deadline = Instant::now() + Duration::from_millis(500);
        let admitted = tokio::time::timeout(
            bound,
            discovery.admit(
                &binding,
                &[retaining.clone(), failed],
                query_id,
                vec![
                    Ok(TailCapture {
                        capture_id: Some(capture_id),
                        streams: Vec::new(),
                        publication_revision: 0,
                    }),
                    Err(TailReadError::Unavailable {
                        detail: "fixture listing failure".to_owned(),
                    }),
                ],
                deadline,
            ),
        )
        .await
        .expect("admission settles by the query deadline");
        assert!(matches!(admitted, Err(TailReadError::Unavailable { .. })));
        assert_eq!(peer.received.load(Ordering::Acquire), 1);

        let deadline = Instant::now() + Duration::from_millis(500);
        tokio::time::timeout(
            bound,
            discovery.release(&binding, query_id, &[(retaining, capture_id)], deadline),
        )
        .await
        .expect("a detached release settles by the query deadline");
        assert_eq!(peer.received.load(Ordering::Acquire), 2);
    }
}

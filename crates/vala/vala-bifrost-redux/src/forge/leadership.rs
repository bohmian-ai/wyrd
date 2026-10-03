//! The Forge leader term, its volatile schedule, and routing of commit notices.
//!
//! One coordinator at a time holds the singleton election row
//! ([`ForgeLeaderElection`]). While it does, it owns a [`ForgeSchedule`] that
//! starts empty on every acquisition, as RisingWave's Iceberg compaction
//! manager does when its meta node becomes leader. Losing or resigning the
//! term drops that schedule at once, so a replaced leader can never dispatch.
//!
//! A successful Iceberg promotion is reported to the live leader by the same
//! locality rule gateway capture uses: in-process when this replica holds the
//! term, otherwise over the private peer listener the term published. A remote
//! leader is resolved only from the live election row, and the notice names
//! the fencing token read there, so a replaced leader refuses it.

use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use vala_sql::queries::forge_leader::ForgeLeaderElection;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::NotifyForgePromotionRequest;
use wyrd_tonic::wyrd::v1::forge_leader_peer_service_client::ForgeLeaderPeerServiceClient;

use super::error::ForgeError;
use super::leader::{DEFAULT_REPORT_TIMEOUT, ForgeCommitNotice, ForgeSchedule};
use crate::oracle::dispatcher::BifrostPeerTls;

/// Lifetime of one leader term without renewal, matching RisingWave's
/// default meta leader lease.
pub(super) const LEADER_TERM: Duration = Duration::from_secs(30);
/// Interval at which a coordinator renews or contends for the term.
pub(super) const LEADER_HEARTBEAT: Duration = Duration::from_secs(10);
/// Deadline for one peer notice to a remote leader.
const PEER_NOTICE_TIMEOUT: Duration = Duration::from_secs(5);
/// Lease key reported when a notice reaches a replica that is not the leader.
const LEADER_LEASE_KEY: &str = "forge:leader";

/// The private peer identity a coordinator publishes and dials with.
pub struct ForgeLeaderPeer {
    /// URI this replica's peer listener is reachable at, published with its term.
    advertise_uri: String,
    /// Mutual TLS identity used to dial a remote leader.
    tls: BifrostPeerTls,
    /// Lazily connected channel to the last dialed leader URI.
    channel: Mutex<Option<(String, Channel)>>,
}

impl ForgeLeaderPeer {
    /// Creates the peer route for one coordinator.
    #[must_use]
    pub fn new(advertise_uri: String, tls: BifrostPeerTls) -> Self {
        Self {
            advertise_uri,
            tls,
            channel: Mutex::new(None),
        }
    }

    /// Returns a channel to `uri`, reusing the cached one when it still matches.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::LeaderPeer`] when tonic rejects the address or
    /// the peer TLS material.
    fn channel(&self, uri: &str) -> Result<Channel, ForgeError> {
        let mut cached = self.channel.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((cached_uri, channel)) = cached.as_ref()
            && cached_uri == uri
        {
            return Ok(channel.clone());
        }
        let channel = self
            .tls
            .endpoint(uri.to_owned())
            .map_err(|error| ForgeError::LeaderPeer {
                detail: format!("endpoint {uri} is invalid: {error}"),
            })?
            .timeout(PEER_NOTICE_TIMEOUT)
            .connect_lazy();
        *cached = Some((uri.to_owned(), channel.clone()));
        Ok(channel)
    }
}

/// One term this replica holds, with the schedule that lives exactly as long.
pub struct ForgeHeldTerm {
    /// Fencing token minted when this term was acquired.
    fencing_token: i64,
    /// Volatile per-table schedule; empty when the term began.
    schedule: ForgeSchedule,
}

impl ForgeHeldTerm {
    /// Returns the fencing token of this term.
    #[must_use]
    pub fn fencing_token(&self) -> i64 {
        self.fencing_token
    }

    /// Returns this term's volatile schedule.
    #[must_use]
    pub fn schedule(&self) -> &ForgeSchedule {
        &self.schedule
    }
}

/// Concrete owner of this coordinator's participation in Forge leadership.
pub(super) struct ForgeLeadership {
    /// Durable singleton election row.
    election: ForgeLeaderElection,
    /// Stable process identity that contends for the term.
    owner: Uuid,
    /// Peer route; absent for a single-process deployment with no peer listener.
    peer: Option<ForgeLeaderPeer>,
    /// The term this replica holds, if any.
    held: RwLock<Option<Arc<ForgeHeldTerm>>>,
}

impl ForgeLeadership {
    /// Creates a non-leader participant over the election row.
    pub(super) fn new(election: ForgeLeaderElection, owner: Uuid) -> Self {
        Self {
            election,
            owner,
            peer: None,
            held: RwLock::new(None),
        }
    }

    /// Installs the peer route this replica publishes and dials with.
    pub(super) fn set_peer(&mut self, peer: ForgeLeaderPeer) {
        self.peer = Some(peer);
    }

    /// Returns the term this replica currently holds.
    pub(super) fn held(&self) -> Option<Arc<ForgeHeldTerm>> {
        self.held
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the held term.
    fn set_held(&self, term: Option<Arc<ForgeHeldTerm>>) {
        *self.held.write().unwrap_or_else(PoisonError::into_inner) = term;
    }

    /// Renews the held term or contends for a new one.
    ///
    /// Returns `true` only when this call acquired a new term, whose schedule
    /// is empty. A failed renewal drops the held term before contending again,
    /// so dispatch stops as soon as authority cannot be proven.
    ///
    /// # Errors
    ///
    /// Returns SQL errors from the election row; the held term is dropped
    /// first, so an unreachable database never leaves a leader dispatching.
    pub(super) async fn heartbeat(&self) -> Result<bool, ForgeError> {
        if let Some(term) = self.held() {
            match self
                .election
                .renew(self.owner, term.fencing_token, LEADER_TERM)
                .await
            {
                Ok(true) => return Ok(false),
                Ok(false) => {
                    tracing::warn!(
                        fencing_token = term.fencing_token,
                        "Forge leader term lost; dropping its schedule"
                    );
                    self.set_held(None);
                }
                Err(error) => {
                    self.set_held(None);
                    return Err(ForgeError::Sql(error));
                }
            }
        }
        let uri = self.peer.as_ref().map(|peer| peer.advertise_uri.as_str());
        let Some(fencing_token) = self
            .election
            .acquire(self.owner, uri, LEADER_TERM)
            .await
            .map_err(ForgeError::Sql)?
        else {
            return Ok(false);
        };
        tracing::info!(fencing_token, "Forge leader term acquired with an empty schedule");
        self.set_held(Some(Arc::new(ForgeHeldTerm {
            fencing_token,
            schedule: ForgeSchedule::new(DEFAULT_REPORT_TIMEOUT),
        })));
        Ok(true)
    }

    /// Ends the held term so a standby can take over immediately.
    ///
    /// # Errors
    ///
    /// Returns SQL errors from the election row; the local term is dropped
    /// regardless, and the row then expires on its own.
    pub(super) async fn resign(&self) -> Result<(), ForgeError> {
        let Some(term) = self.held() else {
            return Ok(());
        };
        self.set_held(None);
        self.election
            .resign(self.owner, term.fencing_token)
            .await
            .map_err(ForgeError::Sql)
    }

    /// Applies one commit notice to the held term's schedule.
    ///
    /// `fencing_token` is the term a remote caller resolved; `None` is the
    /// in-process path, which by construction targets the term held here.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when this replica does not hold the
    /// named term.
    pub(super) fn accept(
        &self,
        fencing_token: Option<i64>,
        notice: ForgeCommitNotice,
        now: DateTime<Utc>,
    ) -> Result<(), ForgeError> {
        match self.held() {
            Some(term) if fencing_token.is_none_or(|token| token == term.fencing_token) => {
                term.schedule.notify_commit(notice, now);
                Ok(())
            }
            _ => Err(ForgeError::FenceLost {
                lease_key: LEADER_LEASE_KEY.to_owned(),
            }),
        }
    }

    /// Delivers one commit notice to the live leader.
    ///
    /// The in-process path is taken when this replica holds the term.
    /// Otherwise the live election row names the leader's peer URI and token.
    /// No live term, or a leader without a peer route, drops the notice: like
    /// RisingWave, ordinary compaction counters are volatile, and the next
    /// commit refreshes the table.
    ///
    /// # Errors
    ///
    /// Returns SQL errors resolving the leader and [`ForgeError::LeaderPeer`]
    /// for a transport failure or refusal by the remote leader.
    pub(super) async fn notify(
        &self,
        notice: ForgeCommitNotice,
        now: DateTime<Utc>,
    ) -> Result<(), ForgeError> {
        if self.held().is_some() {
            return self.accept(None, notice, now);
        }
        let Some(term) = self.election.current().await.map_err(ForgeError::Sql)? else {
            tracing::debug!("no live Forge leader; dropping promotion notice");
            return Ok(());
        };
        let (Some(uri), Some(peer)) = (term.peer_uri.as_deref(), self.peer.as_ref()) else {
            tracing::debug!("Forge leader has no peer route; dropping promotion notice");
            return Ok(());
        };
        let request = NotifyForgePromotionRequest {
            fencing_token: term.fencing_token,
            tenant_id: notice.key.tenant.to_string(),
            namespace: notice.key.table.namespace.clone(),
            table: notice.key.table.table.clone(),
            snapshot_id: notice.snapshot_id,
            properties: notice.settings.to_properties(),
        };
        ForgeLeaderPeerServiceClient::new(peer.channel(uri)?)
            .notify_promotion(request)
            .await
            .map(|_| ())
            .map_err(|status| ForgeError::LeaderPeer {
                detail: format!("promotion notice to {uri}: {status}"),
            })
    }
}

//! The Forge leader term, its volatile schedule, and routing of commit notices.
//!
//! One coordinator at a time holds the singleton election row
//! ([`ForgeLeaderElection`]). While it does, it owns a [`ForgeSchedule`] that
//! starts empty on every acquisition, as `RisingWave`'s Iceberg compaction
//! manager does when its meta node becomes leader. Losing or resigning the
//! term drops that schedule at once, so a replaced leader can never dispatch.
//!
//! A successful Iceberg promotion is reported to the live leader by the same
//! locality rule gateway capture uses: in-process when this replica holds the
//! term, otherwise over the private peer listener the term published. A remote
//! leader is resolved only from the live election row, and the notice names
//! the fencing token read there, so a replaced leader refuses it. Compactor
//! pulls and reports follow the same rule and enter the same handlers.

use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use vala_sql::queries::forge_leader::ForgeLeaderElection;
use wyrd_tonic::tonic::transport::Channel;
use wyrd_tonic::wyrd::v1::forge_leader_peer_service_client::ForgeLeaderPeerServiceClient;
use wyrd_tonic::wyrd::v1::{
    ForgeCompactionTask, NotifyForgePromotionRequest, PullForgeCompactionRequest,
    ReportForgeCompactionRequest,
};

use super::error::ForgeError;
use super::leader::{
    DEFAULT_REPORT_TIMEOUT, ForgeCommitNotice, ForgeCompactionDispatch, ForgeCompactionOutcome,
    ForgeSchedule, ForgeTableKey,
};
use super::settings::ForgeCompactionType;
use crate::oracle::dispatcher::BifrostPeerTls;

/// Lifetime of one leader term without renewal, matching `RisingWave`'s
/// default meta leader lease.
pub(super) const LEADER_TERM: Duration = Duration::from_secs(30);
/// Interval at which a coordinator renews or contends for the term.
pub(super) const LEADER_HEARTBEAT: Duration = Duration::from_secs(10);
/// Deadline for one peer call to a remote leader.
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
        tracing::info!(
            fencing_token,
            "Forge leader term acquired with an empty schedule"
        );
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
        self.term(fencing_token)?
            .schedule
            .notify_commit(notice, now);
        Ok(())
    }

    /// Delivers one commit notice to the live leader.
    ///
    /// The in-process path is taken when this replica holds the term.
    /// Otherwise the live election row names the leader's peer URI and token.
    /// No live term, or a leader without a peer route, drops the notice: like
    /// `RisingWave`, ordinary compaction counters are volatile, and the next
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
        let Some((fencing_token, uri, mut client)) = self.remote().await? else {
            tracing::debug!("no routable Forge leader; dropping promotion notice");
            return Ok(());
        };
        let request = NotifyForgePromotionRequest {
            fencing_token,
            tenant_id: notice.key.tenant.to_string(),
            namespace: notice.key.table.namespace.clone(),
            table: notice.key.table.table.clone(),
            snapshot_id: notice.snapshot_id,
            properties: notice.settings.to_properties(),
        };
        client
            .notify_promotion(request)
            .await
            .map(|_| ())
            .map_err(|status| ForgeError::LeaderPeer {
                detail: format!("promotion notice to {uri}: {status}"),
            })
    }
    /// Returns the held term when it is `fencing_token`, or the local term
    /// for the in-process path (`None`).
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when this replica does not hold the
    /// named term.
    fn term(&self, fencing_token: Option<i64>) -> Result<Arc<ForgeHeldTerm>, ForgeError> {
        match self.held() {
            Some(term) if fencing_token.is_none_or(|token| token == term.fencing_token) => Ok(term),
            _ => Err(ForgeError::FenceLost {
                lease_key: LEADER_LEASE_KEY.to_owned(),
            }),
        }
    }

    /// Answers one compactor pull from the held term's schedule.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when this replica does not hold the
    /// named term.
    pub(super) fn serve_pull(
        &self,
        fencing_token: Option<i64>,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<Vec<ForgeCompactionDispatch>, ForgeError> {
        Ok(self.term(fencing_token)?.schedule.pull(limit, now))
    }

    /// Applies one compactor report to the held term's schedule.
    ///
    /// Returns whether the report matched the table's current task.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when this replica does not hold the
    /// named term.
    pub(super) fn serve_report(
        &self,
        fencing_token: Option<i64>,
        key: &ForgeTableKey,
        task_id: Uuid,
        outcome: ForgeCompactionOutcome,
        now: DateTime<Utc>,
    ) -> Result<bool, ForgeError> {
        Ok(self
            .term(fencing_token)?
            .schedule
            .report(key, task_id, outcome, now))
    }

    /// Resolves the remote leader's term, URI and a client for it.
    ///
    /// Returns `None` when no live term exists or no peer route is
    /// configured on either side.
    ///
    /// # Errors
    ///
    /// Returns SQL errors resolving the leader and [`ForgeError::LeaderPeer`]
    /// when the published URI is not dialable.
    async fn remote(
        &self,
    ) -> Result<Option<(i64, String, ForgeLeaderPeerServiceClient<Channel>)>, ForgeError> {
        let Some(term) = self.election.current().await.map_err(ForgeError::Sql)? else {
            return Ok(None);
        };
        let (Some(uri), Some(peer)) = (term.peer_uri, self.peer.as_ref()) else {
            return Ok(None);
        };
        let client = ForgeLeaderPeerServiceClient::new(peer.channel(&uri)?);
        Ok(Some((term.fencing_token, uri, client)))
    }

    /// Pulls at most `limit` table-level tasks from the live leader.
    ///
    /// The in-process path serves the pull directly when this replica holds
    /// the term; otherwise the pull is sent to the elected leader's peer
    /// listener, and its response is the acknowledgement that ends the pull.
    /// No live leader yields no work.
    ///
    /// # Errors
    ///
    /// Returns SQL errors resolving the leader and [`ForgeError::LeaderPeer`]
    /// for a transport failure, a refusal, or a malformed task.
    pub(super) async fn pull(
        &self,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<Vec<ForgeCompactionDispatch>, ForgeError> {
        if self.held().is_some() {
            return self.serve_pull(None, limit, now);
        }
        let Some((fencing_token, uri, mut client)) = self.remote().await? else {
            return Ok(Vec::new());
        };
        let response = client
            .pull_compaction(PullForgeCompactionRequest {
                fencing_token,
                limit: u32::try_from(limit).unwrap_or(u32::MAX),
            })
            .await
            .map_err(|status| ForgeError::LeaderPeer {
                detail: format!("compaction pull from {uri}: {status}"),
            })?;
        response
            .into_inner()
            .tasks
            .into_iter()
            .map(|task| {
                dispatch_from_wire(task).map_err(|detail| ForgeError::LeaderPeer { detail })
            })
            .collect()
    }

    /// Reports one dispatched task's outcome to the live leader.
    ///
    /// A report that reaches no leader, or a leader that no longer tracks
    /// the task, changes nothing: the schedule ignores stale task identities.
    ///
    /// # Errors
    ///
    /// Returns SQL errors resolving the leader and [`ForgeError::LeaderPeer`]
    /// for a transport failure.
    pub(super) async fn report(
        &self,
        dispatch: &ForgeCompactionDispatch,
        outcome: ForgeCompactionOutcome,
        now: DateTime<Utc>,
    ) -> Result<(), ForgeError> {
        if self.held().is_some() {
            return self
                .serve_report(None, &dispatch.key, dispatch.task_id, outcome, now)
                .map(|_| ());
        }
        let Some((fencing_token, uri, mut client)) = self.remote().await? else {
            return Ok(());
        };
        client
            .report_compaction(ReportForgeCompactionRequest {
                fencing_token,
                task_id: dispatch.task_id.to_string(),
                tenant_id: dispatch.key.tenant.to_string(),
                namespace: dispatch.key.table.namespace.clone(),
                table: dispatch.key.table.table.clone(),
                outcome: outcome_to_wire(outcome) as i32,
            })
            .await
            .map(|_| ())
            .map_err(|status| ForgeError::LeaderPeer {
                detail: format!("compaction report to {uri}: {status}"),
            })
    }
}

/// Encodes one dispatch for the peer wire.
#[must_use]
pub fn dispatch_to_wire(dispatch: &ForgeCompactionDispatch) -> ForgeCompactionTask {
    ForgeCompactionTask {
        task_id: dispatch.task_id.to_string(),
        tenant_id: dispatch.key.tenant.to_string(),
        namespace: dispatch.key.table.namespace.clone(),
        table: dispatch.key.table.table.clone(),
        branch: dispatch.branch.clone(),
        compaction_type: dispatch.compaction_type.as_str().to_owned(),
    }
}

/// Decodes one peer-wire task into the dispatch the leader made.
///
/// # Errors
///
/// Returns a description of the first malformed field.
pub fn dispatch_from_wire(task: ForgeCompactionTask) -> Result<ForgeCompactionDispatch, String> {
    Ok(ForgeCompactionDispatch {
        task_id: task
            .task_id
            .parse()
            .map_err(|_| "task_id is not a UUID".to_owned())?,
        key: table_key_from_wire(&task.tenant_id, task.namespace, task.table)?,
        branch: task.branch,
        compaction_type: ForgeCompactionType::parse(&task.compaction_type)
            .map_err(|error| error.to_string())?,
    })
}

/// Decodes one tenant-qualified table identity from the peer wire.
///
/// # Errors
///
/// Returns a description of the malformed tenant or table identity.
pub fn table_key_from_wire(
    tenant: &str,
    namespace: String,
    table: String,
) -> Result<ForgeTableKey, String> {
    Ok(ForgeTableKey {
        tenant: tenant
            .parse()
            .map_err(|_| "tenant_id is not a tenant id".to_owned())?,
        table: vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
            crate::catalog::BIFROST_CATALOG_NAME,
            namespace,
            table,
        )
        .map_err(|error| error.to_string())?,
    })
}

/// Encodes one report outcome for the peer wire.
#[must_use]
pub fn outcome_to_wire(
    outcome: ForgeCompactionOutcome,
) -> wyrd_tonic::wyrd::v1::ForgeCompactionOutcome {
    use wyrd_tonic::wyrd::v1::ForgeCompactionOutcome as Wire;
    match outcome {
        ForgeCompactionOutcome::Succeeded => Wire::Succeeded,
        ForgeCompactionOutcome::Failed => Wire::Failed,
        ForgeCompactionOutcome::NotStarted => Wire::NotStarted,
    }
}

/// Decodes one report outcome from the peer wire.
///
/// # Errors
///
/// Returns a description when the outcome is unspecified or unknown.
pub fn outcome_from_wire(raw: i32) -> Result<ForgeCompactionOutcome, String> {
    use wyrd_tonic::wyrd::v1::ForgeCompactionOutcome as Wire;
    match Wire::try_from(raw) {
        Ok(Wire::Succeeded) => Ok(ForgeCompactionOutcome::Succeeded),
        Ok(Wire::Failed) => Ok(ForgeCompactionOutcome::Failed),
        Ok(Wire::NotStarted) => Ok(ForgeCompactionOutcome::NotStarted),
        _ => Err(format!("compaction outcome {raw} is not a known outcome")),
    }
}

//! In-process peer topology the Oracle peer-network journeys drive.
//!
//! Every pod is one real `wyrd-server` composed by [`WyrdTestCluster`] inside
//! this test process: its own public HTTP and gRPC sockets, its own private
//! mutually authenticated peer socket, its own Bifrost data root, and its own
//! registration in the shared Postgres membership. Peer traffic between pods
//! crosses real loopback sockets under real mTLS, so the trust boundary under
//! test is the deployed one.
//!
//! What a single process cannot separate is process-wide state: the metrics
//! recorder, the private-plane body-poll counter, the physical-build
//! observation, and the live-producer and cleanup pauses are shared by every
//! pod. The accessors for those say so and take no pod index, so a journey
//! never reads a process-wide number as if it were one pod's.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest as _, Sha256};
use url::Url;
use vala_bifrost_redux::oracle::OraclePreparationPause;
use vala_bifrost_redux::oracle::analytical::{AnalyticalExecutePause, AnalyticalPhysicalEvidence};
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::bifrost::peer_ca::{BifrostPeerCa, BifrostPeerLeaf};
use wyrd_testing::bifrost::telemetry::BifrostMetricKind;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_testing::{Bootstrap, WyrdTestServer};

use crate::support::JourneyError;

/// Longest a membership change may take to become observable through
/// [`PeerCluster::await_membership`].
const MEMBERSHIP_DEADLINE: Duration = Duration::from_secs(45);

/// How long one pod may take to report every readiness probe passing.
const READY_DEADLINE: Duration = Duration::from_secs(60);

/// Interval between readiness observations.
const READY_POLL: Duration = Duration::from_millis(100);

/// Request deadline every statement a journey drives in process carries.
///
/// Sized for the heaviest statement a journey runs: the Analytical spill
/// baseline must sort more than one query's 1.5 GiB limit at the 4 GiB pod
/// floor, which takes about a minute in an unoptimized test build.
const STATEMENT_DEADLINE_MS: i64 = 240_000;

/// [`STATEMENT_DEADLINE_MS`] as the bound on waiting for an armed pause.
const STATEMENT_DEADLINE: Duration = Duration::from_millis(240_000);

/// How long a statement's graph may take to settle after its terminal.
const SETTLEMENT_WAIT: Duration = Duration::from_secs(30);

/// One live multi-pod peer topology with the controls the journeys need.
///
/// Owns the cluster, the per-pod targets it was started from, one member
/// identity for private-plane probes, and the per-pod pauses and in-flight
/// statements a journey arms. Pods are addressed by their zero-based position
/// in the target list, which is also their deterministic node identity minus
/// one.
pub(crate) struct PeerCluster {
    /// The composed pods and their shared Postgres, storage, and peer CA.
    cluster: WyrdTestCluster,
    /// Target each pod was started as, in pod order.
    targets: Vec<BifrostTarget>,
    /// Member leaf of the cluster authority every probe presents.
    member: BifrostPeerLeaf,
    /// Armed follower `ExecuteTask` pause per pod.
    execute_pauses: BTreeMap<usize, Arc<AnalyticalExecutePause>>,
    /// Armed post-pin preparation pause per pod.
    preparation_pauses: BTreeMap<usize, Arc<OraclePreparationPause>>,
    /// Statement a journey started and has not yet joined, per pod.
    active: BTreeMap<usize, QuerySlot>,
    /// Managed event time, in UTC microseconds, stamped on every fixture row.
    ///
    /// Fixed once at launch so every row a journey ingests lands in one
    /// hourly partition, and therefore behind one live route and one Scribe
    /// producer, no matter when the journey runs relative to an hour boundary.
    fixture_event_time_micros: i64,
}

impl PeerCluster {
    /// Starts one pod per target and waits until every pod is ready.
    ///
    /// # Errors
    ///
    /// Returns the cluster boot failure, or a readiness timeout naming the
    /// pod and its failing probes.
    pub(crate) async fn start(targets: &[BifrostTarget]) -> Result<Self, JourneyError> {
        let pods: Vec<_> = targets.iter().map(|target| (*target, None)).collect();
        Self::launch(&pods, false, None).await
    }

    /// Starts one pod per target with every pod's built-in gateway adapters
    /// rooted at the local mock upstream `root`, and waits for readiness.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::start`].
    pub(crate) async fn start_with_gateway_provider_root(
        targets: &[BifrostTarget],
        root: Url,
    ) -> Result<Self, JourneyError> {
        let pods: Vec<_> = targets.iter().map(|target| (*target, None)).collect();
        Self::launch(&pods, false, Some(root)).await
    }

    /// Starts one pod per `(target, slot units)` pair and waits for readiness.
    ///
    /// A stated slot count is the operator setting
    /// `WYRD_BIFROST_ORACLE_QUERY_SLOT_LIMIT`; `None` keeps the count the pod
    /// floor derives.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::start`].
    pub(crate) async fn start_with_slots(
        pods: &[(BifrostTarget, Option<usize>)],
    ) -> Result<Self, JourneyError> {
        Self::launch(pods, false, None).await
    }

    /// Starts every pod but the last, which stays configured and unbooted.
    ///
    /// The last pod keeps its reserved addresses and identity, so a journey
    /// can observe the running topology before [`Self::join`] brings it up.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::start`].
    pub(crate) async fn start_with_joiner(targets: &[BifrostTarget]) -> Result<Self, JourneyError> {
        let pods: Vec<_> = targets.iter().map(|target| (*target, None)).collect();
        Self::launch(&pods, true, None).await
    }

    /// Builds the spec, boots the pods, and waits for each booted pod.
    ///
    /// `gateway_provider_root`, when present, roots every pod's built-in
    /// gateway adapters at that local mock upstream.
    ///
    /// # Errors
    ///
    /// Returns the cluster boot failure, a member-leaf generation failure, or
    /// a readiness timeout.
    async fn launch(
        pods: &[(BifrostTarget, Option<usize>)],
        delay_last: bool,
        gateway_provider_root: Option<Url>,
    ) -> Result<Self, JourneyError> {
        let targets: Vec<BifrostTarget> = pods.iter().map(|(target, _)| *target).collect();
        let mut spec = BifrostClusterSpec::for_targets(&targets);
        if let Some(root) = gateway_provider_root {
            spec = spec.with_gateway_provider_root_for_test(root);
        }
        for (node, (_, slots)) in spec.nodes.iter_mut().zip(pods) {
            if let Some(oracle) = node.oracle.as_mut() {
                oracle.oracle_query_slot_limit = *slots;
            }
        }
        let cluster = if delay_last {
            WyrdTestCluster::start_spec_delayed_last(spec).await?
        } else {
            WyrdTestCluster::start_spec(spec).await?
        };
        let member = cluster.peer_ca().issue_leaf("journey-member")?;
        let peers = Self {
            cluster,
            targets,
            member,
            execute_pauses: BTreeMap::new(),
            preparation_pauses: BTreeMap::new(),
            active: BTreeMap::new(),
            fixture_event_time_micros: chrono::Utc::now().timestamp_micros(),
        };
        let booted = peers.len() - usize::from(delay_last);
        for index in 0..booted {
            peers.await_ready(index).await?;
        }
        Ok(peers)
    }

    /// Returns how many pods the topology was configured with.
    pub(crate) fn len(&self) -> usize {
        self.targets.len()
    }

    /// Returns the target pod `index` was started as.
    ///
    /// # Panics
    ///
    /// Panics when `index` names no configured pod, which is a journey bug.
    pub(crate) fn target(&self, index: usize) -> BifrostTarget {
        self.targets[index]
    }

    /// Returns every pod index started as `target`, in pod order.
    pub(crate) fn indices_of(&self, target: BifrostTarget) -> Vec<usize> {
        (0..self.len())
            .filter(|index| self.targets[*index] == target)
            .collect()
    }

    /// Returns the deterministic node identity of pod `index`.
    ///
    /// Pod `n` is node `n + 1`, the identity [`BifrostClusterSpec::for_targets`]
    /// assigns, so the mapping never depends on which pods are running.
    pub(crate) fn node_id(&self, index: usize) -> NodeId {
        NodeId::new(uuid::Uuid::from_u128(index as u128 + 1))
    }

    /// Returns the running server of pod `index`.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is stopped, killed, or not yet joined.
    pub(crate) fn server(&self, index: usize) -> Result<&WyrdTestServer, JourneyError> {
        self.cluster
            .server_by_node(self.node_id(index))
            .ok_or_else(|| format!("pod {index} is not running").into())
    }

    /// Returns the private peer socket reserved for pod `index`.
    ///
    /// The reservation outlives a stop, so a restarted pod comes back on the
    /// socket membership already published for it.
    ///
    /// # Errors
    ///
    /// Returns a message when `index` names no configured pod.
    pub(crate) fn peer_addr(&self, index: usize) -> Result<SocketAddr, JourneyError> {
        self.cluster
            .peer_addr(self.node_id(index))
            .ok_or_else(|| format!("pod {index} has no reserved peer socket").into())
    }

    /// Returns the authority every pod's peer leaf chains to.
    pub(crate) fn peer_ca(&self) -> &BifrostPeerCa {
        self.cluster.peer_ca()
    }

    /// Returns the shared local object-store root.
    pub(crate) fn storage_root(&self) -> &Path {
        self.cluster.storage_root()
    }

    /// Returns the seeded data tenant every fixture table belongs to.
    pub(crate) fn tenant(&self) -> DataTenantId {
        self.cluster.data_tenant_id()
    }

    /// Waits until pod `index` reports every readiness probe passing.
    ///
    /// A serving pod is ready when every readiness probe passes and its peer
    /// plane obligation is met. A dedicated Forge worker composes no listener
    /// and runs no readiness loop, so its composition returning is the whole
    /// of its startup.
    ///
    /// # Errors
    ///
    /// Returns a timeout naming every probe's reason and the peer-plane state.
    async fn await_ready(&self, index: usize) -> Result<(), JourneyError> {
        if self.targets[index] == BifrostTarget::ForgeWorker {
            return Ok(());
        }
        let deadline = tokio::time::Instant::now() + READY_DEADLINE;
        loop {
            let state = self.server(index)?.state();
            let snapshot = state.readiness.load_full();
            if snapshot.all_ok() && state.peer_plane.is_satisfied() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "pod {index} never became ready: {snapshot:?} peer_required={} \
                     peer_serving={}",
                    state.peer_plane.is_required(),
                    state.peer_plane.is_serving(),
                )
                .into());
            }
            tokio::time::sleep(READY_POLL).await;
        }
    }

    /// Boots the pod [`Self::start_with_joiner`] held back and waits for it.
    ///
    /// # Errors
    ///
    /// Returns the boot failure unchanged — including a refused private
    /// socket — or a readiness timeout.
    pub(crate) async fn join(&mut self, index: usize) -> Result<(), JourneyError> {
        self.cluster.restart_node(self.node_id(index)).await?;
        self.await_ready(index).await
    }

    /// Gracefully stops pod `index` and boots it again on its retained roots.
    ///
    /// The replacement keeps the pod's identity, data root, and reserved
    /// sockets, which is an ordinary pod restart on its own volume.
    ///
    /// # Errors
    ///
    /// Returns the stop or boot failure, or a readiness timeout.
    pub(crate) async fn restart(&mut self, index: usize) -> Result<(), JourneyError> {
        let node_id = self.node_id(index);
        self.cluster.stop_node(node_id).await?;
        self.cluster.restart_node(node_id).await?;
        self.await_ready(index).await
    }

    /// Stops pod `index` through its ordinary graceful shutdown.
    ///
    /// Readiness is removed, the shared shutdown token is cancelled, and the
    /// pod's roles drain within the server's own shutdown budget.
    ///
    /// # Errors
    ///
    /// Returns the shutdown failure unchanged, including a drain that missed
    /// its budget.
    pub(crate) async fn stop(&mut self, index: usize) -> Result<(), JourneyError> {
        self.cluster.stop_node(self.node_id(index)).await?;
        Ok(())
    }

    /// Terminates pod `index` abruptly, without a drain.
    ///
    /// Its public request lifetime is cancelled and its serving task aborted,
    /// which is the closest a single process comes to a pod disappearing.
    /// Aborting the serving task does not end a peer request held at the pod's
    /// execute pause, which observes graph cancellation but not termination,
    /// so a pause armed there is released once the pod is gone. A dead pod's held request must not keep
    /// its leader waiting, and it cannot produce rows from attempts the
    /// termination already cancelled.
    ///
    /// # Errors
    ///
    /// Returns the termination failure unchanged.
    pub(crate) async fn kill(&mut self, index: usize) -> Result<(), JourneyError> {
        self.cluster
            .terminate_node_abruptly_for_test(self.node_id(index))
            .await?;
        if let Some(pause) = self.execute_pauses.remove(&index) {
            pause.release();
        }
        Ok(())
    }

    /// Releases every armed pause, cancels every started statement, and
    /// shuts every running pod down.
    ///
    /// # Errors
    ///
    /// Returns the cluster shutdown failure unchanged.
    pub(crate) async fn shutdown(mut self) -> Result<(), JourneyError> {
        for pause in self.execute_pauses.values() {
            pause.release();
        }
        for pause in self.preparation_pauses.values() {
            pause.release();
        }
        for slot in std::mem::take(&mut self.active).into_values() {
            slot.cancel();
            let _ = slot.join().await;
        }
        self.cluster.shutdown().await?;
        Ok(())
    }

    /// Returns the live Scribe and Oracle leases pod `index` can currently see.
    ///
    /// Membership is refreshed from Postgres first, because the claim is what
    /// this pod sees of its peers now, not what its background refresh cached.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is not running, or the refresh failure.
    pub(crate) async fn membership(
        &self,
        index: usize,
    ) -> Result<Vec<MembershipEntry>, JourneyError> {
        let Some(registry) = self.server(index)?.state().bifrost_cluster_for_test() else {
            return Ok(Vec::new());
        };
        registry.refresh_snapshot().await?;
        Ok(MembershipEntry::project(&registry.snapshot()))
    }

    /// Polls pod `observer`'s live membership cut until `observed` holds.
    ///
    /// Membership is heartbeat-driven: a join appears within one heartbeat,
    /// and a stopped member leaves once its last heartbeat ages past the
    /// fifteen-second liveness cutoff. [`MEMBERSHIP_DEADLINE`] turns a member
    /// that never appears or never leaves into a diagnosable failure; elapsed
    /// time is never itself evidence.
    ///
    /// # Errors
    ///
    /// Returns a failure naming `change` and the last cut seen when the
    /// deadline passes, or the membership refresh failure unchanged.
    pub(crate) async fn await_membership(
        &self,
        observer: usize,
        change: &str,
        observed: impl Fn(&[MembershipEntry]) -> bool,
    ) -> Result<(), JourneyError> {
        let deadline = std::time::Instant::now() + MEMBERSHIP_DEADLINE;
        loop {
            let membership = self.membership(observer).await?;
            if observed(&membership) {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!(
                    "pod {observer} never observed {change}; last membership {membership:?}"
                )
                .into());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Returns the private address pod `index` published into membership.
    ///
    /// Read back from membership rather than recomputed from configuration,
    /// so a pod that registered the wrong endpoint reports it.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod published no live role.
    pub(crate) async fn advertise_addr(&self, index: usize) -> Result<String, JourneyError> {
        let node_id = self.node_id(index).as_uuid();
        self.membership(index)
            .await?
            .into_iter()
            .find(|entry| entry.node_id == node_id)
            .map(|entry| entry.address)
            .ok_or_else(|| format!("pod {index} published no live role").into())
    }

    /// Re-reads the shared membership snapshot into pod `index`'s Oracle.
    ///
    /// A pod that composes no Oracle has nothing to refresh.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is not running, or the refresh failure.
    pub(crate) async fn refresh_snapshot(&self, index: usize) -> Result<(), JourneyError> {
        if let Some(registry) = self.server(index)?.state().oracle_cluster() {
            registry.refresh_snapshot().await?;
        }
        Ok(())
    }

    /// Refreshes every running pod's Oracle membership snapshot.
    ///
    /// # Errors
    ///
    /// Returns the first refresh failure.
    pub(crate) async fn refresh_snapshots(&self) -> Result<(), JourneyError> {
        self.cluster.refresh_oracle_snapshots().await?;
        Ok(())
    }

    /// Resolves pod `index`'s Oracle engine.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is not running or composes no Oracle.
    fn oracle(
        &self,
        index: usize,
    ) -> Result<Arc<vala_bifrost_redux::oracle::Oracle>, JourneyError> {
        self.server(index)?
            .state()
            .bifrost_query()
            .map(|query| Arc::clone(query.engine()))
            .ok_or_else(|| format!("pod {index} composes no Oracle").into())
    }

    /// Registers one `(id Int64, filter_key Utf8)` table through pod `index`.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no catalog, or the refusal.
    pub(crate) async fn register_table(
        &self,
        index: usize,
        table: &str,
    ) -> Result<(), JourneyError> {
        let catalog = self
            .server(index)?
            .state()
            .bifrost_catalog()
            .ok_or_else(|| format!("pod {index} composes no Bifrost catalog"))?;
        catalog
            .create_table(vala_bifrost_redux::catalog::CreateTableRequest {
                table: table_ref(table),
                user_fields: vec![
                    arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
                    arrow::datatypes::Field::new(
                        "filter_key",
                        arrow::datatypes::DataType::Utf8,
                        false,
                    ),
                ],
                tenant: self.tenant(),
                physical_layout: None,
                audit: None,
            })
            .await?;
        Ok(())
    }

    /// Writes deterministic fixture rows through pod `index` and publishes them.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::ingest_live_rows`], and the
    /// publication failure.
    pub(crate) async fn ingest_rows(
        &self,
        index: usize,
        table: &str,
        start_id: i64,
        rows: i64,
        groups: i64,
    ) -> Result<(), JourneyError> {
        self.ingest_live_rows(index, table, start_id, rows, groups)
            .await?;
        self.flush(index).await
    }

    /// Writes deterministic fixture rows and leaves them live on pod `index`.
    ///
    /// The batch is admitted through the same logical ingress seam the public
    /// write surface uses. Nothing is frozen or published, so a later query
    /// reads these rows only through a live fragment on this pod.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Scribe or catalog, the table
    /// is unregistered, or ingest fails.
    pub(crate) async fn ingest_live_rows(
        &self,
        index: usize,
        table: &str,
        start_id: i64,
        rows: i64,
        groups: i64,
    ) -> Result<(), JourneyError> {
        let server = self.server(index)?;
        let scribe = server
            .bifrost_scribe()
            .ok_or_else(|| format!("pod {index} composes no Scribe"))?;
        let catalog = server
            .state()
            .bifrost_catalog()
            .ok_or_else(|| format!("pod {index} composes no Bifrost catalog"))?;
        let tenant = self.tenant();
        let (fingerprint, _) = catalog
            .table_registration(&table_ref(table), tenant)
            .await?;
        let principal = wyrd_runtime::principal::Principal {
            id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_runtime::principal::PrincipalKind::User,
            tenant_id: tenant,
            roles: Vec::new(),
            effective_permissions: wyrd_runtime::PermissionSet::new(),
            credential_id: None,
        };
        scribe
            .ingest_native_for_test(vala_bifrost_redux::scribe::NativeIngressTestFrame {
                principal,
                table: table_ref(table),
                expected_schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                payload: fixture_rows_ipc(start_id, rows, groups, self.fixture_event_time_micros)?,
            })
            .await?;
        Ok(())
    }

    /// Freezes and publishes everything pod `index`'s Scribe holds.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is not running, or the flush failure.
    pub(crate) async fn flush(&self, index: usize) -> Result<(), JourneyError> {
        self.server(index)?.flush_bifrost().await?;
        Ok(())
    }

    /// Runs one statement on pod `index`'s Oracle and returns its row count.
    ///
    /// # Errors
    ///
    /// Returns the statement's own terminal failure.
    pub(crate) async fn execute_sql(&self, index: usize, sql: &str) -> Result<usize, JourneyError> {
        drive_sql(
            self.oracle(index)?,
            self.tenant(),
            sql.to_owned(),
            &mut ResultFold::default(),
        )
        .await
    }

    /// Runs one statement on pod `index` and collects everything that pod saw.
    ///
    /// The terminal reaches this caller before the graph's lifecycle settles,
    /// and the physical evidence is folded inside that settlement, so the wait
    /// is on the settlement counter rather than on the evidence itself: a plan
    /// with no output sort settles carrying none.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Analytical handle, the
    /// statement fails, or its graph does not settle inside the bounded wait.
    pub(crate) async fn execute_analytical_baseline(
        &self,
        index: usize,
        sql: &str,
    ) -> Result<AnalyticalBaselineEvidence, JourneyError> {
        let engine = self.oracle(index)?;
        let supervisor = engine
            .analytical_execution()
            .ok_or_else(|| format!("pod {index} composes no Analytical handle"))?
            .supervisor()
            .clone();
        let settled_before = supervisor.settled_graph_count();
        let mut fold = ResultFold::default();
        let rows = drive_sql(engine, self.tenant(), sql.to_owned(), &mut fold)
            .await
            .map_err(|error| format!("baseline statement failed: {error}"))?;
        let deadline = tokio::time::Instant::now() + SETTLEMENT_WAIT;
        while supervisor.settled_graph_count() == settled_before
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let physical = settled_analytical_evidence(
            settled_before,
            supervisor.settled_graph_count(),
            supervisor.settled_physical_evidence(),
        )?;
        Ok(AnalyticalBaselineEvidence {
            rows,
            result_digest: fold.digest(),
            counts_all_one: fold.counts_all_one,
            keys_strictly_increasing: fold.keys_strictly_increasing,
            physical,
        })
    }

    /// Starts one statement on pod `index` and returns immediately.
    ///
    /// One statement per pod at a time: a journey that needs two on the same
    /// pod is describing a different topology.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod already runs a started statement or
    /// composes no Oracle.
    pub(crate) fn start_sql(&mut self, index: usize, sql: &str) -> Result<(), JourneyError> {
        if self.active.contains_key(&index) {
            return Err(format!("pod {index} already runs a started statement").into());
        }
        let slot = QuerySlot::start(self.oracle(index)?, self.tenant(), sql.to_owned());
        self.active.insert(index, slot);
        Ok(())
    }

    /// Cancels pod `index`'s started statement exactly as a dropped caller would.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod runs no started statement.
    pub(crate) fn cancel_sql(&self, index: usize) -> Result<(), JourneyError> {
        self.active
            .get(&index)
            .ok_or_else(|| format!("pod {index} runs no started statement"))?
            .cancel();
        Ok(())
    }

    /// Joins pod `index`'s started statement and reports its terminal.
    ///
    /// The outer result is the harness's; the inner one is the statement's
    /// row count or its failure text.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod runs no started statement.
    pub(crate) async fn await_sql(
        &mut self,
        index: usize,
    ) -> Result<Result<usize, String>, JourneyError> {
        let slot = self
            .active
            .remove(&index)
            .ok_or_else(|| format!("pod {index} runs no started statement"))?;
        Ok(slot.join().await)
    }

    /// Reports pod `index`'s cumulative graph-lease activations and live leases.
    ///
    /// A pod that composes no Oracle never leases a graph and reports zero.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is not running.
    pub(crate) fn graph_leases(&self, index: usize) -> Result<(u64, usize), JourneyError> {
        Ok(self
            .server(index)?
            .state()
            .bifrost_query()
            .map_or((0, 0), |oracle| oracle.engine().graph_lease_counts()))
    }

    /// Reads pod `index`'s own `(admitted, succeeded)` Analytical attempts.
    ///
    /// The attempt counter family is process-wide in this topology, so a claim
    /// about one pod's attempts reads that pod's supervisor instead.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Oracle or Analytical handle.
    pub(crate) fn attempt_counts(&self, index: usize) -> Result<(u64, u64), JourneyError> {
        Ok(self
            .oracle(index)?
            .analytical_execution()
            .ok_or_else(|| format!("pod {index} composes no Analytical handle"))?
            .supervisor()
            .attempt_counts())
    }

    /// Projects every Oracle ownership total pod `index` can account for.
    ///
    /// Each field is read from the owner that already maintains it — the
    /// Analytical execution handle, Oracle's admission inspection, the pod's
    /// resource root, and the pod's scratch tree — except the two live gauges,
    /// which are process-wide in this topology and therefore the same on every
    /// pod.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Oracle or Analytical handle,
    /// an ownership lock is poisoned, or scratch cannot be measured.
    pub(crate) fn ownership_snapshot(
        &self,
        index: usize,
    ) -> Result<OracleOwnershipSnapshot, JourneyError> {
        let engine = self.oracle(index)?;
        let live = engine
            .analytical_execution()
            .ok_or_else(|| format!("pod {index} composes no Analytical handle"))?
            .live()?;
        let runtime = engine.runtime_inspection();
        let root = engine.role_resources().snapshot()?;
        let scratch = scratch_usage(
            engine
                .analytical_spill_root()
                .ok_or_else(|| format!("pod {index} composes no spill root"))?,
        )?;
        let attempts = "bifrost_oracle_analytical_attempts_active";
        let gauges = self.metric_totals(&[attempts])?;
        Ok(OracleOwnershipSnapshot {
            leader_attempts: live.leader.attempts,
            leader_graphs: live.leader.graphs,
            leader_cleanup_failures: live.leader.cleanup_failures,
            follower_attempts: live.follower.attempts,
            follower_graphs: live.follower.graphs,
            follower_cleanup_failures: live.follower.cleanup_failures,
            active_queries: runtime.active_queries,
            queued_queries: runtime.queued_queries,
            reserved_memory_bytes: runtime.reserved_memory_bytes,
            peer_running: runtime.peer_running,
            held_grants: runtime.held_grants,
            root_active_queries: root.oracle_active_queries,
            root_analytical_queries: root.oracle_analytical_queries,
            root_query_memory_used_bytes: u64::try_from(root.oracle_query_memory_used_bytes)
                .unwrap_or(u64::MAX),
            root_query_active: root.oracle_query_active,
            scratch,
            attempts_active: gauges[attempts],
        })
    }

    /// Totals each named production metric family across the whole process.
    ///
    /// Every pod shares one recorder, so this is the topology's total rather
    /// than one pod's. A family nothing has emitted totals zero.
    ///
    /// # Errors
    ///
    /// Returns the recorder parse failure.
    pub(crate) fn metric_totals(
        &self,
        families: &[&str],
    ) -> Result<BTreeMap<String, f64>, JourneyError> {
        self.metric_totals_labeled(families, &BTreeMap::new())
    }

    /// Totals each named family over the process samples carrying every label.
    ///
    /// A histogram family totals its observation count; its bucket and sum
    /// samples are not observations.
    ///
    /// # Errors
    ///
    /// Returns the recorder parse failure.
    pub(crate) fn metric_totals_labeled(
        &self,
        families: &[&str],
        labels: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, f64>, JourneyError> {
        let mut totals: BTreeMap<String, f64> = families
            .iter()
            .map(|family| ((*family).to_owned(), 0.0))
            .collect();
        for sample in self.cluster.telemetry().snapshot()? {
            let matches = !matches!(
                sample.kind,
                BifrostMetricKind::HistogramBucket | BifrostMetricKind::HistogramSum
            ) && labels
                .iter()
                .all(|(name, value)| sample.labels.get(name) == Some(value));
            if let Some(total) = totals.get_mut(&sample.family)
                && matches
            {
                *total += sample.value;
            }
        }
        Ok(totals)
    }

    /// Returns the process-wide production telemetry capture every pod shares.
    ///
    /// One recorder and tracer serve the whole topology, so a delta from it is
    /// the topology's total; trace identity, not a pod label, separates work.
    pub(crate) fn telemetry(&self) -> &wyrd_testing::bifrost::BifrostTelemetryCapture {
        self.cluster.telemetry()
    }

    /// Reads the process-wide count of private-plane request bodies polled.
    ///
    /// Counted only after a peer leaf passed the fixed peer-name check, so a
    /// delta proves some admitted peer request reached a body. Every pod's
    /// peer plane shares this counter.
    pub(crate) fn peer_body_polls(&self) -> u64 {
        wyrd_server::grpc::peer_body_polls()
    }

    /// Reads how many live fragments pod `index`'s Scribe has executed.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Scribe.
    pub(crate) fn scribe_fragments(&self, index: usize) -> Result<u64, JourneyError> {
        Ok(self
            .server(index)?
            .state()
            .bifrost_ingest()
            .ok_or_else(|| format!("pod {index} composes no Scribe"))?
            .fragment_inspection()
            .0)
    }

    /// Reads the process-wide physical-build observation plus pod `index`'s
    /// active query cuts.
    ///
    /// `total` and `latest_cut_fingerprint` are shared by every pod; the
    /// active cuts are the ones pod `index`'s running-query registry retains.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod is not running.
    pub(crate) fn physical_build_evidence(
        &self,
        index: usize,
    ) -> Result<PhysicalBuildEvidence, JourneyError> {
        let (total, latest_cut_fingerprint) =
            vala_bifrost_redux::oracle::physical_build_observation_for_test();
        let tenant = self.tenant();
        let active_cut_fingerprints = match self.oracle(index) {
            Ok(engine) => {
                let registry = engine.running_queries();
                registry
                    .list(tenant)
                    .iter()
                    .filter_map(|summary| {
                        registry
                            .get(tenant, &summary.request_id)
                            .map(|entry| entry.participant_cut().fingerprint())
                    })
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        Ok(PhysicalBuildEvidence {
            total,
            latest_cut_fingerprint,
            active_cut_fingerprints,
        })
    }

    /// Arms pod `index`'s one-shot refusal of its next distributed physical build.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Oracle.
    pub(crate) fn arm_analytical_plan_failure(&self, index: usize) -> Result<(), JourneyError> {
        self.oracle(index)?.fail_next_analytical_plan_for_test();
        Ok(())
    }

    /// Arms pod `index`'s one-shot follower `ExecuteTask` pause.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Analytical handle.
    pub(crate) fn arm_execute_pause(&mut self, index: usize) -> Result<(), JourneyError> {
        let engine = self.oracle(index)?;
        let handle = engine
            .analytical_execution()
            .ok_or_else(|| format!("pod {index} composes no Analytical handle"))?;
        let pause = Arc::new(AnalyticalExecutePause::default());
        handle
            .worker()
            .bind_execute_pause_for_test(Arc::clone(&pause));
        self.execute_pauses.insert(index, pause);
        Ok(())
    }

    /// Waits until pod `index`'s armed execute pause holds a follower task.
    ///
    /// # Errors
    ///
    /// Returns a message when no pause is armed there or nothing reached it
    /// within one statement deadline.
    pub(crate) async fn await_execute_paused(&self, index: usize) -> Result<(), JourneyError> {
        let pause = self
            .execute_pauses
            .get(&index)
            .ok_or_else(|| format!("pod {index} has no execute pause armed"))?;
        tokio::time::timeout(STATEMENT_DEADLINE, pause.wait_paused())
            .await
            .map_err(|_| format!("no follower task reached pod {index}'s execute pause"))?;
        Ok(())
    }

    /// Releases pod `index`'s armed execute pause.
    ///
    /// # Errors
    ///
    /// Returns a message when no pause is armed there.
    pub(crate) fn release_execute_pause(&self, index: usize) -> Result<(), JourneyError> {
        self.execute_pauses
            .get(&index)
            .ok_or_else(|| format!("pod {index} has no execute pause armed"))?
            .release();
        Ok(())
    }

    /// Arms pod `index`'s post-pin preparation pause for one request identity.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Oracle, or the bind refusal.
    pub(crate) fn arm_preparation_pause(
        &mut self,
        index: usize,
        request_id: &RequestId,
    ) -> Result<(), JourneyError> {
        let pause = Arc::new(OraclePreparationPause::new(request_id.clone()));
        self.oracle(index)?
            .bind_preparation_pause_for_test(Some(Arc::clone(&pause)))?;
        self.preparation_pauses.insert(index, pause);
        Ok(())
    }

    /// Returns the deadline pod `index`'s preparation pause observed, if any.
    pub(crate) fn preparation_pause_deadline(&self, index: usize) -> Option<i64> {
        self.preparation_pauses
            .get(&index)
            .and_then(|pause| pause.observed_deadline_ms())
    }

    /// Releases and unbinds pod `index`'s preparation pause.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Oracle, or the unbind refusal.
    pub(crate) fn release_preparation_pause(&mut self, index: usize) -> Result<(), JourneyError> {
        if let Some(pause) = self.preparation_pauses.remove(&index) {
            pause.release();
        }
        self.oracle(index)?.bind_preparation_pause_for_test(None)?;
        Ok(())
    }

    /// Arms the process-wide Analytical cleanup pause.
    ///
    /// Only the leader of a graph enters it, so in a topology with one leader
    /// it holds exactly that leader's cleanup.
    pub(crate) fn arm_cleanup_pause(&self) {
        vala_bifrost_redux::oracle::analytical::analytical_cleanup_pause_for_test().arm();
    }

    /// Waits until a leader's graph cleanup enters the armed pause.
    ///
    /// # Errors
    ///
    /// Returns a message when no cleanup entered it within one statement
    /// deadline.
    pub(crate) async fn await_cleanup_paused(&self) -> Result<(), JourneyError> {
        let pause = vala_bifrost_redux::oracle::analytical::analytical_cleanup_pause_for_test();
        tokio::time::timeout(STATEMENT_DEADLINE, pause.wait_entered())
            .await
            .map_err(|_| "no graph cleanup reached the cleanup pause")?;
        Ok(())
    }

    /// Releases the process-wide Analytical cleanup pause.
    pub(crate) fn release_cleanup_pause(&self) {
        vala_bifrost_redux::oracle::analytical::analytical_cleanup_pause_for_test().release();
    }

    /// Arms the process-wide Scribe live-production pause.
    pub(crate) fn arm_live_production_pause(&self) {
        vala_bifrost_redux::scribe::tail_rpc::scribe_live_production_pause_for_test().arm();
    }

    /// Waits until a live producer enters the armed production pause.
    ///
    /// # Errors
    ///
    /// Returns a message when no producer entered it within one statement
    /// deadline.
    pub(crate) async fn await_live_production_paused(&self) -> Result<(), JourneyError> {
        let pause = vala_bifrost_redux::scribe::tail_rpc::scribe_live_production_pause_for_test();
        tokio::time::timeout(STATEMENT_DEADLINE, pause.wait_entered())
            .await
            .map_err(|_| "no live producer reached the pause")?;
        Ok(())
    }

    /// Releases the process-wide Scribe live-production pause.
    pub(crate) fn release_live_production_pause(&self) {
        vala_bifrost_redux::scribe::tail_rpc::scribe_live_production_pause_for_test().release();
    }

    /// Reads the process's open live producers and pod `index`'s follower bytes.
    ///
    /// # Errors
    ///
    /// Returns a message when the pod composes no Scribe, or the resource
    /// snapshot failure.
    pub(crate) fn live_scribe_holds(&self, index: usize) -> Result<(usize, usize), JourneyError> {
        let snapshot = self
            .server(index)?
            .state()
            .bifrost_ingest()
            .ok_or_else(|| format!("pod {index} composes no Scribe"))?
            .resources()
            .snapshot()?;
        Ok((
            vala_bifrost_redux::scribe::tail_rpc::open_live_producers_for_test(),
            snapshot.oracle_query_memory_used_bytes,
        ))
    }

    /// Seeds one public Service principal in the shared data tenant.
    ///
    /// # Errors
    ///
    /// Returns a message when no pod serves a public listener, or the
    /// provisioning failure.
    pub(crate) async fn provision_public_api_key(
        &self,
        name: &str,
    ) -> Result<secrecy::SecretString, JourneyError> {
        self.provision_api_key(self.tenant(), name).await
    }

    /// Seeds one public Service principal in a second, foreign data tenant.
    ///
    /// The returned key authenticates against the same pods as
    /// [`Self::provision_public_api_key`], so a journey proves isolation
    /// through the very same public listener.
    ///
    /// # Errors
    ///
    /// Returns the tenant or principal provisioning failure.
    pub(crate) async fn provision_foreign_public_api_key(
        &self,
        name: &str,
    ) -> Result<secrecy::SecretString, JourneyError> {
        let tenant = self.cluster.add_tenant(name).await?;
        self.provision_api_key(tenant, name).await
    }

    /// Bootstraps one admin Service principal in `tenant` through a serving pod.
    ///
    /// # Errors
    ///
    /// Returns a message when no running pod serves HTTP or the bootstrap did
    /// not yield a machine key.
    async fn provision_api_key(
        &self,
        tenant: DataTenantId,
        name: &str,
    ) -> Result<secrecy::SecretString, JourneyError> {
        let server = self
            .cluster
            .servers()
            .find(|server| server.base_url().is_some())
            .ok_or("no running pod serves a public listener")?;
        match server
            .bootstrap_service_in_tenant(tenant, name, &["admin"])
            .await?
        {
            Bootstrap::Machine { api_key, .. } => Ok(api_key),
            Bootstrap::User { .. } => Err("machine bootstrap returned a user".into()),
        }
    }

    /// Deletes one object from the shared local object store.
    ///
    /// `relative_object_key` resolves under [`Self::storage_root`]; an
    /// absolute key or one with a parent-directory component is refused so a
    /// journey cannot reach outside the fixture.
    ///
    /// # Errors
    ///
    /// Returns a message when the key escapes the root or cannot be removed.
    pub(crate) fn remove_storage_object(
        &self,
        relative_object_key: &str,
    ) -> Result<(), JourneyError> {
        let key = Path::new(relative_object_key);
        if key.is_absolute()
            || key
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!(
                "storage object key escapes the storage root: {relative_object_key}"
            )
            .into());
        }
        std::fs::remove_file(self.storage_root().join(key))?;
        Ok(())
    }

    /// Performs one shaped private-plane probe and returns its gRPC outcome.
    ///
    /// Issued as a raw HTTP/2 request rather than through a generated client,
    /// because the claim under test is about the bytes on the wire: which
    /// adapter is addressed, which transport identity carries it, and how the
    /// first gRPC frame is split or coalesced. A mutual probe presents a member
    /// leaf of the cluster authority. A refusal is an outcome, not an error.
    ///
    /// # Errors
    ///
    /// Returns the endpoint, handshake, or send failure when the destination
    /// never answered.
    pub(crate) async fn probe(&self, plan: &PeerProbePlan) -> Result<String, JourneyError> {
        let endpoint = match plan.transport {
            PeerProbeTransport::Mutual => {
                let authority = self.peer_ca();
                wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
                    plan.address.clone(),
                    authority.ca_certificate_pem().as_bytes(),
                    authority.server_name().to_owned(),
                    self.member.certificate_pem().as_bytes(),
                    self.member.private_key_pem().as_bytes(),
                )?
            }
            // Built from the raw address with no trust material at all: the
            // destination must refuse the connection itself.
            PeerProbeTransport::Plaintext => wyrd_tonic::tonic::transport::Endpoint::from_shared(
                plan.address.replace("https://", "http://"),
            )?,
        };
        let mut channel = endpoint.connect().await?;
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(plan.service.path())
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .header("te", "trailers")
            .body(probe_body(plan.framing, plan.payload.as_deref()))?;
        let response = tower::ServiceExt::oneshot(&mut channel, request).await?;
        Ok(probe_outcome(response).await)
    }
}

/// Qualifies one fixture table name in the Bifrost namespace.
fn table_ref(table: &str) -> vala_bifrost_redux::catalog::TableRef {
    vala_bifrost_redux::catalog::TableRef::new(
        vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
        table,
    )
}

/// One live role lease as observed by the pod that reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MembershipEntry {
    /// Runtime node identity holding the lease.
    pub(crate) node_id: uuid::Uuid,
    /// Independently fenced role, rendered as its wire name.
    pub(crate) role: String,
    /// Exact private address the role published.
    pub(crate) address: String,
    /// Whether the role advertised readiness.
    pub(crate) ready: bool,
    /// Monotonic fence this role incarnation holds.
    pub(crate) fencing_token: u64,
}

impl MembershipEntry {
    /// Projects every live Scribe and Oracle lease in one snapshot.
    ///
    /// Sorted by role then address so two pods observing the same membership
    /// produce comparable lists.
    fn project(snapshot: &vala_bifrost_redux::cluster::ClusterSnapshot) -> Vec<Self> {
        let mut entries: Vec<Self> = snapshot
            .live_scribes()
            .into_iter()
            .map(|lease| Self::from_lease("scribe", lease))
            .chain(
                snapshot
                    .live_oracles()
                    .into_iter()
                    .map(|lease| Self::from_lease("oracle", lease)),
            )
            .collect();
        entries
            .sort_by(|left, right| (&left.role, &left.address).cmp(&(&right.role, &right.address)));
        entries
    }

    /// Renders one lease under an already resolved role name.
    fn from_lease(role: &str, lease: &wyrd_spec::vala::api::ClusterRoleLease) -> Self {
        Self {
            node_id: lease.key.node_id.as_uuid(),
            role: role.to_owned(),
            address: lease.address.clone(),
            ready: lease.ready,
            fencing_token: lease.fencing_token,
        }
    }
}

/// Process-wide physical-build evidence plus one pod's active query cuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PhysicalBuildEvidence {
    /// Physical roots this process has begun building since start.
    pub(crate) total: u64,
    /// Canonical membership digest the most recent build entered with.
    pub(crate) latest_cut_fingerprint: String,
    /// Participant-cut digests of the pod's active queries, in request-ID order.
    pub(crate) active_cut_fingerprints: Vec<String>,
}

/// Everything one analytical statement leaves observable on its coordinator.
#[derive(Debug, Clone)]
pub(crate) struct AnalyticalBaselineEvidence {
    /// Rows the statement produced.
    pub(crate) rows: usize,
    /// Hex SHA-256 over every ordered `(filter_key, matched)` pair.
    ///
    /// Each row contributes its key length as four little-endian bytes, the
    /// key's UTF-8 bytes, then the count as eight little-endian bytes.
    pub(crate) result_digest: String,
    /// Whether every row's count column held exactly one.
    pub(crate) counts_all_one: bool,
    /// Whether the key column increased strictly across the whole result.
    pub(crate) keys_strictly_increasing: bool,
    /// The executed plan's own shape and retained spill counters, absent when
    /// the plan carried no uniquely identifiable output sort.
    pub(crate) physical: Option<AnalyticalPhysicalEvidence>,
}

/// Every Oracle ownership total one pod still accounts for.
///
/// A journey proving cleanup compares two of these and fails on any retained
/// owner. Fixed scalar fields only, projected from production owners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OracleOwnershipSnapshot {
    /// Leader-side attempts still supervised.
    pub(crate) leader_attempts: usize,
    /// Graphs registered at this pod's supervisor, led or followed.
    pub(crate) leader_graphs: usize,
    /// Leader-side graphs retained because their cleanup did not complete.
    pub(crate) leader_cleanup_failures: usize,
    /// Follower-side attempts still supervised.
    pub(crate) follower_attempts: usize,
    /// Follower-side graphs still holding a runtime and admitted envelope.
    pub(crate) follower_graphs: usize,
    /// Follower-side graphs retained because their cleanup did not complete.
    pub(crate) follower_cleanup_failures: usize,
    /// Queries currently holding local class and tenant grants.
    pub(crate) active_queries: u64,
    /// Waiters currently queued for a local grant.
    pub(crate) queued_queries: u64,
    /// Memory bytes reserved by active queries.
    pub(crate) reserved_memory_bytes: u64,
    /// Peer running reservations held by this Oracle.
    pub(crate) peer_running: u64,
    /// Analytical graph grants a leader's admit stream still holds open.
    pub(crate) held_grants: u64,
    /// Live Oracle query owners at the pod's resource root.
    pub(crate) root_active_queries: u32,
    /// Live analytical-class Oracle query owners at that root.
    pub(crate) root_analytical_queries: u32,
    /// Memory retained specifically by Oracle query owners.
    pub(crate) root_query_memory_used_bytes: u64,
    /// Whether at least one Oracle query owner is active.
    pub(crate) root_query_active: bool,
    /// The pod's Oracle scratch occupancy.
    pub(crate) scratch: ScratchUsage,
    /// Process-wide `bifrost_oracle_analytical_attempts_active` gauge.
    pub(crate) attempts_active: f64,
}

impl OracleOwnershipSnapshot {
    /// The snapshot of a pod that owns nothing at all.
    pub(crate) const IDLE: Self = Self {
        leader_attempts: 0,
        leader_graphs: 0,
        leader_cleanup_failures: 0,
        follower_attempts: 0,
        follower_graphs: 0,
        follower_cleanup_failures: 0,
        active_queries: 0,
        queued_queries: 0,
        reserved_memory_bytes: 0,
        peer_running: 0,
        held_grants: 0,
        root_active_queries: 0,
        root_analytical_queries: 0,
        root_query_memory_used_bytes: 0,
        root_query_active: false,
        scratch: ScratchUsage {
            entries: 0,
            bytes: 0,
        },
        attempts_active: 0.0,
    };
}

/// One directory tree's entry and byte occupancy at a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScratchUsage {
    /// Files under the root, at any depth.
    pub(crate) entries: u64,
    /// Summed length of those files.
    pub(crate) bytes: u64,
}

/// Trust a probe establishes its connection under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerProbeTransport {
    /// A member leaf of the cluster authority.
    Mutual,
    /// An unencrypted h2c connection carrying no certificate at all.
    Plaintext,
}

/// Which private adapter a peer probe addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PeerProbeService {
    /// `wyrd.v1.OraclePeerService/ReserveSlots`.
    OraclePeer,
    /// Upstream `worker.WorkerService/ExecuteTask`.
    AnalyticalWorker,
    /// Any other private path, named verbatim.
    Path(String),
}

impl PeerProbeService {
    /// Returns the gRPC path this adapter answers on.
    fn path(&self) -> &str {
        match self {
            Self::OraclePeer => "/wyrd.v1.OraclePeerService/ReserveSlots",
            Self::AnalyticalWorker => "/worker.WorkerService/ExecuteTask",
            Self::Path(path) => path.as_str(),
        }
    }
}

/// How a peer probe lays its first gRPC frame onto the wire.
///
/// HTTP/2 does not align DATA frames to gRPC message boundaries, so a private
/// listener must admit a header split across frames and must not overread a
/// frame carrying more than one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerProbeFraming {
    /// One DATA frame carrying exactly one complete message.
    Whole,
    /// The five-byte header split across two DATA frames.
    SplitHeader,
    /// Two complete messages coalesced into one DATA frame.
    Coalesced,
}

/// One private-plane wire probe against a pod's advertised address.
#[derive(Debug, Clone)]
pub(crate) struct PeerProbePlan {
    /// Advertised address of the destination pod.
    address: String,
    /// Private adapter the probe addresses.
    service: PeerProbeService,
    /// How the probe lays the first gRPC frame onto the wire.
    framing: PeerProbeFraming,
    /// Trust the probe dials the destination under.
    transport: PeerProbeTransport,
    /// Exact protobuf message bytes to send; `None` sends an empty message.
    payload: Option<Vec<u8>>,
}

impl PeerProbePlan {
    /// Builds the ordinary probe: a member identity, one whole frame, the
    /// Oracle reserve adapter.
    pub(crate) fn own(address: &str) -> Self {
        Self {
            address: address.to_owned(),
            service: PeerProbeService::OraclePeer,
            framing: PeerProbeFraming::Whole,
            transport: PeerProbeTransport::Mutual,
            payload: None,
        }
    }

    /// Dials under `transport` instead of the member identity.
    pub(crate) fn over(mut self, transport: PeerProbeTransport) -> Self {
        self.transport = transport;
        self
    }

    /// Sends `payload` as the probe's one gRPC message.
    pub(crate) fn carrying(mut self, payload: Vec<u8>) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Addresses an arbitrary gRPC path on the private listener.
    pub(crate) fn on_path(mut self, path: &str) -> Self {
        self.service = PeerProbeService::Path(path.to_owned());
        self
    }

    /// Addresses `service` instead of the Oracle reserve adapter.
    pub(crate) fn against(mut self, service: PeerProbeService) -> Self {
        self.service = service;
        self
    }

    /// Lays the first gRPC frame out as `framing` describes.
    pub(crate) fn framed(mut self, framing: PeerProbeFraming) -> Self {
        self.framing = framing;
        self
    }
}

/// Builds one probe request body laid out as `framing` describes.
///
/// Every variant carries a well-formed first message; only the HTTP/2 frame
/// boundaries differ. An empty protobuf message is a valid first frame for
/// every probed adapter; a supplied payload replaces it verbatim, because a
/// ticket binds the digest of exactly those bytes.
fn probe_body(framing: PeerProbeFraming, payload: Option<&[u8]>) -> wyrd_tonic::tonic::body::Body {
    let message = match payload {
        Some(payload) => {
            let mut framed = vec![0_u8];
            framed.extend_from_slice(
                &u32::try_from(payload.len())
                    .unwrap_or(u32::MAX)
                    .to_be_bytes(),
            );
            framed.extend_from_slice(payload);
            framed
        }
        None => vec![0_u8, 0, 0, 0, 0],
    };
    let chunks = match framing {
        PeerProbeFraming::Whole => vec![message],
        PeerProbeFraming::SplitHeader => vec![message[..2].to_vec(), message[2..].to_vec()],
        PeerProbeFraming::Coalesced => {
            let mut coalesced = message.clone();
            coalesced.extend_from_slice(&message);
            vec![coalesced]
        }
    };
    let frames = futures_util::stream::iter(chunks.into_iter().map(|chunk| {
        Ok::<_, std::convert::Infallible>(http_body::Frame::data(
            wyrd_tonic::tonic::codegen::Bytes::from(chunk),
        ))
    }));
    wyrd_tonic::tonic::body::Body::new(http_body_util::StreamBody::new(frames))
}

/// Reduces one probe response to its non-secret gRPC status name.
///
/// gRPC reports its status in the headers for a trailers-only refusal and in
/// the trailers otherwise, so both are read before the outcome is decided.
async fn probe_outcome(response: http::Response<wyrd_tonic::tonic::body::Body>) -> String {
    let status = |headers: &http::HeaderMap| -> Option<String> {
        headers
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i32>().ok())
            .map(|code| format!("{:?}", wyrd_tonic::tonic::Code::from_i32(code)))
    };
    if let Some(outcome) = status(response.headers()) {
        return outcome;
    }
    let mut body = response.into_body();
    while let Some(frame) = http_body_util::BodyExt::frame(&mut body).await {
        match frame {
            Ok(frame) => {
                if let Some(trailers) = frame.trailers_ref()
                    && let Some(outcome) = status(trailers)
                {
                    return outcome;
                }
            }
            Err(error) => return format!("BodyError({error})"),
        }
    }
    "Ok".to_owned()
}

/// One statement started outside the journey's own await.
///
/// A journey that acts on a query while it runs — pauses a follower, kills
/// it, cancels from the leader — owns the statement's task and its
/// cancellation here, and nothing else about it.
struct QuerySlot {
    /// The running statement.
    task: tokio::task::JoinHandle<Result<usize, JourneyError>>,
    /// Cancellation the statement selects on, mirroring a caller drop.
    cancel: tokio_util::sync::CancellationToken,
}

impl QuerySlot {
    /// Spawns one statement and returns immediately.
    fn start(
        engine: Arc<vala_bifrost_redux::oracle::Oracle>,
        tenant: DataTenantId,
        sql: String,
    ) -> Self {
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let mut fold = ResultFold::default();
            tokio::select! {
                () = token.cancelled() => Err("the attempt was cancelled by its caller".into()),
                outcome = drive_sql(engine, tenant, sql, &mut fold) => outcome,
            }
        });
        Self { task, cancel }
    }

    /// Cancels the statement exactly as a dropped caller would.
    fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Joins the statement and reports its row count or failure text.
    ///
    /// A panicked task is reported as a failure rather than propagated, so a
    /// journey names the claim that broke.
    async fn join(self) -> Result<usize, String> {
        match self.task.await {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(error)) => Err(error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Drives one statement through the production in-process query path.
///
/// # Errors
///
/// Returns the admission, planning, execution, decoding, or terminal failure.
async fn drive_sql(
    engine: Arc<vala_bifrost_redux::oracle::Oracle>,
    tenant: DataTenantId,
    sql: String,
    fold: &mut ResultFold,
) -> Result<usize, JourneyError> {
    let permission = wyrd_runtime::Permission::bifrost_query_read();
    let principal = wyrd_runtime::Principal::new(
        wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        wyrd_runtime::PrincipalKind::User,
        tenant,
        Vec::new(),
        wyrd_runtime::permission::PermissionSet::from_iter([permission.clone()]),
    );
    let context = vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
        principal,
        tenant,
        RequestId::now_v7(),
        None,
        wyrd_spec::vala::api::AuthMethod::Internal,
        permission,
    )?;
    let mut stream = engine
        .query_sql(
            context,
            wyrd_spec::vala::api::BifrostQueryRequest {
                sql,
                deadline_ms: Some(STATEMENT_DEADLINE_MS),
            },
        )
        .await?;
    let mut decoder = vala_bifrost_redux::oracle::QueryIpcDecoder::new();
    let mut rows = 0;
    let mut terminal = None;
    while let Some(frame) = futures_util::StreamExt::next(&mut stream.frames).await {
        match frame? {
            wyrd_spec::vala::api::QueryStreamFrame::Schema(schema) => {
                decoder.accept_schema(&schema.arrow_ipc_schema)?;
            }
            wyrd_spec::vala::api::QueryStreamFrame::Batch(batch) => {
                let decoded = decoder.accept_batch(&batch.arrow_ipc_batch)?;
                rows += decoded.num_rows();
                fold.accept(&decoded);
            }
            wyrd_spec::vala::api::QueryStreamFrame::Terminal(frame) => terminal = Some(frame),
        }
    }
    let terminal = terminal.ok_or("the attempt emitted no terminal frame")?;
    accept_query_terminal(&terminal, rows, &mut decoder)
}

/// Accepts one query's rows only behind a fully validated success terminal.
///
/// Every check is the production contract's own: the closed terminal matrix,
/// the emitted-row reconciliation, the outcome requirement, and the decoder's
/// explicit end-of-stream. `Degraded` is refused because a partial cut is not
/// a baseline result.
///
/// # Errors
///
/// Returns a message when the terminal is malformed, disagrees with the rows
/// emitted before it, is not a success, or does not close the decoder.
fn accept_query_terminal(
    terminal: &wyrd_spec::vala::api::QueryTerminalFrame,
    emitted_rows: usize,
    decoder: &mut vala_bifrost_redux::oracle::QueryIpcDecoder,
) -> Result<usize, JourneyError> {
    terminal.validate()?;
    terminal.validate_emitted_rows(u64::try_from(emitted_rows).unwrap_or(u64::MAX))?;
    if terminal.outcome != wyrd_spec::vala::api::QueryTerminalOutcome::Success {
        return Err(format!(
            "the attempt ended on a {:?} terminal: {:?}",
            terminal.outcome, terminal.error
        )
        .into());
    }
    decoder.accept_eos(&terminal.arrow_ipc_eos)?;
    if !decoder.eos_accepted() {
        return Err("the attempt never closed its Arrow IPC stream".into());
    }
    Ok(emitted_rows)
}

/// Folds one ordered `(Utf8, Int64)` result into assertable evidence.
///
/// Accumulated batch by batch, so the harness never retains a whole result.
#[derive(Debug)]
struct ResultFold {
    /// Running digest over every ordered `(key, count)` pair.
    digest: Sha256,
    /// Whether every count seen so far was exactly one.
    counts_all_one: bool,
    /// Whether keys have increased strictly across every batch boundary.
    keys_strictly_increasing: bool,
    /// Last key accepted, so ordering is checked across batches too.
    previous_key: Option<String>,
}

impl Default for ResultFold {
    /// Starts empty, which trivially satisfies both ordering claims.
    fn default() -> Self {
        Self {
            digest: Sha256::new(),
            counts_all_one: true,
            keys_strictly_increasing: true,
            previous_key: None,
        }
    }
}

impl ResultFold {
    /// Accepts one decoded batch, ignoring a batch of any other shape.
    fn accept(&mut self, batch: &arrow::record_batch::RecordBatch) {
        if batch.num_columns() < 2 {
            return;
        }
        let (Some(keys), Some(counts)) = (
            batch
                .column(0)
                .as_any()
                .downcast_ref::<arrow::array::StringArray>(),
            batch
                .column(1)
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>(),
        ) else {
            return;
        };
        for row in 0..batch.num_rows() {
            let key = keys.value(row);
            let count = counts.value(row);
            if count != 1 {
                self.counts_all_one = false;
            }
            if self
                .previous_key
                .as_ref()
                .is_some_and(|previous| previous.as_str() >= key)
            {
                self.keys_strictly_increasing = false;
            }
            self.previous_key = Some(key.to_owned());
            self.digest.update(
                u32::try_from(key.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes()
                    .as_slice(),
            );
            self.digest.update(key.as_bytes());
            self.digest.update(count.to_le_bytes().as_slice());
        }
    }

    /// Finishes the running digest as lowercase hex.
    fn digest(&self) -> String {
        format!("{:x}", self.digest.clone().finalize())
    }
}

/// Measures one directory tree's file count and byte occupancy.
///
/// An absent root reports zero: a pod that never spilled has nothing to walk.
///
/// # Errors
///
/// Returns the read failure of an existing directory, because an unreadable
/// scratch root would otherwise be reported as an empty one.
fn scratch_usage(root: &Path) -> Result<ScratchUsage, JourneyError> {
    let mut usage = ScratchUsage {
        entries: 0,
        bytes: 0,
    };
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let listing = match std::fs::read_dir(&directory) {
            Ok(listing) => listing,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in listing {
            let entry = entry?;
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                // A spill file removed between the listing and the stat is a
                // cleanup that already happened, not a measurement failure.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                usage.entries = usage.entries.saturating_add(1);
                usage.bytes = usage.bytes.saturating_add(metadata.len());
            }
        }
    }
    Ok(usage)
}

/// Encodes `rows` deterministic `(id, filter_key, wyrd_event_time)` rows as one
/// Arrow IPC stream.
///
/// Ids run `start_id..start_id + rows`, and keys cycle `group_{id % groups}`,
/// so a grouped aggregate has more than one non-trivial group. Every row
/// carries `event_time_micros` in the managed event-time column, which Scribe
/// lifts verbatim, so the caller rather than wall clock picks the partition.
///
/// # Errors
///
/// Returns the batch or IPC encoding failure.
fn fixture_rows_ipc(
    start_id: i64,
    rows: i64,
    groups: i64,
    event_time_micros: i64,
) -> Result<bytes::Bytes, JourneyError> {
    let groups = groups.max(1);
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new("filter_key", arrow::datatypes::DataType::Utf8, false),
        arrow::datatypes::Field::new(
            wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
            arrow::datatypes::DataType::Timestamp(
                arrow::datatypes::TimeUnit::Microsecond,
                Some("UTC".into()),
            ),
            false,
        ),
    ]));
    let ids: Vec<i64> = (start_id..start_id.saturating_add(rows)).collect();
    let keys: Vec<String> = ids
        .iter()
        .map(|id| format!("group_{}", id % groups))
        .collect();
    let event_times = vec![event_time_micros; ids.len()];
    let batch = arrow::record_batch::RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(arrow::array::Int64Array::from(ids)),
            Arc::new(arrow::array::StringArray::from(keys)),
            Arc::new(
                arrow::array::TimestampMicrosecondArray::from(event_times).with_timezone("UTC"),
            ),
        ],
    )?;
    let mut ipc = Vec::new();
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref())?;
        writer.write(&batch)?;
        writer.finish()?;
    }
    Ok(bytes::Bytes::from(ipc))
}

/// Admits settled physical evidence only when the settlement counter advanced.
///
/// The supervisor retains the last settled graph's evidence, so its presence
/// says nothing about which statement folded it; the counter decides.
///
/// # Errors
///
/// Returns a message when `settled_after` did not advance past
/// `settled_before`, meaning this statement's graph never settled inside the
/// bounded wait.
fn settled_analytical_evidence(
    settled_before: u64,
    settled_after: u64,
    evidence: Option<AnalyticalPhysicalEvidence>,
) -> Result<Option<AnalyticalPhysicalEvidence>, JourneyError> {
    if settled_after <= settled_before {
        return Err(
            "the analytical baseline statement's graph did not settle inside its bounded wait"
                .into(),
        );
    }
    Ok(evidence)
}

/// Rows already on the wire are discarded when the terminal is not a
/// validated success.
///
/// Encodes one real schema and batch through the production IPC encoder,
/// decodes them, then presents a structurally valid *failed* terminal whose
/// `row_count` matches the emitted rows, so only the outcome requirement can
/// reject it.
///
/// Required mutation RED: discard the terminal, or accept any outcome the
/// contract validates, and the helper returns the row count for a query that
/// failed after framing began.
///
/// # Panics
///
/// Panics when the fixture stream cannot be encoded or decoded.
#[test]
fn sql_terminal_rejects_failed_output_after_rows() {
    use wyrd_spec::vala::api::{
        QuerySource, QueryTerminalError, QueryTerminalErrorCode, QueryTerminalFrame,
        QueryTerminalOutcome, SourceCompletion, SourceCompletionOutcome,
    };

    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
    ]));
    let batch = arrow::record_batch::RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(arrow::array::Int64Array::from(vec![1_i64, 2, 3]))],
    )
    .expect("the fixture batch matches its own schema");
    let (mut encoder, schema_frame) = vala_bifrost_redux::oracle::QueryIpcEncoder::new(&schema)
        .expect("the fixture schema opens an IPC stream");
    let batch_frame = encoder
        .write(&batch)
        .expect("the fixture batch encodes")
        .expect("a nonempty batch produces a wire frame");
    let mut decoder = vala_bifrost_redux::oracle::QueryIpcDecoder::new();
    decoder
        .accept_schema(&schema_frame.arrow_ipc_schema)
        .expect("the decoder accepts the stream prefix");
    let emitted = decoder
        .accept_batch(&batch_frame.arrow_ipc_batch)
        .expect("the decoder accepts the batch")
        .num_rows();
    assert_eq!(emitted, 3, "the fixture emits the rows it encoded");

    let complete = |source| SourceCompletion {
        source,
        outcome: SourceCompletionOutcome::Complete,
    };
    let failed = QueryTerminalFrame {
        outcome: QueryTerminalOutcome::Failed,
        query_class: wyrd_spec::vala::api::QueryClass::Interactive,
        row_count: u64::try_from(emitted).expect("a fixture row count fits a u64"),
        warnings: Vec::new(),
        source_completion: vec![
            complete(QuerySource::Iceberg),
            complete(QuerySource::HotSealed),
            complete(QuerySource::LiveTail),
        ],
        error: Some(QueryTerminalError {
            code: QueryTerminalErrorCode::QueryExecutionFailed,
            detail: None,
        }),
        arrow_ipc_eos: Vec::new(),
    };
    failed
        .validate()
        .expect("the fixture terminal is structurally valid on its own");

    let refused = accept_query_terminal(&failed, emitted, &mut decoder)
        .expect_err("rows preceding a failed terminal are not a result");
    assert!(
        refused.to_string().contains("Failed terminal"),
        "the refusal names the failed outcome, got {refused}"
    );
    assert!(
        !decoder.eos_accepted(),
        "a failed terminal carries no end-of-stream to accept"
    );
}

/// A settlement counter that never advanced cannot license physical
/// evidence, even when a nonempty value is already retained.
///
/// Required mutation RED: read the evidence without comparing the counters,
/// and the stale value is returned as this statement's own.
///
/// # Panics
///
/// Panics when either guard decision is wrong.
#[test]
fn analytical_baseline_rejects_predecessor_evidence_without_new_settlement() {
    let predecessor = AnalyticalPhysicalEvidence {
        sort_schema: vec!["key".to_owned()],
        sort_ordering: "key@0 ASC".to_owned(),
        spill_count: 7,
        spilled_bytes: 4_096,
        spilled_rows: 128,
        aggregate_group_types: vec!["Int64".to_owned()],
        join_build_schemas: vec![vec!["key".to_owned()]],
    };

    let stale = settled_analytical_evidence(4, 4, Some(predecessor.clone()))
        .expect_err("an unadvanced settlement counter licenses no evidence");
    assert!(
        stale.to_string().contains("analytical baseline"),
        "the refusal names the analytical baseline settlement, got {stale}"
    );

    let settled = settled_analytical_evidence(4, 5, Some(predecessor.clone()))
        .expect("an advanced settlement counter admits the current evidence");
    assert_eq!(
        settled,
        Some(predecessor),
        "the admitted evidence is the value the settled graph folded"
    );
}

//! Multi-process Bifrost peer network for Tier-2 journeys.
//!
//! [`crate::bifrost::cluster::WyrdTestCluster`] runs its nodes as Tokio tasks
//! in one process. That is fast and right for most multi-node coverage, but it
//! cannot prove the peer plane: tasks in one process share a heap, a rustls
//! provider, a resource governor, and — critically — can reach each other
//! without ever crossing a socket. A claim like "all remote work crosses a real
//! private TCP connection between distinct PIDs" is unfalsifiable there.
//!
//! This owner launches each simulated pod as a real child process of the
//! compiled `bifrost_peer_test_node` support binary. Children share only what
//! real replicas share — the repository-managed PostgreSQL database, the object
//! store root, the peer CA, and the peer workload principal — and own
//! everything else: their own composition, runtime, listeners, configuration,
//! WAL, spill, and temporary roots.
//!
//! The parent talks to each child over a private newline-delimited JSON
//! protocol on the child's piped stdin/stdout. It is a test fixture, not a Wyrd
//! wire contract: it is mounted on no listener, carries no tenant data, and
//! never carries key material.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use wyrd_dev_fixtures::pg::PgFixture;

use crate::bifrost::peer_ca::BifrostPeerCa;
use crate::bifrost::peer_keyring::TestPeerKeyring;
use crate::server::TestBifrostPeerTls;

/// Canonical private peer port every simulated pod binds.
///
/// Used verbatim when the platform offers distinct loopback addresses, so
/// membership records one exact pod-like endpoint rather than a
/// host-plus-arbitrary-port pair that no deployment would produce.
pub const CANONICAL_PEER_PORT: u16 = 50052;

/// Canonical public gRPC port every simulated pod binds.
pub const CANONICAL_PUBLIC_GRPC_PORT: u16 = 50051;

/// Canonical public HTTP port every simulated pod binds.
pub const CANONICAL_PUBLIC_HTTP_PORT: u16 = 8080;

/// Longest control line the parent accepts from a child.
///
/// Bounds the stdout reader so a runaway child cannot grow the parent's memory
/// through one unterminated line.
const MAX_CONTROL_LINE_BYTES: usize = 64 * 1024;

/// Complete captured stderr lines retained per child.
const STDERR_TAIL_LINES: usize = 512;

/// How long the parent waits for a child to report `Ready`.
const READY_TIMEOUT: Duration = Duration::from_secs(90);

/// How long the parent waits for a child to answer one control request.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the parent waits for a child to exit after `Shutdown`.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment names the parent uses to configure a child.
///
/// Grouped in one place because the parent writes them and the child reads
/// them, and a typo on either side would surface as an unrelated boot failure.
mod env {
    /// Fixture database every child attaches to.
    pub const DATABASE: &str = "WYRD_PEER_TEST_DATABASE";
    /// Seeded data tenant identity shared by the cluster.
    pub const TENANT_ID: &str = "WYRD_PEER_TEST_TENANT_ID";
    /// Seeded data tenant slug shared by the cluster.
    pub const TENANT_SLUG: &str = "WYRD_PEER_TEST_TENANT_SLUG";
    /// Shared local object-store root.
    pub const STORAGE_ROOT: &str = "WYRD_PEER_TEST_STORAGE_ROOT";
    /// Bifrost target this child serves.
    pub const TARGET: &str = "WYRD_PEER_TEST_TARGET";
    /// Child-private durable Scribe root.
    pub const WAL_ROOT: &str = "WYRD_PEER_TEST_WAL_ROOT";
    /// Child-private durable spill root.
    pub const SPILL_ROOT: &str = "WYRD_PEER_TEST_SPILL_ROOT";
    /// Public HTTP socket this child binds.
    pub const HTTP_BIND: &str = "WYRD_PEER_TEST_HTTP_BIND";
    /// Public gRPC socket this child binds.
    pub const GRPC_BIND: &str = "WYRD_PEER_TEST_GRPC_BIND";
    /// Private peer socket this child binds and advertises.
    pub const PEER_BIND: &str = "WYRD_PEER_TEST_PEER_BIND";
    /// Path to this child's peer CA trust bundle.
    pub const PEER_CA_PATH: &str = "WYRD_PEER_TEST_PEER_CA_PATH";
    /// Path to this child's own dual-EKU leaf chain.
    pub const PEER_CERT_PATH: &str = "WYRD_PEER_TEST_PEER_CERT_PATH";
    /// Path to this child's own peer private key.
    pub const PEER_KEY_PATH: &str = "WYRD_PEER_TEST_PEER_KEY_PATH";
    /// DNS identity every peer dial in this cluster verifies.
    pub const PEER_SERVER_NAME: &str = "WYRD_PEER_TEST_PEER_SERVER_NAME";
    /// Path to the shared peer Service API key file.
    pub const PEER_API_KEY_PATH: &str = "WYRD_PEER_TEST_PEER_API_KEY_PATH";
    /// Identifier of the peer ticket key this child signs with.
    pub const PEER_TICKET_KEY_ID: &str = "WYRD_PEER_TEST_PEER_TICKET_KEY_ID";
    /// Path to this child's peer ticket signing key.
    pub const PEER_TICKET_KEY_PATH: &str = "WYRD_PEER_TEST_PEER_TICKET_KEY_PATH";
    /// Path to the shared published peer ticket verifying manifest.
    pub const PEER_TICKET_KEYRING_PATH: &str = "WYRD_PEER_TEST_PEER_TICKET_KEYRING_PATH";
}

/// Bifrost target one simulated pod serves.
///
/// A closed set rather than a string, so a topology cannot request a target
/// the child binary does not know how to compose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessNodeTarget {
    /// Serve every role in one process.
    All,
    /// Serve APIs and schedule Forge work without executing tasks.
    Server,
    /// Serve only the Oracle query and follower capabilities.
    Oracle,
    /// Serve only the Scribe ingest and tail capabilities.
    Scribe,
    /// Run Forge workers with no public API listeners.
    ForgeWorker,
}

impl ProcessNodeTarget {
    /// Returns the wire name the parent passes and the child parses.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Server => "server",
            Self::Oracle => "oracle",
            Self::Scribe => "scribe",
            Self::ForgeWorker => "forge-worker",
        }
    }

    /// Parses a target from its wire name.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Protocol`] for an unknown name.
    pub fn parse(value: &str) -> Result<Self, ProcessClusterError> {
        match value {
            "all" => Ok(Self::All),
            "server" => Ok(Self::Server),
            "oracle" => Ok(Self::Oracle),
            "scribe" => Ok(Self::Scribe),
            "forge-worker" => Ok(Self::ForgeWorker),
            other => Err(ProcessClusterError::Protocol(format!(
                "unknown Bifrost target {other}"
            ))),
        }
    }
}

/// One request the parent sends to a child over its stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ControlRequest {
    /// Report this child's current identity, addresses, and membership.
    Inspect,
    /// Execute one statement through the inactive Analytical path.
    ExecuteInactiveSql {
        /// Statement to execute.
        sql: String,
    },
    /// Execute one statement and report its complete physical evidence.
    ///
    /// Distinct from [`ControlRequest::ExecuteInactiveSql`] because the
    /// evidence a physical baseline needs — the admitted grant, the scratch
    /// occupancy on either side, the result digest, and the executed plan's own
    /// spill counters — only exists inside the process that ran the query.
    ExecuteAnalyticalBaseline {
        /// Statement to execute.
        sql: String,
    },
    /// Report this child's process-owned Oracle scratch occupancy.
    ScratchUsage,
    /// Report the current total of each named production metric family.
    MetricTotals {
        /// Prometheus family names to total.
        families: Vec<String>,
    },
    /// Register one Bifrost table through this child's own catalog.
    RegisterTable {
        /// Table name inside the `vala.bifrost` namespace.
        table: String,
    },
    /// Write and publish deterministic fixture rows through this child's Scribe.
    IngestRows {
        /// Table name inside the `vala.bifrost` namespace.
        table: String,
        /// First `id` value this request writes.
        start_id: i64,
        /// Number of rows to write.
        rows: i64,
        /// Distinct `filter_key` groups the rows fall into.
        groups: i64,
    },
    /// Re-read the shared membership snapshot into this child's Oracle.
    RefreshSnapshot,
    /// Report this child's graph-lease activations and the leases it still holds.
    GraphLeases,
    /// Report every Oracle ownership total this child can still account for.
    OracleOwnership,
    /// Dial another pod's private peer socket and report the wire outcome.
    ///
    /// The child always presents its configured peer TLS material, so a
    /// success proves the destination admitted this exact process rather than
    /// that the parent could reach the socket. The plan selects which private
    /// adapter is addressed, which workload credential is presented, and how
    /// the first gRPC frame is laid out on the wire.
    PeerProbe(PeerProbePlan),
    /// Report how many request bodies this child's peer plane has polled.
    PeerBodyPolls,
    /// Arm the one-shot follower pause of the next authorized `ExecuteTask`.
    ArmExecutePause,
    /// Block until this child is holding an `ExecuteTask` at that pause.
    AwaitExecutePaused,
    /// Release the held `ExecuteTask` so its graph may finish or fail.
    ReleaseExecutePause,
    /// Start one statement in this child's single active inactive-query slot.
    StartInactiveSql {
        /// Statement to execute.
        sql: String,
    },
    /// Cancel the statement occupying that slot.
    CancelInactiveSql,
    /// Block until that statement reaches its terminal and report it.
    AwaitInactiveSql,
    /// Begin ordered shutdown and exit.
    Shutdown,
}

/// Trust a probe establishes its connection under.
///
/// Named as a closed set because the private plane admits exactly one of them:
/// a plaintext dial has no peer identity to present and must never reach a
/// private adapter, whatever credential it carries above the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerProbeTransport {
    /// The child's own mutually authenticated Bifrost peer identity.
    Mutual,
    /// An unencrypted h2c connection carrying no certificate at all.
    Plaintext,
}

/// One private-plane wire probe a child performs against another pod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerProbePlan {
    /// Advertised address of the destination pod.
    pub address: String,
    /// Private adapter the probe addresses.
    pub service: PeerProbeService,
    /// Workload credential the probe presents.
    pub credential: PeerProbeCredential,
    /// How the probe lays the first gRPC frame onto the wire.
    pub framing: PeerProbeFraming,
    /// Trust the probe dials the destination under.
    pub transport: PeerProbeTransport,
    /// Exact protobuf message bytes to send, when the probe carries a payload.
    ///
    /// A ticket binds the digest of the request it authorizes, so a journey
    /// that mints tickets has to control the exact bytes on the wire. The
    /// parent encodes the request and stamps the ticket; the child only frames
    /// what it is given. `None` keeps the default empty first message, which is
    /// what an admission probe wants.
    pub payload: Option<Vec<u8>>,
}

impl PeerProbePlan {
    /// Builds the ordinary probe: this pod's own identity, one whole frame.
    #[must_use]
    pub fn own(address: &str) -> Self {
        Self {
            address: address.to_owned(),
            service: PeerProbeService::OraclePeer,
            credential: PeerProbeCredential::Own,
            framing: PeerProbeFraming::Whole,
            transport: PeerProbeTransport::Mutual,
            payload: None,
        }
    }

    /// Dials the destination in the clear instead of over the peer identity.
    #[must_use]
    pub fn over(mut self, transport: PeerProbeTransport) -> Self {
        self.transport = transport;
        self
    }

    /// Sends `payload` as the probe's one gRPC message.
    #[must_use]
    pub fn carrying(mut self, payload: Vec<u8>) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Addresses an arbitrary gRPC path on the private listener.
    ///
    /// Used by a journey that must prove a method is refused rather than
    /// answered; the closed [`PeerProbeService`] set covers the methods Wyrd
    /// deliberately serves.
    #[must_use]
    pub fn on_path(mut self, path: &str) -> Self {
        self.service = PeerProbeService::Path(path.to_owned());
        self
    }

    /// Addresses the upstream DataFusion worker adapter instead.
    #[must_use]
    pub fn against(mut self, service: PeerProbeService) -> Self {
        self.service = service;
        self
    }

    /// Presents `credential` instead of this pod's own peer identity.
    #[must_use]
    pub fn presenting(mut self, credential: PeerProbeCredential) -> Self {
        self.credential = credential;
        self
    }

    /// Lays the first gRPC frame out as `framing` describes.
    #[must_use]
    pub fn framed(mut self, framing: PeerProbeFraming) -> Self {
        self.framing = framing;
        self
    }
}

/// Which private adapter a peer probe addresses.
///
/// Both adapters are mounted on the same private listener behind the same
/// authentication layer, so a claim about peer authentication is only proved
/// when it holds for both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerProbeService {
    /// `wyrd.v1.OraclePeerService/ReserveSlots`.
    OraclePeer,
    /// `wyrd.v1.OraclePeerService/ReleaseSlots`.
    OraclePeerRelease,
    /// Upstream `worker.WorkerService/ExecuteTask`.
    AnalyticalWorker,
    /// Any other private path, named verbatim.
    Path(String),
}

impl PeerProbeService {
    /// Returns the gRPC path this adapter answers on.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::OraclePeer => "/wyrd.v1.OraclePeerService/ReserveSlots",
            Self::OraclePeerRelease => "/wyrd.v1.OraclePeerService/ReleaseSlots",
            Self::AnalyticalWorker => "/worker.WorkerService/ExecuteTask",
            Self::Path(path) => path.as_str(),
        }
    }
}

/// Which workload credential a peer probe presents.
///
/// The token variant carries a parent-minted API key for a deliberately wrong
/// principal; the child exchanges it exactly as it would its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerProbeCredential {
    /// This pod's own configured peer Service credential.
    Own,
    /// No `x-wyrd-access-token` metadata at all.
    Absent,
    /// A syntactically invalid bearer that no verifier can accept.
    Invalid,
    /// A parent-supplied API key exchanged for a real access token.
    ApiKey(String),
}

/// How a peer probe lays its first gRPC frame onto the wire.
///
/// HTTP/2 does not align DATA frames to gRPC message boundaries, so a private
/// listener must admit a header split across frames and must not overread a
/// frame carrying more than one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerProbeFraming {
    /// One DATA frame carrying exactly one complete message.
    Whole,
    /// The five-byte header split across two DATA frames.
    SplitHeader,
    /// Two complete messages coalesced into one DATA frame.
    Coalesced,
}

/// One response a child sends to the parent over its stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ControlResponse {
    /// Both sockets are bound and selected role membership is ready.
    Ready(NodeReport),
    /// Answer to [`ControlRequest::Inspect`].
    Inspection(NodeReport),
    /// Answer to [`ControlRequest::ExecuteInactiveSql`].
    Executed {
        /// Rows the statement produced.
        rows: usize,
    },
    /// Answer to [`ControlRequest::ExecuteAnalyticalBaseline`].
    AnalyticalBaseline(Box<AnalyticalBaselineEvidence>),
    /// Answer to [`ControlRequest::ScratchUsage`].
    ScratchUsage(ScratchUsage),
    /// Answer to [`ControlRequest::MetricTotals`].
    MetricTotals {
        /// Total of each requested family, absent families reported as zero.
        totals: BTreeMap<String, f64>,
    },
    /// Answer to [`ControlRequest::RegisterTable`].
    Registered,
    /// Answer to [`ControlRequest::IngestRows`].
    Ingested,
    /// Answer to [`ControlRequest::RefreshSnapshot`].
    Refreshed,
    /// Answer to [`ControlRequest::GraphLeases`].
    GraphLeases {
        /// Graph leases this child activated from a reservation, cumulatively.
        activated: u64,
        /// Graph leases this child still holds.
        live: usize,
    },
    /// Answer to [`ControlRequest::OracleOwnership`].
    OracleOwnership(Box<OracleOwnershipSnapshot>),
    /// Answer to [`ControlRequest::PeerProbe`].
    Probed {
        /// Non-secret gRPC status code name the destination returned.
        outcome: String,
    },
    /// Answer to [`ControlRequest::ArmExecutePause`].
    PauseArmed,
    /// Answer to [`ControlRequest::AwaitExecutePaused`].
    ExecutePaused,
    /// Answer to [`ControlRequest::ReleaseExecutePause`].
    PauseReleased,
    /// Answer to [`ControlRequest::StartInactiveSql`].
    Started,
    /// Answer to [`ControlRequest::CancelInactiveSql`].
    CancelRequested,
    /// Answer to [`ControlRequest::AwaitInactiveSql`].
    InactiveOutcome {
        /// Rows the statement produced, when it succeeded.
        rows: Option<usize>,
        /// The terminal failure, when it did not.
        detail: Option<String>,
    },
    /// Answer to [`ControlRequest::PeerBodyPolls`].
    BodyPolls {
        /// Request bodies this child's peer plane has polled since start.
        count: u64,
    },
    /// The request could not be served.
    ///
    /// Carries a non-secret detail only: the child never renders key material,
    /// API keys, or tenant payloads into a control message.
    Failed {
        /// Non-secret failure detail.
        detail: String,
    },
    /// Ordered shutdown has begun.
    ShuttingDown,
}

/// Everything one analytical statement leaves observable inside its own pod.
///
/// Assembled by the child around a single production execution: the grant is
/// read from the live resource plan before the query runs, the scratch
/// occupancy is measured on both sides of it, the result evidence is folded
/// from the decoded Arrow frames, and the physical evidence is the executed
/// plan's own retained metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticalBaselineEvidence {
    /// Rows the statement produced.
    pub rows: usize,
    /// Hex SHA-256 over every ordered `(filter_key, matched)` pair.
    ///
    /// Each row contributes its key length as four little-endian bytes, the
    /// key's UTF-8 bytes, then the count as eight little-endian bytes, so no
    /// two distinct results can collide by re-splitting the same byte run.
    pub result_digest: String,
    /// Summed `RecordBatch::get_array_memory_size` over every decoded batch.
    pub batch_memory_bytes: u64,
    /// Whether every row's count column held exactly one.
    pub counts_all_one: bool,
    /// Whether the key column increased strictly across the whole result.
    pub keys_strictly_increasing: bool,
    /// Memory ceiling the live resource plan grants one Analytical query.
    pub granted_memory_bytes: u64,
    /// Process-owned Oracle scratch occupancy before the statement ran.
    pub scratch_before: ScratchUsage,
    /// Process-owned Oracle scratch occupancy after its terminal.
    pub scratch_after: ScratchUsage,
    /// The executed plan's own shape and retained spill counters.
    ///
    /// Absent when the plan carried no uniquely identifiable output sort, which
    /// a journey asserting on spill evidence must treat as a failure rather
    /// than as an absent-but-acceptable measurement.
    pub physical: Option<vala_bifrost_redux::oracle::analytical::AnalyticalPhysicalEvidence>,
}

/// Everything one Oracle process still owns, projected from production owners.
///
/// A journey proving cleanup needs one before/after comparison that fails on
/// *any* retained owner, not three chosen gauges. Every field here is read from
/// an owner that already exists in production — the Analytical execution
/// handle, Oracle's own admission inspection, the process resource root, the
/// process scratch tree, and the live production gauges — so this type owns no
/// state, grants no capability, and can never diverge from what it projects.
///
/// Graph and attempt counts are deliberately the whole ownership boundary for
/// what hangs under them: a graph owns its worker, task cache, driver tasks,
/// and connections, so a nonzero graph count is already the leak report and no
/// test-only registry is added to restate it.
///
/// Fixed scalar fields only. Nothing here is unbounded or identity-bearing, so
/// the control protocol carries a constant-size message however large the
/// query was.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OracleOwnershipSnapshot {
    /// Leader-side attempts still supervised.
    pub leader_attempts: usize,
    /// Leader-side graphs still holding a runtime and admitted envelope.
    pub leader_graphs: usize,
    /// Leader-side graphs retained because their cleanup did not complete.
    pub leader_cleanup_failures: usize,
    /// Follower-side attempts still supervised.
    pub follower_attempts: usize,
    /// Follower-side graphs still holding a runtime and admitted envelope.
    pub follower_graphs: usize,
    /// Follower-side graphs retained because their cleanup did not complete.
    pub follower_cleanup_failures: usize,
    /// Queries currently holding local class and tenant grants.
    pub active_queries: u64,
    /// Waiters currently queued for a local grant.
    pub queued_queries: u64,
    /// Memory bytes reserved by active queries.
    pub reserved_memory_bytes: u64,
    /// Spill bytes reserved by active queries.
    pub reserved_spill_bytes: u64,
    /// Peer pending reservations held by this Oracle.
    pub peer_pending: u64,
    /// Peer running reservations held by this Oracle.
    pub peer_running: u64,
    /// Live Oracle query owners at the process resource root.
    pub root_active_queries: u32,
    /// Live analytical-class Oracle query owners at that root.
    pub root_analytical_queries: u32,
    /// Slot units retained by Oracle query owners.
    pub root_query_slot_units: u32,
    /// Memory retained specifically by Oracle query owners.
    pub root_query_memory_used_bytes: u64,
    /// Scratch retained specifically by Oracle query owners.
    pub root_query_scratch_used_bytes: u64,
    /// Whether at least one Oracle query owner is active.
    pub root_query_active: bool,
    /// Process-owned Oracle scratch occupancy.
    pub scratch: ScratchUsage,
    /// Live `bifrost_oracle_analytical_attempts_active` production gauge.
    pub attempts_active: f64,
    /// Live `bifrost_oracle_analytical_exchanges_active` production gauge.
    pub exchanges_active: f64,
    /// Live `oracle_fragments_active` production gauge.
    pub fragments_active: f64,
}

/// One directory tree's entry and byte occupancy at a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScratchUsage {
    /// Files under the root, at any depth.
    pub entries: u64,
    /// Summed length of those files.
    pub bytes: u64,
}

/// What one child reports about itself.
///
/// Every field is directly assertable by a peer-network journey: identity and
/// address prove membership maps to this exact pod, the fingerprint proves the
/// accepted certificate, and readiness proves both listeners are up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeReport {
    /// Operating-system process identity of this child.
    pub pid: u32,
    /// Bifrost target this child serves.
    pub target: ProcessNodeTarget,
    /// Stable runtime node identity this child registered under.
    pub node_id: uuid::Uuid,
    /// Bound public HTTP socket.
    pub http_addr: String,
    /// Bound public gRPC socket.
    pub grpc_addr: String,
    /// Bound private peer socket.
    pub peer_addr: String,
    /// Endpoint this child published into cluster membership.
    pub advertise_addr: String,
    /// SHA-256 fingerprint of this child's own peer leaf certificate.
    pub peer_certificate_fingerprint: String,
    /// Whether every readiness probe passed at report time.
    pub ready: bool,
    /// Durable Scribe root this child mounted.
    pub wal_root: String,
    /// Live role membership this child could observe when it answered.
    pub membership: Vec<MembershipEntry>,
}

/// One live role lease as observed by the child that reported it.
///
/// Projected out of the runtime snapshot rather than read from Postgres by the
/// parent, because the claim under test is what a pod can see and dial, not
/// what a row says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipEntry {
    /// Runtime node identity holding the lease.
    pub node_id: uuid::Uuid,
    /// Independently fenced role, rendered as its wire name.
    pub role: String,
    /// Exact private address the role published.
    pub address: String,
    /// Whether the role advertised readiness.
    pub ready: bool,
    /// Monotonic fence this role incarnation holds.
    pub fencing_token: u64,
}

impl MembershipEntry {
    /// Projects every live Scribe and Oracle lease in one snapshot.
    ///
    /// The result is sorted by role then address so two children observing the
    /// same membership produce comparable reports.
    #[must_use]
    pub fn project(snapshot: &vala_bifrost_redux::cluster::ClusterSnapshot) -> Vec<Self> {
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

/// Why a process-cluster operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ProcessClusterError {
    /// A shared resource could not be prepared.
    #[error("process cluster resource failed: {0}")]
    Resource(String),
    /// A child could not be launched or supervised.
    #[error("process cluster child failed: {0}")]
    Child(String),
    /// A control message was missing, oversized, or unparseable.
    #[error("process cluster control protocol failed: {0}")]
    Protocol(String),
    /// A child did not answer within its deadline.
    #[error("process cluster timed out waiting for {0}")]
    Timeout(String),
}

/// How this platform gives each simulated pod a distinct address.
///
/// A deployment gives every pod its own IP and uses one canonical port. Linux
/// reproduces that directly because the whole `127.0.0.0/8` block is local.
/// macOS routes only `127.0.0.1` unless an operator adds loopback aliases, so
/// the harness falls back to distinct ports on one address and says so, rather
/// than silently claiming pod-like addressing it did not achieve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressPlan {
    /// One distinct loopback address per pod, canonical ports.
    DistinctLoopbackAddresses,
    /// One loopback address, distinct ephemeral ports per pod.
    DistinctPortsOnLocalhost,
}

impl AddressPlan {
    /// Detects whether this platform can assign distinct loopback addresses.
    #[must_use]
    pub fn detect() -> Self {
        match TcpListener::bind((Ipv4Addr::new(127, 0, 0, 2), 0)) {
            Ok(listener) => {
                drop(listener);
                Self::DistinctLoopbackAddresses
            }
            Err(_) => Self::DistinctPortsOnLocalhost,
        }
    }

    /// Allocates the three sockets for the pod at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when an ephemeral port cannot
    /// be reserved under the fallback plan.
    fn sockets(self, index: usize) -> Result<PodSockets, ProcessClusterError> {
        match self {
            Self::DistinctLoopbackAddresses => {
                let host = Ipv4Addr::new(
                    127,
                    0,
                    0,
                    u8::try_from(index + 2).map_err(|_| {
                        ProcessClusterError::Resource(
                            "process cluster exceeded the loopback address block".to_owned(),
                        )
                    })?,
                );
                Ok(PodSockets {
                    http: SocketAddr::from((host, CANONICAL_PUBLIC_HTTP_PORT)),
                    grpc: SocketAddr::from((host, CANONICAL_PUBLIC_GRPC_PORT)),
                    peer: SocketAddr::from((host, CANONICAL_PEER_PORT)),
                })
            }
            Self::DistinctPortsOnLocalhost => Ok(PodSockets {
                http: reserve_ephemeral()?,
                grpc: reserve_ephemeral()?,
                peer: reserve_ephemeral()?,
            }),
        }
    }
}

/// Reserves one currently free loopback socket.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Resource`] when binding or address lookup
/// fails.
fn reserve_ephemeral() -> Result<SocketAddr, ProcessClusterError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
    listener
        .local_addr()
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))
}

/// The three sockets one simulated pod binds.
#[derive(Debug, Clone, Copy)]
struct PodSockets {
    /// Public HTTP socket.
    http: SocketAddr,
    /// Public gRPC socket.
    grpc: SocketAddr,
    /// Private peer socket.
    peer: SocketAddr,
}

/// Bounded in-memory tail of one child's stderr.
///
/// Bounded because a child that fails in a loop would otherwise fill the
/// parent's heap with a failure it has already recorded; the oldest complete
/// lines are dropped so the most recent context survives into the failure
/// message.
#[derive(Debug, Default)]
struct StderrTail {
    /// Retained complete lines, oldest first.
    lines: VecDeque<String>,
}

impl StderrTail {
    /// Records one complete line, evicting the oldest when full.
    fn push(&mut self, line: String) {
        if self.lines.len() == STDERR_TAIL_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Renders the retained lines for a failure message.
    fn render(&self) -> String {
        self.lines.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// A deliberate flaw injected into one child's peer identity material.
///
/// The peer plane is mandatory for a peer-bearing target, so each defect must
/// stop that child from starting at all. A journey uses these to prove the
/// server refuses to serve without complete mutual-TLS material rather than
/// silently degrading to a server-authenticated or plaintext listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerTlsDefect {
    /// Complete, correctly issued material.
    None,
    /// The configured trust root is absent, so no client can be verified.
    MissingCa,
    /// The configured certificate chain is absent.
    MissingCertificate,
    /// The configured private key is absent.
    MissingPrivateKey,
}

impl PeerTlsDefect {
    /// Applies this defect to already materialized peer identity files.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the targeted file cannot
    /// be removed.
    fn apply(self, tls: &TestBifrostPeerTls) -> Result<(), ProcessClusterError> {
        let removed = match self {
            Self::None => return Ok(()),
            Self::MissingCa => &tls.ca_path,
            Self::MissingCertificate => &tls.certificate_path,
            Self::MissingPrivateKey => &tls.private_key_path,
        };
        std::fs::remove_file(removed)
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))
    }
}

/// What happens to a child's durable Scribe volume before it is relaunched.
///
/// A Scribe's runtime `NodeId` lives on that volume, so these are exactly the
/// operational events that decide whether a restarted pod is the same node, a
/// new node, or a node that must refuse to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeAction {
    /// Keep the volume, reproducing an ordinary pod restart.
    Retain,
    /// Replace the volume, reproducing node loss or a fresh claim.
    Reset,
    /// Leave unparseable bytes where the identity document belongs.
    Malformed,
    /// Leave a structurally valid but incomplete identity document.
    Partial,
}

impl VolumeAction {
    /// Prepares `wal_root` for the relaunch this action describes.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the volume cannot be
    /// recreated or the identity document cannot be written.
    fn apply(self, wal_root: &Path) -> Result<(), ProcessClusterError> {
        let identity = wal_root.join(IDENTITY_FILE_NAME);
        let resource = |error: std::io::Error| ProcessClusterError::Resource(error.to_string());
        match self {
            Self::Retain => Ok(()),
            Self::Reset => {
                std::fs::remove_dir_all(wal_root).map_err(resource)?;
                std::fs::create_dir_all(wal_root).map_err(resource)
            }
            Self::Malformed => {
                std::fs::write(&identity, b"{ this is not identity state").map_err(resource)
            }
            Self::Partial => std::fs::write(&identity, br#"{"version":1}"#).map_err(resource),
        }
    }
}

/// File the server persists a Scribe's runtime node identity into.
///
/// Mirrored here rather than imported so the harness can damage the document
/// without the production store offering a way to write a broken one.
const IDENTITY_FILE_NAME: &str = "node-identity.json";

/// Command sent to a child's reaper thread.
enum ReaperCommand {
    /// Terminate the child, then wait for and reap it.
    Kill,
}

/// One simulated pod running as a distinct child process.
///
/// The node owns the child's stdin, its reaper thread, its stdout control
/// reader, and its stderr drain. Teardown joins all three threads, so no work
/// outlives the node and no child outlives its test.
pub struct ProcessNode {
    /// Stable pod label naming this child's private root and certificate.
    label: String,
    /// Bifrost target this child serves.
    target: ProcessNodeTarget,
    /// Report captured when this child announced readiness.
    ready: NodeReport,
    /// Sockets the parent assigned this child.
    sockets: PodSockets,
    /// Child-private root holding its certificates, WAL, and spill.
    root: PathBuf,
    /// Writable end of the child's stdin.
    stdin: Option<std::process::ChildStdin>,
    /// Parsed control responses forwarded by the stdout reader.
    responses: Receiver<Result<ControlResponse, ProcessClusterError>>,
    /// Command channel into the reaper thread.
    reaper: Option<Sender<ReaperCommand>>,
    /// Reaper thread, which owns the `Child` and reports its exit.
    reaper_thread: Option<JoinHandle<()>>,
    /// Stdout control-reader thread.
    stdout_thread: Option<JoinHandle<()>>,
    /// Stderr drain thread.
    stderr_thread: Option<JoinHandle<()>>,
    /// Bounded stderr tail shared with the drain thread.
    stderr: Arc<Mutex<StderrTail>>,
    /// Carries the reaper's own outcome once the child has been waited on.
    ///
    /// A bounded one-slot channel because the reaper reports exactly once: the
    /// first `try_wait`, kill, or wait error it hit, after it finished every
    /// cleanup step that was still possible.
    exited: Receiver<Result<(), String>>,
}

impl std::fmt::Debug for ProcessNode {
    /// Reports identity and addresses without rendering channels or threads.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessNode")
            .field("pid", &self.ready.pid)
            .field("target", &self.target)
            .field("node_id", &self.ready.node_id)
            .field("peer_addr", &self.ready.peer_addr)
            .finish_non_exhaustive()
    }
}

impl ProcessNode {
    /// Returns the readiness report this child published.
    #[must_use]
    pub fn ready_report(&self) -> &NodeReport {
        &self.ready
    }

    /// Returns this child's operating-system process identity.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.ready.pid
    }

    /// Returns the Bifrost target this child serves.
    #[must_use]
    pub fn target(&self) -> ProcessNodeTarget {
        self.target
    }

    /// Returns the stable pod label this child was launched under.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the private peer socket this child bound.
    #[must_use]
    pub fn peer_addr(&self) -> SocketAddr {
        self.sockets.peer
    }

    /// Returns the public gRPC socket this child bound.
    #[must_use]
    pub fn grpc_addr(&self) -> SocketAddr {
        self.sockets.grpc
    }

    /// Returns the public HTTP socket this child bound.
    #[must_use]
    pub fn http_addr(&self) -> SocketAddr {
        self.sockets.http
    }

    /// Returns the child-private root holding its certificates and volumes.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the retained tail of this child's stderr.
    #[must_use]
    pub fn stderr_tail(&self) -> String {
        match self.stderr.lock() {
            Ok(tail) => tail.render(),
            Err(poisoned) => poisoned.into_inner().render(),
        }
    }

    /// Sends one control request and waits for the child's answer.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the child's stdin is closed,
    /// [`ProcessClusterError::Protocol`] when the answer is unparseable, and
    /// [`ProcessClusterError::Timeout`] when the child does not answer within
    /// the control deadline.
    pub fn request(
        &mut self,
        request: &ControlRequest,
    ) -> Result<ControlResponse, ProcessClusterError> {
        self.send(request)?;
        self.await_response(CONTROL_TIMEOUT)
    }

    /// Asks this child to describe itself.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Protocol`] when the child answers with something
    /// other than an inspection.
    pub fn inspect(&mut self) -> Result<NodeReport, ProcessClusterError> {
        match self.request(&ControlRequest::Inspect)? {
            ControlResponse::Inspection(report) => Ok(report),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected an inspection, received {other:?}"
            ))),
        }
    }

    /// Asks this child to register one Bifrost table through its own catalog.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the child could not register it.
    pub fn register_table(&mut self, table: &str) -> Result<(), ProcessClusterError> {
        match self.request(&ControlRequest::RegisterTable {
            table: table.to_owned(),
        })? {
            ControlResponse::Registered => Ok(()),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected a registration, received {other:?}"
            ))),
        }
    }

    /// Asks this child to write and publish deterministic fixture rows.
    ///
    /// The rows are written through this pod's own Scribe and published, so a
    /// later query observes durable state a real replica produced rather than
    /// a fixture-authored file listing. Ids run `start_id..start_id + rows`,
    /// so a caller builds one logical table out of several bounded requests
    /// without any request having to hold the whole table in memory.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the ingest or publication failed.
    pub fn ingest_rows(
        &mut self,
        table: &str,
        start_id: i64,
        rows: i64,
        groups: i64,
    ) -> Result<(), ProcessClusterError> {
        match self.request(&ControlRequest::IngestRows {
            table: table.to_owned(),
            start_id,
            rows,
            groups,
        })? {
            ControlResponse::Ingested => Ok(()),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected an ingest, received {other:?}"
            ))),
        }
    }

    /// Runs one statement and collects its complete in-pod physical evidence.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the statement failed.
    pub fn execute_analytical_baseline(
        &mut self,
        sql: &str,
    ) -> Result<AnalyticalBaselineEvidence, ProcessClusterError> {
        match self.request(&ControlRequest::ExecuteAnalyticalBaseline {
            sql: sql.to_owned(),
        })? {
            ControlResponse::AnalyticalBaseline(evidence) => Ok(*evidence),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(format!(
                "{detail}; stderr tail:\n{}",
                self.stderr_tail()
            ))),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected analytical evidence, received {other:?}"
            ))),
        }
    }

    /// Reads this child's process-owned Oracle scratch occupancy.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the scratch root cannot be measured.
    pub fn scratch_usage(&mut self) -> Result<ScratchUsage, ProcessClusterError> {
        match self.request(&ControlRequest::ScratchUsage)? {
            ControlResponse::ScratchUsage(usage) => Ok(usage),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected scratch usage, received {other:?}"
            ))),
        }
    }

    /// Totals each named production metric family inside this child's process.
    ///
    /// A family this pod has never emitted totals zero, so a caller can take a
    /// baseline before the series exists and still difference against it.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the recorder could not be read.
    pub fn metric_totals(
        &mut self,
        families: &[&str],
    ) -> Result<BTreeMap<String, f64>, ProcessClusterError> {
        match self.request(&ControlRequest::MetricTotals {
            families: families.iter().map(|name| (*name).to_owned()).collect(),
        })? {
            ControlResponse::MetricTotals { totals } => Ok(totals),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected metric totals, received {other:?}"
            ))),
        }
    }

    /// Asks this child to re-read the shared membership snapshot.
    ///
    /// Each pod caches its own view of the cluster, so a journey that changed
    /// membership or published new data refreshes the pods it is about to
    /// query rather than waiting on their background cadence.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the refresh failed.
    pub fn refresh_snapshot(&mut self) -> Result<(), ProcessClusterError> {
        match self.request(&ControlRequest::RefreshSnapshot)? {
            ControlResponse::Refreshed => Ok(()),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected a refresh, received {other:?}"
            ))),
        }
    }

    /// Reports this child's cumulative graph-lease activations and live leases.
    ///
    /// The pair is the exactness evidence a graph lease claims: one activation
    /// per distributed plan on this node regardless of how many stage messages
    /// addressed it, and zero live leases once the plan has ended.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the child could not report.
    pub fn graph_leases(&mut self) -> Result<(u64, usize), ProcessClusterError> {
        match self.request(&ControlRequest::GraphLeases)? {
            ControlResponse::GraphLeases { activated, live } => Ok((activated, live)),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected graph-lease counts, received {other:?}"
            ))),
        }
    }

    /// Reports every Oracle ownership total this child can still account for.
    ///
    /// Taken before and after one execution, an equal pair is the strongest
    /// cleanup claim this harness can make: not that three gauges returned to
    /// zero, but that every production owner the process can name holds exactly
    /// what it held before. Read-only — nothing here releases, resets, or
    /// otherwise touches what it reports.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when this target composes no Oracle or an
    /// ownership lock is poisoned.
    pub fn ownership_snapshot(&mut self) -> Result<OracleOwnershipSnapshot, ProcessClusterError> {
        match self.request(&ControlRequest::OracleOwnership)? {
            ControlResponse::OracleOwnership(snapshot) => Ok(*snapshot),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected an ownership snapshot, received {other:?}"
            ))),
        }
    }

    /// Asks this child to run one statement through inactive Analytical.
    ///
    /// Returns the row count the attempt produced. Nothing in production
    /// routing reaches this seam; the child builds the same authorized
    /// context its public query surface would have built and drives the
    /// stream to its terminal frame, so the caller observes a settled
    /// attempt rather than an abandoned one.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the attempt itself failed.
    pub fn execute_inactive_sql(&mut self, sql: &str) -> Result<usize, ProcessClusterError> {
        match self.request(&ControlRequest::ExecuteInactiveSql {
            sql: sql.to_owned(),
        })? {
            ControlResponse::Executed { rows } => Ok(rows),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected an execution, received {other:?}"
            ))),
        }
    }

    /// Asks this child to dial another pod as its own peer identity.
    ///
    /// Returns the destination's non-secret gRPC outcome. A refusal is an
    /// outcome, not an error: only a failure to reach or be admitted by the
    /// destination is reported as an error.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the dial itself failed.
    pub fn dial_peer(&mut self, address: &str) -> Result<String, ProcessClusterError> {
        self.peer_probe(&PeerProbePlan::own(address))
    }

    /// Asks this child to perform one shaped private-plane probe.
    ///
    /// Returns the destination's non-secret gRPC outcome. A refusal is an
    /// outcome, not an error: only a failure to reach the destination at all
    /// is reported as an error.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the probe itself failed.
    pub fn peer_probe(&mut self, plan: &PeerProbePlan) -> Result<String, ProcessClusterError> {
        match self.request(&ControlRequest::PeerProbe(plan.clone()))? {
            ControlResponse::Probed { outcome } => Ok(outcome),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected a probe outcome, received {other:?}"
            ))),
        }
    }

    /// Reads how many request bodies this child's peer plane has polled.
    ///
    /// The counter is the evidence that authentication precedes body
    /// admission: a refused request must leave it unchanged, and an admitted
    /// one must advance it, so the probe proves itself.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`].
    pub fn peer_body_polls(&mut self) -> Result<u64, ProcessClusterError> {
        match self.request(&ControlRequest::PeerBodyPolls)? {
            ControlResponse::BodyPolls { count } => Ok(count),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected a body-poll count, received {other:?}"
            ))),
        }
    }

    /// Writes one newline-delimited request to the child's stdin.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when stdin is already closed or
    /// the write fails, and [`ProcessClusterError::Protocol`] when the request
    /// cannot be serialized.
    fn send(&mut self, request: &ControlRequest) -> Result<(), ProcessClusterError> {
        let mut line = serde_json::to_vec(request)
            .map_err(|error| ProcessClusterError::Protocol(error.to_string()))?;
        line.push(b'\n');
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| ProcessClusterError::Child("child stdin is closed".to_owned()))?;
        stdin
            .write_all(&line)
            .and_then(|()| stdin.flush())
            .map_err(|error| ProcessClusterError::Child(error.to_string()))
    }

    /// Waits for one control response within `deadline`.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Timeout`] when nothing arrives in time
    /// and [`ProcessClusterError::Child`] when the child's stdout closed.
    fn await_response(&self, deadline: Duration) -> Result<ControlResponse, ProcessClusterError> {
        match self.responses.recv_timeout(deadline) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout) => Err(ProcessClusterError::Timeout(format!(
                "child pid {} control response; stderr tail:\n{}",
                self.ready.pid,
                self.stderr_tail()
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(ProcessClusterError::Child(format!(
                "child pid {} closed its control stream; stderr tail:\n{}",
                self.ready.pid,
                self.stderr_tail()
            ))),
        }
    }

    /// Arms this child's one-shot follower `ExecuteTask` pause.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when this child composes no Oracle.
    pub fn arm_execute_pause(&mut self) -> Result<(), ProcessClusterError> {
        self.expect(&ControlRequest::ArmExecutePause, |response| {
            matches!(response, ControlResponse::PauseArmed)
        })
    }

    /// Blocks until this child is holding an `ExecuteTask` at that pause.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`].
    pub fn await_execute_paused(&mut self) -> Result<(), ProcessClusterError> {
        self.expect(&ControlRequest::AwaitExecutePaused, |response| {
            matches!(response, ControlResponse::ExecutePaused)
        })
    }

    /// Releases the held `ExecuteTask`.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`].
    pub fn release_execute_pause(&mut self) -> Result<(), ProcessClusterError> {
        self.expect(&ControlRequest::ReleaseExecutePause, |response| {
            matches!(response, ControlResponse::PauseReleased)
        })
    }

    /// Starts one statement in this child's single active inactive-query slot.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the slot is already occupied.
    pub fn start_inactive_sql(&mut self, sql: &str) -> Result<(), ProcessClusterError> {
        self.expect(
            &ControlRequest::StartInactiveSql {
                sql: sql.to_owned(),
            },
            |response| matches!(response, ControlResponse::Started),
        )
    }

    /// Cancels the statement occupying that slot.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the slot is empty.
    pub fn cancel_inactive_sql(&mut self) -> Result<(), ProcessClusterError> {
        self.expect(&ControlRequest::CancelInactiveSql, |response| {
            matches!(response, ControlResponse::CancelRequested)
        })
    }

    /// Blocks until that statement reaches its terminal and reports it.
    ///
    /// Returns the row count on success and the terminal failure detail
    /// otherwise. A failed terminal is an outcome, not a harness error.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the slot is empty.
    pub fn await_inactive_sql(&mut self) -> Result<Result<usize, String>, ProcessClusterError> {
        match self.request(&ControlRequest::AwaitInactiveSql)? {
            ControlResponse::InactiveOutcome {
                rows: Some(rows), ..
            } => Ok(Ok(rows)),
            ControlResponse::InactiveOutcome {
                detail: Some(detail),
                ..
            } => Ok(Err(detail)),
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "expected an inactive-query outcome, received {other:?}"
            ))),
        }
    }

    /// Kills and reaps this child without asking it to shut down.
    ///
    /// This is real peer loss: the process disappears mid-graph, so its peers
    /// observe a dead connection rather than a drained one.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the reaper could not
    /// terminate or reap the child, or when a thread this node owns could not
    /// be joined. Every step is still attempted before the first error
    /// returns.
    pub fn kill(&mut self) -> Result<(), ProcessClusterError> {
        self.stdin = None;
        if let Some(reaper) = &self.reaper {
            let _ = reaper.send(ReaperCommand::Kill);
        }
        let reaped = self.await_reaper();
        self.reaper = None;
        let joined = self.join_threads();
        match reaped {
            Err(detail) => Err(ProcessClusterError::Child(detail)),
            Ok(()) => joined,
        }
    }

    /// Waits for the reaper's one report and renders a missing one as failure.
    ///
    /// A reaper that never answers is indistinguishable from one that could not
    /// reap, so the deadline elapsing is itself the process-control error.
    fn await_reaper(&self) -> Result<(), String> {
        match self.exited.recv_timeout(SHUTDOWN_TIMEOUT) {
            Ok(result) => result,
            Err(error) => Err(format!(
                "child pid {} was not reported reaped: {error}",
                self.ready.pid
            )),
        }
    }

    /// Sends one request and requires the child to answer with an exact shape.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::request`], and
    /// [`ProcessClusterError::Child`] when the child reported a failure.
    fn expect(
        &mut self,
        request: &ControlRequest,
        accept: impl Fn(&ControlResponse) -> bool,
    ) -> Result<(), ProcessClusterError> {
        let response = self.request(request)?;
        if accept(&response) {
            return Ok(());
        }
        match response {
            ControlResponse::Failed { detail } => Err(ProcessClusterError::Child(detail)),
            other => Err(ProcessClusterError::Protocol(format!(
                "unexpected answer to {request:?}: {other:?}"
            ))),
        }
    }

    /// Requests ordered shutdown, then reaps and joins everything this node owns.
    ///
    /// Normal teardown sends `Shutdown`, closes stdin, and waits for the child
    /// to exit. A child that does not respond in time is killed and reaped, so
    /// the same call is also the correct path on test failure.
    ///
    /// The first failure in operation order is preserved and returned: a
    /// rejected shutdown request, then a missed graceful exit, then a
    /// forced-kill or reaper failure, then a reader/reaper join failure. Every
    /// later step still runs, so a caller that receives an error has already
    /// had stdin closed, the child terminated, and every thread joined. A
    /// forced kill is itself a failure even when it and the joins succeed:
    /// this node was asked to shut down and did not.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] carrying that first failure.
    pub fn shutdown(&mut self) -> Result<(), ProcessClusterError> {
        let pid = self.ready.pid;
        let mut first: Option<String> = None;
        if let Err(error) = self.send(&ControlRequest::Shutdown) {
            first = Some(format!(
                "child pid {pid} rejected its shutdown request: {error}"
            ));
        }
        self.stdin = None;
        let mut reaped = None;
        if first.is_none() {
            match self.exited.recv_timeout(SHUTDOWN_TIMEOUT) {
                Ok(result) => reaped = Some(result),
                Err(error) => {
                    first = Some(format!(
                        "child pid {pid} did not exit within its shutdown deadline: {error}"
                    ));
                }
            }
        }
        if reaped.is_none() {
            if let Some(reaper) = &self.reaper {
                let _ = reaper.send(ReaperCommand::Kill);
            }
            reaped = Some(self.await_reaper());
        }
        self.reaper = None;
        if let Some(Err(detail)) = reaped
            && first.is_none()
        {
            first = Some(detail);
        }
        let joined = self.join_threads();
        first.map_or(joined, |detail| Err(ProcessClusterError::Child(detail)))
    }

    /// Joins the reaper and both reader threads.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when any thread panicked.
    fn join_threads(&mut self) -> Result<(), ProcessClusterError> {
        let mut panicked = Vec::new();
        for (name, handle) in [
            ("reaper", self.reaper_thread.take()),
            ("stdout reader", self.stdout_thread.take()),
            ("stderr drain", self.stderr_thread.take()),
        ] {
            if let Some(handle) = handle
                && handle.join().is_err()
            {
                panicked.push(name);
            }
        }
        if panicked.is_empty() {
            Ok(())
        } else {
            Err(ProcessClusterError::Child(format!(
                "child pid {} threads panicked: {}",
                self.ready.pid,
                panicked.join(", ")
            )))
        }
    }
}

impl Drop for ProcessNode {
    /// Guarantees no child survives its test, even on panic.
    fn drop(&mut self) {
        if self.reaper_thread.is_some() || self.stdout_thread.is_some() {
            let _ = self.shutdown();
        }
    }
}

/// Shared resources every simulated pod in one cluster attaches to.
///
/// These are exactly the things real replicas share. Everything else — server
/// composition, runtime, listeners, configuration, WAL, spill, temporary roots
/// — is per-child, because sharing any of it would let a journey pass on
/// in-process coupling that a deployment does not have.
struct SharedClusterResources {
    /// Fixture database the parent created, migrated, and seeded.
    fixture: PgFixture,
    /// Local object-store root shared by every child.
    storage_root: tempfile::TempDir,
    /// Peer certificate authority every child's leaf chains to.
    peer_ca: BifrostPeerCa,
    /// One peer ticket keyring published to every child in this topology.
    ///
    /// Peer authority is verified against a published manifest, so a cluster
    /// whose children each generated their own keyring could not accept one
    /// another's tickets. Generating it once here is what makes the topology a
    /// peer plane rather than a set of strangers.
    peer_keyring: TestPeerKeyring,
    /// Root holding each child's private directory.
    node_roots: tempfile::TempDir,
    /// API key of the one shared Bifrost peer Service principal.
    ///
    /// Provisioned once by the parent because the seeding is not idempotent,
    /// and delivered to each child as a file under that child's private root
    /// rather than as an argument or an environment value.
    peer_api_key: secrecy::SecretString,
}

/// A Bifrost peer network of independently launched server processes.
///
/// The requested Oracle replica count is a parameter with no compiled maximum:
/// the one-Oracle topology proves the complete listener, identity, membership,
/// readiness, and cleanup contract with no remote follower, and larger
/// topologies add remote peers without changing any node's configuration.
pub struct BifrostProcessCluster {
    /// Resources shared exactly as real replicas share them.
    shared: SharedClusterResources,
    /// Running children, in launch order.
    nodes: Vec<ProcessNode>,
    /// How this platform assigned each pod its address.
    address_plan: AddressPlan,
    /// Compiled support binary each simulated pod runs.
    binary: PathBuf,
}

impl std::fmt::Debug for BifrostProcessCluster {
    /// Reports topology without rendering shared secrets or handles.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BifrostProcessCluster")
            .field("nodes", &self.nodes)
            .field("address_plan", &self.address_plan)
            .finish_non_exhaustive()
    }
}

/// Everything that decides how one child is launched.
///
/// Kept as one value because a relaunch must reproduce a pod exactly — same
/// label, root, target, and sockets — while varying only the two things a
/// journey deliberately perturbs.
struct LaunchPlan {
    /// Stable pod label naming the private root and certificate.
    label: String,
    /// Bifrost target this child serves.
    target: ProcessNodeTarget,
    /// Sockets the parent assigned this pod.
    sockets: PodSockets,
    /// Deliberate flaw injected into the peer identity material.
    defect: PeerTlsDefect,
    /// What happens to the durable volume before launch.
    volume: VolumeAction,
}

impl BifrostProcessCluster {
    /// Launches one child per requested target over freshly shared resources.
    ///
    /// `binary` is the compiled `bifrost_peer_test_node` support binary. Cargo
    /// publishes its path to integration tests as
    /// `CARGO_BIN_EXE_bifrost_peer_test_node`, so a caller passes that rather
    /// than letting the harness build or locate a binary itself.
    ///
    /// Every child is a distinct PID with its own composition, listeners, and
    /// roots; only the database, object store, peer CA, and peer principal are
    /// shared. The call returns once every child has reported `Ready`, which a
    /// child emits only after both sockets are bound and its selected role
    /// membership is ready at its exact advertised address.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when a shared resource cannot
    /// be prepared, [`ProcessClusterError::Child`] when a child cannot be
    /// launched, and [`ProcessClusterError::Timeout`] when a child does not
    /// become ready. Children launched before a failure are shut down and
    /// reaped before the error returns.
    pub async fn start(
        binary: impl Into<PathBuf>,
        targets: &[ProcessNodeTarget],
    ) -> Result<Self, ProcessClusterError> {
        let fixture = Arc::new(
            PgFixture::start()
                .await
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?,
        );
        let peer_api_key = crate::server::provision_bifrost_peer_principal(
            &fixture,
            crate::server::PeerPrincipalShape::Canonical,
        )
        .await
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        let fixture = Arc::into_inner(fixture).ok_or_else(|| {
            ProcessClusterError::Resource(
                "peer principal provisioning retained the fixture".to_owned(),
            )
        })?;
        let shared = SharedClusterResources {
            fixture,
            peer_api_key,
            storage_root: tempfile::tempdir()
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?,
            peer_keyring: TestPeerKeyring::generate(),
            peer_ca: BifrostPeerCa::generate("localhost")
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?,
            node_roots: tempfile::tempdir()
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?,
        };
        let address_plan = AddressPlan::detect();
        let mut cluster = Self {
            shared,
            nodes: Vec::new(),
            address_plan,
            binary: binary.into(),
        };
        for (index, target) in targets.iter().copied().enumerate() {
            let plan = LaunchPlan {
                label: format!("pod-{index}"),
                target,
                sockets: address_plan.sockets(index)?,
                defect: PeerTlsDefect::None,
                volume: VolumeAction::Retain,
            };
            match cluster.launch(&plan) {
                Ok(node) => cluster.nodes.push(node),
                Err(error) => {
                    // Clean cleanup leaves the launch failure untouched; a
                    // caller diagnosing a launch should not have to read past
                    // teardown noise. Cleanup that itself failed is the more
                    // serious condition and is reported alongside it.
                    return match cluster.shutdown() {
                        Ok(()) => Err(error),
                        Err(cleanup) => Err(ProcessClusterError::Child(format!(
                            "child launch failed: {error}; cleanup after it also failed: {cleanup}"
                        ))),
                    };
                }
            }
        }
        Ok(cluster)
    }

    /// Returns how this platform assigned each pod its address.
    #[must_use]
    pub fn address_plan(&self) -> AddressPlan {
        self.address_plan
    }

    /// Returns the running children in launch order.
    #[must_use]
    pub fn nodes(&self) -> &[ProcessNode] {
        &self.nodes
    }

    /// Returns the running children mutably, for control requests.
    pub fn nodes_mut(&mut self) -> &mut [ProcessNode] {
        &mut self.nodes
    }

    /// Returns the fixture database this cluster's children share.
    #[must_use]
    pub fn fixture(&self) -> &PgFixture {
        &self.shared.fixture
    }

    /// Shuts down and reaps every child, joining all owned threads.
    ///
    /// Every node is attempted in cluster order even after one fails, and the
    /// node list is cleared either way, so a panicking test still leaves no
    /// surviving child. The first child failure is returned with every later
    /// child's label and detail appended, because a journey that only learned
    /// about the first would not know whether the rest were even asked.
    ///
    /// [`Drop`] is the only caller allowed to discard this result.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] naming each child that could not
    /// be shut down, terminated, reaped, or joined.
    pub fn shutdown(&mut self) -> Result<(), ProcessClusterError> {
        let mut failures: Vec<String> = Vec::new();
        for node in &mut self.nodes {
            if let Err(error) = node.shutdown() {
                failures.push(format!("{}: {error}", node.label));
            }
        }
        self.nodes.clear();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(ProcessClusterError::Child(failures.join("; ")))
        }
    }

    /// Returns the peer ticket keyring every child in this cluster loads.
    ///
    /// A journey mints tickets with it directly — under the active key, a
    /// retired one, or one no manifest publishes — which is the only way to
    /// drive rotation and independence from outside the nodes.
    #[must_use]
    pub fn peer_keyring(&self) -> &TestPeerKeyring {
        &self.shared.peer_keyring
    }

    /// Returns the authority every child's peer leaf chains to.
    ///
    /// A journey needs it to dial the peer plane itself: to present a trusted
    /// client certificate, to present one from a foreign authority, and to
    /// verify a served certificate under the wrong name.
    #[must_use]
    pub fn peer_ca(&self) -> &BifrostPeerCa {
        &self.shared.peer_ca
    }

    /// Seeds one public Service principal in the shared fixture tenant.
    ///
    /// Returns its API key, which a journey uses to drive a real public query
    /// against any child's public listener. The principal lives in the shared
    /// database, so one key works against every pod, exactly as a deployment's
    /// caller credential does.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the principal cannot be
    /// provisioned.
    pub async fn provision_public_api_key(
        &self,
        name: &str,
    ) -> Result<secrecy::SecretString, ProcessClusterError> {
        crate::server::provision_tenant_service_principal(
            &self.shared.fixture,
            self.shared.fixture.data_tenant_id(),
            name,
            &["admin"],
        )
        .await
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))
    }

    /// Seeds one deliberately wrong peer Service principal and returns its key.
    ///
    /// A journey uses these to prove the private plane admits exactly one
    /// configured identity: a different SYSTEM_OWNER service, a service with no
    /// peer permission, and a data-tenant service that holds it must all be
    /// refused before the request body is touched.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the principal cannot be
    /// provisioned.
    pub async fn provision_peer_principal(
        &self,
        shape: crate::server::PeerPrincipalShape,
    ) -> Result<secrecy::SecretString, ProcessClusterError> {
        crate::server::provision_bifrost_peer_principal(&self.shared.fixture, shape)
            .await
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))
    }

    /// Launches a throwaway child with damaged peer material and expects it to fail.
    ///
    /// Returns the failure the child reported, joined with its retained stderr,
    /// so the caller can assert on the cause rather than merely on the absence
    /// of a running process. The child gets its own label, root, and ephemeral
    /// sockets, so a failed probe never disturbs the live topology.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the damaged child
    /// nevertheless started and reported readiness, because that is the exact
    /// negative claim under test.
    pub fn probe_startup_failure(
        &self,
        label: &str,
        target: ProcessNodeTarget,
        defect: PeerTlsDefect,
    ) -> Result<String, ProcessClusterError> {
        let plan = LaunchPlan {
            label: label.to_owned(),
            target,
            // Always ephemeral: a probe must never contend for a live pod's
            // canonical socket, and it is never dialed.
            sockets: AddressPlan::DistinctPortsOnLocalhost.sockets(0)?,
            defect,
            volume: VolumeAction::Retain,
        };
        match self.launch(&plan) {
            Ok(mut node) => {
                let report = node.ready_report().clone();
                let cleanup = node.shutdown();
                // The probe's own claim stays primary: the damaged child was
                // supposed to refuse to start, and it did not. A cleanup
                // failure is appended rather than substituted.
                let mut detail =
                    format!("child started with {defect:?} peer material and reported {report:?}");
                if let Err(error) = cleanup {
                    detail.push_str(&format!("; cleanup after it also failed: {error}"));
                }
                Err(ProcessClusterError::Child(detail))
            }
            Err(error) => Ok(error.to_string()),
        }
    }

    /// Stops the child at `index` and launches its replacement over `volume`.
    ///
    /// The replacement keeps the original label, root, target, and sockets, so
    /// it is the same pod coming back rather than a new one. `volume` decides
    /// whether it finds its durable state, a fresh disk, or a damaged identity
    /// document.
    ///
    /// # Errors
    ///
    /// Returns whatever the relaunch fails with. On failure the slot is
    /// removed, so later indices shift; a journey that expects a failed
    /// relaunch must not address earlier nodes by index afterwards.
    pub fn restart(
        &mut self,
        index: usize,
        volume: VolumeAction,
    ) -> Result<&NodeReport, ProcessClusterError> {
        if index >= self.nodes.len() {
            return Err(ProcessClusterError::Resource(format!(
                "no process node at index {index}"
            )));
        }
        let mut previous = self.nodes.remove(index);
        let plan = LaunchPlan {
            label: previous.label.clone(),
            target: previous.target,
            sockets: previous.sockets,
            defect: PeerTlsDefect::None,
            volume,
        };
        // The replacement reuses this pod's label, root, and sockets, so it
        // cannot be launched until the previous process has actually released
        // them. A failed shutdown leaves the slot empty rather than racing a
        // survivor for its own address.
        let stopped = previous.shutdown();
        drop(previous);
        stopped?;
        let node = self.launch(&plan)?;
        self.nodes.insert(index, node);
        Ok(self.nodes[index].ready_report())
    }

    /// Launches one child and waits for its readiness report.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the child's private root
    /// or certificate cannot be created, [`ProcessClusterError::Child`] when
    /// the process cannot be spawned or its pipes are missing, and
    /// [`ProcessClusterError::Timeout`] when it does not report readiness.
    fn launch(&self, plan: &LaunchPlan) -> Result<ProcessNode, ProcessClusterError> {
        let label = plan.label.clone();
        let root = self.shared.node_roots.path().join(&label);
        let wal_root = root.join("wal");
        let spill_root = root.join("spill");
        for directory in [&root, &wal_root, &spill_root] {
            std::fs::create_dir_all(directory)
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        }
        plan.volume.apply(&wal_root)?;
        // The leaf lands under this child's own private root, and only its path
        // is passed on. No key material enters the child's argv or environment
        // value set beyond a filesystem path.
        let tls = self
            .shared
            .peer_ca
            .materialize(&root, &label)
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        plan.defect.apply(&tls)?;
        // Same private-root discipline as the certificate: the active signing
        // key is written under this child's own root and only its path is
        // published.
        let keyring = self
            .shared
            .peer_keyring
            .materialize(&root, &label)
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        // Same reasoning as the certificate: the secret lands in a file under
        // this child's private root and only the path is published.
        let peer_api_key_path = root.join("peer-api-key");
        std::fs::write(
            &peer_api_key_path,
            secrecy::ExposeSecret::expose_secret(&self.shared.peer_api_key),
        )
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        let sockets = plan.sockets;

        let mut command = Command::new(&self.binary);
        command
            .env(env::DATABASE, self.shared.fixture.database_name())
            .env(
                env::TENANT_ID,
                self.shared.fixture.data_tenant_id().as_uuid().to_string(),
            )
            .env(env::TENANT_SLUG, self.shared.fixture.tenant_slug())
            .env(env::STORAGE_ROOT, self.shared.storage_root.path())
            .env(env::TARGET, plan.target.as_str())
            .env(env::WAL_ROOT, &wal_root)
            .env(env::SPILL_ROOT, &spill_root)
            .env(env::HTTP_BIND, sockets.http.to_string())
            .env(env::GRPC_BIND, sockets.grpc.to_string())
            .env(env::PEER_BIND, sockets.peer.to_string())
            .env(env::PEER_CA_PATH, &tls.ca_path)
            .env(env::PEER_CERT_PATH, &tls.certificate_path)
            .env(env::PEER_KEY_PATH, &tls.private_key_path)
            .env(env::PEER_SERVER_NAME, &tls.server_name)
            .env(env::PEER_API_KEY_PATH, &peer_api_key_path)
            .env(env::PEER_TICKET_KEY_ID, &keyring.active_key_id)
            .env(env::PEER_TICKET_KEY_PATH, &keyring.signing_key_path)
            .env(
                env::PEER_TICKET_KEYRING_PATH,
                &keyring.verifying_keyring_path,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?;

        let pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ProcessClusterError::Child("child stdin was not piped".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProcessClusterError::Child("child stdout was not piped".to_owned()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProcessClusterError::Child("child stderr was not piped".to_owned()))?;

        let (response_tx, response_rx) = channel();
        let stdout_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match read_bounded_line(&mut reader, &mut line) {
                    Ok(0) => return,
                    Ok(_) => {
                        let parsed = serde_json::from_str::<ControlResponse>(line.trim())
                            .map_err(|error| ProcessClusterError::Protocol(error.to_string()));
                        if response_tx.send(parsed).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = response_tx.send(Err(error));
                        return;
                    }
                }
            }
        });

        let tail = Arc::new(Mutex::new(StderrTail::default()));
        let drain_tail = Arc::clone(&tail);
        // A child's own view is otherwise reachable only through the retained
        // tail, which a journey renders solely when a control call fails. An
        // assertion failure — the common case while diagnosing — would show
        // nothing at all, so a diagnosing run that already asked for logs
        // through `RUST_LOG` gets every child line on the parent's stderr.
        let echo = std::env::var_os("RUST_LOG").is_some();
        let stderr_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else { return };
                if echo {
                    eprintln!("[child {pid}] {line}");
                }
                match drain_tail.lock() {
                    Ok(mut tail) => tail.push(line),
                    Err(poisoned) => poisoned.into_inner().push(line),
                }
            }
        });

        let (command_tx, command_rx) = channel::<ReaperCommand>();
        let (exit_tx, exit_rx) = std::sync::mpsc::sync_channel(1);
        let reaper_thread = std::thread::spawn(move || reap(child, &command_rx, &exit_tx));

        let mut node = ProcessNode {
            label: plan.label.clone(),
            target: plan.target,
            ready: NodeReport {
                pid,
                target: plan.target,
                node_id: uuid::Uuid::nil(),
                http_addr: sockets.http.to_string(),
                grpc_addr: sockets.grpc.to_string(),
                peer_addr: sockets.peer.to_string(),
                advertise_addr: String::new(),
                peer_certificate_fingerprint: String::new(),
                ready: false,
                wal_root: wal_root.display().to_string(),
                membership: Vec::new(),
            },
            sockets,
            root,
            stdin: Some(stdin),
            responses: response_rx,
            reaper: Some(command_tx),
            reaper_thread: Some(reaper_thread),
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
            stderr: tail,
            exited: exit_rx,
        };
        node.ready = match node.await_response(READY_TIMEOUT)? {
            ControlResponse::Ready(report) => report,
            ControlResponse::Failed { detail } => {
                return Err(ProcessClusterError::Child(format!(
                    "child pid {pid} failed to start: {detail}\nstderr tail:\n{}",
                    node.stderr_tail()
                )));
            }
            other => {
                return Err(ProcessClusterError::Protocol(format!(
                    "expected a readiness report, received {other:?}"
                )));
            }
        };
        Ok(node)
    }
}

impl Drop for BifrostProcessCluster {
    /// Guarantees no child survives the cluster, even on panic.
    ///
    /// The only caller permitted to discard a shutdown result: `Drop` has no
    /// way to return one, and the alternative — panicking during teardown —
    /// would hide whatever the test was actually failing on.
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Reports a natural child exit that ended in failure.
///
/// A child that exited on its own says whether its own shutdown was clean
/// through its exit status, so a non-success status is the only evidence the
/// parent has that ordered shutdown did not complete. A child the parent
/// deliberately killed is never routed here: its non-success status is the
/// expected result of the kill, not a shutdown failure.
fn natural_exit_failure(pid: u32, status: &std::process::ExitStatus) -> Option<String> {
    (!status.success())
        .then(|| format!("child pid {pid} exited with {status} instead of shutting down cleanly"))
}

/// Owns one `Child`, waiting for normal exit while accepting a kill command.
///
/// Separated onto its own thread because the parent must be able to both wait
/// for an orderly exit and force one, and `std::process::Child` offers no way
/// to do that from a single blocking call.
fn reap(
    mut child: Child,
    commands: &Receiver<ReaperCommand>,
    exited: &std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let pid = child.id();
    let mut first: Option<String> = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // The child chose its own exit, so its status is the parent's
                // only report of whether that exit was an ordered one.
                if let Some(detail) = natural_exit_failure(pid, &status) {
                    first.get_or_insert(detail);
                }
                break;
            }
            Ok(None) => {}
            Err(error) => {
                // The status is unreadable, so this loop can no longer decide
                // anything; the unconditional wait below is still attempted.
                first.get_or_insert(format!("child pid {pid} status is unreadable: {error}"));
                break;
            }
        }
        match commands.recv_timeout(Duration::from_millis(50)) {
            Ok(ReaperCommand::Kill) | Err(RecvTimeoutError::Disconnected) => {
                if let Err(error) = child.kill() {
                    first.get_or_insert(format!("child pid {pid} could not be killed: {error}"));
                }
                if let Err(error) = child.wait() {
                    first.get_or_insert(format!("child pid {pid} could not be reaped: {error}"));
                }
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    // Unconditional, so a child that exited on its own is still reaped.
    if let Err(error) = child.wait() {
        first.get_or_insert(format!("child pid {pid} could not be reaped: {error}"));
    }
    // Reported only after every cleanup step that was still possible, so a
    // caller that receives an error has already had the child reaped for it.
    let _ = exited.send(first.map_or(Ok(()), Err));
}

/// Reads one newline-terminated line, refusing an oversized one.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Protocol`] when the line exceeds
/// [`MAX_CONTROL_LINE_BYTES`] and [`ProcessClusterError::Child`] when the pipe
/// read fails.
fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    line: &mut String,
) -> Result<usize, ProcessClusterError> {
    let mut total = 0;
    loop {
        let mut byte = [0_u8; 1];
        match std::io::Read::read(reader, &mut byte) {
            Ok(0) => return Ok(total),
            Ok(_) => {
                total += 1;
                if total > MAX_CONTROL_LINE_BYTES {
                    return Err(ProcessClusterError::Protocol(
                        "child control line exceeded the accepted bound".to_owned(),
                    ));
                }
                if byte[0] == b'\n' {
                    return Ok(total);
                }
                line.push(char::from(byte[0]));
            }
            Err(error) => return Err(ProcessClusterError::Child(error.to_string())),
        }
    }
}

/// Re-exported for the child binary, which is a separate crate root.
pub use child::run_peer_test_node;

/// Child-process side of the peer-network harness.
///
/// Lives in the library rather than in `src/bin` because the control protocol
/// types are shared with the parent and because composing a server needs
/// crate-internal builder seats a separate binary crate cannot reach.
mod child;

#[cfg(test)]
mod tests {
    use super::*;

    /// The ownership control shape survives the control protocol unchanged.
    ///
    /// The snapshot is the parent's only view of a child's retained ownership,
    /// so a field that silently fails to encode would turn a leak into a
    /// passing comparison. Encoding it as the real `ControlResponse` and back
    /// pins both the round trip and the shape itself: fixed scalar fields, no
    /// unbounded collection, and no tenant, query, graph, or node identity.
    ///
    /// # Panics
    ///
    /// Panics when the response does not encode, decode, or compare equal.
    #[test]
    fn oracle_ownership_snapshot_round_trips() {
        let snapshot = OracleOwnershipSnapshot {
            leader_attempts: 1,
            leader_graphs: 2,
            leader_cleanup_failures: 3,
            follower_attempts: 4,
            follower_graphs: 5,
            follower_cleanup_failures: 6,
            active_queries: 7,
            queued_queries: 8,
            reserved_memory_bytes: 9,
            reserved_spill_bytes: 10,
            peer_pending: 11,
            peer_running: 12,
            root_active_queries: 13,
            root_analytical_queries: 14,
            root_query_slot_units: 15,
            root_query_memory_used_bytes: 16,
            root_query_scratch_used_bytes: 17,
            root_query_active: true,
            scratch: ScratchUsage {
                entries: 18,
                bytes: 19,
            },
            attempts_active: 20.0,
            exchanges_active: 21.0,
            fragments_active: 22.0,
        };

        let encoded = serde_json::to_string(&ControlResponse::OracleOwnership(Box::new(snapshot)))
            .expect("the ownership response encodes");
        let decoded: ControlResponse =
            serde_json::from_str(&encoded).expect("the ownership response decodes");
        match decoded {
            ControlResponse::OracleOwnership(returned) => assert_eq!(
                *returned, snapshot,
                "every ownership field survives the control protocol"
            ),
            other => panic!("expected an ownership snapshot, received {other:?}"),
        }

        let fields: std::collections::BTreeSet<String> =
            match serde_json::to_value(snapshot).expect("the snapshot encodes as an object") {
                serde_json::Value::Object(map) => {
                    for (name, value) in &map {
                        assert!(
                            value.is_number() || value.is_boolean() || name == "scratch",
                            "{name} is neither a scalar nor the bounded scratch pair"
                        );
                    }
                    map.keys().cloned().collect()
                }
                other => panic!("the snapshot is not an object: {other:?}"),
            };
        assert_eq!(
            fields.len(),
            22,
            "the ownership shape gained or lost a field without this pin moving"
        );
        assert!(
            !fields.iter().any(|name| name.contains("id")
                || name.contains("tenant")
                || name.contains("label")
                || name.contains("sql")),
            "an ownership snapshot carries no identity: {fields:?}"
        );
    }

    /// An oversized control line is refused rather than buffered.
    ///
    /// Pins the bound that keeps a runaway child from growing the parent's
    /// heap through one unterminated line.
    #[test]
    fn an_oversized_control_line_is_refused() {
        let oversized = "x".repeat(MAX_CONTROL_LINE_BYTES + 1);
        let mut reader = std::io::Cursor::new(oversized.into_bytes());
        let mut line = String::new();
        assert!(matches!(
            read_bounded_line(&mut reader, &mut line),
            Err(ProcessClusterError::Protocol(_))
        ));
    }

    /// A bounded line is read without its terminator.
    #[test]
    fn a_bounded_control_line_is_read_without_its_terminator() {
        let mut reader = std::io::Cursor::new(b"{\"type\":\"Inspect\"}\n".to_vec());
        let mut line = String::new();
        let read = read_bounded_line(&mut reader, &mut line).expect("line reads");
        assert_eq!(read, 19);
        assert_eq!(line, "{\"type\":\"Inspect\"}");
    }

    /// The stderr tail keeps the newest lines and drops the oldest.
    #[test]
    fn the_stderr_tail_drops_the_oldest_lines_when_full() {
        let mut tail = StderrTail::default();
        for index in 0..STDERR_TAIL_LINES + 10 {
            tail.push(format!("line {index}"));
        }
        let rendered = tail.render();
        assert!(!rendered.contains("line 0\n"));
        assert!(rendered.ends_with(&format!("line {}", STDERR_TAIL_LINES + 9)));
    }

    /// A child that exits non-zero on its own is a shutdown failure.
    #[cfg(unix)]
    #[test]
    fn a_natural_non_zero_child_exit_is_a_shutdown_failure() {
        use std::os::unix::process::ExitStatusExt as _;
        let failed = std::process::ExitStatus::from_raw(1 << 8);
        let detail = natural_exit_failure(4242, &failed).expect("a failed exit is reported");
        assert!(detail.contains("4242"), "{detail} names the child");
        assert_eq!(
            natural_exit_failure(4242, &std::process::ExitStatus::from_raw(0)),
            None
        );
    }

    /// Every target round-trips through its wire name.
    #[test]
    fn every_target_round_trips_through_its_wire_name() {
        for target in [
            ProcessNodeTarget::All,
            ProcessNodeTarget::Server,
            ProcessNodeTarget::Oracle,
            ProcessNodeTarget::Scribe,
            ProcessNodeTarget::ForgeWorker,
        ] {
            assert_eq!(
                ProcessNodeTarget::parse(target.as_str()).expect("target parses"),
                target
            );
        }
    }
}

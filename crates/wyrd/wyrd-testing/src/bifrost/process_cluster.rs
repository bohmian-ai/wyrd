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

use std::collections::VecDeque;
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
    /// Begin ordered shutdown and exit.
    Shutdown,
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
        /// Attempt ordinal that produced them.
        attempt: u32,
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
    /// Signalled by the reaper once the child has been waited on.
    exited: Receiver<()>,
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

    /// Requests ordered shutdown, then reaps and joins everything this node owns.
    ///
    /// Normal teardown sends `Shutdown`, closes stdin, and waits for the child
    /// to exit. A child that does not respond in time is killed and reaped, so
    /// the same call is also the correct path on test failure.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when a reader or reaper thread
    /// could not be joined, which is a harness defect rather than detached
    /// work left running.
    pub fn shutdown(&mut self) -> Result<(), ProcessClusterError> {
        let requested = self.send(&ControlRequest::Shutdown).is_ok();
        self.stdin = None;
        let exited = requested && self.exited.recv_timeout(SHUTDOWN_TIMEOUT).is_ok();
        if !exited {
            if let Some(reaper) = &self.reaper {
                let _ = reaper.send(ReaperCommand::Kill);
            }
            let _ = self.exited.recv_timeout(SHUTDOWN_TIMEOUT);
        }
        self.reaper = None;
        self.join_threads()
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
        let peer_api_key = crate::server::provision_oracle_peer_principal(&fixture)
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
            match cluster.launch(index, target) {
                Ok(node) => cluster.nodes.push(node),
                Err(error) => {
                    cluster.shutdown();
                    return Err(error);
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
    /// Called explicitly at the end of a journey and again from `Drop`, so a
    /// panicking test leaves no surviving child.
    pub fn shutdown(&mut self) {
        for node in &mut self.nodes {
            let _ = node.shutdown();
        }
        self.nodes.clear();
    }

    /// Launches one child and waits for its readiness report.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the child's private root
    /// or certificate cannot be created, [`ProcessClusterError::Child`] when
    /// the process cannot be spawned or its pipes are missing, and
    /// [`ProcessClusterError::Timeout`] when it does not report readiness.
    fn launch(
        &self,
        index: usize,
        target: ProcessNodeTarget,
    ) -> Result<ProcessNode, ProcessClusterError> {
        let label = format!("pod-{index}");
        let root = self.shared.node_roots.path().join(&label);
        let wal_root = root.join("wal");
        let spill_root = root.join("spill");
        for directory in [&root, &wal_root, &spill_root] {
            std::fs::create_dir_all(directory)
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        }
        // The leaf lands under this child's own private root, and only its path
        // is passed on. No key material enters the child's argv or environment
        // value set beyond a filesystem path.
        let tls = self
            .shared
            .peer_ca
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
        let sockets = self.address_plan.sockets(index)?;

        let mut command = Command::new(&self.binary);
        command
            .env(env::DATABASE, self.shared.fixture.database_name())
            .env(
                env::TENANT_ID,
                self.shared.fixture.data_tenant_id().as_uuid().to_string(),
            )
            .env(env::TENANT_SLUG, self.shared.fixture.tenant_slug())
            .env(env::STORAGE_ROOT, self.shared.storage_root.path())
            .env(env::TARGET, target.as_str())
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
        let stderr_thread = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else { return };
                match drain_tail.lock() {
                    Ok(mut tail) => tail.push(line),
                    Err(poisoned) => poisoned.into_inner().push(line),
                }
            }
        });

        let (command_tx, command_rx) = channel::<ReaperCommand>();
        let (exit_tx, exit_rx) = channel();
        let reaper_thread = std::thread::spawn(move || reap(child, &command_rx, &exit_tx));

        let mut node = ProcessNode {
            target,
            ready: NodeReport {
                pid,
                target,
                node_id: uuid::Uuid::nil(),
                http_addr: sockets.http.to_string(),
                grpc_addr: sockets.grpc.to_string(),
                peer_addr: sockets.peer.to_string(),
                advertise_addr: String::new(),
                peer_certificate_fingerprint: String::new(),
                ready: false,
                wal_root: wal_root.display().to_string(),
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
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Owns one `Child`, waiting for normal exit while accepting a kill command.
///
/// Separated onto its own thread because the parent must be able to both wait
/// for an orderly exit and force one, and `std::process::Child` offers no way
/// to do that from a single blocking call.
fn reap(mut child: Child, commands: &Receiver<ReaperCommand>, exited: &Sender<()>) {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(_) => break,
        }
        match commands.recv_timeout(Duration::from_millis(50)) {
            Ok(ReaperCommand::Kill) | Err(RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    // Unconditional, so a child that exited on its own is still reaped.
    let _ = child.wait();
    let _ = exited.send(());
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

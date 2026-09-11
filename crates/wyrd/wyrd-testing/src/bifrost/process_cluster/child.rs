//! Child-process side of the multi-process Bifrost peer network.
//!
//! Each simulated pod runs this code as its own `main`. It composes one real
//! `wyrd-server` from the shared resources its parent published, waits until
//! both listeners are bound and its selected role membership is ready at its
//! exact advertised address, and only then announces `Ready`. After that it
//! serves the private control protocol on stdin/stdout until it is told to
//! shut down.
//!
//! Everything a real replica owns privately is owned privately here: the
//! composition, the runtime, the listeners, the configuration, the WAL, the
//! spill root, and the certificate. Only the database, the object store, the
//! peer CA, and the peer Service principal come from the parent.

use std::io::{BufRead as _, Write as _};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use secrecy::SecretString;
use sha2::{Digest as _, Sha256};
use vala_bifrost_redux::oracle::OraclePreparationPause;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_server::config::BifrostTarget;

use super::{
    ControlRequest, ControlResponse, MembershipEntry, NodeReport, PeerProbeCredential,
    PeerProbeFraming, PeerProbePlan, PeerProbeTransport, ProcessClusterError, ProcessNodeTarget,
    env,
};
use crate::bifrost::peer_keyring::TestPeerKeyringPaths;
use crate::server::{TestBifrostPeerTls, WyrdTestServer};

/// Fixed process-visible resources every simulated pod boots under.
///
/// A deployment gives every pod an explicitly bounded envelope, and the whole
/// point of a physical baseline is that the grant, the partition count, and the
/// spill threshold are the same numbers on every run. Observing the host
/// instead would make every physical assertion a property of whichever machine
/// ran the test.
///
/// An Oracle pod is deliberately the tightest of the two: 512 MiB less the
/// 256 MiB unmanaged reserve is the budget the query grant is derived from. A
/// Scribe or Forge pod is sized to complete one table lifecycle instead, which
/// its own boot-time capacity check refuses to do inside the Oracle envelope.
const fn pod_system_resources(
    target: ProcessNodeTarget,
) -> vala_bifrost_redux::resources::SystemResourceSnapshot {
    let memory_limit_bytes = match target {
        ProcessNodeTarget::Oracle => 512 * 1024 * 1024,
        _ => 2 * 1024 * 1024 * 1024,
    };
    vala_bifrost_redux::resources::SystemResourceSnapshot {
        memory_limit_bytes,
        effective_cpu: 4,
        scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
        scratch_available_bytes: 4 * 1024 * 1024 * 1024,
        memory_source: vala_bifrost_redux::resources::ResourceSource::Injected,
        cpu_source: vala_bifrost_redux::resources::ResourceSource::Injected,
    }
}

/// How long a child waits for its own readiness before reporting failure.
const READY_DEADLINE: Duration = Duration::from_secs(60);

/// Request deadline every statement a control request executes carries.
///
/// Named rather than inlined because the parent's `CONTROL_TIMEOUT` is derived
/// from it: the parent must outwait the whole budget one request may legitimately
/// spend, or a slow-but-live statement reads as an unresponsive child.
pub(super) const STATEMENT_DEADLINE_MS: u64 = 30_000;

/// [`STATEMENT_DEADLINE_MS`] as the duration the parent's budget is built from.
pub(super) const STATEMENT_DEADLINE: Duration = Duration::from_millis(STATEMENT_DEADLINE_MS);

/// How long a child waits for an executed statement's graph to settle.
///
/// Spent after the statement's own deadline, so one control request can take
/// this long on top of [`STATEMENT_DEADLINE`].
pub(super) const SETTLEMENT_WAIT: Duration = Duration::from_secs(30);

/// Interval between readiness observations.
const READY_POLL: Duration = Duration::from_millis(100);

/// Runs one simulated pod until its parent shuts it down.
///
/// Returns a failure exit code only when the child could not report anything
/// at all; every other failure is reported to the parent as a structured
/// [`ControlResponse::Failed`] so a journey sees a cause rather than a dead
/// pipe.
#[must_use]
pub fn run_peer_test_node() -> ExitCode {
    install_child_tracing();
    let runtime = wyrd_runtime::runtime();
    match runtime.block_on(serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The parent may already be gone; stderr is drained either way.
            let _ = emit(&ControlResponse::Failed {
                detail: error.to_string(),
            });
            eprintln!("bifrost peer test node failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Composes the server, announces readiness, and serves the control protocol.
///
/// # Errors
///
/// Returns [`ProcessClusterError`] when the environment is incomplete, shared
/// resources cannot be attached, the server cannot be composed or bound, or
/// readiness is not reached within [`READY_DEADLINE`].
async fn serve() -> Result<(), ProcessClusterError> {
    let config = ChildConfig::from_env()?;
    // The recorder alone, installed before composition: the Oracle's
    // pre-registered analytical families must exist from boot, or an absent
    // family would be indistinguishable from a counter that never moved. The
    // trace half of the shared installation is deliberately not taken — its
    // formatting layer writes to stdout, which in a child is the control
    // protocol itself.
    let telemetry = crate::bifrost::BifrostTelemetryCapture::new(
        wyrd_server::app::metrics::install_recorder()
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?,
        wyrd_telemetry::TestTraceCapture::default(),
    );
    let fingerprint = config.certificate_fingerprint()?;
    let (server, credentials, fixture) = config.start().await?;
    let report = await_ready(&server, &config, fingerprint).await?;
    emit(&ControlResponse::Ready(report.clone()))?;

    // One armed pause and one active statement at a time: a journey that needs
    // two is describing a different topology, not a deeper control protocol.
    let mut pause: Option<Arc<vala_bifrost_redux::oracle::analytical::AnalyticalExecutePause>> =
        None;
    let mut active: Option<InactiveQuerySlot> = None;
    let mut preparation_pause: Option<Arc<OraclePreparationPause>> = None;
    let mut cleanup_pause = None;
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        let read = stdin
            .lock()
            .read_line(&mut line)
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
        if read == 0 {
            break;
        }
        let request = match serde_json::from_str::<ControlRequest>(line.trim()) {
            Ok(request) => request,
            Err(error) => {
                emit(&ControlResponse::Failed {
                    detail: format!("unparseable control request: {error}"),
                })?;
                continue;
            }
        };
        match request {
            ControlRequest::Inspect => {
                let report = describe(
                    &server,
                    &config,
                    report.peer_certificate_fingerprint.clone(),
                )
                .await;
                emit(&ControlResponse::Inspection(report))?;
            }
            ControlRequest::RegisterTable { table } => {
                match config.register_table(&server, &table).await {
                    Ok(()) => emit(&ControlResponse::Registered)?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::IngestRows {
                table,
                start_id,
                rows,
                groups,
            } => match config
                .ingest_rows(&server, &table, start_id, rows, groups)
                .await
            {
                Ok(()) => emit(&ControlResponse::Ingested)?,
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::Flush => match server.flush_bifrost().await {
                Ok(()) => emit(&ControlResponse::Flushed)?,
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::ExecuteAnalyticalBaseline { sql } => {
                match config.execute_analytical_baseline(&server, &sql).await {
                    Ok(evidence) => {
                        emit(&ControlResponse::AnalyticalBaseline(Box::new(evidence)))?;
                    }
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::ScratchUsage => match oracle(&server)
                .and_then(|engine| scratch_usage(engine.analytical_spill_root()))
            {
                Ok(usage) => emit(&ControlResponse::ScratchUsage(usage))?,
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::MetricTotals { families, labels } => {
                match metric_totals(&telemetry, &families, &labels) {
                    Ok(totals) => emit(&ControlResponse::MetricTotals { totals })?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::RefreshSnapshot => match refresh_snapshot(&server).await {
                Ok(()) => emit(&ControlResponse::Refreshed)?,
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::GraphLeases => {
                let (activated, live) = graph_lease_counts(&server);
                emit(&ControlResponse::GraphLeases { activated, live })?;
            }
            ControlRequest::OracleOwnership => match ownership_snapshot(&server, &telemetry) {
                Ok(snapshot) => {
                    emit(&ControlResponse::OracleOwnership(Box::new(snapshot)))?;
                }
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::PeerProbe(plan) => {
                match config
                    .peer_probe(&plan, credentials.as_ref(), &fixture)
                    .await
                {
                    Ok(outcome) => emit(&ControlResponse::Probed { outcome })?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::PhysicalBuildEvidence => {
                let (total, latest_cut_fingerprint) =
                    vala_bifrost_redux::oracle::physical_build_observation_for_test();
                let active_cut_fingerprints = match oracle(&server) {
                    Ok(engine) => {
                        let registry = engine.running_queries();
                        registry
                            .list(config.tenant_id)
                            .iter()
                            .filter_map(|summary| {
                                registry
                                    .get(config.tenant_id, &summary.request_id)
                                    .map(|entry| entry.participant_cut().fingerprint())
                            })
                            .collect()
                    }
                    Err(_) => Vec::new(),
                };
                emit(&ControlResponse::PhysicalBuilds(
                    super::PhysicalBuildEvidence {
                        total,
                        latest_cut_fingerprint,
                        active_cut_fingerprints,
                    },
                ))?;
            }
            ControlRequest::PeerBodyPolls => {
                emit(&ControlResponse::BodyPolls {
                    count: wyrd_server::grpc::peer_body_polls(),
                })?;
            }
            ControlRequest::ExecuteInactiveSql { sql } => {
                match config.execute_inactive_sql(&server, &sql).await {
                    Ok(rows) => emit(&ControlResponse::Executed { rows })?,
                    Err(error) => emit(&ControlResponse::Failed {
                        detail: error.to_string(),
                    })?,
                }
            }
            ControlRequest::ArmAnalyticalPlanFailure => match oracle(&server) {
                Ok(engine) => {
                    engine.fail_next_analytical_plan_for_test();
                    emit(&ControlResponse::PlanFailureArmed)?;
                }
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::ArmPreparationPause { request_id } => match oracle(&server) {
                Ok(engine) => {
                    let armed = Arc::new(OraclePreparationPause::new(request_id));
                    engine
                        .bind_preparation_pause_for_test(Some(Arc::clone(&armed)))
                        .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
                    preparation_pause = Some(armed);
                    emit(&ControlResponse::PauseArmed)?;
                }
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::ObservePreparationPause => {
                emit(&ControlResponse::PreparationPauseState {
                    deadline_ms: preparation_pause
                        .as_ref()
                        .and_then(|pause| pause.observed_deadline_ms()),
                })?;
            }
            ControlRequest::ReleasePreparationPause => {
                if let Some(armed) = preparation_pause.take() {
                    armed.release();
                }
                oracle(&server)?
                    .bind_preparation_pause_for_test(None)
                    .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
                emit(&ControlResponse::PauseReleased)?;
            }
            ControlRequest::ArmCleanupPause => {
                let armed =
                    vala_bifrost_redux::oracle::analytical::analytical_cleanup_pause_for_test();
                armed.arm();
                cleanup_pause = Some(armed);
                emit(&ControlResponse::PauseArmed)?;
            }
            ControlRequest::AwaitCleanupPaused => match cleanup_pause.as_ref() {
                Some(armed) => {
                    tokio::time::timeout(STATEMENT_DEADLINE, armed.wait_entered())
                        .await
                        .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
                    emit(&ControlResponse::ExecutePaused)?;
                }
                None => emit(&ControlResponse::Failed {
                    detail: "no cleanup pause armed".to_owned(),
                })?,
            },
            ControlRequest::ReleaseCleanupPause => {
                if let Some(armed) = cleanup_pause.take() {
                    armed.release();
                }
                emit(&ControlResponse::PauseReleased)?;
            }
            ControlRequest::ArmExecutePause => match arm_execute_pause(&server) {
                Ok(armed) => {
                    pause = Some(armed);
                    emit(&ControlResponse::PauseArmed)?;
                }
                Err(error) => emit(&ControlResponse::Failed {
                    detail: error.to_string(),
                })?,
            },
            ControlRequest::AwaitExecutePaused => match pause.as_ref() {
                Some(armed) => {
                    armed.wait_paused().await;
                    emit(&ControlResponse::ExecutePaused)?;
                }
                None => emit(&ControlResponse::Failed {
                    detail: "no execute pause is armed".to_owned(),
                })?,
            },
            ControlRequest::ReleaseExecutePause => match pause.as_ref() {
                Some(armed) => {
                    armed.release();
                    emit(&ControlResponse::PauseReleased)?;
                }
                None => emit(&ControlResponse::Failed {
                    detail: "no execute pause is armed".to_owned(),
                })?,
            },
            ControlRequest::StartInactiveSql { sql } => {
                if active.is_some() {
                    emit(&ControlResponse::Failed {
                        detail: "the inactive-query slot is already occupied".to_owned(),
                    })?;
                } else {
                    match oracle(&server) {
                        Ok(engine) => {
                            active = Some(InactiveQuerySlot::start(engine, config.tenant_id, sql));
                            emit(&ControlResponse::Started)?;
                        }
                        Err(error) => emit(&ControlResponse::Failed {
                            detail: error.to_string(),
                        })?,
                    }
                }
            }
            ControlRequest::CancelInactiveSql => match active.as_ref() {
                Some(slot) => {
                    slot.cancel();
                    emit(&ControlResponse::CancelRequested)?;
                }
                None => emit(&ControlResponse::Failed {
                    detail: "the inactive-query slot is empty".to_owned(),
                })?,
            },
            ControlRequest::AwaitInactiveSql => match active.take() {
                Some(slot) => {
                    let (rows, detail) = slot.join().await;
                    emit(&ControlResponse::InactiveOutcome { rows, detail })?;
                }
                None => emit(&ControlResponse::Failed {
                    detail: "the inactive-query slot is empty".to_owned(),
                })?,
            },
            ControlRequest::Shutdown => {
                emit(&ControlResponse::ShuttingDown)?;
                break;
            }
        }
    }
    if let Some(armed) = preparation_pause.take() {
        armed.release();
    }
    if let Some(armed) = cleanup_pause.take() {
        armed.release();
    }
    server
        .shutdown()
        .await
        .map_err(|error| ProcessClusterError::Child(error.to_string()))
}

/// The one statement a child may have in flight outside its control loop.
///
/// The control protocol is strictly request/response, so a journey that needs
/// to act on a query while it is running — pause a follower, kill it, cancel
/// from the leader — cannot use the synchronous execute request. This owns that
/// statement's task and its cancellation, and nothing else about it.
struct InactiveQuerySlot {
    /// The running statement.
    task: tokio::task::JoinHandle<Result<usize, ProcessClusterError>>,
    /// Cancellation the leader selects on, mirroring a caller drop.
    cancel: tokio_util::sync::CancellationToken,
}

impl InactiveQuerySlot {
    /// Starts one statement and returns immediately.
    fn start(
        engine: Arc<vala_bifrost_redux::oracle::Oracle>,
        tenant_id: wyrd_spec::DataTenantId,
        sql: String,
    ) -> Self {
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let mut fold = ResultFold::default();
            tokio::select! {
                () = token.cancelled() => Err(ProcessClusterError::Child(
                    "the inactive attempt was cancelled by its caller".to_owned(),
                )),
                outcome = drive_inactive_sql(engine, tenant_id, sql, &mut fold) => outcome,
            }
        });
        Self { task, cancel }
    }

    /// Cancels the statement exactly as a dropped caller would.
    fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Joins the statement and reports its terminal.
    ///
    /// A panicked task is reported as a terminal failure rather than
    /// propagated, so a journey names the claim that broke instead of losing
    /// the child.
    async fn join(self) -> (Option<usize>, Option<String>) {
        match self.task.await {
            Ok(Ok(rows)) => (Some(rows), None),
            Ok(Err(error)) => (None, Some(error.to_string())),
            Err(error) => (None, Some(error.to_string())),
        }
    }
}

/// Arms this child's one-shot follower `ExecuteTask` pause.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when this target composes no Oracle
/// or no Analytical execution handle.
fn arm_execute_pause(
    server: &WyrdTestServer,
) -> Result<Arc<vala_bifrost_redux::oracle::analytical::AnalyticalExecutePause>, ProcessClusterError>
{
    let engine = oracle(server)?;
    let handle = engine.analytical_execution().ok_or_else(|| {
        ProcessClusterError::Child("this Oracle composed no Analytical handle".to_owned())
    })?;
    let pause = Arc::new(vala_bifrost_redux::oracle::analytical::AnalyticalExecutePause::default());
    handle
        .worker()
        .bind_execute_pause_for_test(Arc::clone(&pause));
    Ok(pause)
}

/// Installs this child's log subscriber on stderr when `RUST_LOG` asks for one.
///
/// Stderr, never stdout: stdout carries the control protocol, and a log line
/// written there would be read by the parent as a malformed response. The
/// subscriber is installed only when `RUST_LOG` is set, so a lane run stays
/// silent and a diagnosing run gets the child's own view of a multi-process
/// failure, which the parent otherwise cannot see at all.
fn install_child_tracing() {
    let Ok(filter) = std::env::var("RUST_LOG") else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(std::io::stderr)
        .finish();
    // A child installs exactly one subscriber; a failure here means something
    // already owns the global, which is not worth failing a journey over.
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Writes one newline-delimited control response to stdout.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Protocol`] when the response cannot be
/// serialized and [`ProcessClusterError::Child`] when stdout cannot be written.
fn emit(response: &ControlResponse) -> Result<(), ProcessClusterError> {
    let mut line = serde_json::to_vec(response)
        .map_err(|error| ProcessClusterError::Protocol(error.to_string()))?;
    line.push(b'\n');
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle
        .write_all(&line)
        .and_then(|()| handle.flush())
        .map_err(|error| ProcessClusterError::Child(error.to_string()))
}

/// Everything one child reads from its environment.
///
/// Secrets arrive as filesystem paths under this child's own private root, so
/// no key material or API key is ever an argument, an environment value, or a
/// control message.
struct ChildConfig {
    /// Fixture database this child attaches to.
    database: String,
    /// Seeded data tenant identity.
    tenant_id: wyrd_spec::DataTenantId,
    /// Seeded data tenant slug.
    tenant_slug: String,
    /// Shared local object-store root.
    storage_root: PathBuf,
    /// Bifrost target this child serves.
    target: ProcessNodeTarget,
    /// Child-private durable Scribe root.
    wal_root: PathBuf,
    /// Child-private durable spill root.
    spill_root: PathBuf,
    /// Public HTTP socket.
    http_bind: SocketAddr,
    /// Public gRPC socket.
    grpc_bind: SocketAddr,
    /// Private peer socket.
    peer_bind: SocketAddr,
    /// This child's own peer identity material.
    peer_tls: TestBifrostPeerTls,
    /// Path to the shared peer Service API key.
    peer_api_key_path: PathBuf,
    /// Paths of the shared peer ticket keyring published to this child.
    peer_keyring: TestPeerKeyringPaths,
    /// Oracle query slot units this child admits with, when the topology states
    /// one. `None` keeps the memory-derived count every pod ran on before a
    /// journey needed to saturate an admission class deterministically.
    oracle_query_slot_limit: Option<usize>,
}

/// Accepts one query's rows only behind a fully validated success terminal.
///
/// This is the point at which rows already decoded off the wire either become
/// a result or become nothing. Every check is the production contract's own:
/// the closed terminal matrix for the requested visibility, the emitted-row
/// reconciliation, the outcome requirement, and the decoder's explicit
/// end-of-stream. `Degraded` is refused here rather than in the contract
/// because a partial cut is not a baseline result, and the contract has no
/// opinion on which outcomes a given caller will accept.
///
/// Private to this module on purpose: it exists so the terminal decision the
/// process journey depends on is directly testable rather than buried in one
/// long stream loop.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when the terminal is malformed for
/// the requested visibility, disagrees with the rows emitted before it, is not
/// [`wyrd_spec::vala::api::QueryTerminalOutcome::Success`], or does not close
/// the decoder with its own explicit Arrow IPC end-of-stream.
fn accept_query_terminal(
    terminal: &wyrd_spec::vala::api::QueryTerminalFrame,
    visibility: wyrd_spec::vala::api::VisibilityMode,
    emitted_rows: usize,
    decoder: &mut vala_bifrost_redux::oracle::QueryIpcDecoder,
) -> Result<usize, ProcessClusterError> {
    let child = |detail: String| ProcessClusterError::Child(detail);
    terminal
        .validate(visibility)
        .map_err(|error| child(error.to_string()))?;
    terminal
        .validate_emitted_rows(u64::try_from(emitted_rows).unwrap_or(u64::MAX))
        .map_err(|error| child(error.to_string()))?;
    if terminal.outcome != wyrd_spec::vala::api::QueryTerminalOutcome::Success {
        return Err(child(format!(
            "the inactive attempt ended on a {:?} terminal",
            terminal.outcome
        )));
    }
    decoder
        .accept_eos(&terminal.arrow_ipc_eos)
        .map_err(|error| child(error.to_string()))?;
    if !decoder.eos_accepted() {
        return Err(child(
            "the inactive attempt never closed its Arrow IPC stream".to_owned(),
        ));
    }
    Ok(emitted_rows)
}

impl ChildConfig {
    /// Reads and validates the complete child environment.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when a required variable is
    /// missing or cannot be parsed.
    fn from_env() -> Result<Self, ProcessClusterError> {
        let read = |name: &str| -> Result<String, ProcessClusterError> {
            std::env::var(name).map_err(|_| {
                ProcessClusterError::Resource(format!("child environment is missing {name}"))
            })
        };
        let socket = |name: &str| -> Result<SocketAddr, ProcessClusterError> {
            read(name)?.parse().map_err(|error| {
                ProcessClusterError::Resource(format!("{name} is not a socket address: {error}"))
            })
        };
        let tenant_uuid: uuid::Uuid = read(env::TENANT_ID)?.parse().map_err(|error| {
            ProcessClusterError::Resource(format!("tenant id is not a UUID: {error}"))
        })?;
        Ok(Self {
            database: read(env::DATABASE)?,
            tenant_id: wyrd_spec::DataTenantId::new(tenant_uuid).map_err(|error| {
                ProcessClusterError::Resource(format!("tenant id is not a tenant key: {error}"))
            })?,
            tenant_slug: read(env::TENANT_SLUG)?,
            storage_root: PathBuf::from(read(env::STORAGE_ROOT)?),
            target: ProcessNodeTarget::parse(&read(env::TARGET)?)?,
            wal_root: PathBuf::from(read(env::WAL_ROOT)?),
            spill_root: PathBuf::from(read(env::SPILL_ROOT)?),
            http_bind: socket(env::HTTP_BIND)?,
            grpc_bind: socket(env::GRPC_BIND)?,
            peer_bind: socket(env::PEER_BIND)?,
            peer_tls: TestBifrostPeerTls {
                certificate_path: PathBuf::from(read(env::PEER_CERT_PATH)?),
                private_key_path: PathBuf::from(read(env::PEER_KEY_PATH)?),
                ca_path: PathBuf::from(read(env::PEER_CA_PATH)?),
                server_name: read(env::PEER_SERVER_NAME)?,
            },
            peer_api_key_path: PathBuf::from(read(env::PEER_API_KEY_PATH)?),
            peer_keyring: TestPeerKeyringPaths {
                active_key_id: read(env::PEER_TICKET_KEY_ID)?,
                signing_key_path: PathBuf::from(read(env::PEER_TICKET_KEY_PATH)?),
                verifying_keyring_path: PathBuf::from(read(env::PEER_TICKET_KEYRING_PATH)?),
            },
            // Optional: a topology that does not state a slot count keeps the
            // memory-derived one, which is what every pod ran on before any
            // journey needed a saturating class.
            oracle_query_slot_limit: match std::env::var(env::ORACLE_QUERY_SLOT_LIMIT) {
                Ok(value) => Some(value.parse().map_err(|error| {
                    ProcessClusterError::Resource(format!(
                        "{} is not a slot count: {error}",
                        env::ORACLE_QUERY_SLOT_LIMIT
                    ))
                })?),
                Err(_) => None,
            },
        })
    }

    /// Returns the SHA-256 digest of this child's own peer certificate file.
    ///
    /// A certificate is public material, so its digest is safe to report and is
    /// what a journey uses to prove which exact leaf a peer accepted.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the certificate cannot be
    /// read.
    fn certificate_fingerprint(&self) -> Result<String, ProcessClusterError> {
        let bytes = std::fs::read(&self.peer_tls.certificate_path)
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        Ok(hex::encode(Sha256::digest(&bytes)))
    }

    /// Reports whether this child has finished coming up.
    ///
    /// A serving target is ready when every readiness probe passes and its peer
    /// plane obligation is met. A dedicated Forge worker composes no listener
    /// and runs no readiness loop at all, so its own composition returning is
    /// the whole of its startup; holding it to the serving probes would wait
    /// forever on a loop that was never spawned.
    fn is_ready(
        &self,
        snapshot: &wyrd_server::components::health::ReadinessSnapshot,
        state: &wyrd_server::state::AppState,
    ) -> bool {
        if self.target == ProcessNodeTarget::ForgeWorker {
            return true;
        }
        snapshot.all_ok() && state.peer_plane.is_satisfied()
    }

    /// Maps this child's target onto the server's own target enum.
    fn server_target(&self) -> BifrostTarget {
        match self.target {
            ProcessNodeTarget::All => BifrostTarget::All,
            ProcessNodeTarget::Server => BifrostTarget::Server,
            ProcessNodeTarget::Oracle => BifrostTarget::Oracle,
            ProcessNodeTarget::Scribe => BifrostTarget::Scribe,
            ProcessNodeTarget::ForgeWorker => BifrostTarget::ForgeWorker,
        }
    }

    /// Composes and binds this child's server over the shared resources.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Resource`] when the fixture, storage, or
    /// peer credentials cannot be attached, and [`ProcessClusterError::Child`]
    /// when the server cannot be composed or bound.
    async fn start(
        &self,
    ) -> Result<
        (
            WyrdTestServer,
            Arc<dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials>,
            Arc<PgFixture>,
        ),
        ProcessClusterError,
    > {
        let fixture = Arc::new(
            PgFixture::attach(
                self.database.clone(),
                self.tenant_id,
                self.tenant_slug.clone(),
            )
            .await
            .map_err(|error| ProcessClusterError::Resource(error.to_string()))?,
        );
        let api_key = SecretString::from(
            std::fs::read_to_string(&self.peer_api_key_path)
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?
                .trim()
                .to_owned(),
        );
        let credentials =
            crate::server::oracle_peer_credentials_from_key(Arc::clone(&fixture), api_key)
                .await
                .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        let storage = wyrd_storage::StorageHandle::from_settings(wyrd_storage::StorageSettings {
            backend: wyrd_storage::BackendConfig::Local {
                root: self.storage_root.clone(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .map_err(|error| ProcessClusterError::Resource(error.to_string()))?;
        let server = WyrdTestServer::builder()
            .with_bifrost_target_for_test(self.server_target())
            .with_forge_process_role_for_test(self.server_target())
            .with_peer_tls(self.peer_tls.clone())
            .with_peer_keyring_paths(self.peer_keyring.clone())
            .with_peer_bind(self.peer_bind)
            .with_bind_addrs_for_test(self.http_bind, self.grpc_bind)
            .with_durable_bifrost_roots(self.wal_root.clone(), self.spill_root.clone())
            .with_system_resources_for_test(pod_system_resources(self.target));
        let server = match self.oracle_query_slot_limit {
            Some(slots) => server.with_oracle_query_slot_limit_for_test(slots),
            None => server,
        };
        let server = server
            .with_oracle_peer_credentials(Arc::clone(&credentials))
            .with_storage_handle(Arc::clone(&storage))
            .start_with_resources(Arc::clone(&fixture), Arc::clone(&storage), None)
            .await
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?
            .bind()
            .await
            .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
        Ok((server, credentials, fixture))
    }

    /// Registers one Bifrost table through this child's own catalog.
    ///
    /// The table is created by a real pod against the shared catalog, so a
    /// journey can then query it through any pod's public listener without the
    /// parent holding a server of its own.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// catalog or the catalog refuses the table.
    async fn register_table(
        &self,
        server: &WyrdTestServer,
        table: &str,
    ) -> Result<(), ProcessClusterError> {
        let catalog = server.state().bifrost_catalog().ok_or_else(|| {
            ProcessClusterError::Child("this target composes no Bifrost catalog".to_owned())
        })?;
        catalog
            .create_table(vala_bifrost_redux::catalog::CreateTableRequest {
                table: vala_bifrost_redux::catalog::TableRef::new(
                    vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
                    table,
                ),
                user_fields: vec![
                    arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
                    arrow::datatypes::Field::new(
                        "filter_key",
                        arrow::datatypes::DataType::Utf8,
                        false,
                    ),
                ],
                tenant: self.tenant_id,
                physical_layout: None,
                audit: None,
            })
            .await
            .map(|_| ())
            .map_err(|error| ProcessClusterError::Child(error.to_string()))
    }

    /// Writes and publishes deterministic fixture rows through this Scribe.
    ///
    /// The batch is admitted through the same logical ingress seam the public
    /// write surface uses and then frozen and published, so the rows a later
    /// query reads are files this pod's own Scribe encoded.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// Scribe or catalog, the table is unregistered, or ingest, freeze, or
    /// publication fails.
    async fn ingest_rows(
        &self,
        server: &WyrdTestServer,
        table: &str,
        start_id: i64,
        rows: i64,
        groups: i64,
    ) -> Result<(), ProcessClusterError> {
        let child = ProcessClusterError::Child;
        let scribe = server
            .bifrost_scribe()
            .ok_or_else(|| child("this target composes no Scribe".to_owned()))?;
        let catalog = server
            .state()
            .bifrost_catalog()
            .ok_or_else(|| child("this target composes no Bifrost catalog".to_owned()))?;
        let table_ref = vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
            table,
        );
        let (fingerprint, _) = catalog
            .table_registration(&table_ref, self.tenant_id)
            .await
            .map_err(|error| child(error.to_string()))?;
        let principal = wyrd_runtime::principal::Principal {
            id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_runtime::principal::PrincipalKind::User,
            tenant_id: self.tenant_id,
            roles: Vec::new(),
            effective_permissions: wyrd_runtime::PermissionSet::new(),
        };
        scribe
            .ingest_native_for_test(vala_bifrost_redux::scribe::NativeIngressTestFrame {
                principal,
                table: table_ref,
                expected_schema_fingerprint: fingerprint,
                request_id: wyrd_spec::request_id::RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                payload: fixture_rows_ipc(start_id, rows, groups)?,
            })
            .await
            .map_err(|error| child(error.to_string()))?;
        server
            .flush_bifrost()
            .await
            .map_err(|error| child(error.to_string()))?;
        Ok(())
    }

    /// Runs one statement through this node's inactive Analytical path.
    ///
    /// The stream is drained to its terminal frame rather than dropped early,
    /// so the graph and attempt guards it carries settle before the parent
    /// inspects the node. Nothing in routing reaches this seam; the child
    /// authenticates the same way the public query service does and hands
    /// Oracle the identical context its own gRPC surface would have built.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// Oracle, the context cannot be authorized, or the attempt fails at
    /// admission, planning, execution, or decode.
    async fn execute_inactive_sql(
        &self,
        server: &WyrdTestServer,
        sql: &str,
    ) -> Result<usize, ProcessClusterError> {
        drive_inactive_sql(
            oracle(server)?,
            self.tenant_id,
            sql.to_owned(),
            &mut ResultFold::default(),
        )
        .await
    }

    /// Runs one statement and collects everything its own pod can observe.
    ///
    /// The grant is read before the statement runs, from an envelope acquired
    /// out of the live resource plan and immediately dropped, so it is the
    /// arithmetic admission will apply rather than a restatement of a constant.
    /// Scratch is measured on both sides of the same statement, and the
    /// physical evidence is folded by the graph lifecycle before the terminal
    /// this call awaits, so it is already retained by the time it is read.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when this target composes no
    /// Oracle or no Analytical handle, the resource plan admits no Analytical
    /// query, scratch cannot be measured, the statement fails, or the
    /// statement's graph does not settle inside the bounded wait.
    async fn execute_analytical_baseline(
        &self,
        server: &WyrdTestServer,
        sql: &str,
    ) -> Result<super::AnalyticalBaselineEvidence, ProcessClusterError> {
        let child = ProcessClusterError::Child;
        let engine = oracle(server)?;
        let granted_memory_bytes = {
            let envelope = engine
                .role_resources()
                .try_acquire_query(
                    vala_bifrost_redux::resources::OracleResourceRequest::for_class(
                        wyrd_spec::vala::api::QueryClass::Analytical,
                        0.0,
                    ),
                )
                .map_err(|error| child(error.to_string()))?;
            let granted = envelope.granted_memory_bytes;
            drop(envelope);
            u64::try_from(granted).unwrap_or(u64::MAX)
        };
        let scratch_root = engine.analytical_spill_root().to_path_buf();
        let scratch_before = scratch_usage(&scratch_root)?;

        let supervisor = engine
            .analytical_execution()
            .ok_or_else(|| child("this target composes no Analytical handle".to_owned()))?
            .supervisor()
            .clone();
        let settled_before = supervisor.settled_graph_count();

        let mut fold = ResultFold::default();
        let rows = drive_inactive_sql(
            Arc::clone(&engine),
            self.tenant_id,
            sql.to_owned(),
            &mut fold,
        )
        .await
        .map_err(|error| child(format!("baseline statement failed: {error}")))?;

        // The terminal frame reaches this caller before the graph's own
        // lifecycle settles, and the physical evidence is folded inside that
        // settlement, so the statement returning is not yet proof the evidence
        // exists. The wait is on the settlement counter rather than on the
        // evidence itself, because a plan with no output sort settles carrying
        // none — waiting for evidence would stall every such statement for the
        // whole bound and then report its predecessor's numbers.
        let deadline = tokio::time::Instant::now() + SETTLEMENT_WAIT;
        while supervisor.settled_graph_count() == settled_before
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let physical = settled_analytical_evidence(
            settled_before,
            supervisor.settled_graph_count(),
            supervisor.settled_physical_evidence(),
        )?;
        let scratch_after = scratch_usage(&scratch_root)?;
        Ok(super::AnalyticalBaselineEvidence {
            rows,
            result_digest: fold.digest(),
            batch_memory_bytes: fold.batch_memory_bytes,
            counts_all_one: fold.counts_all_one,
            keys_strictly_increasing: fold.keys_strictly_increasing,
            granted_memory_bytes,
            scratch_before,
            scratch_after,
            physical,
        })
    }
}

/// Folds one ordered `(Utf8, Int64)` result into assertable evidence.
///
/// Accumulated frame by frame rather than over a retained result set: the
/// baseline's result is a third of a gigabyte of keys, and holding it to
/// re-walk it later would change the very memory behavior under test.
#[derive(Debug)]
struct ResultFold {
    /// Running digest over every ordered `(key, count)` pair.
    digest: Sha256,
    /// Summed `RecordBatch::get_array_memory_size` over every accepted batch.
    batch_memory_bytes: u64,
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
            batch_memory_bytes: 0,
            counts_all_one: true,
            keys_strictly_increasing: true,
            previous_key: None,
        }
    }
}

impl ResultFold {
    /// Accepts one decoded batch, ignoring a batch of any other shape.
    ///
    /// A statement whose result is not `(Utf8, Int64)` contributes only its
    /// array memory size, so this fold stays usable by callers that want the
    /// row count alone.
    fn accept(&mut self, batch: &arrow::record_batch::RecordBatch) {
        self.batch_memory_bytes = self
            .batch_memory_bytes
            .saturating_add(batch.get_array_memory_size() as u64);
        // Column access is by position, so the arity check has to come first:
        // a projection with a single column is a shape this fold accepts and
        // ignores, not one it may index past.
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
            self.digest
                .update((key.len() as u32).to_le_bytes().as_slice());
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
/// An absent root reports zero rather than an error: a pod that has never
/// spilled has no directory to walk, and that is the baseline a journey
/// compares against.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when an existing directory cannot be
/// read, because an unreadable scratch root would otherwise be reported as an
/// empty one.
fn scratch_usage(root: &std::path::Path) -> Result<super::ScratchUsage, ProcessClusterError> {
    let child = ProcessClusterError::Child;
    let mut usage = super::ScratchUsage {
        entries: 0,
        bytes: 0,
    };
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let listing = match std::fs::read_dir(&directory) {
            Ok(listing) => listing,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(child(error.to_string())),
        };
        for entry in listing {
            let entry = entry.map_err(|error| child(error.to_string()))?;
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                // A spill file removed between the listing and the stat is a
                // cleanup that already happened, not a measurement failure.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(child(error.to_string())),
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

/// Totals each requested production metric family in this process.
///
/// Every sample of a family is summed regardless of its labels, and a family
/// this pod has never emitted totals zero, so a caller can baseline a series
/// before it exists.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when the installed recorder renders
/// an exposition this process cannot parse.
fn metric_totals(
    telemetry: &crate::bifrost::BifrostTelemetryCapture,
    families: &[String],
    labels: &std::collections::BTreeMap<String, String>,
) -> Result<std::collections::BTreeMap<String, f64>, ProcessClusterError> {
    let samples = telemetry
        .snapshot()
        .map_err(|error| ProcessClusterError::Child(error.to_string()))?;
    let mut totals: std::collections::BTreeMap<String, f64> = families
        .iter()
        .map(|family| (family.clone(), 0.0))
        .collect();
    for sample in samples {
        let matches_labels = labels
            .iter()
            .all(|(name, value)| sample.labels.get(name) == Some(value));
        if !matches_labels {
            continue;
        }
        if let Some(total) = totals.get_mut(&sample.family) {
            *total += sample.value;
        }
    }
    Ok(totals)
}

/// Resolves this child's own Oracle engine.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when this target composes no Oracle.
fn oracle(
    server: &WyrdTestServer,
) -> Result<Arc<vala_bifrost_redux::oracle::Oracle>, ProcessClusterError> {
    server
        .state()
        .bifrost_query()
        .map(|query| Arc::clone(query.engine()))
        .ok_or_else(|| ProcessClusterError::Child("this target composes no Oracle".to_owned()))
}

/// Drives one statement through the production inactive Analytical path.
///
/// Owns everything it needs, so the same body serves both the synchronous
/// control request and the single active slot a peer-loss journey starts.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when admission, planning, execution,
/// or decoding fails, which is the attempt's own terminal failure.
async fn drive_inactive_sql(
    engine: Arc<vala_bifrost_redux::oracle::Oracle>,
    tenant_id: wyrd_spec::DataTenantId,
    sql: String,
    fold: &mut ResultFold,
) -> Result<usize, ProcessClusterError> {
    {
        let child = |detail: String| ProcessClusterError::Child(detail);
        let sql = sql.as_str();
        let permission = wyrd_runtime::Permission::bifrost_query_read();
        let principal = wyrd_runtime::Principal::new(
            wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            wyrd_runtime::PrincipalKind::User,
            tenant_id,
            Vec::new(),
            wyrd_runtime::permission::PermissionSet::from_iter([permission.clone()]),
        );
        let context = vala_bifrost_redux::oracle::AuthorizedQueryContext::try_new(
            principal,
            tenant_id,
            wyrd_spec::request_id::RequestId::now_v7(),
            None,
            wyrd_spec::vala::api::AuthMethod::Internal,
            permission,
        )
        .map_err(|error| child(error.to_string()))?;
        // Both query identities are allocated independently on purpose: a
        // leaked public identity into the distributed graph, or the reverse,
        // is exactly what the stage authority's isolation exists to refuse.
        let attempt = vala_bifrost_redux::oracle::analytical::AnalyticalAttemptContext {
            public_query_id: vala_bifrost_redux::oracle::analytical::PublicQueryId::from_uuid(
                uuid::Uuid::now_v7(),
            ),
            datafusion_query_id:
                vala_bifrost_redux::oracle::analytical::DataFusionQueryId::from_uuid(
                    uuid::Uuid::now_v7(),
                ),
            snapshot_digest: format!("snapshot-{}", uuid::Uuid::now_v7().simple()),
            permission_digest: format!("permission-{}", uuid::Uuid::now_v7().simple()),
        };
        let mut stream = engine
            .query_sql_inactive_analytical(
                context,
                wyrd_spec::vala::api::BifrostQueryRequest {
                    sql: sql.to_owned(),
                    visibility: wyrd_spec::vala::api::VisibilityMode::PublishedOnly,
                    freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                    deadline_ms: Some(STATEMENT_DEADLINE_MS),
                },
                attempt,
            )
            .await
            .map_err(|error| child(error.to_string()))?;
        let mut decoder = vala_bifrost_redux::oracle::QueryIpcDecoder::new();
        let mut rows = 0;
        let mut terminal = None;
        while let Some(frame) = futures_util::StreamExt::next(&mut stream.frames).await {
            match frame.map_err(|error| child(error.to_string()))? {
                wyrd_spec::vala::api::QueryStreamFrame::Schema(schema) => {
                    decoder
                        .accept_schema(&schema.arrow_ipc_schema)
                        .map_err(|error| child(error.to_string()))?;
                }
                wyrd_spec::vala::api::QueryStreamFrame::Batch(batch) => {
                    let decoded = decoder
                        .accept_batch(&batch.arrow_ipc_batch)
                        .map_err(|error| child(error.to_string()))?;
                    rows += decoded.num_rows();
                    fold.accept(&decoded);
                }
                wyrd_spec::vala::api::QueryStreamFrame::Terminal(frame) => terminal = Some(frame),
            }
        }
        let terminal = terminal
            .ok_or_else(|| child("the inactive attempt emitted no terminal frame".to_owned()))?;
        accept_query_terminal(
            &terminal,
            wyrd_spec::vala::api::VisibilityMode::PublishedOnly,
            rows,
            &mut decoder,
        )
    }
}

impl ChildConfig {
    /// Performs one shaped private-plane probe and returns its gRPC outcome.
    ///
    /// The probe is issued as a raw HTTP/2 request over this child's own
    /// mutually authenticated channel rather than through a generated client,
    /// because the claim under test is about the bytes on the wire: which
    /// adapter is addressed, which workload credential accompanies it, and how
    /// the first gRPC frame is split or coalesced. A refusal is an outcome, not
    /// an error; only failing to reach the destination is an error.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the endpoint cannot be
    /// built, the handshake fails, the credential cannot be obtained, or the
    /// destination never answered.
    async fn peer_probe(
        &self,
        plan: &PeerProbePlan,
        own: &dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials,
        fixture: &Arc<PgFixture>,
    ) -> Result<String, ProcessClusterError> {
        let child = |error: String| ProcessClusterError::Child(error);
        let read = |path: &std::path::Path| -> Result<Vec<u8>, ProcessClusterError> {
            std::fs::read(path).map_err(|error| ProcessClusterError::Resource(error.to_string()))
        };
        let endpoint = match plan.transport {
            PeerProbeTransport::Mutual => {
                wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
                    plan.address.clone(),
                    &read(&self.peer_tls.ca_path)?,
                    self.peer_tls.server_name.clone(),
                    &read(&self.peer_tls.certificate_path)?,
                    &read(&self.peer_tls.private_key_path)?,
                )
                .map_err(|error| child(error.to_string()))?
            }
            // Deliberately built from the raw address with no trust material
            // at all: the destination must refuse the connection itself, so
            // the probe never gets far enough to present a credential.
            PeerProbeTransport::Plaintext => wyrd_tonic::tonic::transport::Endpoint::from_shared(
                plan.address.replace("https://", "http://"),
            )
            .map_err(|error| child(error.to_string()))?,
        };
        let mut channel = endpoint
            .connect()
            .await
            .map_err(|error| child(error.to_string()))?;
        let bearer = self.probe_bearer(&plan.credential, own, fixture).await?;
        let mut request = http::Request::builder()
            .method(http::Method::POST)
            .uri(plan.service.path())
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .header("te", "trailers");
        if let Some(bearer) = bearer {
            request = request.header("x-wyrd-access-token", bearer);
        }
        let request = request
            .body(probe_body(plan.framing, plan.payload.as_deref()))
            .map_err(|error| child(error.to_string()))?;
        let response = tower::ServiceExt::oneshot(&mut channel, request)
            .await
            .map_err(|error| child(error.to_string()))?;
        Ok(probe_outcome(response).await)
    }

    /// Resolves the bearer value a probe presents, if it presents one.
    ///
    /// An API-key credential is exchanged through the same middleware a real
    /// peer uses, so a probe for a deliberately wrong principal is refused by
    /// authorization rather than by a malformed token.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessClusterError::Child`] when the exchange fails.
    async fn probe_bearer(
        &self,
        credential: &PeerProbeCredential,
        own: &dyn vala_bifrost_redux::oracle::dispatcher::OraclePeerCredentials,
        fixture: &Arc<PgFixture>,
    ) -> Result<Option<String>, ProcessClusterError> {
        let child = |error: String| ProcessClusterError::Child(error);
        match credential {
            PeerProbeCredential::Absent => Ok(None),
            PeerProbeCredential::Invalid => Ok(Some("Bearer not-a-real-token".to_owned())),
            PeerProbeCredential::Own => own
                .bearer(false)
                .await
                .map(|bearer| Some(format!("Bearer {bearer}")))
                .map_err(|error| child(error.to_string())),
            PeerProbeCredential::ApiKey(key) => {
                let credentials = crate::server::oracle_peer_credentials_from_key(
                    Arc::clone(fixture),
                    SecretString::from(key.clone()),
                )
                .await
                .map_err(|error| child(error.to_string()))?;
                credentials
                    .bearer(false)
                    .await
                    .map(|bearer| Some(format!("Bearer {bearer}")))
                    .map_err(|error| child(error.to_string()))
            }
        }
    }
}

/// Builds one probe request body laid out as `framing` describes.
///
/// Every variant carries a well-formed first message; only the HTTP/2 frame
/// boundaries differ, which is exactly the property a private listener must be
/// indifferent to.
fn probe_body(framing: PeerProbeFraming, payload: Option<&[u8]>) -> wyrd_tonic::tonic::body::Body {
    // An empty protobuf message is a valid `ReserveNodeSlotsRequest` and a
    // valid oversized-free first frame for the worker adapter, so the probe
    // never depends on a decodable domain payload to reach the boundary. A
    // parent-supplied payload replaces it verbatim, header included, because a
    // ticket binds the digest of exactly those bytes.
    let message = match payload {
        Some(payload) => {
            let mut framed = vec![0_u8];
            framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
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
/// gRPC reports its status in the response headers for a trailers-only
/// refusal and in the trailers otherwise, so both are read before the outcome
/// is decided.
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

/// Polls this child until every readiness probe passes.
///
/// Readiness includes the private peer listener whenever this target composes
/// one, so a peer-bearing child that bound only its public sockets never
/// announces `Ready`. A target that owns no peer plane is not held back by it.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Timeout`] when readiness is not reached
/// within [`READY_DEADLINE`].
async fn await_ready(
    server: &WyrdTestServer,
    config: &ChildConfig,
    fingerprint: String,
) -> Result<NodeReport, ProcessClusterError> {
    let deadline = Instant::now() + READY_DEADLINE;
    loop {
        let report = describe(server, config, fingerprint.clone()).await;
        if report.ready {
            return Ok(report);
        }
        if Instant::now() >= deadline {
            let snapshot = server.state().readiness.load();
            // Naming the probes is what makes a readiness timeout actionable:
            // without them the parent only learns that some dependency of some
            // role never came up.
            return Err(ProcessClusterError::Timeout(format!(
                "child pid {} readiness: postgres={:?} storage={:?} scribe={:?} oracle={:?} \
                 peer={:?} peer_required={} peer_serving={}",
                std::process::id(),
                snapshot.postgres.reason,
                snapshot.storage.reason,
                snapshot.scribe.reason,
                snapshot.oracle.reason,
                snapshot.peer.reason,
                server.state().peer_plane.is_required(),
                server.state().peer_plane.is_serving(),
            )));
        }
        tokio::time::sleep(READY_POLL).await;
    }
}

/// Builds this child's current self-description.
///
/// The advertised address is read back from the composed runtime rather than
/// recomputed from configuration, so a journey observes what this node actually
/// published. Membership is refreshed from Postgres first, because the point of
/// the report is what this pod can currently see of its peers, not what its
/// background refresh happened to cache.
async fn describe(
    server: &WyrdTestServer,
    config: &ChildConfig,
    fingerprint: String,
) -> NodeReport {
    let state = server.state();
    let snapshot = state.readiness.load();
    let membership = match state.bifrost_cluster_for_test() {
        Some(cluster) => {
            let _ = cluster.refresh_snapshot().await;
            MembershipEntry::project(&cluster.snapshot())
        }
        None => Vec::new(),
    };
    let node_id = server.node_id().as_uuid();
    // The advertised address is whatever this node actually published into
    // membership, so a node that registered the wrong endpoint reports it.
    let advertise_addr = membership
        .iter()
        .find(|entry| entry.node_id == node_id)
        .map(|entry| entry.address.clone())
        .unwrap_or_default();
    NodeReport {
        pid: std::process::id(),
        target: config.target,
        node_id,
        http_addr: config.http_bind.to_string(),
        grpc_addr: config.grpc_bind.to_string(),
        peer_addr: config.peer_bind.to_string(),
        advertise_addr,
        peer_certificate_fingerprint: fingerprint,
        ready: config.is_ready(&snapshot, state),
        wal_root: config.wal_root.display().to_string(),
        membership,
    }
}

/// Re-reads the shared membership snapshot into this pod's Oracle.
///
/// Each pod caches its own cluster view, so a journey that just changed
/// membership or published data refreshes the pods it is about to query
/// instead of waiting on their background cadence. A pod that composes no
/// Oracle has nothing to refresh and reports success.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when the snapshot cannot be read.
async fn refresh_snapshot(server: &WyrdTestServer) -> Result<(), ProcessClusterError> {
    let Some(cluster) = server.state().oracle_cluster() else {
        return Ok(());
    };
    cluster
        .refresh_snapshot()
        .await
        .map_err(|error| ProcessClusterError::Child(error.to_string()))
}

/// Reports this pod's cumulative graph-lease activations and live leases.
///
/// A pod that composes no Oracle owns no reservation registry and therefore
/// reports the baseline, which is the correct answer for a Scribe: it never
/// leases a graph.
fn graph_lease_counts(server: &WyrdTestServer) -> (u64, usize) {
    server
        .state()
        .bifrost_query()
        .map_or((0, 0), |oracle| oracle.engine().graph_lease_counts())
}

/// Projects every Oracle ownership total this process can still account for.
///
/// Purely a read: each field is taken from the owner that already maintains it
/// — the Analytical execution handle's own live inspection, Oracle's admission
/// inspection, the process resource root's snapshot, the process scratch tree,
/// and the installed production gauges. Nothing is recomputed here, so a
/// journey comparing two of these is comparing production accounting rather
/// than this harness's arithmetic.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when this target composes no Oracle
/// or Analytical handle, an ownership lock is poisoned, the resource root
/// cannot be read, or scratch cannot be measured.
fn ownership_snapshot(
    server: &WyrdTestServer,
    telemetry: &crate::bifrost::BifrostTelemetryCapture,
) -> Result<super::OracleOwnershipSnapshot, ProcessClusterError> {
    let child = ProcessClusterError::Child;
    let engine = oracle(server)?;
    let live = engine
        .analytical_execution()
        .ok_or_else(|| child("this target composes no Analytical handle".to_owned()))?
        .live()
        .map_err(|error| child(error.to_string()))?;
    let runtime = engine.runtime_inspection();
    let root = engine
        .role_resources()
        .snapshot()
        .map_err(|error| child(error.to_string()))?;
    let scratch = scratch_usage(engine.analytical_spill_root())?;
    let gauges = metric_totals(
        telemetry,
        &[
            "bifrost_oracle_analytical_attempts_active".to_owned(),
            "bifrost_oracle_analytical_exchanges_active".to_owned(),
            "oracle_fragments_active".to_owned(),
        ],
        &std::collections::BTreeMap::new(),
    )?;
    let gauge = |family: &str| gauges.get(family).copied().unwrap_or_default();
    Ok(super::OracleOwnershipSnapshot {
        leader_attempts: live.leader.attempts,
        leader_graphs: live.leader.graphs,
        leader_cleanup_failures: live.leader.cleanup_failures,
        follower_attempts: live.follower.attempts,
        follower_graphs: live.follower.graphs,
        follower_cleanup_failures: live.follower.cleanup_failures,
        active_queries: runtime.active_queries,
        queued_queries: runtime.queued_queries,
        reserved_memory_bytes: runtime.reserved_memory_bytes,
        reserved_spill_bytes: runtime.reserved_spill_bytes,
        peer_pending: runtime.peer_pending,
        peer_running: runtime.peer_running,
        root_active_queries: root.oracle_active_queries,
        root_analytical_queries: root.oracle_analytical_queries,
        root_query_slot_units: root.oracle_query_slot_units,
        root_query_memory_used_bytes: u64::try_from(root.oracle_query_memory_used_bytes)
            .unwrap_or(u64::MAX),
        root_query_scratch_used_bytes: root.oracle_query_scratch_used_bytes,
        root_query_active: root.oracle_query_active,
        scratch,
        attempts_active: gauge("bifrost_oracle_analytical_attempts_active"),
        exchanges_active: gauge("bifrost_oracle_analytical_exchanges_active"),
        fragments_active: gauge("oracle_fragments_active"),
    })
}

/// Encodes `rows` deterministic `(id, filter_key)` rows as one Arrow IPC stream.
///
/// Ids run `start_id..start_id + rows`, so several bounded requests compose one
/// contiguous logical table. The rows are spread over `groups` distinct keys so
/// a grouped aggregate has more than one non-trivial group, which is what makes
/// a distributed plan exchange partitions rather than collapse to a single
/// stage.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when the batch or its IPC encoding
/// cannot be built.
fn fixture_rows_ipc(
    start_id: i64,
    rows: i64,
    groups: i64,
) -> Result<bytes::Bytes, ProcessClusterError> {
    let child = ProcessClusterError::Child;
    let groups = groups.max(1);
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new("filter_key", arrow::datatypes::DataType::Utf8, false),
    ]));
    let ids: Vec<i64> = (start_id..start_id.saturating_add(rows)).collect();
    let keys: Vec<String> = ids
        .iter()
        .map(|id| format!("group_{}", id % groups))
        .collect();
    let batch = arrow::record_batch::RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(arrow::array::Int64Array::from(ids)),
            Arc::new(arrow::array::StringArray::from(keys)),
        ],
    )
    .map_err(|error| child(error.to_string()))?;
    let mut ipc = Vec::new();
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref())
            .map_err(|error| child(error.to_string()))?;
        writer
            .write(&batch)
            .map_err(|error| child(error.to_string()))?;
        writer.finish().map_err(|error| child(error.to_string()))?;
    }
    Ok(bytes::Bytes::from(ipc))
}

/// Admits settled physical evidence only when the settlement counter advanced.
///
/// The supervisor retains the last settled graph's evidence, so a value being
/// present says nothing about which statement folded it. Reading it after the
/// bounded wait expired would attribute a predecessor's spill numbers to this
/// query. The counter is the only fact that separates the two, so it decides.
///
/// # Errors
///
/// Returns [`ProcessClusterError::Child`] when `settled_after` did not advance
/// past `settled_before`, meaning this statement's graph never settled inside
/// the baseline's bounded wait.
fn settled_analytical_evidence(
    settled_before: u64,
    settled_after: u64,
    evidence: Option<vala_bifrost_redux::oracle::analytical::AnalyticalPhysicalEvidence>,
) -> Result<
    Option<vala_bifrost_redux::oracle::analytical::AnalyticalPhysicalEvidence>,
    ProcessClusterError,
> {
    if settled_after <= settled_before {
        return Err(ProcessClusterError::Child(
            "the analytical baseline statement's graph did not settle inside its bounded wait"
                .to_owned(),
        ));
    }
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wyrd_spec::vala::api::{
        QueryFreshness, QuerySource, QueryTerminalError, QueryTerminalErrorCode,
        QueryTerminalFrame, QueryTerminalOutcome, SourceCompletion, SourceCompletionOutcome,
        VisibilityMode,
    };

    use super::{ProcessClusterError, accept_query_terminal, settled_analytical_evidence};

    /// Rows already on the wire are discarded when the terminal is not a
    /// validated success.
    ///
    /// Encodes one real schema and one real batch through the production IPC
    /// encoder, decodes them through the production decoder, then presents a
    /// structurally valid *failed* terminal whose `row_count` matches the rows
    /// that were emitted. Everything except the outcome agrees, so the only
    /// thing that can reject it is the outcome requirement itself.
    ///
    /// Required mutation RED: discard the terminal, or accept any outcome the
    /// contract validates, and the helper returns the row count for a query
    /// that failed after framing began.
    ///
    /// # Panics
    ///
    /// Panics when the fixture stream cannot be encoded or decoded.
    #[test]
    fn inactive_sql_terminal_rejects_failed_output_after_rows() {
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

        let failed = QueryTerminalFrame {
            outcome: QueryTerminalOutcome::Failed,
            freshness: QueryFreshness::Complete,
            execution_path: wyrd_spec::vala::api::QueryExecutionPath::Interactive,
            row_count: u64::try_from(emitted).expect("a fixture row count fits a u64"),
            warnings: Vec::new(),
            source_completion: vec![
                SourceCompletion {
                    source: QuerySource::Iceberg,
                    outcome: SourceCompletionOutcome::Complete,
                },
                SourceCompletion {
                    source: QuerySource::HotSealed,
                    outcome: SourceCompletionOutcome::Complete,
                },
            ],
            error: Some(QueryTerminalError {
                code: QueryTerminalErrorCode::QueryExecutionFailed,
                detail: None,
            }),
            arrow_ipc_eos: Vec::new(),
        };
        failed
            .validate(VisibilityMode::PublishedOnly)
            .expect("the fixture terminal is structurally valid on its own");

        let refused = accept_query_terminal(
            &failed,
            VisibilityMode::PublishedOnly,
            emitted,
            &mut decoder,
        );
        assert!(
            matches!(refused, Err(ProcessClusterError::Child(_))),
            "rows preceding a failed terminal are not a result: {refused:?}"
        );
        assert!(
            !decoder.eos_accepted(),
            "a failed terminal carries no end-of-stream to accept"
        );
    }

    /// A settlement counter that never advanced cannot license physical
    /// evidence, even when a nonempty value is already retained.
    ///
    /// The retained value in this case belongs to whatever settled before this
    /// statement, so accepting it would report a predecessor's spill numbers as
    /// this query's. The advancing case proves the guard is about the counter
    /// rather than about the evidence being present.
    ///
    /// Required mutation RED: read the evidence without comparing the counters,
    /// and the stale value is returned as this statement's own.
    #[test]
    fn analytical_baseline_rejects_predecessor_evidence_without_new_settlement() {
        let predecessor = vala_bifrost_redux::oracle::analytical::AnalyticalPhysicalEvidence {
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
            matches!(stale, ProcessClusterError::Child(ref detail)
                if detail.contains("analytical baseline")),
            "the refusal is the child error naming the analytical baseline settlement, got {stale:?}"
        );

        let settled = settled_analytical_evidence(4, 5, Some(predecessor.clone()))
            .expect("an advanced settlement counter admits the current evidence");
        assert_eq!(
            settled,
            Some(predecessor),
            "the admitted evidence is the value the settled graph folded"
        );
    }
}

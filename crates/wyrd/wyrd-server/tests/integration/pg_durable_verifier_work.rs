//! The durable publication backend of queued Verifier work, against the
//! in-process server, its File storage and inventory, and the repository
//! Postgres.
//!
//! A trusted hosted caller inside one leased execution publishes through
//! [`Publications`]; no public endpoint accepts arbitrary paths.

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;

use serde_json::{Value, json};
use skald_runtime::{MockProvider, Provider, ProviderRegistry};
use skald_spec::MediaFormat;
use skald_spec::ProviderName;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{TableRef, TableUid};
use vala_bifrost_redux::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_verify::InMemoryTraceSource;
use vala_verify::blocks::{
    ActionExecutor, AgentExecutor, BlockClock, BlockError, CallInput, CallValue, CheckExecutor,
    Checkpoints, DriftChecks, EngineError, EvidenceReader, VerificationEngine,
};
use wyrd_runtime::Permission;
use wyrd_runtime::principal::PrincipalId;
use wyrd_server::components::auth::Caller;
use wyrd_server::components::gateway::GatewayCallContext;
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::scribe_outbox::{ScribeOutbox, ScribeSink, ScribeWrite};
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::authority::SystemReadAuthority;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::blocks::{Execution, HostedBlocks};
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_server::verification::hosted::publications::{Publications, PublishCall};
use wyrd_server::verification::hosted::sandbox::Sandbox;
use wyrd_server::verification::hosted::tools::HostedTools;
use wyrd_server::verification::hosted::workspace::{Workspace, Workspaces};
use wyrd_server::verification::inputs::InputResolver;
use wyrd_server::verification::leases::{HeldLease, LeaseRenewal};
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::state::{
    Checkpoint, Decision, RunState, StateError, StateRow, revision_batch, stand_in_identity,
};
use wyrd_server::verification::telemetry::ExecutionMode;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::card::verifier::{Action, VerifierSpec};
use wyrd_spec::ids::{CardUid, ExecutionId, SlotName};
use wyrd_spec::verification::execution::{
    ExecutedOutcome, InputDeclaration, InvocationSettlement, Producer, ProducerInvocation,
    PublicationCollection, PublicationContent, PublicationFile, PublicationKind,
    PublicationManifest, Speculation, StepStatus, VerificationResult,
};
use wyrd_spec::verification::state::{EXECUTION_STATE_FORMAT, ExecutionState, InvocationRecord};
use wyrd_spec::verification::{PublicationName, TaskId};
use wyrd_spec::verification::{
    VerificationError, VerificationExecutionStatus, VerificationVerdict,
};
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, ExecutionLease, Settlement, TerminalStatus, VerifierExecutionQueue,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::verification::VerificationFixture;

use crate::pg_hosted_artifacts::{NoRegistry, answer, reviewer, saturable, user};
use crate::sandbox_worker::worker;

/// One leased execution of a fresh in-process server, written through the
/// Bifrost execution state its claim holds.
struct Leased {
    /// The server; dropping it stops the server.
    server: WyrdTestServer,
    /// The claimed execution's lease.
    lease: ExecutionLease,
    /// The lease renewal owner every writer's lease registers with, renewing
    /// a three-second lease every half second and hearing cancellations
    /// until the guard drops.
    renewal: (Arc<LeaseRenewal>, tokio_util::sync::DropGuard),
    /// Every writer and its held lease, renewed until the test ends, the
    /// first claim's first.
    held: std::sync::Mutex<Vec<(Arc<RunState>, HeldLease)>>,
    /// The tenant's SYSTEM principal, which reads the state back.
    system: PrincipalId,
    /// The first claim's state writer.
    run: Arc<RunState>,
    /// The claimed execution's Verifier.
    verifier: CardUid,
}

impl Leased {
    /// Start a server, submit one execution, and claim it and its state.
    ///
    /// # Panics
    /// Panics when the server, fixture, submission, or claim fails.
    async fn start() -> Self {
        let server = WyrdTestServer::start_in_process()
            .await
            .expect("test server starts");
        let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
            .await
            .expect("the tenant provisions");
        let (subject, _) = fixture.service("publisher").await.expect("service");
        let verifier = fixture
            .drift_verifier("publishing")
            .await
            .expect("verifier");
        fixture
            .submit(&verifier, &subject, BTreeMap::new())
            .await
            .expect("the execution is admitted");
        let mut conn = server
            .tenant_conn_for(server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        let claimed = VerifierExecutionQueue::default()
            .claim(
                &mut conn,
                chrono::Duration::minutes(5),
                chrono::Duration::minutes(5),
                1,
            )
            .await
            .expect("claim runs")
            .pop()
            .expect("the execution is claimable");
        let system = system_principal_id(&mut conn)
            .await
            .expect("SYSTEM reads")
            .map(PrincipalId::new)
            .expect("the tenant has SYSTEM");
        conn.commit().await.expect("the claim commits");
        let renewal = Arc::new(LeaseRenewal::new(
            server.state().postgres.wyrd().clone(),
            VerifierExecutionQueue::default(),
            std::time::Duration::from_secs(3),
        ));
        let renewing = CancellationToken::new();
        tokio::spawn({
            let (renewal, renewing) = (Arc::clone(&renewal), renewing.clone());
            async move { renewal.run(renewing).await }
        });
        let outbox = Arc::clone(server.state().bifrost.scribe_outbox());
        let (run, lease) = Self::writer(&server, outbox, &renewal, system, &claimed)
            .await
            .expect("the execution state claims");
        Self {
            server,
            lease: claimed.lease,
            renewal: (renewal, renewing.drop_guard()),
            held: std::sync::Mutex::new(vec![(Arc::clone(&run), lease)]),
            system,
            run,
            verifier: claimed.verifier_card_uid,
        }
    }

    /// A user of the execution's tenant granted `verifier:run` on exactly
    /// its Verifier, as its accepted authority is, plus `permissions`.
    fn publisher(&self, permissions: impl IntoIterator<Item = Permission>) -> Caller {
        user(
            self.server.data_tenant_id(),
            std::iter::once(Permission::verifier_run(self.verifier.clone())).chain(permissions),
        )
    }

    /// A query caller reading the execution's records as SYSTEM.
    ///
    /// # Panics
    /// Panics when the SYSTEM read authority does not resolve.
    async fn query(server: &WyrdTestServer, system: PrincipalId) -> ScheduledQueryCaller {
        let tables = ["execution_inputs", "execution_outputs", "execution_events"]
            .map(|name| TableRef::new(BifrostNamespace::Verification, name));
        let read = SystemReadAuthority::resolve(
            server.state(),
            server.data_tenant_id(),
            Some(system),
            &tables,
        )
        .await
        .expect("SYSTEM read authority resolves");
        ScheduledQueryCaller::new(
            server.state().clone(),
            read.into_context(),
            CancellationToken::new(),
        )
    }

    /// Recover `claimed`'s execution state for its lease, as a worker would,
    /// staging its revisions on `outbox`, with the lease it holds.
    ///
    /// # Errors
    /// Returns the [`StateError`] refusing the recovery read.
    ///
    /// # Panics
    /// Panics when the SYSTEM read authority does not resolve, or when the
    /// renewal's held-lease lock is poisoned.
    async fn writer(
        server: &WyrdTestServer,
        outbox: Arc<ScribeOutbox>,
        renewal: &Arc<LeaseRenewal>,
        system: PrincipalId,
        claimed: &ClaimedExecution,
    ) -> Result<(Arc<RunState>, HeldLease), StateError> {
        let tenant = server.data_tenant_id();
        let query = Self::query(server, system).await;
        let lease = renewal.hold(tenant, claimed.lease.token, &CancellationToken::new());
        let run = RunState::recover(
            server.state(),
            outbox,
            tenant,
            StateRow {
                execution_id: claimed.lease.execution_id,
                identity: stand_in_identity(&claimed.verifier_card_uid, &claimed.card_uid),
                verifier_uid: claimed.verifier_card_uid.clone(),
                card_uid: claimed.card_uid.clone(),
                owner_card_uid: None,
                run_id: None,
                deployment_environment: claimed.deployment_environment.clone(),
                attribution: Attribution::new(claimed.attribution.sub_principal_id, system),
            },
            tokio::time::Instant::now() + std::time::Duration::from_secs(300),
            lease.lost().clone(),
            &InputResolver::new(&query, None),
        )
        .await?;
        Ok((Arc::new(run), lease))
    }

    /// The execution's accepted state, read back from Bifrost.
    ///
    /// # Panics
    /// Panics when the state is unreadable.
    async fn accepted(&self) -> ExecutionState {
        self.revision().await.1
    }

    /// The execution's accepted revision and state, read back from Bifrost.
    ///
    /// # Panics
    /// Panics when the state is unreadable.
    async fn revision(&self) -> (i64, ExecutionState) {
        let query = Self::query(&self.server, self.system).await;
        RunState::accepted(
            self.server.state(),
            self.server.data_tenant_id(),
            self.lease.execution_id,
            &InputResolver::new(&query, None),
        )
        .await
        .expect("the state reads")
    }

    /// The publication `name` of `producer`, if accepted.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn read(&self, producer: &Producer, name: &str) -> Option<PublicationContent> {
        self.accepted()
            .await
            .publications
            .into_iter()
            .map(|accepted| accepted.publication)
            .find(|publication| {
                &publication.producer == producer && publication.name.as_str() == name
            })
            .map(|publication| publication.content)
    }

    /// Expire the lease on the database clock and claim the execution and
    /// its state again, returning the new writer; the old one is stale once
    /// the background lease owner's next renewal finds its lease taken.
    ///
    /// # Panics
    /// Panics when the update or either claim fails.
    async fn reclaim(&self) -> Arc<RunState> {
        self.try_reclaim()
            .await
            .expect("the execution state claims")
    }

    /// Expire the lease on the database clock and claim the execution again,
    /// returning its writer's claim of the state.
    ///
    /// # Errors
    /// Returns the [`StateError`] refusing the state claim.
    ///
    /// # Panics
    /// Panics when the update or the queue claim fails.
    async fn try_reclaim(&self) -> Result<Arc<RunState>, StateError> {
        self.reclaim_on(Arc::clone(self.server.state().bifrost.scribe_outbox()))
            .await
    }

    /// Expire the lease on the database clock and claim the execution again,
    /// returning its writer's claim of the state, which stages its revisions
    /// on `outbox`.
    ///
    /// # Errors
    /// Returns the [`StateError`] refusing the state claim.
    ///
    /// # Panics
    /// Panics when the update or the queue claim fails.
    async fn reclaim_on(&self, outbox: Arc<ScribeOutbox>) -> Result<Arc<RunState>, StateError> {
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        sqlx::query(
            "UPDATE wyrd.verifier_executions SET lease_expires_at = statement_timestamp() \
             WHERE execution_id = $1",
        )
        .bind(self.lease.execution_id.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("the lease expires");
        let claimed = VerifierExecutionQueue::default()
            .claim(
                &mut conn,
                chrono::Duration::minutes(5),
                chrono::Duration::minutes(5),
                1,
            )
            .await
            .expect("claim runs")
            .pop()
            .expect("the expired lease is reclaimable");
        conn.commit().await.expect("the reclaim commits");
        let (run, lease) =
            Self::writer(&self.server, outbox, &self.renewal.0, self.system, &claimed).await?;
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((Arc::clone(&run), lease));
        Ok(run)
    }

    /// The bytes of the File `content` retained for `caller`, read by digest
    /// once its background inventory fact has published.
    ///
    /// # Panics
    /// Panics when the File does not read within thirty seconds.
    async fn retained(&self, caller: &Caller, limits: HostedLimits, content: &[u8]) -> Vec<u8> {
        let file = InputDeclaration::File {
            sha256: wyrd_spec::auth::Sha256Hex::digest(content),
            size_bytes: content.len() as u64,
            format: MediaFormat::Markdown,
            filename: None,
        };
        let evidence = TenantEvidence::new(self.server.state().clone(), caller.clone(), limits);
        let read = async {
            loop {
                self.server
                    .flush_bifrost()
                    .await
                    .expect("the File inventory publishes");
                if let Ok((bytes, _)) = evidence.read(&file).await {
                    return bytes;
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(30), read)
            .await
            .expect("the retained File reads")
    }

    /// Wait until every writer's background persistence settles: an
    /// accepted revision holds its every change, or it stopped.
    ///
    /// # Panics
    /// Panics when a writer has not settled within thirty seconds.
    async fn settled(&self) {
        let writers: Vec<_> = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(run, _)| Arc::clone(run))
            .collect();
        for run in writers {
            tokio::time::timeout(std::time::Duration::from_secs(30), run.persisted())
                .await
                .expect("the writer's persistence settles")
                .ok();
        }
    }

    /// Wait until the background lease owner marks the first claim's lease
    /// lost.
    ///
    /// # Panics
    /// Panics when it is not lost within thirty seconds.
    async fn lost(&self) {
        let lost = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)[0]
            .1
            .lost()
            .clone();
        tokio::time::timeout(std::time::Duration::from_secs(30), lost.cancelled())
            .await
            .expect("the lease owner hears the loss");
    }

    /// Every accepted publication of the execution as (task, attempt, name),
    /// in acceptance order.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn entries(&self) -> Vec<(String, i32, String)> {
        self.accepted()
            .await
            .publications
            .into_iter()
            .map(|accepted| {
                let producer = accepted.publication.producer;
                (
                    producer.step_id.as_str().to_owned(),
                    i32::try_from(producer.attempt).expect("attempt fits"),
                    accepted.publication.name.as_str().to_owned(),
                )
            })
            .collect()
    }
}

/// A call of `producer` publishing under `name`.
///
/// # Panics
/// Panics when `name` is not a valid publication name.
fn call(id: &str, producer: &Producer, name: &str) -> PublishCall {
    PublishCall {
        id: id.to_owned(),
        producer: producer.clone(),
        name: PublicationName::new(name).expect("name"),
    }
}

/// The refusal rule of a structured tool error.
///
/// # Panics
/// Panics when `error` carries no structured refusal details.
fn rule(error: &skald_tool::ToolError) -> Value {
    error
        .safe_details()
        .unwrap_or_else(|| panic!("not a structured refusal: {error}"))["rule"]
        .clone()
}

/// A `/work` file, a bounded JSON object, and an explicit JSON null publish
/// under their producer attempt and read back exactly; rewriting scratch
/// after publishing leaves the retained bytes unchanged. An oversized file
/// or value is refused and leaves no entry.
///
/// # Panics
/// Panics when content, digests, or producer identity differ, a later
/// scratch write reaches the retained File, or a refusal leaves an entry.
#[tokio::test(flavor = "multi_thread")]
async fn file_and_json_publications_are_immutable() {
    let leased = Leased::start().await;
    let limits = HostedLimits {
        work_write_max_bytes: 1024,
        tool_result_max_bytes: 64,
        ..HostedLimits::default()
    };
    let caller = leased.publisher([]);
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    let report = "# Review\n\nThe answer holds.\n";
    std::fs::create_dir_all(workspace.work_dir().join("notes")).expect("directory");
    std::fs::write(workspace.work_dir().join("notes/report one.md"), report).expect("write");
    std::fs::write(workspace.work_dir().join("big.bin"), vec![b'x'; 1025]).expect("write");
    let publications = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    let producer = Producer {
        step_id: TaskId::new("review").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };

    let kinds = [
        publications
            .path(
                &call("c1", &producer, "report"),
                &workspace,
                "/work/notes/report one.md",
            )
            .await,
        publications.json(
            &call("c2", &producer, "assessment"),
            json!({"agrees": true}),
        ),
        publications.json(&call("c3", &producer, "nothing"), Value::Null),
    ]
    .map(|kind| kind.expect("the publication is accepted"));
    assert_eq!(
        kinds,
        [
            PublicationKind::File,
            PublicationKind::Json,
            PublicationKind::Json
        ]
    );
    std::fs::write(
        workspace.work_dir().join("notes/report one.md"),
        "rewritten",
    )
    .expect("write");

    let sha256 = wyrd_spec::auth::Sha256Hex::digest(report.as_bytes());
    leased.settled().await;
    assert_eq!(
        leased.read(&producer, "report").await,
        Some(PublicationContent::File {
            manifest: PublicationManifest {
                files: vec![PublicationFile {
                    path: "report one.md".to_owned(),
                    sha256,
                    size_bytes: report.len() as u64,
                }],
                directories: Vec::new(),
            },
        })
    );
    leased.settled().await;
    assert_eq!(
        leased.read(&producer, "assessment").await,
        Some(PublicationContent::Json {
            value: json!({"agrees": true})
        })
    );
    leased.settled().await;
    assert_eq!(
        leased.read(&producer, "nothing").await,
        Some(PublicationContent::Json { value: Value::Null }),
        "an explicit null is a value, not an absence"
    );
    assert_eq!(
        leased.retained(&caller, limits, report.as_bytes()).await,
        report.as_bytes(),
        "scratch writes never reach it"
    );

    let file = publications
        .path(&call("c4", &producer, "big"), &workspace, "/work/big.bin")
        .await
        .expect_err("an oversized file is refused");
    assert_eq!(rule(&file), "work_budget");
    let value = publications
        .json(&call("c5", &producer, "huge"), json!("x".repeat(64)))
        .expect_err("an oversized value is refused");
    assert_eq!(rule(&value), "publication_size");
    for refused in ["big", "huge"] {
        leased.settled().await;
        assert_eq!(leased.read(&producer, refused).await, None);
    }
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A server-produced publication file is stored by the server's own
/// validated put: it opens no external upload session, and once its
/// background inventory fact publishes, the retained File reads exactly.
///
/// # Panics
/// Panics when the publication is refused, an upload session row exists,
/// or the retained File does not read back.
#[tokio::test(flavor = "multi_thread")]
async fn server_produced_files_open_no_upload_session() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    let report = "server produced\n";
    put(&workspace, "report.md", report.as_bytes());
    let publications = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    let producer = Producer {
        step_id: TaskId::new("review").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };

    publications
        .path(
            &call("c1", &producer, "report"),
            &workspace,
            "/work/report.md",
        )
        .await
        .expect("the file publishes");

    let mut conn = leased
        .server
        .tenant_conn_for(leased.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.storage_multipart_uploads")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("upload sessions count");
    conn.commit().await.expect("read commits");
    assert_eq!(
        sessions, 0,
        "a server-produced file opens no upload session"
    );
    assert_eq!(
        leased.retained(&caller, limits, report.as_bytes()).await,
        report.as_bytes()
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Write `bytes` at `/work/<relative>`, creating its parents.
///
/// # Panics
/// Panics when the write fails.
fn put(workspace: &Workspace, relative: &str, bytes: &[u8]) {
    let host = workspace.work_dir().join(relative);
    std::fs::create_dir_all(host.parent().expect("a parent")).expect("parents");
    std::fs::write(host, bytes).expect("write");
}

/// How many entries `/work/<relative>` holds; a case-insensitive or
/// normalizing filesystem merges colliding names, so a case only applies
/// where every entry it wrote exists.
///
/// # Panics
/// Panics when the directory cannot be read.
fn entries(workspace: &Workspace, relative: &str) -> usize {
    std::fs::read_dir(workspace.work_dir().join(relative))
        .expect("directory reads")
        .count()
}

/// A nested directory with Unicode and space names and empty directories
/// publishes completely, each path and byte as written; a tree holding any
/// refused entry, or over a count, depth, or byte limit, publishes nothing.
/// A snapshot waiting on a writer freezes the tree that writer left.
///
/// # Panics
/// Panics when an accepted path or digest differs, a refused tree leaves an
/// entry, or a snapshot mixes states.
#[tokio::test(flavor = "multi_thread")]
async fn directory_snapshot_is_complete_or_rejected() {
    let leased = Leased::start().await;
    let limits = HostedLimits {
        work_write_max_bytes: 1024,
        archive_max_entries: 8,
        ..HostedLimits::default()
    };
    let caller = leased.publisher([]);
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    let publications = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    let producer = Producer {
        step_id: TaskId::new("review").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    let written = [
        ("report/summary.md", "# Summary\n"),
        ("report/naïve dir/notes 1.txt", "größe\n"),
        ("report/deep/a/b/c.txt", "c\n"),
    ];
    for (relative, text) in written {
        put(&workspace, relative, text.as_bytes());
    }
    std::fs::create_dir_all(workspace.work_dir().join("report/empty")).expect("empty");
    assert_eq!(
        publications
            .path(
                &call("tree", &producer, "report"),
                &workspace,
                "/work/report"
            )
            .await
            .expect("the tree publishes"),
        PublicationKind::Directory
    );
    put(&workspace, "report/summary.md", b"rewritten");
    let file = |path: &str, text: &str| PublicationFile {
        path: path.to_owned(),
        sha256: wyrd_spec::auth::Sha256Hex::digest(text.as_bytes()),
        size_bytes: text.len() as u64,
    };
    leased.settled().await;
    assert_eq!(
        leased.read(&producer, "report").await,
        Some(PublicationContent::Directory {
            manifest: PublicationManifest {
                files: vec![
                    file("deep/a/b/c.txt", "c\n"),
                    file("naïve dir/notes 1.txt", "größe\n"),
                    file("summary.md", "# Summary\n"),
                ],
                directories: ["deep", "deep/a", "deep/a/b", "empty", "naïve dir"]
                    .map(str::to_owned)
                    .to_vec(),
            },
        })
    );

    put(&workspace, "backslash/a\\b", b"x");
    put(&workspace, "control/a\u{7}b", b"x");
    put(&workspace, "symlink/target", b"x");
    std::os::unix::fs::symlink(
        workspace.work_dir().join("symlink/target"),
        workspace.work_dir().join("symlink/link"),
    )
    .expect("symlink");
    std::fs::create_dir_all(workspace.work_dir().join("special")).expect("special");
    let _socket = std::os::unix::net::UnixListener::bind(workspace.work_dir().join("special/s"))
        .expect("socket");
    put(&workspace, "case/Readme.md", b"x");
    put(&workspace, "case/README.md", b"y");
    put(&workspace, "nfc/caf\u{e9}", b"x");
    put(&workspace, "nfc/cafe\u{301}", b"y");
    put(&workspace, "kinds/a", b"x");
    std::fs::create_dir_all(workspace.work_dir().join("kinds/A")).expect("directory");
    for index in 0..9 {
        put(&workspace, &format!("count/{index}"), b"x");
    }
    put(&workspace, "bytes/one", &[b'x'; 600]);
    put(&workspace, "bytes/two", &[b'x'; 600]);
    let outside = "is not a file or directory this invocation wrote";
    let mut refused = vec![
        ("backslash", "/work/backslash", "backslash or control"),
        ("control", "/work/control", "backslash or control"),
        ("symlink", "/work/symlink", "link or special"),
        ("special", "/work/special", "link or special"),
        ("count", "/work/count", "more than 8 entries"),
        ("bytes", "/work/bytes", "-byte /work limit"),
        ("traversal", "/work/../work/report", outside),
        ("absolute", "/etc", outside),
        ("missing", "/work/absent", "does not exist"),
    ];
    for case in ["case", "nfc", "kinds"] {
        if entries(&workspace, case) == 2 {
            refused.push((case, "", "collides"));
        }
    }
    {
        use std::os::unix::ffi::OsStrExt as _;
        let name = std::ffi::OsStr::from_bytes(b"bad\xff");
        std::fs::create_dir_all(workspace.work_dir().join("utf8")).expect("utf8");
        if std::fs::write(workspace.work_dir().join("utf8").join(name), b"x").is_ok() {
            refused.push(("utf8", "/work/utf8", "is not UTF-8"));
        }
    }
    {
        use std::os::unix::fs::PermissionsExt as _;
        put(&workspace, "unreadable/locked", b"x");
        let locked = workspace.work_dir().join("unreadable/locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        if std::fs::read(&locked).is_err() {
            refused.push(("unreadable", "/work/unreadable", "workspace IO failed"));
        }
    }
    let deep =
        Arc::new(Workspace::materialize(Vec::new(), HostedLimits::default(), None).expect("deep"));
    put(&deep, &format!("depth/{}x", "d/".repeat(33)), b"x");
    let mut cases: Vec<_> = refused
        .into_iter()
        .map(|(case, path, reason)| {
            let path = if path.is_empty() {
                format!("/work/{case}")
            } else {
                path.to_owned()
            };
            (case, path, reason, &workspace)
        })
        .collect();
    cases.push((
        "depth",
        "/work/depth".to_owned(),
        "deeper than 32 levels",
        &deep,
    ));
    for (case, path, reason, workspace) in cases {
        let error = publications
            .path(&call(case, &producer, case), workspace, &path)
            .await
            .expect_err(case);
        let detail = error.detail().unwrap_or_default().to_owned();
        assert!(detail.contains(reason), "{case}: {error}");
        leased.settled().await;
        assert_eq!(leased.read(&producer, case).await, None, "{case}");
    }

    put(&workspace, "held/one.txt", b"old");
    let writer = workspace.writer().await;
    let held = call("held", &producer, "held");
    let publish = publications.path(&held, &workspace, "/work/held");
    let edit = async {
        tokio::task::yield_now().await;
        put(&workspace, "held/one.txt", b"1");
        put(&workspace, "held/two.txt", b"2");
        drop(writer);
    };
    let (published, ()) = tokio::join!(publish, edit);
    assert_eq!(
        published.expect("the held tree publishes"),
        PublicationKind::Directory
    );
    leased.settled().await;
    assert_eq!(
        leased.read(&producer, "held").await,
        Some(PublicationContent::Directory {
            manifest: PublicationManifest {
                files: vec![file("one.txt", "1"), file("two.txt", "2")],
                directories: Vec::new(),
            },
        }),
        "the snapshot waits for the writer and freezes the tree it left"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A replay of the same call and arguments recovers its one receipt, even
/// after its scratch file is removed or replaced by a directory and while
/// storage is unavailable, answering the stored kind; the same name from
/// another call, or from that call with other arguments, conflicts; a fresh
/// attempt publishes the name again with its own attribution. A live replay
/// adds no revision. An upload that fails leaves nothing readable. Once the
/// lease is reclaimed, an identical JSON replay is refused as the stale
/// writer's first operation, and a file replay and a new publication after it.
///
/// # Panics
/// Panics when a replay adds an entry or a revision, a conflict is accepted,
/// attempts merge, a stale replay succeeds, or a failed or stale publication
/// becomes readable.
#[tokio::test(flavor = "multi_thread")]
async fn publication_replay_preserves_attempt_identity() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    let publications = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    let first = Producer {
        step_id: TaskId::new("review").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    let second = Producer {
        attempt: 2,
        ..first.clone()
    };
    put(&workspace, "notes.md", b"notes");

    let mut published = None;
    for _ in 0..2 {
        assert_eq!(
            publications
                .json(&call("c1", &first, "report"), json!({"v": 1}))
                .expect("the call and its replay answer one receipt"),
            PublicationKind::Json
        );
        publications
            .path(&call("f1", &first, "notes"), &workspace, "/work/notes.md")
            .await
            .expect("the file call and its replay answer one receipt");
        leased.settled().await;
        let revision = leased.revision().await.0;
        assert_eq!(
            *published.get_or_insert(revision),
            revision,
            "a replay adds no revision"
        );
    }
    std::fs::remove_file(workspace.work_dir().join("notes.md")).expect("scratch removed");
    let wyrd_storage::BackendConfig::Local { root } =
        leased.server.state().storage.backend_config()
    else {
        panic!("the test server stores locally");
    };
    let evidence = root
        .join(leased.server.data_tenant_id().to_string())
        .join("evidence/sha256");
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&evidence, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let gone = publications
            .path(&call("f1", &first, "notes"), &workspace, "/work/notes.md")
            .await;
        put(&workspace, "notes.md/now-a-directory.md", b"other");
        let replaced = publications
            .path(&call("f1", &first, "notes"), &workspace, "/work/notes.md")
            .await;
        std::fs::set_permissions(&evidence, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            (
                gone.expect("a replay needs neither scratch nor storage"),
                replaced.expect("a replay ignores what scratch holds now")
            ),
            (PublicationKind::File, PublicationKind::File),
            "a replay answers the stored kind"
        );
    }
    for (id, value) in [("c1", json!({"v": 2})), ("c2", json!({"v": 1}))] {
        let error = publications
            .json(&call(id, &first, "report"), value)
            .expect_err("another call or argument set conflicts");
        assert_eq!(rule(&error), "publication_name");
    }
    publications
        .json(&call("c3", &second, "report"), json!({"v": 3}))
        .expect("a fresh attempt publishes the name again");
    leased.settled().await;
    assert_eq!(
        (
            leased.read(&first, "report").await,
            leased.read(&second, "report").await
        ),
        (
            Some(PublicationContent::Json {
                value: json!({"v": 1})
            }),
            Some(PublicationContent::Json {
                value: json!({"v": 3})
            })
        ),
        "each attempt keeps its own attribution"
    );

    {
        use std::os::unix::fs::PermissionsExt as _;
        put(&workspace, "tree/kept.md", b"kept");
        put(&workspace, "tree/new.md", b"never stored");
        std::fs::set_permissions(&evidence, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let failed = publications
            .path(&call("t1", &first, "tree"), &workspace, "/work/tree")
            .await;
        std::fs::set_permissions(&evidence, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        failed.expect_err("an upload that fails publishes nothing");
    }
    leased.settled().await;

    let current = leased.reclaim().await;
    leased.lost().await;
    let stale = publications
        .json(&call("c1", &first, "report"), json!({"v": 1}))
        .expect_err("a JSON replay as the stale writer's first operation is refused");
    assert!(
        stale
            .to_string()
            .contains("no longer holds its state, lease, or deadline"),
        "{stale}"
    );
    publications
        .path(&call("f1", &first, "notes"), &workspace, "/work/notes.md")
        .await
        .expect_err("a file replay under the stale lease is refused");
    publications
        .json(&call("c4", &second, "late"), json!(null))
        .expect_err("a reclaimed lease publishes nothing");
    Publications::new(leased.server.state(), &caller, &current, limits)
        .json(&call("c5", &second, "late"), json!(null))
        .expect("the current lease publishes");
    leased.settled().await;
    assert_eq!(
        leased.entries().await,
        [
            ("review".to_owned(), 1, "report".to_owned()),
            ("review".to_owned(), 1, "notes".to_owned()),
            ("review".to_owned(), 2, "report".to_owned()),
            ("review".to_owned(), 2, "late".to_owned()),
        ],
        "one entry per accepted call, none for the failed upload or stale lease"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Publication limits bound what one execution retains: a file at the file
/// limit is accepted and mounts, one byte more is refused before upload, and
/// the cumulative count and byte ceilings hold across replays, which are
/// charged once, and across a reclaim, which inherits the spent budget.
/// Accepted files mount from their completed File descriptors while the
/// object inventory cannot be read at all.
///
/// # Panics
/// Panics when a limit admits or refuses the wrong publication, a replay or
/// reclaim changes the charge, or a mount needs the inventory.
#[tokio::test(flavor = "multi_thread")]
async fn publication_limits_survive_reclaim() {
    let leased = Leased::start().await;
    let limits = HostedLimits {
        file_max_bytes: 8,
        files_max_bytes: 16,
        publications_max: 3,
        work_write_max_bytes: 64,
        ..HostedLimits::default()
    };
    let caller = leased.publisher([]);
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    let publications = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    let producer = Producer {
        step_id: TaskId::new("review").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    put(&workspace, "edge.bin", b"12345678");
    put(&workspace, "over.bin", b"123456789");
    put(&workspace, "tail.bin", b"abcdefgh");

    assert_eq!(
        publications
            .path(
                &call("edge", &producer, "edge"),
                &workspace,
                "/work/edge.bin"
            )
            .await
            .expect("a file at the file limit is accepted"),
        PublicationKind::File
    );
    let over = publications
        .path(
            &call("over", &producer, "over"),
            &workspace,
            "/work/over.bin",
        )
        .await
        .expect_err("a file over the file limit is refused");
    assert_eq!(rule(&over), "publication_size");
    publications
        .json(&call("j", &producer, "j"), json!("ab"))
        .expect("a 4-byte value fits the 16-byte total");
    let tail = publications
        .path(
            &call("tail", &producer, "tail"),
            &workspace,
            "/work/tail.bin",
        )
        .await
        .expect_err("12 retained bytes plus 8 exceed 16");
    assert_eq!(rule(&tail), "publication_bytes");
    publications
        .path(
            &call("edge", &producer, "edge"),
            &workspace,
            "/work/edge.bin",
        )
        .await
        .expect("a replay is not charged again");
    leased.settled().await;

    let current = leased.reclaim().await;
    let resumed = Publications::new(leased.server.state(), &caller, &current, limits);
    resumed
        .json(&call("k", &producer, "k"), json!(1))
        .expect("the third publication fits");
    let count = resumed
        .json(&call("l", &producer, "l"), json!(1))
        .expect_err("a reclaim inherits the accepted count");
    assert_eq!(rule(&count), "publication_count");
    for refused in ["over", "tail", "l"] {
        leased.settled().await;
        assert_eq!(leased.read(&producer, refused).await, None, "{refused}");
    }

    let blind = Caller {
        // Not a SYSTEM principal, so every inventory query is unavailable.
        executed_by: Some(wyrd_runtime::principal::PrincipalId::new(
            uuid::Uuid::new_v4(),
        )),
        ..caller
    };
    let evidence = TenantEvidence::new(leased.server.state().clone(), blind, limits)
        .with_execution(Arc::clone(&leased.run));
    evidence
        .read(&InputDeclaration::File {
            sha256: wyrd_spec::auth::Sha256Hex::digest(b"12345678"),
            size_bytes: 8,
            format: MediaFormat::Markdown,
            filename: None,
        })
        .await
        .expect_err("the object inventory cannot be read by this caller");
    let mounted = Workspaces::new(limits, limits.queued.workspaces)
        .open(
            &BTreeMap::from([(
                "/inputs/edge".to_owned(),
                CallInput {
                    value: CallValue::Publication {
                        producer: producer.clone(),
                        name: PublicationName::new("edge").expect("name"),
                    },
                    sources: BTreeSet::new(),
                },
            )]),
            &evidence,
        )
        .await
        .expect("an accepted file mounts from its completed descriptor");
    assert_eq!(
        std::fs::read(mounted.inputs_dir().join("edge")).expect("mounted"),
        b"12345678"
    );
    drop(mounted);
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Workspaces mount retained evidence through the hosted evidence owner: a
/// named member only from its successful attempt, at its destination, and a
/// collection as an honest manifest of every producer status and attempt
/// with its admitted publications, partial work of failed attempts included,
/// under `content/`. Mounted files cite only their producer's original
/// slots, and a missing name refuses the consumer before it runs.
///
/// # Panics
/// Panics when a mounted path, byte, source, manifest field, or refusal
/// differs.
#[tokio::test(flavor = "multi_thread")]
async fn publication_mounts_preserve_status_and_lineage() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let publications = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    let producer = |step: &str, attempt| Producer {
        step_id: TaskId::new(step).expect("task"),
        item_index: None,
        iteration_index: None,
        attempt,
    };
    let (first, second, broken) = (
        producer("review", 1),
        producer("review", 2),
        producer("broken", 1),
    );
    let scratch = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    put(&scratch, "draft.md", b"early draft");
    put(&scratch, "report/summary.md", b"# Summary\nagrees\n");
    put(&scratch, "report/manifest.json", b"{\"forged\": true}");
    std::fs::create_dir_all(scratch.work_dir().join("report/empty")).expect("mkdir");
    put(&scratch, "partial.md", b"half a review");
    publications
        .path(&call("d", &first, "draft"), &scratch, "/work/draft.md")
        .await
        .expect("the failed attempt publishes partial work");
    publications
        .path(&call("r", &second, "report"), &scratch, "/work/report")
        .await
        .expect("the successful attempt publishes its report");
    publications
        .json(&call("a", &second, "assessment"), json!({"agrees": true}))
        .expect("the successful attempt publishes JSON");
    publications
        .path(&call("p", &broken, "partial"), &scratch, "/work/partial.md")
        .await
        .expect("the failed step publishes partial work");
    leased
        .server
        .flush_bifrost()
        .await
        .expect("the File inventory publishes");

    let evidence = TenantEvidence::new(leased.server.state().clone(), caller.clone(), limits)
        .with_execution(Arc::clone(&leased.run));
    let answer = BTreeSet::from([SlotName::new("answer").expect("slot")]);
    let named = |producer: &Producer, name: &str| CallInput {
        value: CallValue::Publication {
            producer: producer.clone(),
            name: PublicationName::new(name).expect("name"),
        },
        sources: answer.clone(),
    };
    let invocation = |attempt, status, reason: Option<&str>| ProducerInvocation {
        item_index: None,
        iteration_index: None,
        attempt,
        status,
        reason: reason.map(str::to_owned),
    };
    let collection = |step: &str, status, reason: Option<&str>, invocations, sources| CallInput {
        value: CallValue::Publications {
            step: TaskId::new(step).expect("task"),
            collection: PublicationCollection {
                status,
                reason: reason.map(str::to_owned),
                speculation: Speculation::None,
                invocations,
                entries: Vec::new(),
            },
        },
        sources,
    };
    let files = BTreeMap::from([
        ("/inputs/report".to_owned(), named(&second, "report")),
        (
            "/inputs/assessment.json".to_owned(),
            named(&second, "assessment"),
        ),
        (
            "/inputs/reviews".to_owned(),
            collection(
                "review",
                StepStatus::Succeeded,
                None,
                vec![
                    invocation(
                        1,
                        StepStatus::Failed,
                        Some("the attempt failed and was retried"),
                    ),
                    invocation(2, StepStatus::Succeeded, None),
                ],
                answer.clone(),
            ),
        ),
        (
            "/inputs/broken".to_owned(),
            collection(
                "broken",
                StepStatus::Failed,
                Some("broken"),
                vec![invocation(1, StepStatus::Failed, Some("broken"))],
                BTreeSet::new(),
            ),
        ),
        (
            "/inputs/gated".to_owned(),
            collection(
                "gated",
                StepStatus::Skipped,
                Some("when `inputs.answer.go` was false"),
                Vec::new(),
                BTreeSet::new(),
            ),
        ),
    ]);
    let workspaces = Workspaces::new(limits, limits.queued.workspaces);
    let mounted = workspaces
        .open(&files, &evidence)
        .await
        .expect("the publications mount");
    let inputs = mounted.inputs_dir();
    let text = |relative: &str| {
        std::fs::read_to_string(inputs.join(relative))
            .unwrap_or_else(|error| panic!("{relative}: {error}"))
    };
    assert_eq!(text("report/summary.md"), "# Summary\nagrees\n");
    assert_eq!(text("report/manifest.json"), "{\"forged\": true}");
    assert!(
        inputs.join("report/empty").is_dir(),
        "empty directories mount"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&text("assessment.json")).expect("JSON"),
        json!({"agrees": true})
    );
    let manifest = |relative: &str| {
        serde_json::from_str::<Value>(&text(relative)).expect("the manifest is JSON")
    };
    assert_eq!(
        manifest("reviews/manifest.json"),
        json!({
            "status": "succeeded", "reason": null, "speculation": "none",
            "invocations": [
                {"item_index": null, "iteration_index": null, "attempt": 1, "status": "failed", "reason": "the attempt failed and was retried"},
                {"item_index": null, "iteration_index": null, "attempt": 2, "status": "succeeded", "reason": null},
            ],
            "entries": [
                {"name": "draft", "kind": "file", "path": "content/attempt-1/draft", "item_index": null, "iteration_index": null, "attempt": 1},
                {"name": "report", "kind": "directory", "path": "content/attempt-2/report", "item_index": null, "iteration_index": null, "attempt": 2},
                {"name": "assessment", "kind": "json", "path": "content/attempt-2/assessment", "item_index": null, "iteration_index": null, "attempt": 2},
            ],
        })
    );
    assert_eq!(text("reviews/content/attempt-1/draft"), "early draft");
    assert_eq!(
        text("reviews/content/attempt-2/report/manifest.json"),
        "{\"forged\": true}",
        "a published manifest.json stays beneath content/"
    );
    assert!(
        inputs
            .join("reviews/content/attempt-2/report/empty")
            .is_dir()
    );
    assert_eq!(
        manifest("broken/manifest.json")["entries"],
        json!([{"name": "partial", "kind": "file", "path": "content/attempt-1/partial", "item_index": null, "iteration_index": null, "attempt": 1}])
    );
    assert_eq!(manifest("broken/manifest.json")["status"], "failed");
    assert_eq!(text("broken/content/attempt-1/partial"), "half a review");
    assert_eq!(
        manifest("gated/manifest.json"),
        json!({"status": "skipped", "reason": "when `inputs.answer.go` was false", "speculation": "none", "invocations": [], "entries": []})
    );

    let listed = mounted.list("/inputs").expect("the inputs list");
    let sources: BTreeMap<&str, &Value> = listed["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| (file["path"].as_str().expect("path"), &file["sources"]))
        .collect();
    assert_eq!(sources["/inputs/report/summary.md"], &json!(["answer"]));
    assert_eq!(
        sources["/inputs/reviews/content/attempt-1/draft"],
        &json!(["answer"])
    );
    assert_eq!(
        sources["/inputs/reviews/manifest.json"],
        &json!([]),
        "the host manifest cites nothing"
    );
    assert_eq!(
        sources["/inputs/broken/content/attempt-1/partial"],
        &json!([]),
        "partial work of a failed step cites no original slot"
    );
    drop(mounted);

    for (producer, name) in [(&first, "report"), (&second, "draft"), (&second, "missing")] {
        let error = workspaces
            .open(
                &BTreeMap::from([("/inputs/x".to_owned(), named(producer, name))]),
                &evidence,
            )
            .await
            .err()
            .expect("a missing named publication refuses the consumer");
        assert!(
            error
                .to_string()
                .contains(&format!("published no `{name}`")),
            "{error}"
        );
    }
    let direct = TenantEvidence::new(leased.server.state().clone(), caller, limits);
    let unbound = workspaces
        .open(
            &BTreeMap::from([("/inputs/x".to_owned(), named(&second, "report"))]),
            &direct,
        )
        .await;
    assert!(unbound.is_err(), "direct verify accepts no publications");
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Cancelling leased work after a checkpoint and a publication fences the
/// worker: the background lease owner hears the cancellation and marks the
/// lease lost, so its next invocation, publication, settlement, and renewal
/// are all stale, the accepted checkpoint and publication survive
/// unchanged, and the execution is never claimed again. A repeated cancel
/// changes nothing.
///
/// # Panics
/// Panics when the loss is not heard, a stale write lands, a checkpoint or
/// publication changes, or a cancelled execution is claimed.
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_work_keeps_checkpoints_and_fences_stale_writes() {
    let leased = Leased::start().await;
    let state = leased.server.state();
    let tenant = leased.server.data_tenant_id();
    let queue = VerifierExecutionQueue::default();
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let publications = Publications::new(state, &caller, &leased.run, limits);
    let task = TaskId::new("review").expect("task");
    let producer = Producer {
        step_id: task.clone(),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    let execution = leased.lease.execution_id;
    let conn = async || {
        leased
            .server
            .tenant_conn_for(tenant)
            .await
            .expect("tenant connection opens")
    };

    leased
        .run
        .begin(&Producer::plain(&task, 1))
        .await
        .expect("the checkpoint is accepted");
    publications
        .json(&call("c1", &producer, "report"), json!({"v": 1}))
        .expect("the live lease publishes");
    leased.settled().await;

    let mut tx = conn().await;
    assert!(queue.cancel(&mut tx, execution).await.expect("cancel runs"));
    tx.commit().await.expect("the cancel commits");
    leased.lost().await;
    leased
        .run
        .begin(&Producer::plain(&task, 2))
        .await
        .expect_err("a cancelled lease checkpoints nothing");
    let mut tx = conn().await;
    assert!(
        !queue.cancel(&mut tx, execution).await.expect("cancel runs"),
        "a repeated cancel writes nothing"
    );
    let stale = (
        queue
            .terminate(
                &mut tx,
                leased.lease,
                TerminalStatus::Errored,
                &VerificationError {
                    code: "late".to_owned(),
                    message: "a late settlement".to_owned(),
                },
            )
            .await
            .expect("terminate runs"),
        queue
            .renew(&mut tx, &[leased.lease.token], chrono::Duration::minutes(5))
            .await
            .expect("renew runs"),
        queue
            .claim(
                &mut tx,
                chrono::Duration::minutes(5),
                chrono::Duration::minutes(5),
                1,
            )
            .await
            .expect("claim runs")
            .len(),
    );
    assert_eq!(stale, (Settlement::StaleLease, Vec::new(), 0));
    let status: (String, Option<String>) = sqlx::query_as(
        "SELECT status, error->>'code' FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&mut **tx.transaction())
    .await
    .expect("status reads");
    tx.commit().await.expect("the reads commit");
    publications
        .json(&call("c2", &producer, "late"), json!(null))
        .expect_err("a cancelled lease publishes nothing");
    let accepted = leased.accepted().await;
    assert_eq!(
        (
            accepted
                .invocations
                .iter()
                .map(|invocation| invocation.producer.attempt)
                .collect::<Vec<_>>(),
            status
        ),
        (
            vec![1],
            ("cancelled".to_owned(), Some("cancelled".to_owned()))
        )
    );
    assert_eq!(
        leased.entries().await,
        [("review".to_owned(), 1, "report".to_owned())],
        "the accepted publication survives and the late one is refused"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A hosted Agent engine over `run`'s execution state: every model call is
/// answered by `model`, and tools run as `caller` on `sandbox`.
///
/// # Panics
/// Never; the concurrency is nonzero.
fn agent_engine(
    leased: &Leased,
    run: &Arc<RunState>,
    caller: &Caller,
    model: &Arc<dyn Provider>,
    sandbox: &Arc<Sandbox>,
    recorded: Vec<InvocationRecord>,
) -> VerificationEngine {
    let limits = saturable();
    let evidence = Arc::new(
        TenantEvidence::new(leased.server.state().clone(), caller.clone(), limits)
            .with_execution(Arc::clone(run)),
    );
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::clone(model));
    let agent = Arc::new(
        AgentExecutor::new(
            Arc::new(providers),
            Arc::new(NoRegistry),
            Arc::new(NoRegistry),
        )
        .with_toolset(Arc::new(
            HostedTools::new(
                leased.server.state().clone(),
                caller.clone(),
                Arc::clone(&evidence),
                Arc::new(Workspaces::new(limits, limits.queued.workspaces)),
                Arc::clone(sandbox),
                limits,
            )
            .with_execution(Arc::clone(run)),
        ))
        .with_evidence(evidence as Arc<dyn EvidenceReader>),
    );
    VerificationEngine::new(
        Arc::clone(&agent) as Arc<dyn ActionExecutor>,
        agent as Arc<dyn CheckExecutor>,
        Arc::new(InMemoryTraceSource::new()),
        std::num::NonZeroUsize::MIN,
    )
    .with_checkpoints(
        Arc::clone(run) as Arc<dyn Checkpoints>,
        &ExecutionState {
            invocations: recorded,
            ..ExecutionState::default()
        },
    )
}

/// A Verifier of the Agent `steps`, each `(id, retries, tools, workspace)`
/// reading JSON input `x` as its `document`, in order.
///
/// # Panics
/// Panics when the Verifier does not decode or validate.
fn agent_steps(steps: &[(&str, u32, &[&str], Value)]) -> Arc<VerifierSpec> {
    let steps: Vec<Value> = steps
        .iter()
        .map(|(id, retries, tools, workspace)| {
            let mut action = serde_json::to_value(Action::Agent(reviewer(tools))).expect("action");
            action["inputs"] = json!({ "document": "inputs.x" });
            action["workspace"] = workspace.clone();
            json!({ "id": id, "retries": retries, "action": action })
        })
        .collect();
    let spec: VerifierSpec = serde_json::from_value(json!({
        "inputs": { "x": { "type": "json" } },
        "steps": steps
    }))
    .expect("Verifier decodes");
    spec.validate().expect("valid Verifier");
    Arc::new(spec)
}

/// A Verifier of one Agent step `id` reading JSON input `x`, declaring
/// `publish` and `code.run`, with `retries` retries.
///
/// # Panics
/// Panics when the Verifier does not decode or validate.
fn publishing_step(id: &str, retries: u32) -> Arc<VerifierSpec> {
    agent_steps(&[(id, retries, &["publish", "code.run"], json!({}))])
}

/// The JSON evidence every Agent step here reads as input `x`.
///
/// # Panics
/// Never; the slot name is valid.
fn notes() -> BTreeMap<SlotName, InputDeclaration> {
    BTreeMap::from([(
        SlotName::new("x").expect("slot"),
        InputDeclaration::Json {
            value: json!("the notes"),
        },
    )])
}

/// An attempt refused Python capacity after it retained a publication is
/// spent, not forgotten. With the only sandbox admission held, `draft`
/// publishes `report` and then is refused at `code.run`; `once`, with no
/// retry, does the same; `early` is refused at `code.run` before retaining
/// anything. Each run defers busy. After a reclaim, `draft` runs attempt 2,
/// which republishes `report` beside attempt 1's still-readable value;
/// `once` is not restarted and makes no model call; `early`, whose refusal
/// retained nothing, runs as attempt 1 once capacity frees.
///
/// # Panics
/// Panics when a refused run is not busy, a retained attempt is forgotten
/// or restarted, a forgotten one is spent, or a publication changes.
#[tokio::test(flavor = "multi_thread")]
async fn retained_publications_keep_their_busy_attempt() {
    let leased = Leased::start().await;
    let limits = saturable();
    let caller = leased.publisher([Permission::tool_invoke_any()]);
    let sandbox = Arc::new(
        Sandbox::start(worker(), limits, limits.queued)
            .await
            .expect("pool starts"),
    );
    let none = serde_json::Map::new();
    let far = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    // One admitted operation holds the only worker admission while pending.
    let mut held = Box::pin(sandbox.run("def f():\n    return 1", "f", &none, Vec::new(), far));
    assert!(futures_util::FutureExt::now_or_never(&mut held).is_none());
    let model = MockProvider::new(ProviderName::OpenAi);
    let provider: Arc<dyn Provider> = Arc::new(model.clone());
    let evidence = BTreeMap::from([(
        SlotName::new("x").expect("slot"),
        InputDeclaration::Json {
            value: json!("the notes"),
        },
    )]);
    let run = async |state: &Arc<RunState>, spec: &Arc<VerifierSpec>| {
        let recorded = state.snapshot().invocations;
        agent_engine(&leased, state, &caller, &provider, &sandbox, recorded)
            .run(Arc::clone(spec), evidence.clone(), far)
            .await
    };
    let publish = |name: &str, value: &str| {
        answer(
            None,
            &[("publish", json!({ "name": name, "value": value }))],
        )
    };
    let code = || answer(None, &[("code_run", json!({ "source": "1" }))]);
    let (draft, once, early) = (
        publishing_step("draft", 1),
        publishing_step("once", 0),
        publishing_step("early", 0),
    );

    for (spec, published) in [
        (&draft, Some("report")),
        (&once, Some("once")),
        (&early, None),
    ] {
        if let Some(name) = published {
            model.push_response(publish(name, "attempt 1"));
        }
        model.push_response(code());
        let refused = run(&leased.run, spec).await;
        assert!(matches!(refused, Err(EngineError::Busy(_))), "{refused:?}");
    }
    assert_eq!(model.remaining(), 0, "every refusal stopped its loop");
    leased.settled().await;
    let accepted = leased.accepted().await;
    assert_eq!(
        attempts(&accepted),
        [("draft".to_owned(), 1), ("once".to_owned(), 1)],
        "an attempt with retained work stays spent; one without is forgotten"
    );
    leased.settled().await;

    let current = leased.reclaim().await;
    model.push_response(publish("report", "attempt 2"));
    model.push_response(answer(Some("{}"), &[]));
    let outcome = run(&current, &draft).await.expect("the retry runs");
    let step = |outcome: &ExecutedOutcome, id: &str| {
        outcome
            .steps
            .iter()
            .find(|step| step.id.as_str() == id)
            .map(|step| (step.status, step.attempts))
    };
    assert_eq!(
        step(&outcome, "draft"),
        Some((StepStatus::Succeeded, 2)),
        "the fresh work runs the next attempt: {outcome:?}"
    );
    let producer = |step: &str, attempt: u32| Producer {
        step_id: TaskId::new(step).expect("task"),
        item_index: None,
        iteration_index: None,
        attempt,
    };
    for (attempt, value) in [(1, "attempt 1"), (2, "attempt 2")] {
        leased.settled().await;
        assert_eq!(
            leased.read(&producer("draft", attempt), "report").await,
            Some(PublicationContent::Json {
                value: json!(value)
            }),
            "attempt {attempt} keeps its own report"
        );
    }

    model.push_response(code());
    model.push_response(answer(Some("{}"), &[]));
    let outcome = run(&current, &once)
        .await
        .expect("the exhausted step settles");
    let (status, spent) = step(&outcome, "once").expect("once settles");
    assert_eq!(
        (status, spent, model.remaining()),
        (StepStatus::Failed, 1, 2),
        "a step with no retry left is not restarted and calls no model"
    );
    leased.settled().await;
    assert_eq!(
        leased.read(&producer("once", 1), "once").await,
        Some(PublicationContent::Json {
            value: json!("attempt 1")
        })
    );

    drop(held);
    let outcome = run(&current, &early).await.expect("the deferred step runs");
    assert_eq!(
        step(&outcome, "early"),
        Some((StepStatus::Succeeded, 1)),
        "a refusal before retained work spends no attempt: {outcome:?}"
    );
    assert_eq!(model.remaining(), 0);
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// The (step, attempt) of every invocation `state` records, in commit order.
fn attempts(state: &ExecutionState) -> Vec<(String, u32)> {
    state
        .invocations
        .iter()
        .map(|invocation| {
            (
                invocation.producer.step_id.as_str().to_owned(),
                invocation.producer.attempt,
            )
        })
        .collect()
}

/// The report the drafter writes and publishes.
const KEPT: &[u8] = b"# Kept\n";

/// An Agent drafter's accepted file reaches its consumer after recovery,
/// from the exact accepted descriptor, while the object inventory cannot be
/// read. The first claim's drafter writes and publishes `report` and
/// settles; its consumer is then refused Python capacity before retaining
/// anything. The reclaim recovers the settled drafter alone. A caller
/// whose inventory queries all fail cannot read the report as evidence,
/// and replaying the drafter's original publish call, whose receipt the
/// drafter already received, with neither scratch nor writable storage,
/// answers the original receipt and accepts nothing. Once capacity frees,
/// the recovered run as that caller never runs the drafter again, and the
/// consumer reads exactly the accepted bytes.
///
/// # Panics
/// Panics when the run is not deferred busy, the recovery holds other
/// attempts, the inventory is readable, the replay accepts or uploads, the
/// drafter runs again, or the consumer reads other bytes.
#[tokio::test(flavor = "multi_thread")]
async fn agent_publications_reach_a_recovered_consumer() {
    let leased = Leased::start().await;
    let limits = saturable();
    let caller = leased.publisher([Permission::tool_invoke_any()]);
    let sandbox = Arc::new(
        Sandbox::start(worker(), limits, limits.queued)
            .await
            .expect("pool starts"),
    );
    let none = serde_json::Map::new();
    let far = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    // One admitted operation holds the only worker admission while pending.
    let mut held = Box::pin(sandbox.run("def f():\n    return 1", "f", &none, Vec::new(), far));
    assert!(futures_util::FutureExt::now_or_never(&mut held).is_none());
    let model = MockProvider::new(ProviderName::OpenAi);
    let provider: Arc<dyn Provider> = Arc::new(model.clone());
    let spec = agent_steps(&[
        ("draft", 0, &["fs.write", "publish"], json!({})),
        (
            "consume",
            1,
            &["fs.read", "code.run"],
            json!({ "/inputs/report.md": "steps.draft.publications.report" }),
        ),
    ]);
    let report = std::str::from_utf8(KEPT).expect("UTF-8");
    model.push_response(answer(
        None,
        &[
            (
                "fs_write",
                json!({ "path": "/work/report.md", "content": report }),
            ),
            (
                "publish",
                json!({ "name": "report", "path": "/work/report.md" }),
            ),
        ],
    ));
    model.push_response(answer(Some("{}"), &[]));
    model.push_response(answer(None, &[("code_run", json!({ "source": "1" }))]));

    let interrupted = agent_engine(
        &leased,
        &leased.run,
        &caller,
        &provider,
        &sandbox,
        Vec::new(),
    )
    .run(Arc::clone(&spec), notes(), far)
    .await;
    assert!(
        matches!(interrupted, Err(EngineError::Busy(_))),
        "{interrupted:?}"
    );
    assert_eq!(model.remaining(), 0);
    leased.settled().await;

    let current = leased.reclaim().await;
    let recovered = current.snapshot();
    assert_eq!(
        attempts(&recovered),
        [("draft".to_owned(), 1)],
        "the reclaim recovers the settled drafter; the refused consumer spent nothing"
    );
    let blind = Caller {
        // Not a SYSTEM principal, so every inventory query is unavailable.
        executed_by: Some(PrincipalId::new(uuid::Uuid::new_v4())),
        ..caller.clone()
    };
    let evidence = TenantEvidence::new(leased.server.state().clone(), blind.clone(), limits)
        .with_execution(Arc::clone(&current));
    let unread = evidence
        .read(&InputDeclaration::File {
            sha256: wyrd_spec::auth::Sha256Hex::digest(KEPT),
            size_bytes: KEPT.len() as u64,
            format: MediaFormat::Markdown,
            filename: None,
        })
        .await
        .expect_err("the object inventory cannot be read by this caller");
    assert!(
        matches!(
            &unread,
            BlockError::Retryable { reason } if reason == "the evidence inventory cannot be read now"
        ),
        "{unread:?}"
    );

    let producer = Producer {
        step_id: TaskId::new("draft").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    leased.settled().await;
    let revision = leased.revision().await.0;
    let wyrd_storage::BackendConfig::Local { root } =
        leased.server.state().storage.backend_config()
    else {
        panic!("the test server stores locally");
    };
    let objects = root
        .join(leased.server.data_tenant_id().to_string())
        .join("evidence/sha256");
    let replayed = {
        std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let empty = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
        // The Agent's second call of its first turn was the publish.
        let replayed = Publications::new(leased.server.state(), &blind, &current, limits)
            .path(
                &call("call_1", &producer, "report"),
                &empty,
                "/work/report.md",
            )
            .await;
        std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        replayed
    };
    assert_eq!(
        replayed.expect("the original call replays without scratch or storage"),
        PublicationKind::File
    );
    leased.settled().await;
    assert_eq!(
        leased.revision().await.0,
        revision,
        "the replay accepts nothing"
    );

    drop(held);
    model.push_response(answer(
        None,
        &[("fs_read", json!({ "path": "/inputs/report.md" }))],
    ));
    model.push_response(answer(Some("{}"), &[]));
    let outcome = agent_engine(
        &leased,
        &current,
        &blind,
        &provider,
        &sandbox,
        recovered.invocations,
    )
    .run(spec, notes(), far)
    .await
    .expect("the recovered run settles");
    assert_eq!(
        model.remaining(),
        0,
        "the settled drafter makes no model call"
    );
    let steps: Vec<_> = outcome
        .steps
        .iter()
        .map(|step| (step.id.as_str(), step.status, step.attempts))
        .collect();
    assert_eq!(
        steps,
        [
            ("draft", StepStatus::Succeeded, 1),
            ("consume", StepStatus::Succeeded, 1)
        ],
        "{outcome:?}"
    );
    leased.settled().await;
    assert_eq!(
        attempts(&leased.accepted().await),
        [("draft".to_owned(), 1), ("consume".to_owned(), 1)]
    );
    let mounted = Workspaces::new(limits, limits.queued.workspaces)
        .open(
            &BTreeMap::from([(
                "/inputs/report.md".to_owned(),
                CallInput {
                    value: CallValue::Publication {
                        producer,
                        name: PublicationName::new("report").expect("name"),
                    },
                    sources: BTreeSet::from([SlotName::new("x").expect("slot")]),
                },
            )]),
            &evidence,
        )
        .await
        .expect("the accepted file mounts from its descriptor");
    assert_eq!(
        std::fs::read(mounted.inputs_dir().join("report.md")).expect("mounted"),
        KEPT
    );
    let read = serde_json::to_vec(&mounted.read("/inputs/report.md", 0).expect("reads"))
        .expect("the read serializes");
    let consumed = &outcome.steps[1].tool_calls[0];
    assert_eq!(
        (
            consumed.output_sha256,
            consumed.returned_bytes,
            consumed.error.as_deref()
        ),
        (
            Some(wyrd_spec::auth::Sha256Hex::digest(&read)),
            read.len() as u64,
            None
        ),
        "the consumer read exactly the accepted bytes"
    );
    drop(mounted);
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A File publication whose first reply never reaches its caller still
/// answers that caller's retry with the original receipt. The caller waits
/// on a reply channel whose sender the responder owns; the responder
/// publishes `report`, succeeds, and drops the sender instead of sending
/// that success, so the reply is lost after acceptance and before
/// delivery. The caller sees no reply, reads the accepted entry back from
/// Bifrost, and retries the same call,
/// producer, name, and path against the reclaimed state with its scratch
/// tree gone and storage unwritable: it receives the File receipt, and the
/// accepted revision and entry are unchanged.
///
/// # Panics
/// Panics when the first publication fails or is not accepted, a reply
/// arrives, or the retry fails, uploads, or changes the accepted revision
/// or entry.
#[tokio::test(flavor = "multi_thread")]
async fn publication_receipt_survives_a_lost_reply() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let producer = Producer {
        step_id: TaskId::new("draft").expect("task"),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    let publish = call("call_1", &producer, "report");
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    put(&workspace, "report.md", KEPT);

    let (reply, received) =
        tokio::sync::oneshot::channel::<Result<PublicationKind, skald_tool::ToolError>>();
    // The responder owns the caller's reply sender and publishes; the loss
    // point is that it drops the sender after its successful result instead
    // of sending it, so the caller's wait ends only once publishing has.
    let responder = async {
        let first = Publications::new(leased.server.state(), &caller, &leased.run, limits)
            .path(&publish, &workspace, "/work/report.md")
            .await;
        drop(reply);
        first
    };
    let (first, lost) = tokio::join!(responder, received);
    assert_eq!(
        first.expect("the first publication succeeds before its reply is lost"),
        PublicationKind::File
    );
    lost.expect_err("the caller never receives the first reply");
    leased.settled().await;

    let (revision, state) = leased.revision().await;
    let accepted = state
        .publications
        .into_iter()
        .map(|accepted| accepted.publication)
        .find(|publication| publication.producer == producer)
        .expect("the publication was accepted before its reply was lost");
    assert_eq!(accepted.name.as_str(), "report");
    assert_eq!(
        accepted.content,
        PublicationContent::File {
            manifest: PublicationManifest {
                files: vec![PublicationFile {
                    path: "report.md".to_owned(),
                    sha256: wyrd_spec::auth::Sha256Hex::digest(KEPT),
                    size_bytes: KEPT.len() as u64,
                }],
                directories: Vec::new(),
            },
        },
        "the accepted File holds the published bytes"
    );
    leased.settled().await;

    let current = leased.reclaim().await;
    std::fs::remove_file(workspace.work_dir().join("report.md")).expect("scratch removed");
    let wyrd_storage::BackendConfig::Local { root } =
        leased.server.state().storage.backend_config()
    else {
        panic!("the test server stores locally");
    };
    let objects = root
        .join(leased.server.data_tenant_id().to_string())
        .join("evidence/sha256");
    std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let retried = Publications::new(leased.server.state(), &caller, &current, limits)
        .path(&publish, &workspace, "/work/report.md")
        .await;
    std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert_eq!(
        retried.expect("the retry replays without scratch or storage"),
        PublicationKind::File,
        "the retry answers the original receipt"
    );
    leased.settled().await;
    assert_eq!(
        leased.revision().await.0,
        revision,
        "the retry accepts nothing"
    );
    leased.settled().await;
    assert_eq!(
        leased.read(&producer, "report").await,
        Some(accepted.content),
        "the accepted entry is unchanged"
    );
    leased.settled().await;
    assert_eq!(
        leased.entries().await,
        [("draft".to_owned(), 1, "report".to_owned())],
        "one entry for the call and its retry"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A model that answers through `inner` once each call is released.
struct HeldModel {
    /// The scripted answers.
    inner: MockProvider,
    /// Permits the held calls wait for.
    release: tokio::sync::Semaphore,
    /// Signalled when a call arrives.
    arrived: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Provider for HeldModel {
    /// Answers `request` through the scripted model after a release permit.
    ///
    /// # Errors
    /// Returns the scripted model's error.
    ///
    /// # Panics
    /// Panics when the release semaphore closes while a call waits.
    async fn send(
        &self,
        request: &skald_spec::ProviderRequest,
    ) -> Result<skald_spec::ProviderResponse, skald_providers::ProviderError> {
        self.arrived.notify_one();
        self.release
            .acquire()
            .await
            .expect("the release stays open")
            .forget();
        self.inner.send(request).await
    }

    /// Streams through the scripted model.
    ///
    /// # Errors
    /// Returns the scripted model's error.
    async fn stream(
        &self,
        request: &skald_spec::ProviderRequest,
    ) -> Result<skald_providers::ProviderStream, skald_providers::ProviderError> {
        self.inner.stream(request).await
    }

    /// The scripted model's provider.
    fn name(&self) -> ProviderName {
        self.inner.name()
    }
}

/// Dropping a running Agent step, as a disconnecting direct caller drops
/// its request, starts no later Agent work. The drafter's first model turn
/// publishes `early` from memory; the run is dropped while its next model
/// call waits, and once that call is released the model has been asked for
/// nothing more and `after` is never published.
///
/// # Panics
/// Panics when the run finishes early, a model call does not arrive, or the
/// dropped run calls the model or publishes again.
#[tokio::test(flavor = "multi_thread")]
async fn dropped_agent_work_makes_no_later_call() {
    let leased = Leased::start().await;
    let limits = saturable();
    let caller = leased.publisher([Permission::tool_invoke_any()]);
    let sandbox = Arc::new(
        Sandbox::start(worker(), limits, limits.queued)
            .await
            .expect("pool starts"),
    );
    let scripted = MockProvider::new(ProviderName::OpenAi);
    for name in ["early", "after"] {
        scripted.push_response(answer(
            None,
            &[("publish", json!({ "name": name, "value": 1 }))],
        ));
    }
    scripted.push_response(answer(Some("{}"), &[]));
    let held = Arc::new(HeldModel {
        inner: scripted.clone(),
        release: tokio::sync::Semaphore::new(0),
        arrived: tokio::sync::Notify::new(),
    });
    let far = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let model = Arc::clone(&held) as Arc<dyn Provider>;
    let engine = agent_engine(&leased, &leased.run, &caller, &model, &sandbox, Vec::new());
    let mut run = Box::pin(engine.run(publishing_step("draft", 0), notes(), far));

    for (turn, release) in [("the first turn", 1), ("the second turn", 0)] {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::select! {
                outcome = &mut run => panic!("returned before {turn}: {outcome:?}"),
                () = held.arrived.notified() => {}
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{turn} reaches the model"));
        held.release.add_permits(release);
    }
    let early = |state: &ExecutionState| {
        state
            .publications
            .iter()
            .any(|accepted| accepted.publication.name.as_str() == "early")
    };
    assert!(
        early(&leased.run.snapshot()),
        "the early publication is live"
    );
    drop(run);
    held.release
        .add_permits(tokio::sync::Semaphore::MAX_PERMITS);

    assert_eq!(
        scripted.remaining(),
        2,
        "a dropped run makes no later model call"
    );
    assert!(
        leased
            .run
            .publications(&TaskId::new("draft").expect("task"))
            .iter()
            .all(|accepted| accepted.publication.name.as_str() != "after"),
        "a dropped run publishes nothing more"
    );
    leased.settled().await;
    assert!(
        early(&leased.accepted().await),
        "the live publication persists"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A verdict-only outcome with no requirement or step.
fn outcome(verdict: VerificationVerdict) -> ExecutedOutcome {
    ExecutedOutcome {
        verdict,
        requirements: Vec::new(),
        steps: Vec::new(),
        evidence: BTreeMap::new(),
    }
}

/// A reclaim resumes invocation attempts from the accepted Bifrost state.
///
/// The first claim begins two attempts and abandons a third before it ran.
/// The reclaim recovers exactly the two begun attempts, both still spent, so
/// a retry of one starts its next attempt, and the abandoned attempt is gone.
///
/// # Panics
/// Panics when a revision is refused or the reclaim recovers other attempts.
#[tokio::test(flavor = "multi_thread")]
async fn invocations_resume_from_bifrost() {
    let leased = Leased::start().await;
    let task = |id: &str| TaskId::new(id).expect("task");
    for step in ["review", "judge", "refused"] {
        leased
            .run
            .begin(&Producer::plain(&task(step), 1))
            .await
            .expect("the attempt is accepted");
    }
    leased
        .run
        .forget(&Producer::plain(&task("refused"), 1))
        .await
        .expect("the attempt refused capacity is forgotten");
    leased.settled().await;

    let current = leased.reclaim().await;
    assert_eq!(
        attempts(&current.snapshot()),
        [("review".to_owned(), 1), ("judge".to_owned(), 1)]
    );
    current
        .begin(&Producer::plain(&task("judge"), 2))
        .await
        .expect("the interrupted attempt retries as its next attempt");
    leased.settled().await;
    assert_eq!(
        attempts(&leased.accepted().await),
        [
            ("review".to_owned(), 1),
            ("judge".to_owned(), 1),
            ("judge".to_owned(), 2)
        ]
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A reclaim recovers every accepted publication with its exact private
/// File descriptors and replay receipt from the Bifrost state.
///
/// The first claim publishes a file and a JSON value. The reclaim's state
/// holds the identical accepted publications, objects included; replaying
/// the same call after its scratch is removed is accepted without a new
/// entry, the replaced writer is refused, and an evidence reader bound to
/// the recovered state resolves the same descriptors from it.
///
/// # Panics
/// Panics when a publication is refused or differs after recovery, a replay
/// adds an entry, the stale writer publishes, or the reader resolves other
/// descriptors.
#[tokio::test(flavor = "multi_thread")]
async fn publications_recover_exact_descriptors_from_run_state() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let task = TaskId::new("review").expect("task");
    leased
        .run
        .begin(&Producer::plain(&task, 1))
        .await
        .expect("the attempt begins");
    let producer = Producer {
        step_id: task.clone(),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    std::fs::write(workspace.work_dir().join("report.md"), "# Kept\n").expect("write");
    let first = Publications::new(leased.server.state(), &caller, &leased.run, limits);
    first
        .path(
            &call("p1", &producer, "report"),
            &workspace,
            "/work/report.md",
        )
        .await
        .expect("the file publishes");
    first
        .json(&call("p2", &producer, "verdict"), json!({"ok": true}))
        .expect("the value publishes");
    let accepted = leased.run.publications(&task);
    leased.settled().await;

    let current = leased.reclaim().await;
    leased.lost().await;
    assert_eq!(
        current.publications(&task),
        accepted,
        "the reclaim recovers the exact accepted publications"
    );
    std::fs::remove_file(workspace.work_dir().join("report.md")).expect("scratch is removed");
    Publications::new(leased.server.state(), &caller, &current, limits)
        .path(
            &call("p1", &producer, "report"),
            &workspace,
            "/work/report.md",
        )
        .await
        .expect("the same call replays its receipt without scratch");
    first
        .json(&call("p3", &producer, "late"), json!(null))
        .expect_err("the replaced writer publishes nothing");
    leased.settled().await;
    assert_eq!(
        leased.entries().await,
        [
            ("review".to_owned(), 1, "report".to_owned()),
            ("review".to_owned(), 1, "verdict".to_owned()),
        ]
    );
    let evidence = TenantEvidence::new(leased.server.state().clone(), caller, limits)
        .with_execution(Arc::clone(&current));
    let recovered = evidence.publications(&task);
    assert_eq!(recovered, accepted);
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// The selected terminal outcome and the public inspection reads come from
/// the accepted Bifrost state.
///
/// The first claim records an attempt and a publication and selects a failed
/// outcome. A reclaim recovers that selection and, asked to select another
/// outcome, returns the original one with its result identity and times.
/// The public invocation and publication listings return the accepted
/// entries for a reader of the execution's Verifier.
///
/// # Panics
/// Panics when a revision is refused, the reclaim selects another outcome or
/// identity, or an inspection read differs from the accepted state.
#[tokio::test(flavor = "multi_thread")]
async fn terminal_selection_and_inspection_use_run_state() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = leased.publisher([]);
    let task = TaskId::new("review").expect("task");
    leased
        .run
        .begin(&Producer::plain(&task, 1))
        .await
        .expect("the attempt begins");
    let producer = Producer {
        step_id: task.clone(),
        item_index: None,
        iteration_index: None,
        attempt: 1,
    };
    Publications::new(leased.server.state(), &caller, &leased.run, limits)
        .json(&call("p1", &producer, "report"), json!({"v": 1}))
        .expect("the value publishes");
    let selected = leased
        .run
        .select(&outcome(VerificationVerdict::Failed), chrono::Utc::now())
        .expect("the outcome is selected");
    leased
        .run
        .persisted()
        .await
        .expect("the selection is accepted");
    leased.settled().await;

    let current = leased.reclaim().await;
    assert_eq!(
        current.snapshot().terminal.map(Decision::Selected),
        Some(selected.clone())
    );
    assert_eq!(
        current
            .select(&outcome(VerificationVerdict::Passed), chrono::Utc::now())
            .expect("the selection reads"),
        selected,
        "a reclaim re-delivers the selected outcome and identity"
    );

    let reader = leased
        .server
        .bootstrap_user("inspection-reader", &["viewer"])
        .await
        .expect("reader bootstraps");
    let wyrd_testing::Bootstrap::User { jwt, .. } = reader else {
        panic!("a user bootstraps as a user");
    };
    let read = async |path: &str| {
        let response = leased
            .server
            .oneshot_authenticated(
                &jwt,
                axum::http::Request::builder()
                    .uri(format!(
                        "/v1/verification/executions/{}/{path}",
                        leased.lease.execution_id
                    ))
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("the read responds");
        assert_eq!(response.status(), axum::http::StatusCode::OK, "{path}");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body reads");
        serde_json::from_slice::<Value>(&body).expect("the page is JSON")
    };
    let invocations = read("invocations").await;
    assert_eq!(
        invocations["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|item| item["producer"]["step_id"].clone())
            .collect::<Vec<_>>(),
        [json!("review")],
        "{invocations}"
    );
    let publications = read("publications").await;
    assert_eq!(
        publications["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|item| item["name"].clone())
            .collect::<Vec<_>>(),
        [json!("report")],
        "{publications}"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A user's bearer token for `name`, bootstrapped with `roles`.
///
/// # Panics
/// Panics when the user does not bootstrap.
async fn jwt(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    let wyrd_testing::Bootstrap::User { jwt, .. } = server
        .bootstrap_user(name, roles)
        .await
        .expect("user bootstraps")
    else {
        panic!("a user bootstraps as a user");
    };
    jwt
}

/// The status and JSON body of `method` on the execution route `path`.
///
/// # Panics
/// Panics when the request does not respond or its body is not JSON.
async fn respond(
    server: &WyrdTestServer,
    jwt: &str,
    method: &str,
    path: &str,
) -> (axum::http::StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            axum::http::Request::builder()
                .method(method)
                .uri(format!("/v1/verification/executions/{path}"))
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("the request responds");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    (
        status,
        serde_json::from_slice(&body).expect("the body is JSON"),
    )
}

impl Leased {
    /// Delete the execution's queue row, then its Verifier and subject Cards.
    ///
    /// # Panics
    /// Panics as [`forget`] does.
    async fn forget_queue_and_cards(&self) {
        forget(&self.server, &[self.lease.execution_id]).await;
    }
}

/// Delete `executions`' queue rows, then every Verifier and subject Card they
/// name.
///
/// # Panics
/// Panics when a delete fails or deletes less than every row and Card.
async fn forget(server: &WyrdTestServer, executions: &[ExecutionId]) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let ids: Vec<uuid::Uuid> = executions.iter().map(ExecutionId::as_uuid).collect();
    let rows: Vec<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
        "DELETE FROM wyrd.verifier_executions WHERE execution_id = ANY($1) \
         RETURNING verifier_card_uid, card_uid",
    )
    .bind(&ids)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("the queue rows delete");
    assert_eq!(rows.len(), executions.len(), "every queue row deletes");
    let cards: BTreeSet<uuid::Uuid> = rows.into_iter().flat_map(|(v, c)| [v, c]).collect();
    let deleted = sqlx::query("DELETE FROM wyrd.cards WHERE card_uid = ANY($1)")
        .bind(cards.iter().copied().collect::<Vec<_>>())
        .execute(&mut **conn.transaction())
        .await
        .expect("the Cards delete")
        .rows_affected();
    assert_eq!(deleted, cards.len() as u64, "every Card deletes");
    conn.commit().await.expect("the deletes commit");
}

/// The table every execution state revision is written to.
const EVENTS: &str = "bifrost.verification.execution_events";

/// An engine failure recorded in the state is the execution's historical
/// error: the public read answers it over a different queue error, and
/// still answers it once the queue row and both Cards are gone. A later
/// failure does not replace it.
///
/// # Panics
/// Panics when a revision is refused or a read differs from the recorded
/// failure.
#[tokio::test(flavor = "multi_thread")]
async fn failures_read_back_from_state_instead_of_the_queue_payload() {
    let leased = Leased::start().await;
    let execution = leased.lease.execution_id;
    let engine = VerificationError {
        code: "engine_failed".to_owned(),
        message: "the engine failed".to_owned(),
    };
    leased
        .run
        .fail(VerificationExecutionStatus::Errored, &engine)
        .expect("the failure is recorded");
    leased
        .run
        .fail(
            VerificationExecutionStatus::TimedOut,
            &VerificationError {
                code: "later".to_owned(),
                message: "a later failure".to_owned(),
            },
        )
        .expect("a repeated failure answers the first");
    leased
        .run
        .persisted()
        .await
        .expect("the failure is accepted");
    let mut conn = leased
        .server
        .tenant_conn_for(leased.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    VerifierExecutionQueue::default()
        .terminate(
            &mut conn,
            leased.lease,
            TerminalStatus::Errored,
            &VerificationError {
                code: "queue_only".to_owned(),
                message: "the queue's copy".to_owned(),
            },
        )
        .await
        .expect("terminate runs");
    conn.commit().await.expect("the settlement commits");

    let viewer = jwt(&leased.server, "failure-reader", &["viewer"]).await;
    let read = async || {
        let (status, body) = respond(&leased.server, &viewer, "GET", &execution.to_string()).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{body}");
        (body["status"].clone(), body["error"]["code"].clone())
    };
    let expected = (json!("errored"), json!("engine_failed"));
    assert_eq!(
        read().await,
        expected,
        "a queued read answers the state's error"
    );
    leased.forget_queue_and_cards().await;
    assert_eq!(
        read().await,
        expected,
        "a retained read answers the state's error"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A state in a format this worker does not run keeps its custody: a
/// reclaim refuses it as incompatible instead of resuming or starting fresh,
/// and the accepted revision is left unchanged.
///
/// # Panics
/// Panics when the forged revision is not accepted, the reclaim resumes or
/// starts fresh, or the accepted revision changes.
#[tokio::test(flavor = "multi_thread")]
async fn state_format_changes_preserve_existing_custody() {
    let leased = Leased::start().await;
    let state = leased.server.state();
    let tenant = leased.server.data_tenant_id();
    let mut newer = serde_json::to_value(ExecutionState::fresh()).expect("state encodes");
    newer["format"] = json!(EXECUTION_STATE_FORMAT + 1);
    newer["sequence"] = json!(7);
    let verifier_version = String::new();
    let mut conn = leased
        .server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let (card, verifier): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT card_uid, verifier_card_uid FROM wyrd.verifier_executions \
         WHERE execution_id = $1",
    )
    .bind(leased.lease.execution_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("the execution's Cards read");
    drop(conn);
    let card_uid = CardUid::from_uuid(card).expect("uid");
    let verifier_uid = CardUid::from_uuid(verifier).expect("uid");
    let row = ResultPayloadBuilder::new(
        ResultRun {
            execution_id: leased.lease.execution_id,
            run_id: None,
            verifier_version: &verifier_version,
            card_uid: &card_uid,
            owner_card_uid: None,
            group_id: None,
            deployment_environment: None,
            binding_id: None,
            trigger: None,
            window: None,
        },
        &verifier_uid,
        chrono::Utc::now(),
        chrono::Utc::now(),
    )
    .checkpoint(1, false, &newer)
    .expect("the revision row builds");
    let (sender, acked) = tokio::sync::oneshot::channel();
    state.bifrost.scribe_outbox().stage(
        tenant,
        ScribeWrite::ExecutionState {
            batch: revision_batch(leased.lease.execution_id, 1),
            checkpoint: Arc::new(Checkpoint::prepared(
                1,
                Arc::new(StateRow {
                    execution_id: leased.lease.execution_id,
                    identity: stand_in_identity(&verifier_uid, &card_uid),
                    verifier_uid,
                    card_uid,
                    owner_card_uid: None,
                    run_id: None,
                    deployment_environment: None,
                    attribution: Attribution::new(leased.system, leased.system),
                }),
                row.row,
            )),
            attribution: Attribution::new(leased.system, leased.system),
            acked: std::sync::Mutex::new(Some(sender)),
        },
    );
    acked.await.expect("the newer-format revision is accepted");

    let refused = leased.try_reclaim().await;
    assert!(
        matches!(refused, Err(StateError::Incompatible(_))),
        "a newer state format is refused: {:?}",
        refused.as_ref().err()
    );
    let query = Leased::query(&leased.server, leased.system).await;
    let kept = RunState::accepted(
        state,
        tenant,
        leased.lease.execution_id,
        &InputResolver::new(&query, None),
    )
    .await;
    assert!(
        matches!(kept, Err(StateError::Incompatible(_))),
        "the accepted revision is unchanged and still refused: {:?}",
        kept.as_ref().err()
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Only queued verification keeps Bifrost execution state; direct
/// verification records its input, output and result alone.
///
/// Two direct executions of one assertion Verifier get distinct identities
/// and admit no queue record. Once their records drain, neither has an
/// execution state revision, and the status read, which resolves through
/// the execution's `direct` input record and its result, returns the
/// outcome the call returned. A queued
/// submission of the same request run by the runtime completes with the
/// result its accepted state selected.
///
/// # Panics
/// Panics when provisioning or a request fails, a direct execution shares an
/// identity, admits a queue record, keeps execution state or reads back
/// another outcome, or the queued result
/// differs from its selected one.
#[tokio::test(flavor = "multi_thread")]
async fn only_queued_verification_keeps_run_state() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    server
        .ensure_builtin_tables_for_test(tenant)
        .await
        .expect("tenant receives every built-in table");
    let fixture = VerificationFixture::provision(server.state(), tenant)
        .await
        .expect("the tenant provisions");
    let (subject, _) = fixture.service("shared").await.expect("service");
    let verifier = fixture
        .verifier(
            "shared-assertion",
            &json!({
                "inputs": { "x": { "type": "json" } },
                "requirements": [{
                    "id": "x_is_one",
                    "statement": "The input x equals one.",
                    "check": { "kind": "assert", "path": "inputs.x",
                               "operator": "equals", "expected": 1 }
                }]
            }),
        )
        .await
        .expect("verifier");
    let wyrd_testing::Bootstrap::Machine { api_key, .. } = server
        .bootstrap_service_in_tenant(tenant, "shared-caller", &["editor"])
        .await
        .expect("caller bootstraps")
    else {
        panic!("service bootstrap returned a user principal");
    };
    let jwt = server
        .exchange_api_key(&api_key)
        .await
        .expect("caller key exchanges");
    let direct = async || {
        let response = server
            .oneshot_authenticated(
                &jwt,
                axum::http::Request::builder()
                    .method(axum::http::Method::POST)
                    .uri("/v1/verification/execute")
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(
                        json!({
                            "verifier_card_uid": verifier,
                            "card_uid": subject,
                            "inputs": { "x": { "kind": "json", "value": 1 } },
                        })
                        .to_string(),
                    ))
                    .expect("request builds"),
            )
            .await
            .expect("the execution responds");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body reads");
        serde_json::from_slice::<VerificationResult>(&body).expect("a result")
    };
    let results = [direct().await, direct().await];
    assert_ne!(
        results[0].execution_id, results[1].execution_id,
        "fresh direct calls are distinct executions"
    );
    server
        .flush_bifrost()
        .await
        .expect("the direct records drain");
    for result in &results {
        assert!(
            fixture
                .execution_state(server.state(), result.execution_id)
                .await
                .expect("the state reads")
                .is_none(),
            "a direct execution keeps no execution state"
        );
        let id = result.execution_id.to_string();
        let response = server
            .oneshot_authenticated(
                &jwt,
                axum::http::Request::builder()
                    .uri(format!("/v1/verification/executions/{id}"))
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("the status responds");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body reads");
        let status: serde_json::Value = serde_json::from_slice(&body).expect("a status");
        assert_eq!(
            (&status["status"], &status["result"]["outcome"]),
            (
                &json!("completed"),
                &serde_json::to_value(&result.outcome).expect("the outcome serializes")
            ),
            "the status reads the recorded outcome"
        );
    }
    assert!(
        fixture
            .executions()
            .await
            .expect("executions read")
            .is_empty(),
        "a direct execution admits no queue record"
    );

    let queued = fixture
        .submit(
            &verifier,
            &subject,
            BTreeMap::from([(
                SlotName::new("x").expect("slot"),
                InputDeclaration::Json { value: json!(1) },
            )]),
        )
        .await
        .expect("the execution is admitted");
    let runtime = VerificationRuntime::builder(server.state())
        .build()
        .expect("the runtime composes");
    let stop = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    let row = loop {
        let row = fixture.execution(queued).await.expect("execution reads");
        if row.status == "completed" {
            break row;
        }
        assert!(tokio::time::Instant::now() < deadline, "{row:?}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    stop.cancel();
    task.await.expect("the runtime stops");
    let selected = fixture
        .execution_state(server.state(), queued)
        .await
        .expect("the state reads")
        .expect("the queued execution claimed its state")
        .terminal
        .expect("the queued execution selected its outcome");
    assert_eq!(row.result_id, Some(selected.result_id.as_uuid()));
    assert_eq!(selected.outcome.verdict, results[0].outcome.verdict);
    server.shutdown().await.expect("test server shuts down");
}

/// A Scribe that admits every frame through the server's own, holding each
/// execution state revision until `release` has a permit.
struct HeldRevisions {
    /// The server's Scribe.
    scribe: Arc<dyn Scribe>,
    /// Permits the held revisions wait for.
    release: tokio::sync::Semaphore,
    /// Signalled when a held revision arrives.
    arrived: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Scribe for HeldRevisions {
    /// Admits `frame` through the server's Scribe, after a release permit
    /// when it is a state revision.
    ///
    /// # Errors
    /// Returns the server Scribe's refusal.
    ///
    /// # Panics
    /// Panics when the release semaphore closes while a revision waits.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        if frame.table.fqn() == EVENTS {
            self.arrived.notify_one();
            self.release
                .acquire()
                .await
                .expect("the release stays open")
                .forget();
        }
        self.scribe.ingest_frame(frame).await
    }

    /// Resolves through the server's Scribe.
    ///
    /// # Errors
    /// Returns the server Scribe's resolution error.
    async fn resolve_write_table(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> Result<TableUid, ScribeError> {
        self.scribe.resolve_write_table(tenant, table).await
    }
}

/// A reclaim recovers exactly the revisions Scribe accepted while the old
/// writer kept recording progress in memory, and the old writer's late
/// revision is refused.
///
/// The factual check's revision is held at Scribe while the sensitive check
/// records its start from memory. Once the factual revision alone is
/// accepted, the old writer's next revision, holding both checks, is held,
/// and the reclaim resumes from the factual check without the sensitive
/// one. The new writer records the combination and the sensitive check and
/// persists them; the old writer's late revision then conflicts with the
/// new writer's, so the old writer stops and the accepted state holds the
/// new writer's three checks.
///
/// # Panics
/// Panics when a step waits for Scribe, the recovered state holds an
/// unaccepted change, or the replaced writer's revision is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn batched_checkpoints_recover_exactly_the_accepted_batch() {
    let leased = Leased::start().await;
    let held = Arc::new(HeldRevisions {
        scribe: Arc::clone(
            leased
                .server
                .state()
                .bifrost_ingest()
                .expect("the server runs Scribe")
                .scribe(),
        ) as _,
        release: tokio::sync::Semaphore::new(0),
        arrived: tokio::sync::Notify::new(),
    });
    let outbox = ScribeSink::local_outbox(
        Arc::clone(&held) as _,
        Some(leased.server.state().postgres.wyrd().clone()),
    );
    let first = leased
        .reclaim_on(outbox)
        .await
        .expect("the execution state claims");
    let task = |id: &str| TaskId::new(id).expect("task");
    let (factual, sensitive, combine) = (task("factual"), task("sensitive"), task("combine"));
    let arrived = async || {
        tokio::time::timeout(std::time::Duration::from_secs(10), held.arrived.notified())
            .await
            .expect("a revision reaches Scribe");
    };

    first
        .begin(&Producer::plain(&factual, 1))
        .await
        .expect("the factual check records from memory");
    arrived().await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        first.begin(&Producer::plain(&sensitive, 1)),
    )
    .await
    .expect("a sibling records while a revision is held")
    .expect("the sensitive check records from memory");
    assert_eq!(attempts(&first.snapshot()).len(), 2);
    held.release.add_permits(1);
    arrived().await;
    assert_eq!(
        attempts(&leased.accepted().await),
        [("factual".to_owned(), 1)],
        "the accepted revision holds only the factual check"
    );

    let current = leased.reclaim().await;
    assert_eq!(attempts(&current.snapshot()), [("factual".to_owned(), 1)]);
    current
        .begin(&Producer::plain(&combine, 1))
        .await
        .expect("the combination records");
    current
        .begin(&Producer::plain(&sensitive, 1))
        .await
        .expect("the sensitive check records");
    current.persisted().await.expect("the new writer persists");
    held.release
        .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
    assert!(
        matches!(first.persisted().await, Err(StateError::Stale)),
        "the replaced writer's late revision is refused"
    );
    assert!(
        first.begin(&Producer::plain(&combine, 1)).await.is_err(),
        "the replaced writer records nothing more"
    );
    assert_eq!(
        attempts(&leased.accepted().await),
        [
            ("factual".to_owned(), 1),
            ("combine".to_owned(), 1),
            ("sensitive".to_owned(), 1)
        ]
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// The settlement each of `state`'s invocation attempts committed, by step.
fn settlements(state: &ExecutionState) -> BTreeMap<String, Option<InvocationSettlement>> {
    state
        .invocations
        .iter()
        .map(|record| {
            (
                record.producer.step_id.as_str().to_owned(),
                record.settlement.clone(),
            )
        })
        .collect()
}

/// The hosted engine releases a dependent from its predecessors' live
/// settlements, and a restart reuses only accepted ones.
///
/// Two independent code steps run in parallel on a writer whose every state
/// revision is held before Scribe; their dependent consumes both outputs.
/// With nothing accepted, the run completes and passes from memory. Once
/// the first revision is accepted, the execution crashes, and a reclaimed
/// engine restores exactly the accepted prefix, retries every step whose
/// interrupted attempt is spent, and passes with both values.
///
/// # Panics
/// Panics when the run waits for Scribe, fails, or the restart restores a
/// settlement that was not accepted or loses one that was.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_settlements_release_their_hosted_dependent() {
    let leased = Leased::start().await;
    let held = Arc::new(HeldRevisions {
        scribe: Arc::clone(
            leased
                .server
                .state()
                .bifrost_ingest()
                .expect("the server runs Scribe")
                .scribe(),
        ) as _,
        release: tokio::sync::Semaphore::new(0),
        arrived: tokio::sync::Notify::new(),
    });
    let outbox = ScribeSink::local_outbox(
        Arc::clone(&held) as _,
        Some(leased.server.state().postgres.wyrd().clone()),
    );
    let writer = leased
        .reclaim_on(outbox)
        .await
        .expect("the execution state claims");
    let blocks = HostedBlocks::start(worker(), HostedLimits::default())
        .await
        .expect("the sandbox starts");
    let code =
        |source: &str| json!({ "kind": "code", "entrypoint": "f", "source": { "inline": source } });
    let spec: VerifierSpec = serde_json::from_value(json!({
        "steps": [
            { "id": "left", "retries": 1, "action": code("def f():\n    return 1") },
            { "id": "right", "retries": 1, "action": code("def f():\n    return 'two'") },
            { "id": "combine", "depends_on": ["left", "right"],
              "action": { "kind": "code", "entrypoint": "f",
                          "source": { "inline": "def f(a, b):\n    return [a, b]" },
                          "inputs": { "a": "steps.left.output", "b": "steps.right.output" } },
              "requirements": [{ "id": "combined", "statement": "The dependent reads both values.",
                  "check": { "kind": "assert", "path": "steps.combine.output",
                             "operator": "equals", "expected": [1, "two"] } }] }
        ]
    }))
    .expect("Verifier decodes");
    spec.validate().expect("valid Verifier");
    let spec = Arc::new(spec);
    let caller = leased.publisher([]);
    let verifier = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
    let routes = BTreeMap::new();
    let run = async |state: &Arc<RunState>| {
        let recorded = state.snapshot().invocations;
        blocks
            .engine(
                leased.server.state(),
                Execution {
                    caller: &caller,
                    verifier: &verifier,
                    routes: &routes,
                    subject: GatewayCallContext::default(),
                    defer_unlanded_traces: false,
                    drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                    clock: BlockClock::default(),
                    execution: Some(Arc::clone(state)),
                    mode: ExecutionMode::Queued,
                },
            )
            .with_checkpoints(
                Arc::clone(state) as Arc<dyn Checkpoints>,
                &ExecutionState {
                    invocations: recorded,
                    ..ExecutionState::default()
                },
            )
            .run(
                Arc::clone(&spec),
                BTreeMap::new(),
                tokio::time::Instant::now() + std::time::Duration::from_secs(60),
            )
            .await
    };
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(60), run(&writer))
        .await
        .expect("the run completes while every revision is held")
        .expect("the run passes from memory");
    assert_eq!(outcome.verdict, VerificationVerdict::Passed, "{outcome:?}");
    assert!(
        settlements(&writer.snapshot()).contains_key("combine"),
        "the dependent ran from live settlements"
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), held.arrived.notified())
        .await
        .expect("a revision reaches Scribe");
    held.release.add_permits(1);
    tokio::time::timeout(std::time::Duration::from_secs(10), held.arrived.notified())
        .await
        .expect("the next revision reaches Scribe");
    let accepted = settlements(&leased.accepted().await);

    let restarted = leased.reclaim().await;
    assert_eq!(
        settlements(&restarted.snapshot()),
        accepted,
        "the restart restores exactly the accepted prefix"
    );
    let outcome = run(&restarted).await.expect("the reclaimed run completes");
    assert_eq!(outcome.verdict, VerificationVerdict::Passed, "{outcome:?}");
    restarted.persisted().await.expect("the restart persists");
    held.release
        .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
    let restored = settlements(&leased.accepted().await);
    for (step, settled) in &accepted {
        if settled.is_some() {
            assert_eq!(&restored[step], settled, "{step} is restored unchanged");
        }
    }
    assert!(
        ["left", "right", "combine"]
            .iter()
            .all(|step| restored.get(*step).is_some_and(Option::is_some)),
        "every step settles: {restored:?}"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

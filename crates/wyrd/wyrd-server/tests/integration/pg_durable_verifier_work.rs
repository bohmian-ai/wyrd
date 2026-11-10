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
use skald_runtime::{MockProvider, ProviderRegistry};
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
use wyrd_runtime::outbox::OutboxSink as _;
use wyrd_runtime::principal::PrincipalId;
use wyrd_server::components::auth::Caller;
use wyrd_server::components::gateway::GatewayCallContext;
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::scribe_outbox::{ScribeOutbox, ScribeSink, ScribeWrite};
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::authority::SystemReadAuthority;
use wyrd_server::verification::fault::PublicationFault;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::blocks::{Execution, HostedBlocks};
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_server::verification::hosted::publications::{Publications, PublishCall};
use wyrd_server::verification::hosted::sandbox::Sandbox;
use wyrd_server::verification::hosted::tools::HostedTools;
use wyrd_server::verification::hosted::workspace::{Workspace, Workspaces};
use wyrd_server::verification::inputs::InputResolver;
use wyrd_server::verification::leases::LeaseRenewal;
use wyrd_server::verification::telemetry::ExecutionMode;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::scheduler::GroupHandoff;
use wyrd_server::verification::state::{
    Checkpoint, Decision, RunState, StateError, StateRow, revision_batch, stand_in_identity,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::card::verifier::{Action, VerifierSpec};
use wyrd_spec::ids::{CardUid, ExecutionId, GroupId, SlotName};
use wyrd_spec::verification::execution::{
    InputDeclaration, InvocationSettlement, Producer, ProducerInvocation, PublicationCollection,
    PublicationContent, PublicationFile, PublicationKind, PublicationManifest, Speculation,
    StepStatus, VerificationOutcome, VerificationResult,
};
use wyrd_spec::verification::state::{EXECUTION_STATE_FORMAT, ExecutionState, InvocationRecord};
use wyrd_spec::verification::{PublicationName, TaskId};
use wyrd_spec::verification::{
    VerificationError, VerificationExecutionStatus, VerificationVerdict,
};
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, ExecutionLease, Settlement, TerminalDelivery, TerminalStatus,
    VerifierExecutionQueue,
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
    /// The lease renewal owner every writer's lease registers with.
    renewal: Arc<LeaseRenewal>,
    /// The tenant's SYSTEM principal, which reads the state back.
    system: PrincipalId,
    /// The first claim's state writer.
    run: Arc<RunState>,
    /// The claimed execution's Verifier.
    verifier: CardUid,
    /// The tenant's verification fixture, for submitting more work.
    fixture: VerificationFixture,
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
            std::time::Duration::from_secs(300),
        ));
        let outbox = Arc::clone(server.state().bifrost.scribe_outbox());
        let run = Self::writer(&server, outbox, &renewal, system, &claimed)
            .await
            .expect("the execution state claims");
        Self {
            server,
            lease: claimed.lease,
            renewal,
            system,
            run,
            verifier: claimed.verifier_card_uid,
            fixture,
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
    /// staging its revisions on `outbox`.
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
    ) -> Result<Arc<RunState>, StateError> {
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
            Some(Arc::new(lease)),
            &InputResolver::new(&query, None),
        )
        .await?;
        Ok(Arc::new(run))
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
    /// its state again, returning the new writer; the old one is stale from
    /// then on.
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
        Self::writer(&self.server, outbox, &self.renewal, self.system, &claimed).await
    }

    /// The execution's deadline on the database clock.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn deadline(&self) -> chrono::DateTime<chrono::Utc> {
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        sqlx::query_scalar(
            "SELECT deadline_at FROM wyrd.verifier_executions WHERE execution_id = $1",
        )
        .bind(self.lease.execution_id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("the deadline reads")
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
        publications
            .json(
                &call("c2", &producer, "assessment"),
                json!({"agrees": true}),
            )
            .await,
        publications
            .json(&call("c3", &producer, "nothing"), Value::Null)
            .await,
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
    assert_eq!(
        leased.read(&producer, "assessment").await,
        Some(PublicationContent::Json {
            value: json!({"agrees": true})
        })
    );
    assert_eq!(
        leased.read(&producer, "nothing").await,
        Some(PublicationContent::Json { value: Value::Null }),
        "an explicit null is a value, not an absence"
    );
    leased
        .server
        .flush_bifrost()
        .await
        .expect("the File inventory publishes");
    let (bytes, _) = TenantEvidence::new(
        leased.server.state().clone(),
        caller.clone(),
        BTreeMap::new(),
        limits,
    )
    .read(&InputDeclaration::File {
        sha256,
        size_bytes: report.len() as u64,
        format: MediaFormat::Markdown,
        filename: None,
    })
    .await
    .expect("the retained File reads");
    assert_eq!(bytes, report.as_bytes(), "scratch writes never reach it");

    let file = publications
        .path(&call("c4", &producer, "big"), &workspace, "/work/big.bin")
        .await
        .expect_err("an oversized file is refused");
    assert_eq!(rule(&file), "work_budget");
    let value = publications
        .json(&call("c5", &producer, "huge"), json!("x".repeat(64)))
        .await
        .expect_err("an oversized value is refused");
    assert_eq!(rule(&value), "publication_size");
    for refused in ["big", "huge"] {
        assert_eq!(leased.read(&producer, refused).await, None);
    }
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
                .await
                .expect("the call and its replay answer one receipt"),
            PublicationKind::Json
        );
        publications
            .path(&call("f1", &first, "notes"), &workspace, "/work/notes.md")
            .await
            .expect("the file call and its replay answer one receipt");
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
            .await
            .expect_err("another call or argument set conflicts");
        assert_eq!(rule(&error), "publication_name");
    }
    publications
        .json(&call("c3", &second, "report"), json!({"v": 3}))
        .await
        .expect("a fresh attempt publishes the name again");
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

    let current = leased.reclaim().await;
    let stale = publications
        .json(&call("c1", &first, "report"), json!({"v": 1}))
        .await
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
        .await
        .expect_err("a reclaimed lease publishes nothing");
    Publications::new(leased.server.state(), &caller, &current, limits)
        .json(&call("c5", &second, "late"), json!(null))
        .await
        .expect("the current lease publishes");
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
        .await
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

    let current = leased.reclaim().await;
    let resumed = Publications::new(leased.server.state(), &caller, &current, limits);
    resumed
        .json(&call("k", &producer, "k"), json!(1))
        .await
        .expect("the third publication fits");
    let count = resumed
        .json(&call("l", &producer, "l"), json!(1))
        .await
        .expect_err("a reclaim inherits the accepted count");
    assert_eq!(rule(&count), "publication_count");
    for refused in ["over", "tail", "l"] {
        assert_eq!(leased.read(&producer, refused).await, None, "{refused}");
    }

    let blind = Caller {
        // Not a SYSTEM principal, so every inventory query is unavailable.
        executed_by: Some(wyrd_runtime::principal::PrincipalId::new(
            uuid::Uuid::new_v4(),
        )),
        ..caller
    };
    let evidence = TenantEvidence::new(
        leased.server.state().clone(),
        blind,
        BTreeMap::new(),
        limits,
    )
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
        .await
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

    let evidence = TenantEvidence::new(
        leased.server.state().clone(),
        caller.clone(),
        BTreeMap::new(),
        limits,
    )
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
    let direct = TenantEvidence::new(
        leased.server.state().clone(),
        caller,
        BTreeMap::new(),
        limits,
    );
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
/// worker: its next invocation, publication, settlement, and renewal are all
/// stale, the accepted checkpoint and publication survive unchanged, and the
/// execution is never claimed again. A repeated cancel changes nothing. A
/// record of a newer worker format is not claimable by this worker even
/// once its lease expires.
///
/// # Panics
/// Panics when a stale write lands, a checkpoint or publication changes, or
/// a cancelled or newer-format execution is claimed.
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
        .await
        .expect("the live lease publishes");

    let mut tx = conn().await;
    sqlx::query(
        "UPDATE wyrd.verifier_executions SET worker_format = 3, \
                lease_expires_at = statement_timestamp() WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .execute(&mut **tx.transaction())
    .await
    .expect("the record moves to a newer format");
    let claimed = queue
        .claim(
            &mut tx,
            chrono::Duration::minutes(5),
            chrono::Duration::minutes(5),
            1,
        )
        .await
        .expect("claim runs");
    assert!(claimed.is_empty(), "an older worker claims a newer record");
    tx.rollback().await.expect("the format change rolls back");

    let mut tx = conn().await;
    assert!(queue.cancel(&mut tx, execution).await.expect("cancel runs"));
    tx.commit().await.expect("the cancel commits");
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
                TerminalDelivery::Acknowledged,
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
        .await
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
    model: &Arc<MockProvider>,
    sandbox: &Arc<Sandbox>,
    recorded: Vec<InvocationRecord>,
) -> VerificationEngine {
    let limits = saturable();
    let evidence = Arc::new(
        TenantEvidence::new(
            leased.server.state().clone(),
            caller.clone(),
            BTreeMap::new(),
            limits,
        )
        .with_execution(Arc::clone(run)),
    );
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::clone(model) as _);
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
    let sandbox = Arc::new(Sandbox::start(worker(), limits, limits.queued).await.expect("pool starts"));
    let none = serde_json::Map::new();
    let far = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    // One admitted operation holds the only worker admission while pending.
    let mut held = Box::pin(sandbox.run("def f():\n    return 1", "f", &none, Vec::new(), far));
    assert!(futures_util::FutureExt::now_or_never(&mut held).is_none());
    let model = Arc::new(MockProvider::new(ProviderName::OpenAi));
    let evidence = BTreeMap::from([(
        SlotName::new("x").expect("slot"),
        InputDeclaration::Json {
            value: json!("the notes"),
        },
    )]);
    let run = async |state: &Arc<RunState>, spec: &Arc<VerifierSpec>| {
        let recorded = state.snapshot().await.invocations;
        agent_engine(&leased, state, &caller, &model, &sandbox, recorded)
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
    let accepted = leased.accepted().await;
    assert_eq!(
        attempts(&accepted),
        [("draft".to_owned(), 1), ("once".to_owned(), 1)],
        "an attempt with retained work stays spent; one without is forgotten"
    );

    let current = leased.reclaim().await;
    model.push_response(publish("report", "attempt 2"));
    model.push_response(answer(Some("{}"), &[]));
    let outcome = run(&current, &draft).await.expect("the retry runs");
    let step = |outcome: &VerificationOutcome, id: &str| {
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
    let sandbox = Arc::new(Sandbox::start(worker(), limits, limits.queued).await.expect("pool starts"));
    let none = serde_json::Map::new();
    let far = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    // One admitted operation holds the only worker admission while pending.
    let mut held = Box::pin(sandbox.run("def f():\n    return 1", "f", &none, Vec::new(), far));
    assert!(futures_util::FutureExt::now_or_never(&mut held).is_none());
    let model = Arc::new(MockProvider::new(ProviderName::OpenAi));
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

    let interrupted = agent_engine(&leased, &leased.run, &caller, &model, &sandbox, Vec::new())
        .run(Arc::clone(&spec), notes(), far)
        .await;
    assert!(
        matches!(interrupted, Err(EngineError::Busy(_))),
        "{interrupted:?}"
    );
    assert_eq!(model.remaining(), 0);

    let current = leased.reclaim().await;
    let recovered = current.snapshot().await;
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
    let evidence = TenantEvidence::new(
        leased.server.state().clone(),
        blind.clone(),
        BTreeMap::new(),
        limits,
    )
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
        &model,
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
    assert_eq!(
        leased.revision().await.0,
        revision,
        "the retry accepts nothing"
    );
    assert_eq!(
        leased.read(&producer, "report").await,
        Some(accepted.content),
        "the accepted entry is unchanged"
    );
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

/// Dropping a running Agent step, as a disconnecting direct caller drops
/// its request, starts no later Agent work. With every state revision held
/// at Scribe, the drafter's attempt is admitted, its first model turn
/// publishes `early`, and the run is dropped while that publication waits
/// for acceptance. Once Scribe is released, the model has still been asked
/// for nothing more and `after` is never published.
///
/// # Panics
/// Panics when the run finishes while held, a revision does not reach
/// Scribe, or the dropped run calls the model or publishes again.
#[tokio::test(flavor = "multi_thread")]
async fn dropped_agent_work_makes_no_later_call() {
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
    let limits = saturable();
    let caller = leased.publisher([Permission::tool_invoke_any()]);
    let sandbox = Arc::new(Sandbox::start(worker(), limits, limits.queued).await.expect("pool starts"));
    let model = Arc::new(MockProvider::new(ProviderName::OpenAi));
    for name in ["early", "after"] {
        model.push_response(answer(
            None,
            &[("publish", json!({ "name": name, "value": 1 }))],
        ));
    }
    model.push_response(answer(Some("{}"), &[]));
    let far = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let engine = agent_engine(&leased, &first, &caller, &model, &sandbox, Vec::new());
    let mut run = Box::pin(engine.run(publishing_step("draft", 0), notes(), far));

    for (revision, asked, release) in [("the attempt", 3, 1), ("the early publication", 2, 0)] {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::select! {
                outcome = &mut run => panic!("returned while held: {outcome:?}"),
                () = held.arrived.notified() => {}
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{revision} reaches Scribe"));
        assert_eq!(model.remaining(), asked, "{revision} waits on Scribe");
        held.release.add_permits(release);
    }
    drop(run);
    held.release
        .add_permits(tokio::sync::Semaphore::MAX_PERMITS);

    assert_eq!(
        model.remaining(),
        2,
        "a dropped run makes no later model call"
    );
    let current = leased.reclaim().await;
    assert!(
        current
            .publications(&TaskId::new("draft").expect("task"))
            .await
            .iter()
            .all(|accepted| accepted.publication.name.as_str() != "after"),
        "a dropped run publishes nothing later"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A verdict-only outcome with no requirement or step.
fn outcome(verdict: VerificationVerdict) -> VerificationOutcome {
    VerificationOutcome {
        verdict,
        requirements: Vec::new(),
        steps: Vec::new(),
        evidence: BTreeMap::new(),
    }
}

/// A reclaim resumes invocation attempts from the accepted Bifrost state
/// under the execution's original deadline.
///
/// The first claim begins two attempts and abandons a third before it ran.
/// The reclaim recovers exactly the two begun attempts, both still spent, so
/// a retry of one starts its next attempt; the abandoned attempt is gone, the
/// database deadline is unchanged, and PostgreSQL holds no invocation copy.
///
/// # Panics
/// Panics when a revision is refused, the reclaim recovers other attempts,
/// the deadline moves, or a PostgreSQL invocation table exists.
#[tokio::test(flavor = "multi_thread")]
async fn invocations_resume_from_bifrost_with_original_bounds() {
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
    let deadline = leased.deadline().await;

    let current = leased.reclaim().await;
    assert_eq!(
        attempts(&current.snapshot().await),
        [("review".to_owned(), 1), ("judge".to_owned(), 1)]
    );
    current
        .begin(&Producer::plain(&task("judge"), 2))
        .await
        .expect("the interrupted attempt retries as its next attempt");
    assert_eq!(
        attempts(&leased.accepted().await),
        [
            ("review".to_owned(), 1),
            ("judge".to_owned(), 1),
            ("judge".to_owned(), 2)
        ]
    );
    assert_eq!(
        leased.deadline().await,
        deadline,
        "a reclaim keeps the deadline"
    );
    let mut conn = leased
        .server
        .tenant_conn_for(leased.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let legacy: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('wyrd.verifier_execution_invocations')::text")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("the catalog reads");
    assert_eq!(legacy, None, "no PostgreSQL invocation copy exists");
    drop(conn);
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
        .await
        .expect("the value publishes");
    let accepted = leased.run.publications(&task).await;

    let current = leased.reclaim().await;
    assert_eq!(
        current.publications(&task).await,
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
        .await
        .expect_err("the replaced writer publishes nothing");
    assert_eq!(
        leased.entries().await,
        [
            ("review".to_owned(), 1, "report".to_owned()),
            ("review".to_owned(), 1, "verdict".to_owned()),
        ]
    );
    let evidence = TenantEvidence::new(
        leased.server.state().clone(),
        caller,
        BTreeMap::new(),
        limits,
    )
    .with_execution(Arc::clone(&current));
    let recovered = evidence
        .publications(&task)
        .await
        .expect("publications read");
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
        .await
        .expect("the value publishes");
    let selected = leased
        .run
        .select(&outcome(VerificationVerdict::Failed), chrono::Utc::now())
        .await
        .expect("the outcome is selected");

    let current = leased.reclaim().await;
    assert_eq!(
        current.snapshot().await.terminal.map(Decision::Selected),
        Some(selected.clone())
    );
    assert_eq!(
        current
            .select(&outcome(VerificationVerdict::Passed), chrono::Utc::now())
            .await
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

/// `execution`'s accepted state, read back from Bifrost as `system`.
///
/// # Panics
/// Panics as [`accepted`] does.
async fn retained(
    server: &WyrdTestServer,
    system: PrincipalId,
    execution: ExecutionId,
) -> ExecutionState {
    accepted(server, system, execution).await.1
}

/// `execution`'s accepted revision and state, read back from Bifrost as
/// `system`; revision 0 when none was accepted.
///
/// # Panics
/// Panics when the state is unreadable.
async fn accepted(
    server: &WyrdTestServer,
    system: PrincipalId,
    execution: ExecutionId,
) -> (i64, ExecutionState) {
    let query = Leased::query(server, system).await;
    RunState::accepted(
        server.state(),
        server.data_tenant_id(),
        execution,
        &InputResolver::new(&query, None),
    )
    .await
    .expect("the state reads")
}

/// The tenant's executions still owing a terminal delivery, with when each
/// is due.
///
/// # Panics
/// Panics when the read fails.
async fn owed(server: &WyrdTestServer) -> BTreeMap<ExecutionId, chrono::DateTime<chrono::Utc>> {
    sqlx::query_as(
        "SELECT execution_id, delivery_due_at FROM wyrd.verifier_executions \
         WHERE delivery_due_at IS NOT NULL",
    )
    .fetch_all(&superuser(server))
    .await
    .expect("the owed deliveries read")
    .into_iter()
    .map(|(id, due): (uuid::Uuid, _)| (ExecutionId::new(id).expect("a stored execution ID"), due))
    .collect()
}

/// The server's superuser pool, for assertions and fault injection.
///
/// # Panics
/// Panics when the fixture exposes no superuser pool.
fn superuser(server: &WyrdTestServer) -> sqlx::PgPool {
    server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool")
}

/// Run `server`'s verification runtime until `done` holds of the owed
/// deliveries, then stop it.
///
/// # Panics
/// Panics when the runtime does not compose or stop, or `done` never holds
/// within a minute.
async fn deliver_until(
    server: &WyrdTestServer,
    done: impl Fn(&BTreeMap<ExecutionId, chrono::DateTime<chrono::Utc>>) -> bool,
) {
    let (stop, task) = start_runtime(server);
    owed_until(server, done).await;
    stop.cancel();
    task.await.expect("the runtime stops");
}

/// Wait until `done` holds of `server`'s owed deliveries, while a runtime
/// the caller started delivers them.
///
/// # Panics
/// Panics when `done` never holds within a minute.
async fn owed_until(
    server: &WyrdTestServer,
    done: impl Fn(&BTreeMap<ExecutionId, chrono::DateTime<chrono::Utc>>) -> bool,
) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    loop {
        let owed = owed(server).await;
        if done(&owed) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the runtime never reached the expected deliveries; owed {owed:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Spawn `server`'s verification runtime; returns its stop token and task.
///
/// # Panics
/// Panics when the runtime does not compose.
fn start_runtime(server: &WyrdTestServer) -> (CancellationToken, tokio::task::JoinHandle<()>) {
    let runtime = VerificationRuntime::builder(server.state())
        .build()
        .expect("the runtime composes");
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    (stop, task)
}

/// A started server with a provisioned tenant, its SYSTEM principal, and
/// one subject Service and Drift Verifier.
///
/// # Panics
/// Panics when the server or fixture fails.
async fn seeded() -> (
    WyrdTestServer,
    VerificationFixture,
    PrincipalId,
    CardUid,
    CardUid,
) {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions");
    let (subject, _) = fixture.service("subject").await.expect("service");
    let verifier = fixture.drift_verifier("delivered").await.expect("verifier");
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let system = system_principal_id(&mut conn)
        .await
        .expect("SYSTEM reads")
        .map(PrincipalId::new)
        .expect("the tenant has SYSTEM");
    conn.commit().await.expect("the read commits");
    (server, fixture, system, subject, verifier)
}

/// A user cancelling an execution that recorded no checkpoint retains the
/// cancellation in its state once the runtime delivers the decision the
/// cancel left owed: once its queue row and both Cards are gone, a
/// reader of its Verifier still reads it cancelled with the queue's error,
/// and a caller without the grant is refused.
///
/// # Panics
/// Panics when the cancel is refused, the state does not hold the
/// cancellation, or a read differs.
#[tokio::test(flavor = "multi_thread")]
async fn cancellation_reads_back_from_state_after_the_queue_and_cards_are_gone() {
    let leased = Leased::start().await;
    let execution = leased.lease.execution_id;
    let editor = jwt(&leased.server, "canceller", &["editor"]).await;
    let (status, body) = respond(
        &leased.server,
        &editor,
        "POST",
        &format!("{execution}/cancel"),
    )
    .await;
    assert_eq!(
        (status, body["status"].clone()),
        (axum::http::StatusCode::OK, json!("cancelled")),
        "{body}"
    );
    assert!(owed(&leased.server).await.contains_key(&execution));
    deliver_until(&leased.server, BTreeMap::is_empty).await;
    let failure = leased
        .accepted()
        .await
        .failure
        .expect("the state holds the cancellation");
    assert_eq!(
        (failure.status, failure.error.code.as_str()),
        (VerificationExecutionStatus::Cancelled, "cancelled")
    );

    leased.forget_queue_and_cards().await;
    let viewer = jwt(&leased.server, "cancel-reader", &["viewer"]).await;
    let (status, body) = respond(&leased.server, &viewer, "GET", &execution.to_string()).await;
    assert_eq!(
        (
            status,
            body["status"].clone(),
            body["error"]["code"].clone(),
            body["result"].clone()
        ),
        (
            axum::http::StatusCode::OK,
            json!("cancelled"),
            json!("cancelled"),
            Value::Null
        ),
        "{body}"
    );
    let outsider = jwt(&leased.server, "cancel-outsider", &[]).await;
    let (status, body) = respond(&leased.server, &outsider, "GET", &execution.to_string()).await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "{body}");
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// The table every execution state revision is written to.
const EVENTS: &str = "bifrost.verification.execution_events";

/// Which of a cancelled execution's two competing state writers Scribe
/// admits first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstWriter {
    /// The worker's checkpoint, confirmed before the cancel.
    Checkpoint,
    /// The server's terminal delivery of the cancellation.
    Delivery,
}

/// A cancellation racing a worker's checkpoint against the server's
/// terminal delivery for the same execution stays retained in both arrival
/// orders.
///
/// The worker confirms its live lease and freezes a checkpoint built on the
/// accepted revision, which is held before Scribe; a user then cancels, and
/// the runtime's delivery freezes the cancellation on that same revision and
/// is held too. The delivery stays owed while unacknowledged. Releasing one
/// writer and then the other forces each arrival order: the losing write is
/// refused on its own, a refused delivery is delivered again on the newer
/// revision, and a refused checkpoint is never accepted. Either way the
/// state reads back cancelled with no result, the accepted report remains
/// publicly listed, the worker records nothing more, and an unrelated
/// execution staged in the held delivery's outbox lane while it is held
/// completes with its result retained and no delivery owed.
///
/// # Panics
/// Panics when a delivery retires before its acknowledgement or stays owed,
/// the cancellation or accepted report is lost, a refused checkpoint is
/// accepted, the cancelled worker records more work, a read returns a
/// result, or the unrelated execution never settles.
#[tokio::test(flavor = "multi_thread")]
async fn cancellation_writer_race_preserves_terminal_delivery() {
    for first in [FirstWriter::Checkpoint, FirstWriter::Delivery] {
        cancellation_race(first).await;
    }
}

/// The Scribe outbox lane every write of `execution` is staged in.
///
/// # Panics
/// Panics when the probe row does not build.
fn outbox_lane(execution: ExecutionId, verifier: &CardUid) -> u64 {
    let now = chrono::Utc::now();
    let row = ResultPayloadBuilder::new(
        ResultRun {
            execution_id: execution,
            run_id: None,
            verifier_version: "",
            card_uid: verifier,
            owner_card_uid: None,
            group_id: None,
            deployment_environment: None,
            binding_id: None,
            trigger: None,
            window: None,
        },
        verifier,
        now,
        now,
    )
    .checkpoint(1, false, &json!({}))
    .expect("the probe row builds");
    ScribeSink::lane(&ScribeWrite::ExecutionInputs {
        row,
        attribution: Attribution::unresolved(),
        queued: None,
    }) % ScribeSink::LANES as u64
}

/// One forced arrival order of
/// [`cancellation_writer_race_preserves_terminal_delivery`].
///
/// # Panics
/// Panics as that test does, naming `first`.
async fn cancellation_race(first: FirstWriter) {
    let leased = Leased::start().await;
    let state = leased.server.state();
    let tenant = leased.server.data_tenant_id();
    let execution = leased.lease.execution_id;
    let review = TaskId::new("review").expect("task");
    let producer = Producer::plain(&review, 1);
    leased
        .run
        .begin(&producer)
        .await
        .expect("the checkpoint is accepted");
    Publications::new(
        state,
        &leased.publisher([]),
        &leased.run,
        HostedLimits::default(),
    )
    .json(&call("c1", &producer, "report"), json!({"v": 1}))
    .await
    .expect("the live lease publishes");

    let held = Arc::new(HeldRevisions {
        scribe: Arc::clone(
            state
                .bifrost_ingest()
                .expect("the server runs Scribe")
                .scribe(),
        ) as _,
        release: tokio::sync::Semaphore::new(0),
        arrived: tokio::sync::Notify::new(),
    });
    let worker = leased
        .reclaim_on(ScribeSink::local_outbox(
            Arc::clone(&held) as _,
            Some(state.postgres.wyrd().clone()),
        ))
        .await
        .expect("the execution state claims");
    let followup = Producer::plain(&TaskId::new("followup").expect("task"), 1);
    let mut checkpoint = Box::pin(worker.begin(&followup));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::select! {
            started = &mut checkpoint => panic!("{first:?}: returned while held: {started:?}"),
            () = held.arrived.notified() => {}
        }
    })
    .await
    .expect("the checkpoint reaches Scribe");

    let mut tx = leased
        .server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    assert!(
        VerifierExecutionQueue::default()
            .cancel(&mut tx, execution)
            .await
            .expect("cancel runs")
    );
    tx.commit().await.expect("the cancel commits");

    let fault = PublicationFault::default();
    let delivery = fault.hold_next(EVENTS);
    let runtime = VerificationRuntime::builder(state)
        .publication_fault(fault)
        .build()
        .expect("the runtime composes");
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    tokio::time::timeout(std::time::Duration::from_mins(1), delivery.arrived())
        .await
        .expect("the delivery reaches Scribe");
    assert!(
        owed(&leased.server).await.contains_key(&execution),
        "{first:?}: an unacknowledged delivery stays owed"
    );
    let (bystander_subject, _) = leased.fixture.service("bystander").await.expect("service");
    let lane = outbox_lane(execution, &leased.verifier);
    let mut bystander = None;
    for _ in 0..64 {
        let submitted = leased
            .fixture
            .submit(&leased.verifier, &bystander_subject, BTreeMap::new())
            .await
            .expect("the unrelated execution is admitted");
        if outbox_lane(submitted, &leased.verifier) == lane {
            bystander = Some(submitted);
            break;
        }
    }
    let bystander = bystander.expect("an unrelated execution shares the held delivery's lane");

    let checkpointed = match first {
        FirstWriter::Checkpoint => {
            held.release
                .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
            let checkpointed = checkpoint.await;
            delivery.release();
            checkpointed
        }
        FirstWriter::Delivery => {
            delivery.release();
            owed_until(&leased.server, |owed| !owed.contains_key(&execution)).await;
            held.release
                .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
            checkpoint.await
        }
    };
    assert_eq!(
        checkpointed.is_ok(),
        first == FirstWriter::Checkpoint,
        "{first:?}: only a checkpoint admitted first is accepted: {checkpointed:?}"
    );
    owed_until(&leased.server, BTreeMap::is_empty).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    while ["queued", "running"].contains(
        &leased
            .fixture
            .execution(bystander)
            .await
            .expect("the unrelated execution reads")
            .status
            .as_str(),
    ) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{first:?}: the unrelated execution never settles"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    owed_until(&leased.server, BTreeMap::is_empty).await;
    stop.cancel();
    task.await.expect("the runtime stops");
    let settled = leased
        .fixture
        .execution(bystander)
        .await
        .expect("the unrelated execution reads");
    assert_eq!(
        (
            settled.status.as_str(),
            retained(&leased.server, leased.system, bystander)
                .await
                .terminal
                .is_some()
        ),
        ("completed", true),
        "{first:?}: the unrelated execution completes and its result is retained"
    );

    let retained = leased.accepted().await;
    let failure = retained
        .failure
        .clone()
        .expect("the state holds the cancellation");
    let mut expected = vec![("review".to_owned(), 1)];
    if first == FirstWriter::Checkpoint {
        expected.push(("followup".to_owned(), 1));
    }
    assert_eq!(
        (
            failure.status,
            failure.error.code.as_str(),
            retained.terminal.is_none(),
            attempts(&retained),
        ),
        (
            VerificationExecutionStatus::Cancelled,
            "cancelled",
            true,
            expected
        ),
        "{first:?}"
    );
    assert_eq!(
        leased.entries().await,
        [("review".to_owned(), 1, "report".to_owned())],
        "{first:?}: the accepted report remains"
    );
    worker
        .begin(&Producer::plain(&TaskId::new("resumed").expect("task"), 1))
        .await
        .expect_err("the cancelled worker records nothing more");
    let viewer = jwt(&leased.server, "race-reader", &["viewer"]).await;
    let (status, body) = respond(&leased.server, &viewer, "GET", &execution.to_string()).await;
    assert_eq!(
        (
            status,
            body["status"].clone(),
            body["error"]["code"].clone(),
            body["result"].clone()
        ),
        (
            axum::http::StatusCode::OK,
            json!("cancelled"),
            json!("cancelled"),
            Value::Null
        ),
        "{first:?}: {body}"
    );
    let (status, body) = respond(
        &leased.server,
        &viewer,
        "GET",
        &format!("{execution}/publications"),
    )
    .await;
    assert_eq!(
        (
            status,
            body["items"].as_array().map(|items| items
                .iter()
                .map(|item| (item["name"].clone(), item["producer"]["step_id"].clone()))
                .collect::<Vec<_>>())
        ),
        (
            axum::http::StatusCode::OK,
            Some(vec![(json!("report"), json!("review"))])
        ),
        "{first:?}: the accepted report stays publicly inspectable: {body}"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A user cancelling a grouped execution before any claim retains the
/// execution's group and its absent owner with its exact Verifier and
/// subject: the cancellation leaves its delivery owed, the runtime records
/// it, and the retained read answers it once the queue row and Cards are
/// gone. A caller without the grant can neither cancel nor read it.
///
/// # Panics
/// Panics when the cancel is refused, a delivery stays owed, the retained
/// identity differs, or a caller without the grant is not refused.
#[tokio::test(flavor = "multi_thread")]
async fn preclaim_group_cancellation_preserves_retained_identity() {
    let (server, fixture, system, subject, verifier) = seeded().await;
    let execution = fixture
        .submit(&verifier, &subject, BTreeMap::new())
        .await
        .expect("the execution is admitted");
    sqlx::query(
        "UPDATE wyrd.verifier_executions SET group_id = 'session-7' WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .execute(&superuser(&server))
    .await
    .expect("the execution is grouped");

    let outsider = jwt(&server, "group-outsider", &[]).await;
    let (status, body) = respond(&server, &outsider, "POST", &format!("{execution}/cancel")).await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "{body}");
    let editor = jwt(&server, "group-canceller", &["editor"]).await;
    let (status, body) = respond(&server, &editor, "POST", &format!("{execution}/cancel")).await;
    assert_eq!(
        (status, body["status"].clone()),
        (axum::http::StatusCode::OK, json!("cancelled")),
        "{body}"
    );
    assert_eq!(
        owed(&server).await.into_keys().collect::<Vec<_>>(),
        [execution]
    );
    deliver_until(&server, BTreeMap::is_empty).await;

    let state = retained(&server, system, execution).await;
    let failure = state.failure.expect("the state holds the cancellation");
    assert_eq!(
        (failure.status, failure.error.code.as_str()),
        (VerificationExecutionStatus::Cancelled, "cancelled")
    );
    let identity = state.identity.expect("the state holds the identity");
    assert_eq!(
        serde_json::to_value(&identity.group_id).expect("the group encodes"),
        json!("session-7")
    );
    assert_eq!(identity.owner_card_uid, None, "a submission names no owner");
    assert_eq!(identity.verifier.uid.as_ref(), Some(&verifier));
    assert_eq!(identity.subject.uid.as_ref(), Some(&subject));

    forget(&server, &[execution]).await;
    let viewer = jwt(&server, "group-reader", &["viewer"]).await;
    let (status, body) = respond(&server, &viewer, "GET", &execution.to_string()).await;
    assert_eq!(
        (
            status,
            body["status"].clone(),
            body["error"]["code"].clone()
        ),
        (
            axum::http::StatusCode::OK,
            json!("cancelled"),
            json!("cancelled")
        ),
        "{body}"
    );
    let (status, body) = respond(&server, &outsider, "GET", &execution.to_string()).await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "{body}");
    server.shutdown().await.expect("test server shuts down");
}

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
        .await
        .expect("the failure is accepted");
    leased
        .run
        .fail(
            VerificationExecutionStatus::TimedOut,
            &VerificationError {
                code: "later".to_owned(),
                message: "a later failure".to_owned(),
            },
        )
        .await
        .expect("a repeated failure is accepted");
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
            TerminalDelivery::Acknowledged,
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

/// Direct and queued verification both select their outcome through the
/// same Bifrost execution state.
///
/// Two direct executions of one assertion Verifier get distinct identities,
/// admit no queue record, and each leaves an accepted state whose selected
/// outcome is the one returned. A queued submission of the same request run
/// by the runtime completes with the result its accepted state selected.
///
/// # Panics
/// Panics when provisioning or a request fails, a direct execution shares an
/// identity, admits a queue record, or returns an outcome its state did not
/// select, or the queued result differs from its selected one.
#[tokio::test(flavor = "multi_thread")]
async fn direct_and_queued_use_bifrost_run_state() {
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
    for result in &results {
        let state = fixture
            .execution_state(server.state(), result.execution_id)
            .await
            .expect("the state reads")
            .expect("a direct execution claims its state");
        assert_eq!(
            state.terminal.map(|selected| selected.outcome),
            Some(result.outcome.clone()),
            "the direct result is the outcome its state selected"
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

/// Whether polling `future` once leaves it pending.
async fn pending<F: Future + Unpin>(future: &mut F) -> bool {
    tokio::time::timeout(std::time::Duration::ZERO, future)
        .await
        .is_err()
}

/// A reclaim recovers exactly the batches Scribe accepted while independent
/// steps recorded their progress concurrently.
///
/// The factual check's caller confirms its lease and freezes its revision,
/// which is held while the sensitive check records its start in memory, and
/// the sensitive waiter is cancelled before any revision holds it.
/// Once the factual revision is accepted, the accepted state holds only the
/// factual check, and the reclaim resumes from it without the sensitive
/// check. The replaced writer then freezes the sensitive check and the
/// dependent combination into one revision, which is refused, so neither
/// becomes accepted. The new writer records both siblings concurrently, and
/// the accepted state then holds all three.
///
/// # Panics
/// Panics when a step returns before its revision is accepted, the recovered
/// state holds an unaccepted change, or the replaced writer is accepted.
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

    let factual_attempt = Producer::plain(&factual, 1);
    let mut factual_started = Box::pin(first.begin(&factual_attempt));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::select! {
            started = &mut factual_started => panic!("returned while held: {started:?}"),
            () = held.arrived.notified() => {}
        }
    })
    .await
    .expect("the factual revision reaches Scribe");
    let sensitive_attempt = Producer::plain(&sensitive, 1);
    let mut sensitive_started = Box::pin(first.begin(&sensitive_attempt));
    assert!(
        pending(&mut sensitive_started).await,
        "the held revision does not hold the sensitive check"
    );
    drop(sensitive_started);
    assert_eq!(attempts(&first.snapshot().await).len(), 2);
    held.release
        .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
    factual_started
        .await
        .expect("the factual revision is accepted");
    assert_eq!(
        attempts(&leased.accepted().await),
        [("factual".to_owned(), 1)],
        "the accepted revision holds only the factual check"
    );

    let current = leased.reclaim().await;
    assert_eq!(
        attempts(&current.snapshot().await),
        [("factual".to_owned(), 1)]
    );
    assert!(
        first.begin(&Producer::plain(&combine, 1)).await.is_err(),
        "the replaced writer's batch is refused"
    );
    assert_eq!(
        attempts(&leased.accepted().await),
        [("factual".to_owned(), 1)]
    );

    let (sensitive_attempt, combine_attempt) =
        (Producer::plain(&sensitive, 1), Producer::plain(&combine, 1));
    let (sensitive_started, combine_started) = tokio::join!(
        current.begin(&sensitive_attempt),
        current.begin(&combine_attempt)
    );
    sensitive_started.expect("the sensitive check is accepted");
    combine_started.expect("the combination is accepted");
    assert_eq!(
        attempts(&leased.accepted().await),
        [
            ("factual".to_owned(), 1),
            ("sensitive".to_owned(), 1),
            ("combine".to_owned(), 1)
        ]
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// `method` on `path` as `jwt` with the JSON `body`, returning the status
/// and JSON answer.
///
/// # Panics
/// Panics when the request does not respond or its body is not JSON.
async fn send_json(
    server: &WyrdTestServer,
    jwt: &str,
    path: &str,
    body: &Value,
) -> (axum::http::StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            axum::http::Request::builder()
                .method("POST")
                .uri(path)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("the request responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Enqueue `verifier` about `subject` as `jwt` through the public route and
/// wait until the queue writer has committed it.
///
/// # Panics
/// Panics when the enqueue is not accepted or never commits.
async fn enqueue(
    server: &WyrdTestServer,
    fixture: &VerificationFixture,
    jwt: &str,
    verifier: &CardUid,
    subject: &CardUid,
) -> ExecutionId {
    let (status, body) = send_json(
        server,
        jwt,
        "/v1/verification/enqueue",
        &json!({"verifier_card_uid": verifier, "card_uid": subject, "inputs": {}}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::ACCEPTED, "{body}");
    let execution: ExecutionId =
        serde_json::from_value(body["execution_id"].clone()).expect("an execution ID");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    while fixture.execution(execution).await.is_err() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the enqueue never commits"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    execution
}

/// Every non-attempt terminal producer leaves its decision owed across every
/// commit boundary, and a restarted runtime records it through the one
/// terminal operation without computing.
///
/// The producers are a user's cancel, a principal's revocation, an admission
/// fenced by that revocation, an admission refused before any target load,
/// and a group activation refused for its lapsed owner; each commits with no
/// runtime and no checkpoint. The revocation fences a live lease while the
/// execution state is unreadable, so its control commit alone stops the
/// computation. A delivery claim that dies, unreadable state, and a missing
/// SYSTEM principal each leave every delivery owed until restored; a
/// delivery whose retiring commit fails keeps its acknowledged decision
/// owed until a later claim, once the failed claim's lease is expired,
/// retires it without writing again. Each then
/// reads back with its status, error, and identity, also once its queue row
/// and Cards are gone.
///
/// # Panics
/// Panics when a producer commits other than owed, a fenced settlement
/// commits, a delivery retires while its dependency is down or before its
/// decision is acknowledged, a decision is written twice, or a retained read
/// differs.
#[tokio::test(flavor = "multi_thread")]
async fn terminal_delivery_survives_every_commit_boundary() {
    let (server, fixture, system, subject, verifier) = seeded().await;
    let pool = superuser(&server);
    let exec = |statement: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("the fault statement applies");
        }
    };
    let system_kind = |kind: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE wyrd.auth_service_accounts SET principal_kind = $2 WHERE id = $1")
                .bind(system.as_uuid())
                .bind(kind)
                .execute(&pool)
                .await
                .expect("the SYSTEM principal changes");
        }
    };
    let wyrd_testing::Bootstrap::User {
        id: submitter,
        jwt: submitting,
    } = server
        .bootstrap_user("delivery-submitter", &["editor"])
        .await
        .expect("user bootstraps")
    else {
        panic!("a user bootstraps as a user");
    };
    let refusing = jwt(&server, "delivery-refused", &["editor"]).await;
    let admin = jwt(&server, "delivery-admin", &["admin"]).await;
    let queue = VerifierExecutionQueue::default();

    let cancelled = enqueue(&server, &fixture, &submitting, &verifier, &subject).await;
    let (status, body) =
        respond(&server, &submitting, "POST", &format!("{cancelled}/cancel")).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");

    let revoked = enqueue(&server, &fixture, &submitting, &verifier, &subject).await;
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let live = queue
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
    conn.commit().await.expect("the claim commits");
    assert_eq!(live.lease.execution_id, revoked);
    exec("REVOKE SELECT ON vala.scribe_batch_commits FROM wyrd_tenant").await;
    let (status, body) = send_json(
        &server,
        &admin,
        &format!("/v1/principals/{submitter}/revoke"),
        &json!({"principal_kind": "user", "reason": "delivery test"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let engine = VerificationError {
        code: "engine_crashed".to_owned(),
        message: "the engine crashed".to_owned(),
    };
    assert_eq!(
        queue
            .terminate(
                &mut conn,
                live.lease,
                TerminalStatus::Errored,
                &engine,
                TerminalDelivery::Acknowledged,
            )
            .await
            .expect("terminate runs"),
        Settlement::StaleLease,
        "the revocation fenced the live computation"
    );
    conn.commit().await.expect("the fenced settlement commits");
    exec("GRANT SELECT ON vala.scribe_batch_commits TO wyrd_tenant").await;

    let fenced = enqueue(&server, &fixture, &submitting, &verifier, &subject).await;
    let refused = enqueue(&server, &fixture, &refusing, &subject, &subject).await;
    let refusal = fixture
        .execution(refused)
        .await
        .expect("execution reads")
        .error_code
        .expect("the refusal records its error");
    let cases = [
        (
            cancelled,
            VerificationExecutionStatus::Cancelled,
            "cancelled",
            &verifier,
        ),
        (
            revoked,
            VerificationExecutionStatus::Cancelled,
            "principal_revoked",
            &verifier,
        ),
        (
            fenced,
            VerificationExecutionStatus::Cancelled,
            "principal_revoked",
            &verifier,
        ),
        (
            refused,
            VerificationExecutionStatus::Errored,
            refusal.as_str(),
            &subject,
        ),
    ];
    for (execution, status, code, _) in &cases {
        let row = fixture
            .execution(*execution)
            .await
            .expect("execution reads");
        assert_eq!(
            (row.status.as_str(), row.error_code.as_deref()),
            (<&str>::from(*status), Some(*code)),
            "{execution} commits its decision"
        );
    }
    let all: BTreeSet<ExecutionId> = cases.iter().map(|case| case.0).collect();
    let owed_at = owed(&server).await;
    assert_eq!(
        owed_at.keys().copied().collect::<BTreeSet<_>>(),
        all,
        "every decision is owed"
    );

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let abandoned = queue
        .claim_deliveries(&mut conn, chrono::Duration::zero(), 10)
        .await
        .expect("the deliveries claim");
    conn.commit().await.expect("the delivery claim commits");
    assert_eq!(
        abandoned.len(),
        all.len(),
        "a dead delivery worker claimed every one"
    );

    let reclaimed = |before: BTreeMap<ExecutionId, chrono::DateTime<chrono::Utc>>| {
        move |owed: &BTreeMap<ExecutionId, chrono::DateTime<chrono::Utc>>| {
            owed.len() == before.len()
                && owed
                    .iter()
                    .all(|(id, due)| before.get(id).is_some_and(|was| was < due))
        }
    };
    exec("REVOKE SELECT ON vala.scribe_batch_commits FROM wyrd_tenant").await;
    deliver_until(&server, reclaimed(owed(&server).await)).await;
    exec("GRANT SELECT ON vala.scribe_batch_commits TO wyrd_tenant").await;
    system_kind("service").await;
    deliver_until(&server, reclaimed(owed(&server).await)).await;
    system_kind("system").await;
    for execution in &all {
        assert_eq!(
            accepted(&server, system, *execution).await.0,
            0,
            "{execution} records nothing while its dependency is down"
        );
    }

    for statement in [
        "CREATE FUNCTION public.fail_retirement() RETURNS trigger LANGUAGE plpgsql \
         AS $$ BEGIN RAISE EXCEPTION 'injected retirement failure'; END $$"
            .to_owned(),
        format!(
            "CREATE TRIGGER fail_retirement BEFORE UPDATE ON wyrd.verifier_executions \
             FOR EACH ROW WHEN (OLD.delivery_due_at IS NOT NULL \
             AND NEW.delivery_due_at IS NULL AND NEW.execution_id = '{}'::uuid) \
             EXECUTE FUNCTION public.fail_retirement()",
            cancelled.as_uuid()
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .expect("the retirement fault installs");
    }
    let (stop, task) = start_runtime(&server);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    while owed(&server).await.into_keys().ne([cancelled])
        || accepted(&server, system, cancelled).await.0 == 0
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the other deliveries never retired; owed {:?}",
            owed(&server).await
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    stop.cancel();
    task.await.expect("the runtime stops");
    assert!(
        owed(&server).await.contains_key(&cancelled),
        "an acknowledged decision stays owed while its retirement fails"
    );
    exec("DROP TRIGGER fail_retirement ON wyrd.verifier_executions").await;
    exec(
        "UPDATE wyrd.verifier_executions SET delivery_due_at = statement_timestamp() \
         WHERE delivery_due_at IS NOT NULL",
    )
    .await;
    deliver_until(&server, BTreeMap::is_empty).await;

    for (execution, status, code, verifier) in &cases {
        let (revision, state) = accepted(&server, system, *execution).await;
        assert_eq!(revision, 1, "{execution} records its decision once");
        let failure = state.failure.expect("the state holds the decision");
        assert_eq!(
            (failure.status, failure.error.code.as_str()),
            (*status, *code)
        );
        assert!(state.terminal.is_none(), "nothing is selected");
        let identity = state.identity.expect("the state holds the identity");
        assert_eq!(
            (
                identity.verifier.uid.as_ref(),
                identity.subject.uid.as_ref()
            ),
            (Some(*verifier), Some(&subject))
        );
    }
    forget(&server, &all.iter().copied().collect::<Vec<_>>()).await;
    let viewer = jwt(&server, "delivery-reader", &["viewer"]).await;
    for (execution, status, code, _) in &cases {
        let (http, body) = respond(&server, &viewer, "GET", &execution.to_string()).await;
        assert_eq!(
            (http, body["status"].clone(), body["error"]["code"].clone()),
            (axum::http::StatusCode::OK, json!(status), json!(code)),
            "{body}"
        );
    }
    server.shutdown().await.expect("test server shuts down");

    let support = crate::pg_verification_groups::Support::start().await;
    support.turn("lapsed", "one", None).await;
    support
        .server
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");
    support
        .exec(
            "UPDATE wyrd.workload_activity
                SET last_authenticated_at = statement_timestamp() - INTERVAL '400 days'",
        )
        .await;
    support.make_due().await;
    let refused = support
        .scheduler(&Arc::new(tokio::sync::Semaphore::new(4)))
        .group_pass(&CancellationToken::new())
        .await
        .expect("the pass runs");
    let [GroupHandoff::Errored(group)] = refused.as_slice() else {
        panic!("a lapsed owner records one refusal: {refused:?}");
    };
    assert!(
        owed(&support.server).await.contains_key(group),
        "the refusal is owed"
    );
    let group_verifier: uuid::Uuid = sqlx::query_scalar(
        "SELECT verifier_card_uid FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(group.as_uuid())
    .fetch_one(&superuser(&support.server))
    .await
    .expect("the refusal's Verifier reads");
    deliver_until(&support.server, |owed| !owed.contains_key(group)).await;
    let mut conn = support
        .server
        .tenant_conn_for(support.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let system = system_principal_id(&mut conn)
        .await
        .expect("SYSTEM reads")
        .map(PrincipalId::new)
        .expect("the tenant has SYSTEM");
    conn.commit().await.expect("the read commits");
    let (revision, state) = accepted(&support.server, system, *group).await;
    assert_eq!(revision, 1, "the refusal records once");
    let failure = state.failure.expect("the state holds the refusal");
    assert_eq!(
        (failure.status, failure.error.code.as_str()),
        (
            VerificationExecutionStatus::Errored,
            "WYRD_AUTH_401_CREDENTIAL_REVOKED"
        )
    );
    let identity = state.identity.expect("the state holds the identity");
    assert_eq!(
        (
            identity.verifier.uid.as_ref(),
            identity.subject.uid.as_ref(),
            identity.owner_card_uid.as_ref(),
            identity.group_id.as_ref().map(GroupId::as_str)
        ),
        (
            Some(&CardUid::from_uuid(group_verifier).expect("a stored Card UID")),
            Some(&support.agent),
            Some(&support.owner),
            Some("lapsed")
        )
    );
    support
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

/// The hosted engine runs a dependent only on its predecessors' accepted
/// settlements, and a restart reuses them.
///
/// Two independent code steps run in parallel on a writer whose every state
/// revision is held before Scribe; their dependent consumes both outputs.
/// Each revision is released one at a time, and when the dependent's first
/// attempt is recorded the accepted state already covers both predecessors'
/// settlements. The execution then crashes before that revision is accepted,
/// and a reclaimed engine restores both settlements without attempting
/// either step again, runs the dependent once, and passes with both values.
///
/// # Panics
/// Panics when the dependent is recorded before both settlements are
/// accepted, a run fails, a settled step is attempted again or changes, or
/// the dependent does not consume both values.
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
            { "id": "left", "action": code("def f():\n    return 1") },
            { "id": "right", "action": code("def f():\n    return 'two'") },
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
    let (evidence, routes) = (BTreeMap::new(), BTreeMap::new());
    let run = async |state: &Arc<RunState>| {
        let recorded = state.snapshot().await.invocations;
        blocks
            .engine(
                leased.server.state(),
                Execution {
                    caller: &caller,
                    verifier: &verifier,
                    evidence: &evidence,
                    routes: &routes,
                    subject: GatewayCallContext::default(),
                    defer_unlanded_traces: false,
                    drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                    clock: BlockClock::default(),
                    execution: Some(Arc::clone(state)),
                    mode: ExecutionMode::Queued,
                },
            )
            .expect("the engine builds")
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
    let combine = |state: &ExecutionState| settlements(state).contains_key("combine");

    let mut crashed = Box::pin(run(&writer));
    let predecessors = loop {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            tokio::select! {
                ran = &mut crashed => panic!("the run finished while a revision was held: {ran:?}"),
                () = held.arrived.notified() => {}
            }
        })
        .await
        .expect("each revision reaches Scribe");
        if combine(&writer.snapshot().await) {
            break settlements(&leased.accepted().await);
        }
        held.release.add_permits(1);
    };
    assert!(
        ["left", "right"]
            .iter()
            .all(|step| predecessors.get(*step).is_some_and(Option::is_some))
            && !predecessors.contains_key("combine"),
        "the dependent starts only once both settlements are accepted: {predecessors:?}"
    );
    drop(crashed);

    let restarted = leased.reclaim().await;
    let outcome = run(&restarted).await.expect("the reclaimed run completes");
    held.release
        .add_permits(tokio::sync::Semaphore::MAX_PERMITS);
    assert_eq!(outcome.verdict, VerificationVerdict::Passed, "{outcome:?}");
    let accepted = leased.accepted().await;
    let restored = settlements(&accepted);
    for step in ["left", "right"] {
        assert_eq!(restored[step], predecessors[step], "{step} is restored");
    }
    assert_eq!(
        attempts(&accepted)
            .into_iter()
            .filter(|(step, _)| ["left", "right", "combine"].contains(&step.as_str()))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ("left".to_owned(), 1),
            ("right".to_owned(), 1),
            ("combine".to_owned(), 1)
        ]),
        "no settled step is attempted again"
    );
    leased
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

//! The durable publication backend of queued Verifier work, against the
//! in-process server, its File storage and inventory, and the repository
//! Postgres.
//!
//! A trusted hosted caller inside one leased execution publishes through
//! [`Publications`]; no public endpoint accepts arbitrary paths.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{Value, json};
use skald_spec::MediaFormat;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::ScribeStateAdvance;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_verify::blocks::{CallInput, CallValue, Checkpoints as _};
use wyrd_runtime::principal::PrincipalId;
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::scribe_outbox::ScribeWrite;
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::authority::SystemReadAuthority;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_server::verification::hosted::publications::{Publications, PublishCall};
use wyrd_server::verification::hosted::workspace::{Workspace, Workspaces};
use wyrd_server::verification::inputs::InputResolver;
use wyrd_server::verification::leases::LeaseRenewal;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::state::{RunState, StateError, StateRow, StateWriter};
use wyrd_spec::auth::Attribution;
use wyrd_spec::ids::{CardUid, SlotName};
use wyrd_spec::verification::execution::{
    InputDeclaration, Producer, ProducerInvocation, PublicationCollection, PublicationContent,
    PublicationFile, PublicationKind, PublicationManifest, Speculation, StepStatus,
    VerificationOutcome, VerificationResult,
};
use wyrd_spec::verification::state::{EXECUTION_STATE_FORMAT, ExecutionState};
use wyrd_spec::verification::{PublicationName, TaskId};
use wyrd_spec::verification::{VerificationError, VerificationVerdict};
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::verifier_executions::{
    ClaimedExecution, ExecutionLease, Settlement, TerminalStatus, VerifierExecutionQueue,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::verification::VerificationFixture;

use crate::pg_hosted_artifacts::user;

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
        let run = Self::writer(&server, &renewal, system, &claimed)
            .await
            .expect("the execution state claims");
        Self {
            server,
            lease: claimed.lease,
            renewal,
            system,
            run,
        }
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

    /// Claim `claimed`'s execution state for its lease, as a worker would.
    ///
    /// # Errors
    /// Returns the [`StateError`] refusing the claim or its recovery read.
    async fn writer(
        server: &WyrdTestServer,
        renewal: &Arc<LeaseRenewal>,
        system: PrincipalId,
        claimed: &ClaimedExecution,
    ) -> Result<Arc<RunState>, StateError> {
        let tenant = server.data_tenant_id();
        let query = Self::query(server, system).await;
        let lease = renewal.hold(tenant, claimed.lease.token, &CancellationToken::new());
        let run = RunState::claim(
            server.state(),
            Arc::clone(server.state().bifrost.scribe_outbox()),
            tenant,
            StateRow {
                execution_id: claimed.lease.execution_id,
                verifier_uid: claimed.verifier_card_uid.clone(),
                card_uid: claimed.card_uid.clone(),
                owner_card_uid: None,
                run_id: None,
                deployment_environment: claimed.deployment_environment.clone(),
                attribution: Attribution::new(claimed.attribution.sub_principal_id, system),
            },
            StateWriter {
                writer: claimed.lease.token.as_uuid(),
                generation: i64::from(claimed.attempt),
                not_after: Some(claimed.deadline_at),
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(300),
            },
            Some(Arc::new(lease)),
            Some(&InputResolver::new(&query, None)),
        )
        .await?;
        Ok(Arc::new(run))
    }

    /// The execution's accepted state, read back from Bifrost.
    ///
    /// # Panics
    /// Panics when the state is unreadable or was never claimed.
    async fn accepted(&self) -> ExecutionState {
        let query = Self::query(&self.server, self.system).await;
        RunState::accepted(
            self.server.state(),
            self.server.data_tenant_id(),
            self.lease.execution_id,
            &InputResolver::new(&query, None),
        )
        .await
        .expect("the state reads")
        .expect("the state was claimed")
        .1
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
        Self::writer(&self.server, &self.renewal, self.system, &claimed).await
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
    let caller = user(leased.server.data_tenant_id(), []);
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
    let caller = user(leased.server.data_tenant_id(), []);
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
/// another call, or from that call with other arguments, conflicts; a fresh attempt publishes the name again with its own
/// attribution. An upload that fails, or a publication under a lease since
/// reclaimed, leaves nothing readable.
///
/// # Panics
/// Panics when a replay adds an entry, a conflict is accepted, attempts
/// merge, or a failed or stale publication becomes readable.
#[tokio::test(flavor = "multi_thread")]
async fn publication_replay_preserves_attempt_identity() {
    let leased = Leased::start().await;
    let limits = HostedLimits::default();
    let caller = user(leased.server.data_tenant_id(), []);
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
        .json(&call("c4", &second, "late"), json!(null))
        .await
        .expect_err("a reclaimed lease publishes nothing");
    assert!(
        stale
            .to_string()
            .contains("no longer holds its state, lease, or deadline"),
        "{stale}"
    );
    publications
        .json(&call("c1", &first, "report"), json!({"v": 1}))
        .await
        .expect_err("a replay under the stale lease is fenced too");
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
    let caller = user(leased.server.data_tenant_id(), []);
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

    let blind = wyrd_server::components::auth::Caller {
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
    let mounted = Workspaces::new(limits)
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
    let caller = user(leased.server.data_tenant_id(), []);
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
    let workspaces = Workspaces::new(limits);
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
    let caller = user(tenant, []);
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
        .begin(&task, 1)
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
        .begin(&task, 2)
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
            .begin(&task(step), 1)
            .await
            .expect("the attempt is accepted");
    }
    leased
        .run
        .forget(&task("refused"), 1)
        .await
        .expect("the attempt refused capacity is forgotten");
    let deadline = leased.deadline().await;

    let current = leased.reclaim().await;
    assert_eq!(
        attempts(&current.snapshot().await),
        [("review".to_owned(), 1), ("judge".to_owned(), 1)]
    );
    current
        .begin(&task("judge"), 2)
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
    let caller = user(leased.server.data_tenant_id(), []);
    let task = TaskId::new("review").expect("task");
    leased
        .run
        .begin(&task, 1)
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
    let caller = user(leased.server.data_tenant_id(), []);
    let task = TaskId::new("review").expect("task");
    leased
        .run
        .begin(&task, 1)
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
    assert_eq!(current.snapshot().await.terminal.as_ref(), Some(&selected));
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
            batch: uuid::Uuid::now_v7(),
            row: row.row,
            attribution: Attribution::new(leased.system, leased.system),
            state: ScribeStateAdvance {
                key: leased.lease.execution_id.as_uuid(),
                writer: leased.lease.token.as_uuid(),
                revision: 1,
                not_after: None,
            },
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

//! The durable publication backend of queued Verifier work, against the
//! in-process server, its File storage and inventory, and the repository
//! Postgres.
//!
//! A trusted hosted caller inside one leased execution publishes through
//! [`Publications`]; no public endpoint accepts arbitrary paths.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use skald_spec::MediaFormat;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_server::verification::hosted::publications::{Publications, PublishCall};
use wyrd_server::verification::hosted::workspace::Workspace;
use wyrd_spec::verification::execution::{
    InputDeclaration, Producer, PublicationContent, PublicationFile, PublicationKind,
    PublicationManifest,
};
use wyrd_spec::verification::{PublicationName, TaskId};
use wyrd_sql::queries::verifier_executions::{ExecutionLease, VerifierExecutionQueue};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::verification::VerificationFixture;

use crate::pg_hosted_artifacts::user;

/// One leased execution of a fresh in-process server.
struct Leased {
    /// The server; dropping it stops the server.
    server: WyrdTestServer,
    /// The claimed execution's lease.
    lease: ExecutionLease,
}

impl Leased {
    /// Start a server, submit one execution, and claim it.
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
        conn.commit().await.expect("the claim commits");
        Self {
            server,
            lease: claimed.lease,
        }
    }

    /// The publication `name` of `producer`, if accepted.
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn read(&self, producer: &Producer, name: &str) -> Option<PublicationContent> {
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        VerifierExecutionQueue::default()
            .publication(
                &mut conn,
                self.lease.execution_id,
                producer,
                &PublicationName::new(name).expect("name"),
            )
            .await
            .expect("the publication reads")
            .map(|publication| {
                assert_eq!(&publication.producer, producer);
                publication.content
            })
    }
}

impl Leased {
    /// Expire the lease on the database clock and claim the execution again,
    /// returning the new lease; the old one is stale from then on.
    ///
    /// # Panics
    /// Panics when the update or claim fails.
    async fn reclaim(&self) -> ExecutionLease {
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
        claimed.lease
    }

    /// Every accepted publication of the execution as (task, attempt, name).
    ///
    /// # Panics
    /// Panics when the read fails.
    async fn entries(&self) -> Vec<(String, i32, String)> {
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        sqlx::query_as(
            "SELECT task_id, attempt, name FROM wyrd.verifier_execution_publications \
             WHERE execution_id = $1 ORDER BY attempt, sequence",
        )
        .bind(self.lease.execution_id.as_uuid())
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("the publications read")
    }
}

/// A call of `producer` publishing under `name`.
fn call(id: &str, producer: &Producer, name: &str) -> PublishCall {
    PublishCall {
        id: id.to_owned(),
        producer: producer.clone(),
        name: PublicationName::new(name).expect("name"),
    }
}

/// The refusal rule of a structured tool error.
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
    let workspace = Workspace::materialize(Vec::new(), limits, None).expect("workspace");
    let report = "# Review\n\nThe answer holds.\n";
    std::fs::create_dir_all(workspace.work_dir().join("notes")).expect("directory");
    std::fs::write(workspace.work_dir().join("notes/report one.md"), report).expect("write");
    std::fs::write(workspace.work_dir().join("big.bin"), vec![b'x'; 1025]).expect("write");
    let publications = Publications::new(leased.server.state(), &caller, leased.lease, limits);
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
    let workspace = Workspace::materialize(Vec::new(), limits, None).expect("workspace");
    let publications = Publications::new(leased.server.state(), &caller, leased.lease, limits);
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
    let deep = Workspace::materialize(Vec::new(), HostedLimits::default(), None).expect("deep");
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

/// A replay of the same call and arguments recovers its one receipt; the
/// same name from another call, or from that call with other arguments,
/// conflicts; a fresh attempt publishes the name again with its own
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
    let workspace = Workspace::materialize(Vec::new(), limits, None).expect("workspace");
    let publications = Publications::new(leased.server.state(), &caller, leased.lease, limits);
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
        stale.to_string().contains("no longer holds its lease"),
        "{stale}"
    );
    publications
        .json(&call("c1", &first, "report"), json!({"v": 1}))
        .await
        .expect_err("a replay under the stale lease is fenced too");
    Publications::new(leased.server.state(), &caller, current, limits)
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

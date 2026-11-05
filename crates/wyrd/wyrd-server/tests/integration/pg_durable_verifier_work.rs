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
            .file(
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
        .file(&call("c4", &producer, "big"), &workspace, "/work/big.bin")
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

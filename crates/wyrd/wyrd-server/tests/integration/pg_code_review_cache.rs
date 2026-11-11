//! The code-review pipeline over Wyrd's own cache custody.
//!
//! The pipeline's public `ReviewPipeline` runs against a routed GitHub fake
//! with [`CodeReviewObjects`] over a real local object store and
//! [`CodeReviewMetadata`] over the repository Postgres. The cases prove a
//! pinned review cold and warm, rename continuity, the store protocols
//! under competition and interruption, database-clock expiry and its sweep,
//! and that absent, expired and failed lookups stay distinct and tenants
//! stay apart.

use std::io::Write as _;
use std::sync::Arc;
use std::time::Duration;

use bohmian_code_review_pipeline::persistence::ContextLookup;
use bohmian_code_review_pipeline::testing::RoutedTransport;
use bohmian_code_review_pipeline::{
    BoxFuture, ContextStore, Contexts, GitHubClient, GitHubHost, HttpRequest, HttpResponse,
    HttpTransport, Identity, InstallationId, InstallationToken, ObjectKey, ObjectSha,
    ObjectStorage, Operation, PersistenceError, PipelineLimits, PullRequestPins,
    RepositoryIdentity, RequestLimits, ReviewPipeline, SNAPSHOT_FORMAT_VERSION, SnapshotPointer,
    SnapshotScope, SnapshotStore, Snapshots, StorageError, TenantId, TransportError,
};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::json;
use sha1::{Digest as _, Sha1};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_server::components::github::review_cache::{CodeReviewMetadata, CodeReviewObjects};
use wyrd_server::postgres::ServerPostgres;
use wyrd_spec::DataTenantId;
use wyrd_storage::{LocalSigner, StorageHandle};

/// Declares `lookupUser`.
const USER: &[u8] = b"export interface User {\n  id: string;\n}\n\nexport function lookupUser(id: string): User {\n  return { id };\n}\n";
/// Calls `lookupUser` on the default branch.
const HANDLER: &[u8] = b"import { lookupUser } from \"./user\";\n\nexport function handleRequest(id: string): string {\n  return lookupUser(id).id;\n}\n";
/// The pinned head's handler.
const PR_HANDLER: &[u8] = b"import { lookupUser } from \"./user\";\n\nexport function handleRequest(id: string): string {\n  return String(lookupUser(id));\n}\n";
/// The pinned change to the handler.
const PR_PATCH: &str = "@@ -1,5 +1,5 @@\n import { lookupUser } from \"./user\";\n \n export function handleRequest(id: string): string {\n-  return lookupUser(id).id;\n+  return String(lookupUser(id));\n }";
/// The repository's stable id.
const REPO: u64 = 1_296_269;
/// The pull request number.
const PR: u64 = 7;

/// Forty copies of `ch` as an object id.
///
/// # Panics
/// Panics if `ch` is not a lowercase hex digit.
fn sha(ch: char) -> ObjectSha {
    ObjectSha::new(&ch.to_string().repeat(40)).expect("sha")
}

/// Git blob id of `bytes`.
fn blob_sha(bytes: &[u8]) -> ObjectSha {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    ObjectSha::new(&hex::encode(hasher.finalize())).expect("blob sha")
}

/// Gzipped tarball of `files` under GitHub's single top-level directory.
///
/// # Panics
/// Panics if a path is not a valid archive entry name, or building the
/// archive or compressing it fails.
fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("acme-app-ccccccc/{path}"), *bytes)
            .expect("entry");
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&builder.into_inner().expect("tar"))
        .expect("gzip");
    gz.finish().expect("gzip finish")
}

/// Routed GitHub shared between a case and its client.
struct FakeGitHub(Arc<RoutedTransport>);

impl HttpTransport for FakeGitHub {
    /// Delegates to the shared routed transport.
    ///
    /// # Errors
    /// Never.
    fn get(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, TransportError>> {
        self.0.get(request)
    }
}

impl FakeGitHub {
    /// A repository whose default branch is commit `c` (tree `7`) holding
    /// the user module and handler, with PR [`PR`] whose live head moved to
    /// `f` while the caller pinned head `a` (tree `8`) changing the handler.
    fn pinned() -> FakeGitHub {
        let github = FakeGitHub(Arc::new(RoutedTransport::default()));
        let base = [("src/user.ts", USER), ("src/handler.ts", HANDLER)];
        let head = [("src/user.ts", USER), ("src/handler.ts", PR_HANDLER)];
        github.0.json(
            &format!("/repositories/{REPO}"),
            &json!({"id": REPO, "full_name": "acme/app", "default_branch": "main", "private": true})
                .to_string(),
        );
        github.0.json(
            "/pulls?state=closed&base=main&sort=updated&direction=desc&per_page=100&page=1",
            "[]",
        );
        github.0.json(
            "/branches/main",
            &json!({"name": "main", "commit": {"sha": sha('c').as_str(), "commit": {
                "tree": {"sha": sha('7').as_str()}, "committer": {"date": "2026-02-01T12:00:00Z"}}}})
            .to_string(),
        );
        github.commit(&sha('c'), &sha('7'), &base);
        github
            .0
            .route(&format!("/tarball/{}", sha('c')), 200, tarball(&base));
        github.0.json(
            &format!("/pulls/{PR}"),
            &json!({
                "number": PR, "state": "open", "draft": false, "merged": false,
                "html_url": "https://github.com/acme/app/pull/7", "title": "Stringify", "body": "Why",
                "base": {"ref": "main", "sha": sha('c').as_str(), "repo": {"id": REPO}},
                "head": {"ref": "feature", "sha": sha('f').as_str(), "repo": {"id": REPO}},
            })
            .to_string(),
        );
        github.commit(&sha('a'), &sha('8'), &head);
        github.0.json(
            &format!("/compare/{}...{}", sha('c'), sha('a')),
            &json!({"status": "ahead", "base_commit": {"sha": sha('c').as_str()},
                    "merge_base_commit": {"sha": sha('c').as_str()},
                    "commits": [{"sha": sha('a').as_str()}],
                    "files": [{"filename": "src/handler.ts", "status": "modified",
                               "sha": blob_sha(PR_HANDLER).as_str(), "patch": PR_PATCH}]})
            .to_string(),
        );
        github
    }

    /// Route `commit` with root `tree` holding `files`, its recursive
    /// listing and every blob.
    fn commit(&self, commit: &ObjectSha, tree: &ObjectSha, files: &[(&str, &[u8])]) {
        self.0.json(
            &format!("/git/commits/{commit}"),
            &json!({"sha": commit.as_str(), "tree": {"sha": tree.as_str()},
                    "committer": {"date": "2026-03-01T00:00:00Z"}})
            .to_string(),
        );
        let entries: Vec<_> = files
            .iter()
            .map(|(path, bytes)| {
                let blob = blob_sha(bytes);
                self.0.route(&format!("/git/blobs/{blob}"), 200, *bytes);
                json!({"path": path, "mode": "100644", "type": "blob", "sha": blob.as_str(), "size": bytes.len()})
            })
            .collect();
        self.0.json(
            &format!("/git/trees/{tree}?recursive=1"),
            &json!({"sha": tree.as_str(), "truncated": false, "tree": entries}).to_string(),
        );
    }

    /// A client over this fake.
    fn client(&self) -> GitHubClient {
        GitHubClient::with_transport(
            GitHubHost::public(),
            InstallationToken::new("ghs_token").expect("token"),
            RequestLimits::default(),
            Box::new(FakeGitHub(Arc::clone(&self.0))),
        )
    }
}

/// The caller's frozen pins for PR [`PR`] at head `a`.
fn pins() -> PullRequestPins {
    PullRequestPins {
        number: PR,
        base: sha('c'),
        head: sha('a'),
        head_repository_id: REPO,
    }
}

/// The verified identity of the repository under `tenant`, labelled
/// `owner/name`.
///
/// # Panics
/// Panics if `owner` or `name` is not a valid repository label.
fn identity(tenant: DataTenantId, owner: &str, name: &str) -> Identity {
    Identity {
        tenant: TenantId::new(&tenant.to_string()).expect("tenant"),
        installation: InstallationId::new(42).expect("installation"),
        repository: RepositoryIdentity::new(REPO, owner, name).expect("repo"),
    }
}

/// Wyrd's cache custody over a real database and a temporary local store.
struct Custody {
    /// Owns the test database.
    fixture: PgFixture,
    /// Root of the local object store.
    root: tempfile::TempDir,
    /// Objects in the local store.
    objects: CodeReviewObjects,
    /// Metadata in Postgres.
    metadata: CodeReviewMetadata,
}

impl Custody {
    /// Start a database and a local store.
    ///
    /// # Panics
    /// Panics when the database or store cannot start.
    async fn start() -> Custody {
        let fixture = PgFixture::start().await.expect("fixture");
        let root = tempfile::tempdir().expect("tempdir");
        let storage = Arc::new(StorageHandle::new(
            LocalSigner::new(root.path().to_path_buf()).expect("local signer"),
        ));
        let postgres = Arc::new(ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        Custody {
            objects: CodeReviewObjects::new(storage),
            metadata: CodeReviewMetadata::new(postgres),
            fixture,
            root,
        }
    }

    /// The seeded tenant's identity under the original labels.
    fn identity(&self) -> Identity {
        identity(self.fixture.data_tenant_id(), "acme", "app")
    }

    /// Relative paths of every object in the local store.
    ///
    /// # Panics
    /// Panics if a store directory or entry cannot be read, or a walked path
    /// is not under the store root.
    fn stored(&self) -> Vec<String> {
        let mut pending = vec![self.root.path().to_path_buf()];
        let mut paths = Vec::new();
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let relative = path.strip_prefix(self.root.path()).expect("under root");
                    paths.push(relative.to_string_lossy().into_owned());
                }
            }
        }
        paths.sort();
        paths
    }

    /// Move every context's stamps 25 hours into the past, past the
    /// pipeline's context lifetime.
    ///
    /// # Panics
    /// Panics when the update fails.
    async fn age_contexts(&self) {
        let pool = self.fixture.superuser_pool().expect("superuser pool");
        sqlx::query(
            "UPDATE wyrd.code_review_contexts
                SET published_at = published_at - interval '25 hours',
                    expires_at = expires_at - interval '25 hours'",
        )
        .execute(&pool)
        .await
        .expect("age contexts");
    }
}

/// A pinned review runs cold then warm over Wyrd custody: the cold run
/// builds one snapshot whose objects sit under the tenant's code-review
/// prefix, the warm run and a run under renamed labels reuse it without
/// another archive, and the published context reads back under the new name.
///
/// # Panics
/// Panics when a review fails, a warm or renamed run rebuilds, an object
/// sits outside the tenant prefix, or a read-back differs.
#[tokio::test]
async fn pinned_review_runs_cold_and_warm_over_wyrd_custody() {
    let custody = Custody::start().await;
    let github = FakeGitHub::pinned();
    let client = github.client();
    let pipeline = ReviewPipeline::new(
        &custody.objects,
        &custody.metadata,
        PipelineLimits::default(),
    );

    let cold = pipeline
        .pinned(&client, &custody.identity(), &pins(), None)
        .await
        .expect("cold pinned review");
    assert_eq!(client.calls().get(Operation::Archive), 1);
    assert_eq!(
        (cold.full.base.clone(), cold.full.head.clone()),
        (sha('c'), sha('a'))
    );
    assert_eq!(cold.active, cold.full);
    assert_eq!(cold.full.files.len(), 1);
    assert_eq!(cold.full.files[0].hunks[0].patch, PR_PATCH);
    assert!(!cold.context.context.spans.is_empty());

    let prefix = format!(
        "{}/code-review/installations/42/repositories/{REPO}/",
        custody.fixture.data_tenant_id()
    );
    let stored = custody.stored();
    assert!(
        stored.iter().any(|path| path.contains("/snapshots/"))
            && stored.iter().any(|path| path.contains("/contexts/")),
        "{stored:?}"
    );
    assert!(
        stored.iter().all(|path| path.starts_with(&prefix)),
        "{stored:?}"
    );

    let warm = pipeline
        .pinned(&client, &custody.identity(), &pins(), None)
        .await
        .expect("warm pinned review");
    assert_eq!(
        client.calls().get(Operation::Archive),
        1,
        "warm reuses the snapshot"
    );
    assert_eq!(warm.full, cold.full);
    assert_eq!(warm.context.context.spans, cold.context.context.spans);

    let renamed = identity(custody.fixture.data_tenant_id(), "new-owner", "renamed");
    let after_rename = pipeline
        .pinned(&client, &renamed, &pins(), None)
        .await
        .expect("pinned review after rename");
    assert_eq!(
        client.calls().get(Operation::Archive),
        1,
        "a rename keeps the snapshot"
    );
    assert_eq!(after_rename.full, cold.full);
    assert_eq!(
        pipeline
            .read(&renamed, &cold.context.record.id)
            .await
            .expect("read under the new name"),
        cold.context.context
    );
}

/// The object adapter refuses an occupied key, distinguishes an absent key
/// and a taken key from a failing store, and deletes idempotently.
///
/// # Panics
/// Panics when a create overwrites, an absent read is not `NotFound`, or a
/// foreign key or a failing store is not reported as a backend failure.
#[tokio::test]
async fn objects_are_create_only_and_failures_stay_distinct() {
    let custody = Custody::start().await;
    let identity = custody.identity();
    let key = ObjectKey::snapshot(&identity, "v", 1, 1024, &sha('c'));

    custody
        .objects
        .create(&key, b"first")
        .await
        .expect("create");
    assert_eq!(
        custody.objects.create(&key, b"second").await,
        Err(StorageError::AlreadyExists { key: key.clone() })
    );
    assert_eq!(custody.objects.read(&key).await.expect("read"), b"first");
    custody.objects.delete(&key).await.expect("delete");
    custody.objects.delete(&key).await.expect("delete again");
    assert_eq!(
        custody.objects.read(&key).await,
        Err(StorageError::NotFound { key: key.clone() })
    );

    let foreign = Identity {
        tenant: TenantId::new("acme").expect("tenant"),
        ..identity
    };
    let foreign_key = ObjectKey::snapshot(&foreign, "v", 1, 1024, &sha('c'));
    assert!(matches!(
        custody.objects.read(&foreign_key).await,
        Err(StorageError::Backend { .. })
    ));

    let broken = tempfile::tempdir().expect("tempdir");
    let objects = CodeReviewObjects::new(Arc::new(StorageHandle::new(
        LocalSigner::new(broken.path().to_path_buf()).expect("local signer"),
    )));
    std::fs::remove_dir(broken.path()).expect("remove the store root");
    std::fs::write(broken.path(), b"not a directory").expect("replace it with a file");
    assert!(
        matches!(
            objects.create(&key, b"bytes").await,
            Err(StorageError::Backend { .. })
        ),
        "a failing store is a backend failure, not a taken key"
    );
    assert!(
        matches!(objects.read(&key).await, Err(StorageError::Backend { .. })),
        "a failing store is a backend failure, not an absent key"
    );
    std::fs::remove_file(broken.path()).expect("clean up");
}

/// The metadata owner keeps the snapshot protocol: one of two competing
/// first publications wins, a claimed record refuses a build's upsert, and
/// a collection interrupted after its claim is finished by the next one.
///
/// # Panics
/// Panics when both or neither publication wins, a claimed record accepts
/// an upsert, or the retried collection does not delete the claimed object.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshot_metadata_survives_competition_and_interruption() {
    let custody = Custody::start().await;
    let identity = custody.identity();
    let scope = SnapshotScope {
        identity: identity.clone(),
        extraction_version: "v".to_owned(),
        format_version: SNAPSHOT_FORMAT_VERSION,
        max_file_bytes: 1024,
    };
    let pointer = |commit: char| {
        let key = ObjectKey::snapshot(&identity, "v", SNAPSHOT_FORMAT_VERSION, 1024, &sha(commit));
        SnapshotPointer {
            tree: sha(commit),
            commit: sha(commit),
            digest: bohmian_code_review_pipeline::ObjectDigest::of(key.as_str().as_bytes()),
            key,
        }
    };
    let (first, second) = (pointer('a'), pointer('b'));

    let (won_a, won_b) = tokio::join!(
        custody
            .metadata
            .publish_snapshot_pointer(&scope, None, &first),
        custody
            .metadata
            .publish_snapshot_pointer(&scope, None, &second),
    );
    assert!(won_a.expect("publish a") ^ won_b.expect("publish b"));
    let current = custody
        .metadata
        .snapshot_pointer(&scope)
        .await
        .expect("read")
        .expect("published");
    let stale = if current == first { &second } else { &first };
    assert!(
        !custody
            .metadata
            .publish_snapshot_pointer(&scope, Some(&stale.tree), &pointer('c'))
            .await
            .expect("stale swap")
    );

    custody
        .metadata
        .upsert_record(&identity, &stale.key)
        .await
        .expect("record");
    custody
        .objects
        .create(&stale.key, b"snapshot")
        .await
        .expect("object");
    assert_eq!(
        custody
            .metadata
            .unreferenced(&identity, Duration::from_secs(3600))
            .await
            .expect("claim within grace"),
        Vec::new(),
        "a recently touched record is protected by the grace"
    );
    assert_eq!(
        custody
            .metadata
            .unreferenced(&identity, Duration::ZERO)
            .await
            .expect("interrupted claim"),
        vec![stale.key.clone()]
    );
    assert_eq!(
        custody.metadata.upsert_record(&identity, &stale.key).await,
        Err(PersistenceError::Collecting {
            key: stale.key.clone()
        })
    );
    let snapshots = Snapshots::new(&custody.objects, &custody.metadata);
    assert_eq!(
        snapshots
            .collect(&identity, Duration::ZERO)
            .await
            .expect("retried collection"),
        vec![stale.key.clone()]
    );
    assert_eq!(
        custody.objects.read(&stale.key).await,
        Err(StorageError::NotFound {
            key: stale.key.clone()
        })
    );
    custody
        .metadata
        .upsert_record(&identity, &stale.key)
        .await
        .expect("a collected key can be rebuilt");
}

/// Context metadata keeps its first stamps, expires on the database clock
/// without being extended by a repeat publication, is swept with its object
/// across tenants, and stays inside its tenant; a failed lookup is a backend
/// error, never absent.
///
/// # Panics
/// Panics when a repeat extends the stamps, a lookup misclassifies, the
/// sweep leaves the expired context, or another tenant sees it.
#[tokio::test]
async fn contexts_expire_without_extension_and_stay_in_their_tenant() {
    let custody = Custody::start().await;
    let github = FakeGitHub::pinned();
    let client = github.client();
    let pipeline = ReviewPipeline::new(
        &custody.objects,
        &custody.metadata,
        PipelineLimits::default(),
    );
    let identity = custody.identity();
    let review = pipeline
        .pinned(&client, &identity, &pins(), None)
        .await
        .expect("pinned review");
    let record = review.context.record.clone();

    let repeat = custody
        .metadata
        .upsert_context(&identity, &record)
        .await
        .expect("repeat publication");
    assert_eq!(repeat, review.context.lifetime, "a repeat keeps its stamps");
    assert_eq!(
        custody
            .metadata
            .context(&identity, &record.id)
            .await
            .expect("lookup"),
        ContextLookup::Live(record.clone())
    );

    let other = custody
        .fixture
        .seed_additional_tenant("code-review-other")
        .await
        .expect("second tenant");
    let outsider = Identity {
        tenant: TenantId::new(&other.to_string()).expect("tenant"),
        ..identity.clone()
    };
    assert_eq!(
        custody
            .metadata
            .context(&outsider, &record.id)
            .await
            .expect("lookup"),
        ContextLookup::Absent
    );
    let unseeded = Identity {
        tenant: TenantId::new(&DataTenantId::new_v7().to_string()).expect("tenant"),
        ..identity.clone()
    };
    assert!(matches!(
        custody.metadata.upsert_context(&unseeded, &record).await,
        Err(PersistenceError::Backend { .. })
    ));
    let foreign = Identity {
        tenant: TenantId::new("acme").expect("tenant"),
        ..identity.clone()
    };
    assert!(matches!(
        custody.metadata.context(&foreign, &record.id).await,
        Err(PersistenceError::Backend { .. })
    ));

    custody.age_contexts().await;
    assert_eq!(
        custody
            .metadata
            .context(&identity, &record.id)
            .await
            .expect("lookup"),
        ContextLookup::Expired
    );
    assert_eq!(
        custody
            .metadata
            .upsert_context(&identity, &record)
            .await
            .expect("repeat after expiry")
            .expires_at,
        review.context.lifetime.expires_at - Duration::from_hours(25),
        "a repeat publication never extends an expired context"
    );
    assert!(matches!(
        pipeline.read(&identity, &record.id).await,
        Err(bohmian_code_review_pipeline::PipelineError::Context(_))
    ));

    let cleanup = Contexts::new(&custody.objects, &custody.metadata)
        .collect_expired(100)
        .await
        .expect("sweep");
    assert_eq!(cleanup.deleted, vec![(identity.clone(), record.id.clone())]);
    assert!(cleanup.lagging.is_empty(), "{:?}", cleanup.lagging);
    assert_eq!(
        custody
            .metadata
            .context(&identity, &record.id)
            .await
            .expect("lookup"),
        ContextLookup::Absent
    );
    assert_eq!(
        custody.objects.read(&record.key).await,
        Err(StorageError::NotFound {
            key: record.key.clone()
        })
    );
}

/// Two cold pinned reviews of one repository racing over Wyrd custody
/// both succeed with the same comparison, converge on one snapshot, and a
/// later review reuses it without another archive.
///
/// # Panics
/// Panics when either racing review fails, they disagree, more than one
/// snapshot object is kept, or the later review rebuilds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn competing_cold_reviews_converge_on_one_snapshot() {
    let custody = Custody::start().await;
    let github = FakeGitHub::pinned();
    let pipeline = ReviewPipeline::new(
        &custody.objects,
        &custody.metadata,
        PipelineLimits::default(),
    );
    let identity = custody.identity();
    let (first_client, second_client) = (github.client(), github.client());
    let pins = pins();

    let (first, second) = tokio::join!(
        pipeline.pinned(&first_client, &identity, &pins, None),
        pipeline.pinned(&second_client, &identity, &pins, None),
    );
    let (first, second) = (
        first.expect("first racing review"),
        second.expect("second racing review"),
    );
    assert_eq!(first.full, second.full);
    assert_eq!(
        custody
            .stored()
            .iter()
            .filter(|path| path.contains("/snapshots/"))
            .count(),
        1,
        "{:?}",
        custody.stored()
    );

    let later = github.client();
    pipeline
        .pinned(&later, &identity, &pins, None)
        .await
        .expect("later review");
    assert_eq!(
        later.calls().get(Operation::Archive),
        0,
        "the later review reuses the converged snapshot"
    );
}

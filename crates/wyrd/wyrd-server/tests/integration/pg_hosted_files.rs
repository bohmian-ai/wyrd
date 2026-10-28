//! Uploaded verification Files through the hosted evidence reader, against
//! the in-process server, its object inventory, and the repository Postgres.
//!
//! Each File is uploaded through the shared storage lifecycle, so the reader
//! locates it only through the completed upload record production writes.

use std::collections::{BTreeMap, BTreeSet};

use skald_spec::{MediaFormat, MediaSource};
use vala_verify::blocks::{
    ActionExecutor, BlockCall, BlockError, CallInput, CallValue, EvidenceReader, Position,
};
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::Action;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};
use wyrd_testing::{Bootstrap, WyrdTestServer};

use crate::pg_hosted_artifacts::user;
use crate::pg_verification_routes::upload_file;

/// An in-process server and an editor's access token for uploading Files.
struct Uploads {
    /// The server; dropping it stops the server.
    server: WyrdTestServer,
    /// The editor's access token.
    jwt: String,
}

impl Uploads {
    /// Start the server and bootstrap the uploading editor.
    ///
    /// # Panics
    /// Panics when the server or bootstrap fails.
    async fn start() -> Self {
        let server = WyrdTestServer::start_in_process()
            .await
            .expect("test server starts");
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("hosted-file-writer", &["editor"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        Self { server, jwt }
    }

    /// Upload `bytes` as a File and return their digest.
    ///
    /// # Panics
    /// Panics when the upload fails.
    async fn put(&self, bytes: &[u8]) -> Sha256Hex {
        upload_file(&self.server, &self.jwt, bytes).await;
        Sha256Hex::digest(bytes)
    }

    /// Evidence citing `slots`, read by a tenant caller with no grants.
    fn reader(&self, slots: BTreeMap<SlotName, InputDeclaration>) -> TenantEvidence {
        TenantEvidence::new(
            self.server.state().clone(),
            user(self.server.data_tenant_id(), []),
            slots,
            HostedLimits::default(),
        )
    }
}

/// A file declaration.
fn file(sha256: Sha256Hex, size_bytes: u64, format: MediaFormat) -> InputDeclaration {
    InputDeclaration::File {
        sha256,
        size_bytes,
        format,
        filename: None,
    }
}

/// The exact uploaded File reads. A declaration with no completed upload
/// record — another length or digest — is not ready yet; another format,
/// a size over the limit, and a non-file declaration are terminal.
///
/// # Panics
/// Panics when a mismatched read succeeds or is classified otherwise.
#[tokio::test(flavor = "current_thread")]
async fn evidence_reads_only_matching_files() {
    let uploads = Uploads::start().await;
    let sha256 = uploads.put(b"%PDF-1.5 body").await;
    let evidence = uploads.reader(BTreeMap::new());
    let (bytes, format) = evidence
        .read(&file(sha256, 13, MediaFormat::Pdf))
        .await
        .expect("the uploaded file reads");
    assert_eq!(
        (bytes.as_slice(), format),
        (&b"%PDF-1.5 body"[..], MediaFormat::Pdf)
    );

    for declaration in [
        file(sha256, 12, MediaFormat::Pdf),
        file(Sha256Hex::digest(b"other"), 13, MediaFormat::Pdf),
    ] {
        let error = evidence.read(&declaration).await.expect_err("not ready");
        assert!(matches!(error, BlockError::Retryable { .. }), "{error}");
    }
    for declaration in [
        file(sha256, 13, MediaFormat::Png),
        file(sha256, 21 * 1024 * 1024, MediaFormat::Pdf),
        InputDeclaration::Json {
            value: serde_json::json!(1),
        },
    ] {
        let error = evidence.read(&declaration).await.expect_err("refused");
        assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
    }
    uploads
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// A stored body whose digest differs from its completed record is refused.
///
/// # Panics
/// Panics when the tampered body reads.
#[tokio::test(flavor = "current_thread")]
async fn evidence_refuses_tampered_body() {
    let uploads = Uploads::start().await;
    let stored = upload_file(&uploads.server, &uploads.jwt, b"hello").await;
    let sha256 = Sha256Hex::digest(b"hello");
    let state = uploads.server.state();
    state
        .storage
        .put_object(
            &wyrd_storage::tenant_path::validate_evidence(&stored, uploads.server.data_tenant_id())
                .expect("the recorded File location validates"),
            b"jello".to_vec(),
        )
        .await
        .expect("tamper");
    let error = uploads
        .reader(BTreeMap::new())
        .read(&file(sha256, 5, MediaFormat::Text))
        .await
        .expect_err("digest mismatch");
    assert!(error.to_string().contains("digest"), "{error}");
    uploads
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Native media is base64 with the format's MIME type; non-native formats
/// are refused.
///
/// # Panics
/// Panics when media binding differs.
#[tokio::test(flavor = "current_thread")]
async fn evidence_binds_native_media_only() {
    let uploads = Uploads::start().await;
    let pdf = uploads.put(b"%PDF-1.5").await;
    let csv = uploads.put(b"a,b").await;
    let evidence = uploads.reader(BTreeMap::new());
    let media = evidence
        .media(&file(pdf, 8, MediaFormat::Pdf))
        .await
        .expect("pdf is native");
    assert_eq!(
        media.source,
        MediaSource::Base64 {
            mime_type: "application/pdf".to_owned(),
            data: "JVBERi0xLjU=".to_owned(),
        }
    );
    assert!(
        evidence
            .media(&file(csv, 3, MediaFormat::Csv))
            .await
            .is_err()
    );
    uploads
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// `extract` reads `from` evidence and returns the locked shape with
/// extractor provenance; non-evidence `from` is refused.
///
/// # Panics
/// Panics when the output or provenance differs.
#[tokio::test(flavor = "current_thread")]
async fn extract_action_returns_locked_shape() {
    let uploads = Uploads::start().await;
    let sha256 = uploads.put(b"a,b\n1,2").await;
    let evidence = uploads.reader(BTreeMap::new());
    let id = TaskId::new("pull").expect("task id");
    let block: Action =
        serde_json::from_value(serde_json::json!({"kind": "extract", "from": "inputs.sheet"}))
            .expect("extract action");
    let call = |value| BlockCall {
        id: &id,
        position: Position::Step,
        block: &block,
        inputs: BTreeMap::from([(
            "from".to_owned(),
            CallInput {
                value,
                sources: BTreeSet::new(),
            },
        )]),
        workspace: BTreeMap::new(),
        deadline: tokio::time::Instant::now(),
    };
    let output = ActionExecutor::execute(
        &evidence,
        call(CallValue::Evidence(file(sha256, 7, MediaFormat::Csv))),
    )
    .await
    .expect("csv extracts");
    assert_eq!(
        output.output,
        serde_json::json!({"format": "csv", "rows": [["a", "b"], ["1", "2"]]})
    );
    assert_eq!(
        output.provenance,
        Some(CallProvenance::Extract {
            version: wyrd_loader::EXTRACT_VERSION.to_owned()
        })
    );
    assert!(
        ActionExecutor::execute(&evidence, call(CallValue::Json(serde_json::json!(1))))
            .await
            .is_err()
    );
    uploads
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Only reachable, pinned slots with existing locations stay cited: PDF
/// pages and regions, text lines, JSON pointers into inline JSON, and
/// canonical DOCX pointers.
///
/// # Panics
/// Panics when a fabricated or unreachable citation survives.
#[tokio::test(flavor = "current_thread")]
async fn citations_must_locate_reachable_evidence() {
    let uploads = Uploads::start().await;
    let mut slots = BTreeMap::new();
    for (slot, bytes, format) in [
        (
            "deck",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards/verifier_building_blocks/samples/sample.pdf"
            ))
            .to_vec(),
            MediaFormat::Pdf,
        ),
        (
            "memo",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards/verifier_building_blocks/samples/sample.docx"
            ))
            .to_vec(),
            MediaFormat::Docx,
        ),
        ("notes", b"alpha\nbeta\ngamma".to_vec(), MediaFormat::Text),
    ] {
        let sha256 = uploads.put(&bytes).await;
        slots.insert(
            SlotName::new(slot).expect("slot"),
            file(sha256, bytes.len() as u64, format),
        );
    }
    slots.insert(
        SlotName::new("facts").expect("slot"),
        InputDeclaration::Json {
            value: serde_json::json!({"rows": [{"days": 30}]}),
        },
    );
    let evidence = uploads.reader(slots);
    let cite = |slot: &str, locator: Locator| Citation {
        slot: SlotName::new(slot).expect("slot"),
        locator,
    };
    let valid = vec![
        cite("deck", Locator::Page(1)),
        cite(
            "deck",
            Locator::Region {
                page: 1,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        ),
        cite("notes", Locator::Lines { start: 2, end: 3 }),
        cite("facts", Locator::JsonPointer("/rows/0/days".to_owned())),
        cite("facts", Locator::JsonPointer("/rows/0".to_owned())),
        cite("memo", Locator::JsonPointer("/blocks/0/text".to_owned())),
    ];
    let invalid = vec![
        cite("deck", Locator::Page(2)),
        cite("deck", Locator::Slide(1)),
        cite("notes", Locator::Lines { start: 3, end: 4 }),
        cite("notes", Locator::Lines { start: 0, end: 1 }),
        cite("facts", Locator::JsonPointer("/rows/1".to_owned())),
        cite("facts", Locator::JsonPointer("/ro".to_owned())),
        cite("memo", Locator::JsonPointer("/format".to_owned())),
        cite("ghost", Locator::Page(1)),
    ];
    let reachable: BTreeSet<SlotName> = ["deck", "memo", "notes", "facts", "ghost"]
        .into_iter()
        .map(|slot| SlotName::new(slot).expect("slot"))
        .collect();
    let mut all = valid.clone();
    all.extend(invalid);
    assert_eq!(evidence.cited(&reachable, all).await.expect("cited"), valid);

    let only_notes = BTreeSet::from([SlotName::new("notes").expect("slot")]);
    assert!(
        evidence
            .cited(&only_notes, vec![cite("deck", Locator::Page(1))])
            .await
            .expect("cited")
            .is_empty()
    );
    uploads
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

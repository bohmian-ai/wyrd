//! Uploads publish exactly the supplied files and prepare extraction
//! outputs without touching the source tree.

use std::fs;

use tempfile::TempDir;
use wyrd_spec::card::artifact::ArtifactSpec;
use wyrd_spec::card::common::NonSecretValue;
use wyrd_spec::envelope::Metadata;

use super::ArtifactUpload;

/// Artifact metadata named `policies`.
///
/// # Panics
/// Never; the name is valid.
fn metadata() -> Metadata {
    serde_json::from_value(serde_json::json!({"name": "policies"})).expect("metadata")
}

/// A `file` Artifact spec.
fn spec() -> ArtifactSpec {
    ArtifactSpec {
        artifact_kind: "file".to_owned(),
        ..ArtifactSpec::default()
    }
}

/// The manifest paths of `upload`'s one submission, which an Artifact
/// upload always produces.
///
/// # Panics
/// Panics when `upload` holds no submission.
fn paths(upload: &ArtifactUpload) -> Vec<&str> {
    upload.input.submissions[0]
        .artifacts
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect()
}

/// A directory holding `files`.
///
/// # Panics
/// Panics when a file cannot be written.
fn tree(files: &[(&str, &[u8])]) -> TempDir {
    let root = TempDir::new().expect("root");
    for (path, bytes) in files {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, bytes).expect("file");
    }
    root
}

/// A file keeps its basename without its parent being scanned, and a
/// directory keeps every regular file, a Card-document name included.
///
/// # Panics
/// Panics when a file is dropped, added, or renamed.
#[test]
fn uploads_publish_exactly_the_supplied_files() {
    let root = tree(&[
        ("card.json", b"{}"),
        ("docs/a.txt", b"a"),
        ("b.yaml", b"b: 1"),
    ]);
    let file = ArtifactUpload::prepare(&root.path().join("docs/a.txt"), metadata(), spec(), false)
        .expect("file uploads");
    assert_eq!(paths(&file), ["a.txt"]);
    let dir = ArtifactUpload::prepare(root.path(), metadata(), spec(), false).expect("dir uploads");
    assert_eq!(paths(&dir), ["b.yaml", "card.json", "docs/a.txt"]);
    assert!(dir.extracted.is_empty() && dir.skipped.is_empty());
    assert!(dir.input.submissions[0].spec.get("metadata").is_none());
}

/// Extraction writes `extracted/P.json` for each supported source outside
/// the source tree, skips the rest, and records the report.
///
/// # Panics
/// Panics when an output, the report, or the untouched source differs.
#[test]
fn uploads_extract_supported_files_before_registration() {
    let pdf = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/verifier_building_blocks/samples/sample.pdf"
    ))
    .expect("fixture");
    let root = tree(&[
        ("policy.md", b"# Refunds\nwithin 30 days\n"),
        ("docs/sample.pdf", &pdf),
        ("raw.bin", b"\x00\x01"),
    ]);
    let upload = ArtifactUpload::prepare(root.path(), metadata(), spec(), true).expect("extracts");
    assert_eq!(
        paths(&upload),
        [
            "docs/sample.pdf",
            "extracted/docs/sample.pdf.json",
            "extracted/policy.md.json",
            "policy.md",
            "raw.bin",
        ]
    );
    assert_eq!(upload.skipped, ["raw.bin"]);
    assert!(!root.path().join("extracted").exists());
    let output = upload
        .input
        .artifact_sources
        .iter()
        .find(|(path, _)| path.as_str() == "extracted/docs/sample.pdf.json")
        .map(|(_, local)| fs::read(local).expect("output"))
        .expect("output source");
    let output: serde_json::Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(output["pages"][0]["text"], "Refunds");
    let text = |value: &str| NonSecretValue::Str(value.to_owned());
    let pair = |source: &str, output: &str| {
        NonSecretValue::Object(
            [("source", text(source)), ("output", text(output))]
                .map(|(key, value)| (key.to_owned(), value))
                .into(),
        )
    };
    let declared: ArtifactSpec =
        serde_json::from_value(upload.input.submissions[0].spec.clone()).expect("spec");
    assert_eq!(
        declared.metadata["extraction"],
        NonSecretValue::Object(
            [
                ("version", text(crate::EXTRACT_VERSION)),
                (
                    "files",
                    NonSecretValue::List(vec![
                        pair("docs/sample.pdf", "extracted/docs/sample.pdf.json"),
                        pair("policy.md", "extracted/policy.md.json"),
                    ]),
                ),
                ("skipped", NonSecretValue::List(vec![text("raw.bin")])),
            ]
            .map(|(key, value)| (key.to_owned(), value))
            .into(),
        )
    );
}

/// Blank kinds, caller reports, output collisions, corrupt documents,
/// links, and empty directories fail before any declaration, each as a
/// validation error whose rule names the repair.
///
/// # Panics
/// Panics when a refused upload prepares or is refused under another code
/// or rule.
#[test]
fn uploads_refuse_invalid_sources_and_reports() {
    let root = tree(&[("policy.md", b"# Refunds")]);
    let blank = ArtifactSpec {
        artifact_kind: " ".to_owned(),
        ..ArtifactSpec::default()
    };
    assert!(ArtifactUpload::prepare(root.path(), metadata(), blank, false).is_err());
    let mut claimed = spec();
    claimed.metadata.insert(
        "extraction".to_owned(),
        NonSecretValue::Str("mine".to_owned()),
    );
    assert!(ArtifactUpload::prepare(root.path(), metadata(), claimed.clone(), true).is_err());
    assert!(ArtifactUpload::prepare(root.path(), metadata(), claimed, false).is_ok());

    let collides = tree(&[("policy.md", b"x"), ("extracted/policy.md.json", b"{}")]);
    let corrupt = tree(&[("broken.pdf", b"%PDF-1.4 truncated")]);
    let empty = TempDir::new().expect("empty");
    let linked = tree(&[("a.txt", b"a")]);
    std::os::unix::fs::symlink(linked.path().join("a.txt"), linked.path().join("b.txt"))
        .expect("link");
    for (source, extract, rule) in [
        (
            collides.path().to_path_buf(),
            true,
            "extraction_output_collision",
        ),
        (corrupt.path().to_path_buf(), true, "extraction_refused"),
        (empty.path().to_path_buf(), false, "artifact_source_empty"),
        (
            linked.path().to_path_buf(),
            false,
            "artifact_source_regular",
        ),
        (
            linked.path().join("b.txt"),
            false,
            "artifact_source_regular",
        ),
    ] {
        let refused = ArtifactUpload::prepare(&source, metadata(), spec(), extract)
            .expect_err("refused before declaring");
        let problem = refused.as_problem_json();
        assert_eq!(
            (refused.code(), &problem["details"]["rule"]),
            ("WYRD_SPEC_400_VALIDATION", &serde_json::json!(rule)),
            "{}: {problem}",
            source.display()
        );
        assert_eq!(problem["details"]["field"], "source", "{problem}");
    }
}

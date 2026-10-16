//! Workspaces stay inside their directory, inside their limits, and serve
//! the same locators extraction produces.

use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::json;
use wyrd_storage::handle::StorageHandle;
use wyrd_storage::local::LocalSigner;

use super::super::extract::tests::{docx, ooxml, pdf};
use super::*;

/// One staged file from `slot`.
fn staged(path: &str, bytes: Vec<u8>, format: MediaFormat) -> Staged {
    Staged {
        path: path.to_owned(),
        bytes,
        format: Some(format),
        sources: BTreeSet::from([SlotName::new("evidence").expect("slot")]),
    }
}

/// `locator` as its wire JSON.
fn loc(locator: Locator) -> Value {
    serde_json::to_value(locator).expect("locator serializes")
}

/// Materialize `files` under default limits.
fn open(files: Vec<Staged>) -> Result<Workspace, BlockError> {
    Workspace::materialize(files, HostedLimits::default(), None)
}

/// A gzip tar holding `entries`, each a raw header name, type, and body.
fn targz(entries: &[(&[u8], tar::EntryType, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, kind, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.as_gnu_mut().expect("gnu header").name[..name.len()].copy_from_slice(name);
        header.set_entry_type(*kind);
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        if *kind == tar::EntryType::Symlink {
            header.set_link_name("/etc/passwd").expect("link name");
        }
        header.set_cksum();
        builder.append(&header, *body).expect("append entry");
    }
    builder
        .into_inner()
        .expect("finish tar")
        .finish()
        .expect("finish gzip")
}

/// `fs.read` and `fs.search` return extraction's locators and keep each
/// file's original slots.
///
/// # Panics
/// Panics when a locator or source differs from extraction.
#[test]
fn workspace_reads_and_searches_located_units() {
    let workspace = open(vec![
        staged(
            "/inputs/deck.pdf",
            pdf(&["Revenue grew", "Costs fell"]),
            MediaFormat::Pdf,
        ),
        staged("/inputs/memo.docx", docx(), MediaFormat::Docx),
        staged(
            "/inputs/data.json",
            br#"{"total": 42}"#.to_vec(),
            MediaFormat::Json,
        ),
    ])
    .expect("materializes");

    let read = workspace.read("/inputs/deck.pdf", 1).expect("pdf reads");
    assert_eq!(
        read["units"],
        json!([{"locator": loc(Locator::Page(2)), "text": "Costs fell"}])
    );
    let found = workspace.search("costs").expect("search");
    assert_eq!(
        found["matches"],
        json!([{"path": "/inputs/deck.pdf", "locator": loc(Locator::Page(2)), "snippet": "Costs fell", "sources": ["evidence"]}])
    );
    assert_eq!(
        workspace.read("/inputs/data.json", 0).expect("json")["units"][0]["locator"],
        loc(Locator::JsonPointer("/total".to_owned()))
    );
    assert!(
        workspace
            .sources("/inputs/memo.docx")
            .is_some_and(|slots| slots.len() == 1)
    );

    std::fs::write(
        workspace.work_dir().join("notes.txt"),
        "draft\ncosts noted\n",
    )
    .expect("write work");
    assert_eq!(
        workspace.read("/work/notes.txt", 1).expect("work reads")["units"][0]["text"],
        "costs noted"
    );
    assert_eq!(
        workspace.search("costs").expect("search")["matches"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let listed = workspace.list("/work").expect("list");
    assert_eq!(
        listed["files"],
        json!([{"path": "/work/notes.txt", "size_bytes": 18, "sources": []}])
    );
}

/// Tool output names the original slot behind an `/inputs` file even when
/// the workspace path shares nothing with the slot name; `/work` files,
/// being derived, name none.
///
/// # Panics
/// Panics when a slot is missing, inferred, or attached to a work file.
#[test]
fn workspace_tools_name_original_slots() {
    let workspace = open(vec![Staged {
        path: "/inputs/manual.txt".to_owned(),
        bytes: b"refunds within 30 days".to_vec(),
        format: Some(MediaFormat::Text),
        sources: BTreeSet::from([SlotName::new("policy").expect("slot")]),
    }])
    .expect("materializes");
    std::fs::write(workspace.work_dir().join("copy.txt"), "refunds noted").expect("work");
    assert_eq!(
        workspace.read("/inputs/manual.txt", 0).expect("read")["sources"],
        json!(["policy"])
    );
    assert_eq!(
        workspace.list("/inputs").expect("list")["files"][0]["sources"],
        json!(["policy"])
    );
    let found = workspace.search("refunds").expect("search");
    assert_eq!(found["matches"][0]["sources"], json!(["policy"]));
    assert_eq!(found["matches"][1]["sources"], json!([]));
    assert_eq!(
        workspace.read("/work/copy.txt", 0).expect("read")["sources"],
        json!([])
    );
}

/// A unit longer than the result limit is read whole by following `next`:
/// its pieces keep the original locator, valid UTF-8, and every character,
/// and each response's actual JSON size fits even when escaping inflates
/// quote- and control-heavy text.
///
/// # Panics
/// Panics when text is lost, a locator changes, or a response overflows.
#[test]
fn workspace_reads_long_units_in_pieces() {
    let max = 512;
    let line = "\"\u{1}é\\".repeat(400);
    let workspace = Workspace::materialize(
        vec![staged(
            "/inputs/long.txt",
            format!("{line}\nshort").into_bytes(),
            MediaFormat::Text,
        )],
        HostedLimits {
            tool_result_max_bytes: max,
            ..HostedLimits::default()
        },
        None,
    )
    .expect("materializes");
    let mut text = String::new();
    let mut start = 0;
    let mut reads = 0;
    loop {
        let read = workspace.read("/inputs/long.txt", start).expect("read");
        assert!(read.to_string().len() <= max, "{}", read.to_string().len());
        for unit in read["units"].as_array().expect("units") {
            if unit["locator"] == loc(Locator::Lines { start: 1, end: 1 }) {
                text.push_str(unit["text"].as_str().expect("text"));
            } else {
                assert_eq!(unit["text"], "short");
            }
        }
        reads += 1;
        match read["next"].as_u64() {
            Some(next) => start = usize::try_from(next).expect("index"),
            None => break,
        }
    }
    assert_eq!(text, line);
    assert!(reads > 1);

    let cramped = Workspace::materialize(
        vec![staged(
            &format!("/inputs/{}.txt", "a".repeat(64)),
            b"x".to_vec(),
            MediaFormat::Text,
        )],
        HostedLimits {
            tool_result_max_bytes: 96,
            ..HostedLimits::default()
        },
        None,
    )
    .expect("materializes");
    assert!(
        cramped
            .read(&format!("/inputs/{}.txt", "a".repeat(64)), 0)
            .is_err()
    );
}

/// Listing and searching a `/work` tree of many empty files stop at the
/// result limit and say so, instead of collecting the whole tree.
///
/// # Panics
/// Panics when a result overflows or hides its truncation.
#[test]
fn workspace_results_stop_at_the_limit() {
    let max = 1024;
    let workspace = Workspace::materialize(
        Vec::new(),
        HostedLimits {
            tool_result_max_bytes: max,
            ..HostedLimits::default()
        },
        None,
    )
    .expect("materializes");
    for index in 0..2_000 {
        let body = if index % 2 == 0 { "needle" } else { "" };
        std::fs::write(workspace.work_dir().join(format!("f{index}.txt")), body)
            .expect("work file");
    }
    for result in [
        workspace.list("/work").expect("list"),
        workspace.search("needle").expect("search"),
    ] {
        assert!(result.to_string().len() <= max);
        assert_eq!(result["truncated"], true);
    }
}

/// Archives unpack under their own path, typed by extension; links,
/// escaping entries, and bombs are refused.
///
/// # Panics
/// Panics when an unsafe archive materializes.
#[test]
fn workspace_unpacks_archives_safely() {
    let zip = ooxml(&[
        ("docs/readme.md", "# Title\nbody"),
        ("raw.bin", "\u{0}\u{1}"),
    ]);
    let workspace = open(vec![staged("/inputs/bundle.zip", zip, MediaFormat::Zip)]).expect("zip");
    assert_eq!(
        workspace
            .read("/inputs/bundle.zip/docs/readme.md", 0)
            .expect("md")["units"][0]["text"],
        "# Title"
    );
    let tar = targz(&[(b"a/notes.txt", tar::EntryType::Regular, b"hello")]);
    let workspace = open(vec![staged("/inputs/b.tar.gz", tar, MediaFormat::TarGz)]).expect("tar");
    assert!(
        workspace
            .inputs_dir()
            .join("b.tar.gz/a/notes.txt")
            .is_file()
    );

    let refused = [
        staged(
            "/inputs/x.zip",
            ooxml(&[("../evil.txt", "x")]),
            MediaFormat::Zip,
        ),
        staged(
            "/inputs/x.tar.gz",
            targz(&[(b"../evil.txt", tar::EntryType::Regular, b"x")]),
            MediaFormat::TarGz,
        ),
        staged(
            "/inputs/x.tar.gz",
            targz(&[(b"link", tar::EntryType::Symlink, b"")]),
            MediaFormat::TarGz,
        ),
        staged(
            "/inputs/x.tar.gz",
            targz(&[(
                b"zeros.txt",
                tar::EntryType::Regular,
                &vec![b'0'; 4 * 1024 * 1024],
            )]),
            MediaFormat::TarGz,
        ),
    ];
    for file in refused {
        let error = open(vec![file]).err().expect("refused");
        assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
    }
}

/// Paths outside `/inputs` or `/work`, traversal, binary reads, links
/// under `/work`, and the scratch budget are refused.
///
/// # Panics
/// Panics when a refused path or budget is accepted.
#[test]
fn workspace_refuses_bad_paths_and_limits() {
    for path in [
        "/inputs/../x.txt",
        "/etc/x.txt",
        "/inputs//x.txt",
        "/inputs/./x.txt",
    ] {
        assert!(
            open(vec![staged(path, b"x".to_vec(), MediaFormat::Text)]).is_err(),
            "{path}"
        );
    }
    let limits = HostedLimits {
        scratch_max_bytes: 4,
        ..HostedLimits::default()
    };
    assert!(
        Workspace::materialize(
            vec![staged(
                "/inputs/a.txt",
                b"hello".to_vec(),
                MediaFormat::Text
            )],
            limits,
            None
        )
        .is_err()
    );

    let workspace = open(vec![staged(
        "/inputs/img.png",
        b"\x89PNG\r\n\x1a\n".to_vec(),
        MediaFormat::Png,
    )])
    .expect("binary materializes");
    assert!(workspace.read("/inputs/img.png", 0).is_err());
    std::os::unix::fs::symlink("/etc/hostname", workspace.work_dir().join("link.txt"))
        .expect("link");
    assert!(workspace.read("/work/link.txt", 0).is_err());
    assert!(workspace.read("/work/../inputs/img.png", 0).is_err());
    assert!(workspace.list("/etc").is_err());
}

/// Dropping a workspace removes its directory and returns its admission.
///
/// # Panics
/// Panics when the directory or permit outlives the workspace.
#[tokio::test]
async fn workspace_cleans_up_on_drop() {
    let root = tempfile::tempdir().expect("temp dir");
    let storage = Arc::new(StorageHandle::new(
        LocalSigner::new(root.path().to_path_buf()).expect("local signer"),
    ));
    let limits = HostedLimits {
        max_workspaces: 1,
        ..HostedLimits::default()
    };
    let evidence = TenantEvidence::new(
        storage,
        wyrd_spec::DataTenantId::new_v7(),
        BTreeMap::new(),
        limits,
    );
    let workspaces = Workspaces::new(limits);
    let files = BTreeMap::from([(
        "/inputs/a.json".to_owned(),
        CallInput {
            value: CallValue::Json(json!({"a": 1})),
            sources: BTreeSet::new(),
        },
    )]);

    let workspace = workspaces.open(&files, &evidence).await.expect("opens");
    let dir = workspace.inputs_dir();
    assert!(dir.join("a.json").is_file());
    assert_eq!(workspaces.permits.available_permits(), 0);
    drop(workspace);
    assert!(!dir.exists());
    assert_eq!(workspaces.permits.available_permits(), 1);
}

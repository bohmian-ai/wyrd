//! Workspaces stay inside their directory, inside their limits, serve the
//! actual lines of their files, and extract only on request.

use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::json;
use wyrd_spec::verification::execution::Locator;
use zip::write::SimpleFileOptions;

use super::*;

/// The stable code and repair rule of a refused tool call.
///
/// # Panics
/// Panics when `result` succeeded.
fn refusal(result: Result<Value, ToolError>) -> (String, Value) {
    let error = result.expect_err("refused");
    let rule = error
        .safe_details()
        .map_or(Value::Null, |details| details["rule"].clone());
    (error.code(), rule)
}

/// The expected refusal for repair `rule`.
fn invalid(rule: &str) -> (String, Value) {
    ("WYRD_SPEC_400_VALIDATION".to_owned(), json!(rule))
}

/// A stored zip archive holding `parts`.
///
/// # Panics
/// Panics when the in-memory zip cannot be written.
fn zipped(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("start entry");
        writer.write_all(body.as_bytes()).expect("write entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

/// One staged file at `path`, sourced from the `evidence` slot.
///
/// # Panics
/// Never; the slot name is valid.
fn staged(path: &str, bytes: Vec<u8>, format: MediaFormat) -> Staged {
    Staged {
        path: path.to_owned(),
        bytes,
        format: Some(format),
        sources: BTreeSet::from([SlotName::new("evidence").expect("slot")]),
    }
}

/// `locator` as its wire JSON.
///
/// # Panics
/// Panics when the locator does not serialize.
fn loc(locator: Locator) -> Value {
    serde_json::to_value(locator).expect("locator serializes")
}

/// A literal, case-insensitive `fs.search` for `query` over every file.
fn query(query: &str) -> Search {
    Search {
        query: query.to_owned(),
        ..Search::default()
    }
}

/// The fixture document bytes of `name`.
///
/// # Panics
/// Panics when the fixture is missing.
fn sample(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../../fixtures/cards/verifier_building_blocks/samples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture")
}

/// Materialize `files` under default limits.
///
/// # Errors
/// Returns [`Workspace::materialize`]'s refusal.
fn open(files: Vec<Staged>) -> Result<Workspace, BlockError> {
    Workspace::materialize(files, HostedLimits::default(), None)
}

/// A gzip tar holding `entries`, each a raw header name, type, and body.
///
/// # Panics
/// Panics when a name exceeds the header or the archive cannot be written.
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

/// `fs.read` serves a file's actual lines unparsed, refuses directories
/// and binary documents, and `fs.extract` writes the extraction to a new
/// `/work` file that `fs.read` then serves, never overwriting one.
///
/// # Panics
/// Panics when a file is parsed on read, a refusal is accepted, or an
/// extraction output is lost or overwritten.
#[test]
fn workspace_reads_lines_and_extracts_on_request() {
    let workspace = open(vec![
        staged("/inputs/deck.pdf", sample("sample.pdf"), MediaFormat::Pdf),
        staged(
            "/inputs/memo.docx",
            sample("sample.docx"),
            MediaFormat::Docx,
        ),
        staged(
            "/inputs/data.json",
            br#"{"total": 42}"#.to_vec(),
            MediaFormat::Json,
        ),
        staged(
            "/inputs/policy/refunds.md",
            b"# Refunds\nwithin 30 days\n".to_vec(),
            MediaFormat::Markdown,
        ),
    ])
    .expect("materializes");

    assert_eq!(
        workspace.read("/inputs/data.json", 0).expect("json")["units"],
        json!([{"locator": loc(Locator::Lines { start: 1, end: 1 }), "text": r#"{"total": 42}"#}])
    );
    assert_eq!(
        workspace
            .read("/inputs/policy/refunds.md", 1)
            .expect("markdown")["units"],
        json!([{"locator": loc(Locator::Lines { start: 2, end: 2 }), "text": "within 30 days"}])
    );
    for (refused, rule) in [
        ("/inputs/memo.docx", "utf8_text"),
        ("/inputs/policy", "regular_file"),
        ("/inputs", "regular_file"),
        ("/work", "regular_file"),
        ("/inputs/missing.md", "workspace_missing"),
    ] {
        assert_eq!(
            refusal(workspace.read(refused, 0)),
            invalid(rule),
            "{refused}"
        );
    }

    let extracted = workspace
        .extract("/inputs/deck.pdf", "/work/deck.json")
        .expect("pdf extracts");
    assert_eq!(
        extracted,
        json!({"output_path": "/work/deck.json", "sources": ["evidence"], "extractor_version": EXTRACT_VERSION})
    );
    let output: Value = serde_json::from_slice(
        &std::fs::read(workspace.work_dir().join("deck.json")).expect("output"),
    )
    .expect("output is JSON");
    assert_eq!(output["pages"][0]["text"], "Refunds");
    assert!(
        workspace.read("/work/deck.json", 0).expect("output reads")["units"]
            .as_array()
            .is_some_and(|units| !units.is_empty())
    );
    assert!(
        workspace
            .extract("/inputs/memo.docx", "/work/nested/memo.json")
            .is_ok()
    );
    for (path, output, rule) in [
        ("/inputs/deck.pdf", "/work/deck.json", "output_exists"),
        ("/inputs/deck.pdf", "/inputs/deck.json", "output_path"),
        ("/inputs/deck.pdf", "/work/../deck.json", "output_path"),
        ("/inputs/policy", "/work/policy.json", "regular_file"),
        (
            "/inputs/missing.pdf",
            "/work/missing.json",
            "workspace_missing",
        ),
    ] {
        assert_eq!(
            refusal(workspace.extract(path, output)),
            invalid(rule),
            "{path} -> {output}"
        );
    }
    let mut corrupt = staged(
        "/inputs/bad.pdf",
        b"%PDF-1.4 truncated".to_vec(),
        MediaFormat::Pdf,
    );
    corrupt.format = None;
    let workspace = open(vec![corrupt]).expect("materializes");
    assert_eq!(
        refusal(workspace.extract("/inputs/bad.pdf", "/work/bad.json")),
        invalid("extraction_refused")
    );
    assert!(!workspace.work_dir().join("bad.json").exists());
    assert_eq!(
        std::fs::read_dir(workspace.root.path())
            .expect("root")
            .count(),
        2,
        "a refused extraction leaves no temporary file"
    );
}

/// `fs.search` returns one hit per matching line in path then line order,
/// literal and case-insensitive by default, with regex, case, path, and
/// glob controls; hidden files are searched and binary ones skipped. A
/// regex spanning a long line still yields its hit with a bounded snippet.
///
/// # Panics
/// Panics when a hit is missing, extra, misordered, or a bad pattern is
/// accepted.
#[test]
fn workspace_search_matches_lines() {
    let workspace = open(vec![
        staged(
            "/inputs/policy/refunds.md",
            b"Refunds within 30 days\nno refund after 60 days\n".to_vec(),
            MediaFormat::Markdown,
        ),
        staged(
            "/inputs/policy/.hidden.txt",
            b"REFUND hidden".to_vec(),
            MediaFormat::Text,
        ),
        staged(
            "/inputs/other/refunds.md",
            b"refund elsewhere".to_vec(),
            MediaFormat::Markdown,
        ),
        staged(
            "/inputs/memo.docx",
            zipped(&[("refund.txt", "refund")]),
            MediaFormat::Docx,
        ),
    ])
    .expect("materializes");
    let hits = |search: Search| {
        workspace.search(&search).expect("search")["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|hit| {
                format!(
                    "{}:{}",
                    hit["path"].as_str().expect("path"),
                    hit["locator"]["lines"]["start"]
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        hits(query("refund")),
        [
            "/inputs/other/refunds.md:1",
            "/inputs/policy/.hidden.txt:1",
            "/inputs/policy/refunds.md:1",
            "/inputs/policy/refunds.md:2",
        ]
    );
    let found = workspace.search(&query("after")).expect("search");
    assert_eq!(
        found["matches"],
        json!([{
            "path": "/inputs/policy/refunds.md",
            "locator": loc(Locator::Lines { start: 2, end: 2 }),
            "snippet": "no refund after 60 days",
            "sources": ["evidence"],
        }])
    );
    let exact = Search {
        case_sensitive: true,
        ..query("Refund")
    };
    assert_eq!(hits(exact), ["/inputs/policy/refunds.md:1"]);
    let pattern = Search {
        regex: true,
        ..query(r"\d+ days$")
    };
    assert_eq!(
        hits(pattern),
        ["/inputs/policy/refunds.md:1", "/inputs/policy/refunds.md:2"]
    );
    assert_eq!(hits(query("d+ days")), Vec::<String>::new());
    let scoped = |path: &str, glob: Option<&str>| Search {
        path: Some(path.to_owned()),
        glob: glob.map(str::to_owned),
        ..query("refund")
    };
    assert_eq!(hits(scoped("/inputs/policy", None)).len(), 3);
    assert_eq!(hits(scoped("/inputs/policy/refunds.md", None)).len(), 2);
    assert_eq!(hits(scoped("/inputs", Some("*.md"))).len(), 3);
    assert_eq!(hits(scoped("/inputs", Some("policy/*.md"))).len(), 2);
    assert_eq!(hits(scoped("/inputs", Some("**/*.txt"))).len(), 1);
    assert_eq!(hits(scoped("/inputs", Some("*/refunds.md"))).len(), 3);
    assert!(hits(scoped("/inputs", Some("inputs/*.md"))).is_empty());
    let rooted = Search {
        glob: Some("inputs/other/*".to_owned()),
        ..query("refund")
    };
    assert_eq!(hits(rooted), ["/inputs/other/refunds.md:1"]);
    for (refused, rule) in [
        (
            Search {
                regex: true,
                ..query("(")
            },
            "search_pattern",
        ),
        (
            Search {
                glob: Some("/inputs/*".to_owned()),
                ..query("refund")
            },
            "search_glob",
        ),
        (
            Search {
                glob: Some("../*".to_owned()),
                ..query("refund")
            },
            "search_glob",
        ),
        (scoped("/inputs/missing", None), "workspace_missing"),
        (query(""), "search_query"),
    ] {
        assert_eq!(
            refusal(workspace.search(&refused)),
            invalid(rule),
            "{refused:?}"
        );
    }
    let record = format!("lead\nREFUND {}\n", "é".repeat(35_000));
    let wide = open(vec![staged(
        "/inputs/record.txt",
        record.into_bytes(),
        MediaFormat::Text,
    )])
    .expect("materializes");
    let found = wide
        .search(&Search {
            regex: true,
            ..query("REFUND.*")
        })
        .expect("search");
    let [hit] = found["matches"].as_array().expect("matches").as_slice() else {
        panic!("one bounded hit: {found}");
    };
    assert_eq!(hit["locator"], loc(Locator::Lines { start: 2, end: 2 }));
    assert_eq!(hit["sources"], json!(["evidence"]));
    assert!(
        hit["snippet"]
            .as_str()
            .expect("snippet")
            .starts_with("REFUND é")
    );
    assert!(found.to_string().len() <= HostedLimits::default().tool_result_max_bytes);
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
    let found = workspace.search(&query("refunds")).expect("search");
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
        workspace.search(&query("needle")).expect("search"),
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
    let zip = zipped(&[
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
    let mut fixed = staged(
        "/inputs/kept.zip",
        zipped(&[("a.txt", "a")]),
        MediaFormat::Zip,
    );
    fixed.format = None;
    let workspace = open(vec![fixed]).expect("fixed archive");
    assert!(workspace.inputs_dir().join("kept.zip").is_file());
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
            zipped(&[("../evil.txt", "x")]),
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
    assert_eq!(
        refusal(workspace.read("/inputs/img.png", 0)),
        invalid("utf8_text")
    );
    std::os::unix::fs::symlink("/etc/hostname", workspace.work_dir().join("link.txt"))
        .expect("link");
    assert_eq!(
        refusal(workspace.read("/work/link.txt", 0)),
        invalid("regular_file")
    );
    assert_eq!(
        refusal(workspace.read("/work/../inputs/img.png", 0)),
        invalid("workspace_path")
    );
    assert_eq!(refusal(workspace.list("/etc")), invalid("workspace_path"));
    assert_eq!(
        refusal(workspace.extract("/inputs/img.png", "/work/img.json")),
        invalid("extractable_document")
    );
}

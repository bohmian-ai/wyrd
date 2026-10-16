//! Exact locked outputs for every extractable format, located units, and
//! refusals.

use std::io::Write as _;

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use serde_json::json;
use zip::write::SimpleFileOptions;

use super::*;

/// A stored zip container holding `parts`.
///
/// # Panics
/// Panics when the in-memory zip cannot be written.
pub(crate) fn ooxml(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, body) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("start part");
        writer.write_all(body.as_bytes()).expect("write part");
    }
    writer.finish().expect("finish zip").into_inner()
}

/// A PDF whose pages show `texts`, an empty text drawing nothing.
///
/// # Panics
/// Panics when the in-memory document cannot be encoded.
pub(crate) fn pdf(texts: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
    });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let mut kids = Vec::new();
    for text in texts {
        let mut operations = Vec::new();
        if !text.is_empty() {
            operations = vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![100.into(), 600.into()]),
                Operation::new("Tj", vec![Object::string_literal(*text)]),
                Operation::new("ET", vec![]),
            ];
        }
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));
        kids.push(
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            })
            .into(),
        );
    }
    let count = i64::try_from(kids.len()).expect("page count");
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count, "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save pdf");
    bytes
}

/// A two-slide deck: a titled slide with body text, a merged table, and
/// notes, then an untitled empty slide.
///
/// # Panics
/// Panics when the in-memory zip cannot be written.
pub(crate) fn pptx() -> Vec<u8> {
    /// The namespace declarations every slide part carries.
    const P: &str = r#"xmlns:p="p" xmlns:a="a" xmlns:r="r""#;
    ooxml(&[
        (
            "ppt/presentation.xml",
            &format!(
                r#"<p:presentation {P}><p:sldIdLst><p:sldId id="257" r:id="rId3"/><p:sldId id="256" r:id="rId2"/></p:sldIdLst></p:presentation>"#
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships><Relationship Id="rId2" Target="slides/slide2.xml"/><Relationship Id="rId3" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            &format!(
                r#"<p:sld {P}><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Q3 &amp; refunds</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:nvPr/></p:nvSpPr><p:txBody><a:p><a:r><a:t>Refunds</a:t></a:r><a:r><a:t> rose</a:t></a:r></a:p><a:p/><a:p><a:r><a:t>Second</a:t></a:r></a:p></p:txBody></p:sp>
<p:graphicFrame><a:graphic><a:graphicData><a:tbl>
<a:tr><a:tc gridSpan="2"><a:txBody><a:p><a:r><a:t>Window</a:t></a:r></a:p></a:txBody></a:tc><a:tc hMerge="1"><a:txBody><a:p><a:r><a:t>hidden</a:t></a:r></a:p></a:txBody></a:tc></a:tr>
<a:tr><a:tc><a:txBody><a:p><a:r><a:t>Standard</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>30</a:t></a:r></a:p></a:txBody></a:tc></a:tr>
</a:tbl></a:graphicData></a:graphic></p:graphicFrame>
</p:spTree></p:cSld></p:sld>"#
            ),
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            r#"<Relationships><Relationship Id="rId1" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/notesSlides/notesSlide1.xml",
            &format!(
                r#"<p:notes {P}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr></p:sp><p:sp><p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Say 30 days</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#
            ),
        ),
        (
            "ppt/slides/slide2.xml",
            &format!(r"<p:sld {P}><p:cSld><p:spTree/></p:cSld></p:sld>"),
        ),
    ])
}

/// A document with a styled heading, a merged table, a content control, and
/// an empty paragraph.
///
/// # Panics
/// Panics when the in-memory zip cannot be written.
pub(crate) fn docx() -> Vec<u8> {
    ooxml(&[(
        "word/document.xml",
        r#"<w:document xmlns:w="w"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Refund policy</w:t></w:r></w:p>
<w:tbl>
<w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Window</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>Days</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Standard</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>EU</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr>
</w:tbl>
<w:sdt><w:sdtContent><w:p><w:r><w:t>Tab</w:t><w:tab/><w:t>bed</w:t><w:br/><w:t>line</w:t></w:r></w:p></w:sdtContent></w:sdt>
<w:p/>
<w:sectPr/>
</w:body></w:document>"#,
    )])
}

/// A sparse workbook: strings, a number, a formula error, a boolean, a
/// cached formula result, and a serial date, on two sheets.
///
/// # Panics
/// Panics when the in-memory zip cannot be written.
pub(crate) fn xlsx() -> Vec<u8> {
    /// The SpreadsheetML namespace declarations every sheet part carries.
    const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    ooxml(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            &format!(
                r#"<workbook {NS}><sheets><sheet name="Refunds" sheetId="1" r:id="rId1"/><sheet name="Empty" sheetId="2" r:id="rId2"/></sheets></workbook>"#
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            &format!(
                r#"<worksheet {NS}><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>Days</t></is></c></row>
<row r="2"><c r="B2"><v>30</v></c></row>
<row r="3"><c r="C3" t="e"><f>1/0</f><v>#DIV/0!</v></c></row>
<row r="4"><c r="D4" t="b"><v>1</v></c><c r="F4"><f>SUM(1,2)</f><v>3</v></c></row>
<row r="5"><c r="E5"><v>45000.5</v></c></row>
</sheetData></worksheet>"#
            ),
        ),
        (
            "xl/worksheets/sheet2.xml",
            &format!(r"<worksheet {NS}><sheetData/></worksheet>"),
        ),
    ])
}

/// Extract under default limits.
///
/// # Errors
/// Returns the extractor's [`BlockError`] for refused or malformed input.
fn extract(bytes: &[u8], format: MediaFormat) -> Result<ExtractOutput, BlockError> {
    Extraction::new(&HostedLimits::default()).run(bytes, format)
}

/// The JSON of a successful extraction.
///
/// # Panics
/// Panics when extraction fails.
fn extracted(bytes: &[u8], format: MediaFormat) -> Value {
    serde_json::to_value(extract(bytes, format).expect("extracts")).expect("serializes")
}

/// PDF text is per page, one-based, with empty pages kept as `""`.
///
/// # Panics
/// Panics when the output differs.
#[test]
fn pdf_extracts_pages() {
    assert_eq!(
        extracted(&pdf(&["Refunds within 30 days", ""]), MediaFormat::Pdf),
        json!({"format":"pdf","pages":[{"number":1,"text":"Refunds within 30 days"},{"number":2,"text":""}]})
    );
}

/// PPTX slides follow presentation order, separate the title, keep merged
/// table positions as `""`, and read body-placeholder notes.
///
/// # Panics
/// Panics when the output differs.
#[test]
fn pptx_extracts_slides() {
    assert_eq!(
        extracted(&pptx(), MediaFormat::Pptx),
        json!({"format":"pptx","slides":[
            {"number":1,"title":"Q3 & refunds","paragraphs":["Refunds rose","Second"],"tables":[{"rows":[["Window",""],["Standard","30"]]}],"notes":"Say 30 days"},
            {"number":2,"title":null,"paragraphs":[],"tables":[],"notes":""}
        ]})
    );
}

/// DOCX blocks keep body order, style IDs, content-control paragraphs,
/// tabs and breaks, empty paragraphs, and merged-cell blanks.
///
/// # Panics
/// Panics when the output differs.
#[test]
fn docx_extracts_ordered_blocks() {
    assert_eq!(
        extracted(&docx(), MediaFormat::Docx),
        json!({"format":"docx","blocks":[
            {"kind":"paragraph","text":"Refund policy","style":"Heading1"},
            {"kind":"table","rows":[["Window","","Days"],["Standard","EU",""]]},
            {"kind":"paragraph","text":"Tab\tbed\nline","style":null},
            {"kind":"paragraph","text":"","style":null}
        ]})
    );
}

/// XLSX cells keep sheet order, sparse addresses, cached formula results,
/// error cells as `null` values, and serial dates as numbers.
///
/// # Panics
/// Panics when the output differs.
#[test]
fn xlsx_extracts_sparse_cells() {
    assert_eq!(
        extracted(&xlsx(), MediaFormat::Xlsx),
        json!({"format":"xlsx","sheets":[
            {"name":"Refunds","cells":[
                {"address":"A1","value":"Days","formula":null,"error":null},
                {"address":"B2","value":30,"formula":null,"error":null},
                {"address":"C3","value":null,"formula":"=1/0","error":"#DIV/0!"},
                {"address":"D4","value":true,"formula":null,"error":null},
                {"address":"F4","value":3,"formula":"=SUM(1,2)","error":null},
                {"address":"E5","value":45000.5,"formula":null,"error":null}
            ]},
            {"name":"Empty","cells":[]}
        ]})
    );
}

/// CSV keeps every row with no header or type inference; JSON keeps the
/// parsed value; text formats keep source text, including empty text.
///
/// # Panics
/// Panics when an output differs.
#[test]
fn text_formats_keep_source() {
    assert_eq!(
        extracted(b"a,b\n1,\"x,y\"\n3\n", MediaFormat::Csv),
        json!({"format":"csv","rows":[["a","b"],["1","x,y"],["3"]]})
    );
    assert_eq!(
        extracted(br#"{"nested":[1,null]}"#, MediaFormat::Json),
        json!({"format":"json","value":{"nested":[1,null]}})
    );
    assert_eq!(
        extracted(b"", MediaFormat::Text),
        json!({"format":"text","text":""})
    );
    assert_eq!(
        extracted(b"# Title", MediaFormat::Markdown),
        json!({"format":"markdown","text":"# Title"})
    );
    assert_eq!(
        extracted(b"<p>x</p>", MediaFormat::Html),
        json!({"format":"html","text":"<p>x</p>"})
    );
}

/// Units locate pages, slides, lines, and canonical JSON pointers.
///
/// # Panics
/// Panics when a unit is missing.
#[test]
fn units_locate_extracted_text() {
    let docx = extract(&docx(), MediaFormat::Docx).expect("docx");
    let located = units(&docx, b"");
    assert!(located.contains(&(
        Locator::JsonPointer("/blocks/0/text".to_owned()),
        "Refund policy".to_owned()
    )));
    assert!(located.contains(&(
        Locator::JsonPointer("/blocks/1/rows/1/1".to_owned()),
        "EU".to_owned()
    )));
    let xlsx = extract(&xlsx(), MediaFormat::Xlsx).expect("xlsx");
    assert!(units(&xlsx, b"").contains(&(
        Locator::JsonPointer("/sheets/0/cells/1/value".to_owned()),
        "30".to_owned()
    )));
    let pptx = extract(&pptx(), MediaFormat::Pptx).expect("pptx");
    assert_eq!(units(&pptx, b"")[1], (Locator::Slide(2), String::new()));
    let text = b"one\ntwo";
    let lines = units(&extract(text, MediaFormat::Text).expect("text"), text);
    assert_eq!(
        lines[1],
        (Locator::Lines { start: 2, end: 2 }, "two".to_owned())
    );
}

/// Null, empty, and empty-container values are citable locations, in plain
/// JSON and in canonical Office output; absent members and indices are not.
///
/// # Panics
/// Panics when an existing empty location is rejected or a missing one is
/// accepted.
#[test]
fn empty_values_are_locations() {
    let json = br#"{"answer":null,"comment":"","items":[],"meta":{}}"#;
    let located = units(&extract(json, MediaFormat::Json).expect("json"), json);
    for pointer in ["", "/answer", "/comment", "/items", "/meta"] {
        assert!(
            locates(&located, &Locator::JsonPointer(pointer.to_owned())),
            "{pointer:?}"
        );
    }
    for pointer in ["/missing", "/items/0", "/meta/key"] {
        assert!(
            !locates(&located, &Locator::JsonPointer(pointer.to_owned())),
            "{pointer:?}"
        );
    }
    let empty = b"{}";
    let root = units(&extract(empty, MediaFormat::Json).expect("json"), empty);
    assert!(locates(&root, &Locator::JsonPointer(String::new())));

    let docx = units(&extract(&docx(), MediaFormat::Docx).expect("docx"), b"");
    for pointer in ["/blocks/3/text", "/blocks/1/rows/0/1"] {
        assert!(
            locates(&docx, &Locator::JsonPointer(pointer.to_owned())),
            "{pointer:?}"
        );
    }
    assert!(!locates(
        &docx,
        &Locator::JsonPointer("/blocks/3/style".to_owned())
    ));
    let xlsx = units(&extract(&xlsx(), MediaFormat::Xlsx).expect("xlsx"), b"");
    for pointer in ["/sheets/0/cells/2/value", "/sheets/1/cells"] {
        assert!(
            locates(&xlsx, &Locator::JsonPointer(pointer.to_owned())),
            "{pointer:?}"
        );
    }
}

/// A tiny DOCX whose merged-cell spans exceed Word's column limit, are zero,
/// or are not numbers is refused before any covered cell is allocated.
///
/// # Panics
/// Panics when a malformed span is expanded.
#[test]
fn docx_refuses_impossible_spans() {
    for span in ["18446744073709551615", "0", "x", "64"] {
        let bytes = ooxml(&[(
            "word/document.xml",
            &format!(
                r#"<w:document xmlns:w="w"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:gridSpan w:val="{span}"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl></w:body></w:document>"#
            ),
        )]);
        let error = extract(&bytes, MediaFormat::Docx).expect_err("refused");
        assert!(
            matches!(error, BlockError::Terminal { .. }),
            "{span}: {error}"
        );
    }
    let row: String = (0..2)
        .map(|_| r#"<w:tc><w:tcPr><w:gridSpan w:val="32"/></w:tcPr><w:p/></w:tc>"#)
        .collect();
    let bytes = ooxml(&[(
        "word/document.xml",
        &format!(
            r#"<w:document xmlns:w="w"><w:body><w:tbl><w:tr>{row}</w:tr></w:tbl></w:body></w:document>"#
        ),
    )]);
    assert!(extract(&bytes, MediaFormat::Docx).is_err());
}

/// Legacy Office, images, archives, and bytes of the wrong format are
/// refused as terminal.
///
/// # Panics
/// Panics when a refusal is accepted.
#[test]
fn unsupported_and_wrong_formats_are_refused() {
    for (bytes, format) in [
        (&b"x"[..], MediaFormat::Doc),
        (b"x", MediaFormat::Png),
        (b"x", MediaFormat::Zip),
        (b"not a pdf", MediaFormat::Pdf),
        (b"not a zip", MediaFormat::Docx),
        (b"{", MediaFormat::Json),
        (b"\xff", MediaFormat::Text),
    ] {
        let error = extract(bytes, format).expect_err("refused");
        assert!(
            matches!(error, BlockError::Terminal { .. }),
            "{format}: {error}"
        );
    }
}

/// Page, text, entry, and ratio limits refuse extraction.
///
/// # Panics
/// Panics when a document over a limit is extracted.
#[test]
fn limits_refuse_extraction() {
    let pages = HostedLimits {
        extract_max_pages: 1,
        ..HostedLimits::default()
    };
    assert!(
        Extraction::new(&pages)
            .run(&pdf(&["a", "b"]), MediaFormat::Pdf)
            .is_err()
    );
    assert!(
        Extraction::new(&pages)
            .run(&pptx(), MediaFormat::Pptx)
            .is_err()
    );
    let text = HostedLimits {
        extract_text_max_bytes: 3,
        ..HostedLimits::default()
    };
    assert!(
        Extraction::new(&text)
            .run(b"four", MediaFormat::Text)
            .is_err()
    );
    let entries = HostedLimits {
        archive_max_entries: 2,
        ..HostedLimits::default()
    };
    assert!(
        Extraction::new(&entries)
            .run(&xlsx(), MediaFormat::Xlsx)
            .is_err()
    );
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("word/document.xml", SimpleFileOptions::default())
        .expect("start part");
    writer
        .write_all(&vec![b' '; 1024 * 1024])
        .expect("write part");
    let bomb = writer.finish().expect("finish zip").into_inner();
    let error = extract(&bomb, MediaFormat::Docx).expect_err("ratio limit");
    assert!(error.to_string().contains("limit"), "{error}");
}

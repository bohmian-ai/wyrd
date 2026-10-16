//! The one format extraction path behind `extract` actions and the
//! filesystem tools.
//!
//! [`Extraction::run`] turns pinned original bytes into the locked
//! [`ExtractOutput`] shape. PDF text comes from `lopdf`, XLSX cells from
//! `calamine`, and DOCX/PPTX structure from their OOXML parts read with
//! `zip` and `quick-xml`. Every OOXML container first passes [`zip_guard`],
//! which streams each entry under the archive byte, entry, and ratio limits
//! so no parser sees an archive bomb. [`units`] projects an output into the
//! located text units that `fs.read`, `fs.search`, and citation validation
//! share. Extraction is text and structure only.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use calamine::{Data, Reader as _, Xlsx};
use quick_xml::events::Event;
use serde_json::{Number, Value};
use skald_spec::MediaFormat;
use vala_eval::blocks::BlockError;
use wyrd_spec::card::verifier::extract::{
    CellValue, DocxBlock, ExtractOutput, PdfPage, PptxSlide, Table, XlsxCell, XlsxSheet,
};
use wyrd_spec::verification::execution::Locator;
use zip::ZipArchive;

use super::HostedLimits;

/// The extractor identity recorded as `extract` provenance.
pub const EXTRACT_VERSION: &str = "wyrd-extract/1";

/// Word's table column limit; a DOCX row spanning more is malformed.
const DOCX_MAX_COLUMNS: usize = 63;

/// One extraction under the Deployment limits.
///
/// Owns the running extracted-text total so every string the parsers emit is
/// charged against [`HostedLimits::extract_text_max_bytes`].
pub struct Extraction<'a> {
    /// The limits in force.
    limits: &'a HostedLimits,
    /// Extracted text bytes so far.
    text_bytes: usize,
}

impl<'a> Extraction<'a> {
    /// An extraction under `limits`.
    #[must_use]
    pub const fn new(limits: &'a HostedLimits) -> Self {
        Self {
            limits,
            text_bytes: 0,
        }
    }

    /// Extract `bytes` of `format` into the locked output shape.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-extractable format, a file
    /// that does not parse as `format`, an encrypted PDF, a document above
    /// the page or text limit, or an OOXML container failing [`zip_guard`].
    pub fn run(mut self, bytes: &[u8], format: MediaFormat) -> Result<ExtractOutput, BlockError> {
        match format {
            MediaFormat::Pdf => self.pdf(bytes),
            MediaFormat::Pptx => self.pptx(bytes),
            MediaFormat::Docx => self.docx(bytes),
            MediaFormat::Xlsx => self.xlsx(bytes),
            MediaFormat::Csv => {
                let mut rows = Vec::new();
                let mut reader = csv::ReaderBuilder::new()
                    .has_headers(false)
                    .flexible(true)
                    .from_reader(bytes);
                for record in reader.records() {
                    let record = record.map_err(|error| refused(format, &error))?;
                    rows.push(
                        record
                            .iter()
                            .map(|cell| self.text(cell.to_owned()))
                            .collect::<Result<_, _>>()?,
                    );
                }
                Ok(ExtractOutput::Csv { rows })
            }
            MediaFormat::Json => {
                self.charge(bytes.len())?;
                let value =
                    serde_json::from_slice(bytes).map_err(|error| refused(format, &error))?;
                Ok(ExtractOutput::Json { value })
            }
            MediaFormat::Text => Ok(ExtractOutput::Text {
                text: self.utf8(bytes, format)?,
            }),
            MediaFormat::Markdown => Ok(ExtractOutput::Markdown {
                text: self.utf8(bytes, format)?,
            }),
            MediaFormat::Html => Ok(ExtractOutput::Html {
                text: self.utf8(bytes, format)?,
            }),
            MediaFormat::Doc | MediaFormat::Xls | MediaFormat::Ppt => Err(terminal(format!(
                "{format} is a legacy Office format; convert it to {}x before extracting",
                format.as_str()
            ))),
            other => Err(terminal(format!(
                "{other} has no text extraction; bind images to a native media placeholder \
                 and archives to a workspace path"
            ))),
        }
    }

    /// Charge `len` extracted text bytes.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] once the text limit is exceeded.
    fn charge(&mut self, len: usize) -> Result<(), BlockError> {
        self.text_bytes = self.text_bytes.saturating_add(len);
        if self.text_bytes > self.limits.extract_text_max_bytes {
            return Err(terminal(format!(
                "extracted text exceeds the {}-byte limit",
                self.limits.extract_text_max_bytes
            )));
        }
        Ok(())
    }

    /// Charge and return `text`.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] once the text limit is exceeded.
    fn text(&mut self, text: String) -> Result<String, BlockError> {
        self.charge(text.len())?;
        Ok(text)
    }

    /// Charge and decode UTF-8 source text.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for invalid UTF-8 or text over the
    /// limit.
    fn utf8(&mut self, bytes: &[u8], format: MediaFormat) -> Result<String, BlockError> {
        let text = std::str::from_utf8(bytes).map_err(|error| refused(format, &error))?;
        self.text(text.to_owned())
    }

    /// Refuse a document with more than the page limit.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] above [`HostedLimits::extract_max_pages`].
    fn pages(&self, count: usize, what: &str) -> Result<(), BlockError> {
        if count > self.limits.extract_max_pages {
            return Err(terminal(format!(
                "the document has {count} {what}, above the {}-{what} extraction limit",
                self.limits.extract_max_pages
            )));
        }
        Ok(())
    }

    /// PDF text by page, each page's decoded content bounded by the text
    /// limit.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unparseable or encrypted PDF
    /// or one over the page or text limit.
    fn pdf(&mut self, bytes: &[u8]) -> Result<ExtractOutput, BlockError> {
        let document =
            lopdf::Document::load_mem(bytes).map_err(|error| refused(MediaFormat::Pdf, &error))?;
        if document.is_encrypted() {
            return Err(terminal("encrypted PDFs cannot be extracted".to_owned()));
        }
        let numbers: Vec<u32> = document.get_pages().into_keys().collect();
        self.pages(numbers.len(), "pages")?;
        let mut pages = Vec::with_capacity(numbers.len());
        for number in numbers {
            let text = document
                .extract_text_with_limit(&[number], self.limits.extract_text_max_bytes)
                .map_err(|error| refused(MediaFormat::Pdf, &error))?;
            pages.push(PdfPage {
                number,
                text: self.text(text.trim_end().to_owned())?,
            });
        }
        Ok(ExtractOutput::Pdf { pages })
    }

    /// PPTX slides in presentation order with title, body paragraphs, tables,
    /// and speaker notes.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a container failing
    /// [`zip_guard`], missing or malformed parts, or limits.
    fn pptx(&mut self, bytes: &[u8]) -> Result<ExtractOutput, BlockError> {
        let mut parts = Parts::open(bytes, MediaFormat::Pptx, self.limits)?;
        let presentation = parts.xml("ppt/presentation.xml")?;
        let rels = parts.rels("ppt/presentation.xml")?;
        let ids: Vec<&Node> = presentation.descendants("sldId");
        self.pages(ids.len(), "slides")?;
        let mut slides = Vec::with_capacity(ids.len());
        for (index, id) in ids.into_iter().enumerate() {
            // `sldId` carries both a numeric `id` and the relationship `r:id`.
            let path = id
                .attrs
                .iter()
                .filter(|(key, _)| key == "id")
                .find_map(|(_, rel)| rels.get(rel))
                .ok_or_else(|| refused(MediaFormat::Pptx, &"a slide has no part"))?;
            let slide = parts.xml(path)?;
            let notes = match parts
                .rels(path)?
                .values()
                .find(|target| target.contains("notesSlide"))
            {
                Some(target) => parts
                    .xml(target)?
                    .descendants("sp")
                    .into_iter()
                    .filter(|shape| placeholder(shape) == Some("body"))
                    .flat_map(|shape| shape.descendants("p"))
                    .map(Node::text)
                    .collect::<Vec<_>>()
                    .join("\n"),
                None => String::new(),
            };
            let mut title = None;
            let mut paragraphs = Vec::new();
            for shape in slide.descendants("sp") {
                let texts = shape
                    .descendants("p")
                    .into_iter()
                    .map(Node::text)
                    .filter(|text| !text.is_empty());
                if matches!(placeholder(shape), Some("title" | "ctrTitle")) {
                    title = Some(self.text(texts.collect::<Vec<_>>().join("\n"))?);
                } else {
                    for text in texts {
                        paragraphs.push(self.text(text)?);
                    }
                }
            }
            let tables = slide
                .descendants("tbl")
                .into_iter()
                .map(|table| {
                    let rows = table
                        .children("tr")
                        .map(|row| {
                            row.children("tc")
                                .map(|cell| {
                                    let covered = cell.attr("hMerge") == Some("1")
                                        || cell.attr("vMerge") == Some("1");
                                    self.text(if covered {
                                        String::new()
                                    } else {
                                        cell.paragraphs()
                                    })
                                })
                                .collect::<Result<_, _>>()
                        })
                        .collect::<Result<_, _>>()?;
                    Ok(Table { rows })
                })
                .collect::<Result<_, BlockError>>()?;
            slides.push(PptxSlide {
                number: u32::try_from(index + 1).unwrap_or(u32::MAX),
                title,
                paragraphs,
                tables,
                notes: self.text(notes)?,
            });
        }
        Ok(ExtractOutput::Pptx { slides })
    }

    /// DOCX paragraphs and tables in body order.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a container failing
    /// [`zip_guard`], a missing or malformed document part, or limits.
    fn docx(&mut self, bytes: &[u8]) -> Result<ExtractOutput, BlockError> {
        let document =
            Parts::open(bytes, MediaFormat::Docx, self.limits)?.xml("word/document.xml")?;
        let body = document
            .descendants("body")
            .into_iter()
            .next()
            .ok_or_else(|| refused(MediaFormat::Docx, &"the document has no body"))?;
        let mut blocks = Vec::new();
        self.docx_blocks(body, &mut blocks)?;
        Ok(ExtractOutput::Docx { blocks })
    }

    /// Append the paragraphs and tables under `parent`, descending into
    /// content controls.
    ///
    /// A merged cell's `gridSpan` expands into covered blank cells, so each
    /// row is checked against Word's column limit before expanding.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] once the text limit is exceeded or a
    /// row's spans are invalid or exceed [`DOCX_MAX_COLUMNS`].
    fn docx_blocks(
        &mut self,
        parent: &Node,
        blocks: &mut Vec<DocxBlock>,
    ) -> Result<(), BlockError> {
        for node in parent.nodes() {
            match node.name.as_str() {
                "p" => blocks.push(DocxBlock::Paragraph {
                    text: self.text(node.text())?,
                    style: node
                        .child("pPr")
                        .and_then(|properties| properties.child("pStyle"))
                        .and_then(|style| style.attr("val"))
                        .map(str::to_owned),
                }),
                "tbl" => {
                    let mut rows = Vec::new();
                    for row in node.children("tr") {
                        let mut cells = Vec::new();
                        for cell in row.children("tc") {
                            let properties = cell.child("tcPr");
                            let covered = properties
                                .and_then(|properties| properties.child("vMerge"))
                                .is_some_and(|merge| {
                                    merge.attr("val").is_none_or(|val| val == "continue")
                                });
                            let span = properties
                                .and_then(|properties| properties.child("gridSpan"))
                                .and_then(|span| span.attr("val"))
                                .map_or(Some(1), |val| val.parse::<usize>().ok())
                                .filter(|&span| span >= 1 && cells.len() + span <= DOCX_MAX_COLUMNS)
                                .ok_or_else(|| {
                                    refused(
                                        MediaFormat::Docx,
                                        &format!(
                                            "a table row spans more than {DOCX_MAX_COLUMNS} columns"
                                        ),
                                    )
                                })?;
                            cells.push(self.text(if covered {
                                String::new()
                            } else {
                                cell.paragraphs()
                            })?);
                            cells
                                .extend(std::iter::repeat_n(String::new(), span.saturating_sub(1)));
                        }
                        rows.push(cells);
                    }
                    blocks.push(DocxBlock::Table { rows });
                }
                "sdt" => {
                    if let Some(content) = node.child("sdtContent") {
                        self.docx_blocks(content, blocks)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// XLSX cells by sheet in workbook order, row then column, with cached
    /// values, formulas, and errors.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a container failing
    /// [`zip_guard`], an unreadable workbook, or the text limit.
    fn xlsx(&mut self, bytes: &[u8]) -> Result<ExtractOutput, BlockError> {
        zip_guard(bytes, self.limits)?;
        let mut workbook =
            Xlsx::new(Cursor::new(bytes)).map_err(|error| refused(MediaFormat::Xlsx, &error))?;
        let mut sheets = Vec::new();
        for name in workbook.sheet_names() {
            let values = workbook
                .worksheet_range(&name)
                .map_err(|error| refused(MediaFormat::Xlsx, &error))?;
            let formulas = workbook
                .worksheet_formula(&name)
                .map_err(|error| refused(MediaFormat::Xlsx, &error))?;
            let mut cells: BTreeMap<(u32, u32), CellParts<'_>> = BTreeMap::new();
            let (row0, col0) = values.start().unwrap_or_default();
            for (row, col, value) in values.used_cells() {
                cells.entry(absolute(row0, col0, row, col)).or_default().0 = Some(value);
            }
            let (row0, col0) = formulas.start().unwrap_or_default();
            for (row, col, formula) in formulas.used_cells() {
                cells.entry(absolute(row0, col0, row, col)).or_default().1 = Some(formula);
            }
            let mut out = Vec::with_capacity(cells.len());
            for ((row, col), (value, formula)) in cells {
                let (value, error) = match value {
                    Some(Data::Error(error)) => (None, Some(self.text(error.to_string())?)),
                    Some(data) => (self.cell(data)?, None),
                    None => (None, None),
                };
                out.push(XlsxCell {
                    address: address(row, col),
                    value,
                    formula: formula
                        .map(|formula| self.text(format!("={}", formula.trim_start_matches('='))))
                        .transpose()?,
                    error,
                });
            }
            sheets.push(XlsxSheet {
                name: self.text(name)?,
                cells: out,
            });
        }
        Ok(ExtractOutput::Xlsx { sheets })
    }

    /// The JSON value of one non-error cell; dates stay serial numbers and
    /// integral numbers serialize as integers.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] once the text limit is exceeded.
    fn cell(&mut self, data: &Data) -> Result<Option<CellValue>, BlockError> {
        // Rust prints integral floats without a fraction, so `30.0` parses back
        // as the integer `30` and compares equal to an authored `30`.
        let number = |value: f64| value.to_string().parse().ok().map(CellValue::Number);
        Ok(match data {
            Data::Int(value) => Some(CellValue::Number(Number::from(*value))),
            Data::Float(value) => number(*value),
            Data::DateTime(value) => number(value.as_f64()),
            Data::Bool(value) => Some(CellValue::Boolean(*value)),
            Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
                Some(CellValue::String(self.text(value.clone())?))
            }
            Data::Error(_) | Data::Empty => None,
        })
    }
}

/// The cached value and formula found for one worksheet position.
type CellParts<'w> = (Option<&'w Data>, Option<&'w String>);

/// Refuse an archive above the entry, decompressed-byte, or ratio limit.
///
/// Every entry is streamed through a reader capped at the remaining budget,
/// so a header that understates an entry's size cannot exceed it.
///
/// # Errors
/// Returns [`BlockError::Terminal`] for an unreadable archive or one over a
/// limit.
pub fn zip_guard(bytes: &[u8], limits: &HostedLimits) -> Result<(), BlockError> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| refused(MediaFormat::Zip, &error))?;
    if archive.len() > limits.archive_max_entries {
        return Err(terminal(format!(
            "the archive has {} entries, above the {}-entry limit",
            archive.len(),
            limits.archive_max_entries
        )));
    }
    let compressed = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let budget = limits
        .archive_max_bytes
        .min(compressed.saturating_mul(limits.archive_max_ratio));
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| refused(MediaFormat::Zip, &error))?;
        total += std::io::copy(&mut entry.take(budget - total + 1), &mut std::io::sink())
            .map_err(|error| refused(MediaFormat::Zip, &error))?;
        if total > budget {
            return Err(terminal(format!(
                "the archive decompresses past its {budget}-byte limit"
            )));
        }
    }
    Ok(())
}

/// Located text units of `output`, shared by `fs.read`, `fs.search`, and
/// citation validation.
///
/// PDF pages and PPTX slides are page and slide units; DOCX and XLSX text
/// fields are JSON-pointer units into the canonical output; JSON leaves are
/// JSON-pointer units into the input document itself; text formats and CSV
/// are one-line units of `source`.
#[must_use]
pub fn units(output: &ExtractOutput, source: &[u8]) -> Vec<(Locator, String)> {
    match output {
        ExtractOutput::Pdf { pages } => pages
            .iter()
            .map(|page| (Locator::Page(page.number), page.text.clone()))
            .collect(),
        ExtractOutput::Pptx { slides } => slides
            .iter()
            .map(|slide| {
                let mut text: Vec<&str> = slide.title.iter().map(String::as_str).collect();
                text.extend(slide.paragraphs.iter().map(String::as_str));
                for table in &slide.tables {
                    text.extend(table.rows.iter().flatten().map(String::as_str));
                }
                text.push(&slide.notes);
                (
                    Locator::Slide(slide.number),
                    text.join("\n").trim_end().to_owned(),
                )
            })
            .collect(),
        ExtractOutput::Docx { .. } | ExtractOutput::Xlsx { .. } => {
            let mut units = Vec::new();
            let canonical = serde_json::to_value(output).unwrap_or(Value::Null);
            leaves(&canonical, String::new(), true, &mut units);
            units
        }
        ExtractOutput::Json { value } => {
            let mut units = Vec::new();
            leaves(value, String::new(), false, &mut units);
            units
        }
        ExtractOutput::Csv { .. }
        | ExtractOutput::Text { .. }
        | ExtractOutput::Markdown { .. }
        | ExtractOutput::Html { .. } => String::from_utf8_lossy(source)
            .lines()
            .enumerate()
            .map(|(index, line)| {
                let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
                (
                    Locator::Lines {
                        start: number,
                        end: number,
                    },
                    line.to_owned(),
                )
            })
            .collect(),
    }
}

/// Whether `locator` names a location in `units`.
///
/// Pages and slides must be present; a line range must lie within the line
/// units; a JSON pointer must name a leaf (empty ones included) or a
/// container of leaves; a region
/// must sit on a present page.
#[must_use]
pub fn locates(units: &[(Locator, String)], locator: &Locator) -> bool {
    match locator {
        Locator::Page(_) | Locator::Slide(_) => units.iter().any(|(unit, _)| unit == locator),
        Locator::Region { page, .. } => units.iter().any(|(unit, _)| *unit == Locator::Page(*page)),
        Locator::Lines { start, end } => {
            *start >= 1
                && start <= end
                && units
                    .iter()
                    .any(|(unit, _)| *unit == Locator::Lines { start: *end, end: *end })
        }
        Locator::JsonPointer(pointer) => units.iter().any(|(unit, _)| {
            matches!(unit, Locator::JsonPointer(leaf)
                if leaf == pointer || leaf.strip_prefix(pointer.as_str()).is_some_and(|rest| rest.starts_with('/')))
        }),
    }
}

/// Collect the leaves under `value` as JSON-pointer units, including null,
/// empty strings, and empty containers with empty text so their locations
/// stay citable; with `canonical`, skip the `format` and `kind` discriminants and DOCX
/// styles of extracted output.
fn leaves(value: &Value, pointer: String, canonical: bool, units: &mut Vec<(Locator, String)>) {
    match value {
        Value::Object(map) if map.is_empty() => {
            units.push((Locator::JsonPointer(pointer), String::new()));
        }
        Value::Array(items) if items.is_empty() => {
            units.push((Locator::JsonPointer(pointer), String::new()));
        }
        Value::Object(map) => {
            for (key, child) in map {
                if !canonical || !matches!(key.as_str(), "format" | "kind" | "style") {
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    leaves(child, format!("{pointer}/{escaped}"), canonical, units);
                }
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                leaves(child, format!("{pointer}/{index}"), canonical, units);
            }
        }
        Value::String(text) => units.push((Locator::JsonPointer(pointer), text.clone())),
        Value::Null => units.push((Locator::JsonPointer(pointer), String::new())),
        other => units.push((Locator::JsonPointer(pointer), other.to_string())),
    }
}

/// The parts of one guarded OOXML container.
struct Parts<'a> {
    /// The container.
    archive: ZipArchive<Cursor<&'a [u8]>>,
    /// Format named in refusals.
    format: MediaFormat,
}

impl<'a> Parts<'a> {
    /// Open `bytes` as an OOXML container after [`zip_guard`].
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a container failing the guard.
    fn open(
        bytes: &'a [u8],
        format: MediaFormat,
        limits: &HostedLimits,
    ) -> Result<Self, BlockError> {
        zip_guard(bytes, limits).map_err(|error| match error {
            BlockError::Terminal { reason } => terminal(format!("{format}: {reason}")),
            retryable @ BlockError::Retryable { .. } => retryable,
        })?;
        let archive =
            ZipArchive::new(Cursor::new(bytes)).map_err(|error| refused(format, &error))?;
        Ok(Self { archive, format })
    }

    /// Parse part `name`.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a missing or malformed part.
    fn xml(&mut self, name: &str) -> Result<Node, BlockError> {
        let mut bytes = Vec::new();
        self.archive
            .by_name(name)
            .and_then(|mut part| Ok(part.read_to_end(&mut bytes)?))
            .map_err(|error| refused(self.format, &format!("{name}: {error}")))?;
        Node::parse(&bytes).map_err(|error| refused(self.format, &format!("{name}: {error}")))
    }

    /// The relationship targets of part `name` by ID, as container paths.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a malformed relationships part.
    fn rels(&mut self, name: &str) -> Result<BTreeMap<String, String>, BlockError> {
        let (dir, file) = name.rsplit_once('/').unwrap_or(("", name));
        let path = format!("{dir}/_rels/{file}.rels");
        if self.archive.index_for_name(&path).is_none() {
            return Ok(BTreeMap::new());
        }
        Ok(self
            .xml(&path)?
            .children("Relationship")
            .filter_map(|rel| {
                let target = rel.attr("Target")?;
                let resolved = match target.strip_prefix('/') {
                    Some(absolute) => absolute.to_owned(),
                    None => resolve(dir, target),
                };
                Some((rel.attr("Id")?.to_owned(), resolved))
            })
            .collect())
    }
}

/// `target` relative to container directory `dir`, folding `..` segments.
fn resolve(dir: &str, target: &str) -> String {
    let mut segments: Vec<&str> = dir
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    for segment in target.split('/') {
        match segment {
            ".." => {
                segments.pop();
            }
            "." | "" => {}
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// The placeholder type of a PPTX shape, `body` for an untyped placeholder.
fn placeholder(shape: &Node) -> Option<&str> {
    let ph = shape.descendants("ph").into_iter().next()?;
    Some(ph.attr("type").unwrap_or("body"))
}

/// Absolute worksheet position of a range-relative cell.
fn absolute(row0: u32, col0: u32, row: usize, col: usize) -> (u32, u32) {
    (
        row0 + u32::try_from(row).unwrap_or(u32::MAX - row0),
        col0 + u32::try_from(col).unwrap_or(u32::MAX - col0),
    )
}

/// The A1 address of zero-based `(row, col)`.
fn address(row: u32, col: u32) -> String {
    let mut letters = Vec::new();
    let mut col = col + 1;
    while col > 0 {
        let rem = (col - 1) % 26;
        letters.push(char::from(b'A' + u8::try_from(rem).unwrap_or(0)));
        col = (col - 1) / 26;
    }
    letters.iter().rev().collect::<String>() + &(row + 1).to_string()
}

/// A terminal error with `reason`.
const fn terminal(reason: String) -> BlockError {
    BlockError::Terminal { reason }
}

/// The refusal of a file that does not parse as `format`.
fn refused(format: MediaFormat, error: &dyn std::fmt::Display) -> BlockError {
    terminal(format!("the file is not a readable {format}: {error}"))
}

/// One XML element with namespace prefixes dropped.
#[derive(Debug, Default)]
struct Node {
    /// Local element name.
    name: String,
    /// Attributes by local name.
    attrs: Vec<(String, String)>,
    /// Children in document order.
    children: Vec<Child>,
}

/// One element child.
#[derive(Debug)]
enum Child {
    /// A nested element.
    Node(Node),
    /// Character data with entities resolved.
    Text(String),
}

impl Node {
    /// Parse `bytes` into its root element's tree.
    ///
    /// # Errors
    /// Returns the parser error for malformed XML.
    fn parse(bytes: &[u8]) -> Result<Self, quick_xml::Error> {
        let mut reader = quick_xml::Reader::from_reader(bytes);
        let mut stack = vec![Self::default()];
        loop {
            match reader.read_event()? {
                Event::Start(start) => stack.push(Self::element(&start)),
                Event::Empty(empty) => {
                    let node = Self::element(&empty);
                    push(&mut stack, Child::Node(node));
                }
                Event::End(_) => {
                    if stack.len() > 1
                        && let Some(node) = stack.pop()
                    {
                        push(&mut stack, Child::Node(node));
                    }
                }
                Event::Text(text) => push(&mut stack, Child::Text(text.decode()?.into_owned())),
                Event::CData(data) => push(&mut stack, Child::Text(data.decode()?.into_owned())),
                Event::GeneralRef(reference) => {
                    let raw = format!("&{};", reference.decode()?);
                    let text = quick_xml::escape::unescape(&raw)
                        .map(std::borrow::Cow::into_owned)
                        .unwrap_or_default();
                    push(&mut stack, Child::Text(text));
                }
                Event::Eof => break,
                _ => {}
            }
        }
        let document = stack.into_iter().next().unwrap_or_default();
        Ok(document
            .children
            .into_iter()
            .find_map(|child| match child {
                Child::Node(root) => Some(root),
                Child::Text(_) => None,
            })
            .unwrap_or_default())
    }

    /// An element without children from a start tag.
    fn element(start: &quick_xml::events::BytesStart<'_>) -> Self {
        Self {
            name: String::from_utf8_lossy(start.local_name().as_ref()).into_owned(),
            attrs: start
                .attributes()
                .flatten()
                .map(|attr| {
                    (
                        String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned(),
                        attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map(std::borrow::Cow::into_owned)
                            .unwrap_or_default(),
                    )
                })
                .collect(),
            children: Vec::new(),
        }
    }

    /// Attribute `name`.
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Child elements.
    fn nodes(&self) -> impl Iterator<Item = &Self> {
        self.children.iter().filter_map(|child| match child {
            Child::Node(node) => Some(node),
            Child::Text(_) => None,
        })
    }

    /// Child elements named `name`.
    fn children<'n>(&'n self, name: &'n str) -> impl Iterator<Item = &'n Self> {
        self.nodes().filter(move |node| node.name == name)
    }

    /// The first child element named `name`.
    fn child<'n>(&'n self, name: &'n str) -> Option<&'n Self> {
        self.children(name).next()
    }

    /// Every descendant element named `name`, in document order, not
    /// descending into a match.
    fn descendants(&self, name: &str) -> Vec<&Self> {
        let mut found = Vec::new();
        for node in self.nodes() {
            if node.name == name {
                found.push(node);
            } else {
                found.extend(node.descendants(name));
            }
        }
        found
    }

    /// The run text of a paragraph: `t` text, tabs, and breaks in order.
    fn text(&self) -> String {
        let mut text = String::new();
        self.collect(&mut text);
        text
    }

    /// Append this element's run text to `text`.
    fn collect(&self, text: &mut String) {
        for node in self.nodes() {
            match node.name.as_str() {
                "t" => {
                    for child in &node.children {
                        if let Child::Text(chars) = child {
                            text.push_str(chars);
                        }
                    }
                }
                "tab" => text.push('\t'),
                "br" | "cr" => text.push('\n'),
                _ => node.collect(text),
            }
        }
    }

    /// The paragraphs of a table cell joined by newlines.
    fn paragraphs(&self) -> String {
        self.descendants("p")
            .into_iter()
            .map(Self::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Append `child` to the innermost open element.
fn push(stack: &mut [Node], child: Child) {
    if let Some(top) = stack.last_mut() {
        top.children.push(child);
    }
}

#[cfg(test)]
pub(crate) mod tests;

//! Locked `extract` action output contract.
//!
//! Every extractable [`MediaFormat`] produces exactly one format-discriminated
//! JSON document. Every field is always present: unavailable values are
//! `null` only where the type allows it, empty collections are `[]`, and
//! empty text is `""`. Counts come from array lengths. Extraction is text and
//! structure, never a claim of visual or layout fidelity.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skald_spec::MediaFormat;

/// The output of one `extract` action, discriminated by `format`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtractOutput {
    /// PDF text by page.
    Pdf {
        /// Pages in document order.
        pages: Vec<PdfPage>,
    },
    /// PowerPoint text and tables by slide.
    Pptx {
        /// Slides in presentation order.
        slides: Vec<PptxSlide>,
    },
    /// Word paragraphs and tables in document order.
    Docx {
        /// Blocks in document order.
        blocks: Vec<DocxBlock>,
    },
    /// Excel cells by sheet.
    Xlsx {
        /// Sheets in workbook order.
        sheets: Vec<XlsxSheet>,
    },
    /// Every CSV row, including the first; no header or type inference.
    Csv {
        /// Rows of cell text.
        rows: Vec<Vec<String>>,
    },
    /// The parsed JSON value.
    Json {
        /// Parsed document.
        value: Value,
    },
    /// Plain text source.
    Text {
        /// Source text.
        text: String,
    },
    /// Markdown source text; structure is not parsed.
    Markdown {
        /// Source text.
        text: String,
    },
    /// HTML source text; structure is not parsed.
    Html {
        /// Source text.
        text: String,
    },
}

/// One PDF page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PdfPage {
    /// One-based page number.
    pub number: u32,
    /// Extracted page text.
    pub text: String,
}

/// One PowerPoint slide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PptxSlide {
    /// One-based slide number in presentation order.
    pub number: u32,
    /// Title placeholder text, or `null` when the slide has none.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub title: Option<String>,
    /// Body paragraphs in reading order.
    pub paragraphs: Vec<String>,
    /// Tables on the slide.
    pub tables: Vec<Table>,
    /// Speaker notes, or `""` when absent.
    pub notes: String,
}

/// One extracted table. A merged cell holds its top-left value and every
/// covered position holds `""`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// Rows of cell text.
    pub rows: Vec<Vec<String>>,
}

/// One Word document block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DocxBlock {
    /// A paragraph.
    Paragraph {
        /// Paragraph text.
        text: String,
        /// Stored paragraph style ID, or `null` when unstyled.
        #[serde(deserialize_with = "Option::deserialize")]
        #[schemars(required)]
        style: Option<String>,
    },
    /// A table. Merged cells follow the [`Table`] rule.
    Table {
        /// Rows of cell text.
        rows: Vec<Vec<String>>,
    },
}

/// One Excel worksheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XlsxSheet {
    /// Sheet name.
    pub name: String,
    /// Present cells in row, then column, order. Absent cells are omitted.
    pub cells: Vec<XlsxCell>,
}

/// One present Excel cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XlsxCell {
    /// Worksheet address such as `B2`, not an offset into `cells`.
    pub address: String,
    /// Stored value or cached formula result; formulas are never
    /// recalculated, dates stay Excel serial numbers, and error cells are
    /// `null`.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub value: Option<CellValue>,
    /// Formula text, or `null` for a literal cell.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub formula: Option<String>,
    /// Error text such as `#DIV/0!`, or `null` when the cell holds no error.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schemars(required)]
    pub error: Option<String>,
}

/// A non-null Excel cell value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum CellValue {
    /// Text value.
    String(String),
    /// Numeric value, including serial dates.
    Number(serde_json::Number),
    /// Boolean value.
    Boolean(bool),
}

impl ExtractOutput {
    /// Returns the JSON Schema of the outputs `extract` can produce for
    /// `formats`: the `oneOf` branches whose `format` discriminant is one of
    /// the given extractable formats. Non-extractable formats contribute no
    /// branch.
    #[must_use]
    pub fn schema_for(formats: &[MediaFormat]) -> Value {
        let mut schema = serde_json::to_value(schemars::schema_for!(Self)).unwrap_or(Value::Null);
        if let Some(Value::Array(branches)) = schema.get_mut("oneOf") {
            branches.retain(|branch| {
                branch
                    .pointer("/properties/format/enum/0")
                    .and_then(Value::as_str)
                    .is_some_and(|tag| formats.iter().any(|format| format.as_str() == tag))
            });
        }
        schema
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The locked DOCX and XLSX examples deserialize and serialize unchanged.
    ///
    /// # Panics
    /// Panics when a locked example does not round-trip exactly.
    #[test]
    fn extract_output_round_trips_locked_examples() {
        for example in [
            json!({"format":"pdf","pages":[{"number":1,"text":"Refunds"}]}),
            json!({"format":"pptx","slides":[{"number":1,"title":null,"paragraphs":["a"],"tables":[{"rows":[["x",""]]}],"notes":""}]}),
            json!({"format":"docx","blocks":[{"kind":"paragraph","text":"Refund policy","style":"Heading1"},{"kind":"table","rows":[["Window","Days"],["Standard","30"]]}]}),
            json!({"format":"xlsx","sheets":[{"name":"Refunds","cells":[{"address":"A1","value":"Days","formula":null,"error":null},{"address":"B2","value":30,"formula":null,"error":null},{"address":"C3","value":null,"formula":"=1/0","error":"#DIV/0!"},{"address":"D4","value":true,"formula":null,"error":null}]}]}),
            json!({"format":"csv","rows":[["a","b"],["1","2"]]}),
            json!({"format":"json","value":{"nested":[1,null]}}),
            json!({"format":"text","text":""}),
            json!({"format":"markdown","text":"# Title"}),
            json!({"format":"html","text":"<p>x</p>"}),
        ] {
            let output: ExtractOutput =
                serde_json::from_value(example.clone()).expect("locked example");
            assert_eq!(serde_json::to_value(&output).expect("serialize"), example);
        }
    }

    /// Every field is required and no extra count field is accepted.
    ///
    /// # Panics
    /// Panics when an incomplete or extended output is accepted.
    #[test]
    fn extract_output_refuses_missing_and_extra_fields() {
        for invalid in [
            json!({"format":"pdf","pages":[],"page_count":0}),
            json!({"format":"pptx","slides":[{"number":1,"paragraphs":[],"tables":[],"notes":""}]}),
            json!({"format":"docx","blocks":[{"kind":"paragraph","text":"x"}]}),
            json!({"format":"xlsx","sheets":[{"name":"s","cells":[{"address":"A1","value":1}]}]}),
            json!({"format":"doc","blocks":[]}),
        ] {
            assert!(
                serde_json::from_value::<ExtractOutput>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    /// The per-format schema keeps only the requested extractable branches.
    ///
    /// # Panics
    /// Panics when the schema keeps the wrong branches.
    #[test]
    fn extract_output_schema_selects_formats() {
        let schema = ExtractOutput::schema_for(&[MediaFormat::Pptx, MediaFormat::Png]);
        let branches = schema["oneOf"].as_array().expect("oneOf");
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0]["properties"]["format"]["enum"], json!(["pptx"]));
        assert_eq!(branches[0]["additionalProperties"], json!(false));
        let slide = &schema["definitions"]["PptxSlide"];
        assert!(
            slide["required"]
                .as_array()
                .expect("required")
                .contains(&json!("title"))
        );
    }
}

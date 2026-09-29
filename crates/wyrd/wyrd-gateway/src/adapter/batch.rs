//! Batches lifecycle actions and batch input files.
//!
//! A batch input file is `OpenAI` batch JSONL whose lines all target one
//! endpoint and name one exact `<provider>/<model>` projection, so a batch is
//! authorized, routed, and accounted as a call to that model. Upload rewrites
//! every line's model to the serving deployment's native identifier.

use std::collections::BTreeSet;
use std::fmt::{self, Debug, Formatter};

use serde_json::Value;
use skald_providers::{OpenAiBatchRoute, UploadContent, UploadFile};
use wyrd_spec::gateway::{GatewayContractError, ModelRef};

/// Endpoints a batch line may target.
pub const BATCH_ENDPOINTS: [&str; 3] = ["/v1/chat/completions", "/v1/responses", "/v1/embeddings"];

/// Largest number of requests one batch input file may carry, the `OpenAI`
/// maximum.
const MAX_BATCH_LINES: usize = 50_000;

/// Validated batch input file.
#[derive(Clone, PartialEq, Eq)]
pub struct BatchInput {
    /// Model every line names.
    pub model: ModelRef,
    /// Endpoint every line targets.
    pub endpoint: String,
    /// Request lines in file order.
    lines: Vec<Value>,
}

impl Debug for BatchInput {
    /// Formats the input with its line count instead of its requests.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchInput")
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("lines", &self.lines.len())
            .finish()
    }
}

impl BatchInput {
    /// Parses batch JSONL `bytes`.
    ///
    /// Blank lines are skipped. Every other line is an object with a unique
    /// non-empty `custom_id`, `method` `POST`, a supported `url`, and an object
    /// `body` whose `model` is a projection; all lines share one `url` and one
    /// model.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayContractError`] naming `file` when the bytes are not
    /// UTF-8, carry no request, or carry more than the maximum, and naming the
    /// offending `file[<index>]` member otherwise.
    pub fn parse(bytes: &[u8]) -> Result<Self, GatewayContractError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| GatewayContractError::new("file", "must be UTF-8 JSONL"))?;
        let mut ids = BTreeSet::new();
        let mut shape: Option<(String, ModelRef)> = None;
        let mut lines = Vec::new();
        for raw in text.lines().filter(|line| !line.trim().is_empty()) {
            let index = lines.len();
            if index == MAX_BATCH_LINES {
                return Err(GatewayContractError::new(
                    "file",
                    "carries more than 50000 requests",
                ));
            }
            let invalid = |member: &str, reason| {
                GatewayContractError::new(format!("file[{index}]{member}"), reason)
            };
            let line: Value =
                serde_json::from_str(raw).map_err(|_| invalid("", "must be a JSON object"))?;
            let text_member = |name| line.get(name).and_then(Value::as_str);
            match text_member("custom_id") {
                Some(id) if !id.is_empty() && ids.insert(id.to_owned()) => {}
                _ => return Err(invalid(".custom_id", "must be a unique non-empty string")),
            }
            if text_member("method") != Some("POST") {
                return Err(invalid(".method", "must be POST"));
            }
            let url = text_member("url")
                .filter(|url| BATCH_ENDPOINTS.contains(url))
                .ok_or_else(|| invalid(".url", "must be a supported batch endpoint"))?;
            let model = line
                .get("body")
                .filter(|body| body.is_object())
                .ok_or_else(|| invalid(".body", "must be a JSON object"))?
                .get("model")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(".body.model", "must be <provider>/<model>"))
                .and_then(|model| {
                    ModelRef::from_projection(model)
                        .map_err(|_| invalid(".body.model", "must be <provider>/<model>"))
                })?;
            match &shape {
                None => shape = Some((url.to_owned(), model)),
                Some((first, _)) if first != url => {
                    return Err(invalid(".url", "must match every other line"));
                }
                Some((_, first)) if *first != model => {
                    return Err(invalid(".body.model", "must match every other line"));
                }
                Some(_) => {}
            }
            lines.push(line);
        }
        let (endpoint, model) =
            shape.ok_or_else(|| GatewayContractError::new("file", "carries no request"))?;
        Ok(Self {
            model,
            endpoint,
            lines,
        })
    }

    /// Serializes the lines as JSONL with every body's model set to the
    /// deployment's `native` model identifier.
    fn native(&self, native: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in &self.lines {
            let mut line = line.clone();
            line["body"]["model"] = Value::String(native.to_owned());
            bytes.extend_from_slice(line.to_string().as_bytes());
            bytes.push(b'\n');
        }
        bytes
    }
}

/// One Batches lifecycle action against a provider; ids are the provider's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchAction {
    /// Uploads a batch input file.
    UploadFile {
        /// Client file name.
        filename: String,
        /// Validated requests.
        input: BatchInput,
    },
    /// Reads a file's content.
    FileContent(String),
    /// Deletes a file.
    DeleteFile(String),
    /// Creates a batch from the call body.
    CreateBatch,
    /// Reads a batch.
    RetrieveBatch(String),
    /// Cancels a batch.
    CancelBatch(String),
}

impl BatchAction {
    /// Skald route of the action and, for an upload, its input file rewritten
    /// to the `native` model.
    pub(crate) fn route(&self, native: &str) -> (OpenAiBatchRoute, Option<UploadFile>) {
        match self {
            Self::UploadFile { filename, input } => (
                OpenAiBatchRoute::UploadFile,
                Some(UploadFile {
                    field: "file".to_owned(),
                    filename: filename.clone(),
                    content_type: "application/jsonl".to_owned(),
                    content: UploadContent::Bytes(input.native(native)),
                }),
            ),
            Self::FileContent(id) => (OpenAiBatchRoute::FileContent(id.clone()), None),
            Self::DeleteFile(id) => (OpenAiBatchRoute::DeleteFile(id.clone()), None),
            Self::CreateBatch => (OpenAiBatchRoute::CreateBatch, None),
            Self::RetrieveBatch(id) => (OpenAiBatchRoute::RetrieveBatch(id.clone()), None),
            Self::CancelBatch(id) => (OpenAiBatchRoute::CancelBatch(id.clone()), None),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Batch input file validation and native model rewriting.

    use serde_json::json;

    use skald_providers::UploadContent;

    use super::{BatchAction, BatchInput};

    /// Builds one request line for `url` and `model` with `id`.
    fn line(id: &str, url: &str, model: &str) -> String {
        json!({"custom_id": id, "method": "POST", "url": url, "body": {"model": model, "messages": []}})
            .to_string()
    }

    /// A well-formed file keeps its model, endpoint, and lines, skips blank
    /// lines, and uploads with every model rewritten to the native identifier.
    ///
    /// # Panics
    ///
    /// Panics when parsing fails or the uploaded lines differ.
    #[test]
    fn batch_input_keeps_one_model_and_uploads_native_lines() {
        let file = format!(
            "{}\n\n{}\n",
            line("a", "/v1/chat/completions", "acme/m"),
            line("b", "/v1/chat/completions", "acme/m")
        );
        let input = BatchInput::parse(file.as_bytes()).expect("file parses");
        assert_eq!(input.model.to_string(), "acme/m");
        assert_eq!(input.endpoint, "/v1/chat/completions");
        let (_, upload) = BatchAction::UploadFile {
            filename: "in.jsonl".to_owned(),
            input,
        }
        .route("m-native");
        let upload = upload.expect("an upload carries its file");
        let UploadContent::Bytes(bytes) = upload.content else {
            panic!("a batch input file is uploaded from memory");
        };
        let sent: Vec<serde_json::Value> = String::from_utf8(bytes)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json line"))
            .collect();
        assert_eq!(
            sent.iter()
                .map(|line| &line["body"]["model"])
                .collect::<Vec<_>>(),
            ["m-native", "m-native"]
        );
        assert_eq!(
            (sent[0]["custom_id"].as_str(), sent[1]["custom_id"].as_str()),
            (Some("a"), Some("b"))
        );
    }

    /// Files that are empty, not JSON, or carry a duplicate id, another method,
    /// an unsupported or mixed endpoint, or a missing or mixed model are
    /// rejected with the offending member.
    ///
    /// # Panics
    ///
    /// Panics when a malformed file parses or names the wrong member.
    #[test]
    fn batch_input_rejects_malformed_or_mixed_files() {
        let chat = "/v1/chat/completions";
        let cases = [
            (String::new(), "file"),
            ("{".to_owned(), "file[0]"),
            (
                format!(
                    "{}\n{}",
                    line("a", chat, "acme/m"),
                    line("a", chat, "acme/m")
                ),
                "file[1].custom_id",
            ),
            (
                line("a", chat, "acme/m").replace("POST", "GET"),
                "file[0].method",
            ),
            (line("a", "/v1/images/generations", "acme/m"), "file[0].url"),
            (
                format!(
                    "{}\n{}",
                    line("a", chat, "acme/m"),
                    line("b", "/v1/embeddings", "acme/m")
                ),
                "file[1].url",
            ),
            (line("a", chat, "m"), "file[0].body.model"),
            (
                format!(
                    "{}\n{}",
                    line("a", chat, "acme/m"),
                    line("b", chat, "acme/n")
                ),
                "file[1].body.model",
            ),
        ];
        for (file, field) in cases {
            let error = BatchInput::parse(file.as_bytes()).expect_err(field);
            assert_eq!(error.field, field, "{error}");
        }
    }
}

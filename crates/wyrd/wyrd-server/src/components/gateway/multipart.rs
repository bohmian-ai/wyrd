//! `multipart/form-data` decoding for gateway media and batch uploads.
//!
//! [`FormReader`] reads a request body chunk by chunk under a byte bound, so
//! the same decoder serves bodies the router already buffered and Audio
//! uploads the router passes through unbuffered. Text parts become members of
//! the operation body (repeated names become arrays); parts that carry a file
//! name become uploaded files in submission order, kept in memory or spooled
//! to anonymous temporary files. Part headers, the preamble, and text members
//! stay in memory under fixed bounds, so a spooled upload never does.

use std::sync::Arc;

use axum::body::{Body, BodyDataStream};
use futures_util::StreamExt as _;
use serde_json::{Map, Value, json};
use tokio::io::AsyncWriteExt as _;
use wyrd_gateway::{UploadContent, UploadFile};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::GatewayContractError;

use super::invocation::invalid_request;

/// Media type of a part that declares none.
const DEFAULT_PART_TYPE: &str = "application/octet-stream";

/// Longest boundary RFC 2046 permits.
const MAX_BOUNDARY: usize = 70;

/// Largest header block of one part, in bytes.
const MAX_PART_HEADERS: usize = 16 * 1024;

/// Largest total of preamble and text members one form holds in memory, in
/// bytes.
const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// Why a form could not be decoded.
#[derive(Debug)]
pub(crate) enum FormError {
    /// The form is malformed, or a text member is not UTF-8.
    Invalid(GatewayContractError),
    /// The body exceeds its byte bound, carried here.
    TooLarge(usize),
    /// The body stream broke, such as a caller disconnecting mid-upload, or a
    /// spool file could not be written.
    Unreadable,
}

impl From<FormError> for WyrdError {
    /// Maps a malformed or unreadable form to `GatewayInvalidRequest` and an
    /// oversized one to `PayloadTooLarge`.
    fn from(error: FormError) -> Self {
        match error {
            FormError::Invalid(error) => invalid_request(error),
            FormError::TooLarge(max_bytes) => Self::PayloadTooLarge {
                message: format!("request body exceeds the {max_bytes}-byte limit"),
                details: json!({ "max_bytes": max_bytes }),
            },
            FormError::Unreadable => {
                invalid_request(GatewayContractError::new("body", "could not be read"))
            }
        }
    }
}

/// Where decoded file content goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileSink {
    /// In memory, for bodies already bounded by the router's buffering.
    Memory,
    /// In anonymous temporary files, for unbuffered uploads.
    Spool,
}

/// Bounded reader of one form body.
///
/// It owns the unread body, the bytes read but not yet consumed, and the
/// bounds every step honours: the whole body stays within `limit`, and
/// in-memory text within `text`.
pub(crate) struct FormReader {
    /// Remaining body chunks.
    chunks: BodyDataStream,
    /// Bytes read but not yet consumed.
    buffer: Vec<u8>,
    /// Body bytes read so far.
    read: usize,
    /// Largest body, in bytes.
    limit: usize,
    /// Remaining in-memory text budget, in bytes.
    text: usize,
}

impl FormReader {
    /// Reads `body` of at most `limit` bytes.
    pub(crate) fn new(body: Body, limit: usize) -> Self {
        Self {
            chunks: body.into_data_stream(),
            buffer: Vec::new(),
            read: 0,
            limit,
            text: MAX_TEXT_BYTES,
        }
    }

    /// Decodes the form declared by `content_type`, putting file content in
    /// `sink`.
    ///
    /// Reading stops at the closing boundary; any epilogue is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`FormError::Invalid`] naming `body` when the content type is
    /// not multipart form data with a boundary, or when a part is
    /// unterminated, lacks a `form-data` disposition naming its field, carries
    /// non-UTF-8 or oversized headers, or the preamble and text exceed their
    /// bound; naming the field when a text part is not UTF-8.
    /// [`FormError::TooLarge`] when the body exceeds its limit, and
    /// [`FormError::Unreadable`] when the body or a spool file fails.
    ///
    /// # Cancellation
    ///
    /// Dropping the future drops the body and every spool file written so far.
    pub(crate) async fn decode(
        mut self,
        content_type: Option<&str>,
        sink: FileSink,
    ) -> Result<(Value, Vec<UploadFile>), FormError> {
        let invalid = |reason| FormError::Invalid(GatewayContractError::new("body", reason));
        let boundary = content_type
            .and_then(|value| {
                let (media, parameters) = value.split_once(';')?;
                media
                    .trim()
                    .eq_ignore_ascii_case("multipart/form-data")
                    .then(|| parameter(parameters, "boundary"))?
            })
            .filter(|boundary| !boundary.is_empty() && boundary.len() <= MAX_BOUNDARY)
            .ok_or_else(|| invalid("must be multipart/form-data with a boundary"))?;
        let opening = format!("--{boundary}");
        let delimiter = format!("\r\n--{boundary}");
        let preamble = self.take_until(opening.as_bytes(), self.text).await?;
        self.text -= preamble.len();
        let mut fields = Map::new();
        let mut files = Vec::new();
        loop {
            while self.buffer.len() < 2 {
                self.fill().await?;
            }
            if self.buffer.starts_with(b"--") {
                break;
            }
            if !self.buffer.starts_with(b"\r\n") {
                return Err(invalid("has a malformed boundary line"));
            }
            self.buffer.drain(..2);
            let headers = self.take_until(b"\r\n\r\n", MAX_PART_HEADERS).await?;
            let headers = std::str::from_utf8(&headers)
                .map_err(|_| invalid("has part headers that are not UTF-8"))?;
            let (name, filename, part_type) = disposition(headers).map_err(FormError::Invalid)?;
            if let Some(filename) = filename {
                let content = match sink {
                    FileSink::Memory => UploadContent::Bytes(
                        self.take_until(delimiter.as_bytes(), self.limit).await?,
                    ),
                    FileSink::Spool => self.spool_until(delimiter.as_bytes()).await?,
                };
                files.push(UploadFile {
                    field: name,
                    filename,
                    content_type: part_type.unwrap_or_else(|| DEFAULT_PART_TYPE.to_owned()),
                    content,
                });
                continue;
            }
            let content = self.take_until(delimiter.as_bytes(), self.text).await?;
            self.text -= content.len();
            let text = String::from_utf8(content).map_err(|_| {
                FormError::Invalid(GatewayContractError::new(
                    name.clone(),
                    "must be UTF-8 text",
                ))
            })?;
            match fields.get_mut(&name) {
                Some(Value::Array(values)) => values.push(Value::String(text)),
                Some(first) => *first = Value::Array(vec![first.take(), Value::String(text)]),
                None => {
                    fields.insert(name, Value::String(text));
                }
            }
        }
        Ok((Value::Object(fields), files))
    }

    /// Appends the next body chunk to the buffer.
    ///
    /// # Errors
    ///
    /// Returns [`FormError::Invalid`] when the body ends, since every caller
    /// still awaits a boundary; [`FormError::TooLarge`] once the body exceeds
    /// its limit; and [`FormError::Unreadable`] when the body stream fails.
    async fn fill(&mut self) -> Result<(), FormError> {
        let chunk = match self.chunks.next().await {
            Some(Ok(chunk)) => chunk,
            Some(Err(_)) => return Err(FormError::Unreadable),
            None => {
                return Err(FormError::Invalid(GatewayContractError::new(
                    "body",
                    "ends before its closing boundary",
                )));
            }
        };
        self.read = self.read.saturating_add(chunk.len());
        if self.read > self.limit {
            return Err(FormError::TooLarge(self.limit));
        }
        self.buffer.extend_from_slice(&chunk);
        Ok(())
    }

    /// Consumes and returns the bytes before `needle`, consuming `needle` too.
    ///
    /// # Errors
    ///
    /// Returns [`FormError::Invalid`] naming `body` when more than `cap` bytes
    /// precede `needle`, and the errors of [`Self::fill`].
    async fn take_until(&mut self, needle: &[u8], cap: usize) -> Result<Vec<u8>, FormError> {
        let mut from = 0;
        loop {
            if let Some(end) = find(&self.buffer[from..], needle).map(|offset| from + offset) {
                if end > cap {
                    break;
                }
                let taken = self.buffer[..end].to_vec();
                self.buffer.drain(..end + needle.len());
                return Ok(taken);
            }
            if self.buffer.len() > cap.saturating_add(needle.len()) {
                break;
            }
            from = self.buffer.len().saturating_sub(needle.len() - 1);
            self.fill().await?;
        }
        Err(FormError::Invalid(GatewayContractError::new(
            "body",
            "has a part or text field that is too large",
        )))
    }

    /// Writes the bytes before `needle` to a new anonymous temporary file,
    /// consuming `needle`, and returns the spooled content.
    ///
    /// Only a partial `needle` is held back between chunks, so memory stays
    /// bounded by one chunk.
    ///
    /// # Errors
    ///
    /// Returns [`FormError::Unreadable`] when the file cannot be created or
    /// written, and the errors of [`Self::fill`].
    async fn spool_until(&mut self, needle: &[u8]) -> Result<UploadContent, FormError> {
        let unwritable = |_| FormError::Unreadable;
        let file = tokio::task::spawn_blocking(tempfile::tempfile)
            .await
            .map_err(|_| FormError::Unreadable)?
            .map_err(unwritable)?;
        let mut file = tokio::fs::File::from_std(file);
        let mut len = 0_u64;
        loop {
            if let Some(end) = find(&self.buffer, needle) {
                file.write_all(&self.buffer[..end])
                    .await
                    .map_err(unwritable)?;
                len += end as u64;
                self.buffer.drain(..end + needle.len());
                break;
            }
            let end = self.buffer.len().saturating_sub(needle.len() - 1);
            file.write_all(&self.buffer[..end])
                .await
                .map_err(unwritable)?;
            len += end as u64;
            self.buffer.drain(..end);
            self.fill().await?;
        }
        file.flush().await.map_err(unwritable)?;
        Ok(UploadContent::Spooled {
            file: Arc::new(file.into_std().await),
            len,
        })
    }
}

/// Field name, file name, and content type of a part's `headers`.
///
/// # Errors
///
/// Returns [`GatewayContractError`] naming `body` when a header line is
/// malformed or the part lacks a `form-data` disposition naming its field.
fn disposition(
    headers: &str,
) -> Result<(String, Option<String>, Option<String>), GatewayContractError> {
    let invalid = |reason: &'static str| GatewayContractError::new("body", reason);
    let (mut name, mut filename, mut part_type) = (None, None, None);
    for line in headers.split("\r\n") {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("has a malformed part header"))?;
        if key.trim().eq_ignore_ascii_case("content-disposition") {
            let (kind, parameters) = value.split_once(';').unwrap_or((value, ""));
            if !kind.trim().eq_ignore_ascii_case("form-data") {
                return Err(invalid("has a part that is not form-data"));
            }
            name = parameter(parameters, "name");
            filename = parameter(parameters, "filename");
        } else if key.trim().eq_ignore_ascii_case("content-type") {
            part_type = Some(value.trim().to_owned());
        }
    }
    let name = name.ok_or_else(|| invalid("has a part that names no field"))?;
    Ok((name, filename, part_type))
}

/// Offset of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Value of header parameter `key` in a `;`-separated parameter list.
///
/// Values may be tokens or quoted strings with backslash escapes; the key
/// matches case-insensitively and exactly, so `filename*` never answers for
/// `filename`. Returns `None` when absent or malformed.
fn parameter(parameters: &str, key: &str) -> Option<String> {
    let mut rest = parameters;
    loop {
        rest = rest.trim_start();
        rest = rest.strip_prefix(';').unwrap_or(rest).trim_start();
        if rest.is_empty() {
            return None;
        }
        let (name, after) = rest.split_once('=')?;
        let after = after.trim_start();
        let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
            let mut value = String::new();
            let mut chars = quoted.char_indices();
            let mut end = None;
            while let Some((index, character)) = chars.next() {
                match character {
                    '\\' => value.extend(chars.next().map(|(_, escaped)| escaped)),
                    '"' => {
                        end = Some(index + 1);
                        break;
                    }
                    other => value.push(other),
                }
            }
            (value, &quoted[end?..])
        } else {
            let end = after.find(';').unwrap_or(after.len());
            (after[..end].trim().to_owned(), &after[end..])
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value);
        }
        rest = next;
    }
}

#[cfg(test)]
mod tests {
    //! Streaming multipart form decoding into JSON members and in-memory or
    //! spooled file parts.

    use std::io::{Read as _, Seek as _, SeekFrom};

    use axum::body::Body;
    use serde_json::{Value, json};
    use wyrd_gateway::{UploadContent, UploadFile};

    use super::{FileSink, FormError, FormReader};

    /// Decodes `body` delivered in `chunk`-byte pieces under `limit`.
    async fn read(
        content_type: Option<&str>,
        body: &[u8],
        chunk: usize,
        limit: usize,
        sink: FileSink,
    ) -> Result<(Value, Vec<UploadFile>), FormError> {
        let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
            body.chunks(chunk).map(|piece| Ok(piece.to_vec())).collect();
        FormReader::new(Body::from_stream(futures_util::stream::iter(chunks)), limit)
            .decode(content_type, sink)
            .await
    }

    /// Bytes of a decoded file's content, read back from its spool file.
    ///
    /// # Panics
    ///
    /// Panics when a spool file cannot be read.
    fn bytes(content: &UploadContent) -> Vec<u8> {
        match content {
            UploadContent::Bytes(bytes) => bytes.clone(),
            UploadContent::Spooled { file, len } => {
                let mut file = &**file;
                file.seek(SeekFrom::Start(0)).expect("spool rewinds");
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).expect("spool reads");
                assert_eq!(bytes.len() as u64, *len);
                bytes
            }
        }
    }

    /// Text parts become body members with repeated names as arrays, file
    /// parts keep their field, quoted file name, type, and exact bytes, and a
    /// quoted boundary with a preamble decodes, whether the body arrives whole
    /// or in pieces that split every delimiter, and whether files stay in
    /// memory or spool.
    ///
    /// # Panics
    ///
    /// Panics when a member or file differs from the submitted form, or a
    /// file lands in the wrong sink.
    #[tokio::test]
    async fn form_parts_become_members_and_files() {
        let mut body = b"preamble\r\n--b;1\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nacme/m\r\n--b;1\r\ncontent-disposition: form-data; name=\"t[]\"\r\n\r\nword\r\n--b;1\r\nContent-Disposition: form-data; name=\"t[]\"\r\n\r\nsegment\r\n--b;1\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a;\\\"b\\\".wav\"\r\nContent-Type: audio/wav\r\n\r\n".to_vec();
        body.extend_from_slice(&[0, 13, 10, 45, 45, 255]);
        body.extend_from_slice(b"\r\n--b;1--\r\nepilogue");
        for (chunk, sink) in [
            (body.len(), FileSink::Memory),
            (3, FileSink::Memory),
            (1, FileSink::Spool),
            (5, FileSink::Spool),
        ] {
            let (fields, files) = read(
                Some("multipart/form-data; boundary=\"b;1\""),
                &body,
                chunk,
                body.len(),
                sink,
            )
            .await
            .expect("form decodes");
            assert_eq!(
                fields,
                json!({"model": "acme/m", "t[]": ["word", "segment"]})
            );
            assert_eq!(files.len(), 1);
            assert_eq!(
                (
                    files[0].field.as_str(),
                    files[0].filename.as_str(),
                    files[0].content_type.as_str(),
                    bytes(&files[0].content)
                ),
                (
                    "file",
                    "a;\"b\".wav",
                    "audio/wav",
                    vec![0, 13, 10, 45, 45, 255]
                )
            );
            assert_eq!(
                matches!(files[0].content, UploadContent::Spooled { .. }),
                sink == FileSink::Spool
            );
        }
    }

    /// A body without a multipart content type or boundary, an unterminated
    /// part, a part naming no field, and non-UTF-8 text are rejected with the
    /// offending member.
    ///
    /// # Panics
    ///
    /// Panics when a malformed form decodes or names the wrong member.
    #[tokio::test]
    async fn malformed_forms_are_rejected() {
        let part = |name: &str| {
            format!("--x\r\nContent-Disposition: form-data; {name}\r\n\r\nv\r\n--x--\r\n")
        };
        let cases: [(Option<&str>, Vec<u8>, &str); 6] = [
            (None, part("name=\"a\"").into_bytes(), "body"),
            (
                Some("application/json"),
                part("name=\"a\"").into_bytes(),
                "body",
            ),
            (
                Some("multipart/form-data; boundary=x"),
                b"--x\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nv".to_vec(),
                "body",
            ),
            (
                Some("multipart/form-data; boundary=x"),
                part("filename=\"f\"").into_bytes(),
                "body",
            ),
            (
                Some("multipart/form-data; boundary=x"),
                b"--x\r\nContent-Disposition: attachment; name=\"a\"\r\n\r\nv\r\n--x--".to_vec(),
                "body",
            ),
            (
                Some("multipart/form-data; boundary=x"),
                b"--x\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n\xff\r\n--x--".to_vec(),
                "a",
            ),
        ];
        for (content_type, body, field) in cases {
            match read(content_type, &body, 4, body.len(), FileSink::Spool).await {
                Err(FormError::Invalid(error)) => assert_eq!(error.field, field, "{error}"),
                other => panic!("{field}: {other:?}"),
            }
        }
    }

    /// A body past its limit is refused as too large even when the excess
    /// lies inside a spooled file, and a body stream that breaks mid-upload,
    /// as a disconnecting caller's does, is unreadable.
    ///
    /// # Panics
    ///
    /// Panics when an oversized or broken body decodes or fails differently.
    #[tokio::test]
    async fn oversized_and_broken_bodies_are_refused() {
        let mut body =
            b"--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.wav\"\r\n\r\n"
                .to_vec();
        body.extend_from_slice(&[9; 64]);
        body.extend_from_slice(b"\r\n--x--\r\n");
        let content_type = Some("multipart/form-data; boundary=x");
        assert!(matches!(
            read(content_type, &body, 8, body.len() - 1, FileSink::Spool).await,
            Err(FormError::TooLarge(limit)) if limit == body.len() - 1
        ));
        let broken = Body::from_stream(futures_util::stream::iter([
            Ok(body[..90].to_vec()),
            Err(std::io::Error::other("caller disconnected")),
        ]));
        assert!(matches!(
            FormReader::new(broken, body.len())
                .decode(content_type, FileSink::Spool)
                .await,
            Err(FormError::Unreadable)
        ));
    }
}

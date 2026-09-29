//! SSE and JSONL decoders for provider-native stream event structs.

use serde::de::DeserializeOwned;

use crate::error::{ProviderError, ProviderResult};

/// Decodes Server-Sent Events `data:` lines into native JSON event structs.
pub fn decode_sse_events<T>(provider: &str, bytes: &[u8]) -> ProviderResult<Vec<T>>
where
    T: DeserializeOwned,
{
    let text =
        std::str::from_utf8(bytes).map_err(|error| ProviderError::decode(provider, error))?;
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" || data.is_empty() {
            continue;
        }
        events.push(
            serde_json::from_str(data).map_err(|error| ProviderError::decode(provider, error))?,
        );
    }
    Ok(events)
}

/// Incremental Server-Sent Events decoder yielding each complete event's data.
///
/// Pushed bytes may split anywhere, including inside a line or a UTF-8
/// sequence; only complete lines are interpreted. An event ends at a blank
/// line and its `data:` lines join with newlines. Comments, other fields, and
/// data-less events yield nothing; the `[DONE]` sentinel is yielded like any
/// other data so callers can tell a terminated stream from a truncated one.
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// Bytes of the incomplete trailing line.
    line: Vec<u8>,
    /// Data lines of the event in progress.
    data: Vec<String>,
}

impl SseDecoder {
    /// Pushes `bytes` and returns the data of every event they complete.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Decode`] for a line that is not UTF-8 and
    /// [`ProviderError::Upstream`] once the buffered event in progress, or a
    /// completed event's joined data, exceeds `limit` bytes, so no event above
    /// the bound is emitted or grows without bound.
    pub fn push(
        &mut self,
        provider: &str,
        bytes: &[u8],
        limit: usize,
    ) -> ProviderResult<Vec<String>> {
        let mut events = Vec::new();
        let mut rest = bytes;
        while let Some(end) = rest.iter().position(|byte| *byte == b'\n') {
            self.line.extend_from_slice(&rest[..end]);
            rest = &rest[end + 1..];
            let line = std::mem::take(&mut self.line);
            let line = std::str::from_utf8(&line)
                .map_err(|error| ProviderError::decode(provider, error))?;
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                let joined = self.data.iter().map(String::len).sum::<usize>()
                    + self.data.len().saturating_sub(1);
                if joined > limit {
                    return Err(Self::oversized(provider, limit));
                }
                let data = std::mem::take(&mut self.data).join("\n");
                if !data.is_empty() {
                    events.push(data);
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                let value = value.strip_prefix(' ').unwrap_or(value);
                let joined = self.data.iter().map(String::len).sum::<usize>()
                    + self.data.len()
                    + value.len();
                if joined > limit {
                    return Err(Self::oversized(provider, limit));
                }
                self.data.push(value.to_owned());
            }
        }
        self.line.extend_from_slice(rest);
        if self.line.len() + self.data.iter().map(String::len).sum::<usize>() > limit {
            return Err(Self::oversized(provider, limit));
        }
        Ok(events)
    }

    /// Whether every pushed byte belongs to a completed event: no partial line
    /// and no event in progress, so a caller may append a whole event after
    /// the bytes relayed so far without corrupting one.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.line.is_empty() && self.data.is_empty()
    }

    /// Rejection of an event, completed or in progress, above `limit` bytes.
    fn oversized(provider: &str, limit: usize) -> ProviderError {
        ProviderError::upstream(
            provider,
            200,
            format!("stream event exceeded {limit} bytes"),
        )
    }
}

/// Incremental JSONL decoder that tolerates partial lines between pushes.
#[derive(Debug, Default)]
pub struct JsonlDecoder {
    buffer: String,
}

impl JsonlDecoder {
    /// Pushes bytes into the decoder and returns complete decoded lines.
    pub fn push<T>(&mut self, provider: &str, bytes: &[u8]) -> ProviderResult<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let text =
            std::str::from_utf8(bytes).map_err(|error| ProviderError::decode(provider, error))?;
        self.buffer.push_str(text);

        let mut out = Vec::new();
        while let Some(index) = self.buffer.find('\n') {
            let line = self.buffer[..index].trim().to_owned();
            self.buffer.replace_range(..=index, "");
            if line.is_empty() {
                continue;
            }
            out.push(
                serde_json::from_str(&line)
                    .map_err(|error| ProviderError::decode(provider, error))?,
            );
        }
        Ok(out)
    }

    /// Finishes the stream and decodes any remaining non-empty buffered line.
    pub fn finish<T>(&mut self, provider: &str) -> ProviderResult<Vec<T>>
    where
        T: DeserializeOwned,
    {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return Ok(Vec::new());
        }
        let line = std::mem::take(&mut self.buffer);
        serde_json::from_str(line.trim())
            .map(|value| vec![value])
            .map_err(|error| ProviderError::decode(provider, error))
    }
}

/// Decodes a complete JSONL payload into native event structs.
pub fn decode_jsonl_events<T>(provider: &str, bytes: &[u8]) -> ProviderResult<Vec<T>>
where
    T: DeserializeOwned,
{
    let mut decoder = JsonlDecoder::default();
    let mut out = decoder.push(provider, bytes)?;
    out.extend(decoder.finish(provider)?);
    Ok(out)
}

#[cfg(test)]
mod stream_decode {
    use crate::error::ProviderError;
    use crate::stream::{JsonlDecoder, SseDecoder, decode_jsonl_events, decode_sse_events};
    use skald_spec::{GoogleGenerateContentResponse, OpenAiChatStreamChunk};

    /// Events split across pushes, inside lines and multibyte characters,
    /// decode once complete; comments yield nothing and `[DONE]` is yielded;
    /// the decoder is idle only between events; and both an unterminated
    /// event, an unterminated multi-line event whose inserted separators cross
    /// the bound, and a completed multi-line event past the bound fail.
    ///
    /// # Panics
    ///
    /// Panics when a push within the bound fails, when the decoded events or
    /// idle states differ, or when an event past the bound is not rejected.
    #[test]
    fn sse_decoder_yields_complete_events_across_pushes_within_a_bound() {
        let payload =
            ": keepalive\r\ndata: {\"a\":\"é\"}\r\n\r\ndata: x\ndata: y\n\ndata: [DONE]\n\n";
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        for byte in payload.as_bytes() {
            events.extend(
                decoder
                    .push("openai", std::slice::from_ref(byte), 64)
                    .expect("bounded"),
            );
        }
        assert_eq!(events, ["{\"a\":\"é\"}", "x\ny", "[DONE]"]);
        assert!(decoder.is_idle());
        decoder.push("openai", b"data: x\n", 64).expect("bounded");
        assert!(!decoder.is_idle(), "an event is in progress");
        let mut partial = SseDecoder::default();
        partial.push("openai", b"data", 64).expect("bounded");
        assert!(!partial.is_idle(), "a line is in progress");
        let oversized = SseDecoder::default().push("openai", &[b'x'; 65], 64);
        assert!(oversized.is_err());
        let line = format!("data: {}\n", "x".repeat(40));
        let completed = format!("{line}{line}\n");
        let completed = SseDecoder::default().push("openai", completed.as_bytes(), 64);
        assert!(matches!(completed, Err(ProviderError::Upstream { .. })));
        let mut open = SseDecoder::default();
        let fits = open
            .push("openai", b"data:\ndata:\ndata:\ndata:\ndata:\n", 4)
            .expect("four separators fit");
        assert!(fits.is_empty());
        let crossed = open.push("openai", b"data:\n", 4);
        assert!(matches!(crossed, Err(ProviderError::Upstream { .. })));
    }

    #[test]
    fn sse_parses_data_lines_and_skips_keepalive() {
        let payload = br#": keepalive
data: {"id":"chunk","object":"chat.completion.chunk","created":1,"model":"gpt","choices":[]}
data: [DONE]
"#;

        let events: Vec<OpenAiChatStreamChunk> =
            decode_sse_events("openai", payload).expect("sse decodes");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "chunk");
    }

    #[test]
    fn jsonl_handles_partial_line_buffer() {
        let mut decoder = JsonlDecoder::default();
        let first: Vec<GoogleGenerateContentResponse> = decoder
            .push("google", br#"{"candidates":[]"#)
            .expect("partial line accepted");
        let second: Vec<GoogleGenerateContentResponse> = decoder
            .push("google", br#","usage_metadata":{"total_token_count":1}}"#)
            .expect("line still partial");
        let third: Vec<GoogleGenerateContentResponse> =
            decoder.push("google", b"\n").expect("line decodes");

        assert!(first.is_empty());
        assert!(second.is_empty());
        assert_eq!(third.len(), 1);
    }

    #[test]
    fn jsonl_decodes_complete_payload() {
        let events: Vec<GoogleGenerateContentResponse> =
            decode_jsonl_events("google", br#"{"candidates":[]}"#).expect("jsonl decodes");

        assert_eq!(events.len(), 1);
    }
}

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

//! Unary batch bounds.

/// Hard bounds enforced before a batch enters Scribe.
#[derive(Clone, Debug)]
pub struct IngestLimits {
    /// Maximum size of one decompressed Arrow IPC batch.
    pub max_frame_bytes: usize,
    /// tonic `max_decoding_message_size` (default 4 MiB silently drops large
    /// frames; the server raises it and enforces its own cap instead).
    pub max_decoding_message_size: usize,
}

impl Default for IngestLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 32 * 1024 * 1024,
            max_decoding_message_size: 32 * 1024 * 1024 + 64 * 1024,
        }
    }
}

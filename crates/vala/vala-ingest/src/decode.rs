//! Arrow IPC frame decoding.

use std::io::Cursor;

use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;

use crate::error::IngestError;

/// Decode one Arrow IPC stream frame into its record batches.
///
/// A frame is a self-describing Arrow IPC stream and may carry more than one
/// batch. Decode is synchronous — CPU-bound but small — and runs inside the
/// stream task (no worker pool).
///
/// # Errors
/// Returns [`IngestError::Decode`] when the bytes are not a valid Arrow IPC
/// stream or a contained batch fails to materialize.
pub fn decode_ipc(bytes: &[u8]) -> Result<Vec<RecordBatch>, IngestError> {
    let reader = StreamReader::try_new(Cursor::new(bytes), None)
        .map_err(|error| IngestError::Decode(error.to_string()))?;
    reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| IngestError::Decode(error.to_string()))
}

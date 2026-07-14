//! Scribe contracts per CONTRACTS §5.
//!
//! The `Scribe` trait is the boundary between the Gate (dispatch) and the Scribe
//! (WAL + memtable + seal). Every method is async; every error is `ScribeError`.

use async_trait::async_trait;
use wyrd_runtime::principal::Principal;
use wyrd_spec::ids::DataTenantId;

/// Append request carrying batch data, schema fingerprint, and Principal.
#[derive(Debug, Clone)]
pub struct ScribeAppend {
    pub table_fqn: String,
    pub schema_fingerprint: [u8; 32],
    pub principal: Principal,
    pub batch_data: Vec<u8>, // Placeholder for Arrow IPC bytes
}

/// Append acknowledgment.
#[derive(Debug, Clone)]
pub struct AppendAck {
    pub batch_id: [u8; 16],
    pub tenant: DataTenantId,
}

/// Scribe-layer errors per CONTRACTS §10.
#[derive(Debug, thiserror::Error)]
pub enum ScribeError {
    #[error("ingest busy for table: {table}")]
    IngestBusy { table: String },

    #[error("WAL disk full")]
    WalDiskFull,

    #[error("schema fingerprint mismatch for table: {table}")]
    FingerprintMismatch { table: String },

    #[error("internal scribe failure: {detail}")]
    Internal { detail: String },
}

impl ScribeError {
    /// Map to the corresponding `BifrostError` variant for HTTP/MCP/CLI surfaces.
    pub fn to_bifrost_error(&self) -> wyrd_spec::vala::error::BifrostError {
        use wyrd_spec::vala::error::BifrostError;
        match self {
            Self::IngestBusy { table } => BifrostError::IngestBusy {
                table: table.clone(),
            },
            Self::WalDiskFull => BifrostError::WalDiskFull,
            Self::FingerprintMismatch { table } => BifrostError::FingerprintMismatch {
                table: table.clone(),
            },
            Self::Internal { detail } => BifrostError::Internal {
                detail: detail.clone(),
            },
        }
    }
}

/// Scribe trait — the durable write boundary.
#[async_trait]
pub trait Scribe: Send + Sync {
    /// Append one batch to the WAL. Returns when the batch is durably recorded
    /// in the WAL (not yet sealed to Parquet).
    async fn append(&self, req: ScribeAppend) -> Result<AppendAck, ScribeError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scribe_error_maps_to_wyrd_error_variants() {
        let busy = ScribeError::IngestBusy {
            table: "test.table".to_string(),
        };
        let wal_full = ScribeError::WalDiskFull;
        let mismatch = ScribeError::FingerprintMismatch {
            table: "test.table".to_string(),
        };

        let busy_bifrost = busy.to_bifrost_error();
        let wal_bifrost = wal_full.to_bifrost_error();
        let mismatch_bifrost = mismatch.to_bifrost_error();

        // Verify the mapping produces the expected BifrostError variants
        match busy_bifrost {
            wyrd_spec::vala::error::BifrostError::IngestBusy { table } => {
                assert_eq!(table, "test.table");
            }
            _ => panic!("expected IngestBusy"),
        }

        match wal_bifrost {
            wyrd_spec::vala::error::BifrostError::WalDiskFull => {}
            _ => panic!("expected WalDiskFull"),
        }

        match mismatch_bifrost {
            wyrd_spec::vala::error::BifrostError::FingerprintMismatch { table } => {
                assert_eq!(table, "test.table");
            }
            _ => panic!("expected FingerprintMismatch"),
        }
    }
}

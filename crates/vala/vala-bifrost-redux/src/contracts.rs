//! Scribe contracts per CONTRACTS §5.
//!
//! The `Scribe` trait is the boundary between the Gate (dispatch) and the
//! Scribe (WAL + memtable + seal). Every method is async; every error is
//! `ScribeError`.
//!
//! `Scribe::append` returns `Result<(), ScribeError>` — a durable ack is
//! signaled by `Ok(())`. Idempotency lives on the request via `batch_id`
//! (client-supplied v7 UUID); the 2PC recovery path in `catalog::recovery`
//! keys off it, so there is no `AppendAck` payload to plumb through Gate.

use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use wyrd_runtime::principal::Principal;
use wyrd_spec::request_id::RequestId;

use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::seal_key::TableRef;

/// Append request carrying batch data, schema fingerprint, and Principal.
///
/// Tenant is not a top-level field — it lives on `principal.tenant_id` and
/// Scribe reads it there. Duplicating it on the wire risks a mismatch
/// between the two.
#[derive(Debug, Clone)]
pub struct ScribeAppend {
    /// Full runtime principal — carries subject, tenant, scopes without
    /// re-derivation. Server-verified upstream by Gate.
    pub principal: Principal,
    /// (namespace, `table_name`) of the target Bifrost table.
    pub table: TableRef,
    /// Arrow rows; column layout matches the table's registered schema.
    pub rows: RecordBatch,
    /// Client-computed fingerprint of the source Arrow schema; Scribe
    /// rejects with `FingerprintMismatch` when the catalog value differs.
    pub schema_fingerprint: SchemaFingerprint,
    /// Server-assigned per-request id (Gate mints it on the way in).
    pub request_id: RequestId,
    /// Client-supplied v7 UUID for idempotency. 2PC recovery keys off this.
    pub batch_id: uuid::Uuid,
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

    #[error("object store PUT failed")]
    ObjectStorePutFailed(#[source] opendal::Error),

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
            Self::ObjectStorePutFailed(e) => BifrostError::Internal {
                detail: format!("object store PUT failed: {e}"),
            },
            Self::Internal { detail } => BifrostError::Internal {
                detail: detail.clone(),
            },
        }
    }
}

impl From<vala_sql::SqlError> for ScribeError {
    fn from(e: vala_sql::SqlError) -> Self {
        Self::Internal {
            detail: e.to_string(),
        }
    }
}

/// Scribe trait — the durable write boundary.
///
/// `Ok(())` signals durable-ack (WAL fsynced). Idempotency lives on
/// `req.batch_id`; the 2PC recovery path uses it to dedupe replays.
#[async_trait]
pub trait Scribe: Send + Sync {
    async fn append(&self, req: ScribeAppend) -> Result<(), ScribeError>;
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

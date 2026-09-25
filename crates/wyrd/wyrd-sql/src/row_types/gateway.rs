//! Row mirrors for the tenant gateway administration and Batches tables.
//!
//! JSONB columns decode as [`serde_json::Value`]; `wyrd-server` converts them
//! into the typed `wyrd_spec::gateway` contracts at its boundary.
#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Redacted `wyrd.gateway_provider_credentials` row.
#[derive(Debug, Clone, FromRow)]
pub struct GatewayCredentialRow {
    /// Tenant-unique credential name.
    pub name: String,
    /// Provider the credential authenticates to.
    pub provider: String,
    /// Redacted source view JSON.
    pub source: Value,
    /// `active` or `revoked`.
    pub state: String,
    /// First creation time.
    pub created_at: DateTime<Utc>,
    /// Last mutation time.
    pub updated_at: DateTime<Utc>,
    /// Last active replacement time.
    pub rotated_at: Option<DateTime<Utc>>,
    /// Revocation time.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// The tenant's `wyrd.gateway_policies` documents; `None` is the default.
#[derive(Debug, Clone, Default, FromRow)]
pub struct GatewayPolicyRow {
    /// Fallback policy JSON.
    pub fallback: Option<Value>,
    /// Governance policy JSON without pricing.
    pub governance: Option<Value>,
    /// Versioned capture policy JSON.
    pub capture: Option<Value>,
}

/// One retained `wyrd.gateway_model_pricing` entry.
#[derive(Debug, Clone, FromRow)]
pub struct GatewayPricingRow {
    /// Authoritative eligibility flag.
    pub active: bool,
    /// Complete pricing entry JSON.
    pub entry: Value,
}

/// One statement-consistent read of every gateway configuration table.
///
/// Arrays are JSON so the whole snapshot comes from a single SQL statement and
/// therefore from one MVCC snapshot even under `READ COMMITTED`.
#[derive(Debug, Clone, FromRow)]
pub struct GatewaySnapshotRow {
    /// Deployment JSON documents ordered by name.
    pub deployments: Value,
    /// Credential objects (name, provider, redacted source, state) ordered by
    /// name.
    pub credentials: Value,
    /// Pricing entries with authoritative `active`.
    pub pricing: Value,
    /// Fallback policy JSON.
    pub fallback: Option<Value>,
    /// Governance policy JSON without pricing.
    pub governance: Option<Value>,
    /// Versioned capture policy JSON.
    pub capture: Option<Value>,
}

/// One `wyrd.gateway_batch_files` row: an uploaded batch input file pinned to
/// the deployment that holds it.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct GatewayBatchFileRow {
    /// Tenant-unique file id.
    pub file_id: Uuid,
    /// Client file name.
    pub filename: String,
    /// Uploaded byte length.
    pub size_bytes: i64,
    /// Base64 SHA-256 of the uploaded bytes.
    pub sha256: String,
    /// Uploaded content type.
    pub content_type: String,
    /// Endpoint every request line targets.
    pub endpoint: String,
    /// `<provider>/<model>` projection that served the upload.
    pub model: String,
    /// Deployment holding the file.
    pub deployment: String,
    /// Provider file id.
    pub upstream_file_id: String,
    /// Upload time.
    pub created_at: DateTime<Utc>,
}

/// One `wyrd.gateway_batches` row: a claimed or created batch.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct GatewayBatchRow {
    /// Tenant-unique batch id.
    pub batch_id: Uuid,
    /// Input file id.
    pub file_id: Uuid,
    /// `<provider>/<model>` projection of the batch.
    pub model: String,
    /// Deployment holding the batch.
    pub deployment: String,
    /// Provider batch id; `None` while creation is pending.
    pub upstream_batch_id: Option<String>,
    /// Last observed provider batch object; `None` while creation is pending.
    pub batch: Option<Value>,
}

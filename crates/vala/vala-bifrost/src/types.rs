use serde::{Deserialize, Serialize};
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableScope {
    TenantOwned,
    SystemShared,
}

impl TableScope {
    /// Canonical string stored in `vala.bifrost_tables.scope`.
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::TenantOwned => "tenant_owned",
            Self::SystemShared => "system_shared",
        }
    }

    /// Decode the persisted `scope` string, rejecting unknown values.
    pub fn from_db_str(s: &str) -> Result<Self, BifrostError> {
        match s {
            "tenant_owned" => Ok(Self::TenantOwned),
            "system_shared" => Ok(Self::SystemShared),
            other => Err(BifrostError::MetadataMismatch(format!(
                "unknown table scope: {other}"
            ))),
        }
    }

    /// The control-plane RLS bind for `vala.bifrost_tables` / `vala.olap_commits`
    /// rows. Derived from scope, never from the caller: `SystemShared`
    /// tables register and commit under the privileged `SYSTEM_OWNER`; `TenantOwned`
    /// tables under the authenticated data tenant. This is one of the two distinct
    /// `DataTenantId` values a Bifrost path carries and must never collapse with
    /// the data/stamp tenant below.
    pub fn control_bind(self, data_tenant: DataTenantId) -> DataTenantId {
        match self {
            Self::SystemShared => DataTenantId::SYSTEM_OWNER,
            Self::TenantOwned => data_tenant,
        }
    }

    /// The tenant value server-stamped into the `data_tenant_id` column. Only
    /// `SystemShared` tables carry that column; `TenantOwned` tables are isolated
    /// by catalog namespace and stamp no tenant. This is the authenticated data
    /// tenant — the *only* trusted source of the row tenant value.
    pub fn stamp_tenant(self, data_tenant: DataTenantId) -> Option<DataTenantId> {
        match self {
            Self::SystemShared => Some(data_tenant),
            Self::TenantOwned => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OlapTableKind {
    Bifrost,
    Traces,
    Eval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionTransform {
    Identity,
    Day,
    Month,
    Year,
    Truncate(i32),
    Bucket(i32),
}

impl PartitionTransform {
    pub fn to_iceberg_transform(&self) -> iceberg::spec::Transform {
        match self {
            Self::Identity => iceberg::spec::Transform::Identity,
            Self::Day => iceberg::spec::Transform::Day,
            Self::Month => iceberg::spec::Transform::Month,
            Self::Year => iceberg::spec::Transform::Year,
            Self::Truncate(w) => iceberg::spec::Transform::Truncate((*w).cast_unsigned()),
            Self::Bucket(n) => iceberg::spec::Transform::Bucket((*n).cast_unsigned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableUid(pub [u8; 16]);

impl TableUid {
    pub fn new_v7() -> Self {
        Self(*uuid::Uuid::now_v7().as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Display for TableUid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", uuid::Uuid::from_bytes(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaFingerprint(pub [u8; 32]);

impl SchemaFingerprint {
    pub fn from_arrow_schema(schema: &arrow::datatypes::Schema) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for field in schema.fields() {
            hasher.update(field.name().as_bytes());
            hasher.update(b"\x00");
            hasher.update(format!("{:?}", field.data_type()).as_bytes());
            hasher.update(b"\x00");
        }
        Self(hasher.finalize().into())
    }
}

impl AsRef<[u8]> for SchemaFingerprint {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

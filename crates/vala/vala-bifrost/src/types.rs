use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableScope {
    TenantOwned,
    SystemShared,
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
            Self::Truncate(w) => iceberg::spec::Transform::Truncate(*w as u32),
            Self::Bucket(n) => iceberg::spec::Transform::Bucket(*n as u32),
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

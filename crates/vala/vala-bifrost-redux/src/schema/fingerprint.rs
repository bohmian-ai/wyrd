//! Schema fingerprinting — copied from vala-bifrost.

use arrow::datatypes::Schema;
use sha2::{Digest, Sha256};

/// SHA-256 fingerprint over an Arrow schema's field names and types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemaFingerprint(pub [u8; 32]);

impl SchemaFingerprint {
    /// Compute a fingerprint from an Arrow schema.
    ///
    /// Hashes each field's name and data type in order. This is a content-based
    /// fingerprint; field reordering or type changes produce a different hash.
    pub fn from_arrow_schema(schema: &Schema) -> Self {
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

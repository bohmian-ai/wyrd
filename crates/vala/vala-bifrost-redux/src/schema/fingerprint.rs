//! Schema fingerprinting — copied from vala-bifrost.

use arrow::datatypes::{DataType, Field, Schema};
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
        Self::from_fields(schema.fields())
    }

    /// Compute the same fingerprint from a borrowed field list.
    ///
    /// The catalog registers a built-in from its declared fields while ingest
    /// fingerprints a decoded batch schema. Both must reach the identical bytes
    /// or a canonical write can never match its own table, so both go through
    /// this one function.
    pub fn from_fields<'a>(fields: impl IntoIterator<Item = &'a std::sync::Arc<Field>>) -> Self {
        let mut hasher = Sha256::new();
        for field in fields {
            hasher.update(field.name().as_bytes());
            hasher.update(b"\x00");
            hasher.update(format!("{:?}", strip_metadata(field.data_type())).as_bytes());
            hasher.update(b"\x00");
        }
        Self(hasher.finalize().into())
    }
}

/// Canonical name given to every list element while fingerprinting.
///
/// A list's element field is positional, not addressable, and each writer names
/// it by its own convention (Arrow says `item`, Iceberg says `element`, the
/// canonical ledgers say `event`). Pinning one name keeps a table's identity
/// stable across those round trips.
const LIST_ELEMENT: &str = "element";

/// Return one Arrow type reduced to its fingerprint-significant form.
///
/// Three normalizations apply, each removing a rendering difference that does
/// not change what the table stores:
///
/// - nested field metadata is dropped, because a nested type's `Debug` includes
///   its children's metadata map, whose `HashMap` iteration order is not stable;
///   the stable ids and sensitivity markers it carries are committed by the
///   canonical physical fingerprint instead;
/// - the large and small variable-width types collapse together, because
///   Iceberg has one binary and one string type and a schema read back from it
///   always returns the large form;
/// - a UTC timestamp offset is written as `UTC`, which Iceberg returns as
///   `+00:00`.
///
/// Without these, no canonical signal table could match its own catalog
/// registration after an Iceberg round trip.
fn strip_metadata(data_type: &DataType) -> DataType {
    match data_type {
        DataType::List(child) => DataType::List(std::sync::Arc::new(Field::new(
            LIST_ELEMENT,
            strip_metadata(child.data_type()),
            child.is_nullable(),
        ))),
        DataType::Struct(children) => {
            DataType::Struct(children.iter().map(|child| bare_field(child)).collect())
        }
        DataType::LargeBinary => DataType::Binary,
        DataType::LargeUtf8 => DataType::Utf8,
        DataType::Timestamp(unit, Some(zone)) if zone.as_ref() == "+00:00" => {
            DataType::Timestamp(*unit, Some("UTC".into()))
        }
        other => other.clone(),
    }
}

/// Return one field with no metadata and a fingerprint-normalized nested type.
fn bare_field(field: &Field) -> Field {
    Field::new(
        field.name(),
        strip_metadata(field.data_type()),
        field.is_nullable(),
    )
}

impl AsRef<[u8]> for SchemaFingerprint {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

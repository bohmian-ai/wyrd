//! Schema fingerprinting — copied from vala-bifrost.

use arrow::datatypes::{DataType, Field, Fields, IntervalUnit, Schema, TimeUnit, UnionMode};
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

    /// Compute a fingerprint over the schema exactly as it is spelled.
    ///
    /// [`Self::from_arrow_schema`] answers a catalog question — is this the same
    /// table? — and therefore normalizes the renderings an Iceberg round trip
    /// changes. A reader that has to decode bytes is asking a different
    /// question: are these the same arrays? `Utf8` and `LargeUtf8` are one
    /// Iceberg type but two memory layouts, so a decode-side identity that
    /// collapsed them would accept a schema whose offset width does not match
    /// the file.
    ///
    /// The encoding is a deterministic recursive walk that commits exactly what
    /// changes an Arrow array's buffers: field and child names, their order,
    /// their nullability — a validity buffer the layout either has or does not —
    /// and each type's exact variant and parameters. Field metadata is
    /// deliberately excluded: it changes no buffer, and its `HashMap` iteration
    /// order is not stable, so including it would make equivalent reconstructed
    /// schemas disagree at random.
    #[must_use]
    pub fn from_arrow_schema_exact(schema: &Schema) -> Self {
        let mut hasher = Sha256::new();
        hash_exact_fields(&mut hasher, schema.fields());
        Self(hasher.finalize().into())
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

/// Hash one ordered field list into the exact-layout encoding.
///
/// The count is committed first so a nested boundary cannot be forged by
/// splitting one field list into two.
fn hash_exact_fields(hasher: &mut Sha256, fields: &Fields) {
    hasher.update((fields.len() as u64).to_le_bytes());
    for field in fields {
        hash_exact_field(hasher, field);
    }
}

/// Hash one field's layout-significant identity: name, nullability, and type.
fn hash_exact_field(hasher: &mut Sha256, field: &Field) {
    hasher.update((field.name().len() as u64).to_le_bytes());
    hasher.update(field.name().as_bytes());
    hasher.update([u8::from(field.is_nullable())]);
    hash_exact_type(hasher, field.data_type());
}

/// Write one type tag, length-prefixed so no two tags can run together.
fn hash_tag(hasher: &mut Sha256, tag: &str) {
    hasher.update((tag.len() as u64).to_le_bytes());
    hasher.update(tag.as_bytes());
}

/// Hash one Arrow time unit as its own stable tag.
fn hash_time_unit(hasher: &mut Sha256, unit: TimeUnit) {
    hash_tag(
        hasher,
        match unit {
            TimeUnit::Second => "s",
            TimeUnit::Millisecond => "ms",
            TimeUnit::Microsecond => "us",
            TimeUnit::Nanosecond => "ns",
        },
    );
}

/// Hash one tagged type that carries a single time unit.
fn hash_timed(hasher: &mut Sha256, tag: &str, unit: TimeUnit) {
    hash_tag(hasher, tag);
    hash_time_unit(hasher, unit);
}

/// Hash one tagged type that wraps exactly one child field.
fn hash_nested(hasher: &mut Sha256, tag: &str, child: &Field) {
    hash_tag(hasher, tag);
    hash_exact_field(hasher, child);
}

/// Return the stable tag of every Arrow type that carries no parameters.
///
/// Returning `None` for a parameterized type is what keeps the layout encoding
/// honest: this match is exhaustive, so an Arrow upgrade that adds a variant
/// fails to compile here instead of silently fingerprinting as an existing
/// layout.
fn parameterless_tag(data_type: &DataType) -> Option<&'static str> {
    Some(match data_type {
        DataType::Null => "null",
        DataType::Boolean => "bool",
        DataType::Int8 => "i8",
        DataType::Int16 => "i16",
        DataType::Int32 => "i32",
        DataType::Int64 => "i64",
        DataType::UInt8 => "u8",
        DataType::UInt16 => "u16",
        DataType::UInt32 => "u32",
        DataType::UInt64 => "u64",
        DataType::Float16 => "f16",
        DataType::Float32 => "f32",
        DataType::Float64 => "f64",
        DataType::Date32 => "date32",
        DataType::Date64 => "date64",
        DataType::Binary => "binary",
        DataType::LargeBinary => "large-binary",
        DataType::BinaryView => "binary-view",
        DataType::Utf8 => "utf8",
        DataType::LargeUtf8 => "large-utf8",
        DataType::Utf8View => "utf8-view",
        DataType::Timestamp(_, _)
        | DataType::Time32(_)
        | DataType::Time64(_)
        | DataType::Duration(_)
        | DataType::Interval(_)
        | DataType::FixedSizeBinary(_)
        | DataType::List(_)
        | DataType::ListView(_)
        | DataType::LargeList(_)
        | DataType::LargeListView(_)
        | DataType::FixedSizeList(_, _)
        | DataType::Struct(_)
        | DataType::Union(_, _)
        | DataType::Dictionary(_, _)
        | DataType::Decimal32(_, _)
        | DataType::Decimal64(_, _)
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _)
        | DataType::Map(_, _)
        | DataType::RunEndEncoded(_, _) => return None,
    })
}

/// Hash one Arrow data type exactly, recursing into every nested child.
///
/// Every variant writes its own tag before its parameters, so no parameter
/// encoding can be confused with another variant's.
///
/// # Panics
///
/// Panics when a type is neither parameterless nor one of the parameterized
/// variants below, which [`parameterless_tag`]'s exhaustive match makes
/// unreachable.
fn hash_exact_type(hasher: &mut Sha256, data_type: &DataType) {
    match data_type {
        DataType::Timestamp(unit, zone) => {
            hash_timed(hasher, "timestamp", *unit);
            match zone {
                Some(zone) => {
                    hash_tag(hasher, "zoned");
                    hash_tag(hasher, zone.as_ref());
                }
                None => hash_tag(hasher, "naive"),
            }
        }
        DataType::Time32(unit) => hash_timed(hasher, "time32", *unit),
        DataType::Time64(unit) => hash_timed(hasher, "time64", *unit),
        DataType::Duration(unit) => hash_timed(hasher, "duration", *unit),
        DataType::Interval(unit) => {
            hash_tag(hasher, "interval");
            hash_tag(
                hasher,
                match unit {
                    IntervalUnit::YearMonth => "year-month",
                    IntervalUnit::DayTime => "day-time",
                    IntervalUnit::MonthDayNano => "month-day-nano",
                },
            );
        }
        DataType::FixedSizeBinary(width) => {
            hash_tag(hasher, "fixed-size-binary");
            hasher.update(width.to_le_bytes());
        }
        DataType::List(child) => hash_nested(hasher, "list", child),
        DataType::ListView(child) => hash_nested(hasher, "list-view", child),
        DataType::LargeList(child) => hash_nested(hasher, "large-list", child),
        DataType::LargeListView(child) => hash_nested(hasher, "large-list-view", child),
        DataType::FixedSizeList(child, width) => {
            hash_tag(hasher, "fixed-size-list");
            hasher.update(width.to_le_bytes());
            hash_exact_field(hasher, child);
        }
        DataType::Struct(children) => {
            hash_tag(hasher, "struct");
            hash_exact_fields(hasher, children);
        }
        DataType::Union(children, mode) => {
            hash_tag(hasher, "union");
            hash_tag(
                hasher,
                match mode {
                    UnionMode::Sparse => "sparse",
                    UnionMode::Dense => "dense",
                },
            );
            hasher.update((children.len() as u64).to_le_bytes());
            for (type_id, child) in children.iter() {
                hasher.update(type_id.to_le_bytes());
                hash_exact_field(hasher, child);
            }
        }
        DataType::Dictionary(key, value) => {
            hash_tag(hasher, "dictionary");
            hash_exact_type(hasher, key);
            hash_exact_type(hasher, value);
        }
        DataType::Decimal32(precision, scale) => {
            hash_decimal(hasher, "decimal32", *precision, *scale);
        }
        DataType::Decimal64(precision, scale) => {
            hash_decimal(hasher, "decimal64", *precision, *scale);
        }
        DataType::Decimal128(precision, scale) => {
            hash_decimal(hasher, "decimal128", *precision, *scale);
        }
        DataType::Decimal256(precision, scale) => {
            hash_decimal(hasher, "decimal256", *precision, *scale);
        }
        DataType::Map(entries, sorted) => {
            hash_tag(hasher, "map");
            hasher.update([u8::from(*sorted)]);
            hash_exact_field(hasher, entries);
        }
        DataType::RunEndEncoded(run_ends, values) => {
            hash_tag(hasher, "run-end-encoded");
            hash_exact_field(hasher, run_ends);
            hash_exact_field(hasher, values);
        }
        parameterless => hash_tag(
            hasher,
            parameterless_tag(parameterless)
                .expect("every non-parameterized Arrow type has a stable tag"),
        ),
    }
}

/// Hash one decimal width's tag, precision, and scale.
fn hash_decimal(hasher: &mut Sha256, tag: &str, precision: u8, scale: i8) {
    hash_tag(hasher, tag);
    hasher.update([precision]);
    hasher.update(scale.to_le_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Build a nested schema whose field metadata is inserted in `order`.
    ///
    /// Metadata does not change an array's memory layout, so two schemas that
    /// differ only in the order their metadata entries were inserted must
    /// fingerprint identically. Building both from one function keeps every
    /// layout-significant part provably equal.
    fn nested_schema(order: [(&str, &str); 2], nullable_child: bool) -> Schema {
        let mut metadata = std::collections::HashMap::new();
        for (key, value) in order {
            metadata.insert((*key).to_owned(), (*value).to_owned());
        }
        let child = Field::new("inner", DataType::Utf8, nullable_child).with_metadata(metadata);
        Schema::new(vec![Field::new(
            "outer",
            DataType::Struct(vec![Arc::new(child)].into()),
            false,
        )])
    }

    /// The exact fingerprint commits layout and nullability, and only those.
    ///
    /// # Panics
    ///
    /// Panics when metadata insertion order changes the fingerprint, when a
    /// top-level or nested nullability change does not, or when the small and
    /// large variable-width layouts collide.
    #[test]
    fn exact_fingerprint_commits_layout_and_nullability_only() {
        const A: (&str, &str) = ("first", "1");
        const B: (&str, &str) = ("second", "2");
        assert_eq!(
            SchemaFingerprint::from_arrow_schema_exact(&nested_schema([A, B], true)),
            SchemaFingerprint::from_arrow_schema_exact(&nested_schema([B, A], true)),
            "metadata insertion order does not change the memory layout"
        );
        assert_ne!(
            SchemaFingerprint::from_arrow_schema_exact(&nested_schema([A, B], true)),
            SchemaFingerprint::from_arrow_schema_exact(&nested_schema([A, B], false)),
            "a nested nullability change is a validity-buffer change"
        );

        let nullable = Schema::new(vec![Field::new("value", DataType::Int64, true)]);
        let required = Schema::new(vec![Field::new("value", DataType::Int64, false)]);
        assert_ne!(
            SchemaFingerprint::from_arrow_schema_exact(&nullable),
            SchemaFingerprint::from_arrow_schema_exact(&required),
            "a top-level nullability change is a validity-buffer change"
        );

        for (small, large) in [
            (DataType::Utf8, DataType::LargeUtf8),
            (DataType::Binary, DataType::LargeBinary),
        ] {
            assert_ne!(
                SchemaFingerprint::from_arrow_schema_exact(&Schema::new(vec![Field::new(
                    "value", small, false
                )])),
                SchemaFingerprint::from_arrow_schema_exact(&Schema::new(vec![Field::new(
                    "value", large, false
                )])),
                "the offset width is part of the memory layout"
            );
        }
    }
}

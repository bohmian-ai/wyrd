//! Arrow field-type helpers and the canonical signal field declaration model.
//!
//! Two layers live here. The free `utf8`/`int64`/… constructors are the small
//! helpers every pre-declared domain table's `arrow_fields()` uses. Above them,
//! [`CanonicalField`] is the declarative ledger entry the three canonical `OTel`
//! signal tables are built from: one immutable record carrying a table-local
//! name, its physical type, nullability, and its projection/permission class.
//! Every downstream representation — the Arrow schema, the canonical physical
//! fingerprint, the sensitivity list, and the canonical Arrow validator — is
//! derived from that one declaration, so no consumer ever re-encodes column
//! order or field semantics. Numeric field ids are not part of a declaration:
//! the registered Iceberg table assigns them, and Scribe stamps the table's
//! ids onto every batch it accepts for that table.

use std::collections::HashMap;

use arrow::datatypes::{DataType, Field, TimeUnit};
use parquet_variant_compute::VariantType;
use wyrd_types::variant::variant_storage_type;

/// Arrow field metadata key Parquet and Iceberg read a field id from.
///
/// Only the registered Iceberg table's assigned ids are ever written under this
/// key: Scribe stamps them from the table on admission, so every Parquet object
/// it seals carries exactly the ids Forge and Oracle resolve columns by.
pub const PARQUET_FIELD_ID: &str = "PARQUET:field_id";

/// Arrow field metadata key carrying the declared sensitivity of a field.
///
/// Sensitivity is semantic metadata, not a physical type property, so it
/// travels on the schema itself. That keeps the canonical Arrow validator and
/// the canonical physical fingerprint computable from a supplied schema alone
/// rather than requiring the reader to hold the ledger.
pub const WYRD_SENSITIVE: &str = "wyrd:sensitive";

pub fn utf8(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Utf8, nullable)
}

/// Declare one UTC microsecond timestamp column in Iceberg's own Arrow form.
///
/// Iceberg reads a `timestamptz` back as the `+00:00` zone, so declaring that
/// spelling keeps every built-in a fixed point of its registered schema.
pub fn ts_us_utc(name: &str, nullable: bool) -> Field {
    Field::new(
        name,
        DataType::Timestamp(
            TimeUnit::Microsecond,
            Some(iceberg::arrow::UTC_TIME_ZONE.into()),
        ),
        nullable,
    )
}

pub fn boolean(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Boolean, nullable)
}

pub fn int32(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Int32, nullable)
}

pub fn int64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Int64, nullable)
}

pub fn float64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Float64, nullable)
}

pub fn fixed_binary(name: &str, size: i32, nullable: bool) -> Field {
    Field::new(name, DataType::FixedSizeBinary(size), nullable)
}

/// Projection and permission class of one canonical signal field.
///
/// The class is declared once per ledger entry and drives two separate
/// downstream decisions: whether a reader may project the column without the
/// payload-read permission ([`Self::SensitivePayload`] may not), and the
/// sensitivity byte the canonical physical fingerprint commits to. Bulk
/// numeric payload — histogram buckets, explicit bounds, quantile values — is
/// [`Self::Payload`] rather than sensitive: it is large enough that a metadata
/// query should skip reading it, but it carries no caller content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldClass {
    /// Queryable metadata a metadata-only projection always reads.
    Metadata,
    /// Bulk payload skipped by metadata-only projections; not access-gated.
    Payload,
    /// Caller content: skipped by metadata projections and access-gated.
    SensitivePayload,
}

impl FieldClass {
    /// Report whether this class requires the payload-read permission.
    ///
    /// This is the single definition of the sensitivity bit committed to by
    /// [`crate::tables::CanonicalPhysicalFingerprint`], so a class change is a
    /// fingerprint change.
    #[must_use]
    pub const fn is_sensitive(self) -> bool {
        matches!(self, Self::SensitivePayload)
    }
}

/// Physical type of one canonical signal field.
///
/// This is a closed set: it is exactly the types the three canonical signal
/// ledgers and the Observation envelope select, and every variant has a pinned
/// fingerprint tag. Representing another OTLP shape requires adding a variant
/// with a new unused tag, never reinterpreting an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalType {
    /// Presence bit or protocol boolean.
    Bool,
    /// Signed 32-bit protocol discriminant.
    Int32,
    /// Signed 64-bit protocol value.
    Int64,
    /// Unsigned 32-bit protocol counter or flag word.
    UInt32,
    /// Unsigned 64-bit protocol counter or `fixed64` nanosecond timestamp.
    UInt64,
    /// IEEE-754 double retained bit-for-bit.
    Float64,
    /// UTF-8 protocol string.
    Utf8,
    /// Deterministic pinned Prost encoding of one protobuf value or list.
    Binary,
    /// Fixed-width protocol identifier of the declared byte width.
    FixedSizeBinary(i32),
    /// Arrow timestamp with the declared unit and optional timezone.
    Timestamp(TimeUnit, Option<&'static str>),
    /// Ordered repeated value whose single child declares the element.
    List(&'static CanonicalField),
    /// Nested record whose children declare its fields in order.
    Struct(&'static [CanonicalField]),
    /// Self-describing semi-structured value in the Parquet/Iceberg Variant
    /// encoding, carried on Arrow as the `arrow.parquet.variant` extension
    /// over [`variant_storage_type`]. Its fingerprint commits only the type tag:
    /// the storage children and any per-file shredding are not logical shape.
    Variant,
}

impl CanonicalType {
    /// Project this declaration into its Arrow type, recursing into children.
    ///
    /// Nested children carry their own sensitivity metadata, so every
    /// schema-only consumer sees the whole tree's classification.
    #[must_use]
    pub fn to_arrow(&self) -> DataType {
        match self {
            Self::Bool => DataType::Boolean,
            Self::Int32 => DataType::Int32,
            Self::Int64 => DataType::Int64,
            Self::UInt32 => DataType::UInt32,
            Self::UInt64 => DataType::UInt64,
            Self::Float64 => DataType::Float64,
            Self::Utf8 => DataType::Utf8,
            Self::Binary => DataType::Binary,
            Self::FixedSizeBinary(width) => DataType::FixedSizeBinary(*width),
            Self::Timestamp(unit, zone) => {
                DataType::Timestamp(*unit, zone.map(std::convert::Into::into))
            }
            Self::List(child) => DataType::List(std::sync::Arc::new(child.to_arrow())),
            Self::Struct(children) => DataType::Struct(
                children
                    .iter()
                    .map(CanonicalField::to_arrow)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Self::Variant => variant_storage_type(),
        }
    }

    /// Return the declared children of a nested type in declaration order.
    ///
    /// Scalars have no children; a list has exactly its element declaration and
    /// a struct has its ordered fields. The canonical physical fingerprint and
    /// the recursive validator both walk the tree through this one accessor.
    #[must_use]
    pub fn children(&self) -> &'static [CanonicalField] {
        match self {
            Self::List(child) => std::slice::from_ref(*child),
            Self::Struct(children) => children,
            _ => &[],
        }
    }
}

/// One immutable canonical ledger entry.
///
/// A declaration carries no numeric field id. The registered Iceberg table
/// assigns ids when it is created, and Scribe, Forge, and Oracle all read them
/// from that table, so the ledger never competes with the table for identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalField {
    /// Physical column or nested child name.
    pub name: &'static str,
    /// Declared physical type.
    pub ty: CanonicalType,
    /// Whether the physical column admits nulls.
    pub nullable: bool,
    /// Projection and permission class.
    pub class: FieldClass,
}

impl CanonicalField {
    /// Declare one queryable metadata field.
    #[must_use]
    pub const fn meta(name: &'static str, ty: CanonicalType, nullable: bool) -> Self {
        Self {
            name,
            ty,
            nullable,
            class: FieldClass::Metadata,
        }
    }

    /// Declare one bulk, non-access-gated payload field.
    #[must_use]
    pub const fn payload(name: &'static str, ty: CanonicalType, nullable: bool) -> Self {
        Self {
            name,
            ty,
            nullable,
            class: FieldClass::Payload,
        }
    }

    /// Declare one access-gated caller-content payload field.
    #[must_use]
    pub const fn sensitive(name: &'static str, ty: CanonicalType, nullable: bool) -> Self {
        Self {
            name,
            ty,
            nullable,
            class: FieldClass::SensitivePayload,
        }
    }

    /// Project this entry into its Arrow field, stamping its semantic metadata.
    ///
    /// A Variant entry also carries the `arrow.parquet.variant` extension
    /// marker, which is what distinguishes it from a plain storage struct.
    ///
    /// The `wyrd:sensitive` metadata is written on this field and, through
    /// [`CanonicalType::to_arrow`], on every nested child, so the whole tree
    /// carries its sensitivity into every schema-only consumer. No field id is
    /// written: the registered Iceberg table owns ids.
    #[must_use]
    pub fn to_arrow(&self) -> Field {
        let field = Field::new(self.name, self.ty.to_arrow(), self.nullable).with_metadata(
            HashMap::from([(
                WYRD_SENSITIVE.to_owned(),
                self.class.is_sensitive().to_string(),
            )]),
        );
        match self.ty {
            CanonicalType::Variant => field.with_extension_type(VariantType),
            _ => field,
        }
    }
}

/// Project a declared ledger into its ordered Arrow fields.
///
/// This is the only conversion from the ledger to Arrow; the physical schema,
/// the catalog registration, the Parquet writer, and the canonical Arrow
/// validator all consume its output rather than restating field order.
#[must_use]
pub fn canonical_arrow_fields(declared: &[CanonicalField]) -> Vec<Field> {
    declared.iter().map(CanonicalField::to_arrow).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nested children carry their own sensitivity and no declared field id.
    ///
    /// # Panics
    ///
    /// Panics when a nested child loses its sensitivity marker or any level of
    /// the tree declares a field id.
    #[test]
    fn nested_declarations_carry_sensitivity_and_no_field_id() {
        /// Sensitive list element, the nested level under test.
        static CHILD: CanonicalField =
            CanonicalField::sensitive("item", CanonicalType::UInt64, false);
        /// Non-sensitive list column whose element is [`CHILD`].
        static ROOT: CanonicalField =
            CanonicalField::payload("counts", CanonicalType::List(&CHILD), true);

        let field = ROOT.to_arrow();
        assert_eq!(field.metadata().get(PARQUET_FIELD_ID), None);
        assert_eq!(
            field.metadata().get(WYRD_SENSITIVE),
            Some(&"false".to_owned())
        );
        let DataType::List(child) = field.data_type() else {
            panic!("list declaration must project an Arrow list");
        };
        assert_eq!(child.name(), "item");
        assert!(!child.is_nullable());
        assert_eq!(child.metadata().get(PARQUET_FIELD_ID), None);
        assert_eq!(
            child.metadata().get(WYRD_SENSITIVE),
            Some(&"true".to_owned())
        );
    }

    /// Only caller content is access-gated; bulk numeric payload is not.
    #[test]
    fn only_sensitive_payload_is_access_gated() {
        assert!(FieldClass::SensitivePayload.is_sensitive());
        assert!(!FieldClass::Payload.is_sensitive());
        assert!(!FieldClass::Metadata.is_sensitive());
    }
}

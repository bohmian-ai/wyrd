//! `FieldResolver` for `wyrd.cards` reserved columns.
#![deny(missing_docs)]

use wyrd_spec::error::WyrdError;
use wyrd_spec::query::{FieldRef, QueryFieldErrorDetail, ValueType};

use crate::query::{FieldColumn, FieldResolver};

/// Reserved columns the card query surface accepts.
const CARD_RESERVED_FIELDS: &[&str] = &[
    "created_at",
    "updated_at",
    "status",
    "name",
    "space",
    "kind",
];

/// Resolves reserved card columns for metadata-query compilation.
pub struct CardFieldResolver;

impl FieldResolver for CardFieldResolver {
    fn surface(&self) -> &'static str {
        "cards"
    }

    fn valid_fields(&self) -> &'static [&'static str] {
        CARD_RESERVED_FIELDS
    }

    fn resolve(&self, field: &FieldRef) -> Result<FieldColumn, WyrdError> {
        match field {
            FieldRef::Reserved(name) => match name.as_str() {
                "created_at" => Ok(FieldColumn::Typed {
                    column: "created_at",
                    ty: ValueType::Timestamp,
                }),
                "updated_at" => Ok(FieldColumn::Typed {
                    column: "updated_at",
                    ty: ValueType::Timestamp,
                }),
                "status" => Ok(FieldColumn::Typed {
                    column: "status",
                    ty: ValueType::Str,
                }),
                "name" => Ok(FieldColumn::Typed {
                    column: "name",
                    ty: ValueType::Str,
                }),
                "space" => Ok(FieldColumn::Typed {
                    column: "space",
                    ty: ValueType::Str,
                }),
                "kind" => Ok(FieldColumn::Typed {
                    column: "kind",
                    ty: ValueType::Str,
                }),
                other => Err(WyrdError::query_invalid_field_detail(
                    format!("unknown card field {other:?}"),
                    QueryFieldErrorDetail {
                        reason: "unknown_field",
                        field: Some(other),
                        surface: Some("cards"),
                        valid_fields: CARD_RESERVED_FIELDS,
                        ..Default::default()
                    },
                )),
            },
            FieldRef::Attribute(_) => Err(WyrdError::query_invalid_field_detail(
                "attributes are queryable on trace surfaces, not cards; use labels or annotations",
                QueryFieldErrorDetail {
                    reason: "unknown_field",
                    surface: Some("cards"),
                    valid_fields: CARD_RESERVED_FIELDS,
                    ..Default::default()
                },
            )),
            FieldRef::Label(_) | FieldRef::Annotation(_) => {
                unreachable!("labels/annotations are lowered by the compiler")
            }
        }
    }
}

//! Authored-version wire-shape for the card-registration path.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::Schema;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::block::VersionBlock;
use crate::error::VersionError;
use crate::range::VersionRange;

/// Authored-version discriminator on the card-registration path.
///
/// Two variants distinguished by input shape:
///
/// - [`VersionSpec::Pin`] — a full 3-part semver triple (with optional
///   pre-release / build metadata), e.g. `"1.4.2"` or `"1.4.2-rc.1+meta"`.
///   The registrar treats this as an exact pin: insert at this version,
///   short-circuit as `IdempotentNoop` on a hash match, return
///   `409 SpecDrift` on a hash conflict.
///
/// - [`VersionSpec::Scope`] — a bare 1- or 2-part partial pin, e.g. `"1"`
///   or `"1.0"`. The registrar treats this as a prefix scope: bump within
///   the prefix line (`"1.x.x"` or `"1.0.x"`) or seed at the padded triple
///   (`"1.0.0"`) if no prior version exists in that line.
///
/// Range comparator syntax (`^`, `~`, `*`, `>=`, ...) is rejected at
/// deserialize time — comparators are read-only and belong on list / query
/// paths, not the register path. See `VersionRange` for the read-side type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VersionSpec {
    /// Exact pin: full 3-part semver triple.
    Pin(VersionBlock),
    /// Prefix scope: bare 1- or 2-part partial pin.
    Scope(VersionRange),
}

impl VersionSpec {
    /// Parse a wire-string into a `VersionSpec`.
    ///
    /// A strict 3-part triple parses as [`VersionSpec::Pin`]; a bare 1- or
    /// 2-part partial parses as [`VersionSpec::Scope`]. Range comparator
    /// syntax is rejected.
    ///
    /// # Errors
    /// Returns an error when the input is empty, contains range comparators,
    /// or is otherwise neither a full triple nor a bare partial.
    pub fn parse(value: &str) -> Result<Self, VersionError> {
        if let Ok(block) = VersionBlock::parse(value) {
            return Ok(Self::Pin(block));
        }
        let range = VersionRange::parse_loose(value)?;
        if range.is_loose_partial() {
            Ok(Self::Scope(range))
        } else {
            Err(VersionError::NotRepresentable {
                reason: "register-path version must be a triple pin or bare partial",
            })
        }
    }

    /// Borrow the canonical wire-string for this spec.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pin(block) => block.as_str(),
            Self::Scope(range) => range.as_str(),
        }
    }

    /// Returns `true` when this spec is an exact-triple pin.
    #[must_use]
    pub fn is_pin(&self) -> bool {
        matches!(self, Self::Pin(_))
    }

    /// Returns `true` when this spec is a prefix scope.
    #[must_use]
    pub fn is_scope(&self) -> bool {
        matches!(self, Self::Scope(_))
    }
}

impl From<VersionBlock> for VersionSpec {
    fn from(block: VersionBlock) -> Self {
        Self::Pin(block)
    }
}

impl From<VersionRange> for VersionSpec {
    fn from(range: VersionRange) -> Self {
        Self::Scope(range)
    }
}

impl fmt::Display for VersionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for VersionSpec {
    type Err = VersionError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for VersionSpec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VersionSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct VersionSpecVisitor;

        impl<'de> Visitor<'de> for VersionSpecVisitor {
            type Value = VersionSpec;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(
                    "a semver triple pin (e.g. \"1.4.2\") or bare partial scope (e.g. \"1\", \"1.0\")",
                )
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<VersionSpec, E> {
                VersionSpec::parse(value).map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<VersionSpec, E> {
                VersionSpec::parse(&value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(VersionSpecVisitor)
    }
}

impl JsonSchema for VersionSpec {
    fn schema_name() -> String {
        "VersionSpec".to_owned()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        serde_json::from_value(serde_json::json!({
            "type": "string",
            "description": "Authored version on the card-registration path. Either a full semver triple pin (e.g. \"1.4.2\", \"1.4.2-rc.1+meta\") or a bare partial scope (e.g. \"1\", \"1.0\"). Range comparators (^, ~, *, >=, ...) are rejected on this path.",
            "examples": ["1.4.2", "1.4.2-rc.1+meta", "1.0", "1"]
        }))
        .expect("VersionSpec schema is valid JSON Schema")
    }
}

#[cfg(feature = "server")]
impl utoipa::PartialSchema for VersionSpec {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};

        utoipa::openapi::RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .description(Some(
                    "Authored version on the card-registration path. \
                     Either a full semver triple pin (e.g. \"1.4.2\", \"1.4.2-rc.1+meta\") \
                     or a bare partial scope (e.g. \"1\", \"1.0\"). \
                     Range comparators (^, ~, *, >=, ...) are rejected on this path.",
                ))
                .build(),
        ))
    }
}

#[cfg(feature = "server")]
impl utoipa::ToSchema for VersionSpec {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("VersionSpec")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_triple_is_pin() {
        let spec = VersionSpec::parse("1.4.2").unwrap();
        assert!(spec.is_pin());
        assert_eq!(spec.as_str(), "1.4.2");
    }

    #[test]
    fn parse_triple_with_prerelease_is_pin() {
        let spec = VersionSpec::parse("1.4.2-rc.1+meta").unwrap();
        assert!(spec.is_pin());
        assert_eq!(spec.as_str(), "1.4.2-rc.1+meta");
    }

    #[test]
    fn parse_bare_minor_is_scope() {
        let spec = VersionSpec::parse("1.0").unwrap();
        assert!(spec.is_scope());
    }

    #[test]
    fn parse_bare_major_is_scope() {
        let spec = VersionSpec::parse("1").unwrap();
        assert!(spec.is_scope());
    }

    #[test]
    fn parse_rejects_caret() {
        assert!(VersionSpec::parse("^1.0").is_err());
        assert!(VersionSpec::parse("^1.0.0").is_err());
    }

    #[test]
    fn parse_rejects_tilde() {
        assert!(VersionSpec::parse("~1.0").is_err());
        assert!(VersionSpec::parse("~1.0.0").is_err());
    }

    #[test]
    fn parse_rejects_wildcard() {
        assert!(VersionSpec::parse("1.*").is_err());
        assert!(VersionSpec::parse("*").is_err());
    }

    #[test]
    fn parse_rejects_comparator_chain() {
        assert!(VersionSpec::parse(">=1.0.0, <2.0.0").is_err());
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(VersionSpec::parse("").is_err());
    }

    #[test]
    fn from_str_round_trips() {
        let spec: VersionSpec = "1.0".parse().unwrap();
        assert_eq!(spec.to_string(), "~1.0");
    }

    #[test]
    fn serialize_pin_to_string() {
        let spec = VersionSpec::parse("1.4.2").unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(json, "\"1.4.2\"");
    }

    #[test]
    fn serialize_scope_emits_normalized_form() {
        let spec = VersionSpec::parse("1.0").unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        // Scope normalizes "1.0" → "~1.0" inside VersionRange; that's the
        // wire form we round-trip. Round-trip stability matters more than
        // preserving the original input.
        assert_eq!(json, "\"~1.0\"");
    }

    #[test]
    fn deserialize_full_triple_to_pin() {
        let spec: VersionSpec = serde_json::from_str("\"1.4.2\"").unwrap();
        assert!(spec.is_pin());
    }

    #[test]
    fn deserialize_bare_partial_to_scope() {
        let spec: VersionSpec = serde_json::from_str("\"1.0\"").unwrap();
        assert!(spec.is_scope());
    }

    #[test]
    fn deserialize_rejects_comparator() {
        let err = serde_json::from_str::<VersionSpec>("\"^1.0\"").unwrap_err();
        assert!(err.to_string().contains("register-path version"));
    }

    #[test]
    fn deserialize_rejects_wildcard() {
        assert!(serde_json::from_str::<VersionSpec>("\"1.*\"").is_err());
    }

    #[test]
    fn round_trip_pin_through_json() {
        let spec = VersionSpec::parse("1.4.2-rc.1+meta").unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        let back: VersionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }
}

//! Typed operands for metadata predicates.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A typed literal operand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Value {
    /// UTF-8 string.
    String(String),
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit float.
    Float(f64),
    /// Boolean.
    Bool(bool),
    /// Duration in whole nanoseconds.
    Duration(i64),
    /// Absolute UTC timestamp.
    Timestamp(DateTime<Utc>),
}

/// The type class of a value or field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// String.
    Str,
    /// Integer.
    Int,
    /// Float.
    Float,
    /// Boolean.
    Bool,
    /// Duration.
    Duration,
    /// Timestamp.
    Timestamp,
}

impl ValueType {
    /// Stable string used in structured query error details.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Str => "string",
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Duration => "duration",
            Self::Timestamp => "timestamp",
        }
    }

    /// True when this type supports ordering comparisons.
    #[must_use]
    pub fn is_orderable(self) -> bool {
        matches!(
            self,
            Self::Int | Self::Float | Self::Duration | Self::Timestamp
        )
    }
}

impl Value {
    /// The type class of this value.
    #[must_use]
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::String(_) => ValueType::Str,
            Self::Int(_) => ValueType::Int,
            Self::Float(_) => ValueType::Float,
            Self::Bool(_) => ValueType::Bool,
            Self::Duration(_) => ValueType::Duration,
            Self::Timestamp(_) => ValueType::Timestamp,
        }
    }

    /// True when this value supports ordering comparisons.
    #[must_use]
    pub fn is_orderable(&self) -> bool {
        self.value_type().is_orderable()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::{Value, ValueType};

    #[test]
    fn value_types_and_ordering_are_stable() {
        let values = [
            (Value::String("x".to_owned()), ValueType::Str, false),
            (Value::Int(1), ValueType::Int, true),
            (Value::Float(1.5), ValueType::Float, true),
            (Value::Bool(true), ValueType::Bool, false),
            (Value::Duration(1), ValueType::Duration, true),
            (
                Value::Timestamp(
                    chrono::Utc
                        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
                        .single()
                        .expect("valid timestamp"),
                ),
                ValueType::Timestamp,
                true,
            ),
        ];

        for (value, ty, orderable) in values {
            assert_eq!(value.value_type(), ty);
            assert_eq!(value.is_orderable(), orderable);
        }
    }
}

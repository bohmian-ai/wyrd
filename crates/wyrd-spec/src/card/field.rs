//! Shared field and tensor-shape contracts for cards that describe typed data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::ColumnName;

/// Ordered field declaration used by Wyrd data schemas and model signatures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FieldSpec {
    /// Canonical column or feature name.
    pub name: ColumnName,
    /// Canonical Arrow logical dtype string.
    pub dtype: String,
    /// Optional tensor or nested value shape; empty means scalar/tabular.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<Dim>,
    /// Whether this field may contain null values.
    #[serde(default)]
    pub nullable: bool,
    /// Additional string metadata that does not change the Wyrd field contract.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl FieldSpec {
    /// Build a scalar, non-nullable field with no extra metadata.
    #[must_use]
    pub fn new(name: ColumnName, dtype: impl Into<String>) -> Self {
        Self {
            name,
            dtype: dtype.into(),
            shape: Vec::new(),
            nullable: false,
            extra: BTreeMap::new(),
        }
    }
}

/// Return true when a dtype string is a canonical Arrow logical dtype.
#[must_use]
pub fn is_canonical_dtype(value: &str) -> bool {
    if value.is_empty() || value.trim() != value {
        return false;
    }
    is_dtype(value)
}

fn is_dtype(value: &str) -> bool {
    is_leaf_dtype(value)
        || is_list_dtype(value, "list<")
        || is_list_dtype(value, "large_list<")
        || is_fixed_size_list_dtype(value)
        || is_struct_dtype(value)
        || is_dictionary_dtype(value)
}

fn is_leaf_dtype(value: &str) -> bool {
    matches!(
        value,
        "bool"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "float16"
            | "float32"
            | "float64"
            | "utf8"
            | "large_utf8"
            | "binary"
            | "large_binary"
            | "date32"
            | "date64"
            | "time32[s]"
            | "time32[ms]"
            | "time64[us]"
            | "time64[ns]"
            | "timestamp[s]"
            | "timestamp[ms]"
            | "timestamp[us]"
            | "timestamp[ns]"
            | "timestamp[s, tz=UTC]"
            | "timestamp[ms, tz=UTC]"
            | "timestamp[us, tz=UTC]"
            | "timestamp[ns, tz=UTC]"
            | "duration[s]"
            | "duration[ms]"
            | "duration[us]"
            | "duration[ns]"
    ) || is_decimal_dtype(value)
}

fn is_decimal_dtype(value: &str) -> bool {
    let Some(inner) = inner_for(value, "decimal128(").or_else(|| inner_for(value, "decimal256("))
    else {
        return false;
    };
    let parts = split_top_level(inner, ',');
    if parts.len() != 2 {
        return false;
    }
    let Some(precision) = parse_positive_i64(parts[0].trim()) else {
        return false;
    };
    let Some(scale) = parse_non_negative_i64(parts[1].trim()) else {
        return false;
    };
    scale <= precision
}

fn is_list_dtype(value: &str, prefix: &str) -> bool {
    inner_for(value, prefix).is_some_and(is_dtype)
}

fn is_fixed_size_list_dtype(value: &str) -> bool {
    let Some(inner) = inner_for(value, "fixed_size_list<") else {
        return false;
    };
    let parts = split_top_level(inner, ',');
    if parts.len() != 2 {
        return false;
    }
    is_dtype(parts[0]) && parse_positive_i64(parts[1].trim()).is_some()
}

fn is_struct_dtype(value: &str) -> bool {
    let Some(inner) = inner_for(value, "struct<") else {
        return false;
    };
    let fields = split_top_level(inner, ',');
    if fields.is_empty() {
        return false;
    }
    fields.into_iter().all(|field| {
        let parts = split_top_level(field, ':');
        parts.len() == 2 && is_struct_field_name(parts[0]) && is_dtype(parts[1])
    })
}

fn is_dictionary_dtype(value: &str) -> bool {
    let Some(inner) = inner_for(value, "dictionary<") else {
        return false;
    };
    let parts = split_top_level(inner, ',');
    if parts.len() != 2 {
        return false;
    }
    is_dictionary_index_dtype(parts[0].trim()) && is_dtype(parts[1].trim())
}

fn is_dictionary_index_dtype(value: &str) -> bool {
    matches!(
        value,
        "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64"
    )
}

fn is_struct_field_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn inner_for<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if !value.starts_with(prefix) {
        return None;
    }
    let closing = match prefix.as_bytes().last() {
        Some(b'<') => '>',
        Some(b'(') => ')',
        _ => return None,
    };
    let start = prefix.len();
    let end = matching_delimiter(value, start - 1, closing)?;
    (end == value.len() - 1).then_some(&value[start..end])
}

fn matching_delimiter(value: &str, open_index: usize, closing: char) -> Option<usize> {
    let opening = match closing {
        '>' => '<',
        ')' => '(',
        _ => return None,
    };
    let mut depth = 0_i64;
    for (index, ch) in value
        .char_indices()
        .skip_while(|(index, _)| *index < open_index)
    {
        if ch == opening {
            depth += 1;
        } else if ch == closing {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
            if depth < 0 {
                return None;
            }
        }
    }
    None
}

fn split_top_level(value: &str, delimiter: char) -> Vec<&str> {
    if value.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let mut angle_depth = 0_i64;
    let mut paren_depth = 0_i64;
    for (index, ch) in value.char_indices() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            _ => {}
        }
        if ch == delimiter && angle_depth == 0 && paren_depth == 0 {
            parts.push(&value[start..index]);
            start = index + ch.len_utf8();
        }
    }
    parts.push(&value[start..]);
    parts
}

fn parse_positive_i64(value: &str) -> Option<i64> {
    let parsed = value.parse::<i64>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn parse_non_negative_i64(value: &str) -> Option<i64> {
    let parsed = value.parse::<i64>().ok()?;
    (parsed >= 0).then_some(parsed)
}

/// One dimension in a field shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "value")]
pub enum Dim {
    /// A fixed, known dimension length.
    Fixed(i64),
    /// A dynamic dimension, optionally named for documentation and signatures.
    Dynamic(Option<String>),
}

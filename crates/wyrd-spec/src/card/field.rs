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

/// Return true when a dtype string is already a canonical Arrow logical dtype.
///
/// This is the pure `wyrd-spec` predicate used by card validators after a
/// surface has normalized source-library dtypes. It intentionally does not
/// inspect Python, pandas, NumPy, torch, or Arrow objects; that source-specific
/// normalization belongs in `wyrd-interfaces`, while this function only checks
/// the durable string grammar stored in [`FieldSpec::dtype`].
#[must_use]
pub fn is_canonical_dtype(value: &str) -> bool {
    if value.is_empty() || value.trim() != value {
        return false;
    }
    is_dtype(value)
}

/// Dispatch to the supported scalar and structural dtype grammars.
///
/// Keeping the dispatcher private keeps the public contract narrow:
/// validators need one boolean predicate, while the implementation can keep one
/// small helper per grammar family for readability and targeted tests.
fn is_dtype(value: &str) -> bool {
    is_leaf_dtype(value)
        || is_list_dtype(value, "list<")
        || is_list_dtype(value, "large_list<")
        || is_fixed_size_list_dtype(value)
        || is_struct_dtype(value)
        || is_dictionary_dtype(value)
}

/// Return true for the canonical scalar Arrow logical dtype names.
///
/// These are the non-structural leaves that can appear directly in a
/// [`FieldSpec`] or inside structural forms such as `list<...>` and
/// `struct<...>`. Decimal forms delegate to [`is_decimal_dtype`] because they
/// include precision and scale parameters.
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

/// Return true for `decimal128(precision, scale)` and `decimal256(...)`.
///
/// Decimal dtypes need a small parser because the canonical string carries two
/// numeric parameters. The check rejects malformed values and scale values
/// larger than precision so ModelCard signatures cannot store impossible
/// decimal definitions.
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

/// Return true for a variable-length list dtype using the supplied prefix.
///
/// `list<...>` and `large_list<...>` share the same recursive inner dtype
/// grammar. The prefix parameter keeps the two public canonical spellings
/// explicit without duplicating the parser.
fn is_list_dtype(value: &str, prefix: &str) -> bool {
    inner_for(value, prefix).is_some_and(is_dtype)
}

/// Return true for `fixed_size_list<inner, n>` with a positive length.
///
/// Fixed-size lists are structural dtypes whose second parameter is a shape
/// count, not another dtype. Validating that count here keeps shape-like dtype
/// metadata deterministic before registry or runtime layers see it.
fn is_fixed_size_list_dtype(value: &str) -> bool {
    let Some(inner) = inner_for(value, "fixed_size_list<") else {
        return false;
    };
    let parts = split_top_level(inner, ',');
    if parts.len() != 2 {
        return false;
    }
    is_dtype(parts[0].trim()) && parse_positive_i64(parts[1].trim()).is_some()
}

/// Return true for `struct<field:dtype,...>` over canonical field dtypes.
///
/// Struct dtypes are allowed in signatures for nested model inputs and outputs.
/// The validator checks field-name syntax locally and recursively validates
/// each field dtype so invalid nested leaves cannot pass through the spec
/// layer.
fn is_struct_dtype(value: &str) -> bool {
    let Some(inner) = inner_for(value, "struct<") else {
        return false;
    };
    let fields = split_top_level(inner, ',');
    if fields.is_empty() {
        return false;
    }
    fields.into_iter().all(|field| {
        let field = field.trim();
        let parts = split_top_level(field, ':');
        parts.len() == 2 && is_struct_field_name(parts[0].trim()) && is_dtype(parts[1].trim())
    })
}

/// Return true for `dictionary<index_dtype, value_dtype>`.
///
/// Dictionary values are how categorical columns are represented in canonical
/// Wyrd strings. The index side is restricted to integer dtypes, while the
/// value side can be any canonical dtype accepted by [`is_dtype`].
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

/// Return true when a dtype is valid for dictionary indices.
///
/// Arrow dictionary indices are integer-like. Keeping this as a separate helper
/// makes that narrower rule visible instead of hiding it in the general dtype
/// predicate.
fn is_dictionary_index_dtype(value: &str) -> bool {
    matches!(
        value,
        "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64"
    )
}

/// Return true when a struct field name is valid in a canonical dtype string.
///
/// These names are local to the dtype grammar, not Wyrd `ColumnName`s. The
/// lighter rule permits common nested field labels while still rejecting empty
/// or punctuation-only names that would make parsing ambiguous.
fn is_struct_field_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Return the delimited payload for a structural dtype prefix.
///
/// This helper verifies that the matching closing delimiter ends the string.
/// That prevents accepting partial values such as `list<int64>extra` while
/// still allowing nested delimiters inside the payload.
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

/// Return the byte index of the closing delimiter that matches an opener.
///
/// Structural dtype parsing needs delimiter matching rather than simple suffix
/// stripping because nested values can contain their own `<...>` or `(...)`
/// pairs. The caller supplies the opening delimiter index and expected closing
/// delimiter.
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

/// Split a structural dtype payload on a delimiter at nesting depth zero.
///
/// Dtype forms such as `struct<a:list<int64>,b:utf8>` contain commas inside
/// nested parameters. This helper only splits separators that belong to the
/// current level so recursive parsers receive intact child dtype strings.
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

/// Parse a positive integer parameter from a dtype string.
///
/// Decimal precision and fixed-size-list length must be greater than zero.
/// Returning `Option` keeps parser callers simple and avoids introducing an
/// error type for a private grammar helper.
fn parse_positive_i64(value: &str) -> Option<i64> {
    let parsed = value.parse::<i64>().ok()?;
    (parsed > 0).then_some(parsed)
}

/// Parse a non-negative integer parameter from a dtype string.
///
/// Decimal scale may be zero but cannot be negative. This is separate from
/// [`parse_positive_i64`] so decimal precision and scale rules remain explicit.
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

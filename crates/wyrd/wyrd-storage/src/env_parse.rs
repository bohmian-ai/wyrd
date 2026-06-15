//! Shared environment variable parsing helpers for wyrd-storage configuration.

use crate::error::{ConfigParseError, StorageError};

/// Read a required environment variable.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the variable is absent or
/// contains non-UTF-8 bytes.
pub fn env_required(var: &'static str) -> Result<String, StorageError> {
    std::env::var(var).map_err(|source| StorageError::ConfigParse {
        var,
        source: ConfigParseError::MissingEnv(source),
    })
}

/// Read an optional environment variable.
///
/// Returns `None` when the variable is not set.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the variable contains
/// non-UTF-8 bytes.
pub fn env_optional(var: &'static str) -> Result<Option<String>, StorageError> {
    match std::env::var(var) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(source) => Err(StorageError::ConfigParse {
            var,
            source: ConfigParseError::MissingEnv(source),
        }),
    }
}

/// Parse a required boolean environment variable with a default.
///
/// Accepts `"true"`, `"1"`, `"yes"`, `"on"` as `true` and `"false"`, `"0"`,
/// `"no"`, `"off"` as `false`.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the value is not a recognised
/// boolean string.
pub fn parse_bool(var: &'static str, default: bool) -> Result<bool, StorageError> {
    match std::env::var(var) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(StorageError::ConfigParse {
                var,
                source: ConfigParseError::InvalidBool(value),
            }),
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(source) => Err(StorageError::ConfigParse {
            var,
            source: ConfigParseError::MissingEnv(source),
        }),
    }
}

/// Parse an optional boolean environment variable.
///
/// Returns `None` when the variable is not set.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the value is not a recognised
/// boolean string.
pub fn parse_bool_optional(var: &'static str) -> Result<Option<bool>, StorageError> {
    let Some(value) = env_optional(var)? else {
        return Ok(None);
    };
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(Some(true)),
        "false" | "0" | "no" | "off" => Ok(Some(false)),
        _ => Err(StorageError::ConfigParse {
            var,
            source: ConfigParseError::InvalidBool(value),
        }),
    }
}

/// Parse a `u32` environment variable clamped to `[lower, upper]`.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the value cannot be parsed as
/// `u32`.
pub fn parse_u32_clamped(
    var: &'static str,
    default: u32,
    lower: u32,
    upper: u32,
) -> Result<u32, StorageError> {
    match std::env::var(var) {
        Ok(value) => {
            let parsed = value
                .parse::<u32>()
                .map_err(|source| StorageError::ConfigParse {
                    var,
                    source: ConfigParseError::InvalidU32 { value, source },
                })?;
            Ok(parsed.clamp(lower, upper))
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(source) => Err(StorageError::ConfigParse {
            var,
            source: ConfigParseError::MissingEnv(source),
        }),
    }
}

/// Parse a `u64` environment variable clamped to `[lower, upper]`.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the value cannot be parsed as
/// `u64`.
pub fn parse_u64_clamped(
    var: &'static str,
    default: u64,
    lower: u64,
    upper: u64,
) -> Result<u64, StorageError> {
    match std::env::var(var) {
        Ok(value) => {
            let parsed = value
                .parse::<u64>()
                .map_err(|source| StorageError::ConfigParse {
                    var,
                    source: ConfigParseError::InvalidU64 { value, source },
                })?;
            Ok(parsed.clamp(lower, upper))
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(source) => Err(StorageError::ConfigParse {
            var,
            source: ConfigParseError::MissingEnv(source),
        }),
    }
}

/// Parse an optional `u64` environment variable.
///
/// Returns `None` when the variable is not set.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the value cannot be parsed as
/// `u64`.
pub fn parse_u64_optional(var: &'static str) -> Result<Option<u64>, StorageError> {
    env_optional(var)?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|source| StorageError::ConfigParse {
                    var,
                    source: ConfigParseError::InvalidU64 { value, source },
                })
        })
        .transpose()
}

/// Parse a `u64` environment variable clamped to `[min, max]`, returned as `i64`.
///
/// The `default` is used when the variable is absent. The fallback parameter
/// has been removed — `i64::try_from(default.clamp(min, max))` is always
/// representable given `max <= i64::MAX`.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] when the value cannot be parsed as
/// `u64`.
///
/// # Panics
/// Panics if `max > i64::MAX`, which violates the caller invariant that
/// clamped values are always representable as `i64`.
pub fn parse_clamped_i64(
    var: &'static str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<i64, StorageError> {
    let value = parse_u64_optional(var)?.unwrap_or(default).clamp(min, max);
    Ok(i64::try_from(value).expect("clamped value fits in i64"))
}

//! Validated metadata labels and annotations.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Queryable metadata labels.
pub type Labels = BTreeMap<LabelKey, LabelValue>;

/// Free-form metadata annotations.
pub type Annotations = BTreeMap<AnnotationKey, AnnotationValue>;

macro_rules! metadata_string_type {
    ($name:ident, $doc:literal, $validator:ident) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema,
        )]
        #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
        pub struct $name(String);

        impl $name {
            /// Build a validated metadata value.
            ///
            /// # Errors
            /// Returns an error when the value is not valid for this metadata type.
            pub fn new(value: impl Into<String>) -> Result<Self, MetadataError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            /// Borrow the metadata value as a string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = MetadataError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

metadata_string_type!(
    LabelKey,
    "Validated key for queryable Card and Run labels.",
    validate_metadata_key
);
metadata_string_type!(
    LabelValue,
    "Validated value for queryable Card and Run labels.",
    validate_label_value
);
metadata_string_type!(
    AnnotationKey,
    "Validated key for free-form Card annotations.",
    validate_metadata_key
);
metadata_string_type!(
    AnnotationValue,
    "Validated value for free-form Card annotations.",
    validate_annotation_value
);

impl LabelKey {
    /// Build a user-authored label key.
    ///
    /// # Errors
    /// Returns an error when the key is invalid or uses a reserved Wyrd prefix.
    pub fn new_user(value: impl Into<String>) -> Result<Self, MetadataError> {
        let key = Self::new(value)?;
        validate_user_key(key.as_str())?;
        Ok(key)
    }
}

impl AnnotationKey {
    /// Build a user-authored annotation key.
    ///
    /// # Errors
    /// Returns an error when the key is invalid or uses a reserved Wyrd prefix.
    pub fn new_user(value: impl Into<String>) -> Result<Self, MetadataError> {
        let key = Self::new(value)?;
        validate_user_key(key.as_str())?;
        Ok(key)
    }
}

impl LabelValue {
    /// Build a user-authored label value.
    ///
    /// # Errors
    /// Returns an error when the value is invalid or appears to contain a secret.
    pub fn new_user(value: impl Into<String>) -> Result<Self, MetadataError> {
        let value = Self::new(value)?;
        validate_user_value(value.as_str())?;
        Ok(value)
    }
}

impl AnnotationValue {
    /// Build a user-authored annotation value.
    ///
    /// # Errors
    /// Returns an error when the value is invalid or appears to contain a secret.
    pub fn new_user(value: impl Into<String>) -> Result<Self, MetadataError> {
        let value = Self::new(value)?;
        validate_user_value(value.as_str())?;
        Ok(value)
    }
}

/// Return true when a metadata key is reserved for Wyrd-owned metadata.
#[must_use]
pub fn is_reserved_metadata_key(value: &str) -> bool {
    value.starts_with("wyrd.io/") || value.starts_with("internal.wyrd.io/")
}

/// Return true when a metadata value appears to contain secret material.
#[must_use]
pub fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("bearer ")
        || lower.contains("-----begin ")
        || lower.starts_with("sk-")
        || lower.starts_with("xoxb-")
        || lower.starts_with("ghp_")
    {
        return true;
    }

    lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| {
            matches!(
                part,
                "secret"
                    | "password"
                    | "passwd"
                    | "token"
                    | "apikey"
                    | "credential"
                    | "credentials"
                    | "privatekey"
            )
        })
}

fn validate_metadata_key(value: &str) -> Result<(), MetadataError> {
    if value.is_empty() {
        return Err(MetadataError::InvalidKey);
    }

    let Some((prefix, name)) = value.rsplit_once('/') else {
        return validate_metadata_name(value);
    };

    if prefix.is_empty() || name.is_empty() || prefix.len() > 253 {
        return Err(MetadataError::InvalidKey);
    }
    validate_dns_prefix(prefix)?;
    validate_metadata_name(name)
}

fn validate_metadata_name(value: &str) -> Result<(), MetadataError> {
    if value.is_empty() || value.len() > 63 {
        return Err(MetadataError::InvalidKey);
    }
    validate_qualified_string(value, MetadataError::InvalidKey)
}

fn validate_label_value(value: &str) -> Result<(), MetadataError> {
    if value.len() > 63 {
        return Err(MetadataError::InvalidLabelValue);
    }
    if value.is_empty() {
        return Ok(());
    }
    validate_qualified_string(value, MetadataError::InvalidLabelValue)
}

fn validate_annotation_value(value: &str) -> Result<(), MetadataError> {
    if value.len() > 4096 {
        return Err(MetadataError::InvalidAnnotationValue);
    }
    Ok(())
}

fn validate_dns_prefix(value: &str) -> Result<(), MetadataError> {
    if value
        .split('.')
        .all(|part| !part.is_empty() && part.len() <= 63 && is_dns_label(part))
    {
        Ok(())
    } else {
        Err(MetadataError::InvalidKey)
    }
}

fn is_dns_label(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn validate_qualified_string(value: &str, error: MetadataError) -> Result<(), MetadataError> {
    let bytes = value.as_bytes();
    if bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.'))
    {
        Ok(())
    } else {
        Err(error)
    }
}

fn validate_user_key(value: &str) -> Result<(), MetadataError> {
    if is_reserved_metadata_key(value) {
        return Err(MetadataError::ReservedKey);
    }
    if looks_like_secret(value) {
        return Err(MetadataError::SecretLikeValue);
    }
    Ok(())
}

fn validate_user_value(value: &str) -> Result<(), MetadataError> {
    if looks_like_secret(value) {
        return Err(MetadataError::SecretLikeValue);
    }
    Ok(())
}

/// Metadata validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    /// Metadata key does not match `[prefix/]name` grammar.
    #[error("metadata key must match [prefix/]name with DNS-style prefix and qualified name")]
    InvalidKey,
    /// Label value is too long or does not match label value grammar.
    #[error("label value must be empty or a qualified string with at most 63 characters")]
    InvalidLabelValue,
    /// Annotation value is too long.
    #[error("annotation value must be at most 4096 characters")]
    InvalidAnnotationValue,
    /// Metadata key uses a Wyrd-reserved prefix.
    #[error("metadata key uses a reserved Wyrd prefix")]
    ReservedKey,
    /// User metadata appears to contain secret material.
    #[error("metadata must not contain secret-looking keys or values")]
    SecretLikeValue,
}

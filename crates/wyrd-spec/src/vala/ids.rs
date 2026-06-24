//! Canonical id newtypes shared across the vala surface.
//!
//! This module owns ids that more than one vala submodule consumes. Eval-scoped
//! ids such as `TaskId`, `ScenarioId`, and `JsonPath` remain in
//! [`crate::vala::eval::ids`].

use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::WyrdError;

/// Session-scoped identifier grouping related eval, record, and observation records.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct SessionId(pub uuid::Uuid);

/// Run identifier carried by every observation from one agent invocation.
///
/// UUIDv7 string at the wire boundary. Client-generated so retries converge on
/// the same id, and so observability surfaces can correlate records emitted
/// before the server first sees the run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    /// Generate a fresh UUIDv7 run identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(crate::ids::uuid7())
    }

    /// Adopt a caller-supplied run identifier.
    #[must_use]
    pub fn from_string(value: String) -> Self {
        Self(value)
    }

    /// Borrow the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Deterministic bucket assignment for stable sampling decisions.
    ///
    /// SHA-256 over the id bytes; the first eight digest bytes are read
    /// big-endian as a `u64` and reduced modulo `buckets`.
    #[must_use]
    pub fn hash_bucket(&self, buckets: u32) -> u32 {
        use sha2::{Digest, Sha256};

        if buckets == 0 {
            return 0;
        }
        let digest = Sha256::digest(self.0.as_bytes());
        let mut prefix = [0_u8; 8];
        prefix.copy_from_slice(&digest[..8]);
        let value = u64::from_be_bytes(prefix);
        (value % u64::from(buckets)) as u32
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Local development session identifier (the code axis before any service
/// identity exists).
///
/// UUIDv7 string at the wire boundary. Client-generated at the start of a local
/// dev session so observations emitted while authoring code — before a commit
/// or a registered card exists — can later be reconciled to the resulting
/// commit and card. See [`crate::vala::correlation::CorrelationContext`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct DevSessionId(String);

impl DevSessionId {
    /// Generate a fresh UUIDv7 dev-session identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(crate::ids::uuid7())
    }

    /// Adopt a caller-supplied dev-session identifier.
    #[must_use]
    pub fn from_string(value: String) -> Self {
        Self(value)
    }

    /// Borrow the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for DevSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DevSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifier for a single vala record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct RecordId(pub uuid::Uuid);

/// Identifier for one workflow execution against one target and input row.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct WorkflowUid(pub uuid::Uuid);

/// Stable user-facing entity identifier for the eval subject or observation subject.
///
/// Entity UIDs must be non-empty and no longer than 512 characters.
/// Deserialization goes through [`EntityUid::new`] so wire payloads cannot bypass
/// validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", schema(value_type = String))]
#[serde(transparent)]
pub struct EntityUid(String);

impl EntityUid {
    /// Constructs a validated entity uid.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the value is empty or longer than
    /// 512 characters.
    pub fn new(value: impl Into<String>) -> Result<Self, WyrdError> {
        let value = value.into();
        if value.is_empty() || value.len() > 512 {
            return Err(WyrdError::Validation {
                message: format!("entity_uid length must be 1..=512, got {}", value.len()),
                details: serde_json::Value::Null,
            });
        }
        Ok(Self(value))
    }

    /// Borrows the validated entity uid as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for EntityUid {
    type Err = WyrdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for EntityUid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for EntityUid {
    fn schema_name() -> String {
        "EntityUid".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(512),
                min_length: Some(1),
                pattern: None,
            })),
            ..Default::default()
        }
        .into()
    }
}

/// Opaque lease token returned by the orchestrator when an eval run is opened.
///
/// The client returns this bearer-secret value on every subsequent eval
/// protocol call as proof of ownership. Validation is intentionally minimal:
/// the cryptographic token shape is a server-internal concern. Wire grammar is
/// non-empty, length `1..=256`, and no control characters. Do not log values of
/// this type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", schema(value_type = String))]
#[serde(transparent)]
pub struct LeaseToken(String);

impl LeaseToken {
    /// Constructs a validated lease token.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the value is empty, longer than
    /// 256 characters, or contains a control character.
    pub fn new(value: impl Into<String>) -> Result<Self, WyrdError> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 {
            return Err(WyrdError::Validation {
                message: format!("lease_token length must be 1..=256, got {}", value.len()),
                details: serde_json::Value::Null,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(WyrdError::Validation {
                message: "lease_token contains control characters".to_string(),
                details: serde_json::Value::Null,
            });
        }
        Ok(Self(value))
    }

    /// Borrows the validated token as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for LeaseToken {
    type Err = WyrdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for LeaseToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for LeaseToken {
    fn schema_name() -> String {
        "LeaseToken".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(256),
                min_length: Some(1),
                pattern: None,
            })),
            ..Default::default()
        }
        .into()
    }
}

/// Sixteen-byte OpenTelemetry trace identifier.
///
/// Wire format is a 32-character lower-case hex string. Deserialization also
/// accepts the legacy byte-array shape so existing eval callers can continue
/// to read older payloads. The all-zero id is the OTel invalid sentinel and is
/// rejected by public constructors and deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", schema(value_type = String))]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// All-zero invalid-id sentinel.
    ///
    /// Constructors never return this value; it is exposed so validation code
    /// can compare against an explicitly invalid sentinel when needed.
    pub const ZERO: TraceId = TraceId([0u8; 16]);

    /// Constructs a trace id from a 32-character hex string.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the string has the wrong length,
    /// contains non-hex characters, or represents the all-zero sentinel.
    pub fn from_hex(value: &str) -> Result<Self, WyrdError> {
        if value.len() != 32 {
            return Err(WyrdError::Validation {
                message: format!("trace_id hex length must be 32, got {}", value.len()),
                details: serde_json::Value::Null,
            });
        }

        let mut bytes = [0u8; 16];
        for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
            let hi = hex_nibble(chunk[0])?;
            let lo = hex_nibble(chunk[1])?;
            bytes[index] = (hi << 4) | lo;
        }

        Self::from_bytes(bytes)
    }

    /// Constructs a trace id from raw bytes.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the bytes are all zero.
    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, WyrdError> {
        if bytes == [0u8; 16] {
            return Err(WyrdError::Validation {
                message: "trace_id all-zero is the OTel invalid sentinel".to_string(),
                details: serde_json::Value::Null,
            });
        }

        Ok(Self(bytes))
    }

    /// Returns the 32-character lower-case hex representation.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(32);
        for byte in &self.0 {
            out.push(hex_char(byte >> 4));
            out.push(hex_char(byte & 0x0f));
        }
        out
    }

    /// Borrows the underlying bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl Serialize for TraceId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for TraceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Hex(String),
            Bytes([u8; 16]),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Hex(value) => Self::from_hex(&value).map_err(serde::de::Error::custom),
            Wire::Bytes(bytes) => Self::from_bytes(bytes).map_err(serde::de::Error::custom),
        }
    }
}

impl schemars::JsonSchema for TraceId {
    fn schema_name() -> String {
        "TraceId".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(32),
                min_length: Some(32),
                pattern: Some(r"^[0-9a-f]{32}$".to_string()),
            })),
            ..Default::default()
        }
        .into()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for TraceId {
    type Err = WyrdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// Eight-byte OpenTelemetry span identifier.
///
/// Wire format is a 16-character lower-case hex string. Deserialization also
/// accepts the legacy byte-array shape so existing eval callers can continue
/// to read older payloads. The all-zero id is the OTel invalid sentinel and is
/// rejected by public constructors and deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", schema(value_type = String))]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// All-zero invalid-id sentinel.
    ///
    /// Constructors never return this value; it is exposed so validation code
    /// can compare against an explicitly invalid sentinel when needed.
    pub const ZERO: SpanId = SpanId([0u8; 8]);

    /// Constructs a span id from a 16-character hex string.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the string has the wrong length,
    /// contains non-hex characters, or represents the all-zero sentinel.
    pub fn from_hex(value: &str) -> Result<Self, WyrdError> {
        if value.len() != 16 {
            return Err(WyrdError::Validation {
                message: format!("span_id hex length must be 16, got {}", value.len()),
                details: serde_json::Value::Null,
            });
        }

        let mut bytes = [0u8; 8];
        for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
            let hi = hex_nibble(chunk[0])?;
            let lo = hex_nibble(chunk[1])?;
            bytes[index] = (hi << 4) | lo;
        }

        Self::from_bytes(bytes)
    }

    /// Constructs a span id from raw bytes.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the bytes are all zero.
    pub fn from_bytes(bytes: [u8; 8]) -> Result<Self, WyrdError> {
        if bytes == [0u8; 8] {
            return Err(WyrdError::Validation {
                message: "span_id all-zero is the OTel invalid sentinel".to_string(),
                details: serde_json::Value::Null,
            });
        }

        Ok(Self(bytes))
    }

    /// Returns the 16-character lower-case hex representation.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(16);
        for byte in &self.0 {
            out.push(hex_char(byte >> 4));
            out.push(hex_char(byte & 0x0f));
        }
        out
    }

    /// Borrows the underlying bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl Serialize for SpanId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for SpanId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Hex(String),
            Bytes([u8; 8]),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Hex(value) => Self::from_hex(&value).map_err(serde::de::Error::custom),
            Wire::Bytes(bytes) => Self::from_bytes(bytes).map_err(serde::de::Error::custom),
        }
    }
}

impl schemars::JsonSchema for SpanId {
    fn schema_name() -> String {
        "SpanId".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, StringValidation};

        SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            string: Some(Box::new(StringValidation {
                max_length: Some(16),
                min_length: Some(16),
                pattern: Some(r"^[0-9a-f]{16}$".to_string()),
            })),
            ..Default::default()
        }
        .into()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for SpanId {
    type Err = WyrdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

fn hex_nibble(byte: u8) -> Result<u8, WyrdError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        other => Err(WyrdError::Validation {
            message: format!("invalid hex character: {:?}", other as char),
            details: serde_json::Value::Null,
        }),
    }
}

fn hex_char(nibble: u8) -> char {
    assert!(nibble < 16, "hex_char received >15 nibble; caller bug");
    if nibble < 10 {
        (b'0' + nibble) as char
    } else {
        (b'a' + nibble - 10) as char
    }
}

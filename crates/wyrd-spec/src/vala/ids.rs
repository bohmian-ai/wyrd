//! Canonical id newtypes shared across the vala surface.
//!
//! This module owns ids that more than one vala submodule consumes. Verifier-scoped
//! ids such as `TaskId` and `JsonPath` live in
//! [`crate::verification::ids`].

use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::WyrdError;

/// Session-scoped identifier grouping related verification, record, and observation records.
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
    ///
    /// # Panics
    ///
    /// Panics if a remainder exceeds its `u32` divisor, which arithmetic rules out.
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
        u32::try_from(value % u64::from(buckets)).expect("a remainder is below its u32 divisor")
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

/// Stable user-facing entity identifier for the verification subject or observation subject.
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
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EntityUid".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "minLength": 1, "maxLength": 512 })
    }
}

/// Sixteen-byte OpenTelemetry trace identifier.
///
/// Wire format is a 32-character lower-case hex string. Deserialization also
/// accepts the legacy byte-array shape so existing callers can continue
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
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TraceId".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "minLength": 32, "maxLength": 32, "pattern": r"^[0-9a-f]{32}$" })
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
/// accepts the legacy byte-array shape so existing callers can continue
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
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SpanId".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "minLength": 16, "maxLength": 16, "pattern": r"^[0-9a-f]{16}$" })
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

#[cfg(test)]
mod tests {
    use super::{EntityUid, RecordId, SessionId, SpanId, TraceId, WorkflowUid};

    #[test]
    fn id_uuid_types_serde_round_trip() {
        let uuid = uuid::Uuid::nil();
        let session = SessionId(uuid);
        let record = RecordId(uuid);
        let workflow = WorkflowUid(uuid);

        assert_eq!(
            serde_json::to_string(&session).expect("session id serializes"),
            format!("\"{uuid}\"")
        );
        assert_eq!(
            session,
            serde_json::from_str(&format!("\"{uuid}\"")).expect("session id")
        );
        assert_eq!(
            record,
            serde_json::from_str(&format!("\"{uuid}\"")).expect("record id")
        );
        assert_eq!(
            workflow,
            serde_json::from_str(&format!("\"{uuid}\"")).expect("workflow id")
        );
    }

    #[test]
    fn entity_uid_serde_round_trip() {
        let entity = EntityUid::new("customer-123").expect("valid entity uid");
        let json = serde_json::to_string(&entity).expect("entity uid serializes");
        assert_eq!(json, "\"customer-123\"");
        let round_trip: EntityUid = serde_json::from_str(&json).expect("entity uid deserializes");
        assert_eq!(entity, round_trip);
    }

    #[test]
    fn trace_id_deserializes_legacy_byte_array_and_serializes_hex() {
        let trace: TraceId = serde_json::from_str("[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]")
            .expect("legacy byte array deserializes");
        let json = serde_json::to_string(&trace).expect("trace id serializes");
        assert_eq!(json, "\"0102030405060708090a0b0c0d0e0f10\"");
    }

    #[test]
    fn span_id_deserializes_legacy_byte_array_and_serializes_hex() {
        let span: SpanId =
            serde_json::from_str("[1,2,3,4,5,6,7,8]").expect("legacy byte array deserializes");
        let json = serde_json::to_string(&span).expect("span id serializes");
        assert_eq!(json, "\"0102030405060708\"");
    }
}

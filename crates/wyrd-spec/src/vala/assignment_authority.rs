//! The assignment-authority digest binding a live Scribe fragment's
//! assignments to the leader's peer context.
//!
//! This module is the Arrow-free, IO-free deterministic byte encoding hashed
//! into the `assignment-authority` digest that binds a Scribe follower's
//! projection and provider cut to the leader's peer context before any read.
//! The context is unsigned; the digest proves the context and the dispatched
//! assignments agree, and origin comes from the mTLS cluster identity. Every
//! byte-level rule here is normative, so its encoding must be reproduced
//! exactly by any producer or verifier.

use sha2::{Digest, Sha256};

/// Input to the assignment-authority digest for exactly one
/// `FollowerScanAssignment`, in canonical (validated, unsorted) order.
///
/// Every list field is hashed in the order given: columns are never sorted
/// by the digest function. The caller is responsible for supplying the
/// already-validated semantic order it wants signed.
#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentDigestInput<'a> {
    /// Stable scan identifier encoded in the physical extension node.
    pub scan_id: &'a str,
    /// Authenticated tenant UUID for this scan.
    pub tenant_uuid: uuid::Uuid,
    /// Non-empty table namespace.
    pub namespace: &'a str,
    /// Non-empty table name.
    pub table: &'a str,
    /// Canonical lowercase 64-hex physical schema fingerprint.
    pub schema_fingerprint_hex: &'a str,
    /// Optional Scribe memory-provider cut for this assignment.
    pub scribe_cut: Option<&'a crate::vala::api::ScribeProviderCut>,
    /// Required output/predicate/hidden-tenant projection closure, in order.
    pub required_columns: &'a [String],
}

/// Digest encoding failure for one assignment-authority input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentDigestError {
    /// The schema fingerprint was not exactly 64 lowercase hex characters.
    #[error("schema fingerprint must be 64 lowercase hex characters, got {0:?}")]
    InvalidSchemaFingerprint(String),
    /// A string field or list exceeded `u32::MAX` bytes/elements.
    #[error("{field} exceeds the u32 length domain encodable by the digest")]
    LengthOverflow {
        /// Name of the field that overflowed.
        field: &'static str,
    },
}

/// Domain separator for the v9 assignment-authority digest.
///
/// v9 drops the persisted-file descriptors and the scan predicates: persisted
/// reads are planned and distributed by `DataFusion`, so an assignment names
/// only a Scribe provider cut and its projection. The domain differs so v8
/// bytes never validate as v9.
const ASSIGNMENT_AUTHORITY_DOMAIN: &[u8] = b"wyrd.oracle.assignment-authority.v9\0";

/// Appends a length-prefixed UTF-8 string: a big-endian `u32` byte length
/// followed by the raw UTF-8 bytes.
///
/// # Errors
///
/// Returns [`AssignmentDigestError::LengthOverflow`] when the string
/// exceeds the `u32` length domain.
fn push_string(buffer: &mut Vec<u8>, value: &str) -> Result<(), AssignmentDigestError> {
    push_bytes(buffer, value.as_bytes(), "string length")
}

/// Appends a big-endian `u32` byte length followed by the raw bytes.
///
/// # Errors
///
/// Returns [`AssignmentDigestError::LengthOverflow`] naming `field` when the
/// slice exceeds the `u32` length domain.
fn push_bytes(
    buffer: &mut Vec<u8>,
    bytes: &[u8],
    field: &'static str,
) -> Result<(), AssignmentDigestError> {
    let len =
        u32::try_from(bytes.len()).map_err(|_| AssignmentDigestError::LengthOverflow { field })?;
    buffer.extend_from_slice(&len.to_be_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

/// Appends a big-endian `u32` count.
fn push_count(
    buffer: &mut Vec<u8>,
    count: usize,
    field: &'static str,
) -> Result<(), AssignmentDigestError> {
    let count =
        u32::try_from(count).map_err(|_| AssignmentDigestError::LengthOverflow { field })?;
    buffer.extend_from_slice(&count.to_be_bytes());
    Ok(())
}

/// Appends `option_tag:u8` (`0` for `None`, `1` for `Some`) followed by the
/// payload writer's output when `Some`.
fn push_option<T>(
    buffer: &mut Vec<u8>,
    value: Option<T>,
    write: impl FnOnce(&mut Vec<u8>, T) -> Result<(), AssignmentDigestError>,
) -> Result<(), AssignmentDigestError> {
    match value {
        None => buffer.push(0),
        Some(inner) => {
            buffer.push(1);
            write(buffer, inner)?;
        }
    }
    Ok(())
}

/// Appends one typed partition as `granularity_tag:u8 || start_unix_micros:i64`
/// big-endian.
///
/// Tag `0` is reserved for "unspecified" and is unreachable here because
/// [`crate::vala::api::TimePartitionWire`] cannot hold it; a decoder that sees
/// `0` must reject the bytes rather than defaulting.
fn push_time_partition(buffer: &mut Vec<u8>, partition: crate::vala::api::TimePartitionWire) {
    buffer.push(partition.granularity_tag());
    buffer.extend_from_slice(&partition.start_unix_micros().to_be_bytes());
}

/// Appends one Scribe provider cut as `writer_epoch:u64` big-endian followed
/// by its start and end partitions.
///
/// The cut names which memory partitions a follower may read on which writer
/// incarnation, so it is digested whole; it carries no size limit.
///
/// # Errors
///
/// Never fails; the `Result` is the shape `push_option` requires.
fn push_scribe_cut(
    buffer: &mut Vec<u8>,
    cut: &crate::vala::api::ScribeProviderCut,
) -> Result<(), AssignmentDigestError> {
    buffer.extend_from_slice(&cut.writer_epoch.to_be_bytes());
    push_time_partition(buffer, cut.start_partition);
    push_time_partition(buffer, cut.end_partition);
    Ok(())
}

/// Decodes a canonical lowercase 64-hex schema fingerprint into its 32 raw
/// bytes.
///
/// # Errors
/// Returns [`AssignmentDigestError::InvalidSchemaFingerprint`] when the value
/// is not exactly 64 lowercase hex characters.
fn decode_schema_fingerprint(hex: &str) -> Result<[u8; 32], AssignmentDigestError> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AssignmentDigestError::InvalidSchemaFingerprint(
            hex.to_string(),
        ));
    }
    let mut out = [0u8; 32];
    for (index, chunk) in out.iter_mut().enumerate() {
        let byte_str = &hex[index * 2..index * 2 + 2];
        *chunk = u8::from_str_radix(byte_str, 16)
            .map_err(|_| AssignmentDigestError::InvalidSchemaFingerprint(hex.to_string()))?;
    }
    Ok(out)
}

/// Appends one assignment's canonical digest bytes in fixed field order.
///
/// # Errors
///
/// Returns [`AssignmentDigestError`] when the schema fingerprint is not
/// canonical hex, or when any length exceeds the `u32` domain the encoding
/// uses.
fn push_assignment(
    buffer: &mut Vec<u8>,
    assignment: &AssignmentDigestInput<'_>,
) -> Result<(), AssignmentDigestError> {
    push_string(buffer, assignment.scan_id)?;
    buffer.extend_from_slice(assignment.tenant_uuid.as_bytes());
    push_string(buffer, assignment.namespace)?;
    push_string(buffer, assignment.table)?;
    let fingerprint = decode_schema_fingerprint(assignment.schema_fingerprint_hex)?;
    buffer.extend_from_slice(&fingerprint);
    push_option(buffer, assignment.scribe_cut, push_scribe_cut)?;
    push_count(
        buffer,
        assignment.required_columns.len(),
        "required_columns",
    )?;
    for column in assignment.required_columns {
        push_string(buffer, column)?;
    }
    Ok(())
}

/// Encodes the canonical assignment-authority digest input bytes for the
/// given assignments, in request order.
///
/// # Errors
/// Returns [`AssignmentDigestError`] when a schema fingerprint is not
/// canonical 64-hex, or when a string/list length exceeds the `u32` domain
/// the encoding uses.
pub fn encode_assignment_authority_bytes(
    assignments: &[AssignmentDigestInput<'_>],
) -> Result<Vec<u8>, AssignmentDigestError> {
    let mut buffer = Vec::new();
    buffer.extend_from_slice(ASSIGNMENT_AUTHORITY_DOMAIN);
    push_count(&mut buffer, assignments.len(), "assignment_count")?;
    for assignment in assignments {
        push_assignment(&mut buffer, assignment)?;
    }
    Ok(buffer)
}

/// Computes the lowercase-hex SHA-256 assignment-authority digest for the
/// given assignments, in request order.
///
/// This is the value a leader places in the peer context claims and a
/// follower recomputes and compares before resolving any provider or issuing
/// object I/O — see the validation-precedence contract on the follower path.
///
/// # Errors
/// Returns [`AssignmentDigestError`] under the same conditions as
/// [`encode_assignment_authority_bytes`].
pub fn assignment_authority_digest(
    assignments: &[AssignmentDigestInput<'_>],
) -> Result<String, AssignmentDigestError> {
    let bytes = encode_assignment_authority_bytes(assignments)?;
    let digest = Sha256::digest(&bytes);
    Ok(hex_lower(&digest))
}

/// Renders raw bytes as lowercase hex, matching the canonical schema
/// fingerprint and digest encoding used across the wire.
#[must_use]
pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::vala::api::{ScribeProviderCut, TimeGranularityWire, TimePartitionWire};

    /// Builds the normative Scribe cut: writer epoch 7 over the two hourly
    /// partitions `2026-08-23T14:00:00Z` and `2026-08-23T15:00:00Z`.
    fn normative_scribe_cut() -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 7,
            start_partition: hour_partition(1_787_493_600_000_000),
            end_partition: hour_partition(1_787_497_200_000_000),
        }
    }

    /// Builds one hourly partition from its exact epoch-microsecond boundary.
    fn hour_partition(start_unix_micros: i64) -> TimePartitionWire {
        let start = chrono::DateTime::from_timestamp_micros(start_unix_micros)
            .expect("fixture start is representable");
        TimePartitionWire::new(TimeGranularityWire::Hour, start)
            .expect("fixture start is canonical")
    }

    /// Returns the normative fingerprint, tenant, and projection closure.
    ///
    /// # Panics
    ///
    /// Panics if the fixed fixture tenant UUID fails to parse, which cannot
    /// happen.
    fn base_vector() -> (String, uuid::Uuid, Vec<String>) {
        let fingerprint: String = (0u8..32).map(|byte| format!("{byte:02x}")).collect();
        let tenant_uuid = uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff")
            .expect("fixture tenant parses");
        let required_columns = vec![
            "service_name".to_string(),
            "wyrd_event_time".to_string(),
            "data_tenant_id".to_string(),
        ];
        (fingerprint, tenant_uuid, required_columns)
    }

    /// Digests one assignment.
    ///
    /// # Panics
    ///
    /// Panics when the assignment cannot be encoded for the digest.
    fn digest_of(assignment: &AssignmentDigestInput<'_>) -> String {
        assignment_authority_digest(std::slice::from_ref(assignment)).expect("fixture encodes")
    }

    /// Every field of an assignment — identity, table, fingerprint, cut
    /// presence and content, and projection order — moves the digest, so a
    /// peer cannot replay a signed context against a different read.
    #[test]
    fn every_assignment_field_changes_the_digest() {
        let (fingerprint, tenant_uuid, required_columns) = base_vector();
        let cut = normative_scribe_cut();
        let baseline = AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            scribe_cut: Some(&cut),
            required_columns: &required_columns,
        };
        let mut other_fingerprint = fingerprint.clone();
        other_fingerprint.replace_range(0..2, "ff");
        let mut other_epoch = cut.clone();
        other_epoch.writer_epoch = 2;
        let mut other_start = cut.clone();
        other_start.start_partition = hour_partition(1_787_490_000_000_000);
        let reordered = vec![
            "wyrd_event_time".to_string(),
            "service_name".to_string(),
            "data_tenant_id".to_string(),
        ];
        let digests = [
            digest_of(&baseline),
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-2",
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                tenant_uuid: uuid::Uuid::from_u128(1),
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                namespace: "logz",
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                table: "recordz",
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                schema_fingerprint_hex: &other_fingerprint,
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                scribe_cut: None,
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                scribe_cut: Some(&other_epoch),
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                scribe_cut: Some(&other_start),
                ..baseline.clone()
            }),
            digest_of(&AssignmentDigestInput {
                required_columns: &reordered,
                ..baseline.clone()
            }),
        ];
        let distinct: std::collections::BTreeSet<_> = digests.iter().collect();
        assert_eq!(distinct.len(), digests.len(), "every field is bound");
    }

    /// A partition start that is not the exact boundary of its granularity is
    /// unrepresentable, so no noncanonical value can ever reach the digest.
    #[test]
    fn noncanonical_partition_starts_are_unrepresentable() {
        let off_hour = chrono::DateTime::from_timestamp_micros(1_787_493_600_000_001)
            .expect("fixture start is representable");
        assert!(TimePartitionWire::new(TimeGranularityWire::Hour, off_hour).is_err());

        let mid_day = chrono::DateTime::from_timestamp_micros(1_787_493_600_000_000)
            .expect("fixture start is representable");
        assert!(TimePartitionWire::new(TimeGranularityWire::Day, mid_day).is_err());
    }

    /// A fingerprint that is not 64 lowercase hex characters is refused.
    #[test]
    fn rejects_non_canonical_schema_fingerprint() {
        let (_, tenant_uuid, required_columns) = base_vector();
        let assignment = AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: "not-hex",
            scribe_cut: None,
            required_columns: &required_columns,
        };
        assert!(matches!(
            assignment_authority_digest(std::slice::from_ref(&assignment)),
            Err(AssignmentDigestError::InvalidSchemaFingerprint(_))
        ));
    }
}

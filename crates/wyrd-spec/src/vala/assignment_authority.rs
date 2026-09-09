//! Closed scan-predicate/literal vocabulary and the v2 signed
//! assignment-authority digest.
//!
//! This module is the Arrow-free, IO-free contract for the predicate and
//! index pushdown closure: the exact leaf predicate/literal enums the
//! `OracleTableProvider` classifier recognizes as `Inexact`-supported, and
//! the deterministic byte encoding hashed into the signed
//! `assignment-authority` digest that binds a follower's projection,
//! predicates, files, and Scribe cut to the leader's ticket before any
//! object I/O. Every byte-level rule here is normative: the digest is a
//! security boundary, not an optimization hint, so its encoding must be
//! reproduced exactly by any producer or verifier.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Closed scalar literal vocabulary for a supported comparison predicate.
///
/// Only these six variants may appear as the literal operand of a supported
/// leaf predicate. `F64Bits` carries the IEEE-754 bit pattern (via
/// [`f64::to_bits`]) rather than a raw `f64` so equality, hashing, and the
/// digest encoding stay exact instead of floating-point-approximate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ScanLiteral {
    /// Boolean literal.
    Bool(bool),
    /// Signed 64-bit integer literal.
    I64(i64),
    /// Unsigned 64-bit integer literal.
    U64(u64),
    /// IEEE-754 double-precision literal, carried as its raw bit pattern.
    F64Bits(u64),
    /// UTF-8 string literal.
    Utf8(String),
    /// Microsecond-precision UTC timestamp literal.
    TimestampMicros(i64),
}

impl ScanLiteral {
    /// Returns the digest literal-type tag defined by the assignment-authority
    /// encoding (`Bool` 0, `I64` 1, `U64` 2, `F64Bits` 3, `Utf8` 4,
    /// `TimestampMicros` 5).
    #[must_use]
    pub fn digest_tag(&self) -> u8 {
        match self {
            ScanLiteral::Bool(_) => 0,
            ScanLiteral::I64(_) => 1,
            ScanLiteral::U64(_) => 2,
            ScanLiteral::F64Bits(_) => 3,
            ScanLiteral::Utf8(_) => 4,
            ScanLiteral::TimestampMicros(_) => 5,
        }
    }
}

/// Closed leaf predicate vocabulary reachable through predicate pushdown.
///
/// This is the complete supported-expression subset: typed comparisons over
/// exactly one unqualified column and one [`ScanLiteral`], plus null-checks.
/// Any DataFusion filter outside this shape (`OR`, `NOT`, casts, functions,
/// arithmetic, column-to-column comparison, qualified/unknown columns,
/// non-finite floats, mixed-type comparisons) is classified `Unsupported` by
/// the caller and never reaches this type. A literal appearing on the left of
/// a comparison is normalized by the caller (operator reversed) before
/// constructing this enum, so every comparison variant here always carries
/// `(column, literal)` in that order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ScanPredicate {
    /// `column = literal`.
    Eq(String, ScanLiteral),
    /// `column != literal`.
    NotEq(String, ScanLiteral),
    /// `column < literal`.
    Lt(String, ScanLiteral),
    /// `column <= literal`.
    LtEq(String, ScanLiteral),
    /// `column > literal`.
    Gt(String, ScanLiteral),
    /// `column >= literal`.
    GtEq(String, ScanLiteral),
    /// `column IS NULL`.
    IsNull(String),
    /// `column IS NOT NULL`.
    IsNotNull(String),
}

impl ScanPredicate {
    /// Returns the digest operator tag defined by the assignment-authority
    /// encoding (`Eq` 0, `NotEq` 1, `Lt` 2, `LtEq` 3, `Gt` 4, `GtEq` 5,
    /// `IsNull` 6, `IsNotNull` 7).
    #[must_use]
    pub fn digest_op_tag(&self) -> u8 {
        match self {
            ScanPredicate::Eq(..) => 0,
            ScanPredicate::NotEq(..) => 1,
            ScanPredicate::Lt(..) => 2,
            ScanPredicate::LtEq(..) => 3,
            ScanPredicate::Gt(..) => 4,
            ScanPredicate::GtEq(..) => 5,
            ScanPredicate::IsNull(..) => 6,
            ScanPredicate::IsNotNull(..) => 7,
        }
    }

    /// Borrows the predicate's column name, regardless of variant.
    #[must_use]
    pub fn column(&self) -> &str {
        match self {
            ScanPredicate::Eq(column, _)
            | ScanPredicate::NotEq(column, _)
            | ScanPredicate::Lt(column, _)
            | ScanPredicate::LtEq(column, _)
            | ScanPredicate::Gt(column, _)
            | ScanPredicate::GtEq(column, _)
            | ScanPredicate::IsNull(column)
            | ScanPredicate::IsNotNull(column) => column,
        }
    }

    /// Borrows the predicate's literal operand, or `None` for a null-check.
    #[must_use]
    pub fn literal(&self) -> Option<&ScanLiteral> {
        match self {
            ScanPredicate::Eq(_, literal)
            | ScanPredicate::NotEq(_, literal)
            | ScanPredicate::Lt(_, literal)
            | ScanPredicate::LtEq(_, literal)
            | ScanPredicate::Gt(_, literal)
            | ScanPredicate::GtEq(_, literal) => Some(literal),
            ScanPredicate::IsNull(_) | ScanPredicate::IsNotNull(_) => None,
        }
    }
}

/// Input to the assignment-authority digest for exactly one
/// `FollowerScanAssignment`, in canonical (validated, unsorted) order.
///
/// Every list field is hashed in the order given: files, columns, and
/// predicates are never sorted by the digest function. The caller is
/// responsible for supplying the already-validated semantic order it wants
/// signed.
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
    /// Signed typed persisted-file descriptors assigned to this follower, in
    /// order. The descriptor's variant, size, row count, source identity, and
    /// declared event-time bounds are all inside the digest, so a follower that
    /// validates the signature has validated the object identity it will read.
    pub files: &'a [crate::vala::api::PersistedFileDescriptor],
    /// Optional Scribe memory-provider cut for this assignment.
    pub scribe_cut: Option<&'a crate::vala::api::ScribeProviderCut>,
    /// Required output/predicate/hidden-tenant projection closure, in order.
    pub required_columns: &'a [String],
    /// Closed leaf predicates, in filter order.
    pub predicates: &'a [ScanPredicate],
    /// Exact protected snapshot cut this follower is authorized to read.
    ///
    /// Signed with the rest of the assignment so a follower cannot be steered
    /// onto a different snapshot, a different table's identity, or a different
    /// epoch than the leader planned under.
    pub reader_cut: &'a crate::vala::api::FollowerReaderCut,
}

/// Digest encoding failure for one assignment-authority input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentDigestError {
    /// The schema fingerprint was not exactly 64 lowercase hex characters.
    #[error("schema fingerprint must be 64 lowercase hex characters, got {0:?}")]
    InvalidSchemaFingerprint(String),
    /// The reader cut's ancestry digest was not exactly 64 lowercase hex
    /// characters, so the cut cannot be bound to a table identity.
    #[error("reader-cut ancestry digest must be 64 lowercase hex characters, got {0:?}")]
    InvalidAncestryDigest(String),
    /// A string field or list exceeded `u32::MAX` bytes/elements.
    #[error("{field} exceeds the u32 length domain encodable by the digest")]
    LengthOverflow {
        /// Name of the field that overflowed.
        field: &'static str,
    },
}

/// Domain separator for the v5 assignment-authority digest.
///
/// v5 appends the required
/// [`crate::vala::api::FollowerReaderCut`] to every assignment, so a signed
/// fragment names the exact protected snapshot the follower may read. The
/// domain differs from v4 so a v4 signature — which carried no cut — can never
/// validate against v5 bytes.
///
/// v4 replaced v3's length-prefixed persisted path strings with typed
/// [`crate::vala::api::PersistedFileDescriptor`] encodings: a source tag, the
/// path, the object size, the record count, the hot source's `vala.file_list`
/// identity and decoded checksum or the Iceberg source's pinned snapshot, and
/// the declared event-time bounds. Every other count, length, option,
/// predicate, projection, cut, and numeric rule is unchanged from v3, and the
/// domain differs so a v3 signature can never validate against v4 bytes.
const ASSIGNMENT_AUTHORITY_DOMAIN: &[u8] = b"wyrd.oracle.assignment-authority.v5\0";

/// Appends a length-prefixed UTF-8 string: a big-endian `u32` byte length
/// followed by the raw UTF-8 bytes.
fn push_string(buffer: &mut Vec<u8>, value: &str) -> Result<(), AssignmentDigestError> {
    let bytes = value.as_bytes();
    let len = u32::try_from(bytes.len()).map_err(|_| AssignmentDigestError::LengthOverflow {
        field: "string length",
    })?;
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

fn push_literal(buffer: &mut Vec<u8>, literal: &ScanLiteral) -> Result<(), AssignmentDigestError> {
    buffer.push(literal.digest_tag());
    match literal {
        ScanLiteral::Bool(value) => buffer.push(u8::from(*value)),
        ScanLiteral::I64(value) => buffer.extend_from_slice(&value.to_be_bytes()),
        ScanLiteral::U64(value) => buffer.extend_from_slice(&value.to_be_bytes()),
        ScanLiteral::F64Bits(bits) => buffer.extend_from_slice(&bits.to_be_bytes()),
        ScanLiteral::Utf8(value) => push_string(buffer, value)?,
        ScanLiteral::TimestampMicros(value) => buffer.extend_from_slice(&value.to_be_bytes()),
    }
    Ok(())
}

fn push_predicate(
    buffer: &mut Vec<u8>,
    predicate: &ScanPredicate,
) -> Result<(), AssignmentDigestError> {
    buffer.push(predicate.digest_op_tag());
    push_string(buffer, predicate.column())?;
    push_option(buffer, predicate.literal(), push_literal)
}

/// Appends one optional big-endian `i64` as `option_tag:u8 || value:i64`.
///
/// The tag is what keeps an absent bound distinct from a zero one: without it,
/// a file that declares no interval and a file that declares the epoch instant
/// would sign identically.
fn push_optional_i64(buffer: &mut Vec<u8>, value: Option<i64>) {
    match value {
        None => buffer.push(0),
        Some(value) => {
            buffer.push(1);
            buffer.extend_from_slice(&value.to_be_bytes());
        }
    }
}

/// Appends one typed persisted-file descriptor.
///
/// The leading source tag is what makes the two variants unambiguous to the
/// digest: `1` for hot, `2` for Iceberg. Tag `0` is deliberately unused so an
/// all-zero or truncated buffer cannot decode as a valid descriptor. Every
/// field except the path is fixed width, and the path keeps its `u32` length
/// prefix, so no two distinct descriptors can encode to the same bytes.
///
/// # Errors
/// Returns [`AssignmentDigestError::LengthOverflow`] when the path exceeds the
/// `u32` length domain.
fn push_file_descriptor(
    buffer: &mut Vec<u8>,
    descriptor: &crate::vala::api::PersistedFileDescriptor,
) -> Result<(), AssignmentDigestError> {
    use crate::vala::api::PersistedFileDescriptor;
    match descriptor {
        PersistedFileDescriptor::Hot(hot) => {
            buffer.push(1);
            push_string(buffer, &hot.path)?;
            buffer.extend_from_slice(&hot.size_bytes.to_be_bytes());
            buffer.extend_from_slice(&hot.row_count.to_be_bytes());
            buffer.extend_from_slice(hot.file_list_id.as_bytes());
            buffer.extend_from_slice(&hot.sha256);
            push_optional_i64(buffer, hot.min_event_time_micros);
            push_optional_i64(buffer, hot.max_event_time_micros);
        }
        PersistedFileDescriptor::Iceberg(iceberg) => {
            buffer.push(2);
            push_string(buffer, &iceberg.path)?;
            buffer.extend_from_slice(&iceberg.size_bytes.to_be_bytes());
            buffer.extend_from_slice(&iceberg.row_count.to_be_bytes());
            buffer.extend_from_slice(&iceberg.snapshot_id.to_be_bytes());
            push_optional_i64(buffer, iceberg.min_event_time_micros);
            push_optional_i64(buffer, iceberg.max_event_time_micros);
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

fn push_scribe_cut(
    buffer: &mut Vec<u8>,
    cut: &crate::vala::api::ScribeProviderCut,
) -> Result<(), AssignmentDigestError> {
    buffer.extend_from_slice(&cut.writer_epoch.to_be_bytes());
    push_time_partition(buffer, cut.start_partition);
    push_time_partition(buffer, cut.end_partition);
    let batch_count = cut.maximum_batch_count;
    buffer.extend_from_slice(&batch_count.to_be_bytes());
    buffer.extend_from_slice(&cut.maximum_retained_bytes.to_be_bytes());
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
/// Returns [`AssignmentDigestError`] when the schema fingerprint or the reader
/// cut's ancestry digest is not canonical hex, or when any length exceeds the
/// `u32` domain the encoding uses.
/// Appends the reader cut's canonical digest bytes.
///
/// Every field is signed, including the ancestry path element by element: a
/// follower that validated the signature has validated the exact lineage it
/// will have to protect, not merely the snapshot's identifier.
///
/// # Errors
///
/// Returns [`AssignmentDigestError::InvalidAncestryDigest`] when the digest is
/// not 64 lowercase hex characters, and
/// [`AssignmentDigestError::LengthOverflow`] when the ancestry path exceeds the
/// `u32` count domain.
fn push_reader_cut(
    buffer: &mut Vec<u8>,
    cut: &crate::vala::api::FollowerReaderCut,
) -> Result<(), AssignmentDigestError> {
    buffer.extend_from_slice(cut.table_uid.as_bytes());
    buffer.extend_from_slice(&cut.snapshot_id.to_be_bytes());
    buffer.extend_from_slice(&cut.snapshot_timestamp_ms.to_be_bytes());
    buffer.extend_from_slice(&cut.retained_head_snapshot_id.to_be_bytes());
    push_count(buffer, cut.ancestry_path.len(), "ancestry_path")?;
    for snapshot_id in &cut.ancestry_path {
        buffer.extend_from_slice(&snapshot_id.to_be_bytes());
    }
    buffer.extend_from_slice(&cut.ancestry_digest_version.to_be_bytes());
    let digest = decode_ancestry_digest(&cut.ancestry_digest_hex)?;
    buffer.extend_from_slice(&digest);
    buffer.extend_from_slice(&cut.target_epoch_fence.to_be_bytes());
    Ok(())
}

/// Decodes the reader cut's canonical lowercase 64-hex ancestry digest.
///
/// # Errors
///
/// Returns [`AssignmentDigestError::InvalidAncestryDigest`] when the input is
/// not exactly 64 lowercase hex characters.
fn decode_ancestry_digest(hex: &str) -> Result<[u8; 32], AssignmentDigestError> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AssignmentDigestError::InvalidAncestryDigest(
            hex.to_string(),
        ));
    }
    let mut out = [0u8; 32];
    for (index, chunk) in out.iter_mut().enumerate() {
        *chunk = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| AssignmentDigestError::InvalidAncestryDigest(hex.to_string()))?;
    }
    Ok(out)
}

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
    push_count(buffer, assignment.files.len(), "files")?;
    for file in assignment.files {
        push_file_descriptor(buffer, file)?;
    }
    push_option(buffer, assignment.scribe_cut, push_scribe_cut)?;
    push_count(
        buffer,
        assignment.required_columns.len(),
        "required_columns",
    )?;
    for column in assignment.required_columns {
        push_string(buffer, column)?;
    }
    push_reader_cut(buffer, assignment.reader_cut)?;
    push_count(buffer, assignment.predicates.len(), "predicates")?;
    for predicate in assignment.predicates {
        push_predicate(buffer, predicate)?;
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
/// This is the value a leader signs into the ticket claims and a follower
/// recomputes and compares before resolving any provider or issuing object
/// I/O — see the validation-precedence contract on the follower ticket path.
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

    /// The normative v5 reader cut every fixture assignment is signed with.
    ///
    /// Table `0a0b0c0d-...`, snapshot `8_675_309` at `1_787_497_200_000`,
    /// retained head `8_675_311`, a three-element ancestry, digest version 1,
    /// an `0x22`-filled ancestry digest, and epoch fence 9. Held as one shared
    /// value so every vector below signs the same cut and any digest change a
    /// test observes is attributable to the field it varied.
    static NORMATIVE_READER_CUT: std::sync::LazyLock<crate::vala::api::FollowerReaderCut> =
        std::sync::LazyLock::new(normative_reader_cut);

    /// Builds the normative v5 reader cut.
    fn normative_reader_cut() -> crate::vala::api::FollowerReaderCut {
        crate::vala::api::FollowerReaderCut {
            table_uid: uuid::Uuid::parse_str("0a0b0c0d-0e0f-1011-1213-141516171819")
                .expect("fixture table identity parses"),
            snapshot_id: 8_675_309,
            snapshot_timestamp_ms: 1_787_497_200_000,
            retained_head_snapshot_id: 8_675_311,
            ancestry_path: vec![8_675_311, 8_675_310, 8_675_309],
            ancestry_digest_version: 1,
            ancestry_digest_hex: "22".repeat(32),
            target_epoch_fence: 9,
        }
    }
    use crate::vala::api::{ScribeProviderCut, TimeGranularityWire, TimePartitionWire};

    /// Builds the normative v3 Scribe cut: writer epoch 7, the two hourly
    /// partitions `2026-08-23T14:00:00Z` and `2026-08-23T15:00:00Z`, 16 maximum
    /// batches, and 1 MiB maximum retained bytes. The projection closure lives
    /// on the enclosing assignment, never on the cut.
    /// The normative v4 hot descriptor: the same object path v3 signed as a
    /// bare string, now carrying its size, record count, `vala.file_list`
    /// identity, decoded checksum, and declared event-time interval.
    fn normative_hot_descriptor() -> crate::vala::api::PersistedFileDescriptor {
        crate::vala::api::PersistedFileDescriptor::Hot(crate::vala::api::HotFileDescriptor {
            path: "s3://bucket/logs/a.parquet".to_string(),
            size_bytes: 4_194_304,
            row_count: 128,
            file_list_id: uuid::Uuid::parse_str("0f0e0d0c-0b0a-0908-0706-050403020100")
                .expect("fixture file-list identity parses"),
            sha256: [0x11; 32],
            min_event_time_micros: Some(1_787_493_600_000_000),
            max_event_time_micros: Some(1_787_497_200_000_000),
        })
    }

    /// The normative v4 Iceberg descriptor, used to prove the source variant is
    /// itself authoritative rather than inferred from the path.
    fn normative_iceberg_descriptor() -> crate::vala::api::PersistedFileDescriptor {
        crate::vala::api::PersistedFileDescriptor::Iceberg(
            crate::vala::api::IcebergFileDescriptor {
                path: "s3://bucket/logs/a.parquet".to_string(),
                size_bytes: 4_194_304,
                row_count: 128,
                snapshot_id: 8_675_309,
                min_event_time_micros: Some(1_787_493_600_000_000),
                max_event_time_micros: Some(1_787_497_200_000_000),
            },
        )
    }

    fn normative_scribe_cut() -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 7,
            start_partition: hour_partition(1_787_493_600_000_000),
            end_partition: hour_partition(1_787_497_200_000_000),
            maximum_batch_count: 16,
            maximum_retained_bytes: 1_048_576,
        }
    }

    /// Builds one hourly partition from its exact epoch-microsecond boundary.
    fn hour_partition(start_unix_micros: i64) -> TimePartitionWire {
        let start = chrono::DateTime::from_timestamp_micros(start_unix_micros)
            .expect("fixture start is representable");
        TimePartitionWire::new(TimeGranularityWire::Hour, start)
            .expect("fixture start is canonical")
    }

    /// Normative v5 vector: one assignment for tenant
    /// `00112233-4455-6677-8899-aabbccddeeff`, table `logs.records`, fingerprint
    /// `00..1f`, one typed hot descriptor, three required columns carried once
    /// on the assignment, a single `Eq(service_name, "api")` predicate, and the
    /// normative Scribe cut and reader cut must encode to exactly 472 bytes and
    /// hash to the fixed digest below. Asserting both the byte length and the
    /// hash prevents a compensating pair of layout mistakes from passing.
    ///
    /// The 112-byte growth over v4's 360 is exactly the reader cut: 16 table
    /// uuid + 8 snapshot + 8 snapshot timestamp + 8 retained head + 4 ancestry
    /// count + 24 for its three entries + 4 digest version + 32 digest + 8
    /// epoch fence.
    #[test]
    fn normative_vector_encodes_to_fixed_length_and_digest() {
        let fingerprint: String = (0u8..32).map(|byte| format!("{byte:02x}")).collect();
        let tenant_uuid = uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap();
        let files = vec![normative_hot_descriptor()];
        let required_columns = vec![
            "service_name".to_string(),
            "wyrd_event_time".to_string(),
            "data_tenant_id".to_string(),
        ];
        let predicates = vec![ScanPredicate::Eq(
            "service_name".to_string(),
            ScanLiteral::Utf8("api".to_string()),
        )];
        let cut = normative_scribe_cut();
        let assignment = AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            files: &files,
            scribe_cut: Some(&cut),
            required_columns: &required_columns,
            predicates: &predicates,
            reader_cut: &NORMATIVE_READER_CUT,
        };

        let bytes = encode_assignment_authority_bytes(std::slice::from_ref(&assignment)).unwrap();
        assert_eq!(
            bytes.len(),
            472,
            "normative vector must encode to exactly 472 bytes"
        );

        let digest = assignment_authority_digest(std::slice::from_ref(&assignment)).unwrap();
        assert_eq!(
            digest,
            "df6be129b4cd43adc133c3adca78a4ea91bdd71716cc2f365f43e28534ea40e6"
        );
    }

    /// Every reader-cut field independently moves the assignment digest.
    ///
    /// The cut is the follower's entire authority over which snapshot it may
    /// read. If any field could be changed without invalidating the signature,
    /// a peer could redirect a follower onto a different table's snapshot, a
    /// different point in the same table's history, or the same snapshot under
    /// a stale epoch fence, and still present a valid assignment.
    ///
    /// Ancestry is checked element by element and by length, because coverage
    /// is decided from the path: a truncated or reordered path would let a
    /// follower protect less than it reads.
    #[test]
    fn follower_reader_cut_changes_assignment_digest() {
        use crate::vala::api::FollowerReaderCut;

        let (fingerprint, tenant_uuid, files, required_columns, predicates) = base_vector();
        let scribe_cut = normative_scribe_cut();
        let digest_with = |cut: &FollowerReaderCut| {
            let input = AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: Some(&scribe_cut),
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: cut,
            };
            assignment_authority_digest(std::slice::from_ref(&input))
                .expect("fixture reader cut is canonical")
        };

        let baseline = digest_with(&NORMATIVE_READER_CUT);
        let mutations: Vec<(&str, FollowerReaderCut)> = vec![
            (
                "table_uid",
                FollowerReaderCut {
                    table_uid: uuid::Uuid::nil(),
                    ..normative_reader_cut()
                },
            ),
            (
                "snapshot_id",
                FollowerReaderCut {
                    snapshot_id: 8_675_308,
                    ..normative_reader_cut()
                },
            ),
            (
                "snapshot_timestamp_ms",
                FollowerReaderCut {
                    snapshot_timestamp_ms: 1_787_497_200_001,
                    ..normative_reader_cut()
                },
            ),
            (
                "retained_head_snapshot_id",
                FollowerReaderCut {
                    retained_head_snapshot_id: 8_675_312,
                    ..normative_reader_cut()
                },
            ),
            (
                "ancestry_path truncated",
                FollowerReaderCut {
                    ancestry_path: vec![8_675_311, 8_675_309],
                    ..normative_reader_cut()
                },
            ),
            (
                "ancestry_path reordered",
                FollowerReaderCut {
                    ancestry_path: vec![8_675_311, 8_675_309, 8_675_310],
                    ..normative_reader_cut()
                },
            ),
            (
                "ancestry_digest_version",
                FollowerReaderCut {
                    ancestry_digest_version: 2,
                    ..normative_reader_cut()
                },
            ),
            (
                "ancestry_digest",
                FollowerReaderCut {
                    ancestry_digest_hex: "23".repeat(32),
                    ..normative_reader_cut()
                },
            ),
            (
                "target_epoch_fence",
                FollowerReaderCut {
                    target_epoch_fence: 10,
                    ..normative_reader_cut()
                },
            ),
        ];
        for (field, mutated) in mutations {
            assert_ne!(
                digest_with(&mutated),
                baseline,
                "reader-cut field {field} must move the assignment digest"
            );
        }

        // A non-canonical ancestry digest is refused rather than hashed as
        // whatever bytes it happens to contain.
        let malformed = FollowerReaderCut {
            ancestry_digest_hex: "NOT-HEX".to_string(),
            ..normative_reader_cut()
        };
        let input = AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            files: &files,
            scribe_cut: Some(&scribe_cut),
            required_columns: &required_columns,
            predicates: &predicates,
            reader_cut: &malformed,
        };
        assert!(matches!(
            assignment_authority_digest(std::slice::from_ref(&input)),
            Err(AssignmentDigestError::InvalidAncestryDigest(_))
        ));
    }

    /// Every descriptor field and the descriptor order are independently
    /// authoritative.
    ///
    /// A follower trusts the descriptor instead of re-querying the catalog, so
    /// any fact a leader could have signed differently — the source variant, the
    /// path, the size, the record count, the `vala.file_list` identity, the
    /// decoded checksum, the pinned snapshot, either event-time bound, or the
    /// position in the list — must move the digest. If one did not, a peer could
    /// substitute a different object, or the same object under different claimed
    /// statistics, and still present a valid signature.
    ///
    /// The two variants deliberately share a path here: the source tag alone
    /// must separate them, because a path is not publication authority.
    #[test]
    fn assignment_authority_v4_binds_every_descriptor_field_and_order() {
        use crate::vala::api::PersistedFileDescriptor;

        let (fingerprint, tenant_uuid, files, required_columns, predicates) = base_vector();
        let cut = normative_scribe_cut();
        let digest_with = |files: &[PersistedFileDescriptor]| {
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files,
                scribe_cut: Some(&cut),
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        };
        let baseline = digest_with(&files);

        // The source tag separates two descriptors that name the same path.
        assert_ne!(
            baseline,
            digest_with(std::slice::from_ref(&normative_iceberg_descriptor())),
            "the source variant is authoritative, not inferred from the path"
        );

        let hot_mutation = |mutate: fn(&mut crate::vala::api::HotFileDescriptor)| {
            let mut descriptor = normative_hot_descriptor();
            if let PersistedFileDescriptor::Hot(hot) = &mut descriptor {
                mutate(hot);
            }
            vec![descriptor]
        };
        for (field, mutated) in [
            (
                "path",
                hot_mutation(|hot| hot.path = "s3://bucket/logs/b.parquet".to_string()),
            ),
            ("size_bytes", hot_mutation(|hot| hot.size_bytes += 1)),
            ("row_count", hot_mutation(|hot| hot.row_count += 1)),
            (
                "file_list_id",
                hot_mutation(|hot| hot.file_list_id = uuid::Uuid::from_u128(9)),
            ),
            ("sha256", hot_mutation(|hot| hot.sha256[31] ^= 0x01)),
            (
                "min_event_time_micros value",
                hot_mutation(|hot| hot.min_event_time_micros = Some(1_787_493_600_000_001)),
            ),
            (
                "min_event_time_micros presence",
                hot_mutation(|hot| {
                    hot.min_event_time_micros = None;
                    hot.max_event_time_micros = None;
                }),
            ),
            (
                "max_event_time_micros value",
                hot_mutation(|hot| hot.max_event_time_micros = Some(1_787_497_200_000_001)),
            ),
        ] {
            assert_ne!(baseline, digest_with(&mutated), "{field} must be bound");
        }

        let iceberg_mutation = |mutate: fn(&mut crate::vala::api::IcebergFileDescriptor)| {
            let mut descriptor = normative_iceberg_descriptor();
            if let PersistedFileDescriptor::Iceberg(iceberg) = &mut descriptor {
                mutate(iceberg);
            }
            vec![descriptor]
        };
        let iceberg_baseline = digest_with(std::slice::from_ref(&normative_iceberg_descriptor()));
        for (field, mutated) in [
            ("size_bytes", iceberg_mutation(|file| file.size_bytes += 1)),
            ("row_count", iceberg_mutation(|file| file.row_count += 1)),
            (
                "snapshot_id",
                iceberg_mutation(|file| file.snapshot_id += 1),
            ),
            (
                "max_event_time_micros",
                iceberg_mutation(|file| file.max_event_time_micros = Some(1_787_497_200_000_001)),
            ),
        ] {
            assert_ne!(
                iceberg_baseline,
                digest_with(&mutated),
                "iceberg {field} must be bound"
            );
        }

        // Order is authoritative: the same two descriptors in the other order
        // must not produce the same digest.
        let forward = vec![normative_hot_descriptor(), normative_iceberg_descriptor()];
        let reversed = vec![normative_iceberg_descriptor(), normative_hot_descriptor()];
        assert_ne!(
            digest_with(&forward),
            digest_with(&reversed),
            "descriptor order must be bound"
        );

        // The v4 domain must separate the same logical assignment from v3.
        assert_ne!(
            baseline, "949a3d1c1e779d23e57fbd1569fab6cdbcd65dd30eac5a0dfa1ab5facd7b4394",
            "a v3 signature must never validate against v4 bytes"
        );
    }

    /// Each partition bound is independently authoritative: changing either
    /// endpoint's granularity or start instant must move the digest, so a
    /// signed cut cannot be replayed against a different partition range.
    #[test]
    fn every_partition_bound_component_changes_the_digest() {
        let (fingerprint, tenant_uuid, files, required_columns, predicates) = base_vector();
        let baseline_cut = normative_scribe_cut();
        let digest_with = |cut: &ScribeProviderCut| {
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: Some(cut),
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        };
        let baseline = digest_with(&baseline_cut);

        let mut moved_start = baseline_cut.clone();
        moved_start.start_partition = hour_partition(1_787_490_000_000_000);
        assert_ne!(baseline, digest_with(&moved_start));

        let mut moved_end = baseline_cut.clone();
        moved_end.end_partition = hour_partition(1_787_500_800_000_000);
        assert_ne!(baseline, digest_with(&moved_end));

        let day_start = chrono::DateTime::from_timestamp_micros(1_787_443_200_000_000)
            .expect("fixture start is representable");
        let mut regranulated = baseline_cut.clone();
        regranulated.start_partition = TimePartitionWire::new(TimeGranularityWire::Day, day_start)
            .expect("fixture day start is canonical");
        regranulated.end_partition = regranulated.start_partition;
        assert_ne!(baseline, digest_with(&regranulated));
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

    fn base_vector() -> (
        String,
        uuid::Uuid,
        Vec<crate::vala::api::PersistedFileDescriptor>,
        Vec<String>,
        Vec<ScanPredicate>,
    ) {
        let fingerprint: String = (0u8..32).map(|byte| format!("{byte:02x}")).collect();
        let tenant_uuid = uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap();
        let files = vec![normative_hot_descriptor()];
        let required_columns = vec![
            "service_name".to_string(),
            "wyrd_event_time".to_string(),
            "data_tenant_id".to_string(),
        ];
        (fingerprint, tenant_uuid, files, required_columns, vec![])
    }

    fn digest_of(assignment: &AssignmentDigestInput<'_>) -> String {
        assignment_authority_digest(std::slice::from_ref(assignment)).unwrap()
    }

    fn baseline_digest(predicates: &[ScanPredicate]) -> String {
        let (fingerprint, tenant_uuid, files, required_columns, _) = base_vector();
        digest_of(&AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            files: &files,
            scribe_cut: None,
            required_columns: &required_columns,
            predicates,
            reader_cut: &NORMATIVE_READER_CUT,
        })
    }

    /// Every mutation class the packet requires must change the digest:
    /// scan id, tenant, namespace, table, fingerprint bytes, file path bytes,
    /// list reordering, float bits, and predicate operator/column/literal.
    #[test]
    fn every_field_class_mutation_changes_the_digest() {
        let predicates = vec![ScanPredicate::Eq(
            "service_name".to_string(),
            ScanLiteral::Utf8("api".to_string()),
        )];
        let baseline = baseline_digest(&predicates);
        let (fingerprint, tenant_uuid, files, required_columns, _) = base_vector();

        // scan_id
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-2",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: None,
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // tenant uuid
        let other_tenant = uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeef0").unwrap();
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid: other_tenant,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: None,
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // namespace
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logz",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: None,
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // table
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "recordz",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: None,
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // fingerprint bytes
        let mut mutated_fingerprint = fingerprint.clone();
        mutated_fingerprint.replace_range(0..2, "ff");
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &mutated_fingerprint,
                files: &files,
                scribe_cut: None,
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // file path bytes
        let mutated_files = vec![{
            let mut descriptor = normative_hot_descriptor();
            if let crate::vala::api::PersistedFileDescriptor::Hot(hot) = &mut descriptor {
                hot.path = "s3://bucket/logs/b.parquet".to_string();
            }
            descriptor
        }];
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &mutated_files,
                scribe_cut: None,
                required_columns: &required_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // required_columns reordered
        let reordered_columns = vec![
            "wyrd_event_time".to_string(),
            "service_name".to_string(),
            "data_tenant_id".to_string(),
        ];
        assert_ne!(
            baseline,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: None,
                required_columns: &reordered_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // predicate operator
        let noteq_predicate = vec![ScanPredicate::NotEq(
            "service_name".to_string(),
            ScanLiteral::Utf8("api".to_string()),
        )];
        assert_ne!(baseline, baseline_digest(&noteq_predicate));

        // predicate column
        let other_column_predicate = vec![ScanPredicate::Eq(
            "other_column".to_string(),
            ScanLiteral::Utf8("api".to_string()),
        )];
        assert_ne!(baseline, baseline_digest(&other_column_predicate));

        // predicate literal
        let other_literal_predicate = vec![ScanPredicate::Eq(
            "service_name".to_string(),
            ScanLiteral::Utf8("apk".to_string()),
        )];
        assert_ne!(baseline, baseline_digest(&other_literal_predicate));

        // float bits
        let float_baseline = baseline_digest(&[ScanPredicate::Gt(
            "duration_ms".to_string(),
            ScanLiteral::F64Bits(1.0_f64.to_bits()),
        )]);
        let float_mutated = baseline_digest(&[ScanPredicate::Gt(
            "duration_ms".to_string(),
            ScanLiteral::F64Bits(1.0000001_f64.to_bits()),
        )]);
        assert_ne!(float_baseline, float_mutated);
    }

    #[test]
    fn scribe_cut_presence_and_content_changes_the_digest() {
        let (fingerprint, tenant_uuid, files, required_columns, predicates) = base_vector();
        let no_cut = digest_of(&AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            files: &files,
            scribe_cut: None,
            required_columns: &required_columns,
            predicates: &predicates,
            reader_cut: &NORMATIVE_READER_CUT,
        });
        let cut = normative_scribe_cut();
        let with_cut = digest_of(&AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            files: &files,
            scribe_cut: Some(&cut),
            required_columns: &required_columns,
            predicates: &predicates,
            reader_cut: &NORMATIVE_READER_CUT,
        });
        assert_ne!(no_cut, with_cut);

        let mut mutated_cut = cut.clone();
        mutated_cut.writer_epoch = 2;
        let with_mutated_cut = digest_of(&AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: &fingerprint,
            files: &files,
            scribe_cut: Some(&mutated_cut),
            required_columns: &required_columns,
            predicates: &predicates,
            reader_cut: &NORMATIVE_READER_CUT,
        });
        assert_ne!(with_cut, with_mutated_cut);

        // The projection closure is carried once, on the assignment. With a cut
        // present it must still be signed there, so a peer cannot widen a
        // Scribe follower's projection while replaying the same cut.
        let reordered_columns = vec![
            "wyrd_event_time".to_string(),
            "service_name".to_string(),
            "data_tenant_id".to_string(),
        ];
        assert_ne!(
            with_cut,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: Some(&cut),
                required_columns: &reordered_columns,
                predicates: &predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );

        // Predicates stay signed alongside a cut for the same reason.
        let widened_predicates = vec![ScanPredicate::NotEq(
            "service_name".to_string(),
            ScanLiteral::Utf8("api".to_string()),
        )];
        assert_ne!(
            with_cut,
            digest_of(&AssignmentDigestInput {
                scan_id: "scan-1",
                tenant_uuid,
                namespace: "logs",
                table: "records",
                schema_fingerprint_hex: &fingerprint,
                files: &files,
                scribe_cut: Some(&cut),
                required_columns: &required_columns,
                predicates: &widened_predicates,
                reader_cut: &NORMATIVE_READER_CUT,
            })
        );
    }

    #[test]
    fn rejects_non_canonical_schema_fingerprint() {
        let (_, tenant_uuid, files, required_columns, predicates) = base_vector();
        let assignment = AssignmentDigestInput {
            scan_id: "scan-1",
            tenant_uuid,
            namespace: "logs",
            table: "records",
            schema_fingerprint_hex: "not-hex",
            files: &files,
            scribe_cut: None,
            required_columns: &required_columns,
            predicates: &predicates,
            reader_cut: &NORMATIVE_READER_CUT,
        };
        assert!(matches!(
            assignment_authority_digest(std::slice::from_ref(&assignment)),
            Err(AssignmentDigestError::InvalidSchemaFingerprint(_))
        ));
    }
}

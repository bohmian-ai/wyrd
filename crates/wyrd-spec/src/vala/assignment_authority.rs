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
    /// Canonical persisted file paths assigned to this follower, in order.
    pub files: &'a [String],
    /// Optional Scribe memory-provider cut for this assignment.
    pub scribe_cut: Option<&'a crate::vala::api::ScribeProviderCut>,
    /// Required output/predicate/hidden-tenant projection closure, in order.
    pub required_columns: &'a [String],
    /// Closed leaf predicates, in filter order.
    pub predicates: &'a [ScanPredicate],
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

/// Domain separator for the v3 assignment-authority digest.
///
/// v3 replaced v2's two length-prefixed event-day strings with two fixed-width
/// typed partitions, and carries the projection closure exactly once, on the
/// assignment itself: the Scribe cut contributes only its epoch, partitions,
/// cursor, ranges, and retention bounds. Every other count, length, option,
/// predicate, file, projection, and numeric rule is unchanged from v2, and the
/// domain differs so a v2 signature can never validate against v3 bytes.
const ASSIGNMENT_AUTHORITY_DOMAIN: &[u8] = b"wyrd.oracle.assignment-authority.v3\0";

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
    buffer.extend_from_slice(&cut.persisted_cursor.to_be_bytes());
    push_count(
        buffer,
        cut.persisted_ranges.len(),
        "scribe_cut.persisted_ranges",
    )?;
    for range in &cut.persisted_ranges {
        buffer.extend_from_slice(&range.start_lsn.to_be_bytes());
        buffer.extend_from_slice(&range.end_lsn.to_be_bytes());
    }
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
        push_string(buffer, file)?;
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
    use crate::vala::api::{ScribeProviderCut, TimeGranularityWire, TimePartitionWire};

    /// Builds the normative v3 Scribe cut: writer epoch 7, the two hourly
    /// partitions `2026-08-23T14:00:00Z` and `2026-08-23T15:00:00Z`, persisted
    /// cursor 41 with the single range `[1, 40]`, 16 maximum batches, and 1 MiB
    /// maximum retained bytes. The projection closure lives on the enclosing
    /// assignment, never on the cut.
    fn normative_scribe_cut() -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 7,
            start_partition: hour_partition(1_787_493_600_000_000),
            end_partition: hour_partition(1_787_497_200_000_000),
            persisted_cursor: 41,
            persisted_ranges: vec![crate::vala::api::PersistedWalRange {
                start_lsn: 1,
                end_lsn: 40,
            }],
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

    /// Normative v3 vector from the task packet: one assignment for tenant
    /// `00112233-4455-6677-8899-aabbccddeeff`, table `logs.records`, fingerprint
    /// `00..1f`, one file, three required columns carried once on the
    /// assignment, a single `Eq(service_name, "api")` predicate, and the
    /// normative Scribe cut must encode to exactly 305 bytes and hash to the
    /// fixed digest below. Asserting both the byte length and the hash prevents
    /// a compensating pair of layout mistakes from passing.
    #[test]
    fn normative_vector_encodes_to_fixed_length_and_digest() {
        let fingerprint: String = (0u8..32).map(|byte| format!("{byte:02x}")).collect();
        let tenant_uuid = uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap();
        let files = vec!["s3://bucket/logs/a.parquet".to_string()];
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
        };

        let bytes = encode_assignment_authority_bytes(std::slice::from_ref(&assignment)).unwrap();
        assert_eq!(
            bytes.len(),
            305,
            "normative vector must encode to exactly 305 bytes"
        );

        let digest = assignment_authority_digest(std::slice::from_ref(&assignment)).unwrap();
        assert_eq!(
            digest,
            "a8ec167a1925681edb74bee14a50a3c9e682bc3c0f62f3bead333e5156711250"
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
        Vec<String>,
        Vec<String>,
        Vec<ScanPredicate>,
    ) {
        let fingerprint: String = (0u8..32).map(|byte| format!("{byte:02x}")).collect();
        let tenant_uuid = uuid::Uuid::parse_str("00112233-4455-6677-8899-aabbccddeeff").unwrap();
        let files = vec!["s3://bucket/logs/a.parquet".to_string()];
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
            })
        );

        // file path bytes
        let mutated_files = vec!["s3://bucket/logs/b.parquet".to_string()];
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
        };
        assert!(matches!(
            assignment_authority_digest(std::slice::from_ref(&assignment)),
            Err(AssignmentDigestError::InvalidSchemaFingerprint(_))
        ));
    }
}

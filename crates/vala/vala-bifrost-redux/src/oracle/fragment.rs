//! Closed, deterministic work units for immutable sealed scans.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Immutable sealed source selected for one fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedScanFile {
    /// Tenant-qualified object location.
    pub location: String,
    /// Inclusive row-group indexes selected from the file.
    pub row_groups: Vec<u32>,
    /// Exact manifest byte size used for bounded reads.
    pub size_bytes: u64,
    /// Exact manifest row estimate used for fragment sizing.
    pub estimated_rows: u64,
}

/// Only the two immutable source tiers eligible for peer execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SealedSourceTier {
    /// Published Iceberg data.
    Iceberg,
    /// Hot sealed files absent from the pinned snapshot.
    HotSealed,
}

/// Closed comparison operations accepted by the sealed-leaf executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeafComparison {
    /// Equal to the typed literal.
    Eq,
    /// Not equal to the typed literal.
    NotEq,
    /// Less than the typed literal.
    Lt,
    /// Less than or equal to the typed literal.
    LtEq,
    /// Greater than the typed literal.
    Gt,
    /// Greater than or equal to the typed literal.
    GtEq,
}

/// Closed typed literals accepted by peer leaf predicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeafScalar {
    /// Boolean literal.
    Boolean(bool),
    /// Signed 64-bit integer literal.
    Int64(i64),
    /// Unsigned 64-bit integer literal.
    UInt64(u64),
    /// IEEE-754 double literal represented by its exact bit pattern.
    Float64Bits(u64),
    /// UTF-8 string literal.
    Utf8(String),
    /// Microseconds since the Unix epoch.
    TimestampMicros(i64),
}

/// Closed leaf predicate representation containing no SQL or serialized plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClosedLeafPredicate {
    /// Selects rows where the named column is null.
    IsNull {
        /// Authorized physical column name.
        column: String,
    },
    /// Selects rows where the named column is not null.
    IsNotNull {
        /// Authorized physical column name.
        column: String,
    },
    /// Compares one named column with one typed literal.
    Compare {
        /// Authorized physical column name.
        column: String,
        /// Closed comparison operator.
        operation: LeafComparison,
        /// Typed literal whose Arrow type must match the column.
        value: LeafScalar,
    },
}

/// Prepared sealed leaf input used by [`FragmentPlanner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSealedLeaf {
    /// Tenant-qualified logical binding.
    pub binding: String,
    /// Immutable source tier.
    pub tier: SealedSourceTier,
    /// Pinned snapshot or manifest digest.
    pub pinned_digest: String,
    /// Ordered object work.
    pub files: Vec<SealedScanFile>,
    /// Projection columns, in requested order.
    pub projection: Vec<String>,
    /// Closed leaf predicates; no SQL or physical plan text is accepted.
    pub predicates: Vec<ClosedLeafPredicate>,
    /// Expected schema fingerprint.
    pub schema_fingerprint: String,
    /// Absolute hard deadline represented as Unix milliseconds.
    pub deadline_unix_ms: i64,
}

/// Deterministic bounded fragment configuration.
#[derive(Debug, Clone, Copy)]
pub struct FragmentConfig {
    /// Maximum files represented by one fragment.
    pub max_files: usize,
}

impl Default for FragmentConfig {
    fn default() -> Self {
        Self { max_files: 16 }
    }
}

/// A closed micro-fragment containing only immutable sealed scan work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedScanFragment {
    /// Stable digest-derived identity.
    pub fragment_id: String,
    /// Tenant-qualified binding.
    pub binding: String,
    /// Source tier.
    pub tier: SealedSourceTier,
    /// Pinned source digest.
    pub pinned_digest: String,
    /// Ordered files and row groups.
    pub files: Vec<SealedScanFile>,
    /// Projection columns.
    pub projection: Vec<String>,
    /// Closed leaf predicates.
    pub predicates: Vec<ClosedLeafPredicate>,
    /// Expected schema fingerprint.
    pub schema_fingerprint: String,
    /// Estimated rows in the ordered manifest.
    pub estimated_rows: u64,
    /// Estimated bytes in the ordered manifest.
    pub estimated_bytes: u64,
    /// Absolute hard deadline represented as Unix milliseconds.
    pub deadline_unix_ms: i64,
}

/// Fragment planning failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FragmentError {
    /// The configured fragment size cannot make progress.
    #[error("fragment file limit must be positive")]
    InvalidLimit,
    /// The leaf has no immutable work.
    #[error("sealed leaf has no files")]
    EmptyLeaf,
    /// A manifest estimate overflowed its fixed wire width.
    #[error("sealed fragment estimate overflow")]
    EstimateOverflow,
    /// A fragment could not be encoded into its closed transport shape.
    #[error("sealed fragment encoding failed")]
    Encoding,
}

/// Stateless owner for deterministic sealed-leaf fragmentation.
#[derive(Debug, Clone, Copy, Default)]
pub struct FragmentPlanner;

impl FragmentPlanner {
    /// Splits an eligible leaf into ordered deterministic micro-fragments.
    ///
    /// # Errors
    /// Returns [`FragmentError`] when the limit is zero or the leaf is empty.
    pub fn plan(
        &self,
        leaf: &PreparedSealedLeaf,
        config: &FragmentConfig,
    ) -> Result<Vec<SealedScanFragment>, FragmentError> {
        if config.max_files == 0 {
            return Err(FragmentError::InvalidLimit);
        }
        if leaf.files.is_empty() {
            return Err(FragmentError::EmptyLeaf);
        }
        leaf.files
            .chunks(config.max_files)
            .map(|files| {
                let estimated_rows = files
                    .iter()
                    .try_fold(0_u64, |total, file| total.checked_add(file.estimated_rows));
                let estimated_bytes = files
                    .iter()
                    .try_fold(0_u64, |total, file| total.checked_add(file.size_bytes));
                let mut fragment = SealedScanFragment {
                    fragment_id: String::new(),
                    binding: leaf.binding.clone(),
                    tier: leaf.tier,
                    pinned_digest: leaf.pinned_digest.clone(),
                    files: files.to_vec(),
                    projection: leaf.projection.clone(),
                    predicates: leaf.predicates.clone(),
                    schema_fingerprint: leaf.schema_fingerprint.clone(),
                    estimated_rows: estimated_rows.ok_or(FragmentError::EstimateOverflow)?,
                    estimated_bytes: estimated_bytes.ok_or(FragmentError::EstimateOverflow)?,
                    deadline_unix_ms: leaf.deadline_unix_ms,
                };
                fragment.fragment_id = fragment.digest();
                Ok(fragment)
            })
            .collect()
    }
}

impl SealedScanFragment {
    /// Computes the stable work identity, excluding any deadline.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"wyrd.oracle.fragment.v1\0");
        for value in [&self.binding, &self.pinned_digest, &self.schema_fingerprint] {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
        hash.update([match self.tier {
            SealedSourceTier::Iceberg => 1,
            SealedSourceTier::HotSealed => 2,
        }]);
        for value in &self.projection {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
        for value in &self.predicates {
            update_predicate_digest(&mut hash, value);
        }
        for file in &self.files {
            hash.update(file.location.as_bytes());
            hash.update([0]);
            for group in &file.row_groups {
                hash.update(group.to_le_bytes());
            }
            hash.update(file.size_bytes.to_le_bytes());
            hash.update(file.estimated_rows.to_le_bytes());
        }
        hash.update(self.estimated_rows.to_le_bytes());
        hash.update(self.estimated_bytes.to_le_bytes());
        hex::encode(hash.finalize())
    }

    /// Encodes the closed fragment representation for local or tonic execution.
    ///
    /// # Errors
    /// Returns [`FragmentError::Encoding`] if the representation cannot be encoded.
    pub fn encode(&self) -> Result<Vec<u8>, FragmentError> {
        serde_json::to_vec(self).map_err(|_| FragmentError::Encoding)
    }

    /// Decodes a closed fragment representation after ticket verification.
    ///
    /// # Errors
    /// Returns [`FragmentError::Encoding`] for malformed or trailing input.
    pub fn decode(bytes: &[u8]) -> Result<Self, FragmentError> {
        serde_json::from_slice(bytes).map_err(|_| FragmentError::Encoding)
    }
}

/// Adds one closed predicate to the portable fragment preimage.
fn update_predicate_digest(hash: &mut Sha256, predicate: &ClosedLeafPredicate) {
    match predicate {
        ClosedLeafPredicate::IsNull { column } => {
            hash.update([1]);
            hash.update(column.as_bytes());
        }
        ClosedLeafPredicate::IsNotNull { column } => {
            hash.update([2]);
            hash.update(column.as_bytes());
        }
        ClosedLeafPredicate::Compare {
            column,
            operation,
            value,
        } => {
            hash.update([3]);
            hash.update(column.as_bytes());
            hash.update([match operation {
                LeafComparison::Eq => 1,
                LeafComparison::NotEq => 2,
                LeafComparison::Lt => 3,
                LeafComparison::LtEq => 4,
                LeafComparison::Gt => 5,
                LeafComparison::GtEq => 6,
            }]);
            match value {
                LeafScalar::Boolean(value) => hash.update([1, u8::from(*value)]),
                LeafScalar::Int64(value) => {
                    hash.update([2]);
                    hash.update(value.to_le_bytes());
                }
                LeafScalar::UInt64(value) => {
                    hash.update([3]);
                    hash.update(value.to_le_bytes());
                }
                LeafScalar::Float64Bits(value) => {
                    hash.update([4]);
                    hash.update(value.to_le_bytes());
                }
                LeafScalar::Utf8(value) => {
                    hash.update([5]);
                    hash.update(value.as_bytes());
                }
                LeafScalar::TimestampMicros(value) => {
                    hash.update([6]);
                    hash.update(value.to_le_bytes());
                }
            }
        }
    }
    hash.update([0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Produces one deterministic closed leaf for fragmentation tests.
    fn leaf() -> PreparedSealedLeaf {
        PreparedSealedLeaf {
            binding: "file:///warehouse/tenants/t/table".to_owned(),
            tier: SealedSourceTier::HotSealed,
            pinned_digest: "manifest".to_owned(),
            files: vec![
                SealedScanFile {
                    location: "file:///warehouse/tenants/t/table/a.parquet".to_owned(),
                    row_groups: vec![0],
                    size_bytes: 10,
                    estimated_rows: 2,
                },
                SealedScanFile {
                    location: "file:///warehouse/tenants/t/table/b.parquet".to_owned(),
                    row_groups: vec![1],
                    size_bytes: 20,
                    estimated_rows: 3,
                },
            ],
            projection: vec!["value".to_owned()],
            predicates: Vec::new(),
            schema_fingerprint: "schema".to_owned(),
            deadline_unix_ms: i64::MAX,
        }
    }

    /// Fragment IDs and bytes remain stable while deadline changes do not alter identity.
    #[test]
    fn oracle_fragment_identity_is_deterministic_and_deadline_independent() {
        let planner = FragmentPlanner;
        let mut first = leaf();
        let planned = planner
            .plan(&first, &FragmentConfig { max_files: 1 })
            .expect("fragment plan");
        first.deadline_unix_ms -= 1;
        let repeated = planner
            .plan(&first, &FragmentConfig { max_files: 1 })
            .expect("fragment plan");
        assert_eq!(planned[0].fragment_id, repeated[0].fragment_id);
        let encoded = planned[0].encode().expect("fragment encode");
        assert_eq!(
            SealedScanFragment::decode(&encoded).expect("fragment decode"),
            planned[0]
        );
        assert_eq!(planned[0].estimated_bytes, 10);
        assert_eq!(planned[0].estimated_rows, 2);
    }

    /// Empty work and a zero file bound fail before producing a fragment.
    #[test]
    fn oracle_fragment_planner_rejects_non_progressing_work() {
        let planner = FragmentPlanner;
        assert_eq!(
            planner.plan(&leaf(), &FragmentConfig { max_files: 0 }),
            Err(FragmentError::InvalidLimit)
        );
        let mut empty = leaf();
        empty.files.clear();
        assert_eq!(
            planner.plan(&empty, &FragmentConfig::default()),
            Err(FragmentError::EmptyLeaf)
        );
    }

    /// Arbitrary SQL text cannot decode as one closed leaf predicate.
    #[test]
    fn oracle_fragment_predicates_reject_sql_text() {
        assert!(
            serde_json::from_str::<ClosedLeafPredicate>("\"drop table observations\"").is_err()
        );
    }
}

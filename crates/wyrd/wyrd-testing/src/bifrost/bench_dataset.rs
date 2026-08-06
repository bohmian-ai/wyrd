//! Deterministic, scale-parameterized Bifrost qualification data contracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Exact count of microseconds in one UTC day used for integral event spacing.
const MICROS_PER_DAY: u64 = 86_400_000_000;
/// Unix-epoch microsecond instant for the deterministic dataset's first day.
const UNIX_MICROS_2026_01_01: i64 = 1_767_225_600_000_000;
/// Closed manifest identifier emitted by the deterministic dataset contract.
const DATASET_SCHEMA_VERSION: &str = "wyrd.bifrost.dataset/v1";

/// Shape of one deterministic qualification dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetShape {
    /// Number of UTC day partitions.
    pub days: u32,
    /// Number of rows in each day partition.
    pub rows_per_day: u64,
}

impl DatasetShape {
    /// Validate and construct a dataset shape.
    ///
    /// # Errors
    /// Returns [`DatasetShapeError`] when the shape has fewer than two days,
    /// no rows, or cannot represent each event-time step as whole micros.
    pub fn new(days: u32, rows_per_day: u64) -> Result<Self, DatasetShapeError> {
        if days < 2 {
            return Err(DatasetShapeError::TooFewDays { days });
        }
        if rows_per_day == 0 {
            return Err(DatasetShapeError::EmptyDay);
        }
        if !MICROS_PER_DAY.is_multiple_of(rows_per_day) {
            return Err(DatasetShapeError::NonIntegralSpacing { rows_per_day });
        }
        Ok(Self { days, rows_per_day })
    }

    /// Return the total number of rows represented by this shape.
    #[must_use]
    pub fn row_count(self) -> u64 {
        u64::from(self.days).saturating_mul(self.rows_per_day)
    }
}

/// Invalid deterministic dataset shape.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DatasetShapeError {
    /// The shape must contain at least two day partitions.
    #[error("dataset requires at least two days, got {days}")]
    TooFewDays { days: u32 },
    /// A day with no rows cannot produce event-time samples.
    #[error("dataset rows_per_day must be greater than zero")]
    EmptyDay,
    /// Event-time spacing must be an integral number of microseconds.
    #[error("rows_per_day {rows_per_day} does not divide one UTC day in micros")]
    NonIntegralSpacing { rows_per_day: u64 },
}

/// One deterministic row emitted by [`BifrostQualificationDataset`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationDatasetRow {
    /// Per-tenant zero-based row identity.
    pub row_id: i64,
    /// Event timestamp in UTC microseconds since Unix epoch.
    pub wyrd_event_time_micros: i64,
    /// Deterministic device identity.
    pub device_id: i64,
    /// Deterministic metric bucket.
    pub metric: String,
    /// Deterministic metric value.
    pub value: f64,
    /// Deterministic 256-character hexadecimal payload.
    pub payload: String,
}

/// Fraction-derived predicates for the five qualification queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationQueries {
    /// Row selected by Q1.
    pub q1_row_id: i64,
    /// Day selected by Q2.
    pub q2_day: u32,
    /// Exclusive device bound selected by Q2.
    pub q2_device_less_than: i64,
    /// Day selected by Q3.
    pub q3_day: u32,
    /// Start hour of Q3's twelve-hour window.
    pub q3_start_hour: u8,
    /// End hour of Q3's twelve-hour window.
    pub q3_end_hour: u8,
    /// First day selected by Q4 (inclusive).
    pub q4_start_day: u32,
    /// Last day selected by Q4 (exclusive).
    pub q4_end_day: u32,
    /// First row selected by Q5 (inclusive).
    pub q5_start_row_id: i64,
    /// Last row selected by Q5 (exclusive).
    pub q5_end_row_id: i64,
}

/// One aggregate used by Q3 and Q4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationAggregate {
    /// Number of rows in the group.
    pub count: u64,
    /// Sum of values in the group.
    pub sum: f64,
    /// Average value in the group.
    pub average: f64,
}

/// Expected deterministic point result for Q1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationPointResult {
    /// The one row returned by Q1.
    pub row: QualificationDatasetRow,
    /// Digest over the canonical result without this digest field.
    pub digest: String,
}

/// Expected deterministic count result for Q2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationCountResult {
    /// Number of rows returned by the predicate.
    pub count: u64,
    /// Digest over the canonical result without this digest field.
    pub digest: String,
}

/// Expected deterministic grouped result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationGroupedResult<K: Ord> {
    /// Groups in canonical key order.
    pub groups: BTreeMap<K, QualificationAggregate>,
    /// Digest over the canonical result without this digest field.
    pub digest: String,
}

/// Expected deterministic Q5 range result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationRangeResult {
    /// Number of rows returned by the range.
    pub row_count: u64,
    /// Digest over the canonical result without this digest field.
    pub digest: String,
}

/// Complete expected-result set for Q1-Q5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationExpectedResults {
    /// Q1 point result.
    pub q1: QualificationPointResult,
    /// Q2 selective count.
    pub q2: QualificationCountResult,
    /// Q3 metric groups.
    pub q3: QualificationGroupedResult<String>,
    /// Q4 device-bucket groups.
    pub q4: QualificationGroupedResult<u64>,
    /// Q5 range result.
    pub q5: QualificationRangeResult,
}

/// Per-day deterministic content facts retained in the checked-in manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationDayAggregate {
    /// Zero-based day partition.
    pub day: u32,
    /// Exact number of rows in this day.
    pub row_count: u64,
    /// Sum of deterministic values.
    pub value_sum: f64,
    /// Average deterministic value.
    pub value_average: f64,
    /// Minimum deterministic value.
    pub value_min: f64,
    /// Maximum deterministic value.
    pub value_max: f64,
}

/// Named column formula recorded in the dataset manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetColumnFormula {
    /// Column name.
    pub name: String,
    /// Human-readable deterministic formula.
    pub formula: String,
}

/// Canonical dataset manifest and its exact serialized bytes/digest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationDatasetManifest {
    /// Closed manifest schema identifier.
    pub schema_version: String,
    /// Dataset shape.
    pub shape: DatasetShape,
    /// Fixed splitmix seed used by every row formula.
    pub seed: u64,
    /// Total rows per tenant.
    pub row_count: u64,
    /// Column formulas by name.
    pub columns: Vec<DatasetColumnFormula>,
    /// Per-day deterministic aggregates.
    pub per_day: Vec<QualificationDayAggregate>,
    /// Fraction-derived query predicates.
    pub queries: QualificationQueries,
    /// Expected Q1-Q5 results.
    pub expected_results: QualificationExpectedResults,
    /// Canonical UTF-8 manifest bytes with one trailing newline.
    #[serde(skip)]
    pub canonical_json: Vec<u8>,
    /// SHA-256 digest of [`Self::canonical_json`].
    #[serde(skip)]
    pub digest: String,
}

/// Checked-in fixture containing smoke content facts and scale reference shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetManifestFixture {
    /// Closed dataset schema identifier.
    pub schema_version: String,
    /// Fully computed smoke manifest.
    pub smoke: QualificationDatasetManifest,
    /// Shape-only scale reference.
    pub scale: DatasetScaleReference,
}

/// Shape-only reference for the optional scale tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetScaleReference {
    /// Scale dataset shape.
    pub shape: DatasetShape,
    /// Fixed splitmix seed.
    pub seed: u64,
    /// Total rows represented by the shape.
    pub row_count: u64,
}

/// Deterministic Bifrost dataset owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostQualificationDataset {
    shape: DatasetShape,
    seed: u64,
}

impl BifrostQualificationDataset {
    /// Construct a dataset using the locked splitmix64(18) seed.
    ///
    /// # Errors
    /// Returns [`DatasetShapeError`] when the shape is invalid.
    pub fn new(shape: DatasetShape) -> Result<Self, DatasetShapeError> {
        DatasetShape::new(shape.days, shape.rows_per_day)?;
        Ok(Self {
            shape,
            seed: splitmix64(18),
        })
    }

    /// Return the shape used by this dataset.
    #[must_use]
    pub fn shape(&self) -> DatasetShape {
        self.shape
    }

    /// Generate one deterministic row.
    #[must_use]
    pub fn row(&self, row_id: i64) -> QualificationDatasetRow {
        let id = u64::try_from(row_id.max(0)).unwrap_or(0);
        let day = id / self.shape.rows_per_day;
        let ordinal = id % self.shape.rows_per_day;
        let spacing = MICROS_PER_DAY / self.shape.rows_per_day;
        let event_time = UNIX_MICROS_2026_01_01
            .saturating_add(i64::try_from(day.saturating_mul(MICROS_PER_DAY)).unwrap_or(i64::MAX))
            .saturating_add(i64::try_from(ordinal.saturating_mul(spacing)).unwrap_or(i64::MAX));
        let metric = format!("metric_{:03}", splitmix(self, 0x02, id) % 100);
        let mut payload = String::with_capacity(256);
        for index in 0..16 {
            payload.push_str(&format!(
                "{:016x}",
                splitmix(self, 0x04, id.saturating_mul(16).saturating_add(index))
            ));
        }
        QualificationDatasetRow {
            row_id,
            wyrd_event_time_micros: event_time,
            device_id: i64::try_from(splitmix(self, 0x01, id) % 10_000).unwrap_or(0),
            metric,
            value: (splitmix(self, 0x03, id) % 1_000_000) as f64 / 1000.0,
            payload,
        }
    }

    /// Derive the five query predicates using integer fractions of the shape.
    #[must_use]
    pub fn queries(&self) -> QualificationQueries {
        let rows = self.shape.row_count();
        let q5_start = rows.saturating_mul(2) / 10;
        let q5_len = 32_768_u64.max(rows / 128);
        QualificationQueries {
            q1_row_id: i64::try_from(rows.saturating_mul(17) / 32).unwrap_or(i64::MAX),
            q2_day: self.shape.days.saturating_mul(3) / 10,
            q2_device_less_than: 32,
            q3_day: (self.shape.days.saturating_mul(4) / 10).min(self.shape.days.saturating_sub(1)),
            q3_start_hour: 0,
            q3_end_hour: 12,
            q4_start_day: self.shape.days.saturating_sub(2),
            q4_end_day: self.shape.days,
            q5_start_row_id: i64::try_from(q5_start).unwrap_or(i64::MAX),
            q5_end_row_id: i64::try_from(q5_start.saturating_add(q5_len)).unwrap_or(i64::MAX),
        }
    }

    /// Compute expected results and stable SHA-256 digests for Q1-Q5.
    #[must_use]
    pub fn expected_results(&self) -> QualificationExpectedResults {
        let queries = self.queries();
        let q1_row = self.row(queries.q1_row_id);
        let q1 = QualificationPointResult {
            digest: digest_without_field(&q1_row),
            row: q1_row,
        };

        let day_start = u64::from(queries.q2_day).saturating_mul(self.shape.rows_per_day);
        let day_end = day_start.saturating_add(self.shape.rows_per_day);
        let q2_count = (day_start..day_end)
            .filter(|id| {
                self.row(i64::try_from(*id).unwrap_or(i64::MAX)).device_id
                    < queries.q2_device_less_than
            })
            .count() as u64;
        let q2_base = (q2_count,);
        let q2 = QualificationCountResult {
            count: q2_count,
            digest: digest_without_field(&q2_base),
        };

        let q3_start = u64::from(queries.q3_day).saturating_mul(self.shape.rows_per_day);
        let q3_end = q3_start.saturating_add(self.shape.rows_per_day / 2);
        let mut q3_groups = BTreeMap::<String, (u64, f64)>::new();
        for id in q3_start..q3_end {
            let row = self.row(i64::try_from(id).unwrap_or(i64::MAX));
            let entry = q3_groups.entry(row.metric).or_default();
            entry.0 = entry.0.saturating_add(1);
            entry.1 += row.value;
        }
        let q3_groups = q3_groups
            .into_iter()
            .map(|(key, (count, sum))| {
                (
                    key,
                    QualificationAggregate {
                        count,
                        sum,
                        average: sum / count as f64,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let q3 = QualificationGroupedResult {
            digest: digest_without_field(&q3_groups),
            groups: q3_groups,
        };

        let q4_start = u64::from(queries.q4_start_day).saturating_mul(self.shape.rows_per_day);
        let q4_end = u64::from(queries.q4_end_day).saturating_mul(self.shape.rows_per_day);
        let mut q4_groups = BTreeMap::<u64, (u64, f64)>::new();
        for id in q4_start..q4_end {
            let row = self.row(i64::try_from(id).unwrap_or(i64::MAX));
            let bucket = u64::try_from(row.device_id).unwrap_or(0) % 256;
            let entry = q4_groups.entry(bucket).or_default();
            entry.0 = entry.0.saturating_add(1);
            entry.1 += row.value;
        }
        let q4_groups = q4_groups
            .into_iter()
            .map(|(key, (count, sum))| {
                (
                    key,
                    QualificationAggregate {
                        count,
                        sum,
                        average: sum / count as f64,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let q4 = QualificationGroupedResult {
            digest: digest_without_field(&q4_groups),
            groups: q4_groups,
        };

        let q5_count = queries
            .q5_end_row_id
            .saturating_sub(queries.q5_start_row_id) as u64;
        let q5 = QualificationRangeResult {
            row_count: q5_count,
            digest: self.q5_digest(queries.q5_start_row_id, queries.q5_end_row_id),
        };
        QualificationExpectedResults { q1, q2, q3, q4, q5 }
    }

    /// Build the canonical manifest, including bytes and digest.
    #[must_use]
    pub fn manifest(&self) -> QualificationDatasetManifest {
        let expected_results = self.expected_results();
        let per_day = (0..self.shape.days)
            .map(|day| {
                let start = u64::from(day).saturating_mul(self.shape.rows_per_day);
                let end = start.saturating_add(self.shape.rows_per_day);
                let mut sum = 0.0;
                let mut min = f64::INFINITY;
                let mut max = f64::NEG_INFINITY;
                for id in start..end {
                    let value = self.row(i64::try_from(id).unwrap_or(i64::MAX)).value;
                    sum += value;
                    min = min.min(value);
                    max = max.max(value);
                }
                QualificationDayAggregate {
                    day,
                    row_count: self.shape.rows_per_day,
                    value_sum: sum,
                    value_average: sum / self.shape.rows_per_day as f64,
                    value_min: min,
                    value_max: max,
                }
            })
            .collect::<Vec<_>>();
        let manifest = QualificationDatasetManifest {
            schema_version: DATASET_SCHEMA_VERSION.to_owned(),
            shape: self.shape,
            seed: self.seed,
            row_count: self.shape.row_count(),
            columns: column_formulas(),
            per_day,
            queries: self.queries(),
            expected_results,
            canonical_json: Vec::new(),
            digest: String::new(),
        };
        let bytes = canonical_bytes(&manifest);
        let digest = sha256_hex(&bytes);
        QualificationDatasetManifest {
            canonical_json: bytes,
            digest,
            ..manifest
        }
    }

    /// Streams canonical all-column Q5 rows into the deterministic digest.
    ///
    /// The result binds the ordered query output rather than just its range
    /// metadata, while retaining a bounded per-row allocation instead of
    /// materializing the result set.
    #[must_use]
    fn q5_digest(&self, start_row_id: i64, end_row_id: i64) -> String {
        q5_digest_for_rows((start_row_id..end_row_id).map(|row_id| self.row(row_id)))
    }
}

/// Returns the locked column formulas included in each dataset manifest.
fn column_formulas() -> Vec<DatasetColumnFormula> {
    [
        ("row_id", "per-tenant sequence 0..N-1"),
        (
            "wyrd_event_time",
            "day_start(day) + ordinal * (86_400_000_000 / rows_per_day) micros UTC",
        ),
        ("device_id", "splitmix64(seed ^ 0x01 ^ row_id) % 10_000"),
        (
            "metric",
            "metric_ + zero_pad3(splitmix64(seed ^ 0x02 ^ row_id) % 100)",
        ),
        (
            "value",
            "splitmix64(seed ^ 0x03 ^ row_id) % 1_000_000 / 1000.0",
        ),
        (
            "payload",
            "16 successive splitmix64(seed ^ 0x04 ^ (row_id * 16 + i)) values as lowercase hex",
        ),
    ]
    .into_iter()
    .map(|(name, formula)| DatasetColumnFormula {
        name: name.to_owned(),
        formula: formula.to_owned(),
    })
    .collect()
}

/// Derives one splitmix value for a dataset row field.
fn splitmix(dataset: &BifrostQualificationDataset, key: u64, value: u64) -> u64 {
    splitmix64(dataset.seed ^ key ^ value)
}

/// Applies the locked SplitMix64 permutation to one unsigned input.
fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Hashes a serde value after recursively sorting object keys.
fn digest_without_field<T: Serialize>(value: &T) -> String {
    sha256_hex(&canonical_value_bytes(value))
}

/// Serializes a value as sorted JSON and appends its canonical trailing newline.
fn canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = canonical_value_bytes(value);
    bytes.push(b'\n');
    bytes
}

/// Serializes a value as sorted JSON without the manifest-only trailing newline.
fn canonical_value_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let value = serde_json::to_value(value).expect("deterministic contract serializes");
    serde_json::to_vec(&sort_json(value)).expect("sorted contract serializes")
}

/// Serializes exactly one Q5 row in the locked all-column order.
///
/// A newline separates rows before they enter the streaming digest, making the
/// ordered row sequence unambiguous without retaining it in memory.
fn canonical_q5_row_bytes(row: &QualificationDatasetRow) -> Vec<u8> {
    canonical_bytes(row)
}

/// Streams ordered Q5 result rows into a content-binding digest.
fn q5_digest_for_rows(rows: impl IntoIterator<Item = QualificationDatasetRow>) -> String {
    let mut hash = Sha256::new();
    for row in rows {
        hash.update(&canonical_q5_row_bytes(&row));
    }
    hash.finish()
}

/// Recursively sorts JSON object keys while preserving array order.
fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            for (key, value) in map {
                sorted.insert(key, sort_json(value));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sort_json).collect())
        }
        value => value,
    }
}

/// Computes a lowercase SHA-256 digest for a bounded byte slice.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finish()
}

/// Incremental SHA-256 state used by manifests and Q5 output digests.
struct Sha256 {
    /// Eight SHA-256 chaining words.
    state: [u32; 8],
    /// Partial input block retained between updates.
    buffer: [u8; 64],
    /// Number of valid bytes in `buffer`.
    buffered: usize,
    /// Total input bytes accepted before final padding.
    length: u64,
}

impl Sha256 {
    /// Initializes the SHA-256 initial vector for one independent digest.
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }

    /// Absorbs bytes without retaining already-compressed Q5 rows.
    fn update(&mut self, bytes: &[u8]) {
        self.length = self.length.saturating_add(bytes.len() as u64);
        let mut input = bytes;
        if self.buffered > 0 {
            let take = (64 - self.buffered).min(input.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&input[..take]);
            self.buffered += take;
            input = &input[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        while input.len() >= 64 {
            self.compress(&input[..64]);
            input = &input[64..];
        }
        self.buffer[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }

    /// Finalizes padding and returns the digest as lowercase hexadecimal.
    fn finish(mut self) -> String {
        let bit_length = self.length.saturating_mul(8);
        self.buffer[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.buffer[self.buffered..].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }
        self.buffer[self.buffered..56].fill(0);
        self.buffer[56..].copy_from_slice(&bit_length.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);
        let mut output = String::with_capacity(64);
        for word in self.state {
            output.push_str(&format!("{word:08x}"));
        }
        output
    }

    /// Compresses one complete SHA-256 input block into the chaining state.
    fn compress(&mut self, block: &[u8]) {
        /// SHA-256 round constants for the fixed compression permutation.
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let x = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let y = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(x)
                .wrapping_add(words[index - 7])
                .wrapping_add(y);
        }
        let mut state = self.state;
        for index in 0..64 {
            let ch = (state[4] & state[5]) ^ (!state[4] & state[6]);
            let sigma =
                state[4].rotate_right(6) ^ state[4].rotate_right(11) ^ state[4].rotate_right(25);
            let temp1 = state[7]
                .wrapping_add(sigma)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let maj = (state[0] & state[1]) ^ (state[0] & state[2]) ^ (state[1] & state[2]);
            let sigma =
                state[0].rotate_right(2) ^ state[0].rotate_right(13) ^ state[0].rotate_right(22);
            let temp2 = sigma.wrapping_add(maj);
            state = [
                temp1.wrapping_add(temp2),
                state[0],
                state[1],
                state[2],
                state[3].wrapping_add(temp1),
                state[4],
                state[5],
                state[6],
            ];
        }
        for (slot, value) in self.state.iter_mut().zip(state) {
            *slot = slot.wrapping_add(value);
        }
    }
}

/// Workload profile schema mirrored by `workload-profiles.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadProfiles {
    /// Closed workload schema identifier.
    pub schema_version: String,
    /// Ordered workload entries.
    pub workloads: Vec<WorkloadProfile>,
}

/// One fixture-defined workload profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadProfile {
    /// Stable workload identifier.
    pub workload_id: String,
    /// Load control and ladder.
    pub control: WorkloadControl,
    /// Full-tier stage timings.
    pub stages: StageDurations,
    /// Smoke-tier stage timings.
    pub smoke: SmokeDurations,
    /// Optional query family identifier.
    pub query_id: Option<String>,
    /// Optional ingest request shape.
    pub ingest_shape: Option<IngestShape>,
    /// Optional tenant matrix.
    pub tenant_matrix: Option<TenantMatrix>,
    /// Optional mixed workload fractions.
    pub mixed_fractions: Option<MixedFractions>,
    /// Forge generations created before a mixed measured window.
    pub forge_debt_generations_per_tenant: Option<u32>,
}

/// Control unit and offered ladder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadControl {
    /// Closed control unit.
    pub unit: String,
    /// Ordered offered values.
    pub ladder: Vec<u64>,
    /// Optional concurrent-operation cap.
    pub max_in_flight: Option<u32>,
}

/// Full-tier stage durations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageDurations {
    /// Warmup seconds.
    pub warmup_seconds: u32,
    /// Measurement seconds.
    pub measure_seconds: u32,
    /// Drain seconds.
    pub drain_seconds: u32,
}

/// Smoke-tier stage durations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeDurations {
    /// Warmup seconds.
    pub warmup_seconds: u32,
    /// Measurement seconds.
    pub measure_seconds: u32,
    /// Drain seconds.
    pub drain_seconds: u32,
}

/// Ingest rows and approximate request bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestShape {
    /// Rows per request.
    pub rows: u64,
    /// Target request bytes.
    pub bytes: u64,
}

/// Total-volume-constant tenant matrix definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantMatrix {
    /// Number of tenants.
    pub tenants: u32,
    /// Budget normalization strategy.
    pub normalization: String,
}

/// Diagonal mixed-workload fractions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixedFractions {
    /// Scribe fraction in percent.
    pub scribe_percent: u8,
    /// Oracle fraction in percent.
    pub oracle_percent: u8,
}

/// Pure unit tests for deterministic data generation and locked fixture profiles.
#[cfg(test)]
mod tests {
    use super::*;

    /// Prove invalid shape values are rejected before row generation.
    #[test]
    fn shape_validation_rejects_invalid_values() {
        assert!(DatasetShape::new(1, 160_000).is_err());
        assert!(DatasetShape::new(2, 0).is_err());
        assert!(DatasetShape::new(2, 7).is_err());
    }

    /// Prove rows and canonical manifests repeat exactly for smoke and scale shapes.
    #[test]
    fn dataset_generator_is_repeatable() {
        for shape in [(2, 160_000), (10, 3_200_000)] {
            let shape = DatasetShape::new(shape.0, shape.1).expect("locked shape");
            let first = BifrostQualificationDataset::new(shape)
                .expect("dataset")
                .manifest();
            let second = BifrostQualificationDataset::new(shape)
                .expect("dataset")
                .manifest();
            assert_eq!(first.canonical_json, second.canonical_json);
            assert_eq!(first.digest, second.digest);
            assert_eq!(first.expected_results, second.expected_results);
        }
    }

    /// Prove the locked smoke behavioral floor without a server or storage dependency.
    #[test]
    fn smoke_shape_satisfies_behavioral_floor() {
        let dataset =
            BifrostQualificationDataset::new(DatasetShape::new(2, 160_000).expect("shape"))
                .expect("dataset");
        assert_eq!(
            dataset.shape(),
            DatasetShape {
                days: 2,
                rows_per_day: 160_000
            }
        );
        let queries = dataset.queries();
        let expected = dataset.expected_results();
        assert_eq!(expected.q1.row.row_id, queries.q1_row_id);
        assert!(expected.q2.count > 0);
        assert_eq!(expected.q3.groups.len(), 100);
        assert_eq!(queries.q4_end_day - queries.q4_start_day, 2);
        assert_eq!(expected.q5.row_count, 32_768);
    }

    /// Prove each query predicate changes with the dataset shape rather than a fixed row count.
    #[test]
    fn queries_scale_with_shape() {
        let small = BifrostQualificationDataset::new(DatasetShape::new(2, 160_000).expect("shape"))
            .expect("dataset")
            .queries();
        let medium =
            BifrostQualificationDataset::new(DatasetShape::new(4, 200_000).expect("shape"))
                .expect("dataset")
                .queries();
        let large =
            BifrostQualificationDataset::new(DatasetShape::new(10, 3_200_000).expect("shape"))
                .expect("dataset")
                .queries();
        assert!(small.q1_row_id < medium.q1_row_id && medium.q1_row_id < large.q1_row_id);
        assert!(
            small.q5_end_row_id - small.q5_start_row_id
                <= medium.q5_end_row_id - medium.q5_start_row_id
        );
        assert!(medium.q4_start_day < large.q4_start_day);
    }

    /// Prove the checked-in dataset fixture round-trips through the Rust contract.
    #[test]
    fn dataset_fixture_agrees_with_generator() {
        let bytes = include_bytes!("../../fixtures/bifrost/qualification/dataset-manifest.json");
        let fixture: DatasetManifestFixture =
            serde_json::from_slice(bytes).expect("dataset fixture");
        assert_eq!(fixture.schema_version, DATASET_SCHEMA_VERSION);
        let generated = BifrostQualificationDataset::new(fixture.smoke.shape)
            .expect("smoke shape")
            .manifest();
        assert_eq!(
            fixture.smoke.shape,
            DatasetShape {
                days: 2,
                rows_per_day: 160_000
            }
        );
        assert_eq!(fixture.smoke.expected_results, generated.expected_results);
        assert_eq!(
            fixture.scale.shape,
            DatasetShape {
                days: 10,
                rows_per_day: 3_200_000
            }
        );
        assert_eq!(fixture.scale.row_count, 32_000_000);
    }

    /// Prove unknown dataset fixture fields fail closed through deny_unknown_fields.
    #[test]
    fn dataset_fixture_rejects_unknown_fields() {
        let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/dataset-manifest.json"
        ))
        .expect("fixture");
        value["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<DatasetManifestFixture>(value).is_err());
    }

    /// Prove the workload fixture contains the locked profile constants.
    #[test]
    fn workload_fixture_equals_packet_constants() {
        let profiles: WorkloadProfiles = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/workload-profiles.json"
        ))
        .expect("workload fixture");
        assert_eq!(profiles, expected_workload_profiles());
    }

    /// Builds the complete packet-owned workload profile fixture expectation.
    fn expected_workload_profiles() -> WorkloadProfiles {
        /// Returns the required full-tier timing for every workload family.
        fn stages() -> StageDurations {
            StageDurations {
                warmup_seconds: 10,
                measure_seconds: 30,
                drain_seconds: 15,
            }
        }

        /// Returns the required one-stage smoke timing for every workload family.
        fn smoke() -> SmokeDurations {
            SmokeDurations {
                warmup_seconds: 1,
                measure_seconds: 2,
                drain_seconds: 5,
            }
        }

        /// Creates a profile while fixing the common timing and absent fields.
        fn profile(id: &str, unit: &str, ladder: Vec<u64>) -> WorkloadProfile {
            WorkloadProfile {
                workload_id: id.to_owned(),
                control: WorkloadControl {
                    unit: unit.to_owned(),
                    ladder,
                    max_in_flight: None,
                },
                stages: stages(),
                smoke: smoke(),
                query_id: None,
                ingest_shape: None,
                tenant_matrix: None,
                mixed_fractions: None,
                forge_debt_generations_per_tenant: None,
            }
        }

        let rate_ladder = vec![100, 200, 500, 1_000, 2_000, 5_000, 10_000];
        let mut workloads = vec![
            WorkloadProfile {
                ingest_shape: Some(IngestShape {
                    rows: 10,
                    bytes: 16_384,
                }),
                ..profile("ingest-small", "requests_per_sec", rate_ladder.clone())
            },
            WorkloadProfile {
                ingest_shape: Some(IngestShape {
                    rows: 250,
                    bytes: 524_288,
                }),
                ..profile("ingest-typical", "requests_per_sec", rate_ladder.clone())
            },
            WorkloadProfile {
                ingest_shape: Some(IngestShape {
                    rows: 1_000,
                    bytes: 4_194_304,
                }),
                ..profile("ingest-large", "requests_per_sec", rate_ladder.clone())
            },
        ];
        for query_id in ["q1", "q2"] {
            workloads.push(WorkloadProfile {
                query_id: Some(query_id.to_owned()),
                control: WorkloadControl {
                    unit: "qps".to_owned(),
                    ladder: rate_ladder.clone(),
                    max_in_flight: Some(256),
                },
                ..profile(query_id, "qps", rate_ladder.clone())
            });
        }
        for (query_id, ladder, unit) in [
            ("q3", vec![1, 2, 4, 8, 16], "logical_bytes_per_sec"),
            ("q4", vec![1, 2, 4, 8], "logical_bytes_per_sec"),
            ("q5", vec![1, 2, 4, 8, 16], "returned_bytes_per_sec"),
        ] {
            workloads.push(WorkloadProfile {
                query_id: Some(query_id.to_owned()),
                ..profile(query_id, unit, ladder)
            });
        }
        for (id, query_id, unit) in [
            ("distributed-q1", "q1", "qps"),
            ("distributed-q4", "q4", "logical_bytes_per_sec"),
        ] {
            workloads.push(WorkloadProfile {
                query_id: Some(query_id.to_owned()),
                ..profile(id, unit, vec![1, 2, 3, 6])
            });
        }
        for fraction in [25, 50, 75] {
            workloads.push(WorkloadProfile {
                mixed_fractions: Some(MixedFractions {
                    scribe_percent: fraction,
                    oracle_percent: fraction,
                }),
                forge_debt_generations_per_tenant: Some(64),
                ..profile(
                    &format!("mixed-{fraction}-{fraction}"),
                    "requests_per_sec",
                    vec![u64::from(fraction)],
                )
            });
        }
        for tenants in [2, 10] {
            workloads.push(WorkloadProfile {
                tenant_matrix: Some(TenantMatrix {
                    tenants,
                    normalization: "budget_normalized_jain".to_owned(),
                }),
                ..profile(
                    &format!("tenants-{tenants}"),
                    "concurrency",
                    vec![1, 2, 4, 8, 16],
                )
            });
        }
        WorkloadProfiles {
            schema_version: "wyrd.bifrost.workload/v1".to_owned(),
            workloads,
        }
    }

    /// Proves a Q5 digest binds one selected row's all-column content.
    #[test]
    fn q5_digest_changes_when_row_content_changes() {
        let dataset = BifrostQualificationDataset::new(
            DatasetShape::new(2, 160_000).expect("locked smoke shape"),
        )
        .expect("dataset");
        let queries = dataset.queries();
        let row = dataset.row(queries.q5_start_row_id);
        let mut changed = row.clone();
        changed.payload.replace_range(0..1, "f");
        assert_ne!(
            canonical_q5_row_bytes(&row),
            canonical_q5_row_bytes(&changed)
        );
        let original = q5_digest_for_rows([row.clone()]);
        let mutated = q5_digest_for_rows([changed]);
        assert_ne!(original, mutated);
    }
}

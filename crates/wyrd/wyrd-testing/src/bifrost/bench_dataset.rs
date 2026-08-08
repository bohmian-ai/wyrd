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
///
/// The dataset binds a `day0_event_micros` anchor: the UTC-micros instant of
/// the first row in day zero. Every row's `wyrd_event_time_micros` is derived
/// relative to this anchor, so a caller (for example the run-relative
/// materializer under D87) can slide the whole event-time axis to a live
/// admission window while keeping all row-content facts — device, metric,
/// value, payload, and every query's row selection — anchor-invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostQualificationDataset {
    shape: DatasetShape,
    seed: u64,
    day0_event_micros: i64,
}

impl BifrostQualificationDataset {
    /// Construct a dataset anchored at the canonical `2026-01-01T00:00:00Z`.
    ///
    /// Delegates to [`Self::with_anchor`] with the locked canonical anchor so
    /// that every generated row, manifest byte, and digest is byte-identical to
    /// the checked-in fixture.
    ///
    /// # Errors
    /// Returns [`DatasetShapeError`] when the shape is invalid.
    pub fn new(shape: DatasetShape) -> Result<Self, DatasetShapeError> {
        Self::with_anchor(shape, UNIX_MICROS_2026_01_01)
    }

    /// Construct a dataset whose day-zero event time is `day0_event_micros`.
    ///
    /// Performs the same shape validation as [`Self::new`] and pins the locked
    /// splitmix64(18) seed; only the absolute event-time anchor differs. All
    /// non-event-time content (device, metric, value, payload) and every query
    /// row selection are independent of the anchor, so shifting it slides the
    /// event-time axis uniformly without changing any counted or grouped fact.
    ///
    /// # Errors
    /// Returns [`DatasetShapeError`] when the shape is invalid.
    pub(crate) fn with_anchor(
        shape: DatasetShape,
        day0_event_micros: i64,
    ) -> Result<Self, DatasetShapeError> {
        DatasetShape::new(shape.days, shape.rows_per_day)?;
        Ok(Self {
            shape,
            seed: splitmix64(18),
            day0_event_micros,
        })
    }

    /// Return the shape used by this dataset.
    #[must_use]
    pub fn shape(&self) -> DatasetShape {
        self.shape
    }

    /// Return the day-zero event-time anchor in UTC microseconds.
    ///
    /// This is the absolute instant of the first row in day zero; day `d`'s
    /// partition begins at `day0_event_micros + d * 86_400_000_000` micros.
    #[must_use]
    pub(crate) fn day0_event_micros(&self) -> i64 {
        self.day0_event_micros
    }

    /// Generate one deterministic row.
    #[must_use]
    pub fn row(&self, row_id: i64) -> QualificationDatasetRow {
        let id = u64::try_from(row_id.max(0)).unwrap_or(0);
        let day = id / self.shape.rows_per_day;
        let ordinal = id % self.shape.rows_per_day;
        let spacing = MICROS_PER_DAY / self.shape.rows_per_day;
        let event_time = self
            .day0_event_micros
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

/// Locked topology fixture (`wyrd.bifrost.topology/v1`).
///
/// Enumerates the exact D70 distributed ladder topologies the runners and
/// report `topology.topology_id` are validated against; the CLI rejects any
/// `--topology` id not present here before starting a cluster. All
/// `wyrd.bifrost.topology/v1` profiles are mixed pods (every pod runs server,
/// oracle, and Forge); dedicated-role topologies are out of v1 scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyProfiles {
    /// Closed topology schema identifier.
    pub schema_version: String,
    /// Ordered topology entries.
    pub topologies: Vec<TopologyProfile>,
}

/// One fixture-defined topology on the D70 distributed ladder.
///
/// Every v1 profile is a uniform mixed-pod cluster: each pod runs server,
/// oracle, and Forge together, so `pods` is the only degree of freedom and
/// runners map it straight onto `BifrostClusterSpec::mixed(pods)`. A
/// dedicated-role topology, if one ever ships, is a `wyrd.bifrost.topology/v2`
/// concern, not an added field here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyProfile {
    /// Stable topology identifier persisted in every report.
    pub topology_id: String,
    /// Number of mixed pods in this topology (server + oracle + Forge each).
    pub pods: u16,
}

/// Locked tier-budget fixture (`wyrd.bifrost.tier-budgets/v1`).
///
/// Carries the D69 wall-clock envelopes the runners enforce so that budgets are
/// fixture inputs read at runtime, never inferred in code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierBudgets {
    /// Closed tier-budget schema identifier.
    pub schema_version: String,
    /// Preflight lane budget in seconds (D69: 120 s, no database).
    pub preflight_seconds: u32,
    /// Per-lane smoke budget in seconds (D69: 180 s).
    pub smoke_lane_seconds: u32,
    /// Full smoke-suite budget in seconds (D69: 600 s).
    pub smoke_suite_seconds: u32,
}

/// Embedded workload fixture bytes shared by the loader and its round-trip test.
const WORKLOAD_PROFILES_JSON: &[u8] =
    include_bytes!("../../fixtures/bifrost/qualification/workload-profiles.json");
/// Embedded topology fixture bytes shared by the loader and its round-trip test.
const TOPOLOGY_PROFILES_JSON: &[u8] =
    include_bytes!("../../fixtures/bifrost/qualification/topology-profiles.json");
/// Embedded tier-budget fixture bytes shared by the loader and its round-trip test.
const TIER_BUDGETS_JSON: &[u8] =
    include_bytes!("../../fixtures/bifrost/qualification/tier-budgets.json");
/// Embedded dataset-manifest fixture bytes shared by the digest accessor and tests.
const DATASET_MANIFEST_JSON: &[u8] =
    include_bytes!("../../fixtures/bifrost/qualification/dataset-manifest.json");

/// Return the lowercase SHA-256 digest of the checked-in workload fixture.
///
/// The family runners stamp this into every report's `workload_digest` so the
/// exact ladder and control fixture a run consumed is recomputable from the
/// same embedded bytes the loader reads.
#[must_use]
pub(crate) fn workload_profiles_digest() -> String {
    sha256_hex(WORKLOAD_PROFILES_JSON)
}

/// Return the lowercase SHA-256 digest of the checked-in dataset manifest fixture.
///
/// The family runners stamp this into every report's `dataset_digest`. It
/// identifies the deterministic dataset contract (shape, seed, column formulas,
/// per-day aggregates, and expected Q1-Q5 results) a run measured against, and
/// is recomputable from the checked-in `dataset-manifest.json` bytes.
#[must_use]
pub(crate) fn dataset_manifest_digest() -> String {
    sha256_hex(DATASET_MANIFEST_JSON)
}

/// Return the locked smoke dataset shape from the checked-in manifest fixture.
///
/// The smoke family runners materialize exactly this shape fresh per invocation
/// (control-flow line 77), so the shape is a fixture input rather than a
/// compiled-in constant. Qualification-tier shape selection is deliberately not
/// provided here; it is T31's run-scope calibration.
///
/// # Panics
/// Panics only if the embedded `dataset-manifest.json` bytes fail to parse,
/// which the `dataset_fixture_agrees_with_generator` test proves cannot happen at
/// runtime; a parse failure is a fixture-corruption invariant, not an input.
#[must_use]
pub(crate) fn smoke_dataset_shape() -> DatasetShape {
    let fixture: DatasetManifestFixture = serde_json::from_slice(DATASET_MANIFEST_JSON)
        .expect("dataset-manifest.json fixture parses");
    fixture.smoke.shape
}

impl WorkloadProfiles {
    /// Load the checked-in workload fixture embedded at compile time.
    ///
    /// This is the single runtime source of every ladder, control unit, and
    /// stage timing the runners consume, so no ladder constant is compiled into
    /// runner code. The `workload_fixture_equals_packet_constants` test proves
    /// the embedded bytes round-trip through this contract, so a parse failure
    /// here is a fixture-corruption invariant, not a runtime input.
    ///
    /// # Panics
    /// Panics if the embedded fixture does not parse as [`WorkloadProfiles`],
    /// which the fixture round-trip test prevents from reaching a build.
    #[must_use]
    pub fn load() -> Self {
        serde_json::from_slice(WORKLOAD_PROFILES_JSON)
            .expect("embedded workload-profiles.json parses as WorkloadProfiles")
    }

    /// Resolve one workload profile by its stable id before any cluster starts.
    ///
    /// The runner and CLI call this to fail closed on an unknown `--workload`
    /// value ahead of every cluster and storage side effect.
    ///
    /// # Errors
    /// Returns [`BenchmarkSelectionError::UnknownWorkload`] when no fixture
    /// entry declares `workload_id`.
    pub fn resolve(&self, workload_id: &str) -> Result<&WorkloadProfile, BenchmarkSelectionError> {
        self.workloads
            .iter()
            .find(|profile| profile.workload_id == workload_id)
            .ok_or_else(|| BenchmarkSelectionError::UnknownWorkload(workload_id.to_owned()))
    }
}

impl TopologyProfiles {
    /// Load the checked-in topology fixture embedded at compile time.
    ///
    /// This is the single runtime source of the locked distributed ladder, so
    /// pod and server counts are never compiled into runner code. The
    /// `topology_fixture_equals_packet_constants` test proves the embedded bytes
    /// round-trip through this contract.
    ///
    /// # Panics
    /// Panics if the embedded fixture does not parse as [`TopologyProfiles`],
    /// which the fixture round-trip test prevents from reaching a build.
    #[must_use]
    pub fn load() -> Self {
        serde_json::from_slice(TOPOLOGY_PROFILES_JSON)
            .expect("embedded topology-profiles.json parses as TopologyProfiles")
    }

    /// Resolve one topology profile by its stable id before any cluster starts.
    ///
    /// The runner and CLI call this to fail closed on an unknown `--topology`
    /// value ahead of every cluster and storage side effect.
    ///
    /// # Errors
    /// Returns [`BenchmarkSelectionError::UnknownTopology`] when no fixture
    /// entry declares `topology_id`.
    pub fn resolve(&self, topology_id: &str) -> Result<&TopologyProfile, BenchmarkSelectionError> {
        self.topologies
            .iter()
            .find(|profile| profile.topology_id == topology_id)
            .ok_or_else(|| BenchmarkSelectionError::UnknownTopology(topology_id.to_owned()))
    }
}

impl TierBudgets {
    /// Load the checked-in tier-budget fixture embedded at compile time.
    ///
    /// This is the single runtime source of the D69 wall-clock envelopes the
    /// runners enforce, so no budget is inferred in code. The
    /// `tier_budget_fixture_equals_packet_constants` test proves the embedded
    /// bytes round-trip through this contract.
    ///
    /// # Panics
    /// Panics if the embedded fixture does not parse as [`TierBudgets`], which
    /// the fixture round-trip test prevents from reaching a build.
    #[must_use]
    pub fn load() -> Self {
        serde_json::from_slice(TIER_BUDGETS_JSON)
            .expect("embedded tier-budgets.json parses as TierBudgets")
    }
}

/// Failure raised when a `--workload` or `--topology` id is not in a locked fixture.
///
/// Runners surface this before starting a cluster so an unknown selection never
/// consumes cluster, storage, or database resources.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BenchmarkSelectionError {
    /// The requested workload id is absent from `workload-profiles.json`.
    #[error("unknown Bifrost workload id: {0}")]
    UnknownWorkload(String),
    /// The requested topology id is absent from `topology-profiles.json`.
    #[error("unknown Bifrost topology id: {0}")]
    UnknownTopology(String),
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

    /// Prove the topology fixture enumerates exactly the locked D70 ladder.
    #[test]
    fn topology_fixture_equals_packet_constants() {
        let profiles: TopologyProfiles = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/topology-profiles.json"
        ))
        .expect("topology fixture");
        assert_eq!(
            profiles,
            TopologyProfiles {
                schema_version: "wyrd.bifrost.topology/v1".to_owned(),
                topologies: vec![
                    TopologyProfile {
                        topology_id: "one-pod".to_owned(),
                        pods: 1,
                    },
                    TopologyProfile {
                        topology_id: "two-pod".to_owned(),
                        pods: 2,
                    },
                    TopologyProfile {
                        topology_id: "three-pod".to_owned(),
                        pods: 3,
                    },
                    TopologyProfile {
                        topology_id: "six-pod".to_owned(),
                        pods: 6,
                    },
                ],
            }
        );
    }

    /// Prove the topology fixture fails closed on unknown fields.
    #[test]
    fn topology_fixture_rejects_unknown_fields() {
        let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/topology-profiles.json"
        ))
        .expect("fixture");
        value["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<TopologyProfiles>(value).is_err());
    }

    /// Prove the tier-budget fixture carries the locked D69 envelopes.
    #[test]
    fn tier_budget_fixture_equals_packet_constants() {
        let budgets: TierBudgets = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/tier-budgets.json"
        ))
        .expect("tier budget fixture");
        assert_eq!(
            budgets,
            TierBudgets {
                schema_version: "wyrd.bifrost.tier-budgets/v1".to_owned(),
                preflight_seconds: 120,
                smoke_lane_seconds: 180,
                smoke_suite_seconds: 600,
            }
        );
    }

    /// Prove the tier-budget fixture fails closed on unknown fields.
    #[test]
    fn tier_budget_fixture_rejects_unknown_fields() {
        let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../fixtures/bifrost/qualification/tier-budgets.json"
        ))
        .expect("fixture");
        value["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<TierBudgets>(value).is_err());
    }

    /// Prove the workload loader resolves every locked id and rejects unknowns.
    #[test]
    fn workload_profiles_resolve_known_and_reject_unknown() {
        let profiles = WorkloadProfiles::load();
        assert_eq!(
            profiles
                .resolve("ingest-small")
                .expect("locked workload")
                .workload_id,
            "ingest-small"
        );
        assert_eq!(
            profiles
                .resolve("distributed-q1")
                .expect("locked workload")
                .query_id
                .as_deref(),
            Some("q1")
        );
        assert_eq!(
            profiles.resolve("does-not-exist"),
            Err(BenchmarkSelectionError::UnknownWorkload(
                "does-not-exist".to_owned()
            ))
        );
    }

    /// Prove the topology loader resolves every locked id and rejects unknowns.
    #[test]
    fn topology_profiles_resolve_known_and_reject_unknown() {
        let profiles = TopologyProfiles::load();
        assert_eq!(
            profiles.resolve("two-pod").expect("locked topology").pods,
            2
        );
        assert_eq!(
            profiles.resolve("four-pod"),
            Err(BenchmarkSelectionError::UnknownTopology(
                "four-pod".to_owned()
            ))
        );
    }

    /// Prove the tier-budget loader returns the locked D69 envelope.
    #[test]
    fn tier_budgets_load_matches_locked_envelope() {
        let budgets = TierBudgets::load();
        assert_eq!(budgets.preflight_seconds, 120);
        assert_eq!(budgets.smoke_lane_seconds, 180);
        assert_eq!(budgets.smoke_suite_seconds, 600);
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

    /// Prove the canonical anchor default is byte-identical to `with_anchor`.
    ///
    /// `new(shape)` must equal `with_anchor(shape, UNIX_MICROS_2026_01_01)` for
    /// every manifest byte and digest, guaranteeing the checked-in fixture and
    /// `dataset_generator_is_repeatable` stay pinned at the canonical anchor.
    #[test]
    fn new_equals_with_anchor_at_canonical_anchor() {
        for shape in [(2, 160_000), (10, 3_200_000)] {
            let shape = DatasetShape::new(shape.0, shape.1).expect("locked shape");
            let default = BifrostQualificationDataset::new(shape)
                .expect("dataset")
                .manifest();
            let explicit = BifrostQualificationDataset::with_anchor(shape, UNIX_MICROS_2026_01_01)
                .expect("dataset")
                .manifest();
            assert_eq!(default.canonical_json, explicit.canonical_json);
            assert_eq!(default.digest, explicit.digest);
            assert_eq!(default.expected_results, explicit.expected_results);
        }
    }

    /// Prove a shifted anchor slides event times uniformly and selects the same rows.
    ///
    /// Every row's `wyrd_event_time_micros` shifts by exactly the anchor delta,
    /// while all content facts and query row selections — Q2 count, Q3/Q4 group
    /// aggregates, Q5 range length, Q1 row identity — remain anchor-invariant.
    #[test]
    fn shifted_anchor_shifts_event_times_but_not_selections() {
        let shape = DatasetShape::new(2, 160_000).expect("locked smoke shape");
        let delta: i64 = 7 * MICROS_PER_DAY as i64;
        let canonical = BifrostQualificationDataset::new(shape).expect("dataset");
        let shifted =
            BifrostQualificationDataset::with_anchor(shape, UNIX_MICROS_2026_01_01 + delta)
                .expect("dataset");

        for row_id in [0_i64, 1, 159_999, 160_000, 319_999] {
            let base = canonical.row(row_id);
            let moved = shifted.row(row_id);
            assert_eq!(
                moved.wyrd_event_time_micros - base.wyrd_event_time_micros,
                delta
            );
            assert_eq!(moved.device_id, base.device_id);
            assert_eq!(moved.metric, base.metric);
            assert_eq!(moved.value, base.value);
            assert_eq!(moved.payload, base.payload);
        }

        let base = canonical.expected_results();
        let moved = shifted.expected_results();
        assert_eq!(moved.q1.row.row_id, base.q1.row.row_id);
        assert_eq!(moved.q2.count, base.q2.count);
        assert_eq!(moved.q3.groups, base.q3.groups);
        assert_eq!(moved.q4.groups, base.q4.groups);
        assert_eq!(moved.q5.row_count, base.q5.row_count);
        assert_eq!(canonical.queries(), shifted.queries());
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

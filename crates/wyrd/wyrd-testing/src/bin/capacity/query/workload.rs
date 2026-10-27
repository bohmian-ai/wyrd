//! The `bifrost.datasets.events` fixture, the named benchmark cases, and their
//! exact answers.
//!
//! Everything the benchmark writes and asks lives here, in plain functions of
//! one row index, so a reader can check any record against its data,
//! statement, and expected answer without following a workload DSL.

use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use num_traits::ToPrimitive;

/// Fully qualified table every read case queries.
///
/// `bifrost.datasets` because it is the one namespace whose tables a public
/// client may register.
pub const TABLE: &str = "bifrost.datasets.events";

/// Second table that takes the concurrent writes, so read answers never move.
pub const INGEST_TABLE: &str = "bifrost.datasets.events_ingest";

/// Rows in the standard fixture.
pub const STANDARD_ROWS: i64 = 10_000_000;

/// Rows in the separate heavy-scan fixture.
pub const HEAVY_ROWS: i64 = 100_000_000;

/// Rows per public ingest request: bounded well below the wire limit.
pub const REQUEST_ROWS: i64 = 16_384;

/// Distinct `service_id` values.
pub const SERVICES: i64 = 32;

/// Distinct `tenant_id` values.
pub const TENANTS: i64 = 16;

/// `event_time` of row zero: 2026-01-01T00:00:00Z in epoch milliseconds.
pub const DAY_START_MS: i64 = 1_767_225_600_000;

/// One hour in milliseconds.
pub const HOUR_MS: i64 = 3_600_000;

/// Hours the fixture's rows span evenly.
pub const DAY_HOURS: i64 = 24;

/// Rows each small aggregate groups.
pub const SMALL_ROWS: i64 = 100_000;

/// Rows each medium aggregate groups.
pub const MILLION_ROWS: i64 = 1_000_000;

/// Expected or returned rows, every column cast to `i64`.
pub type Rows = Vec<Vec<i64>>;

/// Deterministic 64-bit mix (SplitMix64 finalizer) of one row index.
#[must_use]
pub const fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Returns the non-null, all-`Int64`-but-payload user schema.
#[must_use]
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Int64, false),
        Field::new("tenant_id", DataType::Int64, false),
        Field::new("event_time", DataType::Int64, false),
        Field::new("service_id", DataType::Int64, false),
        Field::new("duration_ms", DataType::Int64, false),
        Field::new("payload", DataType::Utf8, false),
    ]))
}

/// One fixture size and every per-row value derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixture {
    /// Rows seeded into [`TABLE`].
    pub rows: i64,
}

impl Fixture {
    /// `event_time` of row `id`: the rows spread evenly over one day.
    #[must_use]
    pub const fn event_time(self, id: i64) -> i64 {
        DAY_START_MS + id * (DAY_HOURS * HOUR_MS) / self.rows
    }

    /// Builds rows `first..last` in ascending `event_id` order.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the columns do not match [`schema`], which
    /// is a fixture defect rather than a runtime condition.
    pub fn batch(self, first: i64, last: i64) -> Result<RecordBatch, arrow::error::ArrowError> {
        let column = |value: fn(Self, i64) -> i64| -> Int64Array {
            (first..last).map(|id| value(self, id)).collect()
        };
        let mut payloads = StringBuilder::with_capacity(
            usize::try_from(last - first).unwrap_or(0),
            usize::try_from((last - first) * 40).unwrap_or(0),
        );
        let mut buffer = [0_u8; 64];
        for id in first..last {
            payloads.append_value(payload(id, &mut buffer));
        }
        RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(column(|_, id| id)),
                Arc::new(column(|_, id| tenant(id))),
                Arc::new(column(Self::event_time)),
                Arc::new(column(|_, id| service(id))),
                Arc::new(column(|_, id| duration(id))),
                Arc::new(payloads.finish()),
            ],
        )
    }
}

/// `tenant_id` of row `id`: runs of 1,000 rows per tenant.
#[must_use]
pub const fn tenant(id: i64) -> i64 {
    (id / 1_000) % TENANTS
}

/// `service_id` of row `id`.
#[must_use]
pub const fn service(id: i64) -> i64 {
    id % SERVICES
}

/// `duration_ms` of row `id`: pseudo-random in `0..5,000`.
#[must_use]
pub const fn duration(id: i64) -> i64 {
    ((mix(id.cast_unsigned()) >> 32) % 5_000).cast_signed()
}

/// Byte length of row `id`'s payload: pseudo-random in `16..=64`.
#[must_use]
pub const fn payload_len(id: i64) -> i64 {
    16 + (mix(id.cast_unsigned()) % 49).cast_signed()
}

/// Writes row `id`'s varying lowercase-hex payload into `buffer`.
///
/// Four chained mixes fill the 64 hex characters, so payloads neither repeat
/// within a row nor share a dictionary across rows.
fn payload(id: i64, buffer: &mut [u8; 64]) -> &str {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut state = mix(!id.cast_unsigned());
    for chunk in buffer.chunks_mut(16) {
        for (shift, byte) in chunk.iter_mut().enumerate() {
            *byte = HEX[((state >> (shift * 4)) & 15) as usize];
        }
        state = mix(state);
    }
    let len = usize::try_from(payload_len(id)).unwrap_or(64);
    std::str::from_utf8(&buffer[..len]).unwrap_or_default()
}

/// Every named read case; SQL, rows examined, and answer sit together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Case {
    /// Q1: one event by ID, rotated across the table.
    Selective,
    /// Q2: `GROUP BY service_id` over about 100,000 events.
    SmallAggregate,
    /// Q3a: the same `GROUP BY` over 1,000,000 events.
    MillionAggregate,
    /// Q3b: the same `GROUP BY` over every event.
    TableAggregate,
    /// Q4: aggregate every event, reading the varying payload.
    FullScan,
}

impl Case {
    /// The benchmark case label its requests carry, so a benchmark-mode
    /// server keeps each class's timings apart.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Selective => "point-lookup",
            Self::SmallAggregate => "small-aggregate",
            Self::MillionAggregate => "million-aggregate",
            Self::TableAggregate => "table-aggregate",
            Self::FullScan => "full-scan",
        }
    }

    /// Distinct statements the case rotates through by `sequence % variants`.
    ///
    /// The selective case derives each ID from the sequence instead, so its
    /// answers are computed per query rather than held.
    #[must_use]
    pub const fn variants(self) -> u64 {
        match self {
            Self::SmallAggregate => 64,
            Self::MillionAggregate => 8,
            Self::Selective | Self::TableAggregate | Self::FullScan => 1,
        }
    }

    /// The statement issued for `sequence`, exactly as sent.
    #[must_use]
    pub fn sql(self, fixture: Fixture, sequence: u64) -> String {
        match self {
            Self::Selective => format!(
                "SELECT event_id, service_id, duration_ms FROM {TABLE} WHERE event_id = {}",
                selective_id(fixture, sequence)
            ),
            Self::SmallAggregate | Self::MillionAggregate | Self::TableAggregate => {
                let (first, last) = self.range(fixture, sequence);
                format!(
                    "SELECT service_id, COUNT(*) AS events, SUM(duration_ms) AS total_ms \
                     FROM {TABLE} WHERE event_id >= {first} AND event_id < {last} \
                     GROUP BY service_id ORDER BY service_id"
                )
            }
            Self::FullScan => format!(
                "SELECT COUNT(*) AS events, SUM(duration_ms) AS total_ms, \
                 SUM(LENGTH(payload)) AS payload_bytes FROM {TABLE}"
            ),
        }
    }

    /// The exact answer for `sequence`, computed from the row functions.
    ///
    /// Aggregate answers loop over every examined row, so callers compute each
    /// of [`Self::variants`] once before measuring.
    #[must_use]
    pub fn expected(self, fixture: Fixture, sequence: u64) -> Rows {
        match self {
            Self::Selective => {
                let id = selective_id(fixture, sequence);
                vec![vec![id, service(id), duration(id)]]
            }
            Self::SmallAggregate | Self::MillionAggregate | Self::TableAggregate => {
                let (first, last) = self.range(fixture, sequence);
                let mut groups = vec![[0_i64; 2]; SERVICES.to_usize().unwrap_or(0)];
                for id in first..last {
                    let group = &mut groups[service(id).to_usize().unwrap_or(0)];
                    group[0] += 1;
                    group[1] += duration(id);
                }
                groups
                    .iter()
                    .zip(0..)
                    .filter(|(group, _)| group[0] > 0)
                    .map(|(group, service)| vec![service, group[0], group[1]])
                    .collect()
            }
            Self::FullScan => {
                let (mut total, mut bytes) = (0, 0);
                for id in 0..fixture.rows {
                    total += duration(id);
                    bytes += payload_len(id);
                }
                vec![vec![fixture.rows, total, bytes]]
            }
        }
    }

    /// The `event_id` range an aggregate groups for `sequence`: its
    /// declared rows, or one variant's share of a smaller (smoke) fixture.
    fn range(self, fixture: Fixture, sequence: u64) -> (i64, i64) {
        let (rows, variants) = match self {
            Self::SmallAggregate => (SMALL_ROWS.min(fixture.rows / 64), 64),
            Self::MillionAggregate => (MILLION_ROWS.min(fixture.rows / 8), 8),
            _ => return (0, fixture.rows),
        };
        let first = (sequence % variants).cast_signed() * (fixture.rows / variants.cast_signed());
        (first, first + rows)
    }
}

/// The event the selective case reads for `sequence`: a multiplicative hash
/// spreading consecutive sequences across the whole table.
#[must_use]
pub const fn selective_id(fixture: Fixture, sequence: u64) -> i64 {
    (sequence.wrapping_mul(2_654_435_761) % fixture.rows.cast_unsigned()).cast_signed()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture rows, statements, and answers are exactly what the report
    /// states, on a size small enough to check by hand.
    ///
    /// # Panics
    ///
    /// Panics when a row value, a statement, or an answer drifts.
    #[test]
    fn events_fixture_answers_are_exact() {
        let fixture = Fixture {
            rows: 64 * 1_000_000,
        };
        let batch = Fixture {
            rows: STANDARD_ROWS,
        }
        .batch(0, 3)
        .expect("fixture rows");
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.schema(), schema());
        assert_eq!(
            Fixture {
                rows: STANDARD_ROWS
            }
            .event_time(STANDARD_ROWS / 2),
            DAY_START_MS + 12 * HOUR_MS
        );
        assert!((16..=64).contains(&payload_len(7)));
        assert_eq!(
            payload(7, &mut [0; 64]).len().to_i64(),
            Some(payload_len(7))
        );
        assert_ne!(payload(7, &mut [0; 64]), payload(8, &mut [0; 64]));
        assert_eq!(
            Case::SmallAggregate.sql(fixture, 65),
            "SELECT service_id, COUNT(*) AS events, SUM(duration_ms) AS total_ms FROM \
             bifrost.datasets.events WHERE event_id >= 1000000 AND event_id < 1100000 \
             GROUP BY service_id ORDER BY service_id"
        );
        let small = Case::SmallAggregate.expected(fixture, 1);
        assert_eq!(small.len(), 32);
        assert_eq!(small.iter().map(|row| row[1]).sum::<i64>(), SMALL_ROWS);
        let id = selective_id(fixture, 3);
        assert_eq!(
            Case::Selective.expected(fixture, 3),
            vec![vec![id, service(id), duration(id)]]
        );
    }
}

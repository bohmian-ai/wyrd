//! Deterministic workload descriptions shared by Bifrost journeys and benches.

use serde::{Deserialize, Serialize};

/// The traffic shape applied to a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficShape {
    /// Constant rate over the workload window.
    Steady,
    /// Alternating quiet and burst windows.
    Bursty,
    /// Small batches with a latency-sensitive query mix.
    LatencySensitive,
}

/// Width of the user schema used by a workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaWidth {
    /// Only system columns and one value column.
    Narrow,
    /// System columns plus payload, dimensions, and trace metadata.
    Wide,
}

/// A named fault boundary that the real harness may inject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FaultPoint {
    /// Fail after a named UTC day slice has fsynced.
    DaySlice { day: String },
    /// Fail on a named age-rotation timer tick.
    TimerTick { tick: u64 },
    /// Fail at a live-tail continuation boundary for one whole LSN.
    ContinuationBoundary { lsn: u64 },
}

/// Required edge cases for the Bifrost workload library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadCase {
    CrossDayPartialFsyncRetry,
    Exact32MibIngest,
    Oversized32MibIngest,
    IdleLowVolumeSealKey,
    MaximumWholeLsnLiveTail,
}

/// One reproducible tenant workload declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadSpec {
    /// Stable ID used to compare reports across stages.
    pub id: String,
    /// Seed for all generated row IDs and scheduling choices.
    pub seed: u64,
    /// Traffic shape for this tenant.
    pub traffic: TrafficShape,
    /// User schema width.
    pub schema: SchemaWidth,
    /// Rows submitted per batch.
    pub batch_size: u64,
    /// Target rows per second.
    pub rows_per_second: u64,
    /// Workload cases exercised by the run.
    pub cases: Vec<WorkloadCase>,
    /// Named fault boundaries used by the run.
    pub fault_points: Vec<FaultPoint>,
}

impl WorkloadSpec {
    /// Construct a deterministic workload with the required edge-case set.
    #[must_use]
    pub fn required(
        id: impl Into<String>,
        seed: u64,
        traffic: TrafficShape,
        schema: SchemaWidth,
    ) -> Self {
        Self {
            id: id.into(),
            seed,
            traffic,
            schema,
            batch_size: 1_000,
            rows_per_second: 100,
            cases: vec![
                WorkloadCase::CrossDayPartialFsyncRetry,
                WorkloadCase::Exact32MibIngest,
                WorkloadCase::Oversized32MibIngest,
                WorkloadCase::IdleLowVolumeSealKey,
                WorkloadCase::MaximumWholeLsnLiveTail,
            ],
            fault_points: vec![
                FaultPoint::DaySlice {
                    day: "workload-day-0".to_owned(),
                },
                FaultPoint::TimerTick { tick: 600 },
                FaultPoint::ContinuationBoundary { lsn: 1 },
            ],
        }
    }

    /// Validate the workload contract before a real run starts.
    pub fn validate(&self) -> Result<(), WorkloadError> {
        if self.id.trim().is_empty() {
            return Err(WorkloadError::EmptyId);
        }
        if self.batch_size == 0 || self.rows_per_second == 0 {
            return Err(WorkloadError::ZeroRate);
        }
        for required in [
            WorkloadCase::CrossDayPartialFsyncRetry,
            WorkloadCase::Exact32MibIngest,
            WorkloadCase::Oversized32MibIngest,
            WorkloadCase::IdleLowVolumeSealKey,
            WorkloadCase::MaximumWholeLsnLiveTail,
        ] {
            if !self.cases.contains(&required) {
                return Err(WorkloadError::MissingCase(required));
            }
        }
        Ok(())
    }
}

/// Workload values that violate the benchmark contract.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkloadError {
    /// A workload ID is blank.
    #[error("workload ID must not be empty")]
    EmptyId,
    /// A workload cannot have zero batch size or rate.
    #[error("workload batch size and rate must be non-zero")]
    ZeroRate,
    /// A required edge case was omitted.
    #[error("workload is missing required case {0:?}")]
    MissingCase(WorkloadCase),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_workload_contains_all_edge_cases() {
        let workload = WorkloadSpec::required(
            "steady-narrow",
            42,
            TrafficShape::Steady,
            SchemaWidth::Narrow,
        );
        workload.validate().expect("required workload is valid");
        assert_eq!(workload.cases.len(), 5);
        assert_eq!(workload.fault_points.len(), 3);
    }

    #[test]
    fn missing_edge_case_is_rejected() {
        let mut workload =
            WorkloadSpec::required("invalid", 42, TrafficShape::Steady, SchemaWidth::Narrow);
        workload.cases.pop();
        assert!(matches!(
            workload.validate(),
            Err(WorkloadError::MissingCase(_))
        ));
    }
}

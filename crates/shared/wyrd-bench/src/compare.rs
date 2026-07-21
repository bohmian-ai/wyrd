//! Normalized pre/post benchmark artifact comparison.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::report::BenchmarkReport;

/// A stage artifact manifest and its reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkStage {
    /// Stage directory name.
    pub stage: String,
    /// Git SHA that produced the stage.
    pub git_sha: String,
    /// Dirty-worktree marker.
    pub dirty_worktree: bool,
    /// Configuration values resolved for the run.
    pub configuration: BTreeMap<String, String>,
    /// Reports keyed by stable workload ID.
    pub reports: Vec<BenchmarkReport>,
}

/// One normalized comparison result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Regression {
    /// Workload and lane identity.
    pub workload_id: String,
    /// Compared metric.
    pub metric: String,
    /// Pre-change value.
    pub before: u64,
    /// Post-change value.
    pub after: u64,
    /// Relative change, where positive means larger.
    pub relative_change: f64,
}

/// Comparison output with no machine-equivalence assumptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Source stage.
    pub before_stage: String,
    /// Compared stage.
    pub after_stage: String,
    /// Regressions above the caller's tolerance.
    pub regressions: Vec<Regression>,
}

/// Errors that make a comparison invalid rather than merely regressed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ComparisonError {
    /// Workload, schema, seed, batch, or topology differs.
    #[error("benchmark configuration drift: {0}")]
    ConfigurationDrift(String),
    /// A workload is missing from one stage.
    #[error("benchmark workload {0} is missing from one stage")]
    MissingWorkload(String),
}

/// Compare identical workloads and configuration, returning normalized SLO data.
pub fn compare_stages(
    before: &BenchmarkStage,
    after: &BenchmarkStage,
    tolerance: f64,
) -> Result<ComparisonReport, ComparisonError> {
    if before.configuration != after.configuration {
        return Err(ComparisonError::ConfigurationDrift(
            "resolved benchmark configuration differs".to_owned(),
        ));
    }
    let mut regressions = Vec::new();
    for before_report in &before.reports {
        let after_report = after
            .reports
            .iter()
            .find(|report| report.workload.id == before_report.workload.id)
            .ok_or_else(|| ComparisonError::MissingWorkload(before_report.workload.id.clone()))?;
        if before_report.workload != after_report.workload
            || before_report.batch_size != after_report.batch_size
            || before_report.pods != after_report.pods
            || before_report.machine.operating_system != after_report.machine.operating_system
            || before_report.machine.cpu_count != after_report.machine.cpu_count
            || before_report.machine.memory_bytes != after_report.machine.memory_bytes
        {
            return Err(ComparisonError::ConfigurationDrift(format!(
                "workload {} or normalized machine identity differs",
                before_report.workload.id
            )));
        }
        compare_metric(
            &mut regressions,
            &before_report.workload.id,
            "write_p99_us",
            before_report.write_latency.p99_us,
            after_report.write_latency.p99_us,
            tolerance,
        );
        compare_metric(
            &mut regressions,
            &before_report.workload.id,
            "query_p99_us",
            before_report.query_latency.p99_us,
            after_report.query_latency.p99_us,
            tolerance,
        );
    }
    for after_report in &after.reports {
        if !before
            .reports
            .iter()
            .any(|report| report.workload.id == after_report.workload.id)
        {
            return Err(ComparisonError::MissingWorkload(
                after_report.workload.id.clone(),
            ));
        }
    }
    Ok(ComparisonReport {
        before_stage: before.stage.clone(),
        after_stage: after.stage.clone(),
        regressions,
    })
}

fn compare_metric(
    regressions: &mut Vec<Regression>,
    workload_id: &str,
    metric: &str,
    before: u64,
    after: u64,
    tolerance: f64,
) {
    if before == 0 {
        return;
    }
    let before_float = decimal_f64(before);
    let after_float = decimal_f64(after);
    let relative_change = (after_float - before_float) / before_float;
    if relative_change > tolerance {
        regressions.push(Regression {
            workload_id: workload_id.to_owned(),
            metric: metric.to_owned(),
            before,
            after,
            relative_change,
        });
    }
}

fn decimal_f64(value: u64) -> f64 {
    value
        .to_string()
        .parse::<f64>()
        .expect("every u64 has a finite decimal f64 representation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        LatencyPercentiles, MachineMetadata, PodMetadata, QueryMeasurements, StorageMeasurements,
    };
    use crate::workload::{SchemaWidth, TrafficShape, WorkloadSpec};

    fn report(p99: u64) -> BenchmarkReport {
        BenchmarkReport {
            report_version: BenchmarkReport::VERSION.to_owned(),
            lane: "test".to_owned(),
            workload: WorkloadSpec::required("same", 1, TrafficShape::Steady, SchemaWidth::Narrow),
            batch_size: 100,
            pods: PodMetadata {
                pod_count: 1,
                pod_ids: vec!["pod-0".to_owned()],
            },
            machine: MachineMetadata::default(),
            write_latency: LatencyPercentiles {
                p99_us: p99,
                ..Default::default()
            },
            query_latency: LatencyPercentiles::default(),
            storage: StorageMeasurements::default(),
            query: QueryMeasurements::default(),
        }
    }

    #[test]
    fn comparison_rejects_configuration_drift() {
        let before = BenchmarkStage {
            stage: "pre-change".to_owned(),
            git_sha: "a".to_owned(),
            dirty_worktree: false,
            configuration: BTreeMap::from([(String::from("seed"), String::from("1"))]),
            reports: vec![report(100)],
        };
        let after = BenchmarkStage {
            configuration: BTreeMap::from([(String::from("seed"), String::from("2"))]),
            ..before.clone()
        };
        assert!(matches!(
            compare_stages(&before, &after, 0.1),
            Err(ComparisonError::ConfigurationDrift(_))
        ));
    }

    #[test]
    fn comparison_reports_latency_regression() {
        let before = BenchmarkStage {
            stage: "pre-change".to_owned(),
            git_sha: "a".to_owned(),
            dirty_worktree: false,
            configuration: BTreeMap::new(),
            reports: vec![report(100)],
        };
        let after = BenchmarkStage {
            stage: "post".to_owned(),
            reports: vec![report(111)],
            ..before.clone()
        };
        let comparison = compare_stages(&before, &after, 0.1).expect("same workload compares");
        assert_eq!(comparison.regressions.len(), 1);
    }
}

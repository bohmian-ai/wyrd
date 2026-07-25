use serde::Serialize;
use wyrd_bench::{BenchmarkReadiness, BifrostReportEnvelope};

#[derive(Debug, Serialize)]
pub(crate) struct LaneExecutionReport {
    pub(crate) envelope: BifrostReportEnvelope,
    pub(crate) elapsed_us: u64,
    pub(crate) verified: bool,
    pub(crate) rows: u64,
}

pub(crate) fn emit_report(
    report: &LaneExecutionReport,
    lane: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository_report_path(&format!("{lane}.json")),
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(report)?))?;
    Ok(())
}

fn repository_report_path(file_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/task16")
        .join(file_name)
}

pub(crate) fn readiness(verified: bool) -> BenchmarkReadiness {
    if verified {
        BenchmarkReadiness::Ready
    } else {
        BenchmarkReadiness::NotReady
    }
}

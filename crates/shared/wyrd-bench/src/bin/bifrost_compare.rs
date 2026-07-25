use std::path::PathBuf;

use wyrd_bench::{BenchmarkStage, compare_bifrost_stages, compare_stages};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let before_path = argument("before")?;
    let after_path = argument("after")?;
    let output_path = argument("output")?;
    let before_text = std::fs::read_to_string(&before_path)?;
    let after_text = std::fs::read_to_string(&after_path)?;
    let before_value: serde_json::Value = serde_json::from_str(&before_text)?;
    let after_value: serde_json::Value = serde_json::from_str(&after_text)?;
    let output = if is_bifrost_stage(&before_value) || is_bifrost_stage(&after_value) {
        if !is_bifrost_stage(&before_value) || !is_bifrost_stage(&after_value) {
            return Err("cannot compare Bifrost reports with historical reports".into());
        }
        compare_bifrost_stages(&before_value, &after_value, 0.10)?
    } else {
        let before: BenchmarkStage = serde_json::from_value(before_value)?;
        let after: BenchmarkStage = serde_json::from_value(after_value)?;
        serde_json::to_value(compare_stages(&before, &after, 0.10)?)?
    };
    std::fs::write(
        output_path,
        format!("{}\n", serde_json::to_string_pretty(&output)?),
    )?;
    Ok(())
}

fn is_bifrost_stage(value: &serde_json::Value) -> bool {
    value
        .get("reports")
        .and_then(serde_json::Value::as_array)
        .and_then(|reports| reports.first())
        .and_then(|report| {
            report
                .get("ack")
                .and_then(|ack| ack.get("envelope"))
                .or_else(|| report.get("envelope"))
        })
        .and_then(|envelope| envelope.get("report_version"))
        .is_some()
}

fn argument(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let prefix = format!("--{name}=");
    std::env::args()
        .find_map(|value| value.strip_prefix(&prefix).map(PathBuf::from))
        .ok_or_else(|| format!("missing --{name}=... argument").into())
}

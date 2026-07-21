use std::path::PathBuf;

use wyrd_bench::{BenchmarkStage, compare_stages};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let before_path = argument("before")?;
    let after_path = argument("after")?;
    let output_path = argument("output")?;
    let before: BenchmarkStage = serde_json::from_str(&std::fs::read_to_string(&before_path)?)?;
    let after: BenchmarkStage = serde_json::from_str(&std::fs::read_to_string(&after_path)?)?;
    let comparison = compare_stages(&before, &after, 0.10)?;
    std::fs::write(
        output_path,
        format!("{}\n", serde_json::to_string_pretty(&comparison)?),
    )?;
    Ok(())
}

fn argument(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let prefix = format!("--{name}=");
    std::env::args()
        .find_map(|value| value.strip_prefix(&prefix).map(PathBuf::from))
        .ok_or_else(|| format!("missing --{name}=... argument").into())
}

use std::path::PathBuf;

use wyrd_bench::compare_bifrost_stages;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let before_path = argument("before")?;
    let after_path = argument("after")?;
    let output_path = argument("output")?;
    let before_text = std::fs::read_to_string(&before_path)?;
    let after_text = std::fs::read_to_string(&after_path)?;
    let before_value: serde_json::Value = serde_json::from_str(&before_text)?;
    let after_value: serde_json::Value = serde_json::from_str(&after_text)?;
    let output = compare_bifrost_stages(&before_value, &after_value, 0.10)?;
    std::fs::write(
        output_path,
        format!("{}\n", serde_json::to_string_pretty(&output)?),
    )?;
    Ok(())
}

fn argument(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let prefix = format!("--{name}=");
    std::env::args()
        .find_map(|value| value.strip_prefix(&prefix).map(PathBuf::from))
        .ok_or_else(|| format!("missing --{name}=... argument").into())
}

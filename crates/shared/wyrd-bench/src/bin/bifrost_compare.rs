//! Compare two controlled full-cluster Bifrost manifests.

use std::path::PathBuf;

use wyrd_bench::{BifrostReferenceProfile, compare_cluster_profiles};

/// Parse manifests, always emit a structured comparison when possible, and
/// return nonzero for incompatible, unsupported, or regressed results.
///
/// # Errors
/// Returns an IO or strict JSON error when inputs cannot be read/decoded or
/// the requested output cannot be written.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let before_path = argument("before")?;
    let after_path = argument("after")?;
    let output_path = argument("output")?;
    let before: BifrostReferenceProfile =
        serde_json::from_str(&std::fs::read_to_string(before_path)?)?;
    let after: BifrostReferenceProfile =
        serde_json::from_str(&std::fs::read_to_string(after_path)?)?;
    let comparison = compare_cluster_profiles(&before, &after);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output_path,
        format!("{}\n", serde_json::to_string_pretty(&comparison)?),
    )?;
    if comparison.ready {
        Ok(())
    } else {
        Err("Bifrost comparison is incompatible, unsupported, or not ready".into())
    }
}

/// Resolve either `--name value` or `--name=value` without positional fallbacks.
///
/// # Errors
/// Returns an error when the required named path is absent.
fn argument(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let prefix = format!("--{name}=");
    let flag = format!("--{name}");
    let mut arguments = std::env::args().skip(1);
    while let Some(value) = arguments.next() {
        if let Some(path) = value.strip_prefix(&prefix) {
            return Ok(PathBuf::from(path));
        }
        if value == flag {
            return arguments
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("missing value after {flag}").into());
        }
    }
    Err(format!("missing {flag} <path>").into())
}

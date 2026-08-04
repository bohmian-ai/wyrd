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
    let (before_path, after_path, output_path) = parse_arguments(std::env::args().skip(1))?;
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

/// Parse the exact comparator grammar and reject repeated/unknown flags.
///
/// # Errors
/// Returns an error for missing, repeated, unknown, or positional arguments.
fn parse_arguments<I, S>(
    arguments: I,
) -> Result<(PathBuf, PathBuf, PathBuf), Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let values = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut before = None;
    let mut after = None;
    let mut output = None;
    let mut index = 0;
    while index < values.len() {
        let flag = values[index].as_str();
        let slot = match flag {
            "--before" => &mut before,
            "--after" => &mut after,
            "--output" => &mut output,
            other => return Err(format!("unknown comparator argument `{other}`").into()),
        };
        if slot.is_some() || index + 1 >= values.len() {
            return Err(format!("{flag} must occur exactly once with a path").into());
        }
        let value = values[index + 1].as_str();
        if value.starts_with('-') {
            return Err(format!("missing path after {flag}").into());
        }
        *slot = Some(PathBuf::from(value));
        index += 2;
    }
    Ok((
        before.ok_or("missing --before <baseline.json>")?,
        after.ok_or("missing --after <candidate.json>")?,
        output.ok_or("missing --output <comparison.json>")?,
    ))
}

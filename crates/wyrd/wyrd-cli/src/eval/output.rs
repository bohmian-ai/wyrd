//! CLI output and local result persistence.

use std::path::{Path, PathBuf};

use vala_eval::{ComparisonResults, EvalResults, compare};

use crate::error::WyrdCliError;

/// Print a structured CLI error.
pub fn print_cli_error(error: &WyrdCliError) {
    let json = serde_json::json!({
        "kind": "wyrd_cli_error",
        "code": error.code(),
        "status": error.status(),
        "message": error.to_string(),
        "remediation": error.remediation(),
    });
    eprintln!("{json}");
    eprintln!("error: {error}");
}

/// Print local eval summary.
pub fn print_local_summary(
    run: &EvalResults,
    comparison: Option<&ComparisonResults>,
    out_dir: Option<&Path>,
) {
    print!("{}", run.as_table());
    if let Some(comparison) = comparison {
        println!(
            "comparison: {} subject delta(s), {} scenario delta(s)",
            comparison.subjects.len(),
            comparison.scenarios.len()
        );
    }
    if let Some(out_dir) = out_dir {
        println!("results: {}", out_dir.join("results.json").display());
    }
}

/// Print server protocol completion.
pub fn print_server_run_complete(server_url: &url::Url, run_id: &wyrd_spec::vala::ids::RunId) {
    println!("Eval run {run_id} complete on {server_url}");
    println!("Results are server-side; the v1 protocol has no results-fetch route.");
}

/// Return true when no pass gate failed.
#[must_use]
pub fn pass_gate_satisfied(run: &EvalResults) -> bool {
    match &run.pass_gate_verdict {
        Some(verdict) => verdict.passed,
        None => true,
    }
}

/// Load optional baseline and compare.
///
/// # Errors
/// Returns an error when the baseline cannot be loaded.
pub fn load_comparison(
    baseline: Option<&Path>,
    run: &EvalResults,
) -> Result<Option<ComparisonResults>, WyrdCliError> {
    baseline
        .map(|path| {
            let baseline =
                EvalResults::load(path).map_err(|source| WyrdCliError::EvalEngine { source })?;
            Ok(compare(
                &baseline,
                run,
                &vala_eval::CompareConfig::default(),
            ))
        })
        .transpose()
}

/// Write local result artifacts and return the output directory.
///
/// # Errors
/// Returns an error when serialization or filesystem writes fail.
pub fn write_results(
    out: Option<&PathBuf>,
    run: &EvalResults,
    comparison: Option<&ComparisonResults>,
) -> Result<PathBuf, WyrdCliError> {
    let out_dir = out.cloned().unwrap_or_else(|| {
        PathBuf::from(".wyrd")
            .join("eval")
            .join(run.identity.run_id.as_str())
    });
    std::fs::create_dir_all(&out_dir).map_err(|source| WyrdCliError::Io { source })?;
    run.save(&out_dir.join("results.json"))
        .map_err(|source| WyrdCliError::EvalEngine { source })?;
    run.save(&out_dir.join("SUMMARY.json"))
        .map_err(|source| WyrdCliError::EvalEngine { source })?;
    if let Some(comparison) = comparison {
        comparison
            .save(&out_dir.join("comparison.json"))
            .map_err(|source| WyrdCliError::EvalEngine { source })?;
    }
    Ok(out_dir)
}

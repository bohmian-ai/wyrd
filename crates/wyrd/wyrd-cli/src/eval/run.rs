//! `wyrd eval run` argument parsing and shared helpers.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::EvalSpec;

use crate::error::WyrdCliError;

/// `wyrd eval` subcommand surface.
#[derive(Debug, Subcommand)]
pub enum EvalCommand {
    /// Score pre-collected records offline against a bare Eval spec.
    Run(EvalRunArgs),
}

/// Flags for `wyrd eval run`.
///
/// `--records` scores pre-collected observations offline against the bare
/// `--eval` spec, attributed to `--subject`.
#[derive(Debug, Args)]
pub struct EvalRunArgs {
    /// Bare `EvalSpec` YAML or JSON file to score against.
    #[arg(long, value_name = "PATH")]
    pub eval: PathBuf,
    /// JSONL file of pre-collected EvalRecordObservation rows.
    #[arg(long, value_name = "PATH")]
    pub records: PathBuf,
    /// Card that emitted the pre-collected records.
    #[arg(long, value_name = "CARD_REF")]
    pub subject: CardRef,
    /// Baseline EvalResults JSON for comparison.
    #[arg(long, value_name = "PATH")]
    pub baseline: Option<PathBuf>,
    /// Directory for local output artifacts.
    #[arg(long, value_name = "DIR", env = "WYRD_EVAL_OUT")]
    pub out: Option<PathBuf>,
    /// Exit non-zero when the local pass gate fails.
    #[arg(long, default_value_t = true)]
    pub fail_on_gate: bool,
}

/// Dispatch an eval subcommand.
///
/// # Errors
/// Returns an error when the Verifier card, records, scoring, or output fails.
pub async fn dispatch(command: EvalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        EvalCommand::Run(args) => crate::eval::local_records::run_local_records(args).await,
    }
}

/// Load a bare [`EvalSpec`] from a local YAML or JSON file.
///
/// YAML is a superset of JSON, so one YAML decode reads both formats. The
/// decoded spec is validated before it is scored.
///
/// # Errors
/// Returns an error when the file cannot be read, does not decode as an
/// `EvalSpec`, or fails spec validation.
pub fn load_eval_spec(path: &Path) -> Result<EvalSpec, WyrdCliError> {
    let raw = std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?;
    let spec: EvalSpec = serde_yaml::from_str(&raw).map_err(|source| WyrdCliError::Parse {
        kind: "Eval spec",
        path: path.display().to_string(),
        detail: source.to_string(),
    })?;
    spec.validate()
        .map_err(|error| WyrdCliError::EvalSpecInvalid {
            detail: error.to_string(),
        })?;
    Ok(spec)
}

//! `wyrd eval run` argument parsing and shared helpers.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Args, Subcommand};
use vala_eval::{JudgeInvoker, MockJudgeInvoker};
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::card::verifier::{VerifierImplementation, VerifierSpec};
use wyrd_spec::envelope::{Card, CardKind, Spec};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::{EvalSpec, EvalTask};

use crate::error::WyrdCliError;

/// `wyrd eval` subcommand surface.
#[derive(Debug, Subcommand)]
pub enum EvalCommand {
    /// Score pre-collected records against an Eval-backed Verifier card.
    Run(EvalRunArgs),
}

/// Flags for `wyrd eval run`.
///
/// Exactly one evaluation source is required: `--server` drives a remote
/// protocol router, while `--records` scores pre-collected observations
/// offline (optionally attributed via `--subject`). The two sources conflict.
#[derive(Debug, Args)]
pub struct EvalRunArgs {
    /// Verifier card filesystem path whose implementation is `eval`.
    #[arg(long, value_name = "PATH")]
    pub eval: String,
    /// JSONL file of pre-collected EvalRecordObservation rows.
    #[arg(long, value_name = "PATH")]
    pub records: PathBuf,
    /// Card that emitted the pre-collected records.
    #[arg(long, value_name = "CARD_REF")]
    pub subject: CardRef,
    /// Use a deterministic judge invoker (all LLM judge tasks return passed: true; results do not reflect real judge behavior).
    #[arg(long, default_value_t = false)]
    pub judge_mock: bool,
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

/// Load an Eval-backed Verifier card from a local YAML or JSON file.
///
/// Returns the card's own [`CardRef`] (kind `Verifier`) with the Eval
/// implementation payload so the local engine scores against it.
///
/// # Errors
/// Returns an error for registry refs, unsupported extensions, IO, parse, or
/// cards that are not `kind: Verifier` with `implementation.kind: eval`.
pub fn load_eval_card(input: &str) -> Result<(CardRef, EvalSpec), WyrdCliError> {
    let path = Path::new(input);
    if !path.exists() {
        return Err(WyrdCliError::RegistryRefUnsupported);
    }
    let raw = std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?;
    let path_display = path.display().to_string();
    let card: Card = match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => serde_json::from_str(&raw).map_err(|source| WyrdCliError::Parse {
            kind: "eval-backed Verifier card JSON",
            path: path_display.clone(),
            detail: source.to_string(),
        })?,
        Some("yaml" | "yml") => {
            serde_yaml::from_str(&raw).map_err(|source| WyrdCliError::Parse {
                kind: "eval-backed Verifier card YAML",
                path: path_display.clone(),
                detail: source.to_string(),
            })?
        }
        _ => return Err(WyrdCliError::CardExtensionUnsupported { path: path_display }),
    };
    let Spec::Verifier(VerifierSpec {
        implementation: VerifierImplementation::Eval(spec),
        ..
    }) = card.spec
    else {
        return Err(WyrdCliError::NotEvalCard);
    };
    spec.validate()
        .map_err(|error| WyrdCliError::EvalSpecInvalid {
            detail: error.to_string(),
        })?;
    let version: VersionBlock = match card.metadata.version {
        Some(VersionSpec::Pin(block)) => block,
        Some(VersionSpec::Scope(_)) => {
            return Err(WyrdCliError::EvalSpecInvalid {
                detail: "metadata.version must be an exact semver pin, not a range".to_owned(),
            });
        }
        None => {
            return Err(WyrdCliError::EvalSpecInvalid {
                detail: "metadata.version is required to run an eval-backed Verifier card"
                    .to_owned(),
            });
        }
    };
    let space = card
        .metadata
        .space
        .ok_or_else(|| WyrdCliError::EvalSpecInvalid {
            detail: "metadata.space is required to run an eval-backed Verifier card".to_owned(),
        })?;
    let eval_ref = CardRef {
        kind: CardKind::Verifier,
        name: card.metadata.name,
        version,
        space: Some(space),
        uid: card.metadata.uid,
    };
    Ok((eval_ref, spec))
}

/// Build a judge invoker for one spec.
///
/// # Errors
/// Returns an error when the spec contains LLM judges and `--judge-mock` is
/// not set.
pub fn judge_for_spec(
    spec: &EvalSpec,
    judge_mock: bool,
) -> Result<Arc<dyn JudgeInvoker>, WyrdCliError> {
    if has_llm_judge(spec) && !judge_mock {
        return Err(WyrdCliError::JudgeMockRequired);
    }
    if judge_mock && has_llm_judge(spec) {
        eprintln!(
            "[judge-mock] LlmJudge tasks will return {{\"passed\": true}}; \
             enable a provider for real verdicts."
        );
    }
    let responses = std::iter::repeat_with(|| Ok(serde_json::json!({"passed": true}))).take(1024);
    Ok(MockJudgeInvoker::new(responses))
}

fn has_llm_judge(spec: &EvalSpec) -> bool {
    spec.tasks
        .values()
        .any(|task| matches!(task, EvalTask::LlmJudge(_)))
}

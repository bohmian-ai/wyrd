//! `wyrd eval run` argument parsing and shared helpers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Args, Subcommand};
use serde::Deserialize;
use url::Url;
use vala_eval::{JudgeInvoker, MockJudgeInvoker};
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::envelope::{Card, CardKind, Spec};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::protocol::SimulatedUserMode;
use wyrd_spec::vala::eval::{EvalSpec, EvalTask, ScenarioId};

use crate::error::WyrdCliError;

/// Scenario-scoped scripted simulated-user rows.
pub type ScriptedUserRows = BTreeMap<(ScenarioId, u32), String>;

/// `wyrd eval` subcommand surface.
#[derive(Debug, Subcommand)]
pub enum EvalCommand {
    /// Execute an Eval card.
    Run(EvalRunArgs),
}

/// Flags for `wyrd eval run`.
///
/// Exactly one evaluation source is required: `--server` drives a remote
/// protocol router, while `--records` scores pre-collected observations
/// offline (optionally attributed via `--subject`). The two sources conflict.
#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("source")
        .required(true)
        .args(["server", "records"]),
))]
pub struct EvalRunArgs {
    /// Eval card filesystem path.
    #[arg(long, value_name = "PATH")]
    pub eval: String,
    /// Agent endpoint URL.
    #[arg(long, value_name = "URL", conflicts_with = "records")]
    pub agent_url: Option<Url>,
    /// JSONL file of pre-collected EvalRecordObservation rows.
    #[arg(long, value_name = "PATH", conflicts_with = "server")]
    pub records: Option<PathBuf>,
    /// Card that emitted the pre-collected records.
    #[arg(long, value_name = "CARD_REF", requires = "records")]
    pub subject: Option<CardRef>,
    /// Drive a remote server protocol router. The credential for the
    /// authenticated `/v1/eval` surface is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", conflicts_with = "records")]
    pub server: Option<Url>,
    /// Use a deterministic judge invoker (all LLM judge tasks return passed: true; results do not reflect real judge behavior).
    #[arg(long, default_value_t = false)]
    pub judge_mock: bool,
    /// Who supplies unscripted user turns.
    #[arg(long, value_enum, default_value_t = SimulatedUserCli::Server)]
    pub simulated_user: SimulatedUserCli,
    /// JSONL file containing client-supplied user turns.
    #[arg(long, value_name = "PATH")]
    pub simulated_user_script: Option<PathBuf>,
    /// Baseline EvalResults JSON for comparison.
    #[arg(long, value_name = "PATH")]
    pub baseline: Option<PathBuf>,
    /// Directory for local output artifacts.
    #[arg(long, value_name = "DIR", env = "WYRD_EVAL_OUT")]
    pub out: Option<PathBuf>,
    /// Agent HTTP timeout in seconds.
    #[arg(long, default_value = "60", value_name = "SECONDS")]
    pub agent_timeout_secs: u64,
    /// Exit non-zero when the local pass gate fails.
    #[arg(long, default_value_t = true)]
    pub fail_on_gate: bool,
}

/// CLI mirror of [`SimulatedUserMode`].
#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum SimulatedUserCli {
    /// Server or local orchestrator supplies user turns.
    Server,
    /// Client script supplies user turns.
    Client,
}

impl From<SimulatedUserCli> for SimulatedUserMode {
    fn from(value: SimulatedUserCli) -> Self {
        match value {
            SimulatedUserCli::Server => Self::Server,
            SimulatedUserCli::Client => Self::Client,
        }
    }
}

/// Dispatch an eval subcommand.
pub async fn dispatch(command: EvalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        EvalCommand::Run(args) => run(args).await,
    }
}

async fn run(args: EvalRunArgs) -> Result<ExitCode, WyrdCliError> {
    validate_args(&args)?;
    if let Some(server_url) = args.server.clone() {
        crate::eval::server::run_server(args, server_url).await
    } else {
        crate::eval::local_records::run_local_records(args).await
    }
}

fn validate_args(args: &EvalRunArgs) -> Result<(), WyrdCliError> {
    if args.server.is_some() && args.agent_url.is_none() {
        return Err(WyrdCliError::ServerRequiresAgentUrl);
    }
    if matches!(args.simulated_user, SimulatedUserCli::Client)
        && args.simulated_user_script.is_none()
        && args.records.is_none()
    {
        return Err(WyrdCliError::SimulatedUserScriptRequired);
    }
    if args.records.is_some() && args.subject.is_none() {
        return Err(WyrdCliError::RecordsRequireSubject);
    }
    Ok(())
}

/// Load an Eval card from a local YAML or JSON file.
///
/// # Errors
/// Returns an error for registry refs, unsupported extensions, IO, parse, or
/// non-Eval cards.
pub fn load_eval_card(input: &str) -> Result<(CardRef, EvalSpec), WyrdCliError> {
    let path = Path::new(input);
    if !path.exists() {
        return Err(WyrdCliError::RegistryRefUnsupported);
    }
    let raw = std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?;
    let path_display = path.display().to_string();
    let card: Card = match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => serde_json::from_str(&raw).map_err(|source| WyrdCliError::Parse {
            kind: "eval card JSON",
            path: path_display.clone(),
            detail: source.to_string(),
        })?,
        Some("yaml" | "yml") => {
            serde_yaml::from_str(&raw).map_err(|source| WyrdCliError::Parse {
                kind: "eval card YAML",
                path: path_display.clone(),
                detail: source.to_string(),
            })?
        }
        _ => return Err(WyrdCliError::CardExtensionUnsupported { path: path_display }),
    };
    if card.kind != CardKind::Eval {
        return Err(WyrdCliError::NotEvalCard);
    }
    let Spec::Eval(spec) = card.spec else {
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
                detail: "metadata.version is required to run an eval card".to_owned(),
            });
        }
    };
    let space = card
        .metadata
        .space
        .ok_or_else(|| WyrdCliError::EvalSpecInvalid {
            detail: "metadata.space is required to run an eval card".to_owned(),
        })?;
    let eval_ref = CardRef {
        kind: CardKind::Eval,
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

/// Load a JSONL simulated-user script.
///
/// # Errors
/// Returns an error on IO or malformed JSONL.
pub fn load_scripted_user(path: &Path) -> Result<ScriptedUserRows, WyrdCliError> {
    #[derive(Deserialize)]
    struct Row {
        scenario_id: ScenarioId,
        turn: u32,
        message: String,
    }

    let raw = std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?;
    let mut out = BTreeMap::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let row: Row = serde_json::from_str(trimmed).map_err(|source| WyrdCliError::Parse {
            kind: "simulated-user JSONL",
            path: path.display().to_string(),
            detail: format!("line {}: {source}", index + 1),
        })?;
        out.insert((row.scenario_id, row.turn), row.message);
    }
    Ok(out)
}

/// Return a scripted message by scenario and turn.
///
/// # Errors
/// Returns an error when the turn is missing.
pub fn scripted_message(
    script: &ScriptedUserRows,
    scenario_id: &ScenarioId,
    turn: u32,
) -> Result<String, WyrdCliError> {
    script
        .get(&(scenario_id.clone(), turn))
        .cloned()
        .ok_or_else(|| WyrdCliError::ScriptedTurnMissing {
            scenario_id: scenario_id.as_str().to_owned(),
            turn,
        })
}

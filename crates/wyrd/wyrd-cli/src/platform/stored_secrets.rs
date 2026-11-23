//! Deployment wrapping-key maintenance from the command line.
//!
//! After rolling out a replacement active wrapping key, a deployment
//! administrator repeats one bounded pass until nothing remains: `routine`
//! for planned rotation, `recovery` after a key or data-key exposure. The
//! destination is always the server's configured active key.

use std::num::NonZeroU32;
use std::process::ExitCode;

use clap::{Args, Subcommand, ValueEnum};
use wyrd_spec::security::stored_secret::{
    StoredSecretMaintenanceMode, StoredSecretMaintenanceRequest,
};

use super::credential::PlatformEndpoint;
use crate::error::WyrdCliError;

/// Stored-secret verbs on the platform plane.
#[derive(Debug, Subcommand)]
pub enum StoredSecretsCommand {
    /// Run one bounded pass moving stored secrets onto the active wrapping
    /// key, and print what it observed.
    Maintain(MaintainArgs),
}

/// How the pass moves an envelope, as the command line spells it.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Mode {
    /// Rewrap each data key; keep the data key and payload.
    Routine,
    /// Reseal each value under a fresh data key; run only into a fresh,
    /// never-reused active key.
    Recovery,
}

impl From<Mode> for StoredSecretMaintenanceMode {
    /// The wire mode for a command-line mode.
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Routine => Self::Routine,
            Mode::Recovery => Self::Recovery,
        }
    }
}

/// Run one maintenance pass.
#[derive(Debug, Args)]
pub struct MaintainArgs {
    /// Routine rotation or compromise recovery.
    #[arg(long, value_enum)]
    pub mode: Mode,
    /// Most envelopes this pass attempts to move. Omit to attempt all within
    /// the server's time budget.
    #[arg(long, value_name = "N")]
    pub limit: Option<NonZeroU32>,
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// Dispatch one stored-secret verb.
///
/// # Errors
/// Returns [`WyrdCliError::NoPlatformCredential`] without a platform
/// credential, and the server's stable Wyrd error when the caller is refused,
/// the deployment has no `[kms]`, or the pass cannot read the catalog.
pub async fn dispatch(command: StoredSecretsCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        StoredSecretsCommand::Maintain(args) => maintain(args).await,
    }
}

/// Run one pass and print its counts, then one line per reported failure.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn maintain(args: MaintainArgs) -> Result<ExitCode, WyrdCliError> {
    let report = args
        .endpoint
        .connect()
        .await?
        .maintain_stored_secrets(&StoredSecretMaintenanceRequest {
            mode: args.mode.into(),
            limit: args.limit,
        })
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    let mode = match report.mode {
        StoredSecretMaintenanceMode::Routine => "routine",
        StoredSecretMaintenanceMode::Recovery => "recovery",
    };
    println!("mode: {mode}");
    println!("current: {}", report.current);
    println!("changed: {}", report.changed);
    println!("remaining: {}", report.remaining);
    println!("bounded: {}", report.bounded);
    for failure in &report.failures {
        println!("failure: {}", serde_json::json!(failure));
    }
    if report.remaining > 0 || report.bounded {
        println!("Repeat with the same --mode to continue.");
    }
    Ok(ExitCode::SUCCESS)
}

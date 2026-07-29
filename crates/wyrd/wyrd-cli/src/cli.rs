//! Top-level `clap` command tree.

use clap::{Parser, Subcommand};

use crate::audit::AuditCommand;
use crate::auth::AuthCommand;
use crate::card::{ApplyArgs, DeleteArgs, GetArgs, LatestArgs, ListArgs, LoadArgs, PlanArgs};
use crate::eval::run::EvalCommand;
use crate::principal::PrincipalCommand;

/// Wyrd command-line interface.
#[derive(Debug, Parser)]
#[command(name = "wyrd", version, about, propagate_version = true)]
pub struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Command,
}

/// Dispatches the parsed client command through its single owning CLI.
impl Cli {
    /// Dispatch the selected client command through its owning CLI capability.
    ///
    /// The method consumes the parser result so each subcommand receives its
    /// owned arguments. It is async because command handlers perform client
    /// HTTP and local runtime IO.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::WyrdCliError`] when parsing-compatible command
    /// inputs fail local validation or a client/server operation fails.
    pub async fn dispatch(self) -> Result<std::process::ExitCode, crate::error::WyrdCliError> {
        match self.command {
            Command::Plan(args) => crate::card::dispatch_plan(args).await,
            Command::Apply(args) => crate::card::dispatch_apply(args).await,
            Command::Get(args) => crate::card::dispatch_get(args).await,
            Command::Latest(args) => crate::card::dispatch_latest(args).await,
            Command::List(args) => crate::card::dispatch_list(args).await,
            Command::Load(args) => crate::card::dispatch_load(args).await,
            Command::Delete(args) => crate::card::dispatch_delete(args).await,
            Command::Audit(command) => crate::audit::dispatch(command).await,
            Command::Auth(command) => crate::auth::dispatch(command).await,
            Command::Eval(command) => crate::eval::run::dispatch(command).await,
            Command::Principal(command) => crate::principal::dispatch(command).await,
        }
    }
}

/// Top-level CLI verbs.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Load and validate a local card tree without contacting a server.
    Plan(PlanArgs),
    /// Register a local card tree and run its artifact lifecycle.
    Apply(ApplyArgs),
    /// Hydrate a selected Card and its reachable graph into --output-dir;
    /// complete artifact downloads are the default and --metadata-only writes
    /// an inspectable, non-runnable bundle.
    Get(GetArgs),
    /// Resolve the latest active card version by name.
    Latest(LatestArgs),
    /// List card summaries with typed server-side filters.
    List(ListArgs),
    /// Load a card and materialize its artifacts.
    Load(LoadArgs),
    /// Soft-delete one exact card.
    Delete(DeleteArgs),
    /// Audit log commands (verify seal checkpoints).
    #[command(subcommand)]
    Audit(AuditCommand),
    /// Authenticate with a Wyrd server (login, refresh, issue-key).
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Run, manage, and compare evaluations.
    #[command(subcommand)]
    Eval(EvalCommand),
    /// Manage Wyrd principals (revoke).
    #[command(subcommand)]
    Principal(PrincipalCommand),
}

//! Top-level `clap` command tree.

use clap::{Parser, Subcommand};

use crate::auth::AuthCommand;
use crate::card::{ApplyArgs, DeleteArgs, GetArgs, LatestArgs, ListArgs, LoadArgs, PlanArgs};
use crate::eval::run::EvalCommand;
use crate::principal::PrincipalCommand;
use crate::query::QueryCommand;

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
    /// Returns [`crate::error::CliBoundaryError`] when parsing-compatible
    /// command inputs fail local validation or a client/server operation fails.
    /// Query commands keep their typed Bifrost error; every other command keeps
    /// the derive-catalogued local CLI error.
    pub async fn dispatch(self) -> Result<std::process::ExitCode, crate::error::CliBoundaryError> {
        match self.command {
            Command::Plan(args) => crate::card::dispatch_plan(args).await.map_err(Into::into),
            Command::Apply(args) => crate::card::dispatch_apply(args).await.map_err(Into::into),
            Command::Get(args) => crate::card::dispatch_get(args).await.map_err(Into::into),
            Command::Latest(args) => crate::card::dispatch_latest(args).await.map_err(Into::into),
            Command::List(args) => crate::card::dispatch_list(args).await.map_err(Into::into),
            Command::Load(args) => crate::card::dispatch_load(args).await.map_err(Into::into),
            Command::Delete(args) => crate::card::dispatch_delete(args).await.map_err(Into::into),
            Command::Auth(command) => crate::auth::dispatch(command).await.map_err(Into::into),
            Command::Eval(command) => crate::eval::run::dispatch(command)
                .await
                .map_err(Into::into),
            Command::Principal(command) => crate::principal::dispatch(command)
                .await
                .map_err(Into::into),
            Command::Query(command) => crate::query::dispatch(command).await,
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
    /// Authenticate with a Wyrd server (login, refresh, issue-key).
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Run, manage, and compare evaluations.
    #[command(subcommand)]
    Eval(EvalCommand),
    /// Manage Wyrd principals (revoke).
    #[command(subcommand)]
    Principal(PrincipalCommand),
    /// Run a terminal-safe streaming Oracle query.
    Query(QueryCommand),
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    /// The whole command tree is internally consistent.
    ///
    /// `propagate_version` pushes `--version` into every subcommand, so a
    /// subcommand that declares its own `--version` makes clap panic when that
    /// subcommand is parsed — not when the binary starts. Per-command unit tests
    /// build a bare wrapper without the propagated flag and cannot see it, so
    /// this asserts the real tree the binary ships.
    #[test]
    fn the_shipped_command_tree_is_consistent() {
        Cli::command().debug_assert();
    }
}

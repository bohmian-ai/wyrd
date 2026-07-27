//! Top-level `clap` command tree.

use clap::{Parser, Subcommand};

use crate::audit::AuditCommand;
use crate::auth::AuthCommand;
use crate::card::{ApplyArgs, DeleteArgs, GetArgs, LatestArgs, ListArgs, LoadArgs, PlanArgs};
use crate::dev::DevCommand;
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
    /// Developer utilities (bootstrap local dev environment).
    #[command(subcommand)]
    Dev(DevCommand),
    /// Run, manage, and compare evaluations.
    #[command(subcommand)]
    Eval(EvalCommand),
    /// Manage Wyrd principals (revoke).
    #[command(subcommand)]
    Principal(PrincipalCommand),
}

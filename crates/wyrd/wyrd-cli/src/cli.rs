//! Top-level `clap` command tree.

use clap::{Parser, Subcommand};

use crate::auth::AuthCommand;
use crate::dev::DevCommand;
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

/// Top-level CLI verbs.
#[derive(Debug, Subcommand)]
pub enum Command {
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
    /// Run a terminal-safe streaming Oracle query.
    Query(QueryCommand),
}

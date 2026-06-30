//! Top-level `clap` command tree.

use clap::{Parser, Subcommand};

use crate::eval::run::EvalCommand;

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
    /// Run, manage, and compare evaluations.
    #[command(subcommand)]
    Eval(EvalCommand),
}

/// `wyrd principal list`, `access`, `credential`, `revoke`, and `user`.
pub mod access;
pub mod assignment;
pub mod credential;
pub mod revoke;
pub mod user;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
/// The `wyrd principal` subcommands: acting on a principal's identity and
/// acting on the credentials that authenticate it are kept apart so revoking
/// access never reads as a credential edit.
pub enum PrincipalCommand {
    /// Suspend a principal so it can mint no new token; tokens it already
    /// holds lapse within five minutes.
    Revoke(revoke::RevokeArgs),
    /// Create principals and administer their credentials.
    #[command(subcommand)]
    Credential(credential::CredentialCommand),
    /// List the tenant's assignable users, services, and agents.
    List(assignment::ListArgs),
    /// Read and change scoped access: bindings, roles, and groups.
    #[command(subcommand)]
    Access(access::AccessCommand),
    /// Create local Users and invite people to sign up.
    #[command(subcommand)]
    User(user::UserCommand),
}

/// Route a `wyrd principal` invocation to the subcommand that owns it.
///
/// # Errors
/// Returns whatever [`WyrdCliError`] the selected subcommand produced.
pub async fn dispatch(command: PrincipalCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        PrincipalCommand::Revoke(args) => revoke::dispatch(args).await,
        PrincipalCommand::Credential(command) => credential::dispatch(command).await,
        PrincipalCommand::List(args) => assignment::list(args).await,
        PrincipalCommand::Access(command) => access::dispatch(command).await,
        PrincipalCommand::User(command) => user::dispatch(command).await,
    }
}

pub mod issue_key;
pub mod login;
pub mod refresh;
pub mod trusted_issuer;
pub mod workload_binding;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Issue an API key bound to a card ref (POST /auth/issue-key).
    IssueKey(issue_key::IssueKeyArgs),
    /// Initiate an OIDC login flow and exchange the callback code for a Wyrd access token.
    Login(login::LoginArgs),
    /// Rotate a Wyrd refresh token and print the new access and refresh tokens.
    Refresh(refresh::RefreshArgs),
    /// Manage trusted OIDC issuers (add, list, rm).
    #[command(subcommand)]
    TrustedIssuer(trusted_issuer::TrustedIssuerCommand),
    /// Manage workload bindings (add, list, rm).
    #[command(subcommand)]
    WorkloadBinding(workload_binding::WorkloadBindingCommand),
}

pub async fn dispatch(command: AuthCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        AuthCommand::IssueKey(args) => issue_key::dispatch(args).await,
        AuthCommand::Login(args) => login::dispatch(args).await,
        AuthCommand::Refresh(args) => refresh::dispatch(args).await,
        AuthCommand::TrustedIssuer(cmd) => trusted_issuer::dispatch(cmd).await,
        AuthCommand::WorkloadBinding(cmd) => workload_binding::dispatch(cmd).await,
    }
}

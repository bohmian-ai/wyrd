/// `wyrd auth grant-role`: grant a Role to a Card-bound principal.
pub mod grant_role;
/// `wyrd auth issue-key`: mint an API key bound to a Card.
pub mod issue_key;
/// `wyrd auth login`, `logout`, and `status`: the saved human user login.
pub mod login;
/// `wyrd auth refresh`: rotate a Wyrd refresh token.
pub mod refresh;
/// `wyrd auth trusted-issuer`: administer the tenant's trusted OIDC issuers.
pub mod trusted_issuer;
/// `wyrd auth workload-binding`: administer issuer-subject to Card bindings.
pub mod workload_binding;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

/// Subcommands of `wyrd auth`; each variant routes to its module's `dispatch`.
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Issue an API key bound to a card ref (POST /auth/issue-key).
    IssueKey(issue_key::IssueKeyArgs),
    /// Grant a Role to the principal bound to a Service or Agent card
    /// (POST /v1/auth/grant-role); requires tenant administration.
    GrantRole(grant_role::GrantRoleArgs),
    /// Sign in to a tenant through the browser and save the Wyrd user login
    /// every local SDK uses; no token is printed.
    Login(login::LoginArgs),
    /// End a saved login: delete it locally, then revoke it on the server
    /// best-effort.
    Logout(login::LogoutArgs),
    /// Show saved logins (server, tenant, expiry) without tokens.
    Status,
    /// Rotate a Wyrd refresh token and print the new access and refresh tokens.
    Refresh(refresh::RefreshArgs),
    /// Manage trusted OIDC issuers (add, list, rm).
    #[command(subcommand)]
    TrustedIssuer(trusted_issuer::TrustedIssuerCommand),
    /// Manage workload bindings (add, list, rm).
    #[command(subcommand)]
    WorkloadBinding(workload_binding::WorkloadBindingCommand),
}

/// Run one `wyrd auth` subcommand by delegating to its owning module.
///
/// # Errors
/// Returns whatever the selected subcommand returns: invalid arguments,
/// client-construction failures, IO failures, or the server's stable Wyrd error.
pub async fn dispatch(command: AuthCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        AuthCommand::IssueKey(args) => issue_key::dispatch(args).await,
        AuthCommand::GrantRole(args) => grant_role::dispatch(args).await,
        AuthCommand::Login(args) => login::login(args).await,
        AuthCommand::Logout(args) => login::logout(args).await,
        AuthCommand::Status => login::status().await,
        AuthCommand::Refresh(args) => refresh::dispatch(args).await,
        AuthCommand::TrustedIssuer(cmd) => trusted_issuer::dispatch(cmd).await,
        AuthCommand::WorkloadBinding(cmd) => workload_binding::dispatch(cmd).await,
    }
}

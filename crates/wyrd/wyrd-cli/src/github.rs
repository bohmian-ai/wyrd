//! Tenant GitHub connection from the command line.
//!
//! Every verb is one call through `wyrd_client::GithubConnection`. `connect`
//! prints GitHub's install URL, which the same principal opens in a browser
//! signed in to Wyrd; responses are the server's redacted views, printed as
//! JSON.

use std::process::ExitCode;

use clap::Subcommand;

use crate::card::print_json;
use crate::error::WyrdCliError;
use crate::principal::credential::TenantEndpoint;

/// GitHub connection verbs inside one tenant.
#[derive(Debug, Subcommand)]
pub enum GithubCommand {
    /// Begin a connection and print GitHub's install URL and its expiry.
    Connect(TenantEndpoint),
    /// Read the tenant's redacted connection.
    Status(TenantEndpoint),
    /// Fence the tenant's GitHub work and release its installation.
    Disconnect(TenantEndpoint),
}

/// Dispatch one GitHub connection verb.
///
/// # Errors
/// Returns client-construction errors and the server's stable Wyrd error
/// when the caller is unauthorized, no App is configured, or the request is
/// refused.
pub async fn dispatch(command: GithubCommand) -> Result<ExitCode, WyrdCliError> {
    let printed = match command {
        GithubCommand::Connect(endpoint) => print_json(
            &crate::client::github(endpoint.server.as_str())
                .await?
                .begin()
                .await?,
        ),
        GithubCommand::Status(endpoint) => print_json(
            &crate::client::github(endpoint.server.as_str())
                .await?
                .get()
                .await?,
        ),
        GithubCommand::Disconnect(endpoint) => print_json(
            &crate::client::github(endpoint.server.as_str())
                .await?
                .disconnect()
                .await?,
        ),
    };
    printed.map(|()| ExitCode::SUCCESS)
}

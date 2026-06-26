pub mod login;
pub mod refresh;

use std::process::ExitCode;

use clap::Subcommand;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Initiate an OIDC login flow and exchange the callback code for a Wyrd access token.
    Login(login::LoginArgs),
    /// Rotate a Wyrd refresh token and print the new access and refresh tokens.
    Refresh(refresh::RefreshArgs),
}

pub async fn dispatch(command: AuthCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        AuthCommand::Login(args) => login::dispatch(args).await,
        AuthCommand::Refresh(args) => refresh::dispatch(args).await,
    }
}

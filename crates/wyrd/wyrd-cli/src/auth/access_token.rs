use std::process::ExitCode;

use clap::Args;

use crate::error::WyrdCliError;

/// Arguments for `wyrd auth access-token`.
#[derive(Debug, Args)]
pub struct AccessTokenArgs {
    /// Wyrd server base URL. The credential resolves from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or the saved
    /// login), never from an argument.
    #[arg(long, env = "WYRD_SERVER_URL")]
    pub server: Option<String>,
}

/// Print the ambient client's current access token followed by a newline.
///
/// Builds the client exactly as every other command does and asks it for its
/// token, which the client exchanges or renews as needed, so the output is
/// usable by `apiKeyHelper`-style tools and shells. Nothing else is written to
/// stdout, and the token is never logged.
///
/// # Errors
/// Returns [`WyrdCliError::NoCredentials`] when no credential resolves, a
/// client-construction error for a rejected endpoint, and
/// [`WyrdCliError::Server`] when the token exchange is refused or unreachable.
pub async fn dispatch(args: AccessTokenArgs) -> Result<ExitCode, WyrdCliError> {
    let client = crate::client::from_global(args.server.as_deref())?;
    let token = client.access_token().await?;
    println!("{}", token.expose());
    Ok(ExitCode::SUCCESS)
}

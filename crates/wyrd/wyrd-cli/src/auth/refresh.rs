use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_client::auth::TokenExchange;
use wyrd_client::transport::HttpConfig;
use wyrd_spec::auth::{SecretBearer, TokenRequest};

use crate::error::WyrdCliError;

#[derive(Debug, Args)]
pub struct RefreshArgs {
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Wyrd refresh token to rotate.
    #[arg(long, value_name = "TOKEN", env = "WYRD_REFRESH_TOKEN")]
    pub refresh_token: String,
}

/// Rotate a refresh token and print the replacement pair.
///
/// # Errors
/// Returns a client-construction error for a rejected endpoint and the server's
/// stable Wyrd error when the refresh token is unknown, expired, or already
/// rotated.
pub async fn dispatch(args: RefreshArgs) -> Result<ExitCode, WyrdCliError> {
    let token = TokenExchange::new(args.server.as_str(), HttpConfig::default().timeout_ms)
        .map_err(crate::client::map_client_error)?
        .exchange(&TokenRequest::RefreshToken {
            refresh_token: SecretBearer::new(args.refresh_token),
        })
        .await
        .map_err(|error| WyrdCliError::Server {
            source: error.into_wyrd(),
        })?;

    super::login::print_tokens(&token);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::RefreshArgs;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: RefreshArgs,
    }

    #[test]
    fn refresh_requires_server_and_token() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "--server",
            "https://acme.wyrd.cloud",
            "--refresh-token",
            "rt_test",
        ]);
        assert!(parsed.is_ok());
    }

    #[test]
    fn refresh_rejects_missing_token() {
        let parsed = Cli::try_parse_from(["wyrd", "--server", "https://acme.wyrd.cloud"]);
        assert!(parsed.is_err());
    }
}

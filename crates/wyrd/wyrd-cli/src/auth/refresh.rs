use std::process::ExitCode;

use clap::Args;
use secrecy::{ExposeSecret, SecretString};
use url::Url;
use wyrd_client::auth::TokenExchange;
use wyrd_client::transport::HttpConfig;
use wyrd_spec::auth::{SecretBearer, TokenRequest};

use crate::error::WyrdCliError;

/// Environment variable carrying the refresh token to rotate.
///
/// The only source: a refresh token is a live credential, so it never enters
/// argv, shell history, or the derived `Debug` of parsed arguments.
const REFRESH_TOKEN_ENV: &str = "WYRD_REFRESH_TOKEN";

/// Arguments for `wyrd auth refresh`: the server whose refresh token to rotate.
#[derive(Debug, Args)]
pub struct RefreshArgs {
    /// Wyrd server base URL. The refresh token is read from
    /// `WYRD_REFRESH_TOKEN`.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Rotate the environment's refresh token and print the replacement pair.
///
/// # Errors
/// Returns [`WyrdCliError::NoRefreshToken`] when `WYRD_REFRESH_TOKEN` is unset
/// or empty, a client-construction error for a rejected endpoint, and the
/// server's stable Wyrd error when the refresh token is unknown, expired, or
/// already rotated.
pub async fn dispatch(args: RefreshArgs) -> Result<ExitCode, WyrdCliError> {
    let refresh_token = std::env::var(REFRESH_TOKEN_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(SecretString::from)
        .ok_or(WyrdCliError::NoRefreshToken)?;
    let token = TokenExchange::new(args.server.as_str(), HttpConfig::default().timeout_ms)
        .map_err(crate::client::map_client_error)?
        .exchange(&TokenRequest::RefreshToken {
            refresh_token: SecretBearer::new(refresh_token.expose_secret().to_owned()),
        })
        .await
        .map_err(|error| WyrdCliError::Server {
            source: error.into_wyrd(),
        })?;

    super::login::print_tokens(&token);
    Ok(ExitCode::SUCCESS)
}

/// Argument parsing for `wyrd auth refresh`.
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::RefreshArgs;

    /// Bare wrapper so the arguments parse without the binary's tree.
    #[derive(Parser)]
    struct Cli {
        /// The arguments under test.
        #[command(flatten)]
        args: RefreshArgs,
    }

    /// The server is the only argument refresh needs.
    #[test]
    fn refresh_requires_only_the_server() {
        let parsed = Cli::try_parse_from(["wyrd", "--server", "https://acme.wyrd.cloud"]);
        assert!(parsed.is_ok());
    }

    /// The refresh token is never an argument, and a refused one is not echoed.
    #[test]
    fn refresh_refuses_a_refresh_token_argument() {
        let secret = "refresh-token-sentinel";
        let refused = Cli::try_parse_from([
            "wyrd",
            "--server",
            "https://acme.wyrd.cloud",
            "--refresh-token",
            secret,
        ])
        .err()
        .expect("--refresh-token is not accepted");
        assert!(!refused.to_string().contains(secret));
    }
}

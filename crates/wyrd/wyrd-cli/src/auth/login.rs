use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_client::auth::TokenExchange;
use wyrd_client::transport::HttpConfig;
use wyrd_spec::auth::{SecretBearer, TokenRequest, TokenResponse};

use crate::error::WyrdCliError;

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Wyrd server base URL (e.g. `https://acme.wyrd.cloud`).
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Trusted OIDC issuer URL registered with this Wyrd tenant.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
}

/// Walk an operator through an interactive OIDC login and print the tokens.
///
/// # Errors
/// Returns a client-construction error for a rejected endpoint, an IO error when
/// the pasted callback cannot be read, [`WyrdCliError::InvalidArgument`] when it
/// carries no code and state, and the server's stable Wyrd error when the issuer
/// is untrusted or the code is rejected.
pub async fn dispatch(args: LoginArgs) -> Result<ExitCode, WyrdCliError> {
    let exchange = TokenExchange::new(args.server.as_str(), HttpConfig::default().timeout_ms)
        .map_err(crate::client::map_client_error)?;

    let init = exchange
        .begin_login(&args.issuer)
        .await
        .map_err(|error| WyrdCliError::Server {
            source: error.into_wyrd(),
        })?;

    println!("Open this URL in your browser:");
    println!("{}", init.authorization_url);
    println!();
    println!("After authenticating, paste the full callback URL (or `code=<>&state=<>`):");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|source| WyrdCliError::Io { source })?;
    let input = input.trim().to_owned();

    let (code, state) = parse_callback_input(&input)?;

    let token = exchange
        .exchange(&TokenRequest::AuthorizationCode {
            code: SecretBearer::new(code),
            state,
        })
        .await
        .map_err(|error| WyrdCliError::Server {
            source: error.into_wyrd(),
        })?;

    print_tokens(&token);
    Ok(ExitCode::SUCCESS)
}

/// Print an issued token pair to the operator terminal.
///
/// The one place either token exists outside the server; neither is written to a
/// file or a log by the CLI.
pub(super) fn print_tokens(token: &TokenResponse) {
    println!("access_token:  {}", token.access_token.expose());
    if let Some(refresh_token) = &token.refresh_token {
        println!("refresh_token: {}", refresh_token.expose());
    }
    println!("expires_at:    {}", token.expires_at);
}

fn parse_callback_input(input: &str) -> Result<(String, String), WyrdCliError> {
    if let Ok(url) = Url::parse(input) {
        let code = url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.into_owned());
        let state = url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned());
        if let (Some(c), Some(s)) = (code, state) {
            return Ok((c, s));
        }
    }

    let pairs: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(input.as_bytes()).collect();
    let code = pairs.get("code").map(|v| v.as_ref().to_owned());
    let state = pairs.get("state").map(|v| v.as_ref().to_owned());
    match (code, state) {
        (Some(c), Some(s)) => Ok((c, s)),
        _ => Err(WyrdCliError::InvalidArgument {
            field: "callback".to_owned(),
            value: input.to_owned(),
            expected: "the full callback URL, or a `code=<>&state=<>` query string".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_callback_input;

    #[test]
    fn parses_full_callback_url() {
        let url = "http://localhost:8080/auth/callback?code=abc123&state=xyz789";
        let (code, state) = parse_callback_input(url).expect("parses");
        assert_eq!(code, "abc123");
        assert_eq!(state, "xyz789");
    }

    #[test]
    fn parses_raw_query_string() {
        let qs = "code=abc123&state=xyz789";
        let (code, state) = parse_callback_input(qs).expect("parses");
        assert_eq!(code, "abc123");
        assert_eq!(state, "xyz789");
    }

    #[test]
    fn rejects_missing_code() {
        let url = "http://localhost:8080/auth/callback?state=xyz789";
        assert!(parse_callback_input(url).is_err());
    }
}

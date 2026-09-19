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

/// Recover the authorization code and state from what the operator pasted.
///
/// The IdP redirects to a callback URL the CLI cannot listen on, so the operator
/// carries the result back by hand. Accepts either the whole URL or just its
/// query string, and reads the two parameters the code exchange needs.
///
/// The pasted text is a live credential: it carries a single-use authorization
/// code that stays redeemable until used or expired. It is therefore never
/// placed in the returned error, which the CLI prints to stderr twice — once as
/// collectable JSON — where it would outlive the login in scrollback and CI
/// logs.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when either `code` or `state` is
/// absent from both readings, naming which parameter was missing and nothing
/// else about the input.
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
        (code, state) => Err(WyrdCliError::InvalidArgument {
            field: "callback".to_owned(),
            value: match (code.is_some(), state.is_some()) {
                (false, true) => "<missing code>".to_owned(),
                (true, false) => "<missing state>".to_owned(),
                _ => "<missing code and state>".to_owned(),
            },
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

    /// The refusal names the missing parameter and never echoes the paste.
    ///
    /// A pasted callback carries a live single-use authorization code, and the
    /// CLI prints this error to stderr twice, once as collectable JSON. Echoing
    /// the input would put that code in scrollback and CI logs, so the rendered
    /// error must contain neither the value nor the `code=` that precedes it.
    #[test]
    fn the_refusal_does_not_echo_the_pasted_callback() {
        let error = parse_callback_input("code=super-secret-code")
            .expect_err("a callback with no state is refused");
        let rendered = error.to_string();
        assert!(
            !rendered.contains("super-secret-code"),
            "the authorization code leaked into the error: {rendered}"
        );
        assert!(
            !rendered.contains("code=super"),
            "the raw paste leaked into the error: {rendered}"
        );
        assert!(
            rendered.contains("<missing state>"),
            "the error names which parameter was absent: {rendered}"
        );
    }
}

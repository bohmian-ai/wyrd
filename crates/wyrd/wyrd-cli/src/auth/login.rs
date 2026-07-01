use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_spec::auth::{LoginInitResponse, SecretBearer, TokenRequest, TokenResponse};

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

pub async fn dispatch(args: LoginArgs) -> Result<ExitCode, WyrdCliError> {
    let client = reqwest::Client::new();

    let login_url = args
        .server
        .join("/auth/login")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let init: LoginInitResponse = client
        .get(login_url)
        .query(&[("issuer", &args.issuer)])
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?
        .error_for_status()
        .map_err(|source| WyrdCliError::Http { source })?
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

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

    let token_url = args
        .server
        .join("/auth/token")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let body = serde_json::to_string(&TokenRequest::AuthorizationCode {
        code: SecretBearer::new(code),
        state,
    })
    .expect("TokenRequest serializes");

    let resp = client
        .post(token_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::AuthFailed { status, detail });
    }

    let token: TokenResponse = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;
    println!("access_token:  {}", token.access_token.expose());
    if let Some(refresh_token) = &token.refresh_token {
        println!("refresh_token: {}", refresh_token.expose());
    }
    println!("expires_at:    {}", token.expires_at);
    Ok(ExitCode::SUCCESS)
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
        _ => Err(WyrdCliError::AuthFailed {
            status: 0,
            detail: "paste the full callback URL or `code=<>&state=<>` query string".to_owned(),
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

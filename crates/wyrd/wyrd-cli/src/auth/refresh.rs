use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_spec::auth::{SecretBearer, TokenRequest, TokenResponse};

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

pub async fn dispatch(args: RefreshArgs) -> Result<ExitCode, WyrdCliError> {
    let token_url = args
        .server
        .join("/auth/token")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let body = serde_json::to_string(&TokenRequest::RefreshToken {
        refresh_token: SecretBearer::new(args.refresh_token),
    })
    .expect("TokenRequest serializes");

    let resp = reqwest::Client::new()
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

    let token: TokenResponse = resp.json().await.map_err(|source| WyrdCliError::Http { source })?;
    println!("access_token:  {}", token.access_token.expose());
    println!("refresh_token: {}", token.refresh_token.expose());
    println!("expires_at:    {}", token.expires_at);
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

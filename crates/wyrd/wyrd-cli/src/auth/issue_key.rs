use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse};
use wyrd_spec::reference::CardRef;

use crate::error::WyrdCliError;

#[derive(Debug, Args)]
pub struct IssueKeyArgs {
    /// Card kind (e.g. service, agent).
    #[arg(long, value_name = "KIND")]
    pub kind: String,
    /// Card name.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Card version (e.g. 1.0.0).
    #[arg(long, value_name = "VERSION")]
    pub version: String,
    /// Card space.
    #[arg(long, value_name = "SPACE")]
    pub space: String,
    /// Optional label stored with the key row.
    #[arg(long, value_name = "TEXT")]
    pub label: Option<String>,
    /// Optional TTL override in seconds.
    #[arg(long, value_name = "SECS")]
    pub expires_in_seconds: Option<u32>,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

pub async fn dispatch(args: IssueKeyArgs) -> Result<ExitCode, WyrdCliError> {
    let card_ref_str = format!(
        "{}/{}/{}@{}",
        args.space, args.kind, args.name, args.version
    );
    let card_ref: CardRef = card_ref_str
        .parse()
        .map_err(|e| WyrdCliError::IssueKeyFailed {
            status: 400,
            detail: format!("invalid card ref: {e}"),
        })?;

    let issue_url = args
        .server
        .join("/auth/issue-key")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let body = serde_json::to_string(&IssueKeyRequest {
        card_ref,
        label: args.label,
        expires_in_seconds: args.expires_in_seconds,
    })
    .expect("IssueKeyRequest serializes");

    let resp = reqwest::Client::new()
        .post(issue_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", args.token),
        )
        .body(body)
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::IssueKeyFailed { status, detail });
    }

    let response: IssueKeyResponse = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    println!("key_id:     {}", response.key_id);
    println!("key:        {}", response.key.expose());
    println!("prefix:     {}", response.prefix);
    println!(
        "card_ref:   {}/{}/{}@{}",
        response.card_ref.space,
        response.card_ref.kind.wire_name(),
        response.card_ref.name,
        response.card_ref.version,
    );
    println!("created_at: {}", response.created_at);
    println!("expires_at: {}", response.expires_at);

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::IssueKeyArgs;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(flatten)]
        args: IssueKeyArgs,
    }

    #[test]
    fn issue_key_parses_required_args() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "--kind",
            "service",
            "--name",
            "my-svc",
            "--version",
            "1.0.0",
            "--space",
            "default",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    #[test]
    fn issue_key_requires_kind() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "--name",
            "my-svc",
            "--version",
            "1.0.0",
            "--space",
            "default",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --kind");
    }

    #[test]
    fn issue_key_accepts_optional_label_and_ttl() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "--kind",
            "agent",
            "--name",
            "my-agent",
            "--version",
            "2.3.1",
            "--space",
            "prod",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
            "--label",
            "ci-key",
            "--expires-in-seconds",
            "3600",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        let cli = parsed.unwrap();
        assert_eq!(cli.args.label.as_deref(), Some("ci-key"));
        assert_eq!(cli.args.expires_in_seconds, Some(3600));
    }
}

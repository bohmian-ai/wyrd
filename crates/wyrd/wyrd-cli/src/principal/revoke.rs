use std::process::ExitCode;

use clap::Args;
use url::Url;
use wyrd_spec::auth::{PrincipalKindTag, RevokePrincipalRequest};

use crate::error::WyrdCliError;

/// CLI mirror of [`PrincipalKindTag`].
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum PrincipalKindCli {
    User,
    Service,
    Agent,
}

impl From<PrincipalKindCli> for PrincipalKindTag {
    fn from(value: PrincipalKindCli) -> Self {
        match value {
            PrincipalKindCli::User => Self::User,
            PrincipalKindCli::Service => Self::Service,
            PrincipalKindCli::Agent => Self::Agent,
        }
    }
}

#[derive(Debug, Args)]
pub struct RevokeArgs {
    /// Principal ID (UUID) to revoke.
    pub id: String,
    /// Principal kind (required — IDs are unique only within a kind table).
    #[arg(long, value_name = "KIND")]
    pub kind: PrincipalKindCli,
    /// Audit reason for the revocation.
    #[arg(long, value_name = "TEXT")]
    pub reason: String,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

pub async fn dispatch(args: RevokeArgs) -> Result<ExitCode, WyrdCliError> {
    let revoke_url = args
        .server
        .join(&format!("/v1/principals/{}/revoke", args.id))
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let body = serde_json::to_string(&RevokePrincipalRequest {
        principal_kind: PrincipalKindTag::from(args.kind),
        reason: args.reason,
    })
    .expect("RevokePrincipalRequest serializes");

    let resp = reqwest::Client::new()
        .post(revoke_url)
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
        return Err(WyrdCliError::RevokeFailed { status, detail });
    }

    let result: serde_json::Value = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_default()
    );
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::RevokeArgs;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(flatten)]
        args: RevokeArgs,
    }

    #[test]
    fn revoke_requires_kind() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "p-id-123",
            "--reason",
            "test",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "revoke must require --kind");
    }

    #[test]
    fn revoke_parses_all_kinds() {
        for kind in ["user", "service", "agent"] {
            let parsed = Cli::try_parse_from([
                "wyrd",
                "p-id-123",
                "--kind",
                kind,
                "--reason",
                "test",
                "--server",
                "https://acme.wyrd.cloud",
                "--token",
                "tok",
            ]);
            assert!(parsed.is_ok(), "kind={kind} failed: {parsed:?}");
        }
    }
}

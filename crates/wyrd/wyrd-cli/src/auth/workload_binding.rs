use std::process::ExitCode;

use clap::{Args, Subcommand};
use url::Url;
use wyrd_spec::auth::{CreateWorkloadBindingRequest, IssuerUrl, WorkloadBindingView};
use wyrd_spec::reference::CardRef;

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum WorkloadBindingCommand {
    /// Create a workload binding (POST /admin/workload-bindings).
    Add(AddArgs),
    /// List workload bindings (GET /admin/workload-bindings).
    List(ListArgs),
    /// Remove a workload binding (DELETE /admin/workload-bindings).
    Rm(RmArgs),
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Trusted issuer URL the binding belongs to.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Token subject the binding matches.
    #[arg(long, value_name = "SUBJECT")]
    pub subject: String,
    /// Optional audience constraint.
    #[arg(long, value_name = "AUD")]
    pub audience: Option<String>,
    /// Card the bound workload acts as (space/kind/name@version[#uid]).
    #[arg(long, value_name = "CARD_REF")]
    pub card: String,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Filter by issuer URL.
    #[arg(long, value_name = "URL")]
    pub issuer: Option<String>,
    /// Filter by subject.
    #[arg(long, value_name = "SUBJECT")]
    pub subject: Option<String>,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

#[derive(Debug, Args)]
pub struct RmArgs {
    /// Trusted issuer URL of the binding to remove.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Subject of the binding to remove.
    #[arg(long, value_name = "SUBJECT")]
    pub subject: String,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

pub async fn dispatch(command: WorkloadBindingCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        WorkloadBindingCommand::Add(args) => add(args).await,
        WorkloadBindingCommand::List(args) => list(args).await,
        WorkloadBindingCommand::Rm(args) => rm(args).await,
    }
}

async fn add(args: AddArgs) -> Result<ExitCode, WyrdCliError> {
    // Validate card ref locally before any HTTP call; fail 400-class on invalid input.
    let card_ref: CardRef = args.card.parse().map_err(|e| WyrdCliError::AdminFailed {
        status: 400,
        detail: format!("invalid --card: {e}"),
    })?;

    let issuer: IssuerUrl = args.issuer.parse().map_err(|e| WyrdCliError::AdminFailed {
        status: 400,
        detail: format!("invalid --issuer URL: {e}"),
    })?;

    let url = args
        .server
        .join("/admin/workload-bindings")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let body = serde_json::to_string(&CreateWorkloadBindingRequest {
        issuer,
        subject: args.subject,
        audience: args.audience,
        card_ref,
    })
    .expect("CreateWorkloadBindingRequest serializes");

    let resp = reqwest::Client::new()
        .post(url)
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
        return Err(WyrdCliError::AdminFailed { status, detail });
    }

    let view: WorkloadBindingView = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    println!("issuer:  {}", view.issuer);
    println!("subject: {}", view.subject);
    if let Some(ref aud) = view.audience {
        println!("audience:{aud}");
    }
    println!("card_ref:{}", view.card_ref);

    Ok(ExitCode::SUCCESS)
}

async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    let url = args
        .server
        .join("/admin/workload-bindings")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let mut builder = reqwest::Client::new().get(url).header(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", args.token),
    );

    if let Some(ref issuer) = args.issuer {
        builder = builder.query(&[("issuer", issuer.as_str())]);
    }
    if let Some(ref subject) = args.subject {
        builder = builder.query(&[("subject", subject.as_str())]);
    }

    let resp = builder
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::AdminFailed { status, detail });
    }

    let views: Vec<WorkloadBindingView> = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    for view in &views {
        println!("{} / {} → {}", view.issuer, view.subject, view.card_ref);
    }

    Ok(ExitCode::SUCCESS)
}

async fn rm(args: RmArgs) -> Result<ExitCode, WyrdCliError> {
    let url = args
        .server
        .join("/admin/workload-bindings")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let resp = reqwest::Client::new()
        .delete(url)
        .query(&[
            ("issuer", args.issuer.as_str()),
            ("subject", args.subject.as_str()),
        ])
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", args.token),
        )
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::AdminFailed { status, detail });
    }

    println!("deleted: {} / {}", args.issuer, args.subject);
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::WorkloadBindingCommand;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(subcommand)]
        command: WorkloadBindingCommand,
    }

    #[test]
    fn add_parses_required_args() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--card",
            "prod/service/my-svc@1.0.0",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    #[test]
    fn add_requires_card() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --card");
    }

    #[test]
    fn add_requires_subject() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--card",
            "prod/service/my-svc@1.0.0",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --subject");
    }

    #[test]
    fn add_accepts_optional_audience() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--audience",
            "myapp",
            "--card",
            "prod/service/my-svc@1.0.0",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            WorkloadBindingCommand::Add(args) => {
                assert_eq!(args.audience.as_deref(), Some("myapp"));
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn invalid_card_ref_fails_parse_before_http() {
        // Clap accepts any string for --card; the rejection is at dispatch time,
        // before any HTTP call. Confirm that the parse of the card ref itself fails.
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--card",
            "not-a-valid-card-ref",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        // Clap itself accepts any string; dispatch() validates and rejects before HTTP.
        assert!(
            parsed.is_ok(),
            "clap parses string args without card validation"
        );
        match parsed.unwrap().command {
            WorkloadBindingCommand::Add(args) => {
                let result: Result<wyrd_spec::reference::CardRef, _> = args.card.parse();
                assert!(result.is_err(), "invalid card ref must fail FromStr parse");
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn list_parses_with_no_filters() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "list",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            WorkloadBindingCommand::List(args) => {
                assert!(args.issuer.is_none());
                assert!(args.subject.is_none());
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn list_accepts_optional_issuer_filter() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "list",
            "--issuer",
            "https://idp.example.com",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            WorkloadBindingCommand::List(args) => {
                assert_eq!(args.issuer.as_deref(), Some("https://idp.example.com"));
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn list_requires_server() {
        let parsed = Cli::try_parse_from(["wyrd", "list", "--token", "tok"]);
        assert!(parsed.is_err(), "must require --server when env is absent");
    }

    #[test]
    fn rm_parses_required_args() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "rm",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    #[test]
    fn rm_requires_subject() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "rm",
            "--issuer",
            "https://idp.example.com",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --subject");
    }

    #[test]
    fn rm_requires_issuer() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "rm",
            "--subject",
            "system:ci-runner",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --issuer");
    }
}

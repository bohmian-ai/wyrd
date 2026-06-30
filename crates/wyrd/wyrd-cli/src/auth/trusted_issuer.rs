use std::process::ExitCode;

use clap::{Args, Subcommand};
use url::Url;
use wyrd_spec::auth::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, IssuerUrl,
    PrincipalKindPayload, TrustedIssuerView,
};

use crate::error::WyrdCliError;

#[derive(Debug, Subcommand)]
pub enum TrustedIssuerCommand {
    /// Register a trusted OIDC issuer (POST /admin/trusted-issuers).
    Add(AddArgs),
    /// List trusted OIDC issuers (GET /admin/trusted-issuers).
    List(ListArgs),
    /// Remove a trusted OIDC issuer (DELETE /admin/trusted-issuers?issuer=&cascade=).
    Rm(RmArgs),
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Trusted OIDC issuer URL.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Expected audience for tokens from this issuer.
    #[arg(long, value_name = "AUD")]
    pub expected_audience: String,
    /// Client ID Wyrd presents to the issuer.
    #[arg(long, value_name = "ID")]
    pub client_id: String,
    /// How Wyrd authenticates to the issuer (SecretBasic, SecretPost, PrivateKeyJwt, Public).
    #[arg(long, value_name = "METHOD")]
    pub client_auth: String,
    /// Client secret (required for SecretBasic and SecretPost).
    #[arg(long, value_name = "SECRET")]
    pub client_secret: Option<String>,
    /// Claim path that yields the principal subject.
    #[arg(long, value_name = "CLAIM")]
    pub claim_subject: String,
    /// Whether tokens represent humans or machine workloads (Human, Workload).
    #[arg(long, value_name = "KIND")]
    pub principal_kind: String,
    /// JWKS key-cache TTL override in seconds.
    #[arg(long, value_name = "SECS")]
    pub jwks_ttl_secs: Option<u64>,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

#[derive(Debug, Args)]
pub struct RmArgs {
    /// Trusted issuer URL to remove.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Remove even if live workload bindings exist.
    #[arg(long)]
    pub cascade: bool,
    /// Wyrd server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
    /// Bearer access token with admin privileges.
    #[arg(long, value_name = "TOKEN", env = "WYRD_ACCESS_TOKEN")]
    pub token: String,
}

pub async fn dispatch(command: TrustedIssuerCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        TrustedIssuerCommand::Add(args) => add(args).await,
        TrustedIssuerCommand::List(args) => list(args).await,
        TrustedIssuerCommand::Rm(args) => rm(args).await,
    }
}

async fn add(args: AddArgs) -> Result<ExitCode, WyrdCliError> {
    let issuer: IssuerUrl = args.issuer.parse().map_err(|e| WyrdCliError::AdminFailed {
        status: 400,
        detail: format!("invalid --issuer URL: {e}"),
    })?;
    let client_auth = parse_client_auth(&args.client_auth)?;
    let principal_kind = parse_principal_kind(&args.principal_kind)?;

    let url = args
        .server
        .join("/v1/admin/trusted-issuers")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let body = serde_json::to_string(&CreateTrustedIssuerRequest {
        issuer,
        expected_audience: args.expected_audience,
        client_id: args.client_id,
        client_auth,
        client_secret: args.client_secret,
        claim_mapping: ClaimMappingPayload {
            subject: args.claim_subject,
            email: None,
            groups: None,
        },
        group_role_map: Default::default(),
        default_roles: Default::default(),
        principal_kind,
        jwks_ttl_secs: args.jwks_ttl_secs,
    })
    .expect("CreateTrustedIssuerRequest serializes");

    let resp = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("x-wyrd-access-token", format!("Bearer {}", args.token))
        .body(body)
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::AdminFailed { status, detail });
    }

    let view: TrustedIssuerView = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    println!("issuer:            {}", view.issuer);
    println!("jwks_uri:          {}", view.jwks_uri);
    println!("expected_audience: {}", view.expected_audience);
    println!("client_id:         {}", view.client_id);
    println!("client_auth:       {}", view.client_auth);
    println!("principal_kind:    {}", view.principal_kind);
    println!("jwks_ttl_secs:     {}", view.jwks_ttl_secs);

    Ok(ExitCode::SUCCESS)
}

async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    let url = args
        .server
        .join("/v1/admin/trusted-issuers")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    let resp = reqwest::Client::new()
        .get(url)
        .header("x-wyrd-access-token", format!("Bearer {}", args.token))
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::AdminFailed { status, detail });
    }

    let views: Vec<TrustedIssuerView> = resp
        .json()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    for view in &views {
        println!("{} ({})", view.issuer, view.principal_kind);
    }

    Ok(ExitCode::SUCCESS)
}

async fn rm(args: RmArgs) -> Result<ExitCode, WyrdCliError> {
    let url = args
        .server
        .join("/v1/admin/trusted-issuers")
        .map_err(|source| WyrdCliError::UrlJoin { source })?;

    // The server addresses the issuer by query, not a path segment: the route is
    // the bare collection path and DeleteIssuerQuery reads ?issuer=&cascade=.
    let cascade = args.cascade.to_string();
    let resp = reqwest::Client::new()
        .delete(url)
        .query(&[
            ("issuer", args.issuer.as_str()),
            ("cascade", cascade.as_str()),
        ])
        .header("x-wyrd-access-token", format!("Bearer {}", args.token))
        .send()
        .await
        .map_err(|source| WyrdCliError::Http { source })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        return Err(WyrdCliError::AdminFailed { status, detail });
    }

    println!("deleted: {}", args.issuer);
    Ok(ExitCode::SUCCESS)
}

fn parse_client_auth(value: &str) -> Result<ClientAuthKind, WyrdCliError> {
    match value {
        "SecretBasic" => Ok(ClientAuthKind::SecretBasic),
        "SecretPost" => Ok(ClientAuthKind::SecretPost),
        "PrivateKeyJwt" => Ok(ClientAuthKind::PrivateKeyJwt),
        "Public" => Ok(ClientAuthKind::Public),
        other => Err(WyrdCliError::AdminFailed {
            status: 400,
            detail: format!(
                "unknown --client-auth {other:?}; expected SecretBasic, SecretPost, PrivateKeyJwt, or Public"
            ),
        }),
    }
}

fn parse_principal_kind(value: &str) -> Result<PrincipalKindPayload, WyrdCliError> {
    match value {
        "Human" => Ok(PrincipalKindPayload::Human),
        "Workload" => Ok(PrincipalKindPayload::Workload),
        other => Err(WyrdCliError::AdminFailed {
            status: 400,
            detail: format!("unknown --principal-kind {other:?}; expected Human or Workload"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::TrustedIssuerCommand;

    #[derive(Debug, Parser)]
    struct Cli {
        #[command(subcommand)]
        command: TrustedIssuerCommand,
    }

    #[test]
    fn add_parses_required_args() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--expected-audience",
            "wyrd",
            "--client-id",
            "myapp",
            "--client-auth",
            "SecretBasic",
            "--claim-subject",
            "sub",
            "--principal-kind",
            "Workload",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    #[test]
    fn add_requires_issuer() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--expected-audience",
            "wyrd",
            "--client-id",
            "myapp",
            "--client-auth",
            "SecretBasic",
            "--claim-subject",
            "sub",
            "--principal-kind",
            "Workload",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --issuer");
    }

    #[test]
    fn add_requires_principal_kind() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--expected-audience",
            "wyrd",
            "--client-id",
            "myapp",
            "--client-auth",
            "SecretBasic",
            "--claim-subject",
            "sub",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --principal-kind");
    }

    #[test]
    fn add_accepts_optional_client_secret_and_ttl() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--expected-audience",
            "wyrd",
            "--client-id",
            "myapp",
            "--client-auth",
            "SecretBasic",
            "--client-secret",
            "s3cr3t",
            "--claim-subject",
            "sub",
            "--principal-kind",
            "Workload",
            "--jwks-ttl-secs",
            "3600",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            TrustedIssuerCommand::Add(args) => {
                assert_eq!(args.client_secret.as_deref(), Some("s3cr3t"));
                assert_eq!(args.jwks_ttl_secs, Some(3600));
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn list_parses_required_args() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "list",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
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
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    #[test]
    fn rm_accepts_cascade_flag() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "rm",
            "--issuer",
            "https://idp.example.com",
            "--cascade",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            TrustedIssuerCommand::Rm(args) => assert!(args.cascade),
            _ => panic!("expected Rm"),
        }
    }

    #[test]
    fn rm_requires_issuer() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "rm",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_err(), "must require --issuer");
    }

    #[test]
    fn client_auth_parse_rejects_unknown() {
        use super::parse_client_auth;
        assert!(parse_client_auth("Unknown").is_err());
    }

    #[test]
    fn principal_kind_parse_rejects_unknown() {
        use super::parse_principal_kind;
        assert!(parse_principal_kind("Robot").is_err());
    }
}

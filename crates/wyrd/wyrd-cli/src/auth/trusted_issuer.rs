use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use url::Url;
use wyrd_spec::auth::{
    ClaimMappingPayload, ClientAuthKind, CreateTrustedIssuerRequest, IssuerTokenPolicy, IssuerUrl,
    SecretBearer, TrustedIssuerView,
};

use crate::error::WyrdCliError;

/// Collection path both the write and the read operations address.
const TRUSTED_ISSUERS_PATH: &str = "/v1/admin/trusted-issuers";

#[derive(Debug, Subcommand)]
pub enum TrustedIssuerCommand {
    /// Register a trusted OIDC issuer (POST /v1/admin/trusted-issuers).
    Add(Box<AddArgs>),
    /// List trusted OIDC issuers (GET /v1/admin/trusted-issuers).
    List(ListArgs),
    /// Remove a trusted OIDC issuer (DELETE /v1/admin/trusted-issuers?issuer=&cascade=).
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
    /// Client secret (required for SecretBasic and SecretPost). Prefer
    /// --client-secret-file or WYRD_ISSUER_CLIENT_SECRET to keep the secret out
    /// of shell history and the process argument list.
    #[arg(long, value_name = "SECRET", value_parser = secret_argument)]
    pub client_secret: Option<SecretString>,
    /// Read the client secret from a file (trailing newline trimmed). Mutually
    /// exclusive with --client-secret.
    #[arg(long, value_name = "PATH", conflicts_with = "client_secret")]
    pub client_secret_file: Option<PathBuf>,
    /// Claim path that yields the principal subject.
    #[arg(long, value_name = "CLAIM")]
    pub claim_subject: String,
    /// Optional claim path that yields the principal email.
    #[arg(long, value_name = "CLAIM")]
    pub claim_email: Option<String>,
    /// Optional claim path that yields the principal groups.
    #[arg(long, value_name = "CLAIM")]
    pub claim_groups: Option<String>,
    /// Role granted to every principal from this issuer. Repeatable.
    #[arg(long = "default-role", value_name = "ROLE")]
    pub default_roles: Vec<String>,
    /// Map an issuer group to a Wyrd role, as `group=role`. Repeatable; the same
    /// group may be given multiple times to grant multiple roles.
    #[arg(long = "group-role", value_name = "GROUP=ROLE")]
    pub group_roles: Vec<String>,
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
        TrustedIssuerCommand::Add(args) => add(*args).await,
        TrustedIssuerCommand::List(args) => list(args).await,
        TrustedIssuerCommand::Rm(args) => rm(args).await,
    }
}

/// Register a trusted OIDC issuer for the caller's tenant.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for a malformed issuer URL, client
/// auth method, principal kind, or group mapping; an IO error when the secret
/// file cannot be read; and the server's stable Wyrd error when the caller is
/// unauthorized or the issuer cannot be screened.
async fn add(args: AddArgs) -> Result<ExitCode, WyrdCliError> {
    let issuer: IssuerUrl = args
        .issuer
        .parse()
        .map_err(|error| invalid("issuer", &args.issuer, &format!("an issuer URL: {error}")))?;
    let client_auth = parse_client_auth(&args.client_auth)?;
    let principal_kind = parse_principal_kind(&args.principal_kind)?;
    let client_secret = resolve_client_secret(args.client_secret, args.client_secret_file)?
        .map(|secret| SecretBearer::new(secret.expose_secret().to_owned()));
    let group_role_map = parse_group_roles(&args.group_roles)?;

    let view: TrustedIssuerView = crate::client::client(args.server.as_str(), &args.token)?
        .request_json(
            Method::POST,
            TRUSTED_ISSUERS_PATH,
            Some(&CreateTrustedIssuerRequest {
                issuer,
                expected_audience: args.expected_audience,
                client_id: args.client_id,
                client_auth,
                client_secret,
                claim_mapping: ClaimMappingPayload {
                    subject: args.claim_subject,
                    email: args.claim_email,
                    groups: args.claim_groups,
                },
                group_role_map,
                default_roles: args.default_roles,
                principal_kind,
                jwks_ttl_secs: args.jwks_ttl_secs,
            }),
        )
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("issuer:            {}", view.issuer);
    println!("jwks_uri:          {}", view.jwks_uri);
    println!("expected_audience: {}", view.expected_audience);
    println!("client_id:         {}", view.client_id);
    println!("client_auth:       {}", view.client_auth);
    println!("principal_kind:    {}", view.principal_kind);
    println!("jwks_ttl_secs:     {}", view.jwks_ttl_secs);

    Ok(ExitCode::SUCCESS)
}

/// List the tenant's trusted OIDC issuers.
///
/// # Errors
/// Returns a client-construction error for a rejected endpoint and the server's
/// stable Wyrd error when the caller is unauthorized or the read fails.
async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    let views: Vec<TrustedIssuerView> = crate::client::client(args.server.as_str(), &args.token)?
        .request_json::<(), _>(Method::GET, TRUSTED_ISSUERS_PATH, None)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for view in &views {
        println!("{} ({})", view.issuer, view.principal_kind);
    }

    Ok(ExitCode::SUCCESS)
}

/// Remove one trusted OIDC issuer.
///
/// The server addresses the issuer by query, not a path segment: the route is
/// the bare collection path and `DeleteIssuerQuery` reads `?issuer=&cascade=`.
///
/// # Errors
/// Returns a client-construction error for a rejected endpoint and the server's
/// stable Wyrd error when the caller is unauthorized, the issuer is unknown, or
/// live workload bindings block an uncascaded removal.
async fn rm(args: RmArgs) -> Result<ExitCode, WyrdCliError> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("issuer", &args.issuer)
        .append_pair("cascade", if args.cascade { "true" } else { "false" })
        .finish();

    crate::client::client(args.server.as_str(), &args.token)?
        .request_json::<(), serde_json::Value>(
            Method::DELETE,
            &format!("{TRUSTED_ISSUERS_PATH}?{query}"),
            None,
        )
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("deleted: {}", args.issuer);
    Ok(ExitCode::SUCCESS)
}

/// Report one rejected argument.
fn invalid(field: &str, value: &str, expected: &str) -> WyrdCliError {
    WyrdCliError::InvalidArgument {
        field: field.to_owned(),
        value: value.to_owned(),
        expected: expected.to_owned(),
    }
}

/// Parse the client authentication method Wyrd presents to the issuer.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for an unknown method.
fn parse_client_auth(value: &str) -> Result<ClientAuthKind, WyrdCliError> {
    match value {
        "SecretBasic" => Ok(ClientAuthKind::SecretBasic),
        "SecretPost" => Ok(ClientAuthKind::SecretPost),
        "PrivateKeyJwt" => Ok(ClientAuthKind::PrivateKeyJwt),
        "Public" => Ok(ClientAuthKind::Public),
        other => Err(invalid(
            "client-auth",
            other,
            "SecretBasic, SecretPost, PrivateKeyJwt, or Public",
        )),
    }
}

/// Parse whether tokens from this issuer represent humans or workloads.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for an unknown kind.
fn parse_principal_kind(value: &str) -> Result<IssuerTokenPolicy, WyrdCliError> {
    match value {
        "Human" => Ok(IssuerTokenPolicy::Human),
        "Workload" => Ok(IssuerTokenPolicy::Workload),
        other => Err(invalid("principal-kind", other, "Human or Workload")),
    }
}

/// Hold a `--client-secret` argument value without it becoming debug-visible.
///
/// `AddArgs` derives `Debug`, and clap itself renders argument values in some
/// error paths, so the secret is wrapped the moment it leaves the command line
/// rather than after resolution. Infallible — every UTF-8 argument is a valid
/// secret — but clap requires the `Result` shape.
///
/// # Errors
/// Never returns an error; the `Result` satisfies clap's parser contract.
fn secret_argument(raw: &str) -> Result<SecretString, std::convert::Infallible> {
    Ok(SecretString::from(raw.to_owned()))
}

/// Resolve the client secret from the flag, a file, or the environment.
///
/// `--client-secret-file` and `--client-secret` are mutually exclusive at the
/// clap layer, so at most one of `inline`/`file` is set. The file path wins when
/// present; otherwise the inline flag; otherwise `WYRD_ISSUER_CLIENT_SECRET`.
/// Keeping the secret in a file or env var avoids leaking it into shell history
/// and the process argument list.
fn resolve_client_secret(
    inline: Option<SecretString>,
    file: Option<PathBuf>,
) -> Result<Option<SecretString>, WyrdCliError> {
    if let Some(path) = file {
        let raw = std::fs::read_to_string(&path).map_err(|source| WyrdCliError::Io { source })?;
        return Ok(Some(SecretString::from(
            raw.trim_end_matches(['\n', '\r']).to_owned(),
        )));
    }
    if let Some(secret) = inline {
        return Ok(Some(secret));
    }
    match std::env::var("WYRD_ISSUER_CLIENT_SECRET") {
        Ok(secret) => Ok(Some(SecretString::from(secret))),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(invalid(
            "WYRD_ISSUER_CLIENT_SECRET",
            "<non-UTF-8>",
            "valid UTF-8",
        )),
    }
}

/// Fold repeated `group=role` flags into the issuer group → Wyrd roles map.
///
/// A group may appear more than once to grant multiple roles; the roles
/// accumulate in flag order. Each entry must contain exactly one `=` with a
/// non-empty group and role.
fn parse_group_roles(entries: &[String]) -> Result<HashMap<String, Vec<String>>, WyrdCliError> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for entry in entries {
        let (group, role) = entry
            .split_once('=')
            .ok_or_else(|| invalid("group-role", entry, "group=role"))?;
        if group.is_empty() || role.is_empty() {
            return Err(invalid(
                "group-role",
                entry,
                "group=role with a non-empty group and role",
            ));
        }
        map.entry(group.to_owned())
            .or_default()
            .push(role.to_owned());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use secrecy::{ExposeSecret, SecretString};

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
                assert_eq!(
                    args.client_secret.as_ref().map(ExposeSecret::expose_secret),
                    Some("s3cr3t")
                );
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

    #[test]
    fn add_rejects_client_secret_and_file_together() {
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
            "SecretPost",
            "--client-secret",
            "s3cr3t",
            "--client-secret-file",
            "/tmp/secret",
            "--claim-subject",
            "sub",
            "--principal-kind",
            "Workload",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(
            parsed.is_err(),
            "--client-secret and --client-secret-file must conflict"
        );
    }

    #[test]
    fn add_accepts_role_group_and_claim_flags() {
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
            "Public",
            "--claim-subject",
            "sub",
            "--claim-email",
            "email",
            "--claim-groups",
            "groups",
            "--default-role",
            "reader",
            "--default-role",
            "writer",
            "--group-role",
            "dev=reader",
            "--group-role",
            "dev=writer",
            "--group-role",
            "ops=admin",
            "--principal-kind",
            "Human",
            "--server",
            "https://acme.wyrd.cloud",
            "--token",
            "tok",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            TrustedIssuerCommand::Add(args) => {
                assert_eq!(args.claim_email.as_deref(), Some("email"));
                assert_eq!(args.claim_groups.as_deref(), Some("groups"));
                assert_eq!(args.default_roles, vec!["reader", "writer"]);
                let map = super::parse_group_roles(&args.group_roles).expect("group roles parse");
                assert_eq!(map["dev"], vec!["reader", "writer"]);
                assert_eq!(map["ops"], vec!["admin"]);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn group_roles_reject_missing_equals() {
        assert!(super::parse_group_roles(&["devreader".to_owned()]).is_err());
        assert!(super::parse_group_roles(&["=reader".to_owned()]).is_err());
        assert!(super::parse_group_roles(&["dev=".to_owned()]).is_err());
    }

    #[test]
    fn resolve_client_secret_prefers_inline_then_reads_file() {
        use super::resolve_client_secret;

        assert_eq!(
            resolve_client_secret(Some(SecretString::from("inline".to_owned())), None)
                .expect("inline resolves")
                .as_ref()
                .map(ExposeSecret::expose_secret),
            Some("inline")
        );

        let dir = std::env::temp_dir();
        let path = dir.join(format!("wyrd-cli-secret-{}", std::process::id()));
        std::fs::write(&path, "file-secret\n").expect("write temp secret");
        let resolved =
            resolve_client_secret(None, Some(path.clone())).expect("file secret resolves");
        std::fs::remove_file(&path).ok();
        assert_eq!(
            resolved.as_ref().map(ExposeSecret::expose_secret),
            Some("file-secret")
        );
    }

    #[tokio::test]
    async fn add_posts_secret_and_role_flags_in_body() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/admin/trusted-issuers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": "https://idp.example.com",
                "jwks_uri": "https://idp.example.com/jwks",
                "expected_audience": "wyrd",
                "client_id": "myapp",
                "client_auth": "SecretPost",
                "principal_kind": "Workload",
                "jwks_ttl_secs": 3600,
                "claim_mapping": { "subject": "sub" },
                "group_role_map": { "dev": ["reader"] },
                "default_roles": ["reader"]
            })))
            .mount(&server)
            .await;

        let args = super::AddArgs {
            issuer: "https://idp.example.com".to_owned(),
            expected_audience: "wyrd".to_owned(),
            client_id: "myapp".to_owned(),
            client_auth: "SecretPost".to_owned(),
            client_secret: Some(SecretString::from("s3cr3t".to_owned())),
            client_secret_file: None,
            claim_subject: "sub".to_owned(),
            claim_email: None,
            claim_groups: None,
            default_roles: vec!["reader".to_owned()],
            group_roles: vec!["dev=reader".to_owned()],
            principal_kind: "Workload".to_owned(),
            jwks_ttl_secs: None,
            server: server.uri().parse().expect("mock uri parses"),
            token: "tok".to_owned(),
        };

        super::add(args).await.expect("add dispatch succeeds");

        let requests = server.received_requests().await.expect("requests recorded");
        assert_eq!(requests.len(), 1, "exactly one POST expected");
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("body is JSON");
        assert_eq!(body["client_secret"], "s3cr3t");
        assert_eq!(body["client_auth"], "SecretPost");
        assert_eq!(body["default_roles"], serde_json::json!(["reader"]));
        assert_eq!(body["group_role_map"]["dev"], serde_json::json!(["reader"]));
        // The admin token is presented on the wire.
        let auth = requests[0]
            .headers
            .get("x-wyrd-access-token")
            .expect("access token header present");
        assert_eq!(auth, "Bearer tok");
    }
}

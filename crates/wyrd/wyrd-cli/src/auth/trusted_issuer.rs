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

/// Subcommands of `wyrd auth trusted-issuer`.
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
/// Arguments for `wyrd auth trusted-issuer add`.
///
/// The client secret is accepted only by file or environment, never inline, so
/// it cannot reach shell history, the process argument list, or the derived
/// `Debug` of parsed arguments.
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
    /// Read the client secret (required for SecretBasic and SecretPost) from a
    /// file, trailing newline trimmed. Without it the secret is read from
    /// `WYRD_ISSUER_CLIENT_SECRET`.
    #[arg(long, value_name = "PATH")]
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
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Arguments for `wyrd auth trusted-issuer list`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Arguments for `wyrd auth trusted-issuer rm`.
#[derive(Debug, Args)]
pub struct RmArgs {
    /// Trusted issuer URL to remove.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Remove even if live workload bindings exist.
    #[arg(long)]
    pub cascade: bool,
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Run one `wyrd auth trusted-issuer` subcommand.
///
/// # Errors
/// Returns the selected operation's error: an invalid argument, an IO failure
/// reading the client secret, or the server's stable Wyrd error.
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
    let client_secret = resolve_client_secret(args.client_secret_file)?
        .map(|secret| SecretBearer::new(secret.expose_secret().to_owned()));
    let group_role_map = parse_group_roles(&args.group_roles)?;

    let view: TrustedIssuerView = crate::client::from_global(Some(args.server.as_str()))?
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
    let views: Vec<TrustedIssuerView> = crate::client::from_global(Some(args.server.as_str()))?
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

    crate::client::from_global(Some(args.server.as_str()))?
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

/// Resolve the client secret from a file or the environment.
///
/// The file path wins when present; otherwise `WYRD_ISSUER_CLIENT_SECRET`.
/// Neither source puts the secret in shell history or the process argument
/// list.
///
/// # Errors
/// Returns [`WyrdCliError::Io`] when `--client-secret-file` cannot be read.
fn resolve_client_secret(file: Option<PathBuf>) -> Result<Option<SecretString>, WyrdCliError> {
    if let Some(path) = file {
        let raw = std::fs::read_to_string(&path).map_err(|source| WyrdCliError::Io { source })?;
        return Ok(Some(SecretString::from(
            raw.trim_end_matches(['\n', '\r']).to_owned(),
        )));
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
///
/// # Errors
/// Returns a validation error when an entry has no `=`, or an empty group or
/// role.
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
    use secrecy::ExposeSecret;

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
        ]);
        assert!(parsed.is_err(), "must require --principal-kind");
    }

    #[test]
    fn add_accepts_optional_client_secret_file_and_ttl() {
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
            "--client-secret-file",
            "/run/secrets/issuer",
            "--claim-subject",
            "sub",
            "--principal-kind",
            "Workload",
            "--jwks-ttl-secs",
            "3600",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            TrustedIssuerCommand::Add(args) => {
                assert_eq!(
                    args.client_secret_file.as_deref(),
                    Some(std::path::Path::new("/run/secrets/issuer"))
                );
                assert_eq!(args.jwks_ttl_secs, Some(3600));
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn list_parses_required_args() {
        let parsed = Cli::try_parse_from(["wyrd", "list", "--server", "https://acme.wyrd.cloud"]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    #[test]
    fn list_requires_server() {
        let parsed = Cli::try_parse_from(["wyrd", "list"]);
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
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            TrustedIssuerCommand::Rm(args) => assert!(args.cascade),
            _ => panic!("expected Rm"),
        }
    }

    #[test]
    fn rm_requires_issuer() {
        let parsed = Cli::try_parse_from(["wyrd", "rm", "--server", "https://acme.wyrd.cloud"]);
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

    /// The client secret is never an argument: an inline value is refused
    /// without being echoed back.
    #[test]
    fn add_refuses_an_inline_client_secret() {
        let secret = "issuer-client-secret-sentinel";
        let refused = Cli::try_parse_from([
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
            secret,
            "--claim-subject",
            "sub",
            "--principal-kind",
            "Workload",
            "--server",
            "https://acme.wyrd.cloud",
        ])
        .expect_err("--client-secret is not accepted");
        assert!(!refused.to_string().contains(secret));
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

    /// A secret file is read with its trailing newline trimmed.
    #[test]
    fn resolve_client_secret_reads_a_file() {
        use super::resolve_client_secret;

        let dir = std::env::temp_dir();
        let path = dir.join(format!("wyrd-cli-secret-{}", std::process::id()));
        std::fs::write(&path, "file-secret\n").expect("write temp secret");
        let resolved = resolve_client_secret(Some(path.clone())).expect("file secret resolves");
        std::fs::remove_file(&path).ok();
        assert_eq!(
            resolved.as_ref().map(ExposeSecret::expose_secret),
            Some("file-secret")
        );
    }
}

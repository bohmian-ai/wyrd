use std::process::ExitCode;

use clap::{Args, Subcommand};
use reqwest::Method;
use url::Url;
use wyrd_spec::auth::{CreateWorkloadBindingRequest, IssuerUrl, WorkloadBindingView};
use wyrd_spec::ids::CardUid;

use crate::error::WyrdCliError;

/// Collection path every workload-binding operation addresses.
const WORKLOAD_BINDINGS_PATH: &str = "/v1/admin/workload-bindings";

/// Subcommands of `wyrd auth workload-binding`.
#[derive(Debug, Subcommand)]
pub enum WorkloadBindingCommand {
    /// Create a workload binding (POST /v1/admin/workload-bindings).
    Add(AddArgs),
    /// List workload bindings (GET /v1/admin/workload-bindings).
    List(ListArgs),
    /// Remove a workload binding (DELETE /v1/admin/workload-bindings).
    Rm(RmArgs),
}

/// Arguments for `wyrd auth workload-binding add`.
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
    /// Active Service account the bound workload authenticates as.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Optional default workload root (a Service Card uid) selected
    /// when the assertion exchange names none.
    #[arg(long, value_name = "CARD_UID")]
    pub workload_uid: Option<CardUid>,
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Arguments for `wyrd auth workload-binding list`; the filters are optional.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Filter by issuer URL.
    #[arg(long, value_name = "URL")]
    pub issuer: Option<String>,
    /// Filter by subject.
    #[arg(long, value_name = "SUBJECT")]
    pub subject: Option<String>,
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Arguments for `wyrd auth workload-binding rm`.
#[derive(Debug, Args)]
pub struct RmArgs {
    /// Trusted issuer URL of the binding to remove.
    #[arg(long, value_name = "URL")]
    pub issuer: String,
    /// Subject of the binding to remove.
    #[arg(long, value_name = "SUBJECT")]
    pub subject: String,
    /// Wyrd server base URL. The credential is read from the ambient chain
    /// (`WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or
    /// `credentials.toml`), never from an argument.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

/// Run one `wyrd auth workload-binding` subcommand.
///
/// # Errors
/// Returns the selected operation's error: an invalid principal id or issuer URL,
/// a client-construction failure, or the server's stable Wyrd error.
pub async fn dispatch(command: WorkloadBindingCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        WorkloadBindingCommand::Add(args) => add(args).await,
        WorkloadBindingCommand::List(args) => list(args).await,
        WorkloadBindingCommand::Rm(args) => rm(args).await,
    }
}

/// Bind one issuer subject to a Service account, so its workload token
/// authenticates.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for a malformed principal id or issuer
/// URL, a client-construction error for a rejected endpoint, and the server's
/// stable Wyrd error when the caller is unauthorized or the issuer is untrusted.
async fn add(args: AddArgs) -> Result<ExitCode, WyrdCliError> {
    // Validate the principal and issuer locally, before any HTTP call.
    let principal_id = crate::principal::credential::principal_id(&args.principal)?;
    let issuer: IssuerUrl = args
        .issuer
        .parse()
        .map_err(|error| invalid("issuer", &args.issuer, &format!("an issuer URL: {error}")))?;

    let view: WorkloadBindingView = crate::client::from_global(Some(args.server.as_str()))?
        .request_json(
            Method::POST,
            WORKLOAD_BINDINGS_PATH,
            Some(&CreateWorkloadBindingRequest {
                issuer,
                subject: args.subject,
                audience: args.audience,
                principal_id,
                workload_uid: args.workload_uid,
            }),
        )
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("issuer:  {}", view.issuer);
    println!("subject: {}", view.subject);
    if let Some(ref aud) = view.audience {
        println!("audience:{aud}");
    }
    println!("principal_id: {}", view.principal_id);
    if let Some(ref uid) = view.workload_uid {
        println!("workload_uid: {uid}");
    }

    Ok(ExitCode::SUCCESS)
}

/// List workload bindings, optionally narrowed by issuer and subject.
///
/// # Errors
/// Returns a client-construction error for a rejected endpoint and the server's
/// stable Wyrd error when the caller is unauthorized or the read fails.
async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    // The serializer is not `Send`, so the path is finished before any await.
    let path = {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        if let Some(ref issuer) = args.issuer {
            query.append_pair("issuer", issuer);
        }
        if let Some(ref subject) = args.subject {
            query.append_pair("subject", subject);
        }
        path_with_query(&query.finish())
    };

    let views: Vec<WorkloadBindingView> = crate::client::from_global(Some(args.server.as_str()))?
        .request_json::<(), _>(Method::GET, &path, None)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for view in &views {
        println!(
            "{} / {} \u{2192} {}",
            view.issuer, view.subject, view.principal_id
        );
    }

    Ok(ExitCode::SUCCESS)
}

/// Remove one workload binding.
///
/// # Errors
/// Returns a client-construction error for a rejected endpoint and the server's
/// stable Wyrd error when the caller is unauthorized or the binding is unknown.
async fn rm(args: RmArgs) -> Result<ExitCode, WyrdCliError> {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("issuer", &args.issuer)
        .append_pair("subject", &args.subject)
        .finish();

    crate::client::from_global(Some(args.server.as_str()))?
        .request_json::<(), serde_json::Value>(Method::DELETE, &path_with_query(&query), None)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("deleted: {} / {}", args.issuer, args.subject);
    Ok(ExitCode::SUCCESS)
}

/// Append an already-encoded query string to the collection path.
///
/// The server addresses a binding by query rather than by path segment, and an
/// empty filter must not leave a bare `?` behind.
fn path_with_query(query: &str) -> String {
    if query.is_empty() {
        WORKLOAD_BINDINGS_PATH.to_owned()
    } else {
        format!("{WORKLOAD_BINDINGS_PATH}?{query}")
    }
}

/// Report one rejected argument.
fn invalid(field: &str, value: &str, expected: &str) -> WyrdCliError {
    WyrdCliError::InvalidArgument {
        field: field.to_owned(),
        value: value.to_owned(),
        expected: expected.to_owned(),
    }
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

    /// `add` parses with its required `--issuer`, `--subject`, `--principal`,
    /// and `--server` arguments.
    ///
    /// # Panics
    /// Panics when the complete argument list fails to parse.
    #[test]
    fn add_parses_required_args() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
    }

    /// `add` refuses an argument list that omits `--principal`.
    ///
    /// # Panics
    /// Panics when the list without `--principal` parses.
    #[test]
    fn add_requires_principal() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_err(), "must require --principal");
    }

    /// `add` refuses an argument list that omits `--subject`.
    ///
    /// # Panics
    /// Panics when the list without `--subject` parses.
    #[test]
    fn add_requires_subject() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_err(), "must require --subject");
    }

    /// `add` accepts an optional `--audience` and carries its value into the
    /// parsed arguments.
    ///
    /// # Panics
    /// Panics when the list fails to parse, parses to another subcommand, or
    /// the audience is not `myapp`.
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
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            WorkloadBindingCommand::Add(args) => {
                assert_eq!(args.audience.as_deref(), Some("myapp"));
            }
            _ => panic!("expected Add"),
        }
    }

    /// A malformed principal id is refused at dispatch, before any HTTP call,
    /// and a malformed default workload uid is refused by the parser.
    ///
    /// # Panics
    /// Panics when either malformed value is accepted.
    #[test]
    fn malformed_principal_and_workload_uid_fail_before_http() {
        assert!(crate::principal::credential::principal_id("not-a-uuid").is_err());
        let parsed = Cli::try_parse_from([
            "wyrd",
            "add",
            "--issuer",
            "https://idp.example.com",
            "--subject",
            "system:ci-runner",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--workload-uid",
            "not-a-card-uid",
            "--server",
            "https://acme.wyrd.cloud",
        ]);
        assert!(parsed.is_err(), "a malformed --workload-uid must not parse");
    }

    /// `list` parses with only `--server` and leaves the issuer and subject
    /// filters unset, so an unfiltered listing is the default.
    ///
    /// # Panics
    ///
    /// Panics when parsing fails, the command is not `List`, or either filter is
    /// set.
    #[test]
    fn list_parses_with_no_filters() {
        let parsed = Cli::try_parse_from(["wyrd", "list", "--server", "https://acme.wyrd.cloud"]);
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
        ]);
        assert!(parsed.is_ok(), "parse failed: {parsed:?}");
        match parsed.unwrap().command {
            WorkloadBindingCommand::List(args) => {
                assert_eq!(args.issuer.as_deref(), Some("https://idp.example.com"));
            }
            _ => panic!("expected List"),
        }
    }

    /// `list` without `--server` is rejected at parse time when the
    /// `WYRD_SERVER_URL` fallback is unset.
    ///
    /// # Panics
    ///
    /// Panics when clap accepts `list` without a server.
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
            "--subject",
            "system:ci-runner",
            "--server",
            "https://acme.wyrd.cloud",
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
        ]);
        assert!(parsed.is_err(), "must require --issuer");
    }
}

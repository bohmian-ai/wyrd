//! Platform credential rotation from the command line.
//!
//! Rotation is issue → verify → revoke, and all three halves are here because a
//! deployment root that can only be minted is not rotatable. Issue prints the
//! plaintext once; nothing here can print it again.

use std::process::ExitCode;

use clap::{Args, Subcommand};
use secrecy::SecretString;
use url::Url;
use uuid::Uuid;
use wyrd_client::Platform;
use wyrd_spec::auth::PrincipalId;

use crate::error::WyrdCliError;

/// Credential verbs on the platform plane.
#[derive(Debug, Subcommand)]
pub enum CredentialCommand {
    /// Mint a credential for a platform principal and print it once.
    Issue(IssueArgs),
    /// List a platform principal's credential metadata.
    List(ListArgs),
    /// Retire one of a platform principal's credentials.
    Revoke(RevokeArgs),
}

/// Environment variable holding the platform credential.
///
/// The credential is accepted only here, never as an argument, so it cannot
/// reach argv, shell history, or the derived `Debug` of parsed arguments.
const PLATFORM_CREDENTIAL_ENV: &str = "WYRD_PLATFORM_CREDENTIAL";

/// How every platform command reaches a deployment.
///
/// Flattened into each verb rather than hoisted onto the group so a command
/// reads the same whether it is invoked directly or through the group. The
/// platform credential is read from [`PLATFORM_CREDENTIAL_ENV`] at connect time.
#[derive(Debug, Args)]
pub struct PlatformEndpoint {
    /// Wyrd server base URL. The platform credential is read from
    /// `WYRD_PLATFORM_CREDENTIAL`.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Url,
}

impl PlatformEndpoint {
    /// Exchange the environment's platform credential and bind a platform
    /// client to the session.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::NoPlatformCredential`] when
    /// `WYRD_PLATFORM_CREDENTIAL` is unset or empty, and
    /// [`WyrdCliError::Server`] when the credential is refused or the platform
    /// plane is not configured on this deployment.
    pub(super) async fn connect(&self) -> Result<Platform, WyrdCliError> {
        let credential = std::env::var(PLATFORM_CREDENTIAL_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .map(SecretString::from)
            .ok_or(WyrdCliError::NoPlatformCredential)?;
        Platform::connect(self.server.as_str(), &credential)
            .await
            .map_err(|source| WyrdCliError::Server { source })
    }
}

/// Mint a credential for an existing platform principal.
#[derive(Debug, Args)]
pub struct IssueArgs {
    /// Platform principal to issue for.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Lifetime in days. Omit for an unbounded credential.
    #[arg(long, value_name = "DAYS")]
    pub expires_in_days: Option<u32>,
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// List a platform principal's credential metadata.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Platform principal whose credentials to list.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// Retire one of a platform principal's credentials.
#[derive(Debug, Args)]
pub struct RevokeArgs {
    /// Platform principal that owns the credential.
    #[arg(long, value_name = "UUID")]
    pub principal: String,
    /// Credential to retire.
    #[arg(long, value_name = "UUID")]
    pub credential_id: Uuid,
    /// Deployment and platform credential.
    #[command(flatten)]
    pub endpoint: PlatformEndpoint,
}

/// Dispatch one credential verb.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the principal is not a UUID,
/// and the server's stable Wyrd error when the caller is unauthorized or the
/// credential is unknown.
pub async fn dispatch(command: CredentialCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        CredentialCommand::Issue(args) => issue(args).await,
        CredentialCommand::List(args) => list(args).await,
        CredentialCommand::Revoke(args) => revoke(args).await,
    }
}

/// Parse an operator-supplied platform principal id.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] when the value is not a UUID.
fn principal_id(value: &str) -> Result<PrincipalId, WyrdCliError> {
    value.parse().map_err(|_| WyrdCliError::InvalidArgument {
        field: "principal".to_owned(),
        value: value.to_owned(),
        expected: "a platform principal UUID".to_owned(),
    })
}

/// Mint a credential and print the plaintext, once.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn issue(args: IssueArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    let issued = args
        .endpoint
        .connect()
        .await?
        .issue_credential(&principal, args.expires_in_days)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("credential_id: {}", issued.id);
    println!("credential: {}", issued.credential.expose());
    println!("This is the only time the credential is shown. Store it now.");
    Ok(ExitCode::SUCCESS)
}

/// Print one principal's credential metadata, newest first.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    let listing = args
        .endpoint
        .connect()
        .await?
        .list_credentials(&principal)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    for credential in listing.credentials {
        let state = credential.revoked_at.map_or_else(
            || {
                credential
                    .expires_at
                    .map_or_else(|| "live".to_owned(), |at| format!("expires {at}"))
            },
            |at| format!("revoked {at}"),
        );
        println!(
            "{}  {}  created {}  {state}",
            credential.id, credential.prefix, credential.created_at
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Retire one credential.
///
/// # Errors
/// Returns the errors documented on [`dispatch`].
async fn revoke(args: RevokeArgs) -> Result<ExitCode, WyrdCliError> {
    let principal = principal_id(&args.principal)?;
    args.endpoint
        .connect()
        .await?
        .revoke_credential(&principal, args.credential_id)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;

    println!("revoked: {}", args.credential_id);
    Ok(ExitCode::SUCCESS)
}

/// Argument parsing for the platform credential verbs.
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::CredentialCommand;

    /// Bare wrapper so the group parses without the binary's propagated flags.
    #[derive(Debug, Parser)]
    struct Cli {
        /// The credential verb under test.
        #[command(subcommand)]
        command: CredentialCommand,
    }

    /// Issuing takes a principal and a deployment, and nothing else is required.
    #[test]
    fn issue_requires_a_principal_and_an_endpoint() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "issue",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://wyrd.example",
        ])
        .expect("the issue verb parses");
        let CredentialCommand::Issue(args) = parsed.command else {
            panic!("issue parsed as another verb");
        };
        assert_eq!(args.expires_in_days, None);
    }

    /// Revoking names the owning principal, so a credential id alone is refused.
    #[test]
    fn revoking_requires_the_owning_principal() {
        let parsed = Cli::try_parse_from([
            "wyrd",
            "revoke",
            "--credential-id",
            "00000000-0000-7000-8000-000000000002",
            "--server",
            "https://wyrd.example",
        ]);
        assert!(parsed.is_err(), "revoke must require --principal");
    }

    /// The platform credential is never an argument: passing one is refused
    /// without echoing it, and parsed arguments cannot render one in `Debug`.
    #[test]
    fn the_platform_credential_is_not_an_argument() {
        let secret = "wyrd_global_secret_value";
        let refused = Cli::try_parse_from([
            "wyrd",
            "list",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://wyrd.example",
            "--credential",
            secret,
        ])
        .expect_err("--credential is not accepted");
        assert!(!refused.to_string().contains(secret));

        let parsed = Cli::try_parse_from([
            "wyrd",
            "list",
            "--principal",
            "00000000-0000-7000-8000-000000000001",
            "--server",
            "https://wyrd.example",
        ])
        .expect("the list verb parses from --server alone");
        assert!(!format!("{parsed:?}").contains(secret));
    }
}

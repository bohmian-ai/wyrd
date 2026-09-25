//! `wyrd gateway` tenant administration commands.
//!
//! Each verb reads at most one JSON document, calls the shared
//! [`wyrd_client::Gateway`] handle, and prints the server's redacted response
//! as JSON on stdout. Authorization, validation, audit, and persistence stay on
//! the server; failures keep their originating stable problem.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;
use serde_json::error::Category;
use wyrd_client::Gateway;
use wyrd_client::gateway_credential::CredentialWriter;
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName};

use crate::error::{CliBoundaryError, WyrdCliError};

/// Tenant gateway administration against one Wyrd server.
#[derive(Debug, Args)]
pub struct GatewayCommand {
    /// Wyrd server base URL. Credentials resolve from the environment or
    /// client configuration, never from arguments.
    #[arg(long, env = "WYRD_SERVER_URL", global = true)]
    pub server: Option<String>,
    /// Administered resource.
    #[command(subcommand)]
    pub resource: GatewayResource,
}

/// Gateway administration resources.
#[derive(Debug, Subcommand)]
pub enum GatewayResource {
    /// Named provider credentials (redacted on every read).
    #[command(subcommand)]
    Credential(CredentialVerb),
    /// Named provider deployments.
    #[command(subcommand)]
    Deployment(DeploymentVerb),
    /// Tenant fallback policy.
    #[command(subcommand)]
    FallbackPolicy(PolicyVerb),
    /// Tenant governance policy (limits, budgets, pricing).
    #[command(subcommand)]
    GovernancePolicy(PolicyVerb),
    /// Tenant payload capture policy.
    #[command(subcommand)]
    CapturePolicy(CaptureVerb),
}

/// Provider credential operations.
#[derive(Debug, Subcommand)]
pub enum CredentialVerb {
    /// Create or rotate the credential named in the JSON document.
    Put(DocumentArg),
    /// Read one redacted credential.
    Get(CredentialNameArg),
    /// List redacted credentials.
    List,
    /// Terminally revoke a credential.
    Revoke(CredentialNameArg),
    /// Delete an unreferenced credential; absent names succeed.
    Delete(CredentialNameArg),
}

/// Provider deployment operations.
#[derive(Debug, Subcommand)]
pub enum DeploymentVerb {
    /// Create or replace the deployment named in the JSON document.
    Put(DocumentArg),
    /// Read one deployment.
    Get(DeploymentNameArg),
    /// List deployments.
    List,
    /// Delete a deployment; absent names succeed.
    Delete(DeploymentNameArg),
}

/// Fallback and governance policy operations.
#[derive(Debug, Subcommand)]
pub enum PolicyVerb {
    /// Replace the policy with the JSON document.
    Put(DocumentArg),
    /// Read the effective policy.
    Get,
    /// Restore the default policy.
    Delete,
}

/// Capture policy operations; capture has no delete.
#[derive(Debug, Subcommand)]
pub enum CaptureVerb {
    /// Replace the capture policy with the JSON document.
    Put(DocumentArg),
    /// Read the effective capture policy.
    Get,
}

/// A JSON request document on disk, or `-` for standard input.
#[derive(Debug, Args)]
pub struct DocumentArg {
    /// Path to the JSON request body, or `-` to read it from standard input.
    #[arg(long, value_name = "PATH")]
    pub file: PathBuf,
}

/// A provider credential name.
#[derive(Debug, Args)]
pub struct CredentialNameArg {
    /// Tenant-scoped credential name.
    #[arg(value_parser = |raw: &str| ProviderCredentialName::new(raw))]
    pub name: ProviderCredentialName,
}

/// A provider deployment name.
#[derive(Debug, Args)]
pub struct DeploymentNameArg {
    /// Tenant-scoped deployment name.
    #[arg(value_parser = |raw: &str| ProviderDeploymentName::new(raw))]
    pub name: ProviderDeploymentName,
}

impl GatewayCommand {
    /// Runs the selected administration verb and prints its JSON response.
    ///
    /// Deletes print nothing. Writes replace the named resource, so rerunning
    /// a command is safe.
    ///
    /// # Errors
    ///
    /// Returns a local configuration, file, parse, or output error, or the
    /// server's stable problem for authorization, validation, conflict,
    /// not-found, or availability failures.
    pub async fn dispatch(self) -> Result<ExitCode, CliBoundaryError> {
        let client = crate::client::from_global(self.server.as_deref())?;
        let gateway = Gateway::new(client.clone());
        match self.resource {
            // Credential mutation runs on the handle no SDK re-exports: a
            // submission can carry a key value, and a name-only revoke or
            // delete cannot be told apart from one aimed at a managed secret.
            GatewayResource::Credential(verb) => {
                let credentials = CredentialWriter::new(client);
                match verb {
                    CredentialVerb::Put(arg) => print(
                        &credentials
                            .put_credential(&read_document(&arg.file)?)
                            .await?,
                    ),
                    CredentialVerb::Get(arg) => print(&gateway.credential(&arg.name).await?),
                    CredentialVerb::List => print(&gateway.credentials().await?),
                    CredentialVerb::Revoke(arg) => {
                        print(&credentials.revoke_credential(&arg.name).await?)
                    }
                    CredentialVerb::Delete(arg) => {
                        Ok(credentials.delete_credential(&arg.name).await?)
                    }
                }
            }
            GatewayResource::Deployment(verb) => match verb {
                DeploymentVerb::Put(arg) => {
                    print(&gateway.put_deployment(&read_document(&arg.file)?).await?)
                }
                DeploymentVerb::Get(arg) => print(&gateway.deployment(&arg.name).await?),
                DeploymentVerb::List => print(&gateway.deployments().await?),
                DeploymentVerb::Delete(arg) => Ok(gateway.delete_deployment(&arg.name).await?),
            },
            GatewayResource::FallbackPolicy(verb) => match verb {
                PolicyVerb::Put(arg) => print(
                    &gateway
                        .put_fallback_policy(&read_document(&arg.file)?)
                        .await?,
                ),
                PolicyVerb::Get => print(&gateway.fallback_policy().await?),
                PolicyVerb::Delete => Ok(gateway.delete_fallback_policy().await?),
            },
            GatewayResource::GovernancePolicy(verb) => match verb {
                PolicyVerb::Put(arg) => print(
                    &gateway
                        .put_governance_policy(&read_document(&arg.file)?)
                        .await?,
                ),
                PolicyVerb::Get => print(&gateway.governance_policy().await?),
                PolicyVerb::Delete => Ok(gateway.delete_governance_policy().await?),
            },
            GatewayResource::CapturePolicy(verb) => match verb {
                CaptureVerb::Put(arg) => print(
                    &gateway
                        .put_capture_policy(&read_document(&arg.file)?)
                        .await?,
                ),
                CaptureVerb::Get => print(&gateway.capture_policy().await?),
            },
        }?;
        Ok(ExitCode::SUCCESS)
    }
}

/// Reads and decodes one typed JSON request document from `path`, or from
/// standard input when `path` is `-`.
///
/// Standard input is how a secret-bearing body reaches the CLI: piping it
/// keeps the value out of the process arguments, out of shell history, and
/// out of any checked-in file.
///
/// A decode failure is reported by kind and position only. `serde_json`'s own
/// message quotes the offending input, which on this path is a provider key,
/// so it is never forwarded; `field` and `value` name the flag and the path,
/// not the body.
///
/// # Errors
///
/// Returns [`WyrdCliError::Io`] when the file or standard input cannot be read
/// and [`WyrdCliError::InvalidArgument`] when it is not the expected contract.
fn read_document<T: DeserializeOwned>(path: &Path) -> Result<T, WyrdCliError> {
    let bytes = if path == Path::new("-") {
        let mut bytes = Vec::new();
        io::Read::read_to_end(&mut io::stdin().lock(), &mut bytes)
            .map(|_| bytes)
            .map_err(|source| WyrdCliError::Io { source })?
    } else {
        std::fs::read(path).map_err(|source| WyrdCliError::Io { source })?
    };
    serde_json::from_slice(&bytes).map_err(|error| WyrdCliError::InvalidArgument {
        field: "--file".to_owned(),
        value: path.display().to_string(),
        expected: decode_failure(&error),
    })
}

/// Describes a JSON decode failure by kind and position, quoting none of the
/// document.
fn decode_failure(error: &JsonError) -> String {
    let kind = match error.classify() {
        Category::Syntax => "well-formed JSON",
        Category::Data => "JSON matching this command's request contract",
        Category::Eof => "a complete JSON document",
        Category::Io => "a readable document",
    };
    format!(
        "{kind} (decode failed at line {}, column {})",
        error.line(),
        error.column()
    )
}

/// Prints one response as pretty JSON followed by a newline.
///
/// # Errors
///
/// Returns [`WyrdCliError::Output`] when stdout cannot be written.
fn print<T: Serialize>(value: &T) -> Result<(), CliBoundaryError> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)
        .map_err(|error| error.to_string())
        .and_then(|()| writeln!(stdout).map_err(|error| error.to_string()))
        .map_err(|detail| CliBoundaryError::Local(WyrdCliError::Output { detail }))
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{CredentialVerb, GatewayCommand, GatewayResource, PolicyVerb};

    /// Parser harness mounting the gateway command at the root.
    #[derive(Debug, Parser)]
    struct TestCli {
        /// Parsed gateway command.
        #[command(flatten)]
        command: GatewayCommand,
    }

    /// The server flag is accepted after the verb, credential arguments are
    /// rejected, and names are validated.
    #[test]
    fn gateway_parser_accepts_trailing_server_rejects_token_and_validates_names() {
        let parsed =
            TestCli::try_parse_from(["wyrd", "credential", "revoke", "primary", "--server", "x"])
                .expect("revoke parses");
        assert!(
            TestCli::try_parse_from(["wyrd", "credential", "list", "--token", "y"]).is_err(),
            "credentials are never accepted as arguments"
        );
        assert!(matches!(
            parsed.command.resource,
            GatewayResource::Credential(CredentialVerb::Revoke(_))
        ));
        assert!(TestCli::try_parse_from(["wyrd", "credential", "get", "Bad Name"]).is_err());
    }

    /// Capture policy has no delete verb; other policies do.
    #[test]
    fn gateway_parser_matches_policy_lifecycles() {
        assert!(matches!(
            TestCli::try_parse_from(["wyrd", "governance-policy", "delete"])
                .expect("governance delete parses")
                .command
                .resource,
            GatewayResource::GovernancePolicy(PolicyVerb::Delete)
        ));
        assert!(TestCli::try_parse_from(["wyrd", "capture-policy", "delete"]).is_err());
    }
}

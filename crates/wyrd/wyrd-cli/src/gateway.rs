//! `wyrd gateway` tenant administration commands.
//!
//! Each verb reads at most one JSON document, calls the shared
//! [`wyrd_client::Gateway`] handle, and prints the server's redacted response
//! as JSON on stdout. `deployment put` also accepts a nonempty list of
//! deployments, validated whole before it writes each one in order, and a
//! composite `{credentials, deployments}` document applied in one server
//! transaction, whose credentials may name local environment variables.
//! Authorization, validation, audit, and persistence stay on the server;
//! failures keep their originating stable problem.

use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;
use serde_json::error::Category;
use wyrd_client::environment::Environment;
use wyrd_client::gateway_credential::CredentialWriter;
use wyrd_client::{Gateway, WyrdClient};
use wyrd_spec::auth::SecretBearer;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayContractError, ProviderCredentialView, ProviderCredentialWrite,
    ProviderCredentialWriteSource, ProviderDeployment, ProviderDeploymentsApplied,
    ProviderDeploymentsApply,
};
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName, ProviderId};

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
    /// Create or replace the deployment named in the JSON document, or each
    /// deployment of a nonempty JSON list in order, or apply a
    /// `{credentials, deployments}` document in one transaction; its
    /// credentials may use `{"local_env": "VARIABLE"}` sources.
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
        let gateway = Gateway::connect_with(&client).await?;
        match self.resource {
            // Credential mutation runs on the handle no SDK re-exports: a
            // submission can carry a key value, and a name-only revoke or
            // delete cannot be told apart from one aimed at a managed secret.
            // The mutating verbs render the in-process command functions.
            GatewayResource::Credential(verb) => match verb {
                CredentialVerb::Put(arg) => {
                    print(&put_credential_with(client, &read_document(&arg.file)?).await?)
                }
                CredentialVerb::Get(arg) => print(&gateway.credential(&arg.name).await?),
                CredentialVerb::List => print(&gateway.credentials().await?),
                CredentialVerb::Revoke(arg) => {
                    print(&revoke_credential_with(client, &arg.name).await?)
                }
                CredentialVerb::Delete(arg) => Ok(delete_credential_with(client, &arg.name).await?),
            },
            GatewayResource::Deployment(verb) => match verb {
                DeploymentVerb::Put(arg) => match DeploymentFile::read(&arg.file)? {
                    DeploymentFile::One(deployment) => {
                        print(&gateway.put_deployment(&deployment).await?)
                    }
                    DeploymentFile::Many(deployments) => {
                        put_deployments(&gateway, &deployments).await
                    }
                    DeploymentFile::Apply(apply) => {
                        print(&apply_deployments_with(client, &apply).await?)
                    }
                },
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

/// Create or rotate one provider credential and return its redacted view.
///
/// The in-process form of `wyrd gateway credential put`; the view is exactly
/// what the command prints. `write` may carry a provider key; it goes only to
/// the server, and the answer never echoes it. The request runs as `client`,
/// or, when it is omitted, as the ambient credential chain.
///
/// # Errors
/// Returns a `WYRD_CLIENT_*` error when the client cannot be built, or the
/// server's stable permission, invalid-configuration, conflict, or
/// availability error.
pub async fn put_provider_credential(
    write: &ProviderCredentialWrite,
    client: Option<WyrdClient>,
) -> Result<ProviderCredentialView, WyrdError> {
    put_credential_with(crate::client::explicit_or_ambient(client)?, write).await
}

/// Terminally revoke one provider credential and return its redacted view.
///
/// The in-process form of `wyrd gateway credential revoke`; repeating is
/// harmless. The request runs as `client`, or the ambient credential chain.
///
/// # Errors
/// Returns a `WYRD_CLIENT_*` error when the client cannot be built, or the
/// server's permission, not-found, or availability error.
pub async fn revoke_provider_credential(
    name: &ProviderCredentialName,
    client: Option<WyrdClient>,
) -> Result<ProviderCredentialView, WyrdError> {
    revoke_credential_with(crate::client::explicit_or_ambient(client)?, name).await
}

/// Delete one unreferenced provider credential; an absent name succeeds.
///
/// The in-process form of `wyrd gateway credential delete`. The request runs
/// as `client`, or the ambient credential chain.
///
/// # Errors
/// Returns a `WYRD_CLIENT_*` error when the client cannot be built, or the
/// server's permission, conflict, or availability error.
pub async fn delete_provider_credential(
    name: &ProviderCredentialName,
    client: Option<WyrdClient>,
) -> Result<(), WyrdError> {
    delete_credential_with(crate::client::explicit_or_ambient(client)?, name).await
}

/// Create or rotate one provider credential over an assembled `client`.
///
/// The single credential-put path behind both [`put_provider_credential`] and
/// the executable, which passes the client it already built.
///
/// # Errors
/// Returns the server's permission, invalid-configuration, conflict, or
/// availability error.
async fn put_credential_with(
    client: WyrdClient,
    write: &ProviderCredentialWrite,
) -> Result<ProviderCredentialView, WyrdError> {
    CredentialWriter::new(client).put_credential(write).await
}

/// Terminally revoke one provider credential over an assembled `client`.
///
/// The single revoke path behind [`revoke_provider_credential`] and the
/// executable.
///
/// # Errors
/// Returns the server's permission, not-found, or availability error.
async fn revoke_credential_with(
    client: WyrdClient,
    name: &ProviderCredentialName,
) -> Result<ProviderCredentialView, WyrdError> {
    CredentialWriter::new(client).revoke_credential(name).await
}

/// Delete one unreferenced provider credential over an assembled `client`.
///
/// The single delete path behind [`delete_provider_credential`] and the
/// executable; an absent name succeeds.
///
/// # Errors
/// Returns the server's permission, conflict, or availability error.
async fn delete_credential_with(
    client: WyrdClient,
    name: &ProviderCredentialName,
) -> Result<(), WyrdError> {
    CredentialWriter::new(client).delete_credential(name).await
}

/// Apply credentials and deployments in one server transaction over an
/// assembled `client`, the path behind `deployment put` and `server dev`.
///
/// # Errors
/// Returns the server's permission, invalid-configuration, conflict, or
/// availability error; nothing is applied on a refusal.
pub(crate) async fn apply_deployments_with(
    client: WyrdClient,
    apply: &ProviderDeploymentsApply,
) -> Result<ProviderDeploymentsApplied, WyrdError> {
    CredentialWriter::new(client).apply_deployments(apply).await
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
    decode_document(path, &read_bytes(path)?)
}

/// Reads the raw bytes of the document at `path`, or of standard input when
/// `path` is `-`.
///
/// # Errors
///
/// Returns [`WyrdCliError::Io`] when the file or standard input cannot be read.
fn read_bytes(path: &Path) -> Result<Vec<u8>, WyrdCliError> {
    if path == Path::new("-") {
        let mut bytes = Vec::new();
        io::Read::read_to_end(&mut io::stdin().lock(), &mut bytes)
            .map(|_| bytes)
            .map_err(|source| WyrdCliError::Io { source })
    } else {
        std::fs::read(path).map_err(|source| WyrdCliError::Io { source })
    }
}

/// Decodes `bytes`, read from `path`, as the request contract `T`, reporting
/// a failure as [`read_document`] does.
///
/// # Errors
///
/// Returns [`WyrdCliError::InvalidArgument`] when `bytes` is not the contract.
fn decode_document<T: DeserializeOwned>(path: &Path, bytes: &[u8]) -> Result<T, WyrdCliError> {
    serde_json::from_slice(bytes).map_err(|error| invalid_file(path, decode_failure(&error)))
}

/// The `--file` argument at `path` refused because it is not `expected`.
fn invalid_file(path: &Path, expected: String) -> WyrdCliError {
    WyrdCliError::InvalidArgument {
        field: "--file".to_owned(),
        value: path.display().to_string(),
        expected,
    }
}

/// The document of `wyrd gateway deployment put`: one deployment object, a
/// JSON list of the same objects, or a composite apply document.
#[derive(Debug)]
pub(crate) enum DeploymentFile {
    /// A single deployment; its put prints the server's response unchanged.
    One(ProviderDeployment),
    /// A nonempty, name-unique list of valid deployments, put in order.
    Many(Vec<ProviderDeployment>),
    /// A validated composite document with local sources already resolved.
    Apply(ProviderDeploymentsApply),
}

/// Fields that tell a composite document from a single deployment, which
/// never carries `deployments`.
#[derive(Deserialize)]
struct DocumentShape {
    /// Present only on a composite document.
    deployments: Option<IgnoredAny>,
}

/// A composite authoring document: the wire apply body whose credentials may
/// also name a local environment variable instead of carrying a value.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringApply {
    /// Credentials to create or rotate.
    #[serde(default)]
    pub(crate) credentials: Vec<AuthoringCredential>,
    /// Deployments to create or replace.
    pub(crate) deployments: Vec<ProviderDeployment>,
}

/// One authoring credential: the wire write with an authoring source.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoringCredential {
    /// Tenant-unique credential name.
    pub(crate) name: ProviderCredentialName,
    /// Provider the credential authenticates to.
    pub(crate) provider: ProviderId,
    /// Where the credential value comes from.
    pub(crate) source: AuthoringSource,
}

/// A credential source as authored locally.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthoringSource {
    /// Name of a local environment variable whose value the CLI submits as a
    /// managed secret; never sent or persisted as a reference.
    LocalEnv(String),
    /// Any existing wire source, sent unchanged.
    #[serde(untagged)]
    Wire(ProviderCredentialWriteSource),
}

impl AuthoringApply {
    /// Resolve every `local_env` source from `environment` into an in-memory
    /// managed secret and validate the resulting wire document whole.
    ///
    /// A variable name is a literal name (`[A-Za-z_][A-Za-z0-9_]*`), never
    /// shell syntax; an unset or blank value is refused without naming it.
    ///
    /// # Errors
    /// Returns the [`GatewayContractError`] for the first invalid source or
    /// document rule; nothing has been sent.
    pub(crate) fn resolve(
        self,
        environment: &Environment,
    ) -> Result<ProviderDeploymentsApply, GatewayContractError> {
        let credentials = self
            .credentials
            .into_iter()
            .enumerate()
            .map(|(index, credential)| {
                let source = match credential.source {
                    AuthoringSource::Wire(source) => source,
                    AuthoringSource::LocalEnv(variable) => {
                        let field = || format!("credentials[{index}].source.local_env");
                        if !is_variable_name(&variable) {
                            return Err(GatewayContractError::new(
                                field(),
                                "must be an environment variable name",
                            ));
                        }
                        let value = environment
                            .var(&variable)
                            .filter(|value| !value.trim().is_empty())
                            .ok_or_else(|| {
                                GatewayContractError::new(
                                    field(),
                                    "names an unset or empty environment variable",
                                )
                            })?;
                        ProviderCredentialWriteSource::ManagedSecret {
                            secret: SecretBearer::new(value),
                        }
                    }
                };
                Ok(ProviderCredentialWrite {
                    name: credential.name,
                    provider: credential.provider,
                    source,
                })
            })
            .collect::<Result<_, _>>()?;
        let apply = ProviderDeploymentsApply {
            credentials,
            deployments: self.deployments,
        };
        apply.validate()?;
        Ok(apply)
    }
}

/// Whether `name` is a portable environment variable name.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|next| next == '_' || next.is_ascii_alphanumeric())
}

impl DeploymentFile {
    /// Reads the deployment document at `path` (`-` for standard input).
    ///
    /// A document whose first token is `[` is a list; an object with
    /// `deployments` is a composite document, resolved and validated whole;
    /// anything else is one deployment. A list is refused whole, before anything is written, when
    /// it is empty, repeats a name, or holds an entry that breaks the
    /// deployment contract, so no earlier entry is written for a later
    /// mistake.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdCliError::Io`] when the document cannot be read and
    /// [`WyrdCliError::InvalidArgument`] when it does not decode, or a list is
    /// empty, repeats a name, or holds an invalid entry.
    pub(crate) fn read(path: &Path) -> Result<Self, WyrdCliError> {
        let bytes = read_bytes(path)?;
        if bytes.trim_ascii_start().first() != Some(&b'[') {
            if decode_document::<DocumentShape>(path, &bytes)?
                .deployments
                .is_some()
            {
                return decode_document::<AuthoringApply>(path, &bytes)?
                    .resolve(&Environment::Process)
                    .map(Self::Apply)
                    .map_err(|error| invalid_file(path, error.to_string()));
            }
            return decode_document(path, &bytes).map(Self::One);
        }
        let deployments: Vec<ProviderDeployment> = decode_document(path, &bytes)?;
        if deployments.is_empty() {
            return Err(invalid_file(
                path,
                "a nonempty list of deployments".to_owned(),
            ));
        }
        let mut names = HashSet::new();
        for (index, deployment) in deployments.iter().enumerate() {
            if !names.insert(&deployment.name) {
                return Err(invalid_file(
                    path,
                    format!("unique deployment names ([{index}] repeats an earlier name)"),
                ));
            }
            deployment
                .validate()
                .map_err(|error| invalid_file(path, format!("[{index}].{error}")))?;
        }
        Ok(Self::Many(deployments))
    }
}

/// How one entry of a deployment list fared.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum PutStatus {
    /// The server stored the deployment.
    Completed,
    /// The server refused the deployment; nothing after it was attempted.
    Failed,
    /// Not sent, because an earlier entry failed.
    Unattempted,
}

/// One line of the ordered outcome a deployment-list put prints.
#[derive(Debug, Serialize)]
struct PutOutcome<'a> {
    /// Deployment the entry names.
    name: &'a ProviderDeploymentName,
    /// What happened to it.
    status: PutStatus,
    /// Stable code of the refusal, for the failed entry only.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

/// Puts each of `deployments` in order through the existing per-deployment
/// write and prints one ordered outcome per entry.
///
/// Writes are separate requests, not one transaction: the first refusal stops
/// the run, every earlier write stays committed, and later entries are
/// reported unattempted. Rerunning the file converges, since each write
/// replaces its named deployment; names absent from the file are untouched.
///
/// # Errors
///
/// Returns the first entry's server refusal after printing the outcomes, or
/// [`WyrdCliError::Output`] when they cannot be printed.
async fn put_deployments(
    gateway: &Gateway,
    deployments: &[ProviderDeployment],
) -> Result<(), CliBoundaryError> {
    let mut outcomes = Vec::with_capacity(deployments.len());
    let mut refusal = None;
    for deployment in deployments {
        let (status, code) = match gateway.put_deployment(deployment).await {
            Ok(_) => (PutStatus::Completed, None),
            Err(error) => {
                let code = error.code().to_owned();
                refusal = Some(error);
                (PutStatus::Failed, Some(code))
            }
        };
        outcomes.push(PutOutcome {
            name: &deployment.name,
            status,
            code,
        });
        if refusal.is_some() {
            break;
        }
    }
    outcomes.extend(
        deployments[outcomes.len()..]
            .iter()
            .map(|deployment| PutOutcome {
                name: &deployment.name,
                status: PutStatus::Unattempted,
                code: None,
            }),
    );
    print(&outcomes)?;
    refusal.map_or(Ok(()), |error| Err(error.into()))
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

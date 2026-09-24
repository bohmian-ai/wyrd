//! Tenant Operator connection administration from the command line.
//!
//! Every verb is one call through `wyrd_client::OperatorConnections`. Create
//! and update read their whole request body — secret included — from a JSON
//! or YAML file or stdin, so a provider credential never enters argv, shell
//! history, or the derived `Debug` of parsed arguments. Responses are the
//! server's redacted views, printed as JSON.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use serde::de::DeserializeOwned;
use wyrd_client::operator_connections::OperatorConnectionId;

use crate::card::print_json;
use crate::error::WyrdCliError;
use crate::principal::credential::TenantEndpoint;

/// Operator connection verbs inside one tenant.
#[derive(Debug, Subcommand)]
pub enum OperatorConnectionCommand {
    /// Create a connection from a request body file (`-` for stdin).
    Create(BodyArgs),
    /// List the tenant's connections, redacted.
    List(TenantEndpoint),
    /// Read one connection, redacted.
    Get(IdArgs),
    /// Patch one connection from a request body file (`-` for stdin).
    Update(UpdateArgs),
    /// Disable one connection; it is never deleted.
    Disable(IdArgs),
}

/// A request body read from a file or stdin.
#[derive(Debug, Args)]
pub struct BodyArgs {
    /// JSON or YAML request body; `-` reads stdin. The body carries the
    /// secret, so it is never an argument.
    #[arg(long, value_name = "PATH")]
    pub body_file: PathBuf,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Address one connection.
#[derive(Debug, Args)]
pub struct IdArgs {
    /// Connection to act on.
    #[arg(long, value_name = "UUID")]
    pub connection_id: OperatorConnectionId,
    /// Deployment; the credential comes from the ambient chain.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Patch one connection from a body file.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Connection to patch.
    #[arg(long, value_name = "UUID")]
    pub connection_id: OperatorConnectionId,
    /// Request body and deployment.
    #[command(flatten)]
    pub body: BodyArgs,
}

/// Dispatch one Operator connection verb.
///
/// # Errors
/// Returns [`WyrdCliError::Io`] when the body cannot be read,
/// [`WyrdCliError::InvalidArgument`] when it does not match the request
/// contract, client-construction errors, and the server's stable Wyrd error
/// when the caller is unauthorized or the request is refused.
pub async fn dispatch(command: OperatorConnectionCommand) -> Result<ExitCode, WyrdCliError> {
    let connections =
        |endpoint: &TenantEndpoint| crate::client::operator_connections(endpoint.server.as_str());
    let printed = match command {
        OperatorConnectionCommand::Create(args) => {
            let request = read_body(&args.body_file)?;
            print_json(&connections(&args.endpoint)?.create(&request).await?)
        }
        OperatorConnectionCommand::List(endpoint) => {
            print_json(&connections(&endpoint)?.list().await?)
        }
        OperatorConnectionCommand::Get(args) => print_json(
            &connections(&args.endpoint)?
                .get(&args.connection_id)
                .await?,
        ),
        OperatorConnectionCommand::Update(args) => {
            let request = read_body(&args.body.body_file)?;
            print_json(
                &connections(&args.body.endpoint)?
                    .update(&args.connection_id, &request)
                    .await?,
            )
        }
        OperatorConnectionCommand::Disable(args) => print_json(
            &connections(&args.endpoint)?
                .disable(&args.connection_id)
                .await?,
        ),
    };
    printed.map(|()| ExitCode::SUCCESS)
}

/// Read and decode a JSON or YAML request body from `path`, or stdin for `-`.
///
/// A decode failure reports only its location, never the offending value,
/// because the body carries a secret.
///
/// # Errors
/// Returns [`WyrdCliError::Io`] when the body cannot be read and
/// [`WyrdCliError::InvalidArgument`] when it does not match `T`.
fn read_body<T: DeserializeOwned>(path: &std::path::Path) -> Result<T, WyrdCliError> {
    let mut text = String::new();
    if path.as_os_str() == "-" {
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|source| WyrdCliError::Io { source })?;
    } else {
        text = std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?;
    }
    serde_yaml::from_str(&text).map_err(|error| WyrdCliError::InvalidArgument {
        field: "body-file".to_owned(),
        value: path.display().to_string(),
        expected: error.location().map_or_else(
            || "an operator connection request body".to_owned(),
            |at| {
                format!(
                    "an operator connection request body (mismatch at line {} column {})",
                    at.line(),
                    at.column()
                )
            },
        ),
    })
}

/// Body decoding never echoes a secret value.
#[cfg(test)]
mod tests {
    use wyrd_client::operator_connections::CreateOperatorConnectionRequest;

    /// A body whose secret has the wrong type is refused by location only.
    ///
    /// # Panics
    /// Panics when the body decodes or the refusal echoes the value.
    #[test]
    fn body_refusal_never_echoes_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("body.yaml");
        std::fs::write(
            &path,
            "provider: slack\nname: ops\nworkspace_id: T1\nbot_token: xoxb-cli-secret\nextra: xoxb-cli-secret\n",
        )
        .expect("body writes");
        let refused = super::read_body::<CreateOperatorConnectionRequest>(&path)
            .expect_err("an unknown field is refused");
        assert!(
            !refused.to_string().contains("xoxb-cli-secret"),
            "{refused}"
        );
    }
}

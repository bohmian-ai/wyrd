//! Test-only napi projection of the in-process `wyrd` CLI commands.
//!
//! Each function runs the same [`wyrd_cli::commands`] implementation the
//! `wyrd` executable renders and returns a [`NativeCliOutcome`]: the value the
//! command prints with `--format json`, or the catalog problem document. The
//! `@wyrd/testing` `cli` wrapper throws that problem as the public SDK's
//! `WyrdError`. A networked command runs as the caller's explicit client when
//! a [`NativeCliConnection`] is given, else through the ambient credential
//! chain, exactly as the executable does.

use napi_derive::napi;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use wyrd_cli::commands::{self, SelectorArgs};
use wyrd_client::WyrdClient;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::ProviderCredentialWrite;
use wyrd_spec::ids::ProviderCredentialName;

/// Explicit client a command runs as: a public `WyrdClient`'s endpoints and
/// its current access token.
///
/// Native objects cannot cross from `@wyrd/sdk` into this addon, so the
/// wrapper reads the client's public `serverUrl`, `grpcUrl`, and
/// `accessToken()` and the command presents that token verbatim.
#[napi(object)]
pub struct NativeCliConnection {
    /// HTTP base URL of the Wyrd server.
    pub server_url: String,
    /// gRPC endpoint of the Wyrd server.
    pub grpc_url: String,
    /// Current bearer of the caller's client.
    pub access_token: String,
}

/// Card selector for `get` and `load`: a `uid` with `kind`, or `kind`,
/// `space`, and `name`; `version` narrows either.
#[napi(object)]
pub struct NativeCardSelector {
    /// Card kind, such as `Model` or `Prompt`.
    pub kind: Option<String>,
    /// Card space.
    pub space: Option<String>,
    /// Card name.
    pub name: Option<String>,
    /// Exact Card version.
    pub version: Option<String>,
    /// Exact Card UID.
    pub uid: Option<String>,
}

impl From<NativeCardSelector> for SelectorArgs {
    /// Hands the selector fields to the shared CLI selector parser unchanged.
    fn from(selector: NativeCardSelector) -> Self {
        Self {
            kind: selector.kind,
            space: selector.space,
            name: selector.name,
            version: selector.version,
            uid: selector.uid,
        }
    }
}

/// Options for `issueKey`: the bound Card and the key's label and lifetime.
#[napi(object)]
pub struct NativeIssueKey {
    /// Card kind, such as `Service` or `Agent`.
    pub kind: String,
    /// Card name.
    pub name: String,
    /// Exact Card version.
    pub version: String,
    /// Card space.
    pub space: String,
    /// Optional label stored with the key row.
    pub label: Option<String>,
    /// Optional lifetime override in seconds.
    pub expires_in_seconds: Option<u32>,
}

/// Closed result of one command: its JSON value or its problem document.
#[napi(object, object_from_js = false)]
pub struct NativeCliOutcome {
    /// JSON the command prints with `--format json`, when it succeeded.
    pub value_json: Option<String>,
    /// Catalog problem document (`code`, `status`, `title`, `detail`,
    /// `remediation`, `details`), when it failed.
    pub problem_json: Option<String>,
}

impl NativeCliOutcome {
    /// Projects one command result onto its JSON value or problem document.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the value cannot be serialized.
    fn from_result<T: Serialize>(result: Result<T, WyrdError>) -> napi::Result<Self> {
        Ok(match result {
            Ok(value) => Self {
                value_json: Some(
                    serde_json::to_string(&value)
                        .map_err(|error| napi::Error::from_reason(error.to_string()))?,
                ),
                problem_json: None,
            },
            Err(error) => Self {
                value_json: None,
                problem_json: Some(error.as_problem_json().to_string()),
            },
        })
    }
}

/// Builds the explicit client `connection` names, or `None` for the ambient
/// chain.
///
/// # Errors
///
/// Returns the catalog client error when the endpoints are invalid.
fn client(connection: Option<NativeCliConnection>) -> Result<Option<WyrdClient>, WyrdError> {
    connection
        .map(|connection| {
            wyrd_client::bifrost::client_from_options(
                Some(&connection.server_url),
                Some(&connection.access_token),
                Some(&connection.grpc_url),
            )
            .map_err(|error| WyrdError::from(&error))
        })
        .transpose()
}

/// Decodes one argument into its typed `wyrd-spec` contract.
///
/// The message names only `field` and quotes no part of the input: a
/// provider credential body can hold a provider key.
///
/// # Errors
///
/// Returns `WYRD_SPEC_400_VALIDATION` naming `field` when the value does not
/// match the contract.
fn decode<T: DeserializeOwned>(value: Value, field: &str) -> Result<T, WyrdError> {
    serde_json::from_value(value).map_err(|_| WyrdError::Validation {
        message: format!("{field} does not match its contract"),
        details: serde_json::json!({ "field": field }),
    })
}

/// Validates a local Card tree without contacting a server (`wyrd plan`).
///
/// # Errors
///
/// Returns a napi error only when the plan cannot be serialized; a load
/// failure is returned in the outcome.
#[napi]
pub fn cli_plan(path: String) -> napi::Result<NativeCliOutcome> {
    NativeCliOutcome::from_result(commands::plan(&path))
}

/// Registers a local Card tree and returns its receipt (`wyrd apply`).
///
/// # Errors
///
/// Returns a napi error only when the receipt cannot be serialized; local,
/// client, and server failures are returned in the outcome.
#[napi]
pub async fn cli_apply(
    path: String,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let result = match client(connection) {
        Ok(client) => commands::apply(&path, client).await,
        Err(error) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

/// Hydrates a Card's reachable graph into `output_dir` (`wyrd get`).
///
/// # Errors
///
/// Returns a napi error only when the summary cannot be serialized; selector,
/// client, and server failures are returned in the outcome.
#[napi]
pub async fn cli_get(
    selector: NativeCardSelector,
    output_dir: String,
    metadata_only: Option<bool>,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let selector = SelectorArgs::from(selector);
    let result = match client(connection) {
        Ok(client) => {
            commands::get(
                &selector,
                &output_dir,
                metadata_only.unwrap_or(false),
                client,
            )
            .await
        }
        Err(error) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

/// Loads one Card and materializes its artifacts (`wyrd load`).
///
/// # Errors
///
/// Returns a napi error only when the output cannot be serialized; selector,
/// client, and server failures are returned in the outcome.
#[napi]
pub async fn cli_load(
    selector: NativeCardSelector,
    path: Option<String>,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let selector = SelectorArgs::from(selector);
    let result = match client(connection) {
        Ok(client) => commands::load(&selector, path.as_deref().map(AsRef::as_ref), client).await,
        Err(error) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

/// Issues an API key bound to one exact Card (`wyrd auth issue-key`).
///
/// The outcome holds the plaintext key exactly once.
///
/// # Errors
///
/// Returns a napi error only when the response cannot be serialized;
/// coordinate, client, and server failures are returned in the outcome.
#[napi]
pub async fn cli_issue_key(
    options: NativeIssueKey,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let result = match client(connection) {
        Ok(client) => {
            commands::issue_key(
                &options.kind,
                &options.name,
                &options.version,
                &options.space,
                options.label.as_deref(),
                options.expires_in_seconds,
                client,
            )
            .await
        }
        Err(error) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

/// Creates or rotates one provider credential (`wyrd gateway credential put`).
///
/// A decode failure quotes no part of the body, which may carry a provider
/// key; the returned view is redacted.
///
/// # Errors
///
/// Returns a napi error only when the view cannot be serialized; a malformed
/// body, client, or server failure is returned in the outcome.
#[napi]
pub async fn cli_put_provider_credential(
    write: Value,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let result = match (
        decode::<ProviderCredentialWrite>(write, "write"),
        client(connection),
    ) {
        (Ok(write), Ok(client)) => commands::put_provider_credential(&write, client).await,
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

/// Terminally revokes one provider credential
/// (`wyrd gateway credential revoke`).
///
/// # Errors
///
/// Returns a napi error only when the view cannot be serialized; an invalid
/// name, client, or server failure is returned in the outcome.
#[napi]
pub async fn cli_revoke_provider_credential(
    name: String,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let name = decode::<ProviderCredentialName>(Value::String(name), "name");
    let result = match (name, client(connection)) {
        (Ok(name), Ok(client)) => commands::revoke_provider_credential(&name, client).await,
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

/// Deletes one unreferenced provider credential; an absent name succeeds
/// (`wyrd gateway credential delete`).
///
/// # Errors
///
/// Returns a napi error only when the result cannot be serialized; an invalid
/// name, client, or server failure is returned in the outcome.
#[napi]
pub async fn cli_delete_provider_credential(
    name: String,
    connection: Option<NativeCliConnection>,
) -> napi::Result<NativeCliOutcome> {
    let name = decode::<ProviderCredentialName>(Value::String(name), "name");
    let result = match (name, client(connection)) {
        (Ok(name), Ok(client)) => commands::delete_provider_credential(&name, client).await,
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    NativeCliOutcome::from_result(result)
}

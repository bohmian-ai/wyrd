//! Thin napi projection of the in-process `wyrd` CLI commands.
//!
//! Each function runs the same [`wyrd_cli::commands`] implementation the
//! `wyrd` executable renders and projects its result through
//! [`NativeLifecycleResult`]: the value the command prints with
//! `--format json`, or the stable catalog error. [`run_wyrd_cli`] is the
//! installed `wyrd` executable itself. No credential is an argument:
//! networked commands read the ambient chain, and `server` re-points only the
//! endpoint.

use napi_derive::napi;
use wyrd_cli::commands::{self, SelectorArgs};
use wyrd_spec::gateway::ProviderCredentialWrite;
use wyrd_spec::ids::ProviderCredentialName;

use crate::NativeLifecycleResult;
use crate::gateway::{decode, decode_name};

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

/// Runs the `wyrd` executable over `args` (argv including the program name)
/// and resolves its process exit code.
#[napi]
pub async fn run_wyrd_cli(args: Vec<String>) -> u32 {
    u32::from(wyrd_cli::run_cli_code(args).await)
}

/// Validates a local Card tree without contacting a server (`wyrd plan`).
///
/// # Errors
///
/// Returns a napi error only when the plan cannot be serialized; a load
/// failure is returned in the result.
#[napi]
pub fn cli_plan(path: String) -> napi::Result<NativeLifecycleResult> {
    NativeLifecycleResult::outcome(commands::plan(path.as_ref()))
}

/// Registers a local Card tree and returns its receipt (`wyrd apply`).
///
/// # Errors
///
/// Returns a napi error only when the receipt cannot be serialized; local,
/// client, and server failures are returned in the result.
#[napi]
pub async fn cli_apply(
    path: String,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    NativeLifecycleResult::outcome(commands::apply(path.as_ref(), server.as_deref()).await)
}

/// Hydrates a Card's reachable graph into `output_dir` (`wyrd get`).
///
/// # Errors
///
/// Returns a napi error only when the summary cannot be serialized; selector,
/// client, and server failures are returned in the result.
#[napi]
pub async fn cli_get(
    selector: NativeCardSelector,
    output_dir: String,
    metadata_only: Option<bool>,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    let selector = SelectorArgs::from(selector);
    NativeLifecycleResult::outcome(
        commands::get(
            &selector,
            output_dir.as_ref(),
            metadata_only.unwrap_or(false),
            server.as_deref(),
        )
        .await,
    )
}

/// Loads one Card and materializes its artifacts (`wyrd load`).
///
/// # Errors
///
/// Returns a napi error only when the output cannot be serialized; selector,
/// client, and server failures are returned in the result.
#[napi]
pub async fn cli_load(
    selector: NativeCardSelector,
    path: Option<String>,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    let selector = SelectorArgs::from(selector);
    NativeLifecycleResult::outcome(
        commands::load(
            &selector,
            path.as_deref().map(AsRef::as_ref),
            server.as_deref(),
        )
        .await,
    )
}

/// Issues an API key bound to one exact Card (`wyrd auth issue-key`).
///
/// The result holds the plaintext key exactly once.
///
/// # Errors
///
/// Returns a napi error only when the response cannot be serialized;
/// coordinate, client, and server failures are returned in the result.
#[napi]
pub async fn cli_issue_key(
    options: NativeIssueKey,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    NativeLifecycleResult::outcome(
        commands::issue_key(
            &options.kind,
            &options.name,
            &options.version,
            &options.space,
            options.label.as_deref(),
            options.expires_in_seconds,
            server.as_deref(),
        )
        .await,
    )
}

/// Creates or rotates one provider credential from its serialized body
/// (`wyrd gateway credential put`).
///
/// A decode failure quotes no part of the body, which may carry a provider
/// key; the returned view is redacted.
///
/// # Errors
///
/// Returns a napi error only when the view cannot be serialized; a malformed
/// body, client, or server failure is returned in the result.
#[napi]
pub async fn cli_put_provider_credential(
    write_json: String,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    let result = match decode::<ProviderCredentialWrite>(&write_json, "write") {
        Ok(write) => commands::put_provider_credential(&write, server.as_deref()).await,
        Err(error) => Err(error),
    };
    NativeLifecycleResult::outcome(result)
}

/// Terminally revokes one provider credential
/// (`wyrd gateway credential revoke`).
///
/// # Errors
///
/// Returns a napi error only when the view cannot be serialized; an invalid
/// name, client, or server failure is returned in the result.
#[napi]
pub async fn cli_revoke_provider_credential(
    name: String,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    let result = match decode_name::<ProviderCredentialName>(name) {
        Ok(name) => commands::revoke_provider_credential(&name, server.as_deref()).await,
        Err(error) => Err(error),
    };
    NativeLifecycleResult::outcome(result)
}

/// Deletes one unreferenced provider credential; an absent name succeeds
/// (`wyrd gateway credential delete`).
///
/// # Errors
///
/// Returns a napi error only when the result cannot be serialized; an invalid
/// name, client, or server failure is returned in the result.
#[napi]
pub async fn cli_delete_provider_credential(
    name: String,
    server: Option<String>,
) -> napi::Result<NativeLifecycleResult> {
    let result = match decode_name::<ProviderCredentialName>(name) {
        Ok(name) => commands::delete_provider_credential(&name, server.as_deref()).await,
        Err(error) => Err(error),
    };
    NativeLifecycleResult::outcome(result)
}

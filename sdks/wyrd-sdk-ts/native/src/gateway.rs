//! Thin napi projection of the Rust-owned tenant gateway administration client.
//!
//! Every operation delegates to [`wyrd_client::Gateway`]. Node passes names as
//! strings and request bodies as JSON text; both decode into the typed
//! `wyrd-spec` contracts here, so validation and the redacted response shapes
//! stay owned by Rust and the server. Results project through
//! [`NativeLifecycleResult`], keeping stable catalog error metadata.
//!
//! A decode failure is reported by kind, position, and argument name only.
//! `serde_json`'s own message quotes the offending input, and a gateway body
//! can carry a provider key, so that text never reaches Node.

use napi_derive::napi;
use serde::de::DeserializeOwned;
use serde_json::Error as JsonError;
use serde_json::error::Category;
use wyrd_client::Gateway;
use wyrd_spec::error::WyrdError;

use crate::{NativeLifecycleResult, napi_error};

/// Tenant gateway administration handle over the shared `wyrd_client` Gateway.
///
/// Provider credential mutation is absent by construction: submitting,
/// rotating, revoking, and deleting a credential live on
/// `wyrd_client::gateway_credential`, which this binding never constructs.
/// A JavaScript caller reaching this exported class directly therefore has no
/// method that can write a managed secret, and no runtime source check is
/// needed. Reads, deployments, policies, and invocation stay.
#[napi]
pub struct NativeGateway {
    /// Shared administration handle owning transport and authentication.
    gateway: Gateway,
}

/// Builds one gateway administration handle without performing IO.
///
/// Omitted arguments resolve through the same shared client configuration
/// chain as `connectCards`, so every capability authenticates identically.
///
/// # Errors
///
/// Returns a napi error when no credential resolves or the HTTP client cannot
/// be built.
#[napi]
pub fn connect_gateway(
    server_url: Option<String>,
    credential: Option<String>,
) -> napi::Result<NativeGateway> {
    let client = wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        None,
    );
    drop(server_url);
    drop(credential);
    let client = client.map_err(napi_error)?;
    Ok(NativeGateway {
        gateway: Gateway::new(client),
    })
}

#[napi]
impl NativeGateway {
    /// Reads one redacted provider credential by name.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; an invalid
    /// name or server failure is returned in the result.
    #[napi]
    pub async fn credential(&self, name: String) -> napi::Result<NativeLifecycleResult> {
        let result = match decode_name(name) {
            Ok(name) => self.gateway.credential(&name).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Lists the tenant's redacted provider credentials ordered by name.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the list cannot be serialized; server
    /// failures are returned in the result.
    #[napi]
    pub async fn credentials(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.credentials().await)
    }

    /// Creates or replaces a provider deployment from its serialized body.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the deployment cannot be serialized; a
    /// malformed body or server failure is returned in the result.
    #[napi]
    pub async fn put_deployment(
        &self,
        deployment_json: String,
    ) -> napi::Result<NativeLifecycleResult> {
        let result = match decode(&deployment_json, "deployment") {
            Ok(deployment) => self.gateway.put_deployment(&deployment).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Reads one provider deployment by name.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the deployment cannot be serialized; an
    /// invalid name or server failure is returned in the result.
    #[napi]
    pub async fn deployment(&self, name: String) -> napi::Result<NativeLifecycleResult> {
        let result = match decode_name(name) {
            Ok(name) => self.gateway.deployment(&name).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Lists the tenant's provider deployments ordered by name.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the list cannot be serialized; server
    /// failures are returned in the result.
    #[napi]
    pub async fn deployments(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.deployments().await)
    }

    /// Deletes one provider deployment; an absent name succeeds.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the result cannot be projected; an
    /// invalid name or server failure is returned in the result.
    #[napi]
    pub async fn delete_deployment(&self, name: String) -> napi::Result<NativeLifecycleResult> {
        let result = match decode_name(name) {
            Ok(name) => self.gateway.delete_deployment(&name).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Replaces the tenant fallback policy from its serialized body.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the policy cannot be serialized; a
    /// malformed body or server failure is returned in the result.
    #[napi]
    pub async fn put_fallback_policy(
        &self,
        policy_json: String,
    ) -> napi::Result<NativeLifecycleResult> {
        let result = match decode(&policy_json, "policy") {
            Ok(policy) => self.gateway.put_fallback_policy(&policy).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Reads the tenant fallback policy, or the default when none is set.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the policy cannot be serialized; server
    /// failures are returned in the result.
    #[napi]
    pub async fn fallback_policy(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.fallback_policy().await)
    }

    /// Restores the default fallback policy.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the result cannot be projected; server
    /// failures are returned in the result.
    #[napi]
    pub async fn delete_fallback_policy(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.delete_fallback_policy().await)
    }

    /// Replaces the tenant governance policy from its serialized body.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the policy cannot be serialized; a
    /// malformed body or server failure is returned in the result.
    #[napi]
    pub async fn put_governance_policy(
        &self,
        policy_json: String,
    ) -> napi::Result<NativeLifecycleResult> {
        let result = match decode(&policy_json, "policy") {
            Ok(policy) => self.gateway.put_governance_policy(&policy).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Reads the tenant governance policy, or the default when none is set.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the policy cannot be serialized; server
    /// failures are returned in the result.
    #[napi]
    pub async fn governance_policy(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.governance_policy().await)
    }

    /// Restores the default governance policy.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the result cannot be projected; server
    /// failures are returned in the result.
    #[napi]
    pub async fn delete_governance_policy(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.delete_governance_policy().await)
    }

    /// Replaces the tenant capture policy from its serialized write body.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the versioned policy cannot be
    /// serialized; a malformed body or server failure is returned in the
    /// result.
    #[napi]
    pub async fn put_capture_policy(
        &self,
        policy_json: String,
    ) -> napi::Result<NativeLifecycleResult> {
        let result = match decode(&policy_json, "policy") {
            Ok(policy) => self.gateway.put_capture_policy(&policy).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Reads the tenant capture policy, or the disabled default.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the policy cannot be serialized; server
    /// failures are returned in the result.
    #[napi]
    pub async fn capture_policy(&self) -> napi::Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.gateway.capture_policy().await)
    }
}

/// Decodes one JSON request body into its typed `wyrd-spec` contract.
///
/// # Errors
///
/// Returns the stable validation error naming `field` when the body does not
/// match the contract, including any contract-level grammar violation.
fn decode<T: DeserializeOwned>(json: &str, field: &str) -> Result<T, WyrdError> {
    serde_json::from_str(json).map_err(|error| invalid(&error, field))
}

/// Decodes one resource name into its grammar-validated identifier type.
///
/// # Errors
///
/// Returns the stable validation error for a name outside the grammar.
fn decode_name<T: DeserializeOwned>(name: String) -> Result<T, WyrdError> {
    serde_json::from_value(serde_json::Value::String(name)).map_err(|error| invalid(&error, "name"))
}

/// Builds the stable validation error for a rejected input `field`.
///
/// The message describes the decode failure by kind and position and quotes
/// no part of the input: a gateway body can hold a provider key.
fn invalid(error: &JsonError, field: &str) -> WyrdError {
    let kind = match error.classify() {
        Category::Syntax => "well-formed JSON",
        Category::Data => "a value matching this argument's contract",
        Category::Eof => "a complete JSON value",
        Category::Io => "a readable value",
    };
    WyrdError::Validation {
        message: format!(
            "{field} is invalid: expected {kind} (decode failed at line {}, column {})",
            error.line(),
            error.column()
        ),
        details: serde_json::json!({ "field": field }),
    }
}

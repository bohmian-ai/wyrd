//! Thin napi projection of the Rust-owned Operator connection handle.
//!
//! Every operation delegates to
//! `wyrd_client::operator_connections::OperatorConnections`; this module only
//! parses Node strings into wire types and projects the server's redacted
//! views through [`NativeLifecycleResult`]. A decode failure names the
//! argument, never its value, so a secret is never echoed.

use std::result::Result as StdResult;

use napi::Result;
use napi_derive::napi;
use serde::de::DeserializeOwned;
use wyrd_client::operator_connections::{
    CreateOperatorConnectionRequest, OperatorConnectionId, OperatorConnections,
    UpdateOperatorConnectionRequest,
};
use wyrd_spec::error::WyrdError;

use crate::{NativeLifecycleResult, NativeWyrdError};

/// Decode one serialized JavaScript argument, naming only the argument and
/// the decode category so a secret value is never echoed; shared by the
/// Operator connection and Verification handles.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` with `details.field` set to `field` when
/// `json` does not match the wire contract.
pub(crate) fn decode<T: DeserializeOwned>(field: &str, json: &str) -> StdResult<T, WyrdError> {
    serde_json::from_str(json).map_err(|error| WyrdError::Validation {
        message: format!("{field} does not match the wire contract"),
        details: serde_json::json!({ "field": field, "category": format!("{:?}", error.classify()) }),
    })
}

/// Decode one connection ID string.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `connection_id` for a non-UUID.
fn connection_id(id: String) -> StdResult<OperatorConnectionId, WyrdError> {
    decode("connection_id", &serde_json::Value::String(id).to_string())
}

/// Tenant-scoped Operator connection handle over the shared `wyrd_client` handle.
#[napi]
pub struct NativeOperatorConnections {
    /// Shared handle owning transport and authentication.
    connections: OperatorConnections,
}

/// Closed result of building one Operator connection handle: a handle or a catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeOperatorConnectionsConnection {
    /// Operator connection handle when construction succeeded.
    pub connections: Option<NativeOperatorConnections>,
    /// Catalog failure when no credential resolves or the client cannot be built.
    pub error: Option<NativeWyrdError>,
}

/// Builds one Operator connection handle without performing IO.
///
/// Omitted arguments resolve through the same shared client configuration
/// chain as `connectCards`.
#[napi]
pub fn connect_operator_connections(
    server_url: Option<String>,
    credential: Option<String>,
    tenant: Option<String>,
) -> NativeOperatorConnectionsConnection {
    let client = wyrd_client::bifrost::client_from_options(
        server_url.as_deref(),
        credential.as_deref(),
        None,
        tenant.as_deref(),
    );
    drop(server_url);
    drop(credential);
    drop(tenant);
    match client {
        Ok(client) => NativeOperatorConnectionsConnection {
            connections: Some(NativeOperatorConnections {
                connections: OperatorConnections::with_client(client),
            }),
            error: None,
        },
        Err(error) => NativeOperatorConnectionsConnection {
            connections: None,
            error: Some(NativeWyrdError::from_wyrd(&WyrdError::from(&error))),
        },
    }
}

#[napi]
impl NativeOperatorConnections {
    /// Creates one connection from a serialized `CreateOperatorConnectionRequest`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed request or server refusal is returned in the result.
    #[napi]
    pub async fn create(&self, request_json: String) -> Result<NativeLifecycleResult> {
        let result = match decode::<CreateOperatorConnectionRequest>("request", &request_json) {
            Ok(request) => self.connections.create(&request).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Lists the caller tenant's redacted connections.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the views cannot be serialized.
    #[napi]
    pub async fn list(&self) -> Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.connections.list().await)
    }

    /// Reads one connection's redacted view.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed ID or server refusal is returned in the result.
    #[napi]
    pub async fn get(&self, id: String) -> Result<NativeLifecycleResult> {
        let result = match connection_id(id) {
            Ok(id) => self.connections.get(&id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Updates or rotates one connection from a serialized
    /// `UpdateOperatorConnectionRequest`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed argument or server refusal is returned in the result.
    #[napi]
    pub async fn update(&self, id: String, request_json: String) -> Result<NativeLifecycleResult> {
        let result = match (
            connection_id(id),
            decode::<UpdateOperatorConnectionRequest>("request", &request_json),
        ) {
            (Ok(id), Ok(request)) => self.connections.update(&id, &request).await,
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Disables one connection; Operators naming it fail closed.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed ID or server refusal is returned in the result.
    #[napi]
    pub async fn disable(&self, id: String) -> Result<NativeLifecycleResult> {
        let result = match connection_id(id) {
            Ok(id) => self.connections.disable(&id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }
}

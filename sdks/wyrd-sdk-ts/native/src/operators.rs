//! Thin napi projection of the Rust-owned Operator connection handle.
//!
//! Every operation delegates to
//! `wyrd_client::operator_connections::OperatorConnections`; this module only
//! parses Node strings into wire types and projects the server's redacted
//! views through [`NativeLifecycleResult`]. A decode failure names the
//! argument, never its value, so a secret is never echoed.

use std::result::Result as StdResult;

use napi::Env;
use napi::bindgen_prelude::PromiseRaw;
use napi_derive::napi;
use serde::de::DeserializeOwned;
use wyrd_client::operator_connections::{
    CreateOperatorConnectionRequest, OperatorConnectionId, OperatorConnections,
    UpdateOperatorConnectionRequest,
};
use wyrd_spec::error::WyrdError;

use crate::client::NativeWyrdClient;
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

/// Closed result of connecting one Operator connection handle: the handle or a
/// catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeOperatorConnectionsConnection {
    /// Connected handle when the connection succeeded.
    pub handle: Option<NativeOperatorConnections>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

#[napi]
impl NativeWyrdClient {
    /// Connects one Operator connection handle that calls the server as this client.
    ///
    /// Completes any credential exchange this client requires before
    /// resolving, as `OperatorConnections::connect_with` does; the public TypeScript
    /// `OperatorConnections.connect` passes the caller's client or the ambient one. A refused
    /// or unreachable exchange is returned as catalog metadata.
    ///
    /// # Errors
    ///
    /// Never returns a napi error; failures are carried in the result.
    #[napi]
    pub fn operator_connections<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeOperatorConnectionsConnection>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            Ok(match OperatorConnections::connect_with(&client).await {
                Ok(connections) => NativeOperatorConnectionsConnection {
                    handle: Some(NativeOperatorConnections { connections }),
                    error: None,
                },
                Err(error) => NativeOperatorConnectionsConnection {
                    handle: None,
                    error: Some(NativeWyrdError::from_wyrd(&error)),
                },
            })
        })
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
    pub fn create<'env>(
        &self,
        env: &'env Env,
        request_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connections = self.connections.clone();
        crate::trace::spawn(env, async move {
            let result = match decode::<CreateOperatorConnectionRequest>("request", &request_json) {
                Ok(request) => connections.create(&request).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Lists the caller tenant's redacted connections.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the views cannot be serialized.
    #[napi]
    pub fn list<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connections = self.connections.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(connections.list().await)
        })
    }

    /// Reads one connection's redacted view.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed ID or server refusal is returned in the result.
    #[napi]
    pub fn get<'env>(
        &self,
        env: &'env Env,
        id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connections = self.connections.clone();
        crate::trace::spawn(env, async move {
            let result = match connection_id(id) {
                Ok(id) => connections.get(&id).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Updates or rotates one connection from a serialized
    /// `UpdateOperatorConnectionRequest`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed argument or server refusal is returned in the result.
    #[napi]
    pub fn update<'env>(
        &self,
        env: &'env Env,
        id: String,
        request_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connections = self.connections.clone();
        crate::trace::spawn(env, async move {
            let result = match (
                connection_id(id),
                decode::<UpdateOperatorConnectionRequest>("request", &request_json),
            ) {
                (Ok(id), Ok(request)) => connections.update(&id, &request).await,
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Disables one connection; Operators naming it fail closed.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// malformed ID or server refusal is returned in the result.
    #[napi]
    pub fn disable<'env>(
        &self,
        env: &'env Env,
        id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connections = self.connections.clone();
        crate::trace::spawn(env, async move {
            let result = match connection_id(id) {
                Ok(id) => connections.disable(&id).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }
}

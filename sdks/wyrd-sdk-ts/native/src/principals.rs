//! Thin napi projection of the Rust-owned tenant principal handle.
//!
//! Every operation delegates to `wyrd_client::principals::Principals`; this
//! module only parses Node strings into wire types and projects the server's
//! responses through [`NativeLifecycleResult`].

use std::result::Result as StdResult;

use napi::Result;
use napi_derive::napi;
use wyrd_client::principals::{
    CreateServicePrincipalRequest, PrincipalId, PrincipalQuery, Principals, RevokePrincipalRequest,
};
use wyrd_spec::error::WyrdError;

use crate::NativeLifecycleResult;
use crate::client::NativeWyrdClient;
use crate::operators::decode;

/// Decode one principal id string.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `principal_id` for a non-UUID.
fn principal_id(id: String) -> StdResult<PrincipalId, WyrdError> {
    decode("principal_id", &serde_json::Value::String(id).to_string())
}

/// Tenant principal handle over the shared `wyrd_client` handle.
#[napi]
pub struct NativePrincipals {
    /// Shared handle owning transport and authentication.
    principals: Principals,
}

#[napi]
impl NativeWyrdClient {
    /// Builds one principal handle that calls the server as this client.
    ///
    /// No IO happens here; the public TypeScript `Principals.connect` passes
    /// the caller's client or the ambient one.
    #[napi]
    pub fn principals(&self) -> NativePrincipals {
        NativePrincipals {
            principals: Principals::with_client(self.client.clone()),
        }
    }
}

#[napi]
impl NativePrincipals {
    /// Creates an unbound Service principal from a serialized
    /// `CreateServicePrincipalRequest`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the response cannot be serialized; a
    /// malformed request or server refusal is returned in the result.
    #[napi]
    pub async fn create_service_principal(
        &self,
        request_json: String,
    ) -> Result<NativeLifecycleResult> {
        let result = match decode::<CreateServicePrincipalRequest>("request", &request_json) {
            Ok(request) => self.principals.create_service_principal(&request).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Issues one more credential for a Service or Agent principal.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the credential cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub async fn issue_credential(&self, principal_id: String) -> Result<NativeLifecycleResult> {
        let result = match self::principal_id(principal_id) {
            Ok(id) => self.principals.issue_credential(&id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Lists a principal's credential metadata.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the list cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub async fn list_credentials(&self, principal_id: String) -> Result<NativeLifecycleResult> {
        let result = match self::principal_id(principal_id) {
            Ok(id) => self.principals.list_credentials(&id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Revokes one credential of a principal.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub async fn revoke_credential(
        &self,
        principal_id: String,
        credential_id: String,
    ) -> Result<NativeLifecycleResult> {
        let credential = decode(
            "credential_id",
            &serde_json::Value::String(credential_id).to_string(),
        );
        let result = match (self::principal_id(principal_id), credential) {
            (Ok(id), Ok(credential)) => self.principals.revoke_credential(&id, credential).await,
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Revokes a principal and every credential it holds.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be serialized; a
    /// malformed argument or server refusal is returned in the result.
    #[napi]
    pub async fn revoke_principal(
        &self,
        principal_id: String,
        request_json: String,
    ) -> Result<NativeLifecycleResult> {
        let result = match (
            self::principal_id(principal_id),
            decode::<RevokePrincipalRequest>("request", &request_json),
        ) {
            (Ok(id), Ok(request)) => self.principals.revoke_principal(&id, &request).await,
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Lists one page of assignable principals from a serialized
    /// `PrincipalQuery`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the page cannot be serialized; a
    /// malformed query or server refusal is returned in the result.
    #[napi]
    pub async fn list(&self, query_json: String) -> Result<NativeLifecycleResult> {
        let result = match decode::<PrincipalQuery>("query", &query_json) {
            Ok(query) => self.principals.list(&query).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Reads a principal's Role assignments.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the assignments cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub async fn roles(&self, principal_id: String) -> Result<NativeLifecycleResult> {
        let result = match self::principal_id(principal_id) {
            Ok(id) => self.principals.roles(&id).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Idempotently grants a direct Role.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the change cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub async fn grant_role(
        &self,
        principal_id: String,
        role: String,
    ) -> Result<NativeLifecycleResult> {
        let result = match self::principal_id(principal_id) {
            Ok(id) => self.principals.grant_role(&id, &role).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }

    /// Idempotently revokes a direct Role; identity-provider assignments stay.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the change cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub async fn revoke_role(
        &self,
        principal_id: String,
        role: String,
    ) -> Result<NativeLifecycleResult> {
        let result = match self::principal_id(principal_id) {
            Ok(id) => self.principals.revoke_role(&id, &role).await,
            Err(error) => Err(error),
        };
        NativeLifecycleResult::outcome(result)
    }
}

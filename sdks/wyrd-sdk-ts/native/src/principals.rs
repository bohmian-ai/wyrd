//! Thin napi projection of the Rust-owned tenant principal handle.
//!
//! Every operation delegates to `wyrd_client::principals::Principals`; this
//! module only parses Node strings into wire types and projects the server's
//! responses through [`NativeLifecycleResult`].

use std::result::Result as StdResult;

use napi::Env;
use napi::bindgen_prelude::PromiseRaw;
use napi_derive::napi;
use wyrd_client::principals::{
    CreateServicePrincipalRequest, PrincipalId, PrincipalQuery, Principals, RevokePrincipalRequest,
};
use wyrd_spec::error::WyrdError;

use crate::{NativeLifecycleResult, NativeWyrdError};
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

/// Closed result of connecting one principal handle: the handle or a
/// catalog error.
#[napi(object, object_from_js = false)]
pub struct NativePrincipalsConnection {
    /// Connected handle when the connection succeeded.
    pub handle: Option<NativePrincipals>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

#[napi]
impl NativeWyrdClient {
    /// Connects one principal handle that calls the server as this client.
    ///
    /// Completes any credential exchange this client requires before
    /// resolving, as `Principals::connect_with` does; the public TypeScript
    /// `Principals.connect` passes the caller's client or the ambient one. A refused
    /// or unreachable exchange is returned as catalog metadata.
    ///
    /// # Errors
    ///
    /// Never returns a napi error; failures are carried in the result.
    #[napi]
    pub fn principals<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativePrincipalsConnection>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            Ok(match Principals::connect_with(&client).await {
                Ok(principals) => NativePrincipalsConnection {
                    handle: Some(NativePrincipals { principals }),
                    error: None,
                },
                Err(error) => NativePrincipalsConnection {
                    handle: None,
                    error: Some(NativeWyrdError::from_wyrd(&error)),
                },
            })
        })
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
    pub fn create_service_principal<'env>(
        &self,
        env: &'env Env,
        request_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match decode::<CreateServicePrincipalRequest>("request", &request_json) {
                Ok(request) => principals.create_service_principal(&request).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Issues one more credential for a Service or Agent principal.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the credential cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub fn issue_credential<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match self::principal_id(principal_id) {
                Ok(id) => principals.issue_credential(&id).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Lists a principal's credential metadata.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the list cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub fn list_credentials<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match self::principal_id(principal_id) {
                Ok(id) => principals.list_credentials(&id).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Revokes one credential of a principal.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub fn revoke_credential<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
        credential_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let credential = decode(
                "credential_id",
                &serde_json::Value::String(credential_id).to_string(),
            );
            let result = match (self::principal_id(principal_id), credential) {
                (Ok(id), Ok(credential)) => principals.revoke_credential(&id, credential).await,
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Revokes a principal and every credential it holds.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the outcome cannot be serialized; a
    /// malformed argument or server refusal is returned in the result.
    #[napi]
    pub fn revoke_principal<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
        request_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match (
                self::principal_id(principal_id),
                decode::<RevokePrincipalRequest>("request", &request_json),
            ) {
                (Ok(id), Ok(request)) => principals.revoke_principal(&id, &request).await,
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Lists one page of assignable principals from a serialized
    /// `PrincipalQuery`.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the page cannot be serialized; a
    /// malformed query or server refusal is returned in the result.
    #[napi]
    pub fn list<'env>(
        &self,
        env: &'env Env,
        query_json: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match decode::<PrincipalQuery>("query", &query_json) {
                Ok(query) => principals.list(&query).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Reads a principal's Role assignments.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the assignments cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub fn roles<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match self::principal_id(principal_id) {
                Ok(id) => principals.roles(&id).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Idempotently grants a direct Role.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the change cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub fn grant_role<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
        role: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match self::principal_id(principal_id) {
                Ok(id) => principals.grant_role(&id, &role).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }

    /// Idempotently revokes a direct Role; identity-provider assignments stay.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the change cannot be serialized; a
    /// malformed id or server refusal is returned in the result.
    #[napi]
    pub fn revoke_role<'env>(
        &self,
        env: &'env Env,
        principal_id: String,
        role: String,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let principals = self.principals.clone();
        crate::trace::spawn(env, async move {
            let result = match self::principal_id(principal_id) {
                Ok(id) => principals.revoke_role(&id, &role).await,
                Err(error) => Err(error),
            };
            NativeLifecycleResult::outcome(result)
        })
    }
}

//! Thin napi projection of the Rust-owned tenant GitHub connection handle.
//!
//! Every operation delegates to `wyrd_client::github::GithubConnection`; this
//! module only projects the server's redacted views and transaction receipts
//! through [`NativeLifecycleResult`].

use napi::Env;
use napi::bindgen_prelude::PromiseRaw;
use napi_derive::napi;
use wyrd_client::github::GithubConnection;

use crate::client::NativeWyrdClient;
use crate::{NativeLifecycleResult, NativeWyrdError};

/// Tenant-scoped GitHub connection handle over the shared `wyrd_client` handle.
#[napi]
pub struct NativeGithubConnection {
    /// Shared handle owning transport and authentication.
    connection: GithubConnection,
}

/// Closed result of connecting one GitHub connection handle: the handle or a
/// catalog error.
#[napi(object, object_from_js = false)]
pub struct NativeGithubConnectionConnection {
    /// Connected handle when the connection succeeded.
    pub handle: Option<NativeGithubConnection>,
    /// Catalog failure otherwise.
    pub error: Option<NativeWyrdError>,
}

#[napi]
impl NativeWyrdClient {
    /// Connects one GitHub connection handle that calls the server as this client.
    ///
    /// Completes any credential exchange this client requires before
    /// resolving, as `GithubConnection::connect_with` does; the public
    /// TypeScript `GithubConnection.connect` passes the caller's client or the
    /// ambient one. A refused or unreachable exchange is returned as catalog
    /// metadata.
    ///
    /// # Errors
    ///
    /// Never returns a napi error; failures are carried in the result.
    #[napi]
    pub fn github_connection<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeGithubConnectionConnection>> {
        let client = self.client.clone();
        crate::trace::spawn(env, async move {
            Ok(match GithubConnection::connect_with(&client).await {
                Ok(connection) => NativeGithubConnectionConnection {
                    handle: Some(NativeGithubConnection { connection }),
                    error: None,
                },
                Err(error) => NativeGithubConnectionConnection {
                    handle: None,
                    error: Some(NativeWyrdError::from_wyrd(&error)),
                },
            })
        })
    }
}

#[napi]
impl NativeGithubConnection {
    /// Begins one connection transaction and returns GitHub's install URL
    /// and its expiry.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the receipt cannot be serialized; a
    /// server refusal is returned in the result.
    #[napi]
    pub fn begin<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connection = self.connection.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(connection.begin().await)
        })
    }

    /// Reads the tenant's redacted connection view.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// server refusal is returned in the result.
    #[napi]
    pub fn get<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connection = self.connection.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(connection.get().await)
        })
    }

    /// Releases the tenant's installation and returns the released view.
    ///
    /// # Errors
    ///
    /// Returns a napi error only when the view cannot be serialized; a
    /// server refusal is returned in the result.
    #[napi]
    pub fn disconnect<'env>(
        &self,
        env: &'env Env,
    ) -> napi::Result<PromiseRaw<'env, NativeLifecycleResult>> {
        let connection = self.connection.clone();
        crate::trace::spawn(env, async move {
            NativeLifecycleResult::outcome(connection.disconnect().await)
        })
    }
}

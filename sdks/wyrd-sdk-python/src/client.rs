//! `PyO3` boundary for the shared [`wyrd_client::WyrdClient`].
//!
//! A thin projection: construction resolves exactly as the Rust client does,
//! and [`PyWyrdClient::on_behalf_of`] calls the Rust RFC 8693 exchange. Token
//! exchange, caching, renewal, and errors stay in the Rust owner.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use secrecy::SecretString;
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::client_from_options;
use wyrd_spec::auth::TokenAudience;
use wyrd_spec::error::WyrdError;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

/// Python-facing handle to one authenticated [`WyrdClient`].
///
/// The delegated client returned by `on_behalf_of` shares this client's
/// connection pools and caches only its short-lived delegated token.
#[pyclass(module = "wyrd.client", name = "WyrdClient", frozen)]
pub struct PyWyrdClient {
    /// Shared transport plus authentication state.
    inner: WyrdClient,
}

impl PyWyrdClient {
    /// Wrap a Rust client another boundary type already holds, such as the
    /// client a `WyrdState` resolved, so Python helpers can authenticate as it.
    pub(crate) const fn from_native(inner: WyrdClient) -> Self {
        Self { inner }
    }

    /// Borrow the wrapped Rust client so another boundary type, such as the
    /// Bifrost facade, can compose it without re-resolving a credential.
    pub(crate) const fn inner(&self) -> &WyrdClient {
        &self.inner
    }

    /// Resolve the client a server-facing surface acts as.
    ///
    /// Every public Python surface that calls the server takes one optional
    /// `client`; this is the single place that turns it into a Rust client.
    /// A supplied client is reused as is, sharing its transport and token
    /// cache. Omitted, the ambient chain resolves exactly as Rust
    /// [`WyrdClient::from_global`] does: the global configuration file, then
    /// the environment, then the saved user login.
    ///
    /// # Arguments
    /// * `client` - The caller's explicit client, or `None` for the ambient chain.
    ///
    /// # Errors
    /// Returns the configuration, credential, or saved-login error raised while
    /// resolving the ambient chain; a supplied client never fails.
    pub(crate) fn resolve(client: Option<&Self>) -> Result<WyrdClient, WyrdError> {
        match client {
            Some(client) => Ok(client.inner.clone()),
            None => WyrdClient::from_global().map_err(WyrdError::from),
        }
    }
}

#[pymethods]
impl PyWyrdClient {
    /// Build a client from optionally overridden transport values.
    ///
    /// Omitted values resolve through `client_from_options`: the environment,
    /// then the saved user login for this server (the one for `WYRD_TENANT`
    /// when set, otherwise the newest), then
    /// `~/.config/wyrd/credentials.toml`.
    ///
    /// # Arguments
    /// * `server_url` - The HTTP server URL, or `None` to resolve it.
    /// * `credential` - The client's own credential: a Wyrd API key
    ///   (`wyrd_sk_…`), exchanged for a short-lived access token and renewed,
    ///   or an access token, presented as-is. `None` resolves the chain.
    /// * `grpc_url` - The gRPC endpoint, or `None` to derive it.
    ///
    /// # Errors
    /// Raises `WyrdError` carrying `WYRD_CLIENT_401_NO_CREDENTIALS` when no
    /// credential resolves, `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` when this
    /// server has saved logins but none for `WYRD_TENANT`, or the saved login
    /// cannot be used, or a transport error when the client cannot be built.
    #[new]
    #[pyo3(signature = (server_url=None, credential=None, grpc_url=None))]
    fn __new__(
        server_url: Option<&str>,
        credential: Option<&str>,
        grpc_url: Option<&str>,
    ) -> WyrdPyResult<Self> {
        client_from_options(server_url, credential, grpc_url)
            .map(|inner| Self { inner })
            .map_err(|error| WyrdPyError::from(WyrdError::from(error)))
    }

    /// The effective HTTP server URL this client sends requests to.
    #[getter]
    fn server_url(&self) -> &str {
        self.inner.server_url()
    }

    /// The effective gRPC endpoint: the explicit `grpc_url` when one was given,
    /// else `WYRD_GRPC_URL`, else the server URL's scheme and host on the
    /// public gRPC port `50051`.
    #[getter]
    fn grpc_url(&self) -> &str {
        self.inner.grpc_url()
    }

    /// Return a current bearer for this client's credential.
    ///
    /// Reads through the Rust client's shared auth middleware with the GIL
    /// released, so an expired token is renewed there; nothing is cached on
    /// the Python side.
    ///
    /// # Errors
    /// Raises the server's stable error when it refuses the credential, or a
    /// transport error when the token endpoint cannot be reached.
    fn access_token(&self, py: Python<'_>) -> WyrdPyResult<String> {
        py.detach(|| wyrd_runtime::runtime().block_on(self.inner.access_token()))
            .map(|bearer| bearer.expose().to_owned())
            .map_err(WyrdPyError::from)
    }

    /// Return a client that acts for the holder of `subject_token`.
    ///
    /// This client's credential is the actor. The first RFC 8693 exchange runs
    /// here, with the GIL released, so a refusal raises at the call site; the
    /// returned client re-exchanges in Rust before expiry.
    ///
    /// # Errors
    /// Raises `WYRD_SPEC_400_VALIDATION` for an audience other than `"wyrd"` or
    /// `"bifrost"`, and the server's stable error when the exchange is refused.
    #[pyo3(signature = (subject_token, audience="bifrost"))]
    fn on_behalf_of(
        &self,
        py: Python<'_>,
        subject_token: String,
        audience: &str,
    ) -> WyrdPyResult<Self> {
        let audience: TokenAudience = serde_json::from_value(serde_json::Value::String(
            audience.to_owned(),
        ))
        .map_err(|error| {
            WyrdPyError::from(WyrdError::Validation {
                message: format!("audience is invalid: {error}"),
                details: serde_json::json!({ "field": "audience" }),
            })
        })?;
        let subject_token = SecretString::from(subject_token);
        py.detach(|| {
            wyrd_runtime::runtime().block_on(self.inner.on_behalf_of(subject_token, audience))
        })
        .map(|inner| Self { inner })
        .map_err(WyrdPyError::from)
    }
}

/// Register [`PyWyrdClient`] on the `client` submodule.
///
/// # Errors
/// Returns `PyO3` registration errors.
pub fn register_client(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyWyrdClient>()
}

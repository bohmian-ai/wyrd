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

#[pymethods]
impl PyWyrdClient {
    /// Build a client from optionally overridden transport values.
    ///
    /// Omitted values resolve through `client_from_options`: the environment,
    /// then `~/.config/wyrd/credentials.toml`.
    ///
    /// # Errors
    /// Raises `WyrdError` carrying `WYRD_CLIENT_401_NO_CREDENTIALS` when no
    /// credential resolves, or a transport error when the client cannot be built.
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

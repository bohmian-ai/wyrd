//! `PyO3` boundary for the shared [`wyrd_client::WyrdClient`].
//!
//! A thin projection: construction resolves exactly as the Rust client does,
//! and [`PyWyrdClient::on_behalf_of`] calls the Rust RFC 8693 exchange. Token
//! exchange, caching, renewal, and errors stay in the Rust owner.

use std::future::Future;
use std::sync::OnceLock;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use secrecy::SecretString;
use tokio::sync::Mutex;
use tokio::time::Instant;
use wyrd_client::WyrdClient;
use wyrd_client::bifrost::client_from_options;
use wyrd_client::transport::trace::with_caller_trace;
use wyrd_spec::auth::TokenAudience;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
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

    /// Clone the optional client a handle constructor received, so the handle
    /// can connect as it later without holding the Python object.
    ///
    /// # Arguments
    /// * `client` - The caller's explicit client, or `None` for the ambient chain.
    pub(crate) fn native(client: Option<&Self>) -> Option<WyrdClient> {
        client.map(|client| client.inner.clone())
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

/// A Python handle's one native connection.
///
/// The constructor keeps only the optional client, so constructing a handle
/// performs no IO. The first server operation runs the shared Rust `connect`
/// (or `connect_with` for a supplied client) once and every later operation
/// reuses that connected handle; a failed attempt stores nothing, so the next
/// operation retries with the same arguments. The eager Python `connect`
/// factories build this and initialize it immediately, so eager and lazy
/// handles share one connection path.
pub(crate) struct LazyConnection<T> {
    /// The client the caller supplied, used as given; `None` resolves the
    /// ambient chain inside the shared Rust `connect`.
    client: Option<WyrdClient>,
    /// The connected native handle, set once.
    handle: OnceLock<T>,
    /// Serializes first connections so concurrent first operations connect
    /// once; awaited only with the GIL released, so a deadline-bounded
    /// caller can stop waiting for another caller's first connection.
    gate: Mutex<()>,
}

impl<T: Send + Sync> LazyConnection<T> {
    /// Retain `client` without resolving or connecting anything.
    ///
    /// # Arguments
    /// * `client` - The caller's explicit client, or `None` for the ambient chain.
    pub(crate) fn new(client: Option<&PyWyrdClient>) -> Self {
        Self {
            client: PyWyrdClient::native(client),
            handle: OnceLock::new(),
            gate: Mutex::new(()),
        }
    }

    /// Retain `handle`, already connected, so no operation connects again:
    /// the form a handle the shared client returned connected takes, such
    /// as `submit`'s queued execution.
    ///
    /// # Arguments
    /// * `handle` - The connected native handle every operation reuses.
    pub(crate) fn ready(handle: T) -> Self {
        Self {
            client: None,
            handle: OnceLock::from(handle),
            gate: Mutex::new(()),
        }
    }

    /// The connected handle when a connection has already succeeded; never
    /// connects, so local state can be read without IO.
    pub(crate) fn connected(&self) -> Option<&T> {
        self.handle.get()
    }

    /// Return the connected handle, connecting it with `connect` on first use.
    ///
    /// `connect` receives the retained client and runs on the shared runtime
    /// with the GIL released, under the caller's active trace context.
    ///
    /// # Arguments
    /// * `py` - The attached interpreter, released while connecting.
    /// * `connect` - The shared Rust connection for this handle kind.
    ///
    /// # Errors
    /// Returns `connect`'s error unchanged; nothing is stored, so a later call
    /// connects again.
    ///
    /// # Panics
    /// Never in practice: only a deadline-bounded connection can expire, and
    /// this one has no deadline.
    pub(crate) fn get<E, F, Fut>(&self, py: Python<'_>, connect: F) -> Result<&T, E>
    where
        E: Send,
        F: FnOnce(Option<WyrdClient>) -> Fut + Send,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        self.connect_by(py, None, connect)
            .expect("a connection without a deadline never expires")
    }

    /// [`LazyConnection::get`] bounded by `deadline`: waiting for another
    /// caller's first connection and this caller's own `connect` both stop
    /// when it passes, and an expired deadline starts no connection.
    ///
    /// An abandoned connection stores nothing and releases the gate, so a
    /// later call connects again; a connected handle returns at once.
    ///
    /// # Arguments
    /// * `py` - The attached interpreter, released while connecting.
    /// * `deadline` - When the caller stops waiting for the connection.
    /// * `connect` - The shared Rust connection for this handle kind.
    ///
    /// # Errors
    /// Returns `None` when `deadline` passes first, and otherwise `connect`'s
    /// error unchanged.
    pub(crate) fn get_before<E, F, Fut>(
        &self,
        py: Python<'_>,
        deadline: Instant,
        connect: F,
    ) -> Option<Result<&T, E>>
    where
        E: Send,
        F: FnOnce(Option<WyrdClient>) -> Fut + Send,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        self.connect_by(py, Some(deadline), connect)
    }

    /// Connect once under the gate, stopping at `deadline` when one is given.
    ///
    /// Returns `None` only when `deadline` passed before the handle was
    /// connected; it is checked before any work, so an expired deadline
    /// takes no gate and performs no IO.
    ///
    /// # Errors
    /// Returns `connect`'s error unchanged; nothing is stored.
    fn connect_by<E, F, Fut>(
        &self,
        py: Python<'_>,
        deadline: Option<Instant>,
        connect: F,
    ) -> Option<Result<&T, E>>
    where
        E: Send,
        F: FnOnce(Option<WyrdClient>) -> Fut + Send,
        Fut: Future<Output = Result<T, E>> + Send,
    {
        if let Some(handle) = self.handle.get() {
            return Some(Ok(handle));
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return None;
        }
        let caller = crate::trace::active_trace_context(py);
        let connected = async {
            let _gate = self.gate.lock().await;
            if let Some(handle) = self.handle.get() {
                return Ok(handle);
            }
            let handle = connect(self.client.clone()).await?;
            Ok(self.handle.get_or_init(|| handle))
        };
        py.detach(|| {
            wyrd_runtime::runtime().block_on(with_caller_trace(caller, async {
                match deadline {
                    Some(deadline) => tokio::time::timeout_at(deadline, connected).await.ok(),
                    None => Some(connected.await),
                }
            }))
        })
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
    /// * `workload_uid` - Exact Service Card UID the client runs as,
    ///   as [`Self::for_workload`] selects it, or `None` for
    ///   `WYRD_WORKLOAD_UID`, else rootless work.
    ///
    /// # Errors
    /// Raises `WyrdError` carrying `WYRD_CLIENT_401_NO_CREDENTIALS` when no
    /// credential resolves, `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` when this
    /// server has saved logins but none for `WYRD_TENANT`, or the saved login
    /// cannot be used, `WYRD_SPEC_400_VALIDATION` when `workload_uid` is not a
    /// UUIDv7, or a transport error when the client cannot be built.
    #[new]
    #[pyo3(signature = (server_url=None, credential=None, grpc_url=None, *, workload_uid=None))]
    fn __new__(
        server_url: Option<&str>,
        credential: Option<&str>,
        grpc_url: Option<&str>,
        workload_uid: Option<&str>,
    ) -> WyrdPyResult<Self> {
        let workload_uid = workload_uid.map(parse_workload_uid).transpose()?;
        let inner = client_from_options(server_url, credential, grpc_url)
            .map_err(|error| WyrdPyError::from(WyrdError::from(error)))?;
        Ok(Self {
            inner: match workload_uid {
                Some(uid) => inner.for_workload(uid),
                None => inner,
            },
        })
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
        crate::trace::block_on(py, self.inner.access_token())
            .map(|bearer| bearer.expose().to_owned())
            .map_err(WyrdPyError::from)
    }

    /// Return an `httpx.Auth` / `httpx2.Auth` that authenticates every
    /// request to Wyrd as this client.
    ///
    /// The flows live in `wyrd.client._http_auth.HttpAuth`, which holds this
    /// client and asks `access_token` on each request, so token caching and
    /// renewal stay in Rust. Being a getter, it exists on every client,
    /// including one returned by `on_behalf_of`.
    ///
    /// # Errors
    /// Raises `ImportError` naming the `gateway` extra when `httpx` or
    /// `httpx2` is not installed.
    #[getter]
    fn http_auth<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        PyModule::import(slf.py(), "wyrd.client._http_auth")?
            .getattr("HttpAuth")?
            .call1((slf,))
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
        crate::trace::block_on(py, self.inner.on_behalf_of(subject_token, audience))
            .map(|inner| Self { inner })
            .map_err(WyrdPyError::from)
    }

    /// The exact Service Card UID this client runs as, or `None`
    /// for rootless work.
    #[getter]
    fn workload_uid(&self) -> Option<&str> {
        self.inner.workload_uid().map(CardUid::as_str)
    }

    /// The deployment environment correlation tag this client's requests
    /// carry, or `None` when none is configured.
    #[getter]
    fn deployment_environment(&self) -> Option<String> {
        self.inner
            .auth()
            .deployment_environment()
            .map(str::to_owned)
    }

    /// Return a client that runs as the exact Service root
    /// `workload_uid`, sharing this client's credential and connection pools
    /// but not its token cache. Performs no IO; the server validates the root
    /// on first use.
    ///
    /// # Errors
    /// Raises `WYRD_SPEC_400_VALIDATION` when `workload_uid` is not a UUIDv7.
    fn for_workload(&self, workload_uid: &str) -> WyrdPyResult<Self> {
        Ok(Self {
            inner: self.inner.for_workload(parse_workload_uid(workload_uid)?),
        })
    }
}

/// Parse a caller's workload root.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `workload_uid` when `uid` is not a
/// UUIDv7.
fn parse_workload_uid(uid: &str) -> WyrdPyResult<CardUid> {
    CardUid::new(uid).map_err(|error| {
        WyrdPyError::from(WyrdError::Validation {
            message: format!("workload_uid is invalid: {error}"),
            details: serde_json::json!({ "field": "workload_uid" }),
        })
    })
}

/// Register [`PyWyrdClient`] on the `client` submodule.
///
/// # Errors
/// Returns `PyO3` registration errors.
pub fn register_client(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyWyrdClient>()
}

//! `PyO3` boundary for the shared tenant principal handle.
//!
//! Each method decodes its Python arguments into the typed wire contract at
//! the edge, calls [`wyrd_client::principals::Principals`] with the GIL
//! released, and returns the server's response as plain Python values. No
//! authorization, assignment, or credential logic lives here.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::Value;
use wyrd_client::principals::{
    CreateServicePrincipalRequest, PrincipalId, PrincipalQuery, Principals as NativePrincipals,
    RevokePrincipalRequest, WorkloadRestriction,
};
use wyrd_utils::py::{WyrdPyResult, pyobject_to_json};

use crate::operators::{decode, to_python};

/// Python-facing tenant principal handle: create Service principals, manage
/// their credentials, discover principals, and grant or revoke direct Roles.
///
/// Every call blocks the calling thread with the GIL released. A Role change
/// reaches the principal at its next token; already-issued tokens keep the
/// Roles they were signed with.
#[pyclass(module = "wyrd._wyrd.auth", name = "Principals")]
pub struct Principals {
    /// The one native connection every call delegates to, connected on the
    /// first server operation (or by `connect`).
    inner: crate::client::LazyConnection<NativePrincipals>,
}

/// Decode a principal id argument.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `principal_id` when the value is
/// not a principal id.
fn principal_id(value: &str) -> WyrdPyResult<PrincipalId> {
    decode("principal_id", Value::from(value))
}

impl Principals {
    /// Return the connected native handle, connecting it on first use.
    ///
    /// # Errors
    /// Returns the stable configuration, credential, or refused-exchange
    /// error of the shared Rust `connect`.
    fn handle(&self, py: Python<'_>) -> WyrdPyResult<&NativePrincipals> {
        Ok(self.inner.get(py, |client| async move {
            match client {
                Some(client) => NativePrincipals::connect_with(&client).await,
                None => NativePrincipals::connect().await,
            }
        })?)
    }
}

#[pymethods]
impl Principals {
    /// Build a principal administration handle acting as `client`, performing no IO.
    ///
    /// The client, or the ambient chain when `client` is omitted, resolves
    /// and connects once on the first server operation, exactly as `connect`
    /// does; until then nothing is read from the environment or the network.
    ///
    /// # Arguments
    /// * `client` - The `WyrdClient` every call is sent as, or `None` for the
    ///   ambient chain.
    #[new]
    #[pyo3(signature = (client=None))]
    fn __new__(client: Option<PyRef<'_, crate::client::PyWyrdClient>>) -> Self {
        Self {
            inner: crate::client::LazyConnection::new(client.as_deref()),
        }
    }

    /// Connect a principal administration handle acting as `client` now.
    ///
    /// Resolves the client (the ambient chain when omitted) and completes any
    /// credential exchange it requires before returning; a supplied bearer
    /// or fresh cached token is reused without a request.
    ///
    /// # Arguments
    /// * `client` - The `WyrdClient` every call is sent as, or `None` for the
    ///   ambient chain.
    ///
    /// # Errors
    /// Raises `WyrdError` with the stable configuration, credential, or
    /// refused-exchange error.
    #[staticmethod]
    #[pyo3(signature = (client=None))]
    fn connect(
        py: Python<'_>,
        client: Option<PyRef<'_, crate::client::PyWyrdClient>>,
    ) -> WyrdPyResult<Self> {
        let handle = Self::__new__(client);
        handle.handle(py)?;
        Ok(handle)
    }

    /// Create an unbound Service principal from a
    /// `CreateServicePrincipalRequest` dict, returning its id and first
    /// credential.
    ///
    /// # Errors
    /// Raises `WyrdError` when `request` does not match the wire contract, the
    /// caller lacks `service_accounts:write`, a Role is unknown, or the request
    /// fails.
    fn create_service_principal(
        &self,
        py: Python<'_>,
        request: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let request: CreateServicePrincipalRequest = decode("request", pyobject_to_json(request)?)?;
        let created =
            crate::trace::block_on(py, self.handle(py)?.create_service_principal(&request))?;
        to_python(py, &created)
    }

    /// Issue one more credential for a Service or Agent principal.
    ///
    /// # Errors
    /// Raises `WyrdError` when `principal_id` is malformed, the caller lacks
    /// `service_accounts:write`, the principal is unknown, or the request
    /// fails.
    fn issue_credential(&self, py: Python<'_>, principal_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let issued = crate::trace::block_on(py, self.handle(py)?.issue_credential(&id))?;
        to_python(py, &issued)
    }

    /// List a principal's credential metadata; secrets are never returned.
    ///
    /// # Errors
    /// Raises `WyrdError` when `principal_id` is malformed, the caller lacks
    /// `service_accounts:write`, the principal is unknown, or the request
    /// fails.
    fn list_credentials(&self, py: Python<'_>, principal_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let listed = crate::trace::block_on(py, self.handle(py)?.list_credentials(&id))?;
        to_python(py, &listed)
    }

    /// Revoke one credential of a principal.
    ///
    /// # Errors
    /// Raises `WyrdError` when an id is malformed, the caller lacks
    /// `service_accounts:write`, the credential is not the principal's, or the
    /// request fails.
    fn revoke_credential(
        &self,
        py: Python<'_>,
        principal_id: &str,
        credential_id: &str,
    ) -> WyrdPyResult<()> {
        let id = self::principal_id(principal_id)?;
        let credential = decode("credential_id", Value::from(credential_id))?;
        crate::trace::block_on(py, self.handle(py)?.revoke_credential(&id, credential))?;
        Ok(())
    }

    /// Revoke a principal and every credential it holds.
    ///
    /// # Errors
    /// Raises `WyrdError` when an argument does not match the wire contract,
    /// the caller lacks the revoke permission, the principal is unknown, or
    /// the request fails.
    fn revoke_principal(
        &self,
        py: Python<'_>,
        principal_id: &str,
        request: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<()> {
        let id = self::principal_id(principal_id)?;
        let request: RevokePrincipalRequest = decode("request", pyobject_to_json(request)?)?;
        crate::trace::block_on(py, self.handle(py)?.revoke_principal(&id, &request))?;
        Ok(())
    }

    /// Replace a Service principal's workload restriction from a
    /// `WorkloadRestriction` dict, returning the updated `PrincipalSummary`.
    ///
    /// # Errors
    /// Raises `WyrdError` when an argument does not match the wire contract,
    /// the caller lacks the required permission, the family already has a
    /// runtime account, or the request fails.
    fn set_workload_restriction(
        &self,
        py: Python<'_>,
        principal_id: &str,
        restriction: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let restriction: WorkloadRestriction =
            decode("restriction", pyobject_to_json(restriction)?)?;
        let summary = crate::trace::block_on(
            py,
            self.handle(py)?.set_workload_restriction(&id, &restriction),
        )?;
        to_python(py, &summary)
    }

    /// One page of assignable principals matching exact filters, ordered by
    /// id; pass a page's `next` as `after` to read the following page.
    ///
    /// # Errors
    /// Raises `WyrdError` when a filter does not match the wire contract, the
    /// caller lacks `service_accounts:write`, or the request fails.
    #[pyo3(signature = (*, kind=None, email=None, name=None, limit=None, after=None))]
    fn list(
        &self,
        py: Python<'_>,
        kind: Option<&str>,
        email: Option<&str>,
        name: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let query: PrincipalQuery = decode(
            "query",
            serde_json::json!({
                "kind": kind, "email": email, "name": name, "limit": limit, "after": after,
            }),
        )?;
        let page = crate::trace::block_on(py, self.handle(py)?.list(&query))?;
        to_python(py, &page)
    }

    /// A principal's Role assignments ordered by Role and source.
    ///
    /// # Errors
    /// Raises `WyrdError` when `principal_id` is malformed, the caller lacks
    /// `service_accounts:write`, the principal is not assignable, or the
    /// request fails.
    fn roles(&self, py: Python<'_>, principal_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let roles = crate::trace::block_on(py, self.handle(py)?.roles(&id))?;
        to_python(py, &roles)
    }

    /// Idempotently grant a direct Role, returning `changed` and the
    /// resulting assignments.
    ///
    /// # Errors
    /// Raises `WyrdError` when `principal_id` is malformed, the caller is not
    /// a tenant administrator, the principal is not assignable, the Role is
    /// unknown, or the request fails.
    fn grant_role(
        &self,
        py: Python<'_>,
        principal_id: &str,
        role: &str,
    ) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let change = crate::trace::block_on(py, self.handle(py)?.grant_role(&id, role))?;
        to_python(py, &change)
    }

    /// Idempotently revoke a direct Role, returning `changed` and the
    /// resulting assignments; identity-provider assignments are untouched.
    ///
    /// # Errors
    /// Raises `WyrdError` when `principal_id` is malformed, the caller is not
    /// a tenant administrator, the principal is not assignable, the Role is
    /// unknown, or the request fails.
    fn revoke_role(
        &self,
        py: Python<'_>,
        principal_id: &str,
        role: &str,
    ) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let change = crate::trace::block_on(py, self.handle(py)?.revoke_role(&id, role))?;
        to_python(py, &change)
    }
}

/// Register the native `principals` submodule's classes.
///
/// # Errors
/// Returns a Python error when the class cannot be added to the module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Principals>()
}

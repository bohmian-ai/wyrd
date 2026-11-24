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
    AccessGroupId, CreateGroupRequest, CreateRoleBindingRequest, CreateServicePrincipalRequest,
    CredentialListQuery, PrincipalId, PrincipalQuery, Principals as NativePrincipals,
    PutRoleRequest, RevokePrincipalRequest, RoleBindingId, RoleBindingQuery, WorkloadRestriction,
};
use wyrd_utils::py::{WyrdPyResult, pyobject_to_json};

use crate::operators::{decode, to_python};

/// Python-facing tenant principal handle: create Service principals, manage
/// their credentials, discover principals, and administer scoped role
/// bindings, custom roles, and local groups.
///
/// Every call blocks the calling thread with the GIL released. A binding
/// change reaches the account at its next request.
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

/// Decode a local group id argument.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` naming `group_id` when the value is not
/// a group id.
fn group_id(value: &str) -> WyrdPyResult<AccessGroupId> {
    decode("group_id", Value::from(value))
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

    /// Read one page of a principal's credential metadata, newest first; an
    /// omitted `limit` reads 50. Pass a page's `next_cursor` as `cursor` with
    /// the same principal to read the following page.
    ///
    /// # Errors
    /// Raises `WyrdError` when an argument does not match the wire contract,
    /// the caller may not read these credentials, the cursor names another
    /// principal, or the request fails.
    #[pyo3(signature = (principal_id, *, limit=None, cursor=None))]
    fn list_credentials_page(
        &self,
        py: Python<'_>,
        principal_id: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let query: CredentialListQuery = decode(
            "query",
            serde_json::json!({ "limit": limit, "cursor": cursor }),
        )?;
        let page = crate::trace::block_on(py, self.handle(py)?.list_credentials_page(&id, query))?;
        to_python(py, &page)
    }

    /// The calling User's safe profile.
    ///
    /// # Errors
    /// Raises `WyrdError` when the caller is not a User or the request fails.
    fn me(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let profile = crate::trace::block_on(py, self.handle(py)?.me())?;
        to_python(py, &profile)
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

    /// One page of assignable principals matching exact and Role filters,
    /// ordered by id, with status counts across every page; pass a page's
    /// `next` as `after` to read the following page.
    ///
    /// # Errors
    /// Raises `WyrdError` when a filter does not match the wire contract, the
    /// caller lacks `service_accounts:write`, or the request fails.
    #[pyo3(signature = (*, kind=None, email=None, name=None, role=None, limit=None, after=None))]
    #[expect(
        clippy::too_many_arguments,
        reason = "each keyword is one `PrincipalQuery` field, the Python projection of its query string."
    )]
    fn list(
        &self,
        py: Python<'_>,
        kind: Option<&str>,
        email: Option<&str>,
        name: Option<&str>,
        role: Option<&str>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let query: PrincipalQuery = decode(
            "query",
            serde_json::json!({
                "kind": kind, "email": email, "name": name, "role": role, "limit": limit,
                "after": after,
            }),
        )?;
        let page = crate::trace::block_on(py, self.handle(py)?.list(&query))?;
        to_python(py, &page)
    }

    /// One account's current access: its bindings, their sources, and the
    /// scoped permissions they compile to.
    ///
    /// # Errors
    /// Raises `WyrdError` when `principal_id` is malformed, the caller lacks
    /// `service_accounts:write`, the account is not assignable, or the
    /// request fails.
    fn access(&self, py: Python<'_>, principal_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id = self::principal_id(principal_id)?;
        let access = crate::trace::block_on(py, self.handle(py)?.access(&id))?;
        to_python(py, &access)
    }

    /// Every role and the operations it bundles.
    ///
    /// # Errors
    /// Raises `WyrdError` when the caller lacks `service_accounts:write` or the
    /// request fails.
    fn roles(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let roles = crate::trace::block_on(py, self.handle(py)?.roles())?;
        to_python(py, &roles)
    }

    /// Create or replace a custom role from a `PutRoleRequest` dict.
    ///
    /// # Errors
    /// Raises `WyrdError` when `request` does not match the wire contract, the
    /// role is built in, the caller is not a tenant administrator, or the
    /// request fails.
    fn put_role(
        &self,
        py: Python<'_>,
        role: &str,
        request: &Bound<'_, PyAny>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let request: PutRoleRequest = decode("request", pyobject_to_json(request)?)?;
        let stored = crate::trace::block_on(py, self.handle(py)?.put_role(role, &request))?;
        to_python(py, &stored)
    }

    /// Delete a custom role no binding uses.
    ///
    /// # Errors
    /// Raises `WyrdError` when the role is built in, unknown, or still bound,
    /// the caller is not a tenant administrator, or the request fails.
    fn delete_role(&self, py: Python<'_>, role: &str) -> WyrdPyResult<Py<PyAny>> {
        let deleted = crate::trace::block_on(py, self.handle(py)?.delete_role(role))?;
        to_python(py, &deleted)
    }

    /// Bindings, optionally for one account, group, or role.
    ///
    /// # Errors
    /// Raises `WyrdError` when a filter is malformed, the caller lacks
    /// `service_accounts:write`, or the request fails.
    #[pyo3(signature = (*, principal_id=None, group_id=None, role=None))]
    fn bindings(
        &self,
        py: Python<'_>,
        principal_id: Option<&str>,
        group_id: Option<&str>,
        role: Option<&str>,
    ) -> WyrdPyResult<Py<PyAny>> {
        let query: RoleBindingQuery = decode(
            "query",
            serde_json::json!({ "principal_id": principal_id, "group_id": group_id, "role": role }),
        )?;
        let bindings = crate::trace::block_on(py, self.handle(py)?.bindings(&query))?;
        to_python(py, &bindings)
    }

    /// Idempotently bind a role at an explicit scope from a
    /// `CreateRoleBindingRequest` dict; a repeat returns the same binding.
    ///
    /// # Errors
    /// Raises `WyrdError` when `request` does not match the wire contract, the
    /// role is unknown, the scope does not apply to it, the subject is not
    /// assignable, the caller is not a tenant administrator, or the request
    /// fails.
    fn bind(&self, py: Python<'_>, request: &Bound<'_, PyAny>) -> WyrdPyResult<Py<PyAny>> {
        let request: CreateRoleBindingRequest = decode("request", pyobject_to_json(request)?)?;
        let binding = crate::trace::block_on(py, self.handle(py)?.bind(&request))?;
        to_python(py, &binding)
    }

    /// Remove one directly administered binding.
    ///
    /// # Errors
    /// Raises `WyrdError` when `binding_id` is malformed, unknown, or a
    /// provider binding, the caller is not a tenant administrator, or the
    /// request fails.
    fn unbind(&self, py: Python<'_>, binding_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id: RoleBindingId = decode("binding_id", Value::from(binding_id))?;
        let binding = crate::trace::block_on(py, self.handle(py)?.unbind(&id))?;
        to_python(py, &binding)
    }

    /// Every local group and its members.
    ///
    /// # Errors
    /// Raises `WyrdError` when the caller lacks `service_accounts:write` or the
    /// request fails.
    fn groups(&self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        let groups = crate::trace::block_on(py, self.handle(py)?.groups())?;
        to_python(py, &groups)
    }

    /// Create a local group named `name`.
    ///
    /// # Errors
    /// Raises `WyrdError` when the name is invalid or taken, the caller is not
    /// a tenant administrator, or the request fails.
    fn create_group(&self, py: Python<'_>, name: &str) -> WyrdPyResult<Py<PyAny>> {
        let request: CreateGroupRequest = decode("request", serde_json::json!({ "name": name }))?;
        let group = crate::trace::block_on(py, self.handle(py)?.create_group(&request))?;
        to_python(py, &group)
    }

    /// Delete a local group with its memberships and bindings.
    ///
    /// # Errors
    /// Raises `WyrdError` when `group_id` is malformed or unknown, the caller
    /// is not a tenant administrator, or the request fails.
    fn delete_group(&self, py: Python<'_>, group_id: &str) -> WyrdPyResult<Py<PyAny>> {
        let id = self::group_id(group_id)?;
        let group = crate::trace::block_on(py, self.handle(py)?.delete_group(&id))?;
        to_python(py, &group)
    }

    /// Add an account to a local group.
    ///
    /// # Errors
    /// Raises `WyrdError` when an id is malformed or unknown, the caller is
    /// not a tenant administrator, or the request fails.
    fn add_group_member(
        &self,
        py: Python<'_>,
        group_id: &str,
        principal_id: &str,
    ) -> WyrdPyResult<Py<PyAny>> {
        let (group, principal) = (self::group_id(group_id)?, self::principal_id(principal_id)?);
        let view =
            crate::trace::block_on(py, self.handle(py)?.add_group_member(&group, &principal))?;
        to_python(py, &view)
    }

    /// Remove an account from a local group.
    ///
    /// # Errors
    /// Raises `WyrdError` when an id is malformed or unknown, the caller is
    /// not a tenant administrator, or the request fails.
    fn remove_group_member(
        &self,
        py: Python<'_>,
        group_id: &str,
        principal_id: &str,
    ) -> WyrdPyResult<Py<PyAny>> {
        let (group, principal) = (self::group_id(group_id)?, self::principal_id(principal_id)?);
        let view =
            crate::trace::block_on(py, self.handle(py)?.remove_group_member(&group, &principal))?;
        to_python(py, &view)
    }
}

/// Register the native `principals` submodule's classes.
///
/// # Errors
/// Returns a Python error when the class cannot be added to the module.
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Principals>()
}
